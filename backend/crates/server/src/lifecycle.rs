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
//! - Only the `simulated` environment is handled. Nothing here signs or
//!   sends a transaction.

use aen_core::authorize::{AgentStatus, Decision, DenyReason, Intent, Snapshot, authorize};
use aen_core::challenge::{ChallengeTerms, Direction, FormulaId, Outcome, Party, resolve};
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

    fn stake(&self) -> Result<Money, ApiError> {
        Money::from_minor(self.stake).ok_or_else(|| ApiError::internal("negative stake"))
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
    pub funded: u32,
    pub resolving: u32,
    pub settled: u32,
    pub voided: u32,
    pub cancelled: u32,
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
    fn ctx(&self) -> AgentCtx {
        AgentCtx {
            id: self.id,
            name: self.name.clone(),
            version: self.version,
            config: self.config.clone(),
            policy: self.policy.clone(),
            available: self.snapshot.available,
            exposure: self.snapshot.exposure,
        }
    }
}

/// Exposure = stakes of the agent's interactions in Funded / Active /
/// Resolving. Realized P&L = the agent's net over `settle` ledger entries.
const AGENT_SQL: &str = "
SELECT a.id, a.name, a.status, a.env, a.current_version, v.config, v.policy, v.graph,
  (SELECT COALESCE(SUM(e.delta), 0)::BIGINT FROM ledger_entries e
    WHERE e.env = a.env AND e.account = 'agent:' || a.id || ':available') AS available,
  (SELECT COALESCE(SUM(i.stake), 0)::BIGINT FROM interactions i
    WHERE i.state IN ('funded', 'active', 'resolving')
      AND (i.proposer_id = a.id OR i.counterparty_id = a.id)) AS exposure,
  (SELECT COALESCE(SUM(e.delta), 0)::BIGINT FROM ledger_entries e
    WHERE e.agent_id = a.id AND e.env = a.env AND e.reason = 'settle') AS realized_pnl
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

async fn require_agent(conn: &mut PgConnection, id: Uuid) -> Result<AgentRec, ApiError> {
    load_agent(conn, id)
        .await?
        .ok_or_else(|| ApiError::internal(format!("agent {id} is missing")))
}

async fn advisory_lock(conn: &mut PgConnection, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(name)
        .execute(conn)
        .await?;
    Ok(())
}

async fn lock_interaction(
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
async fn set_state(
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

async fn record(
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
async fn record_once(
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

fn deny_text(reasons: &[DenyReason]) -> String {
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

fn side_phrase(direction: Direction) -> &'static str {
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

fn stake_text(stake: Money) -> String {
    format!("{} (simulated)", display_usdc(stake))
}

fn ts(secs: i64) -> Result<DateTime<Utc>, ApiError> {
    DateTime::from_timestamp(secs, 0).ok_or_else(|| ApiError::internal("timestamp out of range"))
}

async fn names(
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

    for id in ids(
        pool,
        "SELECT id FROM interactions WHERE state IN ('proposed', 'negotiating', 'accepted', \
         'awaiting_approval') AND start_ts <= $1 ORDER BY created_at, id",
        Some(now),
    )
    .await?
    {
        step(&mut report, expire_one(state, id, now).await);
    }

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

    for id in ids(
        pool,
        "SELECT id FROM interactions WHERE state = 'active' AND end_ts <= $1 \
         ORDER BY created_at, id",
        Some(now),
    )
    .await?
    {
        let outcome = start_resolving(state, id, now, &mut report).await;
        step(&mut report, outcome);
    }

    for id in ids(
        pool,
        "SELECT id FROM interactions WHERE state = 'resolving' ORDER BY created_at, id",
        None,
    )
    .await?
    {
        let outcome = resolve_one(state, id, now, &mut report).await;
        step(&mut report, outcome);
    }

    Ok(report)
}

fn step(report: &mut TickReport, outcome: Result<(), ApiError>) {
    if let Err(e) = outcome {
        report.errors.push(format!("{}: {}", e.code, e.message));
    }
}

async fn ids(pool: &PgPool, sql: &str, now: Option<DateTime<Utc>>) -> Result<Vec<Uuid>, ApiError> {
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
    // Nothing to stake, or not allowed to act: do not even reason.
    if agent.snapshot.status != AgentStatus::Active || agent.snapshot.available.is_zero() {
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

    let min_expiry = now.timestamp() + settings.funding_window_secs + MIN_OPEN_AFTER_FUNDING_SECS;
    let view = match market::discover(state.market.as_ref(), &symbol, window, min_expiry).await {
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
    if trigger == TRIGGER_MARKET_MOVE && view.move_bps.abs() < threshold_bps {
        return Ok(());
    }
    let market_key = view.market.key();
    if has_open_on_market(&mut conn, agent.id, &market_key).await? {
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
        },
    )
    .await;

    let mut tx = state.pool.begin().await?;
    // One proposal is applied at a time, so two ticks cannot both create the
    // same challenge or pick the same counterparty for one market.
    advisory_lock(&mut tx, "aen:propose").await?;
    if has_open_on_market(&mut tx, agent.id, &market_key).await?
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
        state.reasoner.name(),
        now,
    )
    .await?;
    if let Some(error) = &result.error {
        record_once(
            &mut tx,
            agent.id,
            "reasoning_failed",
            format!("\"{}\" could not form a proposal: {error}", agent.name),
            json!({ "run_id": run_id }),
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
                stake_text(terms.stake),
                deny_text(reasons)
            ),
            json!({ "reasons": reasons, "run_id": run_id, "terms": terms }),
        )
        .await?;
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
            stake_text(terms.stake),
        ),
        json!({
            "terms": terms, "market_source": view.source, "fallback_reason": view.fallback_reason,
            "run_id": run_id, "resolution": "backend_administered",
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
async fn pick_counterparty(
    conn: &mut PgConnection,
    proposer: &AgentRec,
    market_key: &str,
) -> Result<Option<AgentRec>, ApiError> {
    let candidates: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM agents WHERE status = 'active' AND id <> $1 AND env = $2 \
         ORDER BY created_at, id",
    )
    .bind(proposer.id)
    .bind(proposer.env.as_str())
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

    let result = if responder.snapshot.status == AgentStatus::Paused {
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
        state.reasoner.name(),
        now,
    )
    .await?;
    if let Some(error) = &result.error {
        record_once(
            &mut tx,
            responder.id,
            "reasoning_failed",
            format!(
                "\"{}\" could not answer a challenge: {error} It will try again.",
                responder.name
            ),
            json!({ "run_id": run_id, "interaction_id": id }),
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
                    stake_text(terms.stake)
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
                        stake_text(new_terms.stake),
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
                json!({ "run_id": run_id }),
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
    if row.start_ts <= now {
        expire_locked(conn, &mut row, now).await?;
        return Ok(CommitOutcome::Expired);
    }
    let env: Env = row
        .env
        .parse()
        .map_err(|()| ApiError::internal("unknown env"))?;
    if env != Env::Simulated {
        // On-chain funding needs the custody decision (D-3); refuse.
        return Err(ApiError::internal("only simulated funds can be committed"));
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
                    stake_text(stake),
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
                        stake_text(stake)
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
                stake_text(stake),
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
    let stake = stake_text(row.stake()?);
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
    let terms = row.terms()?;
    let stake = row.stake()?;
    let env: Env = row
        .env
        .parse()
        .map_err(|()| ApiError::internal("unknown env"))?;
    let (a_name, b_name) = names(conn, &row).await?;
    // Same agent locks, in the same order, as `try_commit`: everything that
    // moves an agent's stake is serialized per agent.
    let mut parties = [row.proposer_id, row.counterparty_id];
    parties.sort();
    for agent in parties {
        advisory_lock(conn, &format!("aen:agent:{agent}")).await?;
    }

    let (outcome, void_reason) = match &observation {
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
                .bind(id)
                .bind(format!("Waiting for market data: {e}"))
                .bind(now)
                .execute(&mut *conn)
                .await?;
                if row.resolve_attempts == 0 {
                    record(
                        conn,
                        Some(row.proposer_id),
                        Some(id),
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

    let (winner, loser) = match outcome {
        Outcome::A => (row.proposer_id, row.counterparty_id),
        Outcome::B => (row.counterparty_id, row.proposer_id),
        Outcome::Void => {
            for agent in [row.proposer_id, row.counterparty_id] {
                ledger_db::post(conn, &ledger::refund(env, agent, id, stake)?).await?;
            }
            sqlx::query("UPDATE interactions SET outcome = 'void' WHERE id = $1")
                .bind(id)
                .execute(&mut *conn)
                .await?;
            let reason = format!("Voided: {void_reason}. Both stakes were refunded.");
            set_state(
                conn,
                &mut row,
                InteractionState::Cancelled,
                Some(&reason),
                now,
            )
            .await?;
            for (agent, name) in [(row.proposer_id, &a_name), (row.counterparty_id, &b_name)] {
                record(
                    conn,
                    Some(agent),
                    Some(id),
                    "challenge_voided",
                    format!(
                        "The challenge between \"{a_name}\" and \"{b_name}\" was voided: \
                         {void_reason}. {} was refunded to \"{name}\".",
                        stake_text(stake)
                    ),
                    json!({ "refund": stake, "env": env }),
                )
                .await?;
            }
            return Ok(Some(Outcome::Void));
        }
    };

    let observed = observation.as_ref().map(|o| o.value).unwrap_or_default();
    let source = observation.as_ref().map(|o| o.source).unwrap_or_default();
    let detail = observation
        .as_ref()
        .map(|o| o.detail.clone())
        .unwrap_or_default();
    ledger_db::post(conn, &ledger::settle(env, id, winner, loser, stake)?).await?;
    sqlx::query(
        "UPDATE interactions SET outcome = $2, winner_id = $3, observed = $4, observed_at = $5 \
         WHERE id = $1",
    )
    .bind(id)
    .bind(if outcome == Outcome::A { "a" } else { "b" })
    .bind(winner)
    .bind(observed)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    let reason = format!(
        "Observed {} from {source} ({detail}); resolution is backend-administered.",
        format_value(terms.formula, observed)
    );
    set_state(
        conn,
        &mut row,
        InteractionState::Settled,
        Some(&reason),
        now,
    )
    .await?;
    let name_of = |agent: Uuid| {
        if agent == row.proposer_id {
            &a_name
        } else {
            &b_name
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
                 {}). Realized P&L {sign}{}.",
                name_of(agent),
                name_of(if won { loser } else { winner }),
                row.market_title,
                format_value(terms.formula, observed),
                format_value(terms.formula, terms.threshold),
                stake_text(stake)
            ),
            json!({
                "outcome": outcome, "winner_id": winner, "observed": observed, "source": source,
                "stake": stake, "env": env, "resolution": "backend_administered",
            }),
        )
        .await?;
    }
    Ok(Some(outcome))
}
