//! Co-captain suggestions: reading the facts, storing drafts, and the
//! owner's accept / dismiss.
//!
//! Nothing here touches the ledger or `authorize`. The only write outside
//! the `suggestions` table is `accept`, which applies a `ProposedChange`
//! through `agents::apply_update`, the same validated and versioned edit the
//! owner's `PUT /agents/{id}` uses.

use std::collections::{HashMap, HashSet};

use aen_core::authorize::DenyReason;
use aen_core::money::{Money, SignedMoney};
use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agents::{self, record_activity};
use crate::cocaptain::{
    APPROVAL_STREAK, Advice, AgentFacts, DecidedApproval, DenialCount, Draft, Facts, Kind,
    LOSING_STREAK, PendingApproval, ProposedChange, Source, rules,
};
use crate::error::{ApiError, Detail};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::interactions::Empty;
use crate::models::AgentConfig;
use crate::state::AppState;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Suggestion {
    pub id: Uuid,
    pub agent_id: Option<Uuid>,
    pub agent_name: Option<String>,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub proposed_change: Option<Value>,
    pub evidence: Value,
    pub loosens_control: bool,
    pub source: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
}

const SUGGESTIONS_SQL: &str = "
SELECT s.id, s.agent_id, a.name AS agent_name, s.kind, s.title, s.body, s.proposed_change,
       s.evidence, s.loosens_control, s.source, s.status, s.created_at, s.decided_at
FROM suggestions s
LEFT JOIN agents a ON a.id = s.agent_id";

const STATUSES: [&str; 4] = ["pending", "accepted", "dismissed", "expired"];

async fn load_one(pool: &PgPool, id: Uuid) -> Result<Suggestion, ApiError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SUGGESTIONS_SQL} WHERE s.id = $1"
    )))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::not_found("suggestion"))
}

fn money(units: i64, what: &str) -> Result<Money, ApiError> {
    Money::from_minor(units).ok_or_else(|| ApiError::internal(format!("negative {what}")))
}

/// Reads everything the co-captain may know, straight from the stored
/// records. Read-only.
pub async fn gather(pool: &PgPool, now: DateTime<Utc>) -> Result<Facts, ApiError> {
    let unallocated: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries \
         WHERE env = 'simulated' AND account = 'user:unallocated'",
    )
    .fetch_one(pool)
    .await?;

    let allocated: HashMap<Uuid, i64> = sqlx::query_as::<_, (Uuid, i64)>(
        "SELECT e.agent_id, SUM(e.delta)::BIGINT FROM ledger_entries e \
         JOIN agents a ON a.id = e.agent_id AND a.env = e.env \
         WHERE e.reason IN ('allocate', 'deallocate') \
           AND e.account = 'agent:' || a.id || ':available' \
         GROUP BY e.agent_id",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();

    // One row per settlement and agent, newest first.
    let settles: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT x.agent_id, x.net FROM ( \
             SELECT e.agent_id, SUM(e.delta)::BIGINT AS net, MAX(e.seq) AS last_seq, \
                    ROW_NUMBER() OVER (PARTITION BY e.agent_id ORDER BY MAX(e.seq) DESC) AS rn \
             FROM ledger_entries e JOIN agents a ON a.id = e.agent_id AND a.env = e.env \
             WHERE e.reason = 'settle' GROUP BY e.agent_id, e.idempotency_key) x \
         WHERE x.rn <= $1 ORDER BY x.agent_id, x.rn",
    )
    .bind(LOSING_STREAK as i64)
    .fetch_all(pool)
    .await?;

    let pending: Vec<(Uuid, Uuid, i64, DateTime<Utc>, String)> = sqlx::query_as(
        "SELECT ap.agent_id, ap.id, ap.stake, i.start_ts, i.market_title FROM approvals ap \
         JOIN interactions i ON i.id = ap.interaction_id \
         WHERE ap.status = 'pending' AND i.state = 'awaiting_approval' \
         ORDER BY i.start_ts, ap.id",
    )
    .fetch_all(pool)
    .await?;

    let decided: Vec<(Uuid, Uuid, String, i64)> = sqlx::query_as(
        "SELECT ap.agent_id, ap.id, ap.status, ap.stake FROM approvals ap \
         JOIN interactions i ON i.id = ap.interaction_id \
         JOIN agents a ON a.id = ap.agent_id \
         WHERE ap.status IN ('approved', 'rejected') \
           AND a.current_version = CASE WHEN i.proposer_id = a.id THEN i.proposer_version \
                                        ELSE i.counterparty_version END \
         ORDER BY ap.decided_at DESC, ap.id",
    )
    .fetch_all(pool)
    .await?;

    let denials: Vec<(Uuid, String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT act.agent_id, r.reason, COUNT(*)::BIGINT, \
                MAX(COALESCE(act.detail->>'stake', act.detail->'terms'->>'stake')::BIGINT) \
         FROM activity act \
         JOIN agents a ON a.id = act.agent_id \
         JOIN agent_versions v ON v.agent_id = a.id AND v.version = a.current_version \
         CROSS JOIN LATERAL jsonb_array_elements_text(act.detail->'reasons') AS r(reason) \
         WHERE act.kind IN ('proposal_denied', 'response_denied', 'commit_denied') \
           AND act.ts >= v.created_at \
         GROUP BY act.agent_id, r.reason ORDER BY act.agent_id, r.reason",
    )
    .fetch_all(pool)
    .await?;

    let interactions: HashMap<Uuid, (i64, Option<DateTime<Utc>>)> =
        sqlx::query_as::<_, (Uuid, i64, Option<DateTime<Utc>>)>(
            "SELECT a.id, \
                    COUNT(*) FILTER (WHERE i.state NOT IN ('settled', 'cancelled', 'disputed', \
                                                           'failed'))::BIGINT, \
                    MAX(i.updated_at) \
             FROM agents a \
             JOIN interactions i ON i.proposer_id = a.id OR i.counterparty_id = a.id \
             GROUP BY a.id",
        )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, open, last)| (id, (open, last)))
        .collect();

    let mut out = Vec::new();
    for agent in agents::load(pool, None).await? {
        let id = agent.id;
        let bad =
            |what: &str| ApiError::internal(format!("agent {id}: stored {what} is unreadable"));
        let config: AgentConfig =
            serde_json::from_value(agent.config).map_err(|_| bad("config"))?;
        let (open_interactions, last_interaction_at) =
            interactions.get(&id).copied().unwrap_or((0, None));
        out.push(AgentFacts {
            id,
            name: agent.name,
            status: agent.status,
            version: agent.version,
            updated_at: agent.updated_at,
            risk_tolerance: config.risk_tolerance,
            policy: serde_json::from_value(agent.policy).map_err(|_| bad("policy"))?,
            available: agent.balances.available,
            committed: agent.balances.committed,
            realized_pnl: agent.stats.realized_pnl,
            settled: agent.stats.settled,
            wins: agent.stats.wins,
            net_allocated: SignedMoney::from_minor(allocated.get(&id).copied().unwrap_or(0)),
            recent_settle_nets: settles
                .iter()
                .filter(|(a, _)| *a == id)
                .map(|(_, net)| SignedMoney::from_minor(*net))
                .collect(),
            pending_approvals: pending
                .iter()
                .filter(|p| p.0 == id)
                .map(|(_, approval, stake, expires_at, title)| {
                    Ok(PendingApproval {
                        id: *approval,
                        stake: money(*stake, "stake")?,
                        expires_at: *expires_at,
                        market_title: title.clone(),
                    })
                })
                .collect::<Result<_, ApiError>>()?,
            recent_approvals: decided
                .iter()
                .filter(|d| d.0 == id)
                .take(APPROVAL_STREAK)
                .map(|(_, approval, status, stake)| {
                    Ok(DecidedApproval {
                        id: *approval,
                        approved: status == "approved",
                        stake: money(*stake, "stake")?,
                    })
                })
                .collect::<Result<_, ApiError>>()?,
            denials: denials
                .iter()
                .filter(|d| d.0 == id)
                .filter_map(|(_, reason, count, stake)| {
                    // A reason this build does not know is ignored.
                    let reason: DenyReason =
                        serde_json::from_value(Value::String(reason.clone())).ok()?;
                    Some(DenialCount {
                        reason,
                        count: *count,
                        max_stake: stake.and_then(Money::from_minor),
                    })
                })
                .collect(),
            open_interactions,
            last_interaction_at,
        });
    }
    Ok(Facts {
        now,
        unallocated: money(unallocated, "wallet balance")?,
        agents: out,
    })
}

/// Stores a draft unless it is a duplicate: the agent already has a pending
/// suggestion of this kind, or this exact subject was suggested before
/// (whatever became of it). Returns the new id.
async fn insert(pool: &PgPool, draft: &Draft) -> Result<Option<Uuid>, ApiError> {
    let change = match &draft.proposed_change {
        Some(change) => Some(serde_json::to_value(change).map_err(ApiError::internal)?),
        None => None,
    };
    Ok(sqlx::query_scalar(
        "INSERT INTO suggestions (agent_id, kind, subject, title, body, proposed_change, \
         evidence, loosens_control, source) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind(draft.agent_id)
    .bind(draft.kind.as_str())
    .bind(&draft.subject)
    .bind(&draft.title)
    .bind(&draft.body)
    .bind(change)
    .bind(&draft.evidence)
    .bind(draft.loosens_control)
    .bind(draft.source.as_str())
    .fetch_optional(pool)
    .await?)
}

/// Marks pending rule-kind suggestions `expired` when their rule no longer
/// fires for that subject (the approval was decided, the agent was resumed
/// ...). The row is kept; `general` suggestions are left to the owner.
async fn expire_stale(pool: &PgPool, current: &[Draft]) -> Result<(), ApiError> {
    let live: HashSet<(Option<Uuid>, &str, &str)> = current
        .iter()
        .map(|d| (d.agent_id, d.kind.as_str(), d.subject.as_str()))
        .collect();
    let pending: Vec<(Uuid, Option<Uuid>, String, String)> = sqlx::query_as(
        "SELECT id, agent_id, kind, subject FROM suggestions \
         WHERE status = 'pending' AND kind <> $1",
    )
    .bind(Kind::General.as_str())
    .fetch_all(pool)
    .await?;
    for (id, agent_id, kind, subject) in pending {
        if !live.contains(&(agent_id, kind.as_str(), subject.as_str())) {
            sqlx::query(
                "UPDATE suggestions SET status = 'expired', decided_at = now() \
                 WHERE id = $1 AND status = 'pending'",
            )
            .bind(id)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

pub struct Generated {
    pub source: Source,
    pub created: Vec<Uuid>,
}

/// Reads the facts, retires suggestions that no longer apply and stores new
/// ones. With `ask_model` the configured co-captain (Grok when a key is set)
/// writes the advice; without it only the rules run, which is what the
/// scheduler does on every tick.
pub async fn generate(
    state: &AppState,
    now: DateTime<Utc>,
    ask_model: bool,
) -> Result<Generated, ApiError> {
    let facts = gather(&state.pool, now).await?;
    let by_rule = rules(&facts);
    expire_stale(&state.pool, &by_rule).await?;
    let advice = if ask_model {
        state.cocaptain.advise(&facts).await
    } else {
        Advice {
            source: Source::Rules,
            drafts: by_rule,
        }
    };
    let mut created = Vec::new();
    for draft in advice.drafts {
        // Whatever produced the draft, it is checked against the facts here.
        match draft.vet(&facts) {
            Ok(draft) => created.extend(insert(&state.pool, &draft).await?),
            Err(error) => tracing::warn!(%error, "dropped an invalid suggestion"),
        }
    }
    Ok(Generated {
        source: advice.source,
        created,
    })
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    pub status: Option<String>,
    pub agent_id: Option<String>,
}

pub async fn list(
    State(pool): State<PgPool>,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Vec<Suggestion>>, ApiError> {
    let status = params.status.filter(|s| !s.is_empty());
    if let Some(s) = &status
        && !STATUSES.contains(&s.as_str())
    {
        return Err(ApiError::validation(vec![Detail::new(
            "status",
            "invalid_value",
            "status must be pending, accepted, dismissed or expired",
        )]));
    }
    let agent_id = match params.agent_id.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => {
            Some(Uuid::parse_str(s).map_err(|_| ApiError::bad_request("agent_id must be a UUID"))?)
        }
        None => None,
    };
    let rows: Vec<Suggestion> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SUGGESTIONS_SQL} WHERE ($1::text IS NULL OR s.status = $1) \
         AND ($2::uuid IS NULL OR s.agent_id = $2) ORDER BY s.seq DESC LIMIT 200"
    )))
    .bind(status)
    .bind(agent_id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Serialize)]
pub struct GenerateResult {
    /// `grok` when the model answered, `rules` otherwise (no key, or a
    /// fallback). Each suggestion carries its own `source`.
    pub source: Source,
    pub created: Vec<Suggestion>,
}

pub async fn generate_now(
    State(state): State<AppState>,
    ApiJson(_): ApiJson<Empty>,
) -> Result<Json<GenerateResult>, ApiError> {
    let generated = generate(&state, Utc::now(), true).await?;
    let mut created = Vec::new();
    for id in generated.created {
        created.push(load_one(&state.pool, id).await?);
    }
    Ok(Json(GenerateResult {
        source: generated.source,
        created,
    }))
}

/// The owner's decision. Deciding the same way twice is a no-op; deciding a
/// suggestion that already has a different outcome is a 409.
async fn decide(pool: &PgPool, id: Uuid, accept: bool) -> Result<Json<Suggestion>, ApiError> {
    let (wanted, kind) = if accept {
        ("accepted", "suggestion_accepted")
    } else {
        ("dismissed", "suggestion_dismissed")
    };
    let mut tx = pool.begin().await?;
    let row: Option<(Option<Uuid>, String, Option<Value>, String)> = sqlx::query_as(
        "SELECT agent_id, status, proposed_change, title FROM suggestions \
         WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let (agent_id, status, change, title) = row.ok_or_else(|| ApiError::not_found("suggestion"))?;
    if status == wanted {
        drop(tx);
        return Ok(Json(load_one(pool, id).await?));
    }
    if status != "pending" {
        return Err(ApiError::conflict(
            "suggestion_not_pending",
            format!("this suggestion is already {status}"),
        ));
    }

    // Accepting is the owner editing the agent: the stored change is parsed
    // again into the typed patch, merged into the *current* version and sent
    // through the normal validation. Any failure leaves everything as it was.
    let mut version = None;
    if let (true, Some(change)) = (accept, change) {
        let invalid = |message: String| ApiError::unprocessable("invalid_proposed_change", message);
        let change: ProposedChange = serde_json::from_value(change)
            .map_err(|e| invalid(format!("the proposed change is not allowed: {e}")))?;
        let agent_id =
            agent_id.ok_or_else(|| invalid("the proposed change names no agent".into()))?;
        let mut input = agents::current_input(&mut tx, agent_id)
            .await?
            .ok_or_else(|| ApiError::not_found("agent"))?;
        let before = input.clone();
        change.apply(&mut input);
        if input == before {
            return Err(invalid(
                "the agent already has these values; dismiss the suggestion instead".into(),
            ));
        }
        version = Some(agents::apply_update(&mut tx, agent_id, input).await?);
    }

    sqlx::query("UPDATE suggestions SET status = $2, decided_at = now() WHERE id = $1")
        .bind(id)
        .bind(wanted)
        .execute(&mut *tx)
        .await?;
    record_activity(
        &mut tx,
        agent_id,
        kind,
        format!("You {wanted} the co-captain's suggestion \"{title}\""),
        Some(json!({ "suggestion_id": id, "version": version })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(load_one(pool, id).await?))
}

pub async fn accept(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(_): ApiJson<Empty>,
) -> Result<Json<Suggestion>, ApiError> {
    decide(&pool, id, true).await
}

pub async fn dismiss(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(_): ApiJson<Empty>,
) -> Result<Json<Suggestion>, ApiError> {
    decide(&pool, id, false).await
}
