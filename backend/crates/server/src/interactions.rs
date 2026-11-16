//! M2 endpoints: interactions, approvals, portfolio stats and the dev tick.

use std::collections::BTreeMap;

use aen_core::challenge::ChallengeTerms;
use aen_core::ledger::Env;
use aen_core::money::{Money, SignedMoney};
use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::agents;
use crate::error::{ApiError, Detail};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::ledger_db;
use crate::lifecycle::{self, InteractionRow, TickReport, claim_text, format_value};
use crate::market::fake::FakeMarketUpdate;
use crate::models::{ActivityItem, LedgerEntry};
use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 100;
const MAX_LIMIT: i64 = 500;

#[derive(sqlx::FromRow)]
struct JoinedRow {
    #[sqlx(flatten)]
    row: InteractionRow,
    proposer_name: String,
    counterparty_name: String,
    pending_approvals: i64,
}

const INTERACTIONS_SQL: &str = "
SELECT i.*, pa.name AS proposer_name, pb.name AS counterparty_name,
       (SELECT COUNT(*) FROM approvals ap
         WHERE ap.interaction_id = i.id AND ap.status = 'pending') AS pending_approvals
FROM interactions i
JOIN agents pa ON pa.id = i.proposer_id
JOIN agents pb ON pb.id = i.counterparty_id";

#[derive(Debug, Serialize)]
pub struct Interaction {
    pub id: Uuid,
    pub kind: String,
    pub env: String,
    pub state: String,
    pub proposer_id: Uuid,
    pub proposer_name: String,
    pub counterparty_id: Uuid,
    pub counterparty_name: String,
    pub terms: ChallengeTerms,
    pub stake: Money,
    pub market_title: String,
    /// `gemini` or `fake`: where the proposer's market snapshot came from.
    pub market_source: Option<String>,
    /// The proposer's claim as a sentence fragment.
    pub claim: String,
    pub turn: Option<Uuid>,
    pub rounds: i32,
    pub max_rounds: i32,
    pub pending_approvals: i64,
    pub outcome: Option<String>,
    pub winner_id: Option<Uuid>,
    pub observed: Option<i64>,
    pub observed_display: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub status_reason: Option<String>,
    /// Always `backend_administered`: the backend reads the market and
    /// decides. This is not trustless.
    pub resolution: &'static str,
    /// Funding deadline.
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn to_interaction(joined: JoinedRow, max_rounds: i32) -> Result<Interaction, ApiError> {
    let JoinedRow {
        row,
        proposer_name,
        counterparty_name,
        pending_approvals,
    } = joined;
    let terms = row.terms()?;
    Ok(Interaction {
        id: row.id,
        kind: row.kind,
        env: row.env,
        state: row.state,
        proposer_id: row.proposer_id,
        proposer_name,
        counterparty_id: row.counterparty_id,
        counterparty_name,
        stake: terms.stake,
        claim: claim_text(&terms, &row.market_title),
        market_source: row
            .market_view
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_owned),
        market_title: row.market_title,
        turn: row.turn,
        rounds: row.rounds,
        max_rounds,
        pending_approvals,
        outcome: row.outcome,
        winner_id: row.winner_id,
        observed: row.observed,
        observed_display: row.observed.map(|v| format_value(terms.formula, v)),
        observed_at: row.observed_at,
        status_reason: row.status_reason,
        resolution: "backend_administered",
        start_at: row.start_ts,
        end_at: row.end_ts,
        created_at: row.created_at,
        updated_at: row.updated_at,
        terms,
    })
}

fn parse_uuid(value: Option<&str>, name: &str) -> Result<Option<Uuid>, ApiError> {
    match value.filter(|s| !s.is_empty()) {
        Some(s) => Uuid::parse_str(s)
            .map(Some)
            .map_err(|_| ApiError::bad_request(format!("{name} must be a UUID"))),
        None => Ok(None),
    }
}

#[derive(Deserialize)]
pub struct ListParams {
    agent_id: Option<String>,
    state: Option<String>,
    limit: Option<String>,
}

pub async fn list(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Vec<Interaction>>, ApiError> {
    let agent_id = parse_uuid(params.agent_id.as_deref(), "agent_id")?;
    let wanted = params.state.filter(|s| !s.is_empty());
    let limit = match params.limit.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => s
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| ApiError::bad_request("limit must be a positive integer"))?
            .min(MAX_LIMIT),
        None => DEFAULT_LIMIT,
    };
    let rows: Vec<JoinedRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{INTERACTIONS_SQL} \
         WHERE ($1::uuid IS NULL OR i.proposer_id = $1 OR i.counterparty_id = $1) \
           AND ($2::text IS NULL OR i.state = $2) \
         ORDER BY i.created_at DESC, i.id LIMIT $3"
    )))
    .bind(agent_id)
    .bind(wanted)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;
    let max = state.settings.max_negotiation_rounds;
    Ok(Json(
        rows.into_iter()
            .map(|r| to_interaction(r, max))
            .collect::<Result<_, _>>()?,
    ))
}

async fn load_interaction(state: &AppState, id: Uuid) -> Result<Interaction, ApiError> {
    let row: Option<JoinedRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{INTERACTIONS_SQL} WHERE i.id = $1"
    )))
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    to_interaction(
        row.ok_or_else(|| ApiError::not_found("interaction"))?,
        state.settings.max_negotiation_rounds,
    )
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct NegotiationRound {
    pub seq: i32,
    pub agent_id: Uuid,
    pub action: String,
    pub terms: Value,
    pub summary: String,
    pub ts: DateTime<Utc>,
}

/// One graph run: decision summaries per node and the intents it produced.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Run {
    pub id: Uuid,
    pub ts: DateTime<Utc>,
    pub agent_id: Uuid,
    pub agent_version: i32,
    pub trigger: String,
    pub status: String,
    pub error: Option<String>,
    pub reasoner: String,
    pub trace: Value,
    pub intents: Value,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Approval {
    pub id: Uuid,
    pub interaction_id: Uuid,
    pub agent_id: Uuid,
    pub agent_name: String,
    #[sqlx(try_from = "i64")]
    pub stake: StakeOut,
    pub status: String,
    pub reason: String,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    /// The funding deadline: a pending approval expires then.
    pub expires_at: DateTime<Utc>,
    pub market_title: String,
    pub env: String,
}

/// A stored stake, serialized like `Money`.
#[derive(Debug, Serialize)]
#[serde(transparent)]
pub struct StakeOut(Money);

impl TryFrom<i64> for StakeOut {
    type Error = &'static str;
    fn try_from(units: i64) -> Result<Self, Self::Error> {
        Money::from_minor(units)
            .map(StakeOut)
            .ok_or("negative stake")
    }
}

const APPROVALS_SQL: &str = "
SELECT ap.id, ap.interaction_id, ap.agent_id, a.name AS agent_name, ap.stake, ap.status,
       ap.reason, ap.requested_at, ap.decided_at, i.start_ts AS expires_at, i.market_title, i.env
FROM approvals ap
JOIN agents a ON a.id = ap.agent_id
JOIN interactions i ON i.id = ap.interaction_id";

#[derive(Debug, Serialize)]
pub struct InteractionDetail {
    #[serde(flatten)]
    pub interaction: Interaction,
    pub negotiation: Vec<NegotiationRound>,
    pub runs: Vec<Run>,
    pub approvals: Vec<Approval>,
    pub ledger: Vec<LedgerEntry>,
    pub activity: Vec<ActivityItem>,
}

pub async fn get(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<InteractionDetail>, ApiError> {
    let interaction = load_interaction(&state, id).await?;
    let pool = &state.pool;
    let negotiation = sqlx::query_as(
        "SELECT seq, agent_id, action, terms, summary, ts FROM negotiation_rounds \
         WHERE interaction_id = $1 ORDER BY seq",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let runs = sqlx::query_as(
        "SELECT id, ts, agent_id, agent_version, trigger, status, error, reasoner, trace, intents \
         FROM agent_runs WHERE interaction_id = $1 ORDER BY seq",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let approvals = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{APPROVALS_SQL} WHERE ap.interaction_id = $1 ORDER BY ap.requested_at, ap.id"
    )))
    .bind(id)
    .fetch_all(pool)
    .await?;
    let ledger = ledger_db::interaction_entries(pool, id).await?;
    let activity = sqlx::query_as(
        "SELECT id, ts, agent_id, interaction_id, kind, summary, detail FROM activity \
         WHERE interaction_id = $1 ORDER BY seq",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(Json(InteractionDetail {
        interaction,
        negotiation,
        runs,
        approvals,
        ledger,
        activity,
    }))
}

#[derive(Deserialize)]
pub struct ApprovalParams {
    status: Option<String>,
}

/// Pending approvals by default; `?status=all` or a specific status.
pub async fn approvals(
    State(state): State<AppState>,
    ApiQuery(params): ApiQuery<ApprovalParams>,
) -> Result<Json<Vec<Approval>>, ApiError> {
    let status = params.status.filter(|s| !s.is_empty());
    let status = match status.as_deref() {
        None => Some("pending"),
        Some("all") => None,
        Some(s @ ("pending" | "approved" | "rejected" | "expired" | "superseded")) => Some(s),
        Some(_) => return Err(ApiError::bad_request("unknown approval status")),
    };
    let rows = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{APPROVALS_SQL} WHERE ($1::text IS NULL OR ap.status = $1) \
         ORDER BY ap.requested_at DESC, ap.id LIMIT {MAX_LIMIT}"
    )))
    .bind(status)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

/// Body of the approve / reject calls: an empty JSON object. A body is
/// required so that the JSON content-type rule covers these endpoints too.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Debug, Serialize)]
pub struct ApprovalResult {
    pub approval: Approval,
    pub interaction: Interaction,
}

async fn decide(
    state: &AppState,
    id: Uuid,
    approve: bool,
) -> Result<Json<ApprovalResult>, ApiError> {
    let interaction_id = lifecycle::decide_approval(state, id, approve, Utc::now()).await?;
    let approval = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{APPROVALS_SQL} WHERE ap.id = $1"
    )))
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(ApprovalResult {
        approval,
        interaction: load_interaction(state, interaction_id).await?,
    }))
}

pub async fn approve(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(_): ApiJson<Empty>,
) -> Result<Json<ApprovalResult>, ApiError> {
    decide(&state, id, true).await
}

pub async fn reject(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(_): ApiJson<Empty>,
) -> Result<Json<ApprovalResult>, ApiError> {
    decide(&state, id, false).await
}

// ---------------------------------------------------------------------------
// Portfolio stats
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PortfolioTotals {
    pub unallocated: Money,
    pub available: Money,
    pub committed: Money,
    pub allocated: Money,
    pub realized_pnl: SignedMoney,
    /// Always null: a binary challenge has no fair mark before it resolves.
    pub unrealized_pnl: Option<SignedMoney>,
    pub at_risk: Money,
}

#[derive(Debug, Serialize)]
pub struct PnlPoint {
    pub ts: DateTime<Utc>,
    pub interaction_id: Option<Uuid>,
    pub agent_id: Uuid,
    pub delta: SignedMoney,
    pub agent_cumulative: SignedMoney,
    pub portfolio_cumulative: SignedMoney,
}

#[derive(Debug, Serialize)]
pub struct Allocation {
    pub agent_id: Uuid,
    pub name: String,
    pub available: Money,
    pub committed: Money,
}

#[derive(Debug, Serialize)]
pub struct AgentPerformance {
    pub agent_id: Uuid,
    pub name: String,
    pub realized_pnl: SignedMoney,
    pub settled: i64,
    pub wins: i64,
    pub losses: i64,
    /// Null until the agent has a settled challenge.
    pub win_rate_bps: Option<i64>,
}

#[derive(Debug, Default, Serialize)]
pub struct OutcomeCounts {
    pub settled: i64,
    pub voided: i64,
    pub cancelled: i64,
    pub failed: i64,
    pub open: i64,
    pub active: i64,
    pub by_state: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize)]
pub struct Portfolio {
    pub env: Env,
    pub as_of: DateTime<Utc>,
    pub totals: PortfolioTotals,
    pub pnl_over_time: Vec<PnlPoint>,
    pub allocation: Vec<Allocation>,
    pub agents: Vec<AgentPerformance>,
    pub outcomes: OutcomeCounts,
    pub formulas: BTreeMap<&'static str, &'static str>,
}

/// Every metric's definition, returned with the numbers.
const FORMULAS: [(&str, &str); 11] = [
    ("unallocated", "ledger balance of user:unallocated"),
    (
        "available",
        "sum of ledger balances of agent:{id}:available",
    ),
    (
        "committed",
        "sum of ledger balances of agent:{id}:committed",
    ),
    ("allocated", "available + committed"),
    (
        "realized_pnl",
        "sum over settled challenges of (payout - stake) = net of the agent's `settle` ledger \
         entries; deposits, allocations, deallocations, stake commits and refunds are excluded",
    ),
    (
        "unrealized_pnl",
        "not computed: a binary challenge has no fair mark before resolution; see at_risk",
    ),
    (
        "at_risk",
        "committed (stakes locked in funded, active or resolving challenges)",
    ),
    (
        "pnl_over_time",
        "one point per agent per settled challenge, ordered by settlement time; ts = the \
         lifecycle clock at settlement (the simulated `now` of a dev tick, real time otherwise); \
         cumulative sums of delta",
    ),
    (
        "win_rate_bps",
        "wins * 10000 / settled; voided and cancelled challenges are excluded",
    ),
    (
        "outcomes",
        "settled = state settled; voided = cancelled with outcome void (stakes refunded); \
         cancelled = cancelled before funding; open = any non-terminal state; active = funded, \
         active or resolving",
    ),
    (
        "portfolio_note",
        "with one owner on both sides of a challenge, portfolio realized P&L nets to zero",
    ),
];

pub async fn portfolio(State(pool): State<PgPool>) -> Result<Json<Portfolio>, ApiError> {
    let env = Env::Simulated;
    let agent_rows = agents::load(&pool, None).await?;
    let unallocated: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries \
         WHERE env = $1 AND account = 'user:unallocated'",
    )
    .bind(env.as_str())
    .fetch_one(&pool)
    .await?;
    let overflow = || ApiError::internal("portfolio total overflow");

    let mut available = Money::ZERO;
    let mut committed = Money::ZERO;
    let mut realized = SignedMoney::ZERO;
    let mut allocation = Vec::new();
    let mut performance = Vec::new();
    for a in agent_rows.iter().filter(|a| a.env == env) {
        available = available
            .checked_add(a.balances.available)
            .ok_or_else(overflow)?;
        committed = committed
            .checked_add(a.balances.committed)
            .ok_or_else(overflow)?;
        realized = realized
            .checked_add(a.stats.realized_pnl)
            .ok_or_else(overflow)?;
        allocation.push(Allocation {
            agent_id: a.id,
            name: a.name.clone(),
            available: a.balances.available,
            committed: a.balances.committed,
        });
        performance.push(AgentPerformance {
            agent_id: a.id,
            name: a.name.clone(),
            realized_pnl: a.stats.realized_pnl,
            settled: a.stats.settled,
            wins: a.stats.wins,
            losses: a.stats.settled - a.stats.wins,
            win_rate_bps: (a.stats.settled > 0).then(|| a.stats.wins * 10_000 / a.stats.settled),
        });
    }

    let settlements: Vec<(DateTime<Utc>, Option<Uuid>, Uuid, i64)> = sqlx::query_as(
        "SELECT COALESCE(i.settled_at, MIN(e.ts)) AS settled_at, e.interaction_id, e.agent_id, \
                SUM(e.delta)::BIGINT \
         FROM ledger_entries e LEFT JOIN interactions i ON i.id = e.interaction_id \
         WHERE e.env = $1 AND e.reason = 'settle' AND e.agent_id IS NOT NULL \
         GROUP BY e.idempotency_key, e.interaction_id, e.agent_id, i.settled_at \
         ORDER BY 1, MIN(e.seq), e.agent_id",
    )
    .bind(env.as_str())
    .fetch_all(&pool)
    .await?;
    let mut per_agent: BTreeMap<Uuid, SignedMoney> = BTreeMap::new();
    let mut running = SignedMoney::ZERO;
    let mut pnl_over_time = Vec::with_capacity(settlements.len());
    for (ts, interaction_id, agent_id, delta) in settlements {
        let delta = SignedMoney::from_minor(delta);
        let agent_total = per_agent.entry(agent_id).or_default();
        *agent_total = agent_total.checked_add(delta).ok_or_else(overflow)?;
        running = running.checked_add(delta).ok_or_else(overflow)?;
        pnl_over_time.push(PnlPoint {
            ts,
            interaction_id,
            agent_id,
            delta,
            agent_cumulative: *agent_total,
            portfolio_cumulative: running,
        });
    }

    let states: Vec<(String, Option<String>, i64)> = sqlx::query_as(
        "SELECT state, outcome, COUNT(*) FROM interactions WHERE env = $1 GROUP BY state, outcome",
    )
    .bind(env.as_str())
    .fetch_all(&pool)
    .await?;
    let mut outcomes = OutcomeCounts::default();
    for (state, outcome, count) in states {
        *outcomes.by_state.entry(state.clone()).or_default() += count;
        match (state.as_str(), outcome.as_deref()) {
            ("settled", _) => outcomes.settled += count,
            ("cancelled", Some("void")) => outcomes.voided += count,
            ("cancelled", _) => outcomes.cancelled += count,
            ("failed" | "disputed", _) => outcomes.failed += count,
            (open, _) => {
                outcomes.open += count;
                if matches!(open, "funded" | "active" | "resolving") {
                    outcomes.active += count;
                }
            }
        }
    }

    Ok(Json(Portfolio {
        env,
        as_of: Utc::now(),
        totals: PortfolioTotals {
            unallocated: Money::from_minor(unallocated)
                .ok_or_else(|| ApiError::internal("negative wallet balance"))?,
            available,
            committed,
            allocated: available.checked_add(committed).ok_or_else(overflow)?,
            realized_pnl: realized,
            unrealized_pnl: None,
            at_risk: committed,
        },
        pnl_over_time,
        allocation,
        agents: performance,
        outcomes,
        formulas: FORMULAS.into_iter().collect(),
    }))
}

// ---------------------------------------------------------------------------
// Dev-only controls
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TickBody {
    /// Simulated clock, accepted only with the fake market: real market data
    /// must never be combined with a made-up time.
    pub now: Option<DateTime<Utc>>,
    /// Lets a simulated `now` pass the funding deadline of challenges that
    /// are still being negotiated or approved (they expire, as they would in
    /// real time). Without it such a jump is refused.
    #[serde(default)]
    pub allow_expiry: bool,
}

/// Runs one lifecycle tick now. Demo / test trigger; the interval scheduler
/// calls the same function.
pub async fn dev_tick(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<TickBody>,
) -> Result<Json<TickReport>, ApiError> {
    let now = match body.now {
        Some(_) if state.fake_market.is_none() => {
            return Err(ApiError::validation(vec![Detail::new(
                "now",
                "simulated_clock_not_allowed",
                "`now` can only be set when the server runs on the fake market",
            )]));
        }
        Some(now) => {
            // A guard on the dev clock only, not a change to the deadline
            // rule: real ticks expire unfunded challenges exactly as before.
            let (at_risk, earliest): (i64, Option<DateTime<Utc>>) = sqlx::query_as(
                "SELECT COUNT(*), MIN(start_ts) FROM interactions WHERE state IN ('proposed', \
                 'negotiating', 'accepted', 'awaiting_approval') AND start_ts <= $1",
            )
            .bind(now)
            .fetch_one(&state.pool)
            .await?;
            if at_risk > 0 && !body.allow_expiry {
                let deadline = earliest.map(|t| t.to_rfc3339()).unwrap_or_default();
                return Err(ApiError::conflict(
                    "clock_jump_would_expire",
                    format!(
                        "{at_risk} challenge(s) are not funded yet and their funding deadline \
                         (earliest {deadline}) is before this simulated time, so they would \
                         expire. Tick without `now` (and decide pending approvals) until they \
                         are active, or send `allow_expiry: true`."
                    ),
                ));
            }
            now
        }
        None => Utc::now(),
    };
    Ok(Json(lifecycle::tick(&state, now).await?))
}

/// Changes what the fake market returns (404 when real market data is used).
pub async fn dev_fake_market(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<FakeMarketUpdate>,
) -> Result<Json<Value>, ApiError> {
    let fake = state
        .fake_market
        .as_ref()
        .ok_or_else(|| ApiError::not_found("fake market"))?;
    fake.update(&body).map_err(|field| {
        ApiError::validation(vec![Detail::new(field, "out_of_range", "invalid value")])
    })?;
    Ok(Json(
        serde_json::json!({ "status": "ok", "source": "fake" }),
    ))
}
