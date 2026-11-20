//! Local forum persistence, provenance and financial isolation.
mod common;
use aen_server::{DEFAULT_CORS_ORIGIN, MIGRATOR, app_with_state, guard::ApiToken, state::AppState};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{Mutex, MutexGuard};
use tower::ServiceExt;

static LOCK: Mutex<()> = Mutex::const_new(());
const OWNER: &str = "forum-owner-token-000000000000000000000000000";
const BOT: &str = "forum-bot-token-00000000000000000000000000000";
struct Ctx {
    app: Router,
    pool: PgPool,
    _guard: MutexGuard<'static, ()>,
}
async fn send(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let mut r = Request::builder().method(method).uri(path);
    if let Some(t) = token {
        r = r.header("authorization", format!("Bearer {t}"));
    }
    let b = if let Some(v) = body {
        r = r.header("content-type", "application/json");
        Body::from(v.to_string())
    } else {
        Body::empty()
    };
    let response = app.clone().oneshot(r.body(b).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}
async fn setup() -> Ctx {
    let guard = LOCK.lock().await;
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        format!(
            "postgres://{}@localhost/aen_test",
            std::env::var("USER").unwrap()
        )
    });
    let pool = PgPool::connect(&url).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    common::wipe(&pool).await;
    let mut state = AppState::offline(pool.clone());
    state.settings.guard.api_token = ApiToken::new(OWNER);
    state.settings.sparring_bot_token = ApiToken::new(BOT);
    Ctx {
        app: app_with_state(state, DEFAULT_CORS_ORIGIN.parse().unwrap()),
        pool,
        _guard: guard,
    }
}
fn post() -> Value {
    json!({"title":"Test thesis","body":"A counterargument worth discussing.","category":"discussion","agent_id":null})
}
#[tokio::test]
async fn forum_persists_owner_and_agent_attribution_without_financial_effects() {
    let c = setup().await;
    let (s, p) = send(
        &c.app,
        "POST",
        "/api/forum/posts",
        Some(post()),
        Some(OWNER),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["is_demo"], false);
    assert!(p["agent_name"].is_null());
    let id = p["id"].as_str().unwrap();
    let (_, templates) = send(&c.app, "GET", "/api/templates", None, Some(OWNER)).await;
    let t = &templates[0];
    let (s,a)=send(&c.app,"POST","/api/agents",Some(json!({"name":"Forum analyst","description":"Owner-directed forum attribution","config":t["config"],"policy":t["policy"],"graph":t["graph"]})),Some(OWNER)).await;
    assert_eq!(s, StatusCode::CREATED, "{a}");
    let path = format!("/api/forum/posts/{id}/replies");
    let (s, p) = send(
        &c.app,
        "POST",
        &path,
        Some(json!({"body":"An owner-assisted analyst reply.","agent_id":a["id"]})),
        Some(OWNER),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["replies"][0]["agent_name"], "Forum analyst");
    assert_eq!(p["replies"][0]["is_demo"], false);
    let (_, saved) = send(
        &c.app,
        "GET",
        &format!("/api/forum/posts/{id}"),
        None,
        Some(OWNER),
    )
    .await;
    assert_eq!(saved, p);
    let (_, listed) = send(&c.app, "GET", "/api/forum/posts", None, Some(OWNER)).await;
    assert_eq!(listed[0], p);
    let n:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM ledger_entries)+(SELECT COUNT(*) FROM interactions)+(SELECT COUNT(*) FROM approvals)").fetch_one(&c.pool).await.unwrap();
    assert_eq!(n, 0);
}
#[tokio::test]
async fn forum_seed_is_fictional_idempotent_and_does_not_replace_owner_content() {
    let c = setup().await;
    send(
        &c.app,
        "POST",
        "/api/forum/posts",
        Some(post()),
        Some(OWNER),
    )
    .await;
    for _ in 0..2 {
        let (s, posts) = send(
            &c.app,
            "POST",
            "/api/dev/seed-forum",
            Some(json!({})),
            Some(OWNER),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{posts}");
        let list = posts.as_array().unwrap();
        assert_eq!(list.len(), 7);
        let examples: Vec<_> = list.iter().filter(|p| p["is_demo"] == true).collect();
        assert_eq!(examples.len(), 6);
        for p in examples {
            assert!(p["body"].as_str().unwrap().starts_with("Fictional demo"));
            assert_eq!(p["replies"].as_array().unwrap().len(), 1);
            assert_eq!(p["replies"][0]["is_demo"], true);
        }
    }
}
#[tokio::test]
async fn forum_requires_owner_auth_and_valid_bounded_content() {
    let c = setup().await;
    assert_eq!(
        send(&c.app, "GET", "/api/forum/posts", None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&c.app, "POST", "/api/forum/posts", Some(post()), Some(BOT))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    for (field, value) in [
        ("title", json!(" ")),
        ("title", json!("x".repeat(161))),
        ("body", json!("x".repeat(4001))),
        ("category", json!("financial_authorization")),
        ("is_demo", json!(true)),
    ] {
        let mut b = post();
        b[field] = value;
        assert_eq!(
            send(&c.app, "POST", "/api/forum/posts", Some(b), Some(OWNER))
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let mut b = post();
    b["agent_id"] = json!("00000000-0000-4000-8000-00000000ffff");
    assert_eq!(
        send(&c.app, "POST", "/api/forum/posts", Some(b), Some(OWNER))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, p) = send(
        &c.app,
        "POST",
        "/api/forum/posts",
        Some(post()),
        Some(OWNER),
    )
    .await;
    let path = format!("/api/forum/posts/{}/replies", p["id"].as_str().unwrap());
    assert_eq!(
        send(
            &c.app,
            "POST",
            &path,
            Some(json!({"body":"x".repeat(2001)})),
            Some(OWNER)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        send(
            &c.app,
            "POST",
            "/api/forum/posts/00000000-0000-4000-8000-00000000ffff/replies",
            Some(json!({"body":"reply"})),
            Some(OWNER)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}
