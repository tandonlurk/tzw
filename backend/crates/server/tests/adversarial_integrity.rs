#![allow(dead_code)]
//! Adversarial API tests: concurrent allocation, idempotency abuse, malformed
//! amounts and headers. Uses its own database (`aen_sec_test` unless
//! `TEST_DATABASE_URL` is set) and truncates it on every test.

use aen_server::{DEFAULT_CORS_ORIGIN, MIGRATOR, app};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{Mutex, MutexGuard};
use tower::ServiceExt;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

struct Ctx {
    app: Router,
    pool: PgPool,
    _guard: MutexGuard<'static, ()>,
}

async fn setup() -> Ctx {
    let guard = DB_LOCK.lock().await;
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| match std::env::var("USER") {
        Ok(user) => format!("postgres://{user}@localhost/aen_sec_test"),
        Err(_) => "postgres://localhost/aen_sec_test".to_owned(),
    });
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(30)
        .connect(&url)
        .await
        .expect("connect to test db");
    MIGRATOR.run(&pool).await.expect("run migrations");
    sqlx::query("TRUNCATE activity, ledger_entries, agent_versions, agents")
        .execute(&pool)
        .await
        .unwrap();
    Ctx {
        app: app(pool.clone(), DEFAULT_CORS_ORIGIN.parse().unwrap()),
        pool,
        _guard: guard,
    }
}

async fn raw(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn send(app: &Router, path: &str, body: Value, key: Option<&str>) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(k) = key {
        req = req.header("Idempotency-Key", k);
    }
    raw(app, req.body(Body::from(body.to_string())).unwrap()).await
}

fn agent_input(name: &str) -> Value {
    json!({
        "name": name, "description": "adversarial",
        "config": {"goals": "g", "strategy": "s", "categories": ["crypto"],
                   "horizon_secs": 3600, "risk_tolerance": "medium", "schedule_secs": 60},
        "policy": {"max_per_interaction": "10000000", "max_aggregate_exposure": "50000000",
                   "approval_threshold": "5000000", "allowed_interaction_types": ["challenge"],
                   "min_conviction_bps": 6000, "stop_loss": null},
        "graph": {"schema_version": 1,
            "nodes": [{"id": "n1", "type": "trigger.schedule", "params": {"every_secs": 60}},
                      {"id": "n2", "type": "reason.evaluate_opportunity", "params": {}},
                      {"id": "n3", "type": "action.request_approval", "params": {}}],
            "edges": [{"from": "n1", "from_port": "out", "to": "n2", "to_port": "in"},
                      {"from": "n2", "from_port": "out", "to": "n3", "to_port": "in"}]}
    })
}

async fn agent(app: &Router, name: &str) -> String {
    let (s, a) = send(app, "/api/agents", agent_input(name), None).await;
    assert_eq!(s, StatusCode::CREATED, "{a}");
    a["id"].as_str().unwrap().to_owned()
}

async fn deposit(app: &Router, units: &str) -> (StatusCode, Value) {
    send(app, "/api/wallet/simulated-deposit", json!({"amount": units}), None).await
}

async fn put(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder().method("PUT").uri(path).header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap();
    raw(app, req).await
}

#[tokio::test]
async fn ledger_and_versions_are_append_only_at_the_database_level() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    deposit(&c.app, "100").await;
    send(&c.app, &format!("/api/agents/{a}/allocate"), json!({"amount": "10"}), None).await;
    for sql in [
        "UPDATE ledger_entries SET delta = delta + 1",
        "DELETE FROM ledger_entries",
        "UPDATE agent_versions SET config = '{}'::jsonb",
        "DELETE FROM agent_versions",
    ] {
        assert!(sqlx::query(sql).execute(&c.pool).await.is_err(), "{sql} must be refused");
    }
    // Direct forged insert that does not balance is not stopped by the DB
    // (balance is enforced in application code only): documented as SEC-08.
}

#[tokio::test]
async fn concurrent_edits_get_distinct_consecutive_versions_and_keep_history() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    let mut tasks = Vec::new();
    for i in 0..12 {
        let app = c.app.clone();
        let a = a.clone();
        tasks.push(tokio::spawn(async move {
            put(&app, &format!("/api/agents/{a}"), agent_input(&format!("edit{i}"))).await.0
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let versions: Vec<i32> = sqlx::query_scalar("SELECT version FROM agent_versions WHERE agent_id = $1::uuid ORDER BY version")
        .bind(&a).fetch_all(&c.pool).await.unwrap();
    assert_eq!(versions, (1..=13).collect::<Vec<_>>());
    let name: String = sqlx::query_scalar("SELECT name FROM agents WHERE id = $1::uuid").bind(&a).fetch_one(&c.pool).await.unwrap();
    assert!(name.starts_with("edit"));
}

#[tokio::test]
async fn extreme_and_inconsistent_policies_are_rejected_on_write() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    let mut cases: Vec<(&str, Value)> = vec![];
    for (field, v) in [
        ("max_per_interaction", json!("9223372036854775808")),
        ("max_per_interaction", json!(10)),
        ("max_per_interaction", json!("-1")),
        ("approval_threshold", json!("999999999999")), // above max_per_interaction
        ("max_aggregate_exposure", json!("1")),        // below max_per_interaction
        ("allowed_interaction_types", json!([])),
        ("allowed_interaction_types", json!(["challenge", "challenge"])),
        ("allowed_interaction_types", json!(["withdraw"])),
        ("min_conviction_bps", json!(10001)),
        ("min_conviction_bps", json!(-1)),
        ("stop_loss", json!("0")),
    ] {
        let mut input = agent_input("P");
        input["policy"][field] = v;
        cases.push((field, input));
    }
    for (field, input) in cases {
        let (s, body) = put(&c.app, &format!("/api/agents/{a}"), input).await;
        assert!(s.is_client_error(), "{field}: {s} {body}");
    }
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_versions").fetch_one(&c.pool).await.unwrap();
    assert_eq!(n, 1, "rejected edits must not create versions");
}

#[tokio::test]
async fn status_endpoint_rejects_unknown_values() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    for body in [json!({"status": "deleted"}), json!({"status": ""}), json!({"status": null}), json!({}), json!({"status": "ACTIVE"})] {
        let (s, _) = send(&c.app, &format!("/api/agents/{a}/status"), body.clone(), None).await;
        assert!(s.is_client_error(), "{body} -> {s}");
    }
}

/// CSRF: `ApiJson` reads the body bytes and ignores Content-Type, so a hostile web page can make the
/// user's browser send a "simple" cross-origin POST (`text/plain`, no preflight) that mutates state.
/// CORS only stops the page from READING the response. Expected secure behaviour: reject non-JSON
/// content types with 415.
#[tokio::test]
#[ignore = "BUG SEC-01: POST endpoints accept text/plain bodies, so cross-site forms can move (simulated) funds without a CORS preflight"]
async fn state_changing_posts_must_require_a_json_content_type() {
    let c = setup().await;
    for ct in ["text/plain", "application/x-www-form-urlencoded", "multipart/form-data; boundary=x"] {
        let req = Request::builder()
            .method("POST")
            .uri("/api/wallet/simulated-deposit")
            .header("content-type", ct)
            .header("origin", "https://evil.example")
            .body(Body::from(r#"{"amount":"1000000"}"#))
            .unwrap();
        let (s, _) = raw(&c.app, req).await;
        assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE, "content-type {ct} was accepted: {s}");
    }
    // no Content-Type at all
    let req = Request::builder().method("POST").uri("/api/wallet/simulated-deposit")
        .body(Body::from(r#"{"amount":"1000000"}"#)).unwrap();
    assert_eq!(raw(&c.app, req).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
}
