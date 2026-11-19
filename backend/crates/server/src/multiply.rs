//! Agent multiplier (D-73): `POST /agents/{id}/multiply` clones one agent N
//! times so an owner can run many slightly different copies of a strategy.
//!
//! - Each clone is an ordinary agent: own id, version history and simulated
//!   allocation. It goes through the same validation (`insert_agent`) and
//!   ledger code (`ledger::allocate`) as `POST /agents` and `/allocate`.
//! - Capital comes only from the owner's unallocated simulated balance, as
//!   one `allocate` posting set per clone, all in one transaction: nothing is
//!   minted, and a shortfall writes nothing.
//! - Variance is seeded and bounded. It touches only strategy parameters
//!   (trigger thresholds and windows, graph conviction gates, approach
//!   ambition, horizon). Money limits are copied unchanged, and the policy's
//!   `min_conviction_bps` can only go up, so a clone is never looser than its
//!   source.
//! - Clones and their source form a family. Siblings never challenge each
//!   other (see `lifecycle::pick_counterparty`).

use aen_core::ledger::{self, Env};
use aen_core::money::Money;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::agents::{current_input, insert_agent, record_activity};
use crate::error::{ApiError, Detail};
use crate::extract::{ApiJson, ApiPath};
use crate::ledger_db::{self, PostOutcome};
use crate::models::{AgentInput, MAX_NAME_LEN, display_bps, display_usdc};

/// Clones per request.
pub const MAX_CLONES_PER_REQUEST: u32 = 100;
/// Clones per family, over all requests. Bounds the work of one tick.
pub const MAX_CLONES_PER_FAMILY: i64 = 200;
/// Largest variance: each varied parameter moves by at most +/-25 %.
pub const MAX_VARIANCE_BPS: u32 = 2_500;

const SIMULATED_SUFFIX: &str = " (simulated)";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiplyBody {
    /// Number of clones, 1..=100.
    pub count: u32,
    /// 0..=2500 basis points (0-25 %).
    pub variance_bps: u32,
    /// Total simulated capital, split evenly; the rounding remainder stays
    /// unallocated.
    pub budget: Money,
    /// Fixes the variance. Omitted: the server picks one and returns it.
    #[serde(default)]
    pub seed: Option<u32>,
    /// Plan only: validate, compute names, allocations and parameters, and
    /// write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// One varied parameter of one clone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamChange {
    /// Dotted path into the agent input, e.g.
    /// `graph.nodes.trigger.params.threshold_bps`.
    pub path: String,
    pub from: i64,
    pub to: i64,
    /// The drawn offset before clamping, in bps of `from`.
    pub offset_bps: i64,
}

#[derive(Debug, Serialize)]
pub struct PlannedClone {
    /// `None` on a dry run.
    pub agent_id: Option<Uuid>,
    pub name: String,
    pub index: i32,
    pub allocation: Money,
    pub params: Vec<ParamChange>,
}

#[derive(Debug, Serialize)]
pub struct SourceRef {
    pub agent_id: Uuid,
    pub name: String,
    pub version: i32,
}

#[derive(Debug, Serialize)]
pub struct MultiplyReport {
    pub env: Env,
    pub dry_run: bool,
    /// `None` on a dry run.
    pub multiplication_id: Option<Uuid>,
    pub family_id: Uuid,
    pub source: SourceRef,
    pub count: u32,
    pub variance_bps: u32,
    pub seed: u32,
    pub budget: Money,
    pub per_clone: Money,
    /// `per_clone * count`: what leaves the unallocated balance.
    pub total: Money,
    /// `budget - total`: stays unallocated.
    pub remainder: Money,
    /// The owner's unallocated balance before this request.
    pub unallocated: Money,
    /// Whether `unallocated >= total` (always true on a real run).
    pub funds_ok: bool,
    pub clones: Vec<PlannedClone>,
    pub note: &'static str,
}

// ---------------------------------------------------------------------------
// Pure parts: budget split and seeded variance
// ---------------------------------------------------------------------------

/// `(per_clone, total, remainder)`: an even split rounded down. `None` when
/// `count` is 0 or the budget is too small to give every clone at least one
/// minor unit.
pub fn split_budget(budget: Money, count: u32) -> Option<(Money, Money, Money)> {
    if count == 0 {
        return None;
    }
    let n = i64::from(count);
    let per = budget.minor() / n;
    if per == 0 {
        return None;
    }
    let total = per.checked_mul(n)?;
    Some((
        Money::from_minor(per)?,
        Money::from_minor(total)?,
        Money::from_minor(budget.minor() - total)?,
    ))
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// A deterministic draw for (seed, clone index, parameter path). Independent
/// of every other parameter, so adding a node does not reshuffle the rest.
fn draw(seed: u32, index: i32, path: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a
    for b in path.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    splitmix64(splitmix64((u64::from(seed) << 32) | u64::from(index as u32)) ^ h)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Way {
    /// Offset in `[-v, +v]`.
    Both,
    /// Offset in `[0, +v]`: the value can only rise (a stricter gate).
    UpOnly,
}

/// The offset in bps of the current value.
fn offset_bps(seed: u32, index: i32, path: &str, variance_bps: u32, way: Way) -> i64 {
    let v = u64::from(variance_bps);
    if v == 0 {
        return 0;
    }
    let x = draw(seed, index, path);
    match way {
        Way::Both => (x % (2 * v + 1)) as i64 - v as i64,
        Way::UpOnly => (x % (v + 1)) as i64,
    }
}

/// `value * (1 + offset)`, rounded to the nearest integer, clamped to
/// `[min, max]`.
fn scaled(value: i64, offset_bps: i64, min: i64, max: i64) -> i64 {
    let num = i128::from(value) * i128::from(10_000 + offset_bps);
    let rounded = (num + 5_000).div_euclid(10_000);
    rounded.clamp(i128::from(min), i128::from(max)) as i64
}

struct Varier {
    seed: u32,
    index: i32,
    variance_bps: u32,
    changes: Vec<ParamChange>,
}

impl Varier {
    fn apply(&mut self, path: String, value: i64, min: i64, max: i64, way: Way) -> i64 {
        let offset = offset_bps(self.seed, self.index, &path, self.variance_bps, way);
        let min = if way == Way::UpOnly {
            min.max(value)
        } else {
            min
        };
        let to = scaled(value, offset, min, max.max(min));
        self.changes.push(ParamChange {
            path,
            from: value,
            to,
            offset_bps: offset,
        });
        to
    }

    /// Varies `params[key]` in place when it is an integer.
    fn param(&mut self, params: &mut Value, prefix: &str, key: &str, min: i64, max: i64) {
        if let Some(value) = params.get(key).and_then(Value::as_i64) {
            let to = self.apply(format!("{prefix}.{key}"), value, min, max, Way::Both);
            params[key] = json!(to);
        }
    }
}

/// The clone's input: `source` with seeded, bounded variance applied to its
/// strategy parameters, plus the list of what changed. Money limits and
/// money-valued graph params are copied unchanged; the policy's
/// `min_conviction_bps` only ever rises.
pub fn vary(
    source: &AgentInput,
    seed: u32,
    index: i32,
    variance_bps: u32,
) -> (AgentInput, Vec<ParamChange>) {
    let mut out = source.clone();
    let mut v = Varier {
        seed,
        index,
        variance_bps: variance_bps.min(MAX_VARIANCE_BPS),
        changes: Vec::new(),
    };

    let horizon = v.apply(
        "config.horizon_secs".into(),
        i64::from(out.config.horizon_secs),
        1,
        i64::from(u32::MAX),
        Way::Both,
    );
    out.config.horizon_secs = horizon as u32;
    let conviction = v.apply(
        "policy.min_conviction_bps".into(),
        i64::from(out.policy.min_conviction_bps),
        0,
        i64::from(aen_core::policy::MAX_BPS),
        Way::UpOnly,
    );
    out.policy.min_conviction_bps = conviction as u32;

    for node in &mut out.graph.nodes {
        let prefix = format!("graph.nodes.{}.params", node.id);
        match node.node_type.as_str() {
            "trigger.market_move" => {
                v.param(&mut node.params, &prefix, "threshold_bps", 1, 10_000);
                v.param(&mut node.params, &prefix, "window_secs", 1, 604_800);
            }
            "trigger.schedule" => v.param(&mut node.params, &prefix, "every_secs", 1, 86_400),
            "cond.conviction_gte" => v.param(&mut node.params, &prefix, "min_bps", 0, 10_000),
            aen_core::graph::APPROACH_PROFILE => {
                if let Some(stance) = node.params.get_mut("stance") {
                    v.param(
                        stance,
                        &format!("{prefix}.stance"),
                        "ambition_bps",
                        0,
                        10_000,
                    );
                }
            }
            _ => {}
        }
    }
    (out, v.changes)
}

/// `"Demo Bull (simulated)"`, 7 -> `"Demo Bull #7 (simulated)"`; other names
/// get ` #7` appended. Kept within the agent name limit.
pub fn clone_name(root: &str, index: i32) -> String {
    let root = root.trim();
    let (stem, suffix) = match root.strip_suffix(SIMULATED_SUFFIX) {
        Some(stem) => (stem.trim_end(), SIMULATED_SUFFIX),
        None => (root, ""),
    };
    let tag = format!(" #{index}");
    let room = MAX_NAME_LEN.saturating_sub(tag.chars().count() + suffix.chars().count());
    let stem: String = stem.chars().take(room).collect();
    format!("{}{tag}{suffix}", stem.trim_end())
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

fn validate(body: &MultiplyBody) -> Result<(Money, Money, Money), ApiError> {
    let mut details = Vec::new();
    if body.count == 0 || body.count > MAX_CLONES_PER_REQUEST {
        details.push(Detail::new(
            "count",
            "out_of_range",
            format!("count must be between 1 and {MAX_CLONES_PER_REQUEST}"),
        ));
    }
    if body.variance_bps > MAX_VARIANCE_BPS {
        details.push(Detail::new(
            "variance_bps",
            "out_of_range",
            format!("variance_bps must be between 0 and {MAX_VARIANCE_BPS}"),
        ));
    }
    let split = split_budget(body.budget, body.count);
    if split.is_none() && details.is_empty() {
        details.push(Detail::new(
            "budget",
            "budget_too_small",
            "the budget must give every clone at least 0.000001 USDC",
        ));
    }
    match split {
        Some(split) if details.is_empty() => Ok(split),
        _ => Err(ApiError::validation(details)),
    }
}

/// The family an agent belongs to: its root's id (its own id if it is not a
/// clone). Used by the runtime's sibling rule.
pub async fn family_of(conn: &mut PgConnection, agent: Uuid) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE((SELECT family_id FROM agent_clones WHERE agent_id = $1), $1)",
    )
    .bind(agent)
    .fetch_one(conn)
    .await
}

pub async fn multiply(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<MultiplyBody>,
) -> Result<(StatusCode, Json<MultiplyReport>), ApiError> {
    let (per_clone, total, remainder) = validate(&body)?;
    let seed = body.seed.unwrap_or_else(|| Uuid::new_v4().as_u128() as u32);
    let env = Env::Simulated;

    let mut tx = pool.begin().await?;
    // Serializes multiplications, so clone indexes and the family cap hold.
    crate::lifecycle::advisory_lock(&mut tx, "aen:multiply").await?;
    // Locks the source row; its current version is what gets copied.
    let source = current_input(&mut tx, id)
        .await?
        .ok_or_else(|| ApiError::not_found("agent"))?;
    let (source_env, status, version): (String, String, i32) =
        sqlx::query_as("SELECT env, status, current_version FROM agents WHERE id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if source_env != env.as_str() {
        return Err(ApiError::unprocessable(
            "unsupported_env",
            "only agents on simulated funds can be multiplied",
        ));
    }
    if status != "active" {
        return Err(ApiError::conflict(
            "source_paused",
            "resume the agent before multiplying it",
        ));
    }
    let family_id = family_of(&mut tx, id).await?;
    let (existing, last_index): (i64, i32) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(MAX(clone_index), 0) FROM agent_clones WHERE family_id = $1",
    )
    .bind(family_id)
    .fetch_one(&mut *tx)
    .await?;
    if existing + i64::from(body.count) > MAX_CLONES_PER_FAMILY {
        return Err(ApiError::conflict(
            "family_full",
            format!(
                "a family holds at most {MAX_CLONES_PER_FAMILY} clones; this one has {existing}"
            ),
        ));
    }
    let root_name: String = sqlx::query_scalar("SELECT name FROM agents WHERE id = $1")
        .bind(family_id)
        .fetch_one(&mut *tx)
        .await?;

    // Same lock name as `ledger_db::post`, so the balance read here cannot be
    // overtaken by another posting before ours.
    crate::lifecycle::advisory_lock(&mut tx, &format!("acct:{}:user:unallocated", env.as_str()))
        .await?;
    let unallocated: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries \
         WHERE env = $1 AND account = 'user:unallocated'",
    )
    .bind(env.as_str())
    .fetch_one(&mut *tx)
    .await?;
    let unallocated = Money::from_minor(unallocated)
        .ok_or_else(|| ApiError::internal("negative unallocated balance"))?;
    let funds_ok = unallocated >= total;
    if !funds_ok && !body.dry_run {
        return Err(ApiError::unprocessable(
            "insufficient_funds",
            format!(
                "{} clones at {} each need {} (simulated); your unallocated balance is {}",
                body.count,
                display_usdc(per_clone),
                display_usdc(total),
                display_usdc(unallocated)
            ),
        ));
    }

    let multiplication_id = Uuid::new_v4();
    let mut clones = Vec::with_capacity(body.count as usize);
    let mut inputs = Vec::with_capacity(body.count as usize);
    for k in 1..=body.count as i32 {
        let index = last_index + k;
        let (mut input, params) = vary(&source, seed, index, body.variance_bps);
        input.name = clone_name(&root_name, index);
        input.description = format!(
            "Clone #{index} of \"{}\" (version {version}): same strategy with up to {} \
             parameter variance (seed {seed}). Simulated funds only.",
            source.name,
            display_bps(i64::from(body.variance_bps))
        );
        // Defense in depth: variance must never loosen a money limit.
        let (p, s) = (&input.policy, &source.policy);
        if p.max_per_interaction > s.max_per_interaction
            || p.max_aggregate_exposure > s.max_aggregate_exposure
            || p.approval_threshold > s.approval_threshold
            || p.stop_loss != s.stop_loss
            || p.allowed_interaction_types != s.allowed_interaction_types
            || p.min_conviction_bps < s.min_conviction_bps
        {
            return Err(ApiError::internal("variance loosened a policy limit"));
        }
        let details = input.validate();
        if !details.is_empty() {
            return Err(ApiError::validation(details));
        }
        clones.push(PlannedClone {
            agent_id: None,
            name: input.name.clone(),
            index,
            allocation: per_clone,
            params,
        });
        inputs.push(input);
    }

    let report = |clones, multiplication_id| MultiplyReport {
        env,
        dry_run: body.dry_run,
        multiplication_id,
        family_id,
        source: SourceRef {
            agent_id: id,
            name: source.name.clone(),
            version,
        },
        count: body.count,
        variance_bps: body.variance_bps,
        seed,
        budget: body.budget,
        per_clone,
        total,
        remainder,
        unallocated,
        funds_ok,
        clones,
        note: "Simulated funds only: nothing here is a real or testnet asset.",
    };
    if body.dry_run {
        tx.rollback().await?;
        return Ok((StatusCode::OK, Json(report(clones, None))));
    }

    sqlx::query(
        "INSERT INTO agent_multiplications (id, family_id, source_agent_id, source_version, \
         count, variance_bps, seed, budget, per_clone, env) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(multiplication_id)
    .bind(family_id)
    .bind(id)
    .bind(version)
    .bind(body.count as i32)
    .bind(body.variance_bps as i32)
    .bind(i64::from(seed))
    .bind(body.budget.minor())
    .bind(per_clone.minor())
    .bind(env.as_str())
    .execute(&mut *tx)
    .await?;

    for (clone, input) in clones.iter_mut().zip(inputs) {
        let clone_id = Uuid::new_v4();
        insert_agent(&mut tx, clone_id, input).await?;
        sqlx::query(
            "INSERT INTO agent_clones (agent_id, family_id, multiplication_id, clone_index, \
             variance) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(clone_id)
        .bind(family_id)
        .bind(multiplication_id)
        .bind(clone.index)
        .bind(json!({
            "variance_bps": body.variance_bps,
            "seed": seed,
            "source_agent_id": id,
            "source_version": version,
            "params": clone.params,
        }))
        .execute(&mut *tx)
        .await?;
        let key = format!("multiply:{multiplication_id}:{}", clone.index);
        let set = ledger::allocate(env, clone_id, per_clone, &key)?;
        if ledger_db::post(&mut tx, &set).await? == PostOutcome::Posted {
            record_activity(
                &mut tx,
                Some(clone_id),
                "funds_allocated",
                format!(
                    "{} (simulated) allocated to \"{}\"",
                    display_usdc(per_clone),
                    clone.name
                ),
                Some(json!({
                    "amount": per_clone, "env": env, "source": "multiply",
                    "multiplication_id": multiplication_id,
                })),
            )
            .await?;
        }
        clone.agent_id = Some(clone_id);
    }
    record_activity(
        &mut tx,
        Some(id),
        "agents_multiplied",
        format!(
            "\"{}\" was multiplied {}x: {} clones with up to {} parameter variance, {} \
             (simulated) each, {} (simulated) in total",
            source.name,
            body.count,
            body.count,
            display_bps(i64::from(body.variance_bps)),
            display_usdc(per_clone),
            display_usdc(total)
        ),
        Some(json!({
            "multiplication_id": multiplication_id, "family_id": family_id,
            "count": body.count, "variance_bps": body.variance_bps, "seed": seed,
            "per_clone": per_clone, "total": total, "env": env,
            "clone_ids": clones.iter().filter_map(|c| c.agent_id).collect::<Vec<_>>(),
        })),
    )
    .await?;
    tx.commit().await?;

    Ok((
        StatusCode::CREATED,
        Json(report(clones, Some(multiplication_id))),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> AgentInput {
        serde_json::from_value(json!({
            "name": "Demo Bull (simulated)", "description": "d",
            "config": { "goals": "g", "strategy": "s", "categories": [], "horizon_secs": 3600,
                        "risk_tolerance": "medium", "schedule_secs": 60 },
            "policy": { "max_per_interaction": "10000000", "max_aggregate_exposure": "50000000",
                        "approval_threshold": "5000000", "allowed_interaction_types": ["challenge"],
                        "min_conviction_bps": 6000, "stop_loss": "20000000" },
            "graph": { "schema_version": 1, "edges": [], "nodes": [
                { "id": "trigger", "type": "trigger.market_move",
                  "params": { "symbol": "BTCUSD", "window_secs": 3600, "threshold_bps": 200 } },
                { "id": "conviction", "type": "cond.conviction_gte", "params": { "min_bps": 6000 } },
                { "id": "stake", "type": "cond.stake_lte", "params": { "max": "5000000" } },
                { "id": "approach", "type": "approach.profile",
                  "params": { "stance": { "ambition_bps": 10000, "collaboration_bps": 1500 } } }
            ] }
        }))
        .unwrap()
    }

    #[test]
    fn split_rounds_down_and_keeps_the_remainder() {
        let m = |u| Money::from_minor(u).unwrap();
        assert_eq!(split_budget(m(100), 3), Some((m(33), m(99), m(1))));
        assert_eq!(
            split_budget(m(100_000_000), 10),
            Some((m(10_000_000), m(100_000_000), m(0)))
        );
        assert_eq!(split_budget(m(5), 10), None);
        assert_eq!(split_budget(m(5), 0), None);
    }

    #[test]
    fn variance_is_reproducible_and_bounded() {
        let src = input();
        for index in 1..=200 {
            let (a, ca) = vary(&src, 42, index, 2500);
            let (b, cb) = vary(&src, 42, index, 2500);
            assert_eq!(a, b);
            assert_eq!(ca, cb);
            for c in &ca {
                assert!(c.offset_bps.abs() <= 2500, "{c:?}");
                let lo = (c.from as f64 * 0.75).floor() as i64;
                let hi = (c.from as f64 * 1.25).ceil() as i64;
                assert!(c.to >= lo.max(0) && c.to <= hi, "{c:?}");
            }
            // Money is never varied; the policy is never looser.
            assert_eq!(a.policy.max_per_interaction, src.policy.max_per_interaction);
            assert_eq!(
                a.policy.max_aggregate_exposure,
                src.policy.max_aggregate_exposure
            );
            assert_eq!(a.policy.approval_threshold, src.policy.approval_threshold);
            assert_eq!(a.policy.stop_loss, src.policy.stop_loss);
            assert!(a.policy.min_conviction_bps >= src.policy.min_conviction_bps);
            assert_eq!(a.graph.nodes[2].params, src.graph.nodes[2].params);
        }
        // Different seeds give different clones.
        assert_ne!(vary(&src, 1, 1, 2500).1, vary(&src, 2, 1, 2500).1);
    }

    #[test]
    fn zero_variance_is_an_exact_copy() {
        let src = input();
        let (out, changes) = vary(&src, 7, 3, 0);
        assert_eq!(out, src);
        assert!(changes.iter().all(|c| c.from == c.to && c.offset_bps == 0));
        assert_eq!(changes.len(), 6);
    }

    #[test]
    fn clone_names() {
        assert_eq!(
            clone_name("Demo Bull (simulated)", 7),
            "Demo Bull #7 (simulated)"
        );
        assert_eq!(clone_name("Bull", 12), "Bull #12");
        let long = "x".repeat(150);
        assert_eq!(clone_name(&long, 100).chars().count(), MAX_NAME_LEN);
    }
}
