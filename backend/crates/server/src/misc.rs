//! Health, templates, graph catalog / validation, wallet and activity.

use aen_core::graph::{self, BlockDef, Graph, GraphError};
use aen_core::ledger::{self, Account, Env};
use aen_core::money::Money;
use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::agents::record_activity;
use crate::error::ApiError;
use crate::extract::{ApiJson, ApiQuery};
use crate::ledger_db::{self, PostOutcome};
use crate::models::{ActivityItem, AmountBody, Template, Wallet, display_usdc};

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn not_found() -> ApiError {
    ApiError::not_found("route")
}

pub async fn templates(State(pool): State<PgPool>) -> Result<Json<Vec<Template>>, ApiError> {
    let rows: Vec<Template> = sqlx::query_as(
        "SELECT id, name, description, config, policy, graph FROM templates ORDER BY position",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(rows))
}

pub async fn blocks() -> Json<&'static [BlockDef]> {
    Json(graph::catalog())
}

#[derive(serde::Serialize)]
pub struct ValidateResponse {
    valid: bool,
    errors: Vec<GraphError>,
}

pub async fn validate_graph(ApiJson(graph): ApiJson<Graph>) -> Json<ValidateResponse> {
    let errors = graph::validate(&graph);
    Json(ValidateResponse {
        valid: errors.is_empty(),
        errors,
    })
}

/// User-level overview of simulated funds (the only env with funds in M1).
async fn load_wallet(pool: &PgPool) -> Result<Wallet, ApiError> {
    let env = Env::Simulated;
    let (unallocated, allocated, committed): (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(delta) FILTER (WHERE account = 'user:unallocated'), 0)::BIGINT, \
                COALESCE(SUM(delta) FILTER (WHERE account LIKE 'agent:%'), 0)::BIGINT, \
                COALESCE(SUM(delta) FILTER (WHERE account LIKE 'agent:%:committed'), 0)::BIGINT \
         FROM ledger_entries WHERE env = $1",
    )
    .bind(env.as_str())
    .fetch_one(pool)
    .await?;
    let money = |units: i64| {
        Money::from_minor(units).ok_or_else(|| ApiError::internal("negative wallet balance"))
    };
    Ok(Wallet {
        env,
        unallocated: money(unallocated)?,
        allocated: money(allocated)?,
        committed: money(committed)?,
        entries: ledger_db::account_entries(pool, env, Account::UserUnallocated).await?,
    })
}

pub async fn wallet(State(pool): State<PgPool>) -> Result<Json<Wallet>, ApiError> {
    Ok(Json(load_wallet(&pool).await?))
}

pub async fn simulated_deposit(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<AmountBody>,
) -> Result<Json<Wallet>, ApiError> {
    let key = ledger_db::idempotency_key(&headers)?;
    let set = ledger::deposit(Env::Simulated, body.amount, &key)?;
    let mut tx = pool.begin().await?;
    if ledger_db::post(&mut tx, &set).await? == PostOutcome::Posted {
        record_activity(
            &mut tx,
            None,
            "simulated_deposit",
            format!(
                "{} of simulated funds added to the wallet",
                display_usdc(body.amount)
            ),
            Some(json!({ "amount": body.amount, "env": Env::Simulated })),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(load_wallet(&pool).await?))
}

const DEFAULT_ACTIVITY_LIMIT: i64 = 50;
const MAX_ACTIVITY_LIMIT: i64 = 200;

// Strings, so that an empty `agent_id=` means "no filter" instead of an error.
#[derive(Deserialize)]
pub struct ActivityParams {
    agent_id: Option<String>,
    interaction_id: Option<String>,
    collaboration_id: Option<String>,
    firm_id: Option<String>,
    limit: Option<String>,
}

pub async fn activity(
    State(pool): State<PgPool>,
    ApiQuery(params): ApiQuery<ActivityParams>,
) -> Result<Json<Vec<ActivityItem>>, ApiError> {
    let agent_id = match params.agent_id.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => {
            Some(Uuid::parse_str(s).map_err(|_| ApiError::bad_request("agent_id must be a UUID"))?)
        }
        None => None,
    };
    let interaction_id = match params.interaction_id.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => Some(
            Uuid::parse_str(s)
                .map_err(|_| ApiError::bad_request("interaction_id must be a UUID"))?,
        ),
        None => None,
    };
    let collaboration_id = match params.collaboration_id.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => Some(
            Uuid::parse_str(s)
                .map_err(|_| ApiError::bad_request("collaboration_id must be a UUID"))?,
        ),
        None => None,
    };
    let firm_id = match params.firm_id.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => {
            Some(Uuid::parse_str(s).map_err(|_| ApiError::bad_request("firm_id must be a UUID"))?)
        }
        None => None,
    };
    let limit = match params.limit.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => s
            .parse::<i64>()
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| ApiError::bad_request("limit must be a positive integer"))?
            .min(MAX_ACTIVITY_LIMIT),
        None => DEFAULT_ACTIVITY_LIMIT,
    };
    let rows: Vec<ActivityItem> = sqlx::query_as(
        "SELECT id, ts, agent_id, interaction_id, collaboration_id, firm_id, kind, summary, detail \
         FROM activity \
         WHERE ($1::uuid IS NULL OR agent_id = $1) \
           AND ($3::uuid IS NULL OR interaction_id = $3) \
           AND ($4::uuid IS NULL OR collaboration_id = $4) \
           AND ($5::uuid IS NULL OR firm_id = $5) ORDER BY seq DESC LIMIT $2",
    )
    .bind(agent_id)
    .bind(limit)
    .bind(interaction_id)
    .bind(collaboration_id)
    .bind(firm_id)
    .fetch_all(&pool)
    .await?;
    Ok(Json(rows))
}
