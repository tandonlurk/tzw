//! Two-agent collaboration agreements (MVP, simulated funds only).
//!
//! The owner drives them over REST on behalf of the agents: propose, accept
//! or reject, fund, complete or cancel. Every agent decision and every
//! commitment of funds goes through `core::authorize` with a fresh snapshot
//! read under the same per-agent locks the challenge lifecycle uses. Payouts
//! and refunds release what was committed, by the funded terms, like a
//! challenge settlement. Money moves only as balanced ledger posting sets. Completion is
//! declared by the owner: backend-administered, not trustless.

use aen_core::authorize::{Decision, DenyReason, Intent, authorize};
use aen_core::collaboration::{BPS, CollabState, CollabTerms};
use aen_core::ledger::{self, Env};
use aen_core::money::{Money, SignedMoney};
use aen_core::policy::InteractionType;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::error::{ApiError, Detail};
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::ledger_db;
use crate::lifecycle::{AgentRec, load_agent};
use crate::models::{ActivityItem, LedgerEntry, display_bps, display_usdc};

/// How completion is decided, returned with every collaboration.
pub const RESOLUTION: &str = "owner_declared";

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBody {
    pub proposer_id: Uuid,
    pub partner_id: Uuid,
    pub objective: String,
    pub proposer_share_bps: u32,
    pub proposer_contribution: Money,
    pub partner_contribution: Money,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FundBody {
    /// The owner's approval for a contribution above an agent's
    /// `approval_threshold`. Limits are still enforced either way.
    #[serde(default)]
    pub owner_approved: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelBody {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyBody {}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    agent_id: Option<String>,
    state: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Party {
    pub agent_id: Uuid,
    pub name: String,
    pub contribution: Money,
    /// Share of the pool paid to this party on completion.
    pub share_bps: u32,
    /// What completion would pay this party under the agreed split.
    pub expected_payout: Money,
    /// Set once completed.
    pub payout: Option<Money>,
    /// `payout - contribution`, once completed.
    pub net: Option<SignedMoney>,
}

#[derive(Debug, Serialize)]
pub struct Collaboration {
    pub id: Uuid,
    pub env: String,
    pub state: CollabState,
    pub objective: String,
    pub proposer: Party,
    pub partner: Party,
    /// Sum of both contributions.
    pub pool: Money,
    pub resolution: &'static str,
    pub status_reason: Option<String>,
    /// What the owner may do next, e.g. `["accept", "reject", "cancel"]`.
    pub actions: Vec<&'static str>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub funded_at: Option<DateTime<Utc>>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct CollaborationDetail {
    pub collaboration: Collaboration,
    pub ledger: Vec<LedgerEntry>,
    pub activity: Vec<ActivityItem>,
}

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: Uuid,
    env: String,
    state: String,
    proposer_id: Uuid,
    partner_id: Uuid,
    objective: String,
    proposer_share_bps: i32,
    proposer_contribution: i64,
    partner_contribution: i64,
    proposer_payout: Option<i64>,
    partner_payout: Option<i64>,
    status_reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    funded_at: Option<DateTime<Utc>>,
    closed_at: Option<DateTime<Utc>>,
    proposer_name: String,
    partner_name: String,
}

const SELECT_SQL: &str = "
SELECT c.id, c.env, c.state, c.proposer_id, c.partner_id, c.objective, c.proposer_share_bps,
       c.proposer_contribution, c.partner_contribution, c.proposer_payout, c.partner_payout,
       c.status_reason, c.created_at, c.updated_at, c.funded_at, c.closed_at,
       p.name AS proposer_name, q.name AS partner_name
FROM collaborations c
JOIN agents p ON p.id = c.proposer_id
JOIN agents q ON q.id = c.partner_id";

fn money(units: i64) -> Result<Money, ApiError> {
    Money::from_minor(units).ok_or_else(|| ApiError::internal("negative amount stored"))
}

impl Row {
    fn state(&self) -> Result<CollabState, ApiError> {
        CollabState::parse(&self.state).ok_or_else(|| ApiError::internal("unknown state"))
    }

    fn terms(&self) -> Result<CollabTerms, ApiError> {
        Ok(CollabTerms {
            objective: self.objective.clone(),
            proposer_share_bps: u32::try_from(self.proposer_share_bps)
                .map_err(|_| ApiError::internal("bad share"))?,
            proposer_contribution: money(self.proposer_contribution)?,
            partner_contribution: money(self.partner_contribution)?,
        })
    }

    fn env(&self) -> Result<Env, ApiError> {
        self.env
            .parse()
            .map_err(|()| ApiError::internal("unknown env"))
    }

    fn view(self) -> Result<Collaboration, ApiError> {
        let state = self.state()?;
        let terms = self.terms()?;
        let pool = terms
            .pool()
            .ok_or_else(|| ApiError::internal("pool overflow"))?;
        let (exp_a, exp_b) = terms
            .payouts()
            .ok_or_else(|| ApiError::internal("bad split"))?;
        let party =
            |agent_id, name, contribution: Money, share_bps, expected, paid: Option<i64>| {
                let payout = paid.map(money).transpose()?;
                Ok::<_, ApiError>(Party {
                    agent_id,
                    name,
                    contribution,
                    share_bps,
                    expected_payout: expected,
                    payout,
                    net: payout.map(|p| net_of(p, contribution)),
                })
            };
        Ok(Collaboration {
            id: self.id,
            env: self.env,
            state,
            proposer: party(
                self.proposer_id,
                self.proposer_name,
                terms.proposer_contribution,
                terms.proposer_share_bps,
                exp_a,
                self.proposer_payout,
            )?,
            partner: party(
                self.partner_id,
                self.partner_name,
                terms.partner_contribution,
                BPS - terms.proposer_share_bps,
                exp_b,
                self.partner_payout,
            )?,
            objective: self.objective,
            pool,
            resolution: RESOLUTION,
            status_reason: self.status_reason,
            actions: actions(state),
            created_at: self.created_at,
            updated_at: self.updated_at,
            funded_at: self.funded_at,
            closed_at: self.closed_at,
        })
    }
}

/// The owner's next possible steps in `state`.
pub fn actions(state: CollabState) -> Vec<&'static str> {
    match state {
        CollabState::Proposed => vec!["accept", "reject", "cancel"],
        CollabState::Accepted => vec!["fund", "cancel"],
        CollabState::Funded => vec!["complete", "cancel"],
        CollabState::Rejected | CollabState::Completed | CollabState::Cancelled => vec![],
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn load_row(conn: &mut PgConnection, id: Uuid, for_update: bool) -> Result<Row, ApiError> {
    if for_update {
        // Lock the collaboration row first (as the lifecycle locks an
        // interaction), then the agents.
        sqlx::query("SELECT 1 FROM collaborations WHERE id = $1 FOR UPDATE")
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT_SQL} WHERE c.id = $1")))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ApiError::not_found("collaboration"))
}

/// The same per-agent advisory locks as the challenge lifecycle, in the same
/// (sorted) order, so collaboration and challenge money moves serialize.
async fn lock_agents(conn: &mut PgConnection, agents: [Uuid; 2]) -> Result<(), ApiError> {
    let mut agents = agents;
    agents.sort();
    for agent in agents {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!("aen:agent:{agent}"))
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

async fn require_agent(conn: &mut PgConnection, id: Uuid) -> Result<AgentRec, ApiError> {
    load_agent(conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("agent"))
}

async fn record(
    conn: &mut PgConnection,
    agent_id: Uuid,
    collaboration_id: Option<Uuid>,
    kind: &str,
    summary: String,
    detail: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO activity (agent_id, collaboration_id, kind, summary, detail) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(agent_id)
    .bind(collaboration_id)
    .bind(kind)
    .bind(summary)
    .bind(detail)
    .execute(conn)
    .await?;
    Ok(())
}

/// `payout - contribution`; both are non-negative `i64`, so this cannot
/// overflow.
fn net_of(payout: Money, contribution: Money) -> SignedMoney {
    SignedMoney::from_minor(payout.minor() - contribution.minor())
}

fn amount(m: Money) -> String {
    format!("{} (simulated)", display_usdc(m))
}

fn signed(m: SignedMoney) -> String {
    let sign = if m.is_negative() { "-" } else { "+" };
    let abs = Money::from_minor(m.minor().saturating_abs()).unwrap_or(Money::ZERO);
    format!("{sign}{}", amount(abs))
}

fn deny_phrase(reason: DenyReason) -> &'static str {
    match reason {
        DenyReason::AgentPaused => "the agent is paused",
        DenyReason::StopLossTriggered => "its stop-loss has been reached",
        DenyReason::InteractionTypeNotAllowed => "collaborations are not allowed by its policy",
        DenyReason::IllegalInteractionState => "the collaboration is not in a state that allows it",
        DenyReason::ZeroAmount => "the contribution is zero",
        DenyReason::AmountOverflow => "the amount is too large",
        DenyReason::ExceedsMaxPerInteraction => {
            "the contribution is above its per-interaction limit"
        }
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

/// A refused step: written to activity (outside the rolled-back
/// transaction) and returned as 422 `authorization_denied`.
struct Denial {
    agent: Uuid,
    role: &'static str,
    name: String,
    reasons: Vec<DenyReason>,
}

async fn deny(
    pool: &PgPool,
    collaboration: Option<Uuid>,
    step: &str,
    denials: Vec<Denial>,
) -> ApiError {
    let mut details = Vec::new();
    for d in &denials {
        let text = deny_text(&d.reasons);
        let summary = format!("\"{}\" could not {step} the collaboration: {text}.", d.name);
        if let Ok(mut conn) = pool.acquire().await {
            // Best effort: the refusal is returned to the caller either way.
            let _ = record(
                &mut conn,
                d.agent,
                collaboration,
                "collaboration_denied",
                summary,
                json!({ "step": step, "role": d.role, "reasons": d.reasons }),
            )
            .await;
        }
        for r in &d.reasons {
            details.push(Detail::new(
                d.role,
                serde_json::to_value(r)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_owned))
                    .unwrap_or_default(),
                format!("\"{}\": {}", d.name, deny_phrase(*r)),
            ));
        }
    }
    let mut err = ApiError::unprocessable(
        "authorization_denied",
        format!("the permission engine refused to {step} this collaboration"),
    );
    err.details = details;
    err
}

fn invalid_state(state: CollabState, step: &str) -> ApiError {
    ApiError::conflict(
        "invalid_state",
        format!(
            "the collaboration is {}; it cannot be {step}",
            state.as_str()
        ),
    )
}

async fn respond(pool: &PgPool, id: Uuid) -> Result<Json<Collaboration>, ApiError> {
    let mut conn = pool.acquire().await?;
    Ok(Json(load_row(&mut conn, id, false).await?.view()?))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn list(
    State(pool): State<PgPool>,
    ApiQuery(params): ApiQuery<ListParams>,
) -> Result<Json<Vec<Collaboration>>, ApiError> {
    let agent_id = match params.agent_id.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => {
            Some(Uuid::parse_str(s).map_err(|_| ApiError::bad_request("agent_id must be a UUID"))?)
        }
        None => None,
    };
    let state = match params.state.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => Some(
            CollabState::parse(s)
                .ok_or_else(|| ApiError::bad_request("unknown collaboration state"))?
                .as_str(),
        ),
        None => None,
    };
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_SQL} WHERE ($1::uuid IS NULL OR c.proposer_id = $1 OR c.partner_id = $1) \
         AND ($2::text IS NULL OR c.state = $2) ORDER BY c.seq DESC LIMIT 200"
    )))
    .bind(agent_id)
    .bind(state)
    .fetch_all(&pool)
    .await?;
    Ok(Json(
        rows.into_iter().map(Row::view).collect::<Result<_, _>>()?,
    ))
}

pub async fn get(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<CollaborationDetail>, ApiError> {
    let mut conn = pool.acquire().await?;
    let collaboration = load_row(&mut conn, id, false).await?.view()?;
    let ledger = ledger_db::collaboration_entries(&mut *conn, id).await?;
    let activity = sqlx::query_as(
        "SELECT id, ts, agent_id, interaction_id, collaboration_id, kind, summary, detail \
         FROM activity WHERE collaboration_id = $1 ORDER BY seq",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(CollaborationDetail {
        collaboration,
        ledger,
        activity,
    }))
}

pub async fn create(
    State(pool): State<PgPool>,
    ApiJson(body): ApiJson<CreateBody>,
) -> Result<(StatusCode, Json<Collaboration>), ApiError> {
    let terms = CollabTerms {
        objective: body.objective.trim().to_owned(),
        proposer_share_bps: body.proposer_share_bps,
        proposer_contribution: body.proposer_contribution,
        partner_contribution: body.partner_contribution,
    };
    let mut details: Vec<Detail> = terms
        .validate()
        .into_iter()
        .map(|(field, e)| {
            let code = serde_json::to_value(e)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            Detail::new(field, code.clone(), code.replace('_', " "))
        })
        .collect();
    if body.proposer_id == body.partner_id {
        details.push(Detail::new(
            "partner_id",
            "same_agent",
            "an agent cannot collaborate with itself",
        ));
    }
    if !details.is_empty() {
        return Err(ApiError::validation(details));
    }

    let mut tx = pool.begin().await?;
    lock_agents(&mut tx, [body.proposer_id, body.partner_id]).await?;
    let proposer = require_agent(&mut tx, body.proposer_id).await?;
    let partner = require_agent(&mut tx, body.partner_id).await?;
    if proposer.env != Env::Simulated || partner.env != Env::Simulated {
        return Err(ApiError::unprocessable(
            "unsupported_env",
            "collaborations are only available with simulated funds",
        ));
    }
    let intent = Intent::ProposeCollaboration {
        contribution: terms.proposer_contribution,
    };
    let decision = authorize(&intent, &proposer.policy, &proposer.snapshot, None);
    if let Decision::Deny(reasons) = decision {
        drop(tx);
        return Err(deny(
            &pool,
            None,
            "propose",
            vec![Denial {
                agent: proposer.id,
                role: "proposer",
                name: proposer.name,
                reasons,
            }],
        )
        .await);
    }

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO collaborations (id, env, state, proposer_id, partner_id, proposer_version, \
         objective, proposer_share_bps, proposer_contribution, partner_contribution) \
         VALUES ($1, $2, 'proposed', $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(Env::Simulated.as_str())
    .bind(proposer.id)
    .bind(partner.id)
    .bind(proposer.version)
    .bind(&terms.objective)
    .bind(terms.proposer_share_bps as i32)
    .bind(terms.proposer_contribution.minor())
    .bind(terms.partner_contribution.minor())
    .execute(&mut *tx)
    .await?;
    let note = if decision == Decision::RequireApproval {
        " Its contribution is above its automatic limit, so funding will need the owner's approval."
    } else {
        ""
    };
    record(
        &mut tx,
        proposer.id,
        Some(id),
        "collaboration_proposed",
        format!(
            "\"{}\" proposed a collaboration to \"{}\": \"{}\". Contributions {} and {}; \
             payout split {} / {} of the pool.{note}",
            proposer.name,
            partner.name,
            terms.objective,
            amount(terms.proposer_contribution),
            amount(terms.partner_contribution),
            display_bps(i64::from(terms.proposer_share_bps)),
            display_bps(i64::from(BPS - terms.proposer_share_bps)),
        ),
        json!({ "terms": terms, "env": Env::Simulated, "partner_id": partner.id }),
    )
    .await?;
    tx.commit().await?;
    let Json(view) = respond(&pool, id).await?;
    Ok((StatusCode::CREATED, Json(view)))
}

pub async fn accept(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(_): ApiJson<EmptyBody>,
) -> Result<Json<Collaboration>, ApiError> {
    decide(&pool, id, true).await
}

pub async fn reject(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(_): ApiJson<EmptyBody>,
) -> Result<Json<Collaboration>, ApiError> {
    decide(&pool, id, false).await
}

/// The partner's answer to a proposal.
async fn decide(pool: &PgPool, id: Uuid, accept: bool) -> Result<Json<Collaboration>, ApiError> {
    let (to, step) = if accept {
        (CollabState::Accepted, "accept")
    } else {
        (CollabState::Rejected, "reject")
    };
    let mut tx = pool.begin().await?;
    let row = load_row(&mut tx, id, true).await?;
    let state = row.state()?;
    if !state.can_become(to) {
        return Err(invalid_state(
            state,
            if accept { "accepted" } else { "rejected" },
        ));
    }
    lock_agents(&mut tx, [row.proposer_id, row.partner_id]).await?;
    let partner = require_agent(&mut tx, row.partner_id).await?;
    let terms = row.terms()?;
    // Accepting is the partner entering the agreement with its own
    // contribution: the same check as proposing it (`ProposeCollaboration`,
    // limits, balance, allowed type, pause). The collaboration's state was
    // checked above. `core::Intent` has no collaboration-specific accept
    // variant: adding one is a change to the sandbox's strict Intent mirror
    // (D-41), left to its owner.
    let decision = if accept {
        let intent = Intent::ProposeCollaboration {
            contribution: terms.partner_contribution,
        };
        authorize(&intent, &partner.policy, &partner.snapshot, None)
    } else {
        authorize(
            &Intent::Reject,
            &partner.policy,
            &partner.snapshot,
            Some(state.as_interaction_state()),
        )
    };
    if let Decision::Deny(reasons) = decision {
        drop(tx);
        return Err(deny(
            pool,
            Some(id),
            step,
            vec![Denial {
                agent: partner.id,
                role: "partner",
                name: partner.name,
                reasons,
            }],
        )
        .await);
    }
    sqlx::query(
        "UPDATE collaborations SET state = $2, partner_version = $3, updated_at = now(), \
         closed_at = CASE WHEN $2 = 'rejected' THEN now() END WHERE id = $1",
    )
    .bind(id)
    .bind(to.as_str())
    .bind(partner.version)
    .execute(&mut *tx)
    .await?;
    let (kind, summary) = if accept {
        let note = if decision == Decision::RequireApproval {
            " Its contribution is above its automatic limit, so funding will need the owner's \
             approval."
        } else {
            ""
        };
        (
            "collaboration_accepted",
            format!(
                "\"{}\" accepted the collaboration with \"{}\": \"{}\" (contribution {}).{note}",
                partner.name,
                row.proposer_name,
                terms.objective,
                amount(terms.partner_contribution)
            ),
        )
    } else {
        (
            "collaboration_rejected",
            format!(
                "\"{}\" rejected the collaboration proposed by \"{}\": \"{}\".",
                partner.name, row.proposer_name, terms.objective
            ),
        )
    };
    record(
        &mut tx,
        partner.id,
        Some(id),
        kind,
        summary,
        json!({ "proposer_id": row.proposer_id }),
    )
    .await?;
    tx.commit().await?;
    respond(pool, id).await
}

pub async fn fund(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<FundBody>,
) -> Result<Json<Collaboration>, ApiError> {
    let mut tx = pool.begin().await?;
    let row = load_row(&mut tx, id, true).await?;
    let state = row.state()?;
    if !state.can_become(CollabState::Funded) {
        return Err(invalid_state(state, "funded"));
    }
    let terms = row.terms()?;
    let env = row.env()?;
    lock_agents(&mut tx, [row.proposer_id, row.partner_id]).await?;
    let parties = [
        (
            require_agent(&mut tx, row.proposer_id).await?,
            "proposer",
            terms.proposer_contribution,
        ),
        (
            require_agent(&mut tx, row.partner_id).await?,
            "partner",
            terms.partner_contribution,
        ),
    ];

    // Both sides are checked against fresh balances and limits before either
    // contribution is posted.
    let mut denials = Vec::new();
    let mut needs_approval = Vec::new();
    for (agent, role, contribution) in &parties {
        let intent = Intent::CommitFunds {
            interaction_type: InteractionType::Collaboration,
            stake: *contribution,
        };
        match authorize(
            &intent,
            &agent.policy,
            &agent.snapshot,
            Some(state.as_interaction_state()),
        ) {
            Decision::Allow => {}
            Decision::RequireApproval => needs_approval.push((agent, *role, *contribution)),
            Decision::Deny(reasons) => denials.push(Denial {
                agent: agent.id,
                role,
                name: agent.name.clone(),
                reasons,
            }),
        }
    }
    if !denials.is_empty() {
        drop(tx);
        return Err(deny(&pool, Some(id), "fund", denials).await);
    }
    if !needs_approval.is_empty() && !body.owner_approved {
        let mut err = ApiError::conflict(
            "approval_required",
            "a contribution is above the agent's automatic limit: fund again with \
             `owner_approved: true` to approve it",
        );
        err.details = needs_approval
            .iter()
            .map(|(agent, role, c)| {
                Detail::new(
                    *role,
                    "above_approval_threshold",
                    format!(
                        "\"{}\" contributes {}, above its automatic limit of {}",
                        agent.name,
                        amount(*c),
                        amount(agent.policy.approval_threshold)
                    ),
                )
            })
            .collect();
        return Err(err);
    }

    for (agent, _, contribution) in &parties {
        ledger_db::post(
            &mut tx,
            &ledger::collab_commit(env, agent.id, id, *contribution)?,
        )
        .await?;
    }
    sqlx::query(
        "UPDATE collaborations SET state = 'funded', funded_at = now(), updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    for (agent, role, contribution) in &parties {
        let approved = needs_approval.iter().any(|(a, ..)| a.id == agent.id);
        let other = if *role == "proposer" {
            &row.partner_name
        } else {
            &row.proposer_name
        };
        record(
            &mut tx,
            agent.id,
            Some(id),
            "collaboration_funded",
            format!(
                "\"{}\" committed {} to the collaboration with \"{other}\": \"{}\".{}",
                agent.name,
                amount(*contribution),
                terms.objective,
                if approved {
                    " The owner approved this contribution."
                } else {
                    ""
                }
            ),
            json!({ "contribution": contribution, "env": env, "owner_approved": approved }),
        )
        .await?;
    }
    tx.commit().await?;
    respond(&pool, id).await
}

pub async fn complete(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(_): ApiJson<EmptyBody>,
) -> Result<Json<Collaboration>, ApiError> {
    let mut tx = pool.begin().await?;
    let row = load_row(&mut tx, id, true).await?;
    let state = row.state()?;
    if !state.can_become(CollabState::Completed) {
        return Err(invalid_state(state, "completed"));
    }
    let terms = row.terms()?;
    let env = row.env()?;
    lock_agents(&mut tx, [row.proposer_id, row.partner_id]).await?;
    let proposer = require_agent(&mut tx, row.proposer_id).await?;
    let partner = require_agent(&mut tx, row.partner_id).await?;
    // Like a challenge settlement: the payout releases escrow under terms
    // both agents were authorized for when it was funded, so it is not gated
    // again (a later pause or policy change must not trap the money).

    let (a_out, b_out) = terms
        .payouts()
        .ok_or_else(|| ApiError::internal("bad split"))?;
    ledger_db::post(
        &mut tx,
        &ledger::collab_payout(
            env,
            id,
            [
                (proposer.id, terms.proposer_contribution, a_out),
                (partner.id, terms.partner_contribution, b_out),
            ],
        )?,
    )
    .await?;
    sqlx::query(
        "UPDATE collaborations SET state = 'completed', proposer_payout = $2, \
         partner_payout = $3, closed_at = now(), updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(a_out.minor())
    .bind(b_out.minor())
    .execute(&mut *tx)
    .await?;
    for (agent, contribution, payout) in [
        (&proposer, terms.proposer_contribution, a_out),
        (&partner, terms.partner_contribution, b_out),
    ] {
        let net = net_of(payout, contribution);
        record(
            &mut tx,
            agent.id,
            Some(id),
            "collaboration_completed",
            format!(
                "Collaboration \"{}\" completed: \"{}\" received {} for a {} contribution \
                 (net {}).",
                terms.objective,
                agent.name,
                amount(payout),
                amount(contribution),
                signed(net)
            ),
            json!({
                "contribution": contribution, "payout": payout, "net": net, "env": env,
                "resolution": RESOLUTION,
            }),
        )
        .await?;
    }
    tx.commit().await?;
    respond(&pool, id).await
}

pub async fn cancel(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<CancelBody>,
) -> Result<Json<Collaboration>, ApiError> {
    let reason = body
        .reason
        .map(|r| r.trim().to_owned())
        .filter(|r| !r.is_empty());
    if reason.as_ref().is_some_and(|r| r.chars().count() > 200) {
        return Err(ApiError::validation(vec![Detail::new(
            "reason",
            "too_long",
            "reason must be at most 200 characters",
        )]));
    }
    let mut tx = pool.begin().await?;
    let row = load_row(&mut tx, id, true).await?;
    let state = row.state()?;
    if !state.can_become(CollabState::Cancelled) {
        return Err(invalid_state(state, "cancelled"));
    }
    let terms = row.terms()?;
    let env = row.env()?;
    lock_agents(&mut tx, [row.proposer_id, row.partner_id]).await?;
    let proposer = require_agent(&mut tx, row.proposer_id).await?;
    let partner = require_agent(&mut tx, row.partner_id).await?;
    let funded = state == CollabState::Funded;
    let why = reason
        .as_deref()
        .map(|r| format!(" Reason: {r}."))
        .unwrap_or_default();

    if funded {
        // Refunds return each contribution to its own agent, as a challenge
        // refund does; nothing new is put at risk, so no gate.
        for (agent, contribution) in [
            (&proposer, terms.proposer_contribution),
            (&partner, terms.partner_contribution),
        ] {
            ledger_db::post(
                &mut tx,
                &ledger::collab_refund(env, agent.id, id, contribution)?,
            )
            .await?;
            record(
                &mut tx,
                agent.id,
                Some(id),
                "collaboration_cancelled",
                format!(
                    "Collaboration \"{}\" was cancelled: {} was refunded to \"{}\".{why}",
                    terms.objective,
                    amount(contribution),
                    agent.name
                ),
                json!({ "refund": contribution, "env": env, "reason": reason }),
            )
            .await?;
        }
    } else {
        record(
            &mut tx,
            proposer.id,
            Some(id),
            "collaboration_cancelled",
            format!(
                "Collaboration \"{}\" between \"{}\" and \"{}\" was cancelled before funding; no \
                 funds moved.{why}",
                terms.objective, proposer.name, partner.name
            ),
            json!({ "env": env, "reason": reason }),
        )
        .await?;
    }
    sqlx::query(
        "UPDATE collaborations SET state = 'cancelled', status_reason = $2, closed_at = now(), \
         updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    respond(&pool, id).await
}
