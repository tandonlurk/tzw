//! Local forum, authored by the owner (optionally attributed to an agent).
//! Posts never create challenges, approvals, configuration edits, or payments.
use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    error::{ApiError, Detail},
    extract::{ApiJson, ApiPath},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Discussion,
    Opportunity,
    Research,
}
impl Category {
    fn as_str(self) -> &'static str {
        match self {
            Self::Discussion => "discussion",
            Self::Opportunity => "opportunity",
            Self::Research => "research",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostInput {
    pub title: String,
    pub body: String,
    pub category: Category,
    pub agent_id: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplyInput {
    pub body: String,
    pub agent_id: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Serialize, FromRow)]
pub struct Post {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub category: String,
    pub agent_id: Option<Uuid>,
    pub agent_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_demo: bool,
    #[sqlx(skip)]
    pub replies: Vec<Reply>,
}
#[derive(Serialize, FromRow)]
pub struct Reply {
    pub id: Uuid,
    pub body: String,
    pub agent_id: Option<Uuid>,
    pub agent_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_demo: bool,
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
async fn author(conn: &mut PgConnection, agent: Option<Uuid>) -> Result<Option<String>, ApiError> {
    match agent {
        None => Ok(None),
        Some(id) => Ok(Some(
            sqlx::query_scalar::<_, String>("SELECT name FROM agents WHERE id=$1")
                .bind(id)
                .fetch_optional(conn)
                .await?
                .ok_or_else(|| ApiError::not_found("agent"))?,
        )),
    }
}
async fn load(pool: &PgPool, id: Uuid) -> Result<Post, ApiError> {
    let mut p=sqlx::query_as::<_,Post>("SELECT id,title,body,category,agent_id,agent_name,created_at,is_demo FROM forum_posts WHERE id=$1")
        .bind(id).fetch_optional(pool).await?.ok_or_else(||ApiError::not_found("forum post"))?;
    p.replies=sqlx::query_as::<_,Reply>("SELECT id,body,agent_id,agent_name,created_at,is_demo FROM forum_replies WHERE post_id=$1 ORDER BY created_at,id")
        .bind(id).fetch_all(pool).await?;
    Ok(p)
}
pub async fn list(State(pool): State<PgPool>) -> Result<Json<Vec<Post>>, ApiError> {
    let ids = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM forum_posts ORDER BY created_at DESC,id DESC LIMIT 50",
    )
    .fetch_all(&pool)
    .await?;
    let mut posts = Vec::with_capacity(ids.len());
    for id in ids {
        posts.push(load(&pool, id).await?);
    }
    Ok(Json(posts))
}
pub async fn get(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Post>, ApiError> {
    Ok(Json(load(&pool, id).await?))
}
pub async fn create(
    State(pool): State<PgPool>,
    ApiJson(input): ApiJson<PostInput>,
) -> Result<Json<Post>, ApiError> {
    let title = text(&input.title, "title", 160)?;
    let body = text(&input.body, "body", 4000)?;
    let mut tx = pool.begin().await?;
    let name = author(&mut tx, input.agent_id).await?;
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO forum_posts(id,title,body,category,agent_id,agent_name) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(id).bind(title).bind(body).bind(input.category.as_str()).bind(input.agent_id).bind(name).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(load(&pool, id).await?))
}
pub async fn reply(
    State(pool): State<PgPool>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(input): ApiJson<ReplyInput>,
) -> Result<Json<Post>, ApiError> {
    let body = text(&input.body, "body", 2000)?;
    let mut tx = pool.begin().await?;
    if sqlx::query_scalar::<_, Uuid>("SELECT id FROM forum_posts WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .is_none()
    {
        return Err(ApiError::not_found("forum post"));
    }
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM forum_replies WHERE post_id=$1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= 100 {
        return Err(ApiError::conflict(
            "reply_limit",
            "a thread is limited to 100 replies",
        ));
    }
    let name = author(&mut tx, input.agent_id).await?;
    sqlx::query(
        "INSERT INTO forum_replies(id,post_id,body,agent_id,agent_name) VALUES($1,$2,$3,$4,$5)",
    )
    .bind(Uuid::new_v4())
    .bind(id)
    .bind(body)
    .bind(input.agent_id)
    .bind(name)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(load(&pool, id).await?))
}

/// Fictional examples, isolated from owner-authored content. Fixed IDs make repeat calls safe.
pub async fn seed(
    State(pool): State<PgPool>,
    ApiJson(Empty {}): ApiJson<Empty>,
) -> Result<Json<Vec<Post>>, ApiError> {
    let examples = [
        (
            "A second signal before chasing BTC momentum",
            "Fictional demo discussion: a short rally alone may not justify a challenge. What independent observation would change the thesis?",
            "discussion",
            "Fictional demo reply: define the horizon and a falsifiable threshold before considering a stake.",
        ),
        (
            "Seeking an analyst for an energy-market research firm",
            "Fictional demo opportunity: a lead is looking for a research analyst to compare WTI and natural-gas event terms. No hiring or payment has occurred.",
            "opportunity",
            "Fictional demo reply: agree on the deliverable, source freshness, and compensation before joining.",
        ),
        (
            "Resolution checklist: price observation is not an oracle",
            "Fictional demo research: record the market source, exact comparison, deadline, and missing-data refund rule before accepting a challenge.",
            "research",
            "Fictional demo reply: separate discovery data from the authoritative resolution source.",
        ),
        (
            "How should a cautious strategy use its available capital?",
            "Fictional demo discussion: compare idle funds, open exposure, and maximum per-challenge stake. A wallet transfer is not a profit.",
            "discussion",
            "Fictional demo reply: preserve an available reserve rather than committing the whole allocation.",
        ),
        (
            "Research desk: complementary roles beat duplicate work",
            "Fictional demo opportunity: a scout discovers an event, an analyst checks evidence, and the lead decides whether to propose a challenge. This is an example team, not a real solicitation.",
            "opportunity",
            "Fictional demo reply: make role responsibilities and the payout split explicit in the firm agreement.",
        ),
        (
            "Strategy replicas need independent counterparties",
            "Fictional demo research: multiplier clones are separate funded agents. Members of the same family do not challenge one another, preventing a family from manufacturing its own results.",
            "research",
            "Fictional demo reply: evaluate total family exposure as well as each agent's individual limits.",
        ),
    ];
    let mut tx = pool.begin().await?;
    for (i, (title, body, category, reply)) in examples.iter().enumerate() {
        let post_id = Uuid::from_u128(0x00000000_0000_4000_8000_00000000f100 + i as u128);
        let reply_id = Uuid::from_u128(0x00000000_0000_4000_8000_00000000f200 + i as u128);
        sqlx::query("INSERT INTO forum_posts(id,title,body,category,is_demo,created_at) VALUES($1,$2,$3,$4,true,now()-($5*interval '1 hour')) ON CONFLICT(id) DO NOTHING")
            .bind(post_id).bind(title).bind(body).bind(category).bind((i+1) as i32).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO forum_replies(id,post_id,body,is_demo,created_at) VALUES($1,$2,$3,true,now()-($4*interval '1 hour')+interval '5 minutes') ON CONFLICT(id) DO NOTHING")
            .bind(reply_id).bind(post_id).bind(reply).bind((i+1) as i32).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    list(State(pool)).await
}
