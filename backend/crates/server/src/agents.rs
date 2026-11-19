//! Agent endpoints: CRUD, versions, status, ledger, allocate / deallocate.

use aen_core::authorize::AgentStatus;
use aen_core::ledger::{self, Env, LedgerError, PostingSet};
use aen_core::money::{Money, SignedMoney};
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

use crate::custom_code;
use crate::error::ApiError;
use crate::extract::{ApiJson, ApiPath};
use crate::ledger_db::{self, PostOutcome};
use crate::models::{
    Agent, AgentInput, AgentVersion, AmountBody, Balances, CloneInfo, LedgerEntry, Stats,
    StatusBody, display_usdc,
};
use crate::state::AppState;

#[derive(sqlx::FromRow)]
struct AgentRow {
    id: Uuid,
    name: String,
    description: String,
    status: String,
    env: String,
    wallet_address: Option<String>,
    vault_address: Option<String>,
    vault_owner: Option<String>,
    current_version: i32,
    config: Value,
    policy: Value,
    graph: Value,
    available: i64,
    committed: i64,
    realized_pnl: i64,
    settled: i64,
    wins: i64,
    firm_income: i64,
    family_id: Option<Uuid>,
    clone_index: Option<i32>,
    multiplication_id: Option<Uuid>,
    clone_variance: Option<Value>,
    multiplication_seed: Option<i64>,
    multiplication_variance_bps: Option<i32>,
    multiplication_source: Option<Uuid>,
    multiplication_source_version: Option<i32>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<AgentRow> for Agent {
    type Error = ApiError;

    fn try_from(r: AgentRow) -> Result<Self, ApiError> {
        let money = |units: i64, what: &str| {
            Money::from_minor(units)
                .ok_or_else(|| ApiError::internal(format!("negative {what} balance for {}", r.id)))
        };
        let clone = match (
            r.clone_index,
            r.multiplication_id,
            r.multiplication_source,
            r.multiplication_source_version,
            r.multiplication_variance_bps,
            r.multiplication_seed,
        ) {
            (
                Some(index),
                Some(multiplication_id),
                Some(source),
                Some(version),
                Some(v),
                Some(seed),
            ) => Some(CloneInfo {
                index,
                multiplication_id,
                source_agent_id: source,
                source_version: version,
                variance_bps: v,
                seed,
                params: r
                    .clone_variance
                    .as_ref()
                    .and_then(|c| c.get("params").cloned())
                    .unwrap_or(Value::Null),
            }),
            _ => None,
        };
        Ok(Agent {
            id: r.id,
            family_id: r.family_id,
            clone,
            status: match r.status.as_str() {
                "active" => AgentStatus::Active,
                "paused" => AgentStatus::Paused,
                other => return Err(ApiError::internal(format!("unknown status {other}"))),
            },
            env: r
                .env
                .parse()
                .map_err(|()| ApiError::internal("unknown env"))?,
            balances: Balances {
                available: money(r.available, "available")?,
                committed: money(r.committed, "committed")?,
            },
            stats: Stats {
                realized_pnl: SignedMoney::from_minor(r.realized_pnl),
                settled: r.settled,
                wins: r.wins,
                firm_income: money(r.firm_income, "firm income")?,
            },
            name: r.name,
            description: r.description,
            wallet_address: r.wallet_address,
            vault_address: r.vault_address,
            vault_owner: r.vault_owner,
            version: r.current_version,
            config: r.config,
            policy: r.policy,
            graph: r.graph,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

/// Agents with balances and stats computed from the ledger (never stored).
/// Realized P&L is the agent's net over `settle` posting sets, so allocations
/// and refunds are excluded; a win is a settle set with a positive net. A
/// firm lead's realized P&L is net of what it paid members (task fees and
/// profit shares); what a member receives is `firm_income`, never P&L.
const AGENTS_SQL: &str = "
SELECT a.id, a.name, a.description, a.status, a.env, a.wallet_address, a.vault_address,
       a.vault_owner, a.current_version,
       v.config, v.policy, v.graph, a.created_at, a.updated_at,
       COALESCE(b.available, 0)::BIGINT AS available,
       COALESCE(b.committed, 0)::BIGINT AS committed,
       (COALESCE(s.realized_pnl, 0) + COALESCE(f.paid, 0))::BIGINT AS realized_pnl,
       COALESCE(s.settled, 0)::BIGINT AS settled,
       COALESCE(s.wins, 0)::BIGINT AS wins,
       COALESCE(f.income, 0)::BIGINT AS firm_income,
       COALESCE(c.family_id,
                CASE WHEN EXISTS (SELECT 1 FROM agent_clones x WHERE x.family_id = a.id)
                     THEN a.id END) AS family_id,
       c.clone_index, c.multiplication_id, c.variance AS clone_variance,
       m.seed AS multiplication_seed, m.variance_bps AS multiplication_variance_bps,
       m.source_agent_id AS multiplication_source,
       m.source_version AS multiplication_source_version
FROM agents a
JOIN agent_versions v ON v.agent_id = a.id AND v.version = a.current_version
LEFT JOIN agent_clones c ON c.agent_id = a.id
LEFT JOIN agent_multiplications m ON m.id = c.multiplication_id
LEFT JOIN LATERAL (
    SELECT SUM(e.delta) FILTER (WHERE e.account = 'agent:' || a.id || ':available') AS available,
           SUM(e.delta) FILTER (WHERE e.account = 'agent:' || a.id || ':committed') AS committed
    FROM ledger_entries e
    WHERE e.agent_id = a.id AND e.env = a.env
) b ON TRUE
LEFT JOIN LATERAL (
    SELECT SUM(x.net) AS realized_pnl, COUNT(*) AS settled, COUNT(*) FILTER (WHERE x.net > 0) AS wins
    FROM (
        SELECT SUM(e.delta) AS net
        FROM ledger_entries e
        WHERE e.agent_id = a.id AND e.env = a.env AND e.reason = 'settle'
        GROUP BY e.idempotency_key
    ) x
) s ON TRUE
LEFT JOIN LATERAL (
    SELECT SUM(e.delta) FILTER (WHERE e.delta > 0) AS income,
           SUM(e.delta) FILTER (WHERE e.delta < 0) AS paid
    FROM ledger_entries e
    WHERE e.agent_id = a.id AND e.env = a.env AND e.reason IN ('firm_task_fee', 'firm_payout')
) f ON TRUE
WHERE ($1::uuid IS NULL OR a.id = $1)
ORDER BY a.created_at, a.id";

pub(crate) async fn load<'e>(
    ex: impl PgExecutor<'e>,
    id: Option<Uuid>,
) -> Result<Vec<Agent>, ApiError> {
    let rows: Vec<AgentRow> = sqlx::query_as(AGENTS_SQL).bind(id).fetch_all(ex).await?;
    rows.into_iter().map(Agent::try_from).collect()
}

async fn load_one<'e>(ex: impl PgExecutor<'e>, id: Uuid) -> Result<Agent, ApiError> {
    load(ex, Some(id))
        .await?
        .pop()
        .ok_or_else(|| ApiError::not_found("agent"))
}

pub async fn record_activity(
    conn: &mut PgConnection,
    agent_id: Option<Uuid>,
    kind: &str,
    summary: String,
    detail: Option<Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO activity (agent_id, kind, summary, detail) VALUES ($1, $2, $3, $4)")
        .bind(agent_id)
        .bind(kind)
        .bind(summary)
        .bind(detail)
        .execute(conn)
        .await?;
    Ok(())
}

fn to_json<T: serde::Serialize>(value: &T) -> Result<Value, ApiError> {
    serde_json::to_value(value).map_err(ApiError::internal)
}

async fn insert_version(
    conn: &mut PgConnection,
    agent_id: Uuid,
    version: i32,
    input: &AgentInput,
) -> Result<(), ApiError> {
    sqlx::query(
        "INSERT INTO agent_versions (agent_id, version, config, policy, graph) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(agent_id)
    .bind(version)
    .bind(to_json(&input.config)?)
    .bind(to_json(&input.policy)?)
    .bind(to_json(&input.graph)?)
    .execute(conn)
    .await?;
    Ok(())
}

fn validated(input: AgentInput) -> Result<AgentInput, ApiError> {
    let details = input.validate();
    if details.is_empty() {
        Ok(input)
    } else {
        Err(ApiError::validation(details))
    }
}

pub async fn list(State(pool): State<PgPool>) -> Result<Json<Vec<Agent>>, ApiError> {
    Ok(Json(load(&pool, None).await?))
}

pub async fn get(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Agent>, ApiError> {
    Ok(Json(load_one(&pool, id).await?))
}

pub async fn create(
    State(state): State<AppState>,
    ApiJson(input): ApiJson<AgentInput>,
) -> Result<(StatusCode, Json<Agent>), ApiError> {
    let pool = &state.pool;
    let id = Uuid::new_v4();
    let input = validated(input)?;
    // Before the transaction: the dry run holds no connection.
    custom_code::check_on_save(state.sandbox.as_ref(), &input, id, &state.settings).await?;
    let mut tx = pool.begin().await?;
    insert_agent(&mut tx, id, input).await?;
    tx.commit().await?;

    Ok((StatusCode::CREATED, Json(load_one(pool, id).await?)))
}

/// Validates `input` and writes a new agent (version 1) with the given id.
pub(crate) async fn insert_agent(
    conn: &mut PgConnection,
    id: Uuid,
    input: AgentInput,
) -> Result<(), ApiError> {
    let input = validated(input)?;
    let name = input.name.trim();
    // M1: agents hold simulated funds only and have no onchain wallet yet.
    sqlx::query(
        "INSERT INTO agents (id, name, description, status, env, current_version) \
         VALUES ($1, $2, $3, 'active', $4, 1)",
    )
    .bind(id)
    .bind(name)
    .bind(&input.description)
    .bind(Env::Simulated.as_str())
    .execute(&mut *conn)
    .await?;
    insert_version(conn, id, 1, &input).await?;
    record_activity(
        conn,
        Some(id),
        "agent_created",
        format!("Agent \"{name}\" was created"),
        Some(json!({ "version": 1 })),
    )
    .await?;
    Ok(())
}

pub async fn update(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(input): ApiJson<AgentInput>,
) -> Result<Json<Agent>, ApiError> {
    let pool = &state.pool;
    let input = validated(input)?;
    custom_code::check_on_save(state.sandbox.as_ref(), &input, id, &state.settings).await?;
    let mut tx = pool.begin().await?;
    apply_update(&mut tx, id, input).await?;
    tx.commit().await?;

    Ok(Json(load_one(pool, id).await?))
}

/// The one way an agent's configuration changes: validates `input`, writes a
/// new immutable version and records it. The owner's edit and an accepted
/// co-captain suggestion both go through here. Returns the new version.
pub(crate) async fn apply_update(
    conn: &mut PgConnection,
    id: Uuid,
    input: AgentInput,
) -> Result<i32, ApiError> {
    let input = validated(input)?;
    let name = input.name.trim();

    // The row lock serializes concurrent edits so versions never collide.
    let current: Option<i32> =
        sqlx::query_scalar("SELECT current_version FROM agents WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    let version = current.ok_or_else(|| ApiError::not_found("agent"))? + 1;
    insert_version(conn, id, version, &input).await?;
    sqlx::query(
        "UPDATE agents SET name = $2, description = $3, current_version = $4, updated_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .bind(name)
    .bind(&input.description)
    .bind(version)
    .execute(&mut *conn)
    .await?;
    record_activity(
        conn,
        Some(id),
        "agent_updated",
        format!("Agent \"{name}\" configuration changed (version {version})"),
        Some(json!({ "version": version })),
    )
    .await?;
    Ok(version)
}

/// The agent's current version as an editable input, with the agent row
/// locked for the rest of the transaction.
pub(crate) async fn current_input(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<Option<AgentInput>, ApiError> {
    let row: Option<(String, String, Value, Value, Value)> = sqlx::query_as(
        "SELECT a.name, a.description, v.config, v.policy, v.graph FROM agents a \
         JOIN agent_versions v ON v.agent_id = a.id AND v.version = a.current_version \
         WHERE a.id = $1 FOR UPDATE OF a",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    let Some((name, description, config, policy, graph)) = row else {
        return Ok(None);
    };
    let bad = |what: &str| ApiError::internal(format!("agent {id}: stored {what} is unreadable"));
    Ok(Some(AgentInput {
        name,
        description,
        config: serde_json::from_value(config).map_err(|_| bad("config"))?,
        policy: serde_json::from_value(policy).map_err(|_| bad("policy"))?,
        graph: serde_json::from_value(graph).map_err(|_| bad("graph"))?,
    }))
}

pub async fn set_status(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<StatusBody>,
) -> Result<Json<Agent>, ApiError> {
    let (status, kind, verb) = match body.status {
        AgentStatus::Active => ("active", "agent_resumed", "resumed"),
        AgentStatus::Paused => ("paused", "agent_paused", "paused"),
    };
    let mut tx = pool.begin().await?;
    // Only a real change is written and recorded.
    let changed: Option<String> = sqlx::query_scalar(
        "UPDATE agents SET status = $2, updated_at = now() \
         WHERE id = $1 AND status <> $2 RETURNING name",
    )
    .bind(id)
    .bind(status)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(name) = changed {
        record_activity(
            &mut tx,
            Some(id),
            kind,
            format!("Agent \"{name}\" was {verb}"),
            None,
        )
        .await?;
    }
    tx.commit().await?;

    Ok(Json(load_one(&pool, id).await?))
}

async fn ensure_exists(pool: &PgPool, id: Uuid) -> Result<(), ApiError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM agents WHERE id = $1)")
        .bind(id)
        .fetch_one(pool)
        .await?;
    if exists {
        Ok(())
    } else {
        Err(ApiError::not_found("agent"))
    }
}

pub async fn versions(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Vec<AgentVersion>>, ApiError> {
    ensure_exists(&pool, id).await?;
    let rows: Vec<AgentVersion> = sqlx::query_as(
        "SELECT version, created_at, config, policy, graph FROM agent_versions \
         WHERE agent_id = $1 ORDER BY version DESC",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(rows))
}

pub async fn ledger_entries(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Vec<LedgerEntry>>, ApiError> {
    ensure_exists(&pool, id).await?;
    Ok(Json(ledger_db::agent_entries(&pool, id).await?))
}

type Mover = fn(Env, Uuid, Money, &str) -> Result<PostingSet, LedgerError>;

/// Shared by allocate and deallocate: one transaction holds the balance
/// check, the ledger write and the activity record.
async fn move_funds(
    pool: &PgPool,
    id: Uuid,
    amount: Money,
    headers: &HeaderMap,
    build: Mover,
    kind: &str,
    verb: &str,
) -> Result<Json<Agent>, ApiError> {
    let key = ledger_db::idempotency_key(headers)?;
    let mut tx = pool.begin().await?;
    let agent: Option<(String, String)> =
        sqlx::query_as("SELECT name, env FROM agents WHERE id = $1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let (name, env) = agent.ok_or_else(|| ApiError::not_found("agent"))?;
    if env != Env::Simulated.as_str() {
        return Err(ApiError::unprocessable(
            "unsupported_env",
            "only simulated funds can be moved through this endpoint",
        ));
    }
    let set = build(Env::Simulated, id, amount, &key)?;
    if ledger_db::post(&mut tx, &set).await? == PostOutcome::Posted {
        record_activity(
            &mut tx,
            Some(id),
            kind,
            format!("{} (simulated) {verb} \"{name}\"", display_usdc(amount)),
            Some(json!({ "amount": amount, "env": Env::Simulated })),
        )
        .await?;
    }
    tx.commit().await?;

    Ok(Json(load_one(pool, id).await?))
}

pub async fn allocate(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<AmountBody>,
) -> Result<Json<Agent>, ApiError> {
    move_funds(
        &pool,
        id,
        body.amount,
        &headers,
        ledger::allocate,
        "funds_allocated",
        "allocated to",
    )
    .await
}

pub async fn deallocate(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<AmountBody>,
) -> Result<Json<Agent>, ApiError> {
    move_funds(
        &pool,
        id,
        body.amount,
        &headers,
        ledger::deallocate,
        "funds_deallocated",
        "returned from",
    )
    .await
}
