//! Firms (docs/design/approach-and-firms.md section 2). Simulated env only.
//!
//! A lead agent founds a firm; member agents scout markets or review the
//! lead's drafts. A contribution is *used* when it fed a challenge the lead
//! actually proposed. Used contributions earn the firm's task fee (paid by
//! the lead through `authorize`) and, when the challenge settles with a
//! profit for the lead, a share of the member pool (`core::firm::split_profit`).
//! Losses stay with the lead. Every transfer is a balanced ledger posting
//! set between agents' available balances.

use aen_core::approach::{Approach, MarketFilter, market_matches, may_join_firms, may_lead_firm};
use aen_core::authorize::{Decision, Intent, authorize};
use aen_core::challenge::{ChallengeTerms, Direction, Party};
use aen_core::firm::{
    self, FirmOpening, FirmState, FirmTerms, MemberRole, RoleSlot, analysts_approve, choose_firm,
    member_pool_bps, split_profit,
};
use aen_core::ledger::{self, Env};
use aen_core::money::{Money, SignedMoney};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::error::{ApiError, Detail};
use crate::extract::{ApiJson, ApiPath};
use crate::ledger_db;
use crate::lifecycle::{
    AgentRec, TickReport, advisory_lock, deny_text, load_agent, require_agent, stake_text,
};
use crate::market::MarketView;
use crate::models::display_bps;
use crate::reasoner::{ReasonRequest, Task};
use crate::state::AppState;

/// A scout candidate older than this is not consumed by the lead.
pub const CANDIDATE_TTL_SECS: i64 = 3600;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBody {
    pub lead_agent_id: Uuid,
    pub name: String,
    pub mandate: String,
    #[serde(default)]
    pub lead_share_bps: Option<u32>,
    #[serde(default)]
    pub task_fee: Option<Money>,
    #[serde(default)]
    pub roles: Option<Vec<RoleSlot>>,
    #[serde(default)]
    pub markets: Option<MarketFilter>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinBody {
    pub agent_id: Uuid,
    pub role: MemberRole,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaveBody {
    pub agent_id: Uuid,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmptyBody {}

#[derive(Debug, Serialize)]
pub struct RoleView {
    pub role: MemberRole,
    pub slots: u32,
    pub weight_bps: u32,
    /// Current members in this role.
    pub filled: u32,
}

#[derive(Debug, Serialize)]
pub struct MemberView {
    pub agent_id: Uuid,
    pub name: String,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
    /// Set once the agent left (or the firm closed).
    pub left_at: Option<DateTime<Utc>>,
    pub contributions: i64,
    pub used: i64,
    /// Task fees plus profit shares received from this firm.
    pub earned: Money,
}

#[derive(Debug, Serialize)]
pub struct FirmStats {
    /// Challenges the lead proposed for the firm.
    pub challenges: i64,
    pub settled: i64,
    /// The lead's settled P&L on firm challenges, net of everything paid to
    /// members (task fees and profit shares).
    pub lead_realized_pnl: SignedMoney,
    pub paid_to_members: Money,
}

#[derive(Debug, Serialize)]
pub struct Firm {
    pub id: Uuid,
    pub env: String,
    pub name: String,
    pub mandate: String,
    pub lead_agent_id: Uuid,
    pub lead_name: String,
    pub lead_share_bps: u32,
    /// `10000 - lead_share_bps`; read-only.
    pub member_pool_bps: u32,
    pub task_fee: Money,
    pub roles: Vec<RoleView>,
    /// As set at creation; `null` = the lead's approach markets.
    pub markets: Option<MarketFilter>,
    /// The markets the firm works on right now.
    pub effective_markets: MarketFilter,
    pub state: FirmState,
    pub members: Vec<MemberView>,
    pub stats: FirmStats,
    /// What the owner may do next: `join`, `leave`, `close`.
    pub actions: Vec<&'static str>,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Contribution {
    pub id: Uuid,
    pub firm_id: Uuid,
    pub agent_id: Uuid,
    pub agent_name: String,
    pub role: String,
    pub kind: String,
    pub market_id: String,
    pub side: Option<String>,
    pub conviction_bps: i32,
    pub interaction_id: Option<Uuid>,
    pub used: bool,
    pub fee_paid: Money,
    pub payout: Money,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct ContributionRow {
    id: Uuid,
    firm_id: Uuid,
    agent_id: Uuid,
    agent_name: String,
    role: String,
    kind: String,
    market_id: String,
    side: Option<String>,
    conviction_bps: i32,
    interaction_id: Option<Uuid>,
    used: bool,
    fee_paid: i64,
    payout: i64,
    summary: String,
    created_at: DateTime<Utc>,
}

impl ContributionRow {
    fn view(self) -> Result<Contribution, ApiError> {
        Ok(Contribution {
            id: self.id,
            firm_id: self.firm_id,
            agent_id: self.agent_id,
            agent_name: self.agent_name,
            role: self.role,
            kind: self.kind,
            market_id: self.market_id,
            side: self.side,
            conviction_bps: self.conviction_bps,
            interaction_id: self.interaction_id,
            used: self.used,
            fee_paid: money(self.fee_paid)?,
            payout: money(self.payout)?,
            summary: self.summary,
            created_at: self.created_at,
        })
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FirmRow {
    pub id: Uuid,
    pub env: String,
    pub name: String,
    pub mandate: String,
    pub lead_agent_id: Uuid,
    pub lead_name: String,
    pub lead_share_bps: i32,
    pub task_fee: i64,
    pub roles: Value,
    pub markets: Option<Value>,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

const SELECT_SQL: &str = "
SELECT f.id, f.env, f.name, f.mandate, f.lead_agent_id, a.name AS lead_name, f.lead_share_bps,
       f.task_fee, f.roles, f.markets, f.state, f.created_at, f.closed_at
FROM firms f JOIN agents a ON a.id = f.lead_agent_id";

impl FirmRow {
    pub fn state(&self) -> Result<FirmState, ApiError> {
        FirmState::parse(&self.state).ok_or_else(|| ApiError::internal("unknown firm state"))
    }

    pub fn roles(&self) -> Result<Vec<RoleSlot>, ApiError> {
        serde_json::from_value(self.roles.clone())
            .map_err(|_| ApiError::internal("stored firm roles are unreadable"))
    }

    pub fn markets(&self) -> Result<Option<MarketFilter>, ApiError> {
        self.markets
            .clone()
            .filter(|v| !v.is_null())
            .map(serde_json::from_value)
            .transpose()
            .map_err(|_| ApiError::internal("stored firm markets are unreadable"))
    }

    pub fn task_fee(&self) -> Result<Money, ApiError> {
        Money::from_minor(self.task_fee).ok_or_else(|| ApiError::internal("negative task fee"))
    }

    pub fn member_pool_bps(&self) -> u32 {
        member_pool_bps(u32::try_from(self.lead_share_bps).unwrap_or(firm::DEFAULT_LEAD_SHARE_BPS))
    }
}

fn money(units: i64) -> Result<Money, ApiError> {
    Money::from_minor(units).ok_or_else(|| ApiError::internal("negative amount stored"))
}

/// Names the request field an error is about, for the form that sent it.
fn at(mut e: ApiError, path: &str) -> ApiError {
    e.details = vec![Detail::new(path, e.code, e.message.clone())];
    e
}

fn usdc(m: Money) -> String {
    stake_text(m, Env::Simulated)
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

pub async fn load_row(conn: &mut PgConnection, id: Uuid) -> Result<FirmRow, ApiError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT_SQL} WHERE f.id = $1")))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ApiError::not_found("firm"))
}

async fn lock_row(conn: &mut PgConnection, id: Uuid) -> Result<FirmRow, ApiError> {
    sqlx::query("SELECT 1 FROM firms WHERE id = $1 FOR UPDATE")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    load_row(conn, id).await
}

/// The open firm `agent` leads, if any.
pub async fn led_firm(conn: &mut PgConnection, agent: Uuid) -> Result<Option<FirmRow>, ApiError> {
    Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_SQL} WHERE f.lead_agent_id = $1 AND f.state = 'open'"
    )))
    .bind(agent)
    .fetch_optional(&mut *conn)
    .await?)
}

/// The open firm `agent` is a current member of, with its role.
pub async fn membership(
    conn: &mut PgConnection,
    agent: Uuid,
) -> Result<Option<(FirmRow, MemberRole)>, ApiError> {
    let row: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT m.firm_id, m.role FROM firm_members m JOIN firms f ON f.id = m.firm_id \
         WHERE m.agent_id = $1 AND m.left_at IS NULL AND f.state = 'open'",
    )
    .bind(agent)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((firm_id, role)) = row else {
        return Ok(None);
    };
    let role = MemberRole::parse(&role).ok_or_else(|| ApiError::internal("unknown role"))?;
    Ok(Some((load_row(conn, firm_id).await?, role)))
}

/// The approach markets of an agent's current version (defaults if none).
async fn agent_markets(conn: &mut PgConnection, agent: Uuid) -> Result<MarketFilter, ApiError> {
    Ok(match load_agent(conn, agent).await? {
        Some(a) => Approach::effective(&a.graph).markets,
        None => MarketFilter::default(),
    })
}

/// The firm's own markets, or its lead's.
pub async fn effective_markets(
    conn: &mut PgConnection,
    row: &FirmRow,
) -> Result<MarketFilter, ApiError> {
    match row.markets()? {
        Some(m) => Ok(m),
        None => agent_markets(conn, row.lead_agent_id).await,
    }
}

#[derive(sqlx::FromRow)]
struct MemberRow {
    agent_id: Uuid,
    name: String,
    role: String,
    joined_at: DateTime<Utc>,
    left_at: Option<DateTime<Utc>>,
    contributions: i64,
    used: i64,
    earned: i64,
}

/// Every agent that has been a member, latest membership per agent.
async fn members(conn: &mut PgConnection, firm: Uuid) -> Result<Vec<MemberView>, ApiError> {
    let rows: Vec<MemberRow> = sqlx::query_as(
        "SELECT * FROM (
           SELECT DISTINCT ON (m.agent_id) m.agent_id, a.name, m.role, m.joined_at, m.left_at,
             (SELECT COUNT(*) FROM firm_contributions c
               WHERE c.firm_id = m.firm_id AND c.agent_id = m.agent_id) AS contributions,
             (SELECT COUNT(*) FROM firm_contributions c
               WHERE c.firm_id = m.firm_id AND c.agent_id = m.agent_id AND c.used) AS used,
             (SELECT COALESCE(SUM(c.fee_paid + c.payout), 0)::BIGINT FROM firm_contributions c
               WHERE c.firm_id = m.firm_id AND c.agent_id = m.agent_id) AS earned
           FROM firm_members m JOIN agents a ON a.id = m.agent_id
           WHERE m.firm_id = $1
           ORDER BY m.agent_id, m.joined_at DESC
         ) x ORDER BY x.joined_at, x.agent_id",
    )
    .bind(firm)
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter()
        .map(|r| {
            Ok(MemberView {
                agent_id: r.agent_id,
                name: r.name,
                role: MemberRole::parse(&r.role).ok_or_else(|| ApiError::internal("bad role"))?,
                joined_at: r.joined_at,
                left_at: r.left_at,
                contributions: r.contributions,
                used: r.used,
                earned: money(r.earned)?,
            })
        })
        .collect()
}

async fn stats(conn: &mut PgConnection, row: &FirmRow) -> Result<FirmStats, ApiError> {
    let (challenges, settled): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE state = 'settled') FROM interactions \
         WHERE firm_id = $1",
    )
    .bind(row.id)
    .fetch_one(&mut *conn)
    .await?;
    let settle_pnl: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(e.delta), 0)::BIGINT FROM ledger_entries e \
         JOIN interactions i ON i.id = e.interaction_id \
         WHERE i.firm_id = $1 AND e.reason = 'settle' AND e.agent_id = $2",
    )
    .bind(row.id)
    .bind(row.lead_agent_id)
    .fetch_one(&mut *conn)
    .await?;
    let paid: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(fee_paid + payout), 0)::BIGINT FROM firm_contributions \
         WHERE firm_id = $1",
    )
    .bind(row.id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(FirmStats {
        challenges,
        settled,
        lead_realized_pnl: SignedMoney::from_minor(settle_pnl.saturating_sub(paid)),
        paid_to_members: money(paid)?,
    })
}

pub async fn view(conn: &mut PgConnection, row: FirmRow) -> Result<Firm, ApiError> {
    let state = row.state()?;
    let members = members(conn, row.id).await?;
    let roles: Vec<RoleView> = row
        .roles()?
        .into_iter()
        .map(|r| RoleView {
            role: r.role,
            slots: r.slots,
            weight_bps: r.weight_bps,
            filled: members
                .iter()
                .filter(|m| m.role == r.role && m.left_at.is_none())
                .count() as u32,
        })
        .collect();
    let mut actions = Vec::new();
    if state == FirmState::Open {
        if roles.iter().any(|r| r.filled < r.slots) {
            actions.push("join");
        }
        if members.iter().any(|m| m.left_at.is_none()) {
            actions.push("leave");
        }
        actions.push("close");
    }
    Ok(Firm {
        stats: stats(conn, &row).await?,
        effective_markets: effective_markets(conn, &row).await?,
        markets: row.markets()?,
        id: row.id,
        member_pool_bps: row.member_pool_bps(),
        lead_share_bps: u32::try_from(row.lead_share_bps).unwrap_or_default(),
        task_fee: row.task_fee()?,
        env: row.env,
        name: row.name,
        mandate: row.mandate,
        lead_agent_id: row.lead_agent_id,
        lead_name: row.lead_name,
        roles,
        state,
        members,
        actions,
        created_at: row.created_at,
        closed_at: row.closed_at,
    })
}

async fn respond(pool: &PgPool, id: Uuid) -> Result<Json<Firm>, ApiError> {
    let mut conn = pool.acquire().await?;
    let row = load_row(&mut conn, id).await?;
    Ok(Json(view(&mut conn, row).await?))
}

pub(crate) async fn record(
    conn: &mut PgConnection,
    agent: Option<Uuid>,
    firm: Uuid,
    interaction: Option<Uuid>,
    kind: &str,
    summary: String,
    detail: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO activity (agent_id, firm_id, interaction_id, kind, summary, detail) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(agent)
    .bind(firm)
    .bind(interaction)
    .bind(kind)
    .bind(summary)
    .bind(detail)
    .execute(conn)
    .await?;
    Ok(())
}

async fn current_role_count(
    conn: &mut PgConnection,
    firm: Uuid,
    role: MemberRole,
) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM firm_members WHERE firm_id = $1 AND role = $2 AND left_at IS NULL",
    )
    .bind(firm)
    .bind(role.as_str())
    .fetch_one(&mut *conn)
    .await?)
}

async fn is_current_member(conn: &mut PgConnection, agent: Uuid) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM firm_members WHERE agent_id = $1 AND left_at IS NULL)",
    )
    .bind(agent)
    .fetch_one(&mut *conn)
    .await?)
}

/// Adds `agent` to the locked, open `firm` in `role`. Checks the slot and
/// that the agent is not already a member anywhere or the firm's lead.
async fn add_member(
    conn: &mut PgConnection,
    firm: &FirmRow,
    agent: &AgentRec,
    role: MemberRole,
    how: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    if firm.state()? != FirmState::Open {
        return Err(ApiError::conflict("invalid_state", "the firm is closed"));
    }
    if agent.id == firm.lead_agent_id {
        return Err(at(
            ApiError::unprocessable("is_lead", "the lead cannot join its own firm as a member"),
            "agent_id",
        ));
    }
    if agent.env != Env::Simulated {
        return Err(at(
            ApiError::unprocessable(
                "unsupported_env",
                "firms are available for simulated agents only",
            ),
            "agent_id",
        ));
    }
    // Serialize joins per agent so two firms cannot take it at once.
    advisory_lock(conn, &format!("aen:firm-member:{}", agent.id)).await?;
    if is_current_member(conn, agent.id).await? {
        return Err(at(
            ApiError::conflict("already_member", "the agent is already a member of a firm"),
            "agent_id",
        ));
    }
    let slots = firm
        .roles()?
        .iter()
        .find(|r| r.role == role)
        .map_or(0, |r| i64::from(r.slots));
    if current_role_count(conn, firm.id, role).await? >= slots {
        return Err(at(
            ApiError::conflict(
                "no_free_slot",
                format!("the firm has no free {} slot", role.as_str()),
            ),
            "role",
        ));
    }
    sqlx::query(
        "INSERT INTO firm_members (firm_id, agent_id, role, joined_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(firm.id)
    .bind(agent.id)
    .bind(role.as_str())
    .bind(now)
    .execute(&mut *conn)
    .await?;
    record(
        conn,
        Some(agent.id),
        firm.id,
        None,
        "firm_joined",
        format!(
            "\"{}\" joined the firm \"{}\" (led by \"{}\") as a {}{how}. It earns a task fee of \
             {} per used contribution and a share of the {} member pool.",
            agent.name,
            firm.name,
            firm.lead_name,
            role.as_str(),
            usdc(firm.task_fee()?),
            display_bps(i64::from(firm.member_pool_bps())),
        ),
        json!({ "firm_id": firm.id, "role": role }),
    )
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn list(State(pool): State<PgPool>) -> Result<Json<Vec<Firm>>, ApiError> {
    let mut conn = pool.acquire().await?;
    let rows: Vec<FirmRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_SQL} ORDER BY f.created_at DESC, f.id"
    )))
    .fetch_all(&mut *conn)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(view(&mut conn, row).await?);
    }
    Ok(Json(out))
}

pub async fn get(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Firm>, ApiError> {
    respond(&pool, id).await
}

pub async fn create(
    State(pool): State<PgPool>,
    ApiJson(body): ApiJson<CreateBody>,
) -> Result<(StatusCode, Json<Firm>), ApiError> {
    let terms = FirmTerms {
        name: body.name.trim().to_owned(),
        mandate: body.mandate.trim().to_owned(),
        lead_share_bps: body.lead_share_bps.unwrap_or(firm::DEFAULT_LEAD_SHARE_BPS),
        task_fee: body.task_fee.unwrap_or(Money::ZERO),
        roles: body.roles.unwrap_or_else(firm::default_roles),
        markets: body.markets,
    };
    let details: Vec<Detail> = terms
        .validate()
        .into_iter()
        .map(|(path, message)| Detail::new(path, "invalid", message))
        .collect();
    if !details.is_empty() {
        return Err(ApiError::validation(details));
    }

    let now = Utc::now();
    let id = Uuid::new_v4();
    let mut tx = pool.begin().await?;
    advisory_lock(&mut tx, &format!("aen:firm-lead:{}", body.lead_agent_id)).await?;
    let lead = load_agent(&mut tx, body.lead_agent_id)
        .await?
        .ok_or_else(|| at(ApiError::not_found("agent"), "lead_agent_id"))?;
    let refuse = |code, message: &str| at(ApiError::unprocessable(code, message), "lead_agent_id");
    if lead.env != Env::Simulated {
        return Err(refuse(
            "unsupported_env",
            "firms are available for simulated agents only",
        ));
    }
    let approach = Approach::effective(&lead.graph);
    if !may_lead_firm(&approach) {
        return Err(refuse(
            "not_a_lead",
            "the agent's approach must have firm_role \"lead\" and collaboration of at least 30%",
        ));
    }
    if led_firm(&mut tx, lead.id).await?.is_some() {
        return Err(refuse(
            "already_leads_firm",
            "the agent already leads an open firm",
        ));
    }
    if is_current_member(&mut tx, lead.id).await? {
        return Err(refuse(
            "already_member",
            "the agent is a member of another firm; it must leave first",
        ));
    }
    sqlx::query(
        "INSERT INTO firms (id, env, name, mandate, lead_agent_id, lead_share_bps, task_fee, \
         roles, markets, state, created_at) \
         VALUES ($1, 'simulated', $2, $3, $4, $5, $6, $7, $8, 'open', $9)",
    )
    .bind(id)
    .bind(&terms.name)
    .bind(&terms.mandate)
    .bind(lead.id)
    .bind(i32::try_from(terms.lead_share_bps).map_err(ApiError::internal)?)
    .bind(terms.task_fee.minor())
    .bind(serde_json::to_value(&terms.roles).map_err(ApiError::internal)?)
    .bind(
        terms
            .markets
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .map_err(ApiError::internal)?,
    )
    .bind(now)
    .execute(&mut *tx)
    .await?;
    record(
        &mut tx,
        Some(lead.id),
        id,
        None,
        "firm_created",
        format!(
            "\"{}\" founded the firm \"{}\": it keeps {} of the profit on firm challenges and \
             shares {} with members. Mandate: {}",
            lead.name,
            terms.name,
            display_bps(i64::from(terms.lead_share_bps)),
            display_bps(i64::from(terms.member_pool_bps())),
            terms.mandate
        ),
        json!({
            "firm_id": id, "lead_share_bps": terms.lead_share_bps,
            "task_fee": terms.task_fee, "roles": terms.roles, "env": Env::Simulated,
        }),
    )
    .await?;
    let row = load_row(&mut tx, id).await?;
    let firm = view(&mut tx, row).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(firm)))
}

pub async fn join(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<JoinBody>,
) -> Result<Json<Firm>, ApiError> {
    let mut tx = pool.begin().await?;
    let firm = lock_row(&mut tx, id).await?;
    let agent = load_agent(&mut tx, body.agent_id)
        .await?
        .ok_or_else(|| at(ApiError::not_found("agent"), "agent_id"))?;
    add_member(
        &mut tx,
        &firm,
        &agent,
        body.role,
        " (added by its owner)",
        Utc::now(),
    )
    .await?;
    tx.commit().await?;
    respond(&pool, id).await
}

pub async fn leave(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<LeaveBody>,
) -> Result<Json<Firm>, ApiError> {
    let mut tx = pool.begin().await?;
    let firm = lock_row(&mut tx, id).await?;
    let left = sqlx::query(
        "UPDATE firm_members SET left_at = $3 WHERE firm_id = $1 AND agent_id = $2 \
         AND left_at IS NULL",
    )
    .bind(id)
    .bind(body.agent_id)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;
    if left.rows_affected() == 0 {
        return Err(at(
            ApiError::conflict(
                "not_a_member",
                "the agent is not a current member of this firm",
            ),
            "agent_id",
        ));
    }
    let name: String = sqlx::query_scalar("SELECT name FROM agents WHERE id = $1")
        .bind(body.agent_id)
        .fetch_one(&mut *tx)
        .await?;
    record(
        &mut tx,
        Some(body.agent_id),
        id,
        None,
        "firm_left",
        format!(
            "\"{name}\" left the firm \"{}\". Payouts on challenges it already contributed to \
             still settle.",
            firm.name
        ),
        json!({ "firm_id": id }),
    )
    .await?;
    tx.commit().await?;
    respond(&pool, id).await
}

pub async fn close(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(_body): ApiJson<EmptyBody>,
) -> Result<Json<Firm>, ApiError> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let firm = lock_row(&mut tx, id).await?;
    if firm.state()? != FirmState::Open {
        return Err(ApiError::conflict(
            "invalid_state",
            "the firm is already closed",
        ));
    }
    sqlx::query("UPDATE firms SET state = 'closed', closed_at = $2 WHERE id = $1")
        .bind(id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE firm_members SET left_at = $2 WHERE firm_id = $1 AND left_at IS NULL")
        .bind(id)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    record(
        &mut tx,
        Some(firm.lead_agent_id),
        id,
        None,
        "firm_closed",
        format!(
            "\"{}\" closed the firm \"{}\". No new contributions; payouts on open firm \
             challenges still settle.",
            firm.lead_name, firm.name
        ),
        json!({ "firm_id": id }),
    )
    .await?;
    tx.commit().await?;
    respond(&pool, id).await
}

pub async fn contributions(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Vec<Contribution>>, ApiError> {
    let mut conn = pool.acquire().await?;
    load_row(&mut conn, id).await?;
    let rows: Vec<ContributionRow> = sqlx::query_as(
        "SELECT c.id, c.firm_id, c.agent_id, a.name AS agent_name, c.role, c.kind, c.market_id, \
         c.side, c.conviction_bps, c.interaction_id, c.used, c.fee_paid, c.payout, c.summary, \
         c.created_at FROM firm_contributions c JOIN agents a ON a.id = c.agent_id \
         WHERE c.firm_id = $1 ORDER BY c.seq DESC",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(ContributionRow::view)
            .collect::<Result<_, _>>()?,
    ))
}

// ---------------------------------------------------------------------------
// Runtime (called from the tick)
// ---------------------------------------------------------------------------

const fn side_str(d: Direction) -> &'static str {
    match d {
        Direction::Above => "above",
        Direction::Below => "below",
    }
}

/// Autonomous membership: every active simulated agent with
/// `firm_role = member` and collaboration ≥ 30 % that is not in a firm joins
/// the best open firm (see `core::firm::choose_firm`).
pub async fn auto_join_phase(
    state: &AppState,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let pool = &state.pool;
    let open: Vec<FirmRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "{SELECT_SQL} WHERE f.state = 'open' ORDER BY f.created_at, f.id"
    )))
    .fetch_all(pool)
    .await?;
    if open.is_empty() {
        return Ok(());
    }
    let candidates: Vec<Uuid> = sqlx::query_scalar(
        "SELECT a.id FROM agents a WHERE a.status = 'active' AND a.env = 'simulated' \
         AND NOT EXISTS (SELECT 1 FROM firm_members m WHERE m.agent_id = a.id AND m.left_at IS NULL) \
         AND NOT EXISTS (SELECT 1 FROM firms f WHERE f.lead_agent_id = a.id AND f.state = 'open') \
         ORDER BY a.created_at, a.id",
    )
    .fetch_all(pool)
    .await?;
    for agent_id in candidates {
        let outcome = join_one(state, agent_id, &open, now, report).await;
        crate::lifecycle::step(report, outcome);
    }
    Ok(())
}

async fn join_one(
    state: &AppState,
    agent_id: Uuid,
    open: &[FirmRow],
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let mut conn = state.pool.acquire().await?;
    let Some(agent) = load_agent(&mut conn, agent_id).await? else {
        return Ok(());
    };
    let approach = Approach::effective(&agent.graph);
    if !may_join_firms(&approach) {
        return Ok(());
    }
    let mut markets = Vec::with_capacity(open.len());
    let mut filled = Vec::with_capacity(open.len());
    for f in open {
        markets.push(effective_markets(&mut conn, f).await?);
        let mut free = Vec::new();
        for r in f.roles()? {
            let taken = current_role_count(&mut conn, f.id, r.role).await?;
            let left = (i64::from(r.slots) - taken).max(0) as u32;
            free.push((r.role, r.weight_bps, left));
        }
        filled.push(free);
    }
    drop(conn);
    let openings: Vec<FirmOpening<'_, usize>> = open
        .iter()
        .enumerate()
        .filter(|(_, f)| f.lead_agent_id != agent.id)
        .map(|(i, f)| FirmOpening {
            id: i,
            member_pool_bps: f.member_pool_bps(),
            markets: &markets[i],
            free: filled[i].clone(),
        })
        .collect();
    let Some((i, role)) = choose_firm(&approach.markets, &openings) else {
        return Ok(());
    };
    let mut tx = state.pool.begin().await?;
    let firm = lock_row(&mut tx, open[i].id).await?;
    match add_member(&mut tx, &firm, &agent, role, "", now).await {
        Ok(()) => {
            tx.commit().await?;
            report.firm_joined += 1;
            Ok(())
        }
        // Lost a race for the slot or the firm closed: try again next tick.
        Err(e) if e.code == "no_free_slot" || e.code == "already_member" => Ok(()),
        Err(e) if e.code == "invalid_state" => Ok(()),
        Err(e) => Err(e),
    }
}

/// Asks `agent`'s reasoner for a view on `market`.
async fn evaluate(
    state: &AppState,
    agent: &AgentRec,
    market: &MarketView,
    now: DateTime<Utc>,
    incoming: Option<&ChallengeTerms>,
) -> Result<crate::reasoner::ReasonerOutput, String> {
    let ctx = agent.ctx();
    let req = ReasonRequest {
        task: if incoming.is_some() {
            Task::EvaluateProposal
        } else {
            Task::EvaluateOpportunity
        },
        agent: &ctx,
        market,
        now: now.timestamp(),
        incoming,
        party: Party::A,
        funding_window_secs: state.settings.funding_window_secs,
        instructions: None,
    };
    let out = state
        .reasoner
        .reason(&req)
        .await
        .map_err(|e| e.to_string())?;
    out.validate(&req).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Scout task: when `view` is inside both the firm's markets and the
/// scout's own, the scout evaluates it and submits one candidate (at most
/// one unused candidate per market per scout).
pub async fn scout(
    state: &AppState,
    agent: &AgentRec,
    firm: &FirmRow,
    view: &MarketView,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let mut conn = state.pool.acquire().await?;
    let facts = view.facts(now.timestamp());
    let firm_markets = effective_markets(&mut conn, firm).await?;
    let own = Approach::effective(&agent.graph).markets;
    if market_matches(&firm_markets, &facts).is_err() || market_matches(&own, &facts).is_err() {
        return Ok(());
    }
    let market_id = view.market.key();
    let pending: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM firm_contributions WHERE firm_id = $1 AND agent_id = $2 \
         AND market_id = $3 AND kind = 'scout_candidate' AND NOT used AND created_at > $4)",
    )
    .bind(firm.id)
    .bind(agent.id)
    .bind(&market_id)
    .bind(now - chrono::Duration::seconds(CANDIDATE_TTL_SECS))
    .fetch_one(&mut *conn)
    .await?;
    if pending {
        return Ok(());
    }
    drop(conn);
    let out = match evaluate(state, agent, view, now, None).await {
        Ok(out) => out,
        Err(e) => {
            report.errors.push(format!("reasoner: {e}"));
            return Ok(());
        }
    };
    let summary = format!(
        "\"{}\" scouted {} for \"{}\": expects it to end {} ({} conviction).",
        agent.name,
        view.title,
        firm.name,
        side_str(out.stance),
        display_bps(i64::from(out.conviction_bps))
    );
    let mut tx = state.pool.begin().await?;
    // The firm may have closed or the scout left while it was reasoning.
    match membership(&mut tx, agent.id).await? {
        Some((f, MemberRole::Scout)) if f.id == firm.id => {}
        _ => return Ok(()),
    }
    let id = Uuid::new_v4();
    insert_contribution(
        &mut tx,
        id,
        firm.id,
        agent.id,
        MemberRole::Scout,
        &market_id,
        Some(view),
        out.stance,
        out.conviction_bps,
        &summary,
        now,
    )
    .await?;
    record(
        &mut tx,
        Some(agent.id),
        firm.id,
        None,
        "firm_contribution",
        summary,
        json!({ "firm_id": firm.id, "contribution_id": id, "kind": "scout_candidate",
                "market_id": market_id, "side": out.stance, "conviction_bps": out.conviction_bps }),
    )
    .await?;
    tx.commit().await?;
    report.firm_contributions += 1;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_contribution(
    conn: &mut PgConnection,
    id: Uuid,
    firm: Uuid,
    agent: Uuid,
    role: MemberRole,
    market_id: &str,
    view: Option<&MarketView>,
    side: Direction,
    conviction_bps: u32,
    summary: &str,
    now: DateTime<Utc>,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO firm_contributions (id, firm_id, agent_id, role, kind, market_id, \
         market_view, side, conviction_bps, summary, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(firm)
    .bind(agent)
    .bind(role.as_str())
    .bind(role.contribution_kind().as_str())
    .bind(market_id)
    .bind(
        view.map(serde_json::to_value)
            .transpose()
            .map_err(ApiError::internal)?,
    )
    .bind(side_str(side))
    .bind(i32::try_from(conviction_bps.min(10_000)).unwrap_or(0))
    .bind(summary)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// The lead's pick among its scouts' fresh, unused candidates: highest
/// conviction first, then newest. Candidates on markets the lead already
/// has an open challenge on are skipped.
pub async fn best_candidate(
    conn: &mut PgConnection,
    firm: &FirmRow,
    now: DateTime<Utc>,
) -> Result<Option<(Uuid, MarketView)>, ApiError> {
    let rows: Vec<(Uuid, Value)> = sqlx::query_as(
        "SELECT c.id, c.market_view FROM firm_contributions c \
         WHERE c.firm_id = $1 AND c.kind = 'scout_candidate' AND NOT c.used \
           AND c.market_view IS NOT NULL AND c.created_at > $2 \
           AND NOT EXISTS (SELECT 1 FROM interactions i WHERE i.market_key = c.market_id \
             AND (i.proposer_id = $3 OR i.counterparty_id = $3) \
             AND i.state NOT IN ('settled', 'cancelled', 'failed')) \
         ORDER BY c.conviction_bps DESC, c.seq DESC LIMIT 5",
    )
    .bind(firm.id)
    .bind(now - chrono::Duration::seconds(CANDIDATE_TTL_SECS))
    .bind(firm.lead_agent_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .find_map(|(id, v)| serde_json::from_value(v).ok().map(|view| (id, view))))
}

/// One analyst's review of a lead's draft.
pub struct Signal {
    pub agent: Uuid,
    pub name: String,
    /// Conviction that the drafted side (the lead's) wins.
    pub conviction_bps: u32,
    pub summary: String,
}

/// Analyst task: every current analyst evaluates the lead's draft with its
/// own reasoner. A failing reasoner gives no signal (reported).
pub async fn analyst_signals(
    state: &AppState,
    firm: &FirmRow,
    view: &MarketView,
    terms: &ChallengeTerms,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<Vec<Signal>, ApiError> {
    let mut conn = state.pool.acquire().await?;
    let analysts: Vec<Uuid> = sqlx::query_scalar(
        "SELECT agent_id FROM firm_members WHERE firm_id = $1 AND role = 'analyst' \
         AND left_at IS NULL ORDER BY joined_at, agent_id",
    )
    .bind(firm.id)
    .fetch_all(&mut *conn)
    .await?;
    let mut recs = Vec::new();
    for id in analysts {
        if let Some(a) = load_agent(&mut conn, id).await? {
            recs.push(a);
        }
    }
    drop(conn);
    let mut out = Vec::new();
    for analyst in recs {
        match evaluate(state, &analyst, view, now, None).await {
            Ok(o) => {
                let conviction = if o.stance == terms.direction {
                    o.conviction_bps
                } else {
                    10_000 - o.conviction_bps.min(10_000)
                };
                let summary = format!(
                    "\"{}\" reviewed \"{}\"'s draft on {}: {} conviction that \"{} {}\" wins.",
                    analyst.name,
                    firm.lead_name,
                    view.title,
                    display_bps(i64::from(conviction)),
                    side_str(terms.direction),
                    crate::lifecycle::format_value(terms.formula, terms.threshold),
                );
                out.push(Signal {
                    agent: analyst.id,
                    name: analyst.name,
                    conviction_bps: conviction,
                    summary,
                });
            }
            Err(e) => report.errors.push(format!("reasoner: {e}")),
        }
    }
    Ok(out)
}

/// Records analyst signals (unused) and returns their ids, then applies the
/// gate: `true` when the lead may propose.
#[allow(clippy::too_many_arguments)]
pub async fn record_signals(
    conn: &mut PgConnection,
    firm: &FirmRow,
    market_id: &str,
    terms: &ChallengeTerms,
    signals: &[Signal],
    lead_min_conviction_bps: u32,
    now: DateTime<Utc>,
    report: &mut TickReport,
) -> Result<(Vec<Uuid>, bool), ApiError> {
    let mut ids = Vec::new();
    for s in signals {
        let id = Uuid::new_v4();
        insert_contribution(
            conn,
            id,
            firm.id,
            s.agent,
            MemberRole::Analyst,
            market_id,
            None,
            terms.direction,
            s.conviction_bps,
            &s.summary,
            now,
        )
        .await?;
        record(
            conn,
            Some(s.agent),
            firm.id,
            None,
            "firm_contribution",
            s.summary.clone(),
            json!({ "firm_id": firm.id, "contribution_id": id, "kind": "analyst_signal",
                    "market_id": market_id, "conviction_bps": s.conviction_bps }),
        )
        .await?;
        report.firm_contributions += 1;
        ids.push(id);
    }
    let convictions: Vec<u32> = signals.iter().map(|s| s.conviction_bps).collect();
    let approved = analysts_approve(&convictions, lead_min_conviction_bps);
    if !approved {
        let mean = firm::analyst_consensus_bps(&convictions).unwrap_or_default();
        record(
            conn,
            Some(firm.lead_agent_id),
            firm.id,
            None,
            "firm_draft_dropped",
            format!(
                "\"{}\" dropped a draft challenge: its analysts' average conviction ({}) is \
                 below its minimum ({}). No funds moved.",
                firm.lead_name,
                display_bps(i64::from(mean)),
                display_bps(i64::from(lead_min_conviction_bps)),
            ),
            json!({ "firm_id": firm.id, "market_id": market_id, "consensus_bps": mean,
                    "min_conviction_bps": lead_min_conviction_bps }),
        )
        .await?;
    }
    Ok((ids, approved))
}

/// Marks contributions as used by `interaction` (the lead's firm challenge)
/// and pays each its task fee, gated by `authorize` as a payment by the lead.
/// Runs in the transaction that created the interaction.
pub async fn use_contributions(
    conn: &mut PgConnection,
    firm: &FirmRow,
    interaction: Uuid,
    contributions: &[Uuid],
) -> Result<(), ApiError> {
    sqlx::query("UPDATE interactions SET firm_id = $2 WHERE id = $1")
        .bind(interaction)
        .bind(firm.id)
        .execute(&mut *conn)
        .await?;
    let fee = firm.task_fee()?;
    for id in contributions {
        let row: Option<(Uuid, String, bool)> = sqlx::query_as(
            "SELECT c.agent_id, a.name, c.used FROM firm_contributions c \
             JOIN agents a ON a.id = c.agent_id WHERE c.id = $1 FOR UPDATE OF c",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((member, member_name, false)) = row else {
            continue;
        };
        sqlx::query("UPDATE firm_contributions SET used = true, interaction_id = $2 WHERE id = $1")
            .bind(id)
            .bind(interaction)
            .execute(&mut *conn)
            .await?;
        if fee.is_zero() {
            continue;
        }
        // Lock both agents in sorted order (as every money move does), then
        // read a fresh snapshot of the payer.
        let mut agents = [firm.lead_agent_id, member];
        agents.sort();
        for a in agents {
            advisory_lock(conn, &format!("aen:agent:{a}")).await?;
        }
        let lead = require_agent(conn, firm.lead_agent_id).await?;
        let decision = authorize(
            &Intent::PayTaskFee { fee },
            &lead.policy,
            &lead.snapshot,
            None,
        );
        match decision {
            Decision::Allow => {
                let set = ledger::firm_task_fee(
                    Env::Simulated,
                    lead.id,
                    member,
                    *id,
                    Some(interaction),
                    fee,
                )?;
                ledger_db::post(conn, &set).await?;
                sqlx::query("UPDATE firm_contributions SET fee_paid = $2 WHERE id = $1")
                    .bind(id)
                    .bind(fee.minor())
                    .execute(&mut *conn)
                    .await?;
                record(
                    conn,
                    Some(member),
                    firm.id,
                    Some(interaction),
                    "firm_task_fee",
                    format!(
                        "\"{}\" paid \"{member_name}\" a task fee of {} for a contribution it \
                         used in a firm challenge.",
                        lead.name,
                        usdc(fee)
                    ),
                    json!({ "firm_id": firm.id, "contribution_id": id, "fee": fee,
                            "amount": fee, "payer": lead.id, "env": Env::Simulated }),
                )
                .await?;
            }
            other => {
                let why = match &other {
                    Decision::Deny(reasons) => deny_text(reasons),
                    _ => "the fee is above its approval threshold (fees are never paid on \
                          approval)"
                        .to_owned(),
                };
                record(
                    conn,
                    Some(firm.lead_agent_id),
                    firm.id,
                    Some(interaction),
                    "firm_fee_denied",
                    format!(
                        "\"{}\" could not pay \"{member_name}\" its task fee of {}: {why}. The \
                         contribution still counts for the profit share.",
                        lead.name,
                        usdc(fee)
                    ),
                    json!({ "firm_id": firm.id, "contribution_id": id, "fee": fee,
                            "decision": match other {
                                Decision::Deny(r) => json!({ "deny": r }),
                                _ => json!("require_approval"),
                            } }),
                )
                .await?;
            }
        }
    }
    Ok(())
}

/// After a firm challenge settles: the lead shares a positive realized P&L
/// with the members whose contributions were used on it (one posting set per
/// interaction). Runs in the settling transaction; a loss or a void pays
/// nothing. Returns whether anything was paid.
pub async fn pay_profit_share(
    conn: &mut PgConnection,
    interaction: Uuid,
) -> Result<bool, ApiError> {
    let firm_id: Option<Uuid> =
        sqlx::query_scalar("SELECT firm_id FROM interactions WHERE id = $1")
            .bind(interaction)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();
    let Some(firm_id) = firm_id else {
        return Ok(false);
    };
    let firm = load_row(conn, firm_id).await?;
    let profit: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries \
         WHERE interaction_id = $1 AND reason = 'settle' AND agent_id = $2",
    )
    .bind(interaction)
    .bind(firm.lead_agent_id)
    .fetch_one(&mut *conn)
    .await?;
    if profit <= 0 {
        return Ok(false);
    }
    let profit = money(profit)?;
    let used: Vec<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT c.id, c.agent_id, a.name, c.role FROM firm_contributions c \
         JOIN agents a ON a.id = c.agent_id \
         WHERE c.interaction_id = $1 AND c.used ORDER BY c.seq",
    )
    .bind(interaction)
    .fetch_all(&mut *conn)
    .await?;
    if used.is_empty() {
        return Ok(false);
    }
    let roles: Vec<MemberRole> = used
        .iter()
        .map(|u| MemberRole::parse(&u.3).ok_or_else(|| ApiError::internal("bad role")))
        .collect::<Result<_, _>>()?;
    let split = split_profit(profit, firm.member_pool_bps(), &firm.roles()?, &roles);
    let payouts: Vec<(Uuid, Money)> = used
        .iter()
        .zip(&split.payouts)
        .map(|(u, p)| (u.1, *p))
        .collect();
    if payouts.iter().all(|(_, p)| p.is_zero()) {
        return Ok(false);
    }
    let paid_total = money(profit.minor() - split.lead_keeps.minor())?;
    let set = ledger::firm_payout(Env::Simulated, firm.lead_agent_id, interaction, &payouts)?;
    ledger_db::post(conn, &set).await?;
    for ((id, member, name, role), amount) in used.iter().zip(&split.payouts) {
        sqlx::query("UPDATE firm_contributions SET payout = $2 WHERE id = $1")
            .bind(id)
            .bind(amount.minor())
            .execute(&mut *conn)
            .await?;
        if amount.is_zero() {
            continue;
        }
        record(
            conn,
            Some(*member),
            firm.id,
            Some(interaction),
            "firm_payout",
            format!(
                "\"{}\" shared its profit with \"{name}\": {} for its {role} contribution to a \
                 settled firm challenge.",
                firm.lead_name,
                usdc(*amount)
            ),
            json!({ "firm_id": firm.id, "contribution_id": id, "payout": amount,
                    "amount": amount, "profit": profit, "pool": split.pool,
                    "env": Env::Simulated }),
        )
        .await?;
    }
    record(
        conn,
        Some(firm.lead_agent_id),
        firm.id,
        Some(interaction),
        "firm_payout",
        format!(
            "\"{}\" made {} on a firm challenge and paid {} of it to its members ({} member \
             pool); it keeps {}.",
            firm.lead_name,
            usdc(profit),
            usdc(paid_total),
            display_bps(i64::from(firm.member_pool_bps())),
            usdc(split.lead_keeps)
        ),
        json!({ "firm_id": firm.id, "profit": profit, "pool": split.pool,
                "amount": paid_total, "lead_keeps": split.lead_keeps, "env": Env::Simulated }),
    )
    .await?;
    Ok(true)
}
