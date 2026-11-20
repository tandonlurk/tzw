//! The challenge lifecycle: discover, propose, negotiate, authorize, commit,
//! resolve, settle.
//!
//! Rules this module keeps:
//! - Reasoning happens outside database transactions and only yields
//!   `PlannedIntent`s. Every intent is judged by `core::authorize` against a
//!   snapshot read inside the same transaction that applies it.
//! - Every state change goes through the `core` transition table, under a row
//!   lock on the interaction, after re-checking the state. A step that finds
//!   the interaction already moved does nothing, so ticks are idempotent and
//!   may run concurrently.
//! - Stakes move only through `ledger_db::post`, with idempotency keys
//!   derived from the interaction.
//! - Simulated interactions never touch a chain. `arc_testnet` ones (both
//!   agents linked to an onchain vault) reserve their stakes in the ledger
//!   here, exactly like simulated ones, and are then staked, resolved and
//!   released onchain by `lifecycle_arc`; the ledger follows the escrow.
//! - `arc_testnet` interactions run on the wall clock: the dev tick's
//!   simulated `now` applies to simulated interactions only.

use aen_core::approach::{Approach, MissReason, market_matches, may_initiate_challenges};
use aen_core::authorize::{AgentStatus, Decision, DenyReason, Intent, Snapshot, authorize};
use aen_core::challenge::{ChallengeTerms, Direction, FormulaId, Outcome, Party, resolve};
use aen_core::firm::MemberRole;
use aen_core::graph::Graph;
use aen_core::interaction::InteractionState;
use aen_core::ledger::{self, Account, Env};
use aen_core::money::{Money, SignedMoney};
use aen_core::policy::{AgentPolicy, InteractionType};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::error::ApiError;
use crate::graph_exec::{
    self, PlannedIntent, RunInput, RunResult, TRIGGER_CHALLENGE_RECEIVED, TRIGGER_MARKET_MOVE,
    TRIGGER_SCHEDULE,
};
use crate::ledger_db;
use crate::market::{self, MarketView};
use crate::models::{AgentConfig, display_usdc};
use crate::reasoner::AgentCtx;
use crate::state::{AppState, Settings};

/// Discovery only proposes on contracts that stay open this long after the
/// funding deadline.
const MIN_OPEN_AFTER_FUNDING_SECS: i64 = 60;

const OPEN_STATES_SQL: &str = "state NOT IN ('settled', 'cancelled', 'failed', 'disputed')";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InteractionRow {
    pub id: Uuid,
    pub kind: String,
    pub env: String,
    pub state: String,
    pub proposer_id: Uuid,
    pub counterparty_id: Uuid,
    pub proposer_version: i32,
    pub counterparty_version: i32,
    pub terms: Value,
    pub stake: i64,
    pub market_key: String,
    pub market_title: String,
    pub market_view: Value,
    pub start_ts: DateTime<Utc>,
    pub end_ts: DateTime<Utc>,
    pub turn: Option<Uuid>,
    pub rounds: i32,
    pub proposer_ask_owner: bool,
    pub counterparty_ask_owner: bool,
    pub outcome: Option<String>,
    pub winner_id: Option<Uuid>,
    pub observed: Option<i64>,
    pub observed_at: Option<DateTime<Utc>>,
    pub resolve_attempts: i32,
    pub status_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub escrow_create_tx: Option<String>,
    pub proposer_stake_tx: Option<String>,
    pub counterparty_stake_tx: Option<String>,
    pub escrow_settle_tx: Option<String>,
    #[sqlx(default)]
    pub proposer_release_tx: Option<String>,
    #[sqlx(default)]
    pub counterparty_release_tx: Option<String>,
    pub pending_outcome: Option<Value>,
    pub vaults_released_at: Option<DateTime<Utc>>,
    /// Set on a firm lead's firm challenges.
    #[sqlx(default)]
    pub firm_id: Option<Uuid>,
}

impl InteractionRow {
    pub fn state(&self) -> Result<InteractionState, ApiError> {
        serde_json::from_value(Value::String(self.state.clone()))
            .map_err(|_| ApiError::internal(format!("unknown interaction state {}", self.state)))
    }

    pub fn terms(&self) -> Result<ChallengeTerms, ApiError> {
        serde_json::from_value(self.terms.clone())
            .map_err(|e| ApiError::internal(format!("stored terms are unreadable: {e}")))
    }

    pub(crate) fn stake(&self) -> Result<Money, ApiError> {
        Money::from_minor(self.stake).ok_or_else(|| ApiError::internal("negative stake"))
    }

    pub(crate) fn env(&self) -> Result<Env, ApiError> {
        self.env
            .parse()
            .map_err(|()| ApiError::internal("unknown env"))
    }

    fn party_of(&self, agent: Uuid) -> Party {
        if agent == self.proposer_id {
            Party::A
        } else {
            Party::B
        }
    }

    fn other(&self, agent: Uuid) -> Uuid {
        if agent == self.proposer_id {
            self.counterparty_id
        } else {
            self.proposer_id
        }
    }
}

/// The clock an interaction in `env` runs on: `arc_testnet` interactions
/// always use the wall clock (escrow deadlines are real time), simulated ones
/// the tick's clock, which the dev tick may set.
pub(crate) fn clock_for(env: &str, tick_now: DateTime<Utc>) -> DateTime<Utc> {
    if env == Env::ArcTestnet.as_str() {
        Utc::now()
    } else {
        tick_now
    }
}

pub fn state_str(state: InteractionState) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// What one tick did. Counts only; details are in the activity log.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct TickReport {
    pub now: Option<DateTime<Utc>>,
    pub proposed: u32,
    pub denied: u32,
    pub countered: u32,
    pub accepted: u32,
    pub rejected: u32,
    pub approvals_requested: u32,
    /// Ledger reservations made this tick (`accepted -> funded`). An owner
    /// approval through the REST API reserves outside a tick and is not
    /// counted here.
    pub funded: u32,
    /// Arc testnet challenges whose escrow went live this tick
    /// (`funded -> active`, both stakes deposited onchain).
    pub activated: u32,
    pub resolving: u32,
    pub settled: u32,
    pub voided: u32,
    pub cancelled: u32,
    /// New co-captain suggestions (rule-based; advisory only).
    pub suggestions: u32,
    /// Agents that joined a firm on their own this tick.
    pub firm_joined: u32,
    /// Scout candidates and analyst signals recorded this tick.
    pub firm_contributions: u32,
    /// Problems that did not stop the tick (market data, reasoner).
    pub errors: Vec<String>,
}

/// An agent with its current version and a fresh financial snapshot.
#[derive(Debug, Clone)]
pub struct AgentRec {
    pub id: Uuid,
    pub name: String,
    pub env: Env,
    pub version: i32,
    pub config: AgentConfig,
    pub policy: AgentPolicy,
    pub graph: Graph,
    pub snapshot: Snapshot,
}

impl AgentRec {
    pub fn ctx(&self) -> AgentCtx {
        AgentCtx {
            id: self.id,
            name: self.name.clone(),
            version: self.version,
            env: self.env,
            config: self.config.clone(),
            policy: self.policy.clone(),
            available: self.snapshot.available,
            exposure: self.snapshot.exposure,
        }
    }
}

/// Exposure = stakes of the agent's interactions in Funded / Active /
/// Resolving, plus its contributions to funded collaborations. Realized
/// P&L = the agent's net over `settle` ledger entries (challenges only),
/// less what it paid firm members (task fees, profit shares). Firm income
/// received is not challenge P&L (see `firm_income` on the agent).
const AGENT_SQL: &str = "
SELECT a.id, a.name, a.status, a.env, a.current_version, v.config, v.policy, v.graph,
  (SELECT COALESCE(SUM(e.delta), 0)::BIGINT FROM ledger_entries e
    WHERE e.env = a.env AND e.account = 'agent:' || a.id || ':available') AS available,
  (SELECT COALESCE(SUM(i.stake), 0)::BIGINT FROM interactions i
    WHERE i.state IN ('funded', 'active', 'resolving')
      AND (i.proposer_id = a.id OR i.counterparty_id = a.id))
  + (SELECT COALESCE(SUM(CASE WHEN c.proposer_id = a.id THEN c.proposer_contribution
                              ELSE c.partner_contribution END), 0)::BIGINT
     FROM collaborations c
     WHERE c.state = 'funded' AND (c.proposer_id = a.id OR c.partner_id = a.id)) AS exposure,
  (SELECT COALESCE(SUM(e.delta), 0)::BIGINT FROM ledger_entries e
    WHERE e.agent_id = a.id AND e.env = a.env
      AND (e.reason = 'settle'
           OR (e.reason IN ('firm_payout', 'firm_task_fee') AND e.delta < 0))) AS realized_pnl
FROM agents a
JOIN agent_versions v ON v.agent_id = a.id AND v.version = a.current_version
WHERE a.id = $1";

type AgentTuple = (
    Uuid,
    String,
    String,
    String,
    i32,
    Value,
    Value,
    Value,
    i64,
    i64,
    i64,
);

pub async fn load_agent(conn: &mut PgConnection, id: Uuid) -> Result<Option<AgentRec>, ApiError> {
    let row: Option<AgentTuple> = sqlx::query_as(AGENT_SQL)
        .bind(id)
        .fetch_optional(conn)
        .await?;
    let Some((id, name, status, env, version, config, policy, graph, available, exposure, pnl)) =
        row
    else {
        return Ok(None);
    };
    let bad = |what: &str| ApiError::internal(format!("agent {id}: stored {what} is unreadable"));
    let money = |units: i64, what: &str| Money::from_minor(units).ok_or_else(|| bad(what));
    Ok(Some(AgentRec {
        id,
        name,
        env: env.parse().map_err(|()| bad("env"))?,
        version,
        config: serde_json::from_value(config).map_err(|_| bad("config"))?,
        policy: serde_json::from_value(policy).map_err(|_| bad("policy"))?,
        graph: serde_json::from_value(graph).map_err(|_| bad("graph"))?,
        snapshot: Snapshot {
            status: if status == "active" {
                AgentStatus::Active
            } else {
                AgentStatus::Paused
            },
            available: money(available, "available balance")?,
            exposure: money(exposure, "exposure")?,
            realized_pnl: SignedMoney::from_minor(pnl),
        },
    }))
}

pub(crate) async fn require_agent(conn: &mut PgConnection, id: Uuid) -> Result<AgentRec, ApiError> {
    load_agent(conn, id)
        .await?
        .ok_or_else(|| ApiError::internal(format!("agent {id} is missing")))
}

pub(crate) async fn advisory_lock(conn: &mut PgConnection, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(name)
        .execute(conn)
        .await?;
    Ok(())
}

pub(crate) async fn lock_interaction(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<InteractionRow>, ApiError> {
    Ok(
        sqlx::query_as("SELECT * FROM interactions WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(conn)
            .await?,
    )
}

/// Moves a locked interaction along the transition table.
pub(crate) async fn set_state(
    conn: &mut PgConnection,
    row: &mut InteractionRow,
    to: InteractionState,
    reason: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    let to = row.state()?.transition(to).map_err(ApiError::internal)?;
    let to_text = state_str(to);
    sqlx::query(
        "UPDATE interactions SET state = $2, status_reason = COALESCE($3, status_reason), \
         updated_at = $4 WHERE id = $1",
    )
    .bind(row.id)
    .bind(&to_text)
    .bind(reason)
    .bind(now)
    .execute(conn)
    .await?;
    row.state = to_text;
    if let Some(reason) = reason {
        row.status_reason = Some(reason.to_owned());
    }
    Ok(())
}

pub(crate) async fn record(
    conn: &mut PgConnection,
    agent_id: Option<Uuid>,
    interaction_id: Option<Uuid>,
    kind: &str,
    summary: String,
    detail: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO activity (agent_id, interaction_id, kind, summary, detail) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(agent_id)
    .bind(interaction_id)
    .bind(kind)
    .bind(summary)
    .bind(detail)
    .execute(conn)
    .await?;
    Ok(())
}

/// Like `record`, but skipped when the agent's latest activity already says
/// exactly this, so a condition that persists across ticks is logged once.
pub(crate) async fn record_once(
    conn: &mut PgConnection,
    agent_id: Uuid,
    kind: &str,
    summary: String,
    detail: Value,
) -> Result<(), sqlx::Error> {
    let last: Option<(String, String)> = sqlx::query_as(
        "SELECT kind, summary FROM activity WHERE agent_id = $1 ORDER BY seq DESC LIMIT 1",
    )
    .bind(agent_id)
    .fetch_optional(&mut *conn)
    .await?;
    if last.is_some_and(|(k, s)| k == kind && s == summary) {
        return Ok(());
    }
    record(conn, Some(agent_id), None, kind, summary, detail).await
}

async fn insert_run(
    conn: &mut PgConnection,
    agent: &AgentRec,
    trigger: &str,
    interaction_id: Option<Uuid>,
    result: &RunResult,
    reasoner: &str,
    now: DateTime<Utc>,
) -> Result<Uuid, ApiError> {
    let status = if result.error.is_some() {
        "error"
    } else if result.intents.is_empty() {
        "no_action"
    } else {
        "ok"
    };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO agent_runs \
         (ts, agent_id, agent_version, trigger, interaction_id, status, error, reasoner, trace, intents) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
    )
    .bind(now)
    .bind(agent.id)
    .bind(agent.version)
    .bind(trigger)
    .bind(interaction_id)
    .bind(status)
    .bind(&result.error)
    .bind(reasoner)
    .bind(serde_json::to_value(&result.trace).map_err(ApiError::internal)?)
    .bind(serde_json::to_value(&result.intents).map_err(ApiError::internal)?)
    .fetch_one(conn)
    .await?;
    Ok(id)
}

fn deny_phrase(reason: DenyReason) -> &'static str {
    match reason {
        DenyReason::AgentPaused => "the agent is paused",
        DenyReason::StopLossTriggered => "its stop-loss has been reached",
        DenyReason::InteractionTypeNotAllowed => "challenges are not allowed by its policy",
        DenyReason::IllegalInteractionState => "the challenge is not in a state that allows it",
        DenyReason::ZeroAmount => "the stake is zero",
        DenyReason::AmountOverflow => "the amount is too large",
        DenyReason::ExceedsMaxPerInteraction => "the stake is above its per-challenge limit",
        DenyReason::ExceedsAggregateExposure => "it would exceed its total exposure limit",
        DenyReason::InsufficientAvailableBalance => "its available balance is too low",
    }
}

pub(crate) fn deny_text(reasons: &[DenyReason]) -> String {
    reasons
        .iter()
        .map(|r| deny_phrase(*r))
        .collect::<Vec<_>>()
        .join("; ")
}

/// `5500` bps -> `0.55`; micro-USD -> `65000.00 USD`.
pub fn format_value(formula: FormulaId, value: i64) -> String {
    match formula {
        FormulaId::PmYesPriceBpsV1 => {
            let frac = format!("{:04}", value % 10_000);
            let frac = frac.trim_end_matches('0');
            format!("{}.{frac:0<2} (YES price)", value / 10_000)
        }
        FormulaId::SpotLastPriceMicrosV1 => {
            format!(
                "{}.{:02} USD",
                value / 1_000_000,
                (value % 1_000_000) / 10_000
            )
        }
    }
}

pub(crate) fn side_phrase(direction: Direction) -> &'static str {
    match direction {
        Direction::Above => "at or above",
        Direction::Below => "below",
    }
}

/// The claim party A makes, as a sentence fragment.
pub fn claim_text(terms: &ChallengeTerms, title: &str) -> String {
    format!(
        "\"{title}\" will be {} {}",
        side_phrase(terms.direction),
        format_value(terms.formula, terms.threshold)
    )
}

/// Every stake is labelled with where its funds live.
pub(crate) fn stake_text(stake: Money, env: Env) -> String {
    match env {
        Env::Simulated => format!("{} (simulated)", display_usdc(stake)),
        Env::ArcTestnet => format!("{} (Arc testnet USDC)", display_usdc(stake)),
    }
}

pub(crate) fn ts(secs: i64) -> Result<DateTime<Utc>, ApiError> {
    DateTime::from_timestamp(secs, 0).ok_or_else(|| ApiError::internal("timestamp out of range"))
}

pub(crate) async fn names(
    conn: &mut PgConnection,
    row: &InteractionRow,
) -> Result<(String, String), ApiError> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, name FROM agents WHERE id = ANY($1)")
            .bind(vec![row.proposer_id, row.counterparty_id])
            .fetch_all(conn)
            .await?;
    let name = |id: Uuid| {
        rows.iter()
            .find(|(i, _)| *i == id)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| id.to_string())
    };
    Ok((name(row.proposer_id), name(row.counterparty_id)))
}

/// Cancels a locked, not-yet-funded interaction and closes its approvals.
async fn cancel(
    conn: &mut PgConnection,
    row: &mut InteractionRow,
    reason: &str,
    approval_status: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    set_state(conn, row, InteractionState::Cancelled, Some(reason), now).await?;
    sqlx::query("UPDATE interactions SET turn = NULL WHERE id = $1")
        .bind(row.id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "UPDATE approvals SET status = $2, decided_at = $3 \
         WHERE interaction_id = $1 AND status = 'pending'",
    )
    .bind(row.id)
    .bind(approval_status)
    .bind(now)
    .execute(conn)
    .await?;
    row.turn = None;
    Ok(())
}

// ---------------------------------------------------------------------------
// Approach
// ---------------------------------------------------------------------------

fn miss_text(misses: &[MissReason]) -> String {
    misses
        .iter()
        .map(|m| match m {
            MissReason::Category => "category",
            MissReason::Subcategory => "subcategory",
            MissReason::Structure => "market structure",
            MissReason::Keyword => "keywords",
            MissReason::Symbol => "asset",
            MissReason::MoveBps => "price move",
            MissReason::YesPriceBps => "YES price",
            MissReason::ExpirySecs => "time to expiry",
            MissReason::VolumeDelta24hBps => "24h volume change",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Interactions the agent is in that are not finished.
async fn open_interactions(conn: &mut PgConnection, agent: Uuid) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM interactions WHERE (proposer_id = $1 OR counterparty_id = $1) \
         AND state IN ('proposed', 'negotiating', 'accepted', 'awaiting_approval', 'funded', \
                       'active', 'resolving')",
    )
    .bind(agent)
    .fetch_one(conn)
    .await?)
}

/// With an approach node: a proposal below the approach's minimum conviction
/// is dropped (returns the activity to record), and a stake above the
/// approach's stake target is lowered to it. Explicit `cond.*` blocks have
/// already applied; `authorize` still applies afterwards.
fn apply_approach_sizing(
    agent: &AgentRec,
    approach: Option<&Approach>,
    result: &mut RunResult,
) -> Option<(String, Value)> {
    let approach = approach?;
    if result.error.is_some() {
        return None;
    }
    let conviction = result.conviction_bps;
    let min = approach.min_conviction_bps();
    let index = result
        .intents
        .iter()
        .position(|p| matches!(p.intent, Intent::ProposeChallenge { .. }))?;
    if conviction.is_some_and(|c| c < min) {
        result.intents.remove(index);
        return Some((
            format!(
                "\"{}\" passed on an opportunity: its conviction is below the {} its approach \
                 asks for.",
                agent.name,
                crate::models::display_bps(i64::from(min))
            ),
            json!({ "reason": "min_conviction", "conviction_bps": conviction, "min_bps": min }),
        ));
    }
    let target = approach.stake_target(agent.policy.max_per_interaction);
    let planned = &mut result.intents[index];
    if let Some(terms) = planned.terms.as_mut()
        && terms.stake > target
        && !target.is_zero()
    {
        terms.stake = target;
        planned.intent = Intent::ProposeChallenge { stake: target };
    }
    None
}

// ---------------------------------------------------------------------------
// Tick
// ---------------------------------------------------------------------------

/// Advances everything by one step. Phases run in lifecycle order, so a new
/// proposal can be answered and funded within the same tick. A failure in one
/// agent or interaction is reported and does not stop the others.
pub async fn tick(state: &AppState, now: DateTime<Utc>) -> Result<TickReport, ApiError> {
    let mut report = TickReport {
        now: Some(now),
        ..TickReport::default()
    };
    let pool = &state.pool;
    // Rows are selected with the later of the two clocks and each step
    // re-checks with its interaction's own clock (`clock_for`), so arc_testnet
    // interactions follow the wall clock whatever the dev tick says.
    let select_now = now.max(Utc::now());

    for id in ids(
        pool,
        "SELECT id FROM interactions WHERE state IN ('proposed', 'negotiating', 'accepted', \
         'awaiting_approval') AND start_ts <= $1 ORDER BY created_at, id",
        Some(select_now),
    )
    .await?
    {
        step(&mut report, expire_one(state, id, now).await);
    }

    // Firms: agents open to membership join before anyone discovers, so a
    // new scout can work in the same tick.
    let joined = crate::firms::auto_join_phase(state, now, &mut report).await;
    step(&mut report, joined);

    for id in ids(
        pool,
        "SELECT id FROM agents WHERE status = 'active' ORDER BY created_at, id",
        None,
    )
    .await?
    {
        let outcome = discover_one(state, id, now, &mut report).await;
        step(&mut report, outcome);
    }

    for id in ids(
        pool,
        "SELECT id FROM interactions WHERE state IN ('proposed', 'negotiating') \
         ORDER BY created_at, id",
        None,
    )
    .await?
    {
        let outcome = respond_one(state, id, now, &mut report).await;
        step(&mut report, outcome);
    }

    for id in ids(
        pool,
        "SELECT id FROM interactions WHERE state IN ('accepted', 'awaiting_approval') \
         ORDER BY created_at, id",
        None,
    )
    .await?
    {
        let outcome = commit_one(state, id, now, &mut report).await;
        step(&mut report, outcome);
    }

    // Arc testnet: stake funded challenges into the escrow.
    crate::lifecycle_arc::fund_phase(state, &mut report).await?;

    for id in ids(
        pool,
        "SELECT id FROM interactions WHERE state = 'active' AND end_ts <= $1 \
         ORDER BY created_at, id",
        Some(select_now),
    )
    .await?
    {
        let outcome = start_resolving(state, id, now, &mut report).await;
        step(&mut report, outcome);
    }

    for id in ids(
        pool,
        "SELECT id FROM interactions WHERE state = 'resolving' AND env = 'simulated' \
         ORDER BY created_at, id",
        None,
    )
    .await?
    {
        let outcome = resolve_one(state, id, now, &mut report).await;
        step(&mut report, outcome);
    }

    // Arc testnet: resolve onchain, settle the ledger, release the vaults.
    crate::lifecycle_arc::resolve_phase(state, &mut report).await?;

    // QuickNode Streams events that arrived before the server confirmed the
    // same transaction (D-74). Read-only for funds and states.
    crate::streams::recheck(state, None).await;

    // Last, and read-only as far as funds and interactions go: the rule-based
    // co-captain looks at what the tick left behind. No model call here.
    match crate::suggestions::generate(state, now, false).await {
        Ok(generated) => report.suggestions = generated.created.len() as u32,
        Err(e) => step(&mut report, Err(e)),
    }

    Ok(report)
}

pub(crate) fn step(report: &mut TickReport, outcome: Result<(), ApiError>) {
    if let Err(e) = outcome {
        report.errors.push(format!("{}: {}", e.code, e.message));
    }
}

pub(crate) async fn ids(
    pool: &PgPool,
    sql: &str,
    now: Option<DateTime<Utc>>,
) -> Result<Vec<Uuid>, ApiError> {
    let query = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()));
    let query = match now {
        Some(now) => query.bind(now),
        None => query,
    };
    Ok(query.fetch_all(pool).await?)
}

// ---------------------------------------------------------------------------
// Expiry
// ---------------------------------------------------------------------------

async fn expire_one(state: &AppState, id: Uuid, now: DateTime<Utc>) -> Result<(), ApiError> {
    let mut tx = state.pool.begin().await?;
    let Some(mut row) = lock_interaction(&mut tx, id).await? else {
        return Ok(());
    };
    let now = clock_for(&row.env, now);
    let current = row.state()?;
    let unfunded = matches!(
        current,
        InteractionState::Proposed
            | InteractionState::Negotiating
            | InteractionState::Accepted
            | InteractionState::AwaitingApproval
    );
    if !unfunded || row.start_ts > now {
        return Ok(());
    }
    expire_locked(&mut tx, &mut row, now).await?;
    tx.commit().await?;
    Ok(())
}

async fn expire_locked(
    conn: &mut PgConnection,
    row: &mut InteractionRow,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    cancel(
        conn,
        row,
        "The funding deadline passed before both stakes were committed.",
        "expired",
        now,
    )
    .await?;
    let (a, b) = names(conn, row).await?;
    record(
        conn,
        Some(row.proposer_id),
        Some(row.id),
        "challenge_expired",
        format!(
            "The challenge between \"{a}\" and \"{b}\" expired: stakes were not committed before \
             the funding deadline. No funds moved."
        ),
        json!({ "state": row.state }),
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Discovery and proposal
// ---------------------------------------------------------------------------

async fn has_open_on_market(
    conn: &mut PgConnection,
    agent: Uuid,
    market_key: &str,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT EXISTS (SELECT 1 FROM interactions WHERE market_key = $2 AND {OPEN_STATES_SQL} \
         AND (proposer_id = $1 OR counterparty_id = $1))"
    )))
    .bind(agent)
    .bind(market_key)
    .fetch_one(conn)
    .await
}

/// Whether the agent proposed on this market less than `cooldown_secs` ago,
/// or tried to and was blocked (denied by `authorize`, or no counterparty)
/// under its current config version. Counted from the proposal
/// (`created_at`) or the blocked run (`ts`), so it holds whether that
/// challenge was rejected, expired, cancelled or settled. A runtime rule: no
/// reasoner or graph can shorten it.
async fn in_proposal_cooldown(
    conn: &mut PgConnection,
    agent: &AgentRec,
    market_key: &str,
    cooldown_secs: i64,
    now: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    if cooldown_secs <= 0 {
        return Ok(false);
    }
    let last: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT GREATEST( \
             (SELECT MAX(created_at) FROM interactions \
              WHERE proposer_id = $1 AND market_key = $2), \
             (SELECT MAX(ts) FROM agent_runs \
              WHERE agent_id = $1 AND market_key = $2 AND blocked IS NOT NULL \
                AND agent_version = $3))",
    )
    .bind(agent.id)
    .bind(market_key)
    .bind(agent.version)
    .fetch_one(conn)
    .await?;
    Ok(last.is_some_and(|t| (now - t).num_seconds() < cooldown_secs))
}

/// Marks a discovery run as a blocked proposal on `market_key`, which starts
/// the proposal cooldown for that market (see `in_proposal_cooldown`).
async fn mark_blocked(
    conn: &mut PgConnection,
    run_id: Uuid,
    market_key: &str,
    blocked: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE agent_runs SET market_key = $2, blocked = $3 WHERE id = $1")
        .bind(run_id)
        .bind(market_key)
        .bind(blocked)
        .execute(conn)
        .await?;
    Ok(())
}

async fn schedule_due(
    conn: &mut PgConnection,
    agent: Uuid,
    every_secs: i64,
    now: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let last: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT MAX(ts) FROM agent_runs WHERE agent_id = $1 AND trigger = $2")
            .bind(agent)
            .bind(TRIGGER_SCHEDULE)
            .fetch_one(conn)
            .await?;
    Ok(last.is_none_or(|t| (now - t).num_seconds() >= every_secs))
}

async fn discover_one(
    state: &AppState,
    agent_id: Uuid,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let settings = &state.settings;
    let mut conn = state.pool.acquire().await?;
    let Some(agent) = load_agent(&mut conn, agent_id).await? else {
        return Ok(());
    };
    let now = clock_for(agent.env.as_str(), now);
    // Not allowed to act: do not even reason. A vault agent does not propose
    // while this server has no chain access.
    if agent.snapshot.status != AgentStatus::Active
        || (agent.env == Env::ArcTestnet && state.chain.is_none())
    {
        return Ok(());
    }
    // A firm scout works without funds of its own; anyone else with nothing
    // to stake does not reason.
    let scouting = match agent.env {
        Env::Simulated => match crate::firms::membership(&mut conn, agent.id).await? {
            Some((firm, MemberRole::Scout)) => Some(firm),
            _ => None,
        },
        Env::ArcTestnet => None,
    };
    if agent.snapshot.available.is_zero() && scouting.is_none() {
        return Ok(());
    }

    let int = |node: &aen_core::graph::Node, name: &str| {
        node.params.get(name).and_then(Value::as_i64).unwrap_or(0)
    };
    let (trigger, symbol, window, threshold_bps, every_secs) =
        if let Some(node) = graph_exec::find_trigger(&agent.graph, TRIGGER_MARKET_MOVE) {
            let symbol = node
                .params
                .get("symbol")
                .and_then(Value::as_str)
                .unwrap_or(&settings.default_symbol)
                .to_owned();
            (
                TRIGGER_MARKET_MOVE,
                symbol,
                int(node, "window_secs").max(1),
                int(node, "threshold_bps"),
                0,
            )
        } else if let Some(node) = graph_exec::find_trigger(&agent.graph, TRIGGER_SCHEDULE) {
            (
                TRIGGER_SCHEDULE,
                settings.default_symbol.clone(),
                settings.default_window_secs,
                0,
                int(node, "every_secs").max(1),
            )
        } else {
            return Ok(());
        };
    if trigger == TRIGGER_SCHEDULE && !schedule_due(&mut conn, agent.id, every_secs, now).await? {
        return Ok(());
    }

    // The approach (docs/design/approach-and-firms.md): market filter and
    // stance gates always apply (defaults are no-ops); the ambition-derived
    // sizing (stake target, minimum conviction, open-deal cap) applies only
    // when the graph has an `approach.profile` node (D-64).
    let approach = Approach::from_graph(&agent.graph);
    let effective = approach.clone().unwrap_or_default();
    // A firm lead first consumes its scouts' best fresh candidate.
    let led = if agent.env == Env::Simulated {
        crate::firms::led_firm(&mut conn, agent.id).await?
    } else {
        None
    };
    let candidate = match &led {
        Some(firm) => crate::firms::best_candidate(&mut conn, firm, now).await?,
        None => None,
    };
    let candidate_id = candidate.as_ref().map(|(id, _)| *id);

    let min_expiry = now.timestamp() + settings.funding_window_secs + MIN_OPEN_AFTER_FUNDING_SECS;
    let discovered = match candidate {
        Some((_, view)) => Ok(view),
        None => market::discover(state.market.as_ref(), &symbol, window, min_expiry).await,
    };
    let view = match discovered {
        Ok(view) => view,
        Err(e) => {
            // Surfaced, never defaulted: without data the agent does nothing.
            record_once(
                &mut conn,
                agent.id,
                "market_data_error",
                format!(
                    "\"{}\" could not look for opportunities: market data is unavailable ({e}).",
                    agent.name
                ),
                json!({ "symbol": symbol, "error": e.to_string() }),
            )
            .await?;
            report.errors.push(format!("market data: {e}"));
            return Ok(());
        }
    };
    if candidate_id.is_none()
        && trigger == TRIGGER_MARKET_MOVE
        && view.move_bps.abs() < threshold_bps
    {
        return Ok(());
    }
    // Scout task: independent of the agent's own proposals.
    if let Some(firm) = &scouting {
        let outcome = crate::firms::scout(state, &agent, firm, &view, now, report).await;
        step(report, outcome);
    }
    if agent.snapshot.available.is_zero() || !may_initiate_challenges(&effective.stance) {
        return Ok(());
    }
    if let Err(misses) = market_matches(&effective.markets, &view.facts(now.timestamp())) {
        record_once(
            &mut conn,
            agent.id,
            "approach_filtered",
            format!(
                "\"{}\" skipped markets outside its approach ({}).",
                agent.name,
                miss_text(&misses)
            ),
            json!({ "market": view.market.key(), "title": view.title, "misses": misses }),
        )
        .await?;
        return Ok(());
    }
    if let Some(a) = &approach
        && open_interactions(&mut conn, agent.id).await? >= i64::from(a.max_open_interactions())
    {
        return Ok(());
    }
    let market_key = view.market.key();
    let cooldown = settings.proposal_cooldown_secs;
    // Checked before reasoning, so a cooling-down agent costs no model call.
    if has_open_on_market(&mut conn, agent.id, &market_key).await?
        || in_proposal_cooldown(&mut conn, &agent, &market_key, cooldown, now).await?
    {
        return Ok(());
    }
    drop(conn);

    let ctx = agent.ctx();
    let result = graph_exec::run(
        state.reasoner.as_ref(),
        &RunInput {
            graph: &agent.graph,
            trigger,
            agent: &ctx,
            market: &view,
            now: now.timestamp(),
            incoming: None,
            party: Party::A,
            funding_window_secs: settings.funding_window_secs,
            sandbox: state.sandbox.as_ref(),
        },
    )
    .await;
    let mut result = result;
    let filtered = apply_approach_sizing(&agent, approach.as_ref(), &mut result);
    // Analyst task: the firm's analysts review the lead's draft.
    let draft = result
        .intents
        .iter()
        .find(|p| matches!(p.intent, Intent::ProposeChallenge { .. }))
        .and_then(|p| p.terms.clone());
    let signals = match (&led, &draft) {
        (Some(firm), Some(terms)) if result.error.is_none() => {
            crate::firms::analyst_signals(state, firm, &view, terms, now, report).await?
        }
        _ => Vec::new(),
    };

    let mut tx = state.pool.begin().await?;
    // One proposal is applied at a time, so two ticks cannot both create the
    // same challenge or pick the same counterparty for one market.
    advisory_lock(&mut tx, "aen:propose").await?;
    if has_open_on_market(&mut tx, agent.id, &market_key).await?
        || in_proposal_cooldown(&mut tx, &agent, &market_key, cooldown, now).await?
        || (trigger == TRIGGER_SCHEDULE
            && !schedule_due(&mut tx, agent.id, every_secs, now).await?)
    {
        return Ok(());
    }
    let run_id = insert_run(
        &mut tx,
        &agent,
        trigger,
        None,
        &result,
        result.engine(state.reasoner.name()),
        now,
    )
    .await?;
    if let Some((summary, detail)) = filtered {
        record_once(&mut tx, agent.id, "approach_filtered", summary, detail).await?;
    }
    if let Some(error) = &result.error {
        let (kind, summary) = match &result.code {
            Some(_) => (
                "custom_code_failed",
                format!(
                    "\"{}\"'s {error}. Nothing was proposed and no funds moved.",
                    agent.name
                ),
            ),
            None => (
                "reasoning_failed",
                format!("\"{}\" could not form a proposal: {error}", agent.name),
            ),
        };
        record_once(
            &mut tx,
            agent.id,
            kind,
            summary,
            json!({ "run_id": run_id, "code": result.code }),
        )
        .await?;
        report.errors.push(format!("reasoner: {error}"));
        tx.commit().await?;
        return Ok(());
    }
    let planned = result
        .intents
        .iter()
        .find(|p| matches!(p.intent, Intent::ProposeChallenge { .. }));
    let Some(planned) = planned else {
        tx.commit().await?;
        return Ok(());
    };
    let Some(terms) = planned.terms.clone() else {
        tx.commit().await?;
        return Ok(());
    };
    if !terms.validate().is_empty()
        || planned.intent.stake() != Some(terms.stake)
        || terms.market != view.market
        || terms.start_ts <= now.timestamp()
    {
        report
            .errors
            .push("proposal terms failed validation".into());
        tx.commit().await?;
        return Ok(());
    }

    // The analyst gate (firm drafts only). Signals are recorded either way.
    let mut used_contributions: Vec<Uuid> = candidate_id.into_iter().collect();
    if let Some(firm) = &led
        && !signals.is_empty()
    {
        let min = approach.as_ref().map_or(
            agent.policy.min_conviction_bps,
            Approach::min_conviction_bps,
        );
        let (ids, approved) = crate::firms::record_signals(
            &mut tx,
            firm,
            &market_key,
            &terms,
            &signals,
            min,
            now,
            report,
        )
        .await?;
        if !approved {
            tx.commit().await?;
            return Ok(());
        }
        used_contributions.extend(ids);
    }

    // The gate: a fresh snapshot, read in the transaction that applies it.
    let fresh = require_agent(&mut tx, agent.id).await?;
    let decision = authorize(&planned.intent, &fresh.policy, &fresh.snapshot, None);
    if let Decision::Deny(reasons) = &decision {
        record_once(
            &mut tx,
            agent.id,
            "proposal_denied",
            format!(
                "\"{}\" wanted to stake {} on a challenge, but it was blocked: {}.",
                agent.name,
                stake_text(terms.stake, agent.env),
                deny_text(reasons)
            ),
            json!({ "reasons": reasons, "run_id": run_id, "terms": terms }),
        )
        .await?;
        // Rate-limited like a proposal, so a denied agent does not retry
        // (and reason, or call a model) on every tick.
        mark_blocked(&mut tx, run_id, &market_key, "denied").await?;
        report.denied += 1;
        tx.commit().await?;
        return Ok(());
    }

    let Some(counterparty) = pick_counterparty(&mut tx, &fresh, &market_key).await? else {
        record_once(
            &mut tx,
            agent.id,
            "no_counterparty",
            format!(
                "\"{}\" found an opportunity, but no other active agent is listening for challenges.",
                agent.name
            ),
            json!({ "run_id": run_id }),
        )
        .await?;
        mark_blocked(&mut tx, run_id, &market_key, "no_counterparty").await?;
        tx.commit().await?;
        return Ok(());
    };

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO interactions \
         (id, kind, env, state, proposer_id, counterparty_id, proposer_version, \
          counterparty_version, terms, stake, market_key, market_title, market_view, start_ts, \
          end_ts, turn, rounds, proposer_ask_owner, created_at, updated_at) \
         VALUES ($1, 'challenge', $2, 'proposed', $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, \
                 $4, 1, $14, $15, $15)",
    )
    .bind(id)
    .bind(fresh.env.as_str())
    .bind(fresh.id)
    .bind(counterparty.id)
    .bind(fresh.version)
    .bind(counterparty.version)
    .bind(serde_json::to_value(&terms).map_err(ApiError::internal)?)
    .bind(terms.stake.minor())
    .bind(&market_key)
    .bind(&view.title)
    .bind(serde_json::to_value(&view).map_err(ApiError::internal)?)
    .bind(ts(terms.start_ts)?)
    .bind(ts(terms.end_ts)?)
    .bind(planned.ask_owner)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    insert_round(
        &mut tx,
        id,
        1,
        fresh.id,
        "propose",
        &terms,
        &planned.summary,
        now,
    )
    .await?;
    sqlx::query("UPDATE agent_runs SET interaction_id = $2 WHERE id = $1")
        .bind(run_id)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    // A firm challenge: the contributions that fed it are used (and earn
    // their task fee).
    if let Some(firm) = &led {
        crate::firms::use_contributions(&mut tx, firm, id, &used_contributions).await?;
    }
    let approval_note = if decision == Decision::RequireApproval || planned.ask_owner {
        " Its owner will be asked to approve the stake before funds are committed."
    } else {
        ""
    };
    record(
        &mut tx,
        Some(fresh.id),
        Some(id),
        "challenge_proposed",
        format!(
            "\"{}\" challenged \"{}\": {} at {}. Stake {} each.{approval_note}",
            fresh.name,
            counterparty.name,
            claim_text(&terms, &view.title),
            ts(terms.end_ts)?.format("%Y-%m-%d %H:%M UTC"),
            stake_text(terms.stake, fresh.env),
        ),
        json!({
            "terms": terms, "market_source": view.source, "fallback_reason": view.fallback_reason,
            "run_id": run_id, "resolution": "backend_administered",
            "firm_id": led.as_ref().map(|f| f.id),
        }),
    )
    .await?;
    tx.commit().await?;
    report.proposed += 1;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_round(
    conn: &mut PgConnection,
    interaction: Uuid,
    seq: i32,
    agent: Uuid,
    action: &str,
    terms: &ChallengeTerms,
    summary: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO negotiation_rounds (interaction_id, seq, agent_id, action, terms, summary, ts) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(interaction)
    .bind(seq)
    .bind(agent)
    .bind(action)
    .bind(serde_json::to_value(terms).map_err(ApiError::internal)?)
    .bind(summary)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

async fn next_round_seq(conn: &mut PgConnection, interaction: Uuid) -> Result<i32, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT COALESCE(MAX(seq), 0) + 1 FROM negotiation_rounds WHERE interaction_id = $1",
    )
    .bind(interaction)
    .fetch_one(conn)
    .await
}

/// The first other active agent (oldest first) in the same environment that
/// listens for challenges and has no open interaction on this market.
///
/// Never a sibling (D-73): agents of one family (a multiplied agent and its
/// clones) do not challenge each other, since that would only move the
/// owner's money between the owner's own agents.
async fn pick_counterparty(
    conn: &mut PgConnection,
    proposer: &AgentRec,
    market_key: &str,
) -> Result<Option<AgentRec>, ApiError> {
    let family = crate::multiply::family_of(conn, proposer.id).await?;
    let candidates: Vec<Uuid> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT a.id FROM agents a LEFT JOIN agent_clones c ON c.agent_id = a.id \
         WHERE a.status = 'active' AND a.id <> $1 AND a.env = $2 \
           AND COALESCE(c.family_id, a.id) <> $3 \
           AND NOT EXISTS (SELECT 1 FROM interactions i WHERE i.market_key = $4 \
                           AND i.{OPEN_STATES_SQL} \
                           AND (i.proposer_id = a.id OR i.counterparty_id = a.id)) \
         ORDER BY a.created_at, a.id"
    )))
    .bind(proposer.id)
    .bind(proposer.env.as_str())
    .bind(family)
    .bind(market_key)
    .fetch_all(&mut *conn)
    .await?;
    for id in candidates {
        let Some(candidate) = load_agent(conn, id).await? else {
            continue;
        };
        if graph_exec::find_trigger(&candidate.graph, TRIGGER_CHALLENGE_RECEIVED).is_some()
            && !has_open_on_market(conn, id, market_key).await?
        {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------
// Negotiation
// ---------------------------------------------------------------------------

async fn respond_one(
    state: &AppState,
    id: Uuid,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let mut conn = state.pool.acquire().await?;
    let seen: Option<InteractionRow> = sqlx::query_as("SELECT * FROM interactions WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
    let Some(seen) = seen else { return Ok(()) };
    let now = clock_for(&seen.env, now);
    let negotiating = |s: InteractionState| {
        matches!(
            s,
            InteractionState::Proposed | InteractionState::Negotiating
        )
    };
    let Some(responder_id) = seen.turn else {
        return Ok(());
    };
    if !negotiating(seen.state()?) || seen.start_ts <= now {
        return Ok(());
    }
    let responder = require_agent(&mut conn, responder_id).await?;
    drop(conn);

    let terms = seen.terms()?;
    let party = seen.party_of(responder_id);
    let view: MarketView = serde_json::from_value(seen.market_view.clone())
        .map_err(|e| ApiError::internal(format!("stored market view is unreadable: {e}")))?;

    let outside_approach = market_matches(
        &Approach::effective(&responder.graph).markets,
        &view.facts(now.timestamp()),
    )
    .is_err();
    let result = if outside_approach {
        RunResult {
            intents: vec![PlannedIntent {
                intent: Intent::Reject,
                terms: None,
                ask_owner: false,
                summary: "This market is outside my approach.".into(),
            }],
            trace: Vec::new(),
            error: None,
            code: None,
            conviction_bps: None,
        }
    } else if responder.snapshot.status == AgentStatus::Paused {
        // A paused agent does not reason; the permission engine turns this
        // into a denial below.
        RunResult {
            intents: vec![PlannedIntent {
                intent: Intent::Reject,
                terms: None,
                ask_owner: false,
                summary: "The agent is paused.".into(),
            }],
            trace: Vec::new(),
            error: None,
            code: None,
            conviction_bps: None,
        }
    } else {
        let own_flow = graph_exec::find_trigger(&responder.graph, TRIGGER_CHALLENGE_RECEIVED);
        let fallback;
        let graph = if own_flow.is_some() {
            &responder.graph
        } else {
            fallback = graph_exec::default_response_graph(responder.policy.min_conviction_bps);
            &fallback
        };
        let ctx = responder.ctx();
        graph_exec::run(
            state.reasoner.as_ref(),
            &RunInput {
                graph,
                trigger: TRIGGER_CHALLENGE_RECEIVED,
                agent: &ctx,
                market: &view,
                now: now.timestamp(),
                incoming: Some(&terms),
                party,
                funding_window_secs: state.settings.funding_window_secs,
                sandbox: state.sandbox.as_ref(),
            },
        )
        .await
    };

    let mut tx = state.pool.begin().await?;
    let Some(mut row) = lock_interaction(&mut tx, id).await? else {
        return Ok(());
    };
    let current = row.state()?;
    // Someone else already answered (or the terms moved on): do nothing.
    if !negotiating(current) || row.turn != Some(responder_id) || row.rounds != seen.rounds {
        return Ok(());
    }
    let run_id = insert_run(
        &mut tx,
        &responder,
        TRIGGER_CHALLENGE_RECEIVED,
        Some(id),
        &result,
        result.engine(state.reasoner.name()),
        now,
    )
    .await?;
    if let Some(error) = &result.error {
        let (kind, summary) = match &result.code {
            Some(_) => (
                "custom_code_failed",
                format!(
                    "\"{}\" could not answer a challenge: {error}. No funds moved; it will try \
                     again.",
                    responder.name
                ),
            ),
            None => (
                "reasoning_failed",
                format!(
                    "\"{}\" could not answer a challenge: {error} It will try again.",
                    responder.name
                ),
            ),
        };
        record_once(
            &mut tx,
            responder.id,
            kind,
            summary,
            json!({ "run_id": run_id, "interaction_id": id, "code": result.code }),
        )
        .await?;
        report.errors.push(format!("reasoner: {error}"));
        tx.commit().await?;
        return Ok(());
    }
    let (a_name, b_name) = names(&mut tx, &row).await?;
    let other_name = if party == Party::A { &b_name } else { &a_name };
    let me = &responder.name;

    let planned = result.intents.iter().find(|p| {
        matches!(
            p.intent,
            Intent::AcceptChallenge { .. } | Intent::CounterOffer { .. } | Intent::Reject
        )
    });
    // A flow that ends without an action is a decline (default).
    let declined = PlannedIntent {
        intent: Intent::Reject,
        terms: None,
        ask_owner: false,
        summary: "The agent's flow ended without an answer.".into(),
    };
    let planned = planned.unwrap_or(&declined);

    let fresh = require_agent(&mut tx, responder.id).await?;
    let decision = authorize(
        &planned.intent,
        &fresh.policy,
        &fresh.snapshot,
        Some(current),
    );
    if let Decision::Deny(reasons) = &decision {
        let text = deny_text(reasons);
        cancel(
            &mut tx,
            &mut row,
            &format!("\"{me}\" could not answer: {text}."),
            "superseded",
            now,
        )
        .await?;
        record(
            &mut tx,
            Some(responder.id),
            Some(id),
            "response_denied",
            format!(
                "\"{me}\" could not answer the challenge from \"{other_name}\": {text}. The \
                 challenge was cancelled; no funds moved."
            ),
            json!({ "reasons": reasons, "intent": planned.intent, "run_id": run_id }),
        )
        .await?;
        report.denied += 1;
        report.cancelled += 1;
        tx.commit().await?;
        return Ok(());
    }

    let seq = next_round_seq(&mut tx, id).await?;
    match planned.intent {
        Intent::AcceptChallenge { stake } if stake == terms.stake => {
            insert_round(
                &mut tx,
                id,
                seq,
                responder.id,
                "accept",
                &terms,
                &planned.summary,
                now,
            )
            .await?;
            let column = if party == Party::A {
                "proposer_ask_owner"
            } else {
                "counterparty_ask_owner"
            };
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE interactions SET turn = NULL, {column} = {column} OR $2 WHERE id = $1"
            )))
            .bind(id)
            .bind(planned.ask_owner)
            .execute(&mut *tx)
            .await?;
            set_state(&mut tx, &mut row, InteractionState::Accepted, None, now).await?;
            record(
                &mut tx,
                Some(responder.id),
                Some(id),
                "challenge_accepted",
                format!(
                    "\"{me}\" accepted the challenge from \"{other_name}\": {}. Stake {} each.",
                    claim_text(&terms, &row.market_title),
                    stake_text(terms.stake, responder.env)
                ),
                json!({ "terms": terms, "run_id": run_id }),
            )
            .await?;
            report.accepted += 1;
        }
        Intent::CounterOffer { stake } => {
            let Some(new_terms) = planned.terms.clone() else {
                return Err(ApiError::internal("counter-offer without terms"));
            };
            let only_stake_and_threshold = ChallengeTerms {
                stake: terms.stake,
                threshold: terms.threshold,
                ..new_terms.clone()
            } == terms;
            if !new_terms.validate().is_empty()
                || !only_stake_and_threshold
                || new_terms == terms
                || stake != new_terms.stake
            {
                return Err(ApiError::internal("counter-offer terms failed validation"));
            }
            if row.rounds >= state.settings.max_negotiation_rounds {
                let reason = format!(
                    "Negotiation ended without agreement after {} offers (the limit).",
                    row.rounds
                );
                cancel(&mut tx, &mut row, &reason, "superseded", now).await?;
                record(
                    &mut tx,
                    Some(responder.id),
                    Some(id),
                    "negotiation_cap_reached",
                    format!(
                        "\"{a_name}\" and \"{b_name}\" could not agree after {} offers, so the \
                         challenge was cancelled. No funds moved.",
                        row.rounds
                    ),
                    json!({ "rounds": row.rounds, "run_id": run_id }),
                )
                .await?;
                report.cancelled += 1;
            } else {
                insert_round(
                    &mut tx,
                    id,
                    seq,
                    responder.id,
                    "counter",
                    &new_terms,
                    &planned.summary,
                    now,
                )
                .await?;
                sqlx::query(
                    "UPDATE interactions SET terms = $2, stake = $3, rounds = rounds + 1, \
                     turn = $4, updated_at = $5 WHERE id = $1",
                )
                .bind(id)
                .bind(serde_json::to_value(&new_terms).map_err(ApiError::internal)?)
                .bind(new_terms.stake.minor())
                .bind(row.other(responder.id))
                .bind(now)
                .execute(&mut *tx)
                .await?;
                if current == InteractionState::Proposed {
                    set_state(&mut tx, &mut row, InteractionState::Negotiating, None, now).await?;
                }
                record(
                    &mut tx,
                    Some(responder.id),
                    Some(id),
                    "challenge_countered",
                    format!(
                        "\"{me}\" made a counter-offer to \"{other_name}\": {}. Stake {} each \
                         (offer {} of {}).",
                        claim_text(&new_terms, &row.market_title),
                        stake_text(new_terms.stake, responder.env),
                        row.rounds + 1,
                        state.settings.max_negotiation_rounds
                    ),
                    json!({ "terms": new_terms, "previous_terms": terms, "run_id": run_id }),
                )
                .await?;
                report.countered += 1;
            }
        }
        Intent::Reject => {
            insert_round(
                &mut tx,
                id,
                seq,
                responder.id,
                "reject",
                &terms,
                &planned.summary,
                now,
            )
            .await?;
            cancel(
                &mut tx,
                &mut row,
                &format!("\"{me}\" rejected the challenge: {}", planned.summary),
                "superseded",
                now,
            )
            .await?;
            record(
                &mut tx,
                Some(responder.id),
                Some(id),
                "challenge_rejected",
                format!(
                    "\"{me}\" rejected the challenge from \"{other_name}\": {} No funds moved.",
                    planned.summary
                ),
                json!({
                    "run_id": run_id,
                    "reason": outside_approach.then_some("outside_approach"),
                }),
            )
            .await?;
            report.rejected += 1;
            report.cancelled += 1;
        }
        _ => {
            return Err(ApiError::internal(
                "answer does not match the terms on the table",
            ));
        }
    }
    tx.commit().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Commit (authorization at execution time)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitOutcome {
    /// The interaction was not waiting for funds.
    Skipped,
    AwaitingApproval {
        newly_requested: u32,
    },
    Funded,
    Denied,
    Expired,
}

async fn commit_one(
    state: &AppState,
    id: Uuid,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let mut tx = state.pool.begin().await?;
    let outcome = try_commit(&mut tx, id, now).await?;
    tx.commit().await?;
    match outcome {
        CommitOutcome::Funded => report.funded += 1,
        CommitOutcome::AwaitingApproval { newly_requested } => {
            report.approvals_requested += newly_requested;
        }
        CommitOutcome::Denied => {
            report.denied += 1;
            report.cancelled += 1;
        }
        CommitOutcome::Expired => report.cancelled += 1,
        CommitOutcome::Skipped => {}
    }
    Ok(())
}

/// Tries to commit both stakes of an accepted challenge, inside the caller's
/// transaction. Both stakes are committed, or neither is.
///
/// Each party is authorized against a snapshot read here, after locking the
/// interaction, both agents and their available accounts, so nothing can
/// change a balance or an exposure between the check and the ledger write.
/// An owner approval satisfies `RequireApproval` only; a `Deny` still blocks.
pub async fn try_commit(
    conn: &mut PgConnection,
    id: Uuid,
    now: DateTime<Utc>,
) -> Result<CommitOutcome, ApiError> {
    let Some(mut row) = lock_interaction(conn, id).await? else {
        return Ok(CommitOutcome::Skipped);
    };
    let current = row.state()?;
    if !matches!(
        current,
        InteractionState::Accepted | InteractionState::AwaitingApproval
    ) {
        return Ok(CommitOutcome::Skipped);
    }
    let now = clock_for(&row.env, now);
    if row.start_ts <= now {
        expire_locked(conn, &mut row, now).await?;
        return Ok(CommitOutcome::Expired);
    }
    let env = row.env()?;
    if env == Env::ArcTestnet {
        // Both sides must still be vault agents; the onchain stake itself
        // happens in `lifecycle_arc` after this ledger reservation.
        let vaults: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM agents WHERE id = ANY($1) AND env = 'arc_testnet' \
             AND vault_address IS NOT NULL",
        )
        .bind(vec![row.proposer_id, row.counterparty_id])
        .fetch_one(&mut *conn)
        .await?;
        if vaults != 2 {
            cancel(
                conn,
                &mut row,
                "Stakes could not be committed: both agents need a linked Arc testnet vault.",
                "superseded",
                now,
            )
            .await?;
            return Ok(CommitOutcome::Denied);
        }
    }
    let stake = row.stake()?;
    let (a_name, b_name) = names(conn, &row).await?;

    // Fixed lock order (sorted agent ids): concurrent commits cannot deadlock.
    let mut parties = [row.proposer_id, row.counterparty_id];
    parties.sort();
    for agent in parties {
        advisory_lock(conn, &format!("aen:agent:{agent}")).await?;
    }
    for agent in parties {
        let account = Account::AgentAvailable(agent);
        advisory_lock(conn, &format!("acct:{}:{account}", env.as_str())).await?;
    }

    let intent = Intent::CommitFunds {
        interaction_type: InteractionType::Challenge,
        stake,
    };
    let mut denied: Vec<(AgentRec, Vec<DenyReason>)> = Vec::new();
    let mut need_approval: Vec<AgentRec> = Vec::new();
    for (agent, ask_owner) in [
        (row.proposer_id, row.proposer_ask_owner),
        (row.counterparty_id, row.counterparty_ask_owner),
    ] {
        let rec = require_agent(conn, agent).await?;
        match authorize(&intent, &rec.policy, &rec.snapshot, Some(current)) {
            Decision::Deny(reasons) => denied.push((rec, reasons)),
            Decision::RequireApproval => need_approval.push(rec),
            Decision::Allow if ask_owner => need_approval.push(rec),
            Decision::Allow => {}
        }
    }

    if !denied.is_empty() {
        let text = denied
            .iter()
            .map(|(rec, reasons)| format!("\"{}\": {}", rec.name, deny_text(reasons)))
            .collect::<Vec<_>>()
            .join(" / ");
        cancel(
            conn,
            &mut row,
            &format!("Stakes could not be committed ({text})."),
            "superseded",
            now,
        )
        .await?;
        for (rec, reasons) in &denied {
            record(
                conn,
                Some(rec.id),
                Some(id),
                "commit_denied",
                format!(
                    "\"{}\" was not allowed to commit {} to its challenge: {}. The challenge \
                     between \"{a_name}\" and \"{b_name}\" was cancelled; no funds moved.",
                    rec.name,
                    stake_text(stake, env),
                    deny_text(reasons)
                ),
                json!({ "reasons": reasons, "stake": stake }),
            )
            .await?;
        }
        return Ok(CommitOutcome::Denied);
    }

    let mut waiting = false;
    let mut newly_requested = 0;
    for rec in &need_approval {
        let existing: Option<(String, i64)> = sqlx::query_as(
            "SELECT status, stake FROM approvals WHERE interaction_id = $1 AND agent_id = $2",
        )
        .bind(id)
        .bind(rec.id)
        .fetch_optional(&mut *conn)
        .await?;
        match existing {
            // An approval counts only for the exact stake it was given for.
            Some((status, approved_stake))
                if status == "approved" && approved_stake == stake.minor() => {}
            Some(_) => waiting = true,
            None => {
                let reason = if stake > rec.policy.approval_threshold {
                    format!(
                        "The stake is above this agent's automatic limit of {}.",
                        display_usdc(rec.policy.approval_threshold)
                    )
                } else {
                    "This agent's flow asks for owner approval.".to_owned()
                };
                sqlx::query(
                    "INSERT INTO approvals (id, interaction_id, agent_id, stake, status, reason, \
                     requested_at) VALUES ($1, $2, $3, $4, 'pending', $5, $6)",
                )
                .bind(Uuid::new_v4())
                .bind(id)
                .bind(rec.id)
                .bind(stake.minor())
                .bind(&reason)
                .bind(now)
                .execute(&mut *conn)
                .await?;
                record(
                    conn,
                    Some(rec.id),
                    Some(id),
                    "approval_requested",
                    format!(
                        "\"{}\" needs your approval to commit {} to a challenge. {reason}",
                        rec.name,
                        stake_text(stake, env)
                    ),
                    json!({ "stake": stake }),
                )
                .await?;
                waiting = true;
                newly_requested += 1;
            }
        }
    }
    if waiting {
        if current == InteractionState::Accepted {
            set_state(
                conn,
                &mut row,
                InteractionState::AwaitingApproval,
                None,
                now,
            )
            .await?;
        }
        return Ok(CommitOutcome::AwaitingApproval { newly_requested });
    }

    // Both authorized: commit both stakes in this transaction. If either
    // posting fails the error propagates and the caller's transaction rolls
    // back, so one side can never be funded alone.
    for agent in [row.proposer_id, row.counterparty_id] {
        let set = ledger::commit_stake(env, agent, id, stake)?;
        ledger_db::post(conn, &set).await?;
    }
    set_state(conn, &mut row, InteractionState::Funded, None, now).await?;
    if env == Env::ArcTestnet {
        // Reserved in the ledger; `lifecycle_arc` stakes both vaults into the
        // escrow and moves the challenge to Active once the escrow says so.
        record(
            conn,
            Some(row.proposer_id),
            Some(id),
            "stake_reserved",
            format!(
                "\"{a_name}\" and \"{b_name}\" each reserved {} for their challenge. Both \
                 vaults will now stake it into the Arc testnet escrow before {}.",
                stake_text(stake, env),
                row.start_ts.format("%Y-%m-%d %H:%M:%S UTC")
            ),
            json!({ "stake": stake, "env": env }),
        )
        .await?;
        return Ok(CommitOutcome::Funded);
    }
    set_state(conn, &mut row, InteractionState::Active, None, now).await?;
    let terms = row.terms()?;
    for (agent, name, side) in [
        (row.proposer_id, &a_name, terms.side_of(Party::A)),
        (row.counterparty_id, &b_name, terms.side_of(Party::B)),
    ] {
        record(
            conn,
            Some(agent),
            Some(id),
            "stake_committed",
            format!(
                "\"{name}\" committed {} to its challenge and holds \"{} {}\". The challenge is \
                 active until {}.",
                stake_text(stake, env),
                side_phrase(side),
                format_value(terms.formula, terms.threshold),
                row.end_ts.format("%Y-%m-%d %H:%M UTC")
            ),
            json!({ "stake": stake, "env": env, "side": side }),
        )
        .await?;
    }
    Ok(CommitOutcome::Funded)
}

// ---------------------------------------------------------------------------
// Approvals
// ---------------------------------------------------------------------------

/// The owner's decision on one approval. Approving re-runs `try_commit`, so
/// the stake is authorized again against current balances and limits.
/// Returns the interaction id.
pub async fn decide_approval(
    state: &AppState,
    approval_id: Uuid,
    approve: bool,
    now: DateTime<Utc>,
) -> Result<Uuid, ApiError> {
    let mut tx = state.pool.begin().await?;
    let found: Option<(Uuid, Uuid)> =
        sqlx::query_as("SELECT interaction_id, agent_id FROM approvals WHERE id = $1")
            .bind(approval_id)
            .fetch_optional(&mut *tx)
            .await?;
    let (interaction_id, agent_id) = found.ok_or_else(|| ApiError::not_found("approval"))?;
    // Interaction first, then the approval: the same order `try_commit` uses.
    let mut row = lock_interaction(&mut tx, interaction_id)
        .await?
        .ok_or_else(|| ApiError::internal("approval without interaction"))?;
    let status: String =
        sqlx::query_scalar("SELECT status FROM approvals WHERE id = $1 FOR UPDATE")
            .bind(approval_id)
            .fetch_one(&mut *tx)
            .await?;
    if status != "pending" {
        return Err(ApiError::conflict(
            "approval_not_pending",
            format!("this approval is already {status}"),
        ));
    }
    if row.state()? != InteractionState::AwaitingApproval {
        return Err(ApiError::conflict(
            "approval_stale",
            "the challenge is no longer waiting for approval",
        ));
    }
    if row.start_ts <= now {
        // Too late: record the expiry, then tell the caller.
        expire_locked(&mut tx, &mut row, now).await?;
        tx.commit().await?;
        return Err(ApiError::conflict(
            "approval_expired",
            "the funding deadline has passed; the challenge was cancelled",
        ));
    }
    let agent_name: String = sqlx::query_scalar("SELECT name FROM agents WHERE id = $1")
        .bind(agent_id)
        .fetch_one(&mut *tx)
        .await?;
    let stake = stake_text(row.stake()?, row.env()?);
    sqlx::query("UPDATE approvals SET status = $2, decided_at = $3 WHERE id = $1")
        .bind(approval_id)
        .bind(if approve { "approved" } else { "rejected" })
        .bind(now)
        .execute(&mut *tx)
        .await?;
    if approve {
        record(
            &mut tx,
            Some(agent_id),
            Some(interaction_id),
            "approval_granted",
            format!("You approved \"{agent_name}\" committing {stake} to its challenge."),
            json!({ "approval_id": approval_id }),
        )
        .await?;
        // Not authorization by itself: limits and balances are checked again.
        try_commit(&mut tx, interaction_id, now).await?;
    } else {
        cancel(
            &mut tx,
            &mut row,
            &format!("The owner of \"{agent_name}\" rejected the stake."),
            "superseded",
            now,
        )
        .await?;
        record(
            &mut tx,
            Some(agent_id),
            Some(interaction_id),
            "approval_rejected",
            format!(
                "You rejected \"{agent_name}\" committing {stake}. The challenge was cancelled; \
                 no funds moved."
            ),
            json!({ "approval_id": approval_id }),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(interaction_id)
}

// ---------------------------------------------------------------------------
// Resolution and settlement
// ---------------------------------------------------------------------------

async fn start_resolving(
    state: &AppState,
    id: Uuid,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let mut tx = state.pool.begin().await?;
    let Some(mut row) = lock_interaction(&mut tx, id).await? else {
        return Ok(());
    };
    let now = clock_for(&row.env, now);
    if row.state()? != InteractionState::Active || row.end_ts > now {
        return Ok(());
    }
    set_state(&mut tx, &mut row, InteractionState::Resolving, None, now).await?;
    tx.commit().await?;
    report.resolving += 1;
    Ok(())
}

async fn resolve_one(
    state: &AppState,
    id: Uuid,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let seen: Option<InteractionRow> = sqlx::query_as("SELECT * FROM interactions WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    let Some(seen) = seen else { return Ok(()) };
    if seen.state()? != InteractionState::Resolving {
        return Ok(());
    }
    let terms = seen.terms()?;
    // Network read outside any transaction. An error stays an error: a
    // missing price is never replaced by a default.
    let observation = market::observe(state.market.as_ref(), &terms.market, terms.formula).await;
    if let Err(e) = &observation {
        report.errors.push(format!("market data: {e}"));
    }

    let mut tx = state.pool.begin().await?;
    let settled = settle_locked(&mut tx, &state.settings, id, observation, now).await?;
    tx.commit().await?;
    match settled {
        Some(Outcome::Void) => report.voided += 1,
        Some(_) => report.settled += 1,
        None => {}
    }
    Ok(())
}

/// Settles (or voids) a `Resolving` interaction inside the caller's
/// transaction, under a row lock, after re-checking the state: the second of
/// two concurrent settlements finds `Settled` and does nothing (SEC-03).
/// Returns `None` when nothing was settled.
pub async fn settle_locked(
    conn: &mut PgConnection,
    settings: &Settings,
    id: Uuid,
    observation: Result<market::Observation, market::MarketError>,
    now: DateTime<Utc>,
) -> Result<Option<Outcome>, ApiError> {
    let Some(mut row) = lock_interaction(conn, id).await? else {
        return Ok(None);
    };
    if row.state()? != InteractionState::Resolving {
        return Ok(None);
    }
    let names = names(conn, &row).await?;
    let Some(decided) = decide_outcome(conn, settings, &row, &observation, &names, now).await?
    else {
        return Ok(None);
    };
    apply_outcome(conn, &mut row, &decided, &names, None, now).await
}

/// What the market data says about a `Resolving` challenge. Kept as JSON in
/// `interactions.pending_outcome` while an onchain call is retried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub(crate) struct Decided {
    pub outcome: Outcome,
    /// Why the challenge is void (empty unless `outcome` is `Void`).
    pub void_reason: String,
    pub observed: i64,
    pub source: String,
    pub detail: String,
}

/// Decides the outcome from one observation. `None` means wait: market data
/// is missing and the grace period has not ended (the delay is recorded once).
/// The caller holds the row lock.
pub(crate) async fn decide_outcome(
    conn: &mut PgConnection,
    settings: &Settings,
    row: &InteractionRow,
    observation: &Result<market::Observation, market::MarketError>,
    (a_name, b_name): &(String, String),
    now: DateTime<Utc>,
) -> Result<Option<Decided>, ApiError> {
    let terms = row.terms()?;
    let (outcome, void_reason) = match observation {
        Ok(obs) => match resolve(&terms, Some(obs.value)) {
            Outcome::Void => (
                Outcome::Void,
                format!("the market returned an impossible value ({})", obs.value),
            ),
            decided => (decided, String::new()),
        },
        Err(e) => {
            let deadline = row.end_ts + chrono::Duration::seconds(settings.resolution_grace_secs);
            if now < deadline {
                // Keep trying until the grace period ends.
                sqlx::query(
                    "UPDATE interactions SET resolve_attempts = resolve_attempts + 1, \
                     status_reason = $2, updated_at = $3 WHERE id = $1",
                )
                .bind(row.id)
                .bind(format!("Waiting for market data: {e}"))
                .bind(now)
                .execute(&mut *conn)
                .await?;
                if row.resolve_attempts == 0 {
                    record(
                        conn,
                        Some(row.proposer_id),
                        Some(row.id),
                        "resolution_delayed",
                        format!(
                            "The challenge between \"{a_name}\" and \"{b_name}\" could not be \
                             resolved yet: market data is unavailable ({e}). It will be retried \
                             until {}, then refunded.",
                            deadline.format("%Y-%m-%d %H:%M UTC")
                        ),
                        json!({ "error": e.to_string() }),
                    )
                    .await?;
                }
                return Ok(None);
            }
            (
                resolve(&terms, None),
                format!("no market data was available within the grace period ({e})"),
            )
        }
    };
    let obs = observation.as_ref().ok();
    Ok(Some(Decided {
        outcome,
        void_reason,
        observed: obs.map(|o| o.value).unwrap_or_default(),
        source: obs.map(|o| o.source.to_owned()).unwrap_or_default(),
        detail: obs.map(|o| o.detail.clone()).unwrap_or_default(),
    }))
}

/// Posts the decided outcome to the ledger and closes the interaction. The
/// caller holds the row lock and has checked it is `Resolving`. `tx_hash` is
/// the escrow transaction that already moved the funds onchain, if any.
pub(crate) async fn apply_outcome(
    conn: &mut PgConnection,
    row: &mut InteractionRow,
    decided: &Decided,
    (a_name, b_name): &(String, String),
    tx_hash: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Option<Outcome>, ApiError> {
    let id = row.id;
    let terms = row.terms()?;
    let stake = row.stake()?;
    let env = row.env()?;
    // Same agent locks, in the same order, as `try_commit`: everything that
    // moves an agent's stake is serialized per agent.
    let mut parties = [row.proposer_id, row.counterparty_id];
    parties.sort();
    for agent in parties {
        advisory_lock(conn, &format!("aen:agent:{agent}")).await?;
    }
    let with_tx = |set: ledger::PostingSet| match tx_hash {
        Some(tx) => set.with_tx_hash(tx),
        None => set,
    };
    let onchain = if env == Env::ArcTestnet {
        " Funds moved onchain through the Arc testnet escrow."
    } else {
        ""
    };

    let (winner, loser) = match decided.outcome {
        Outcome::A => (row.proposer_id, row.counterparty_id),
        Outcome::B => (row.counterparty_id, row.proposer_id),
        Outcome::Void => {
            for agent in [row.proposer_id, row.counterparty_id] {
                ledger_db::post(conn, &with_tx(ledger::refund(env, agent, id, stake)?)).await?;
            }
            sqlx::query("UPDATE interactions SET outcome = 'void' WHERE id = $1")
                .bind(id)
                .execute(&mut *conn)
                .await?;
            let void_reason = &decided.void_reason;
            let reason = format!("Voided: {void_reason}. Both stakes were refunded.");
            set_state(conn, row, InteractionState::Cancelled, Some(&reason), now).await?;
            for (agent, name) in [(row.proposer_id, a_name), (row.counterparty_id, b_name)] {
                record(
                    conn,
                    Some(agent),
                    Some(id),
                    "challenge_voided",
                    format!(
                        "The challenge between \"{a_name}\" and \"{b_name}\" was voided: \
                         {void_reason}. {} was refunded to \"{name}\".{onchain}",
                        stake_text(stake, env)
                    ),
                    json!({ "refund": stake, "env": env, "tx_hash": tx_hash }),
                )
                .await?;
            }
            return Ok(Some(Outcome::Void));
        }
    };

    let Decided {
        observed,
        source,
        detail,
        ..
    } = decided;
    ledger_db::post(
        conn,
        &with_tx(ledger::settle(env, id, winner, loser, stake)?),
    )
    .await?;
    sqlx::query(
        "UPDATE interactions SET outcome = $2, winner_id = $3, observed = $4, observed_at = $5, \
         settled_at = $5 WHERE id = $1",
    )
    .bind(id)
    .bind(if decided.outcome == Outcome::A {
        "a"
    } else {
        "b"
    })
    .bind(winner)
    .bind(observed)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    let reason = format!(
        "Observed {} from {source} ({detail}); resolution is backend-administered.",
        format_value(terms.formula, *observed)
    );
    set_state(conn, row, InteractionState::Settled, Some(&reason), now).await?;
    let name_of = |agent: Uuid| {
        if agent == row.proposer_id {
            a_name
        } else {
            b_name
        }
    };
    for (agent, won) in [(winner, true), (loser, false)] {
        let (verb, sign) = if won { ("won", "+") } else { ("lost", "-") };
        record(
            conn,
            Some(agent),
            Some(id),
            "challenge_settled",
            format!(
                "\"{}\" {verb} its challenge against \"{}\": {} was observed at {} (threshold \
                 {}). Realized P&L {sign}{}.{onchain}",
                name_of(agent),
                name_of(if won { loser } else { winner }),
                row.market_title,
                format_value(terms.formula, *observed),
                format_value(terms.formula, terms.threshold),
                stake_text(stake, env)
            ),
            json!({
                "outcome": decided.outcome, "winner_id": winner, "observed": observed,
                "source": source, "stake": stake, "env": env,
                "resolution": "backend_administered", "tx_hash": tx_hash,
            }),
        )
        .await?;
    }
    // A firm challenge: the lead shares a profit with its members.
    crate::firms::pay_profit_share(conn, id).await?;
    Ok(Some(decided.outcome))
}
