//! Grok Bot's practice history. Never creates intents, changes strategy or
//! policy, writes ledger entries, or changes a live interaction.
use crate::{
    error::{ApiError, Detail},
    extract::{ApiJson, ApiPath},
};
use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;
pub const MAX_ROUNDS: i64 = 12;
#[derive(Serialize, FromRow)]
pub struct Session {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub agent_version: i32,
    pub thesis: String,
    pub agent_snapshot: Value,
    pub created_at: DateTime<Utc>,
    #[sqlx(skip)]
    pub rounds: Vec<Round>,
}
#[derive(Serialize, FromRow)]
pub struct Round {
    pub id: Uuid,
    pub role: String,
    pub source: String,
    pub argument: String,
    pub evidence_urls: Value,
    pub proposed_revision: Option<String>,
    pub created_at: DateTime<Utc>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSession {
    pub thesis: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoundInput {
    pub role: String,
    pub argument: String,
    #[serde(default)]
    pub evidence_urls: Vec<String>,
    pub proposed_revision: Option<String>,
}
// No role field: Bot callers cannot impersonate the owner.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BotRoundInput {
    pub argument: String,
    #[serde(default)]
    pub evidence_urls: Vec<String>,
    pub proposed_revision: Option<String>,
}
fn text(value: &str, field: &str, max: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max {
        return Err(ApiError::validation(vec![Detail::new(
            field,
            "out_of_range",
            format!("must contain 1–{max} characters"),
        )]));
    }
    Ok(value.to_owned())
}
fn validate(input: &RoundInput) -> Result<(String, Option<String>), ApiError> {
    let argument = text(&input.argument, "argument", 6000)?;
    let revision = input
        .proposed_revision
        .as_ref()
        .map(|v| text(v, "proposed_revision", 4000))
        .transpose()?;
    if input.evidence_urls.len() > 8
        || input.evidence_urls.iter().any(|s| {
            s.len() > 2048
                || reqwest::Url::parse(s).map_or(true, |u| {
                    !matches!(u.scheme(), "http" | "https")
                        || u.host_str().is_none()
                        || !u.username().is_empty()
                        || u.password().is_some()
                })
        })
    {
        return Err(ApiError::validation(vec![Detail::new(
            "evidence_urls",
            "invalid_urls",
            "at most 8 HTTP(S) URLs, each at most 2048 bytes, without embedded credentials",
        )]));
    }
    Ok((argument, revision))
}
async fn load(pool: &PgPool, id: Uuid) -> Result<Session, ApiError> {
    let mut s=sqlx::query_as::<_,Session>("SELECT id,agent_id,agent_version,thesis,agent_snapshot,created_at FROM sparring_sessions WHERE id=$1")
        .bind(id).fetch_optional(pool).await?.ok_or_else(||ApiError::not_found("sparring session"))?;
    s.rounds=sqlx::query_as::<_,Round>("SELECT id,role,source,argument,evidence_urls,proposed_revision,created_at FROM sparring_rounds WHERE session_id=$1 ORDER BY position")
        .bind(id).fetch_all(pool).await?;
    Ok(s)
}
pub async fn get(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Session>, ApiError> {
    Ok(Json(load(&pool, id).await?))
}
pub async fn list(
    State(pool): State<PgPool>,
    ApiPath(agent): ApiPath<Uuid>,
) -> Result<Json<Vec<Session>>, ApiError> {
    if !sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM agents WHERE id=$1)")
        .bind(agent)
        .fetch_one(&pool)
        .await?
    {
        return Err(ApiError::not_found("agent"));
    }
    let ids=sqlx::query_scalar::<_,Uuid>("SELECT id FROM sparring_sessions WHERE agent_id=$1 ORDER BY created_at DESC,id DESC LIMIT 50")
        .bind(agent).fetch_all(&pool).await?;
    let mut sessions = Vec::with_capacity(ids.len());
    for id in ids {
        sessions.push(load(&pool, id).await?);
    }
    Ok(Json(sessions))
}
pub async fn create(
    State(pool): State<PgPool>,
    ApiPath(agent): ApiPath<Uuid>,
    ApiJson(input): ApiJson<CreateSession>,
) -> Result<Json<Session>, ApiError> {
    let thesis = text(&input.thesis, "thesis", 4000)?;
    let id = Uuid::new_v4();
    // Snapshot and version captured atomically. Explicit whitelist excludes
    // policy, graph, custom code, balances, wallet addresses and all keys.
    let saved=sqlx::query("INSERT INTO sparring_sessions(id,agent_id,agent_version,thesis,agent_snapshot) SELECT $1,a.id,a.current_version,$3,jsonb_build_object('name',a.name,'goals',v.config->'goals','strategy',v.config->'strategy','horizon_secs',v.config->'horizon_secs') FROM agents a JOIN agent_versions v ON v.agent_id=a.id AND v.version=a.current_version WHERE a.id=$2")
        .bind(id).bind(agent).bind(thesis).execute(&pool).await?;
    if saved.rows_affected() == 0 {
        return Err(ApiError::not_found("agent"));
    }
    Ok(Json(load(&pool, id).await?))
}
pub async fn owner_round(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(input): ApiJson<RoundInput>,
) -> Result<Json<Session>, ApiError> {
    if input.role != "owner" {
        return Err(ApiError::unprocessable(
            "invalid_role",
            "owner endpoint accepts only role owner; adversary rounds require the restricted Bot endpoint",
        ));
    }
    append(&pool, id, input, "owner").await
}
pub async fn bot_round(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(input): ApiJson<BotRoundInput>,
) -> Result<Json<Session>, ApiError> {
    append(
        &pool,
        id,
        RoundInput {
            role: "adversary".into(),
            argument: input.argument,
            evidence_urls: input.evidence_urls,
            proposed_revision: input.proposed_revision,
        },
        "grok_bot",
    )
    .await
}
async fn append(
    pool: &PgPool,
    id: Uuid,
    input: RoundInput,
    source: &str,
) -> Result<Json<Session>, ApiError> {
    let (argument, revision) = validate(&input)?;
    let mut tx = pool.begin().await?;
    // Serialize round admission so concurrent requests cannot exceed the cap.
    let exists =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM sparring_sessions WHERE id=$1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    if exists.is_none() {
        return Err(ApiError::not_found("sparring session"));
    }
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sparring_rounds WHERE session_id=$1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
    if count >= MAX_ROUNDS {
        return Err(ApiError::conflict(
            "round_limit",
            "practice session is limited to 12 rounds; start a new session",
        ));
    }
    sqlx::query("INSERT INTO sparring_rounds(id,session_id,position,role,source,argument,evidence_urls,proposed_revision) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(Uuid::new_v4()).bind(id).bind((count+1) as i32).bind(input.role).bind(source).bind(argument)
        .bind(serde_json::json!(input.evidence_urls)).bind(revision).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(load(pool, id).await?))
}
