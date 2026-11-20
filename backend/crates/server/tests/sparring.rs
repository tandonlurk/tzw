//! External Grok Bot capability boundary and practice-history persistence.
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
const OWNER: &str = "owner-token-00000000000000000000000000000000";
const BOT: &str = "bot-token-000000000000000000000000000000000";
struct Ctx {
    pool: PgPool,
    state: AppState,
    app: Router,
    agent: String,
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
fn router(state: AppState) -> Router {
    app_with_state(state, DEFAULT_CORS_ORIGIN.parse().unwrap())
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
    let open = router(state.clone());
    let (_, t) = send(&open, "GET", "/api/templates", None, None).await;
    let t = &t[0];
    let (status,a)=send(&open,"POST","/api/agents",Some(json!({"name":"Practice subject","description":"test","config":t["config"],"policy":t["policy"],"graph":t["graph"]})),None).await;
    assert!(status.is_success(), "{a}");
    state.settings.guard.api_token = ApiToken::new(OWNER);
    state.settings.sparring_bot_token = ApiToken::new(BOT);
    Ctx {
        app: router(state.clone()),
        state,
        pool,
        agent: a["id"].as_str().unwrap().to_owned(),
        _guard: guard,
    }
}
async fn session(c: &Ctx) -> Value {
    let (s, v) = send(
        &c.app,
        "POST",
        &format!("/api/agents/{}/sparring", c.agent),
        Some(json!({"thesis":"A short rally is enough evidence to enter a challenge."})),
        Some(OWNER),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v
}
fn argument() -> Value {
    json!({"argument":"One short rally does not establish predictive accuracy.","evidence_urls":["https://example.com/evidence"],"proposed_revision":"Require an independent signal before proposing."})
}
#[tokio::test]
async fn practice_rounds_persist_snapshot_and_never_move_funds_or_edit_strategy() {
    let c = setup().await;
    let s = session(&c).await;
    let id = s["id"].as_str().unwrap();
    assert_eq!(s["agent_version"], 1);
    assert!(s["rounds"].as_array().unwrap().is_empty());
    assert_eq!(s["agent_snapshot"].as_object().unwrap().len(), 4);
    assert!(s["agent_snapshot"].get("policy").is_none());
    assert!(s["agent_snapshot"].get("wallet_address").is_none());
    let (_, bot_read) = send(
        &c.app,
        "GET",
        &format!("/api/bot/sparring/{id}"),
        None,
        Some(BOT),
    )
    .await;
    assert_eq!(bot_read, s);
    let (status, rounds) = send(
        &c.app,
        "POST",
        &format!("/api/bot/sparring/{id}/rounds"),
        Some(argument()),
        Some(BOT),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rounds}");
    assert_eq!(rounds["rounds"][0]["source"], "grok_bot");
    assert_eq!(rounds["rounds"][0]["role"], "adversary");
    let (status,rounds)=send(&c.app,"POST",&format!("/api/sparring/{id}/rounds"),Some(json!({"role":"owner","argument":"I will test a second signal before revising the strategy.","evidence_urls":[],"proposed_revision":null})),Some(OWNER)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rounds["rounds"][1]["source"], "owner");
    let (_, listed) = send(
        &c.app,
        "GET",
        &format!("/api/agents/{}/sparring", c.agent),
        None,
        Some(OWNER),
    )
    .await;
    assert_eq!(listed[0], rounds);
    let n:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM ledger_entries)+(SELECT COUNT(*) FROM interactions)+(SELECT COUNT(*) FROM approvals)").fetch_one(&c.pool).await.unwrap();
    assert_eq!(n, 0);
    let version: i32 = sqlx::query_scalar("SELECT current_version FROM agents WHERE id=$1::uuid")
        .bind(&c.agent)
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(version, 1);
    // A later config version cannot change the session's captured preferences.
    let (_, mut a) = send(
        &c.app,
        "GET",
        &format!("/api/agents/{}", c.agent),
        None,
        Some(OWNER),
    )
    .await;
    let original = s["agent_snapshot"]["strategy"].clone();
    a["config"]["strategy"] = json!("New strategy");
    let payload = json!({"name":a["name"],"description":a["description"],"config":a["config"],"policy":a["policy"],"graph":a["graph"]});
    let (status, v) = send(
        &c.app,
        "PUT",
        &format!("/api/agents/{}", c.agent),
        Some(payload),
        Some(OWNER),
    )
    .await;
    assert!(status.is_success(), "{v}");
    let (_, saved) = send(
        &c.app,
        "GET",
        &format!("/api/sparring/{id}"),
        None,
        Some(OWNER),
    )
    .await;
    assert_eq!(saved["agent_snapshot"]["strategy"], original);
    assert_eq!(saved["agent_version"], 1);
    assert!(
        sqlx::query("DELETE FROM sparring_rounds")
            .execute(&c.pool)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn restricted_token_cannot_escape_bot_routes_or_impersonate_owner() {
    let c = setup().await;
    let s = session(&c).await;
    let id = s["id"].as_str().unwrap();
    let path = format!("/api/bot/sparring/{id}");
    for token in [None, Some(OWNER), Some("wrong")] {
        assert_eq!(
            send(&c.app, "GET", &path, None, token).await.0,
            StatusCode::UNAUTHORIZED
        );
    }
    for path in ["/api/agents", "/api/wallet", "/api/approvals"] {
        assert_eq!(
            send(&c.app, "GET", path, None, Some(BOT)).await.0,
            StatusCode::FORBIDDEN
        );
    }
    let mut spoof = argument();
    spoof["role"] = json!("owner");
    assert_eq!(
        send(
            &c.app,
            "POST",
            &format!("{path}/rounds"),
            Some(spoof),
            Some(BOT)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut spoof = argument();
    spoof["role"] = json!("adversary");
    assert_eq!(
        send(
            &c.app,
            "POST",
            &format!("/api/sparring/{id}/rounds"),
            Some(spoof),
            Some(OWNER)
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut state = c.state.clone();
    state.settings.guard.api_token = None;
    state.settings.sparring_bot_token = None;
    assert_eq!(
        send(&router(state.clone()), "GET", &path, None, None)
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    state.settings.sparring_bot_token = ApiToken::new(BOT);
    assert_eq!(
        send(
            &router(state.clone()),
            "GET",
            "/api/wallet",
            None,
            Some(BOT)
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    state.settings.guard.api_token = ApiToken::new(BOT);
    assert_eq!(
        send(&router(state), "GET", &path, None, Some(BOT)).await.0,
        StatusCode::UNAUTHORIZED
    );
}
#[tokio::test]
async fn validation_caps_rounds_and_does_not_fetch_evidence_urls() {
    let c = setup().await;
    let s = session(&c).await;
    let id = s["id"].as_str().unwrap();
    let path = format!("/api/bot/sparring/{id}/rounds");
    for body in [json!({"thesis":" "}), json!({"thesis":"x".repeat(4001)})] {
        assert_eq!(
            send(
                &c.app,
                "POST",
                &format!("/api/agents/{}/sparring", c.agent),
                Some(body),
                Some(OWNER)
            )
            .await
            .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    for url in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "https://user:pass@example.com",
        "not a url",
    ] {
        let mut b = argument();
        b["evidence_urls"] = json!([url]);
        assert_eq!(
            send(&c.app, "POST", &path, Some(b), Some(BOT)).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    for (field, value) in [
        ("argument", json!("x".repeat(6001))),
        ("argument", json!(" ")),
        ("proposed_revision", json!("x".repeat(4001))),
        ("evidence_urls", json!(vec!["https://example.com"; 9])),
    ] {
        let mut b = argument();
        b[field] = value;
        assert_eq!(
            send(&c.app, "POST", &path, Some(b), Some(BOT)).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    // Parallel callers serialized under the session lock; exactly 12 succeed.
    let mut tasks = Vec::new();
    for _ in 0..15 {
        let app = c.app.clone();
        let path = path.clone();
        tasks.push(tokio::spawn(async move {
            send(&app, "POST", &path, Some(argument()), Some(BOT))
                .await
                .0
        }));
    }
    let mut admitted = 0;
    for task in tasks {
        let status = task.await.unwrap();
        if status == StatusCode::OK {
            admitted += 1;
        } else {
            assert_eq!(status, StatusCode::CONFLICT);
        }
    }
    assert_eq!(admitted, 12);
    let (_, v) = send(
        &c.app,
        "GET",
        &format!("/api/sparring/{id}"),
        None,
        Some(OWNER),
    )
    .await;
    assert_eq!(v["rounds"].as_array().unwrap().len(), 12);
}
#[tokio::test]
async fn bot_routes_keep_host_and_origin_protection() {
    let c = setup().await;
    let s = session(&c).await;
    let path = format!("/api/bot/sparring/{}", s["id"].as_str().unwrap());
    for (header, value) in [("host", "evil.example"), ("origin", "https://evil.example")] {
        let request = Request::builder()
            .uri(&path)
            .header("authorization", format!("Bearer {BOT}"))
            .header(header, value)
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            c.app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
    }
}
