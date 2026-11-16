//! Adversarial API tests: concurrent allocation, idempotency abuse, malformed
//! amounts and headers. Uses its own database (`aen_sec_test` unless
//! `TEST_DATABASE_URL` is set) and truncates it on every test.

mod common;

use aen_core::ledger::{Account, Env, Posting, PostingSet, Reason};
use aen_core::money::SignedMoney;
use aen_server::guard::ApiToken;
use aen_server::ledger_db::{self, PostOutcome};
use aen_server::state::AppState;
use aen_server::{DEFAULT_CORS_ORIGIN, MIGRATOR, app, app_with_state};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{Mutex, MutexGuard};
use tower::ServiceExt;
use uuid::Uuid;

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
    common::wipe(&pool).await;
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
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
    send(
        app,
        "/api/wallet/simulated-deposit",
        json!({"amount": units}),
        None,
    )
    .await
}

/// Global ledger invariants straight from SQL.
async fn assert_ledger_sane(pool: &PgPool) {
    let total: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(delta),0)::BIGINT FROM ledger_entries")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(total, 0, "ledger must sum to zero");
    let negatives: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (SELECT account FROM ledger_entries WHERE account <> 'external' \
         GROUP BY env, account HAVING SUM(delta) < 0) x",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(negatives, 0, "no non-external account may be negative");
}

async fn unallocated(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta),0)::BIGINT FROM ledger_entries WHERE account = 'user:unallocated'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn parallel_allocations_cannot_overdraw_the_wallet() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    let b = agent(&c.app, "B").await;
    assert_eq!(deposit(&c.app, "100").await.0, StatusCode::OK);

    let mut tasks = Vec::new();
    for i in 0..24 {
        let app = c.app.clone();
        let target = if i % 2 == 0 { a.clone() } else { b.clone() };
        tasks.push(tokio::spawn(async move {
            send(
                &app,
                &format!("/api/agents/{target}/allocate"),
                json!({"amount": "30"}),
                Some(&format!("k{i}")),
            )
            .await
        }));
    }
    let mut ok = 0;
    for t in tasks {
        let (s, body) = t.await.unwrap();
        match s {
            StatusCode::OK => ok += 1,
            s => assert!(s.is_client_error(), "unexpected {s}: {body}"),
        }
    }
    assert_eq!(ok, 3, "exactly floor(100/30) allocations may succeed");
    assert_eq!(unallocated(&c.pool).await, 10);
    assert_ledger_sane(&c.pool).await;
}

#[tokio::test]
async fn parallel_replays_of_one_idempotency_key_post_once() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    deposit(&c.app, "1000").await;
    let mut tasks = Vec::new();
    for _ in 0..16 {
        let app = c.app.clone();
        let a = a.clone();
        tasks.push(tokio::spawn(async move {
            send(
                &app,
                &format!("/api/agents/{a}/allocate"),
                json!({"amount": "100"}),
                Some("same-key"),
            )
            .await
        }));
    }
    for t in tasks {
        let (s, body) = t.await.unwrap();
        assert_eq!(s, StatusCode::OK, "{body}");
    }
    assert_eq!(unallocated(&c.pool).await, 900, "debited exactly once");
    let acts: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM activity WHERE kind = 'funds_allocated'")
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(acts, 1, "one activity record for a replayed request");
    assert_ledger_sane(&c.pool).await;
}

#[tokio::test]
async fn one_key_with_different_amounts_yields_one_winner_and_conflicts() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    deposit(&c.app, "1000").await;
    let mut tasks = Vec::new();
    for i in 1..=10 {
        let app = c.app.clone();
        let a = a.clone();
        tasks.push(tokio::spawn(async move {
            send(
                &app,
                &format!("/api/agents/{a}/allocate"),
                json!({"amount": format!("{}", i * 10)}),
                Some("k"),
            )
            .await
            .0
        }));
    }
    let mut ok = 0;
    let mut conflict = 0;
    for t in tasks {
        match t.await.unwrap() {
            StatusCode::OK => ok += 1,
            StatusCode::CONFLICT => conflict += 1,
            s => panic!("unexpected {s}"),
        }
    }
    assert_eq!((ok, conflict), (1, 9));
    assert_ledger_sane(&c.pool).await;
}

#[tokio::test]
async fn idempotency_key_cannot_be_reused_across_endpoints_or_agents() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    let b = agent(&c.app, "B").await;
    deposit(&c.app, "1000").await;
    assert_eq!(
        send(
            &c.app,
            &format!("/api/agents/{a}/allocate"),
            json!({"amount": "100"}),
            Some("k")
        )
        .await
        .0,
        StatusCode::OK
    );
    // same amount, other agent
    assert_eq!(
        send(
            &c.app,
            &format!("/api/agents/{b}/allocate"),
            json!({"amount": "100"}),
            Some("k")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    // opposite direction
    assert_eq!(
        send(
            &c.app,
            &format!("/api/agents/{a}/deallocate"),
            json!({"amount": "100"}),
            Some("k")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    // deposit with the same key
    assert_eq!(
        send(
            &c.app,
            "/api/wallet/simulated-deposit",
            json!({"amount": "100"}),
            Some("k")
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_ledger_sane(&c.pool).await;
}

#[tokio::test]
async fn random_mixed_concurrent_traffic_keeps_ledger_sane() {
    let c = setup().await;
    let ids = [
        agent(&c.app, "A").await,
        agent(&c.app, "B").await,
        agent(&c.app, "C").await,
    ];
    deposit(&c.app, "500").await;
    let mut tasks = Vec::new();
    let mut x: u64 = 0xDEAD_BEEF;
    for i in 0..120 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let app = c.app.clone();
        let id = ids[(x % 3) as usize].clone();
        let amount = format!("{}", 1 + (x >> 8) % 120);
        let kind = (x >> 20) % 3;
        tasks.push(tokio::spawn(async move {
            let key = format!("m{i}");
            let s = match kind {
                0 => {
                    send(
                        &app,
                        "/api/wallet/simulated-deposit",
                        json!({"amount": amount}),
                        Some(&key),
                    )
                    .await
                    .0
                }
                1 => {
                    send(
                        &app,
                        &format!("/api/agents/{id}/allocate"),
                        json!({"amount": amount}),
                        Some(&key),
                    )
                    .await
                    .0
                }
                _ => {
                    send(
                        &app,
                        &format!("/api/agents/{id}/deallocate"),
                        json!({"amount": amount}),
                        Some(&key),
                    )
                    .await
                    .0
                }
            };
            assert!(s == StatusCode::OK || s.is_client_error(), "got {s}");
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    assert_ledger_sane(&c.pool).await;
}

#[tokio::test]
async fn malformed_amounts_never_reach_the_ledger_or_cause_5xx() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    deposit(&c.app, "1000").await;
    let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    let bad = [
        json!({"amount": 5}),
        json!({"amount": -5}),
        json!({"amount": "-5"}),
        json!({"amount": "0"}),
        json!({"amount": "1.5"}),
        json!({"amount": "1e3"}),
        json!({"amount": ""}),
        json!({"amount": "007"}),
        json!({"amount": " 5"}),
        json!({"amount": "+5"}),
        json!({"amount": null}),
        json!({"amount": ["5"]}),
        json!({"amount": "9223372036854775808"}),
        json!({"amount": "99999999999999999999999999999999"}),
        json!({}),
        json!([]),
        json!("5"),
        json!(null),
    ];
    for body in bad {
        for path in [
            format!("/api/agents/{a}/allocate"),
            format!("/api/agents/{a}/deallocate"),
            "/api/wallet/simulated-deposit".to_owned(),
        ] {
            let (s, resp) = send(&c.app, &path, body.clone(), None).await;
            assert!(s.is_client_error(), "{path} {body} -> {s} {resp}");
        }
    }
    let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[tokio::test]
async fn bad_headers_and_paths_are_client_errors() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    deposit(&c.app, "1000").await;
    let p = format!("/api/agents/{a}/allocate");
    for key in ["", "   ", &"x".repeat(129)] {
        let (s, _) = send(&c.app, &p, json!({"amount": "1"}), Some(key)).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "key {key:?}");
    }
    // non-ASCII header value
    let req = Request::builder()
        .method("POST")
        .uri(&p)
        .header("content-type", "application/json")
        .header("Idempotency-Key", "caf\u{e9}".as_bytes())
        .body(Body::from(r#"{"amount":"1"}"#))
        .unwrap();
    assert_eq!(raw(&c.app, req).await.0, StatusCode::BAD_REQUEST);
    // exactly 128 chars is fine
    assert_eq!(
        send(&c.app, &p, json!({"amount": "1"}), Some(&"y".repeat(128)))
            .await
            .0,
        StatusCode::OK
    );
    // key that looks like SQL / lock names
    assert_eq!(
        send(
            &c.app,
            &p,
            json!({"amount": "1"}),
            Some("'; DROP TABLE ledger_entries; --")
        )
        .await
        .0,
        StatusCode::OK
    );
    // bad ids
    for id in [
        "not-a-uuid",
        "00000000-0000-0000-0000-000000000000",
        "%00",
        "1%27%20OR%201=1",
    ] {
        let (s, _) = send(
            &c.app,
            &format!("/api/agents/{id}/allocate"),
            json!({"amount": "1"}),
            None,
        )
        .await;
        assert!(
            s == StatusCode::BAD_REQUEST || s == StatusCode::NOT_FOUND,
            "{id} -> {s}"
        );
    }
    // wrong content type / garbage / oversized body
    let req = Request::builder()
        .method("POST")
        .uri(&p)
        .header("content-type", "text/plain")
        .body(Body::from("amount=1"))
        .unwrap();
    assert!(raw(&c.app, req).await.0.is_client_error());
    let req = Request::builder()
        .method("POST")
        .uri(&p)
        .header("content-type", "application/json")
        .body(Body::from("{\"amount\":"))
        .unwrap();
    assert!(raw(&c.app, req).await.0.is_client_error());
    let big = format!(
        "{{\"amount\":\"1\",\"pad\":\"{}\"}}",
        "a".repeat(5 * 1024 * 1024)
    );
    let req = Request::builder()
        .method("POST")
        .uri(&p)
        .header("content-type", "application/json")
        .body(Body::from(big))
        .unwrap();
    // SEC-05: an oversized body is 413, not 400.
    assert_eq!(raw(&c.app, req).await.0, StatusCode::PAYLOAD_TOO_LARGE);
    assert_ledger_sane(&c.pool).await;
}

#[tokio::test]
async fn deallocate_cannot_exceed_agent_available_and_unknown_agent_is_404() {
    let c = setup().await;
    let a = agent(&c.app, "A").await;
    deposit(&c.app, "100").await;
    send(
        &c.app,
        &format!("/api/agents/{a}/allocate"),
        json!({"amount": "40"}),
        None,
    )
    .await;
    let (s, _) = send(
        &c.app,
        &format!("/api/agents/{a}/deallocate"),
        json!({"amount": "41"}),
        None,
    )
    .await;
    assert!(s.is_client_error());
    let (s, _) = send(
        &c.app,
        &format!("/api/agents/{}/allocate", uuid_like()),
        json!({"amount": "1"}),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_ledger_sane(&c.pool).await;
}

fn uuid_like() -> &'static str {
    "123e4567-e89b-12d3-a456-426614174000"
}

#[tokio::test]
async fn agent_text_fields_are_stored_literally() {
    let c = setup().await;
    let nasty = "Robert'); DROP TABLE agents;-- <script>alert(1)</script>";
    let id = agent(&c.app, nasty).await;
    let res = c
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/agents/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["name"], nasty);
    // oversize name rejected with 422, not 500
    let (s, _) = send(
        &c.app,
        "/api/agents",
        agent_input(&"n".repeat(10_000)),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Two deposits of i64::MAX must be refused cleanly (the second would overflow
/// the Postgres BIGINT sum) and must not poison `/wallet`.
#[tokio::test]
async fn deposit_overflow_is_refused_and_does_not_brick_the_wallet() {
    let c = setup().await;
    let max = "9223372036854775807";
    let (s1, _) = deposit(&c.app, max).await;
    let (s2, _) = deposit(&c.app, max).await;
    assert!(s1.is_success());
    assert!(s2.is_client_error(), "second deposit -> {s2}");
    let res = c
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/wallet")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(res.status().is_success(), "/api/wallet -> {}", res.status());
    assert_ledger_sane(&c.pool).await;
}

/// Unknown body fields are ignored: a client cannot pick the environment.
#[tokio::test]
async fn extra_env_field_cannot_select_a_real_environment() {
    let c = setup().await;
    let (s, _) = send(
        &c.app,
        "/api/wallet/simulated-deposit",
        json!({"amount": "5", "env": "arc_testnet"}),
        None,
    )
    .await;
    assert!(s.is_success());
    let envs: Vec<String> = sqlx::query_scalar("SELECT DISTINCT env FROM ledger_entries")
        .fetch_all(&c.pool)
        .await
        .unwrap();
    assert_eq!(envs, vec!["simulated".to_owned()]);
}

// ---------------------------------------------------------------------------
// SEC-02: Host allowlist, Origin check, bearer token
// ---------------------------------------------------------------------------

fn get_req(path: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut req = Request::builder().method("GET").uri(path);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req.body(Body::empty()).unwrap()
}

fn deposit_req(headers: &[(&str, &str)]) -> Request<Body> {
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/wallet/simulated-deposit")
        .header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req.body(Body::from(r#"{"amount":"1000000"}"#)).unwrap()
}

/// DNS rebinding: a page on `evil.example` that resolves to 127.0.0.1 makes
/// the browser send `Host: evil.example:8080`. Before SEC-02 it was served.
#[tokio::test]
async fn a_foreign_host_is_refused() {
    let c = setup().await;
    for host in [
        "evil.example:8080",
        "evil.example",
        "localhost.evil.example",
    ] {
        let (s, body) = raw(&c.app, get_req("/api/wallet", &[("host", host)])).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{host}");
        assert_eq!(body["error"]["code"], "host_not_allowed");
        let (s, _) = raw(&c.app, deposit_req(&[("host", host)])).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{host}");
    }
    assert_eq!(unallocated(&c.pool).await, 0);
    // The local names are served, with or without a port.
    for host in [
        "localhost:8080",
        "127.0.0.1:8080",
        "[::1]:8080",
        "LOCALHOST",
    ] {
        let (s, _) = raw(&c.app, get_req("/api/wallet", &[("host", host)])).await;
        assert_eq!(s, StatusCode::OK, "{host}");
    }
}

/// CSRF: a page on another origin can make the browser send a JSON POST once
/// it passes (or skips) preflight; the API used to process it.
#[tokio::test]
async fn a_foreign_origin_cannot_change_state() {
    let c = setup().await;
    for origin in ["https://evil.example", "null", "http://localhost:3001"] {
        let req = deposit_req(&[("host", "localhost:8080"), ("origin", origin)]);
        let (s, body) = raw(&c.app, req).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{origin}");
        assert_eq!(body["error"]["code"], "origin_not_allowed");
    }
    assert_eq!(unallocated(&c.pool).await, 0);

    // The default local setup is unchanged: the web app on localhost:3000
    // calls the API on localhost:8080 straight from the browser.
    let req = deposit_req(&[("host", "localhost:8080"), ("origin", DEFAULT_CORS_ORIGIN)]);
    let res = c.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        DEFAULT_CORS_ORIGIN
    );
    // And curl / server-side code without an Origin still works.
    let (s, _) = raw(&c.app, deposit_req(&[])).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(unallocated(&c.pool).await, 2_000_000);
}

#[tokio::test]
async fn with_a_token_every_route_but_health_needs_it() {
    let c = setup().await;
    let token = "t".repeat(40);
    let mut state = AppState::offline(c.pool.clone());
    state.settings.guard.api_token = ApiToken::new(&token);
    let app = app_with_state(state, DEFAULT_CORS_ORIGIN.parse().unwrap());
    let good = format!("Bearer {token}");
    let wrong = format!("Bearer {}", "u".repeat(40));
    let short = format!("Bearer {}", &token[..39]);

    for auth in [
        None,
        Some(wrong.as_str()),
        Some(short.as_str()),
        Some(token.as_str()),
    ] {
        let headers: Vec<(&str, &str)> = auth.map(|a| ("authorization", a)).into_iter().collect();
        let res = app
            .clone()
            .oneshot(get_req("/api/agents", &headers))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{auth:?}");
        assert_eq!(res.headers().get("www-authenticate").unwrap(), "Bearer");
        let (s, _) = raw(&app, deposit_req(&headers)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        // Unknown paths do not reveal anything either.
        let (s, _) = raw(&app, get_req("/api/nope", &headers)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(unallocated(&c.pool).await, 0);

    let (s, _) = raw(&app, get_req("/api/health", &[])).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = raw(&app, get_req("/api/agents", &[("authorization", &good)])).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = raw(&app, deposit_req(&[("authorization", &good)])).await;
    assert_eq!(s, StatusCode::OK);
    // The token does not replace the Host and Origin checks.
    let (s, _) = raw(
        &app,
        deposit_req(&[("authorization", &good), ("host", "evil.example")]),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // A CORS preflight is answered without the token.
    let preflight = Request::builder()
        .method("OPTIONS")
        .uri("/api/agents")
        .header("origin", DEFAULT_CORS_ORIGIN)
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "authorization,content-type",
        )
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(preflight).await.unwrap();
    assert!(res.status().is_success(), "{}", res.status());
    assert_eq!(unallocated(&c.pool).await, 1_000_000);
}

// ---------------------------------------------------------------------------
// SEC-09: idempotency keys scoped per env and caller; interaction compared
// ---------------------------------------------------------------------------

/// A client used to be able to pre-claim a key the server uses for its own
/// postings (`demo-seed:deposit`, `settle:{id}`, ...), which made that
/// posting fail with 409 (or silently count as done).
#[tokio::test]
async fn a_client_cannot_pre_claim_a_server_idempotency_key() {
    let c = setup().await;
    let (s, _) = send(
        &c.app,
        "/api/wallet/simulated-deposit",
        json!({"amount": "1"}),
        Some("demo-seed:deposit"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, body) = send(&c.app, "/api/dev/seed-demo", json!({}), None).await;
    assert_eq!(s, StatusCode::OK, "{body}");
    assert_eq!(unallocated(&c.pool).await, 1);
    // The client's own replay still works.
    let (s, _) = send(
        &c.app,
        "/api/wallet/simulated-deposit",
        json!({"amount": "1"}),
        Some("demo-seed:deposit"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(unallocated(&c.pool).await, 1);
}

fn set_with(env: Env, key: &str, interaction: Option<Uuid>) -> PostingSet {
    PostingSet::new(
        env,
        Reason::Deposit,
        key,
        interaction,
        vec![
            Posting {
                account: Account::External,
                delta: SignedMoney::from_minor(-5),
            },
            Posting {
                account: Account::UserUnallocated,
                delta: SignedMoney::from_minor(5),
            },
        ],
    )
    .unwrap()
}

async fn post_set(pool: &PgPool, set: &PostingSet) -> Result<PostOutcome, String> {
    let mut tx = pool.begin().await.unwrap();
    let out = ledger_db::post(&mut tx, set)
        .await
        .map_err(|e| e.code.to_owned())?;
    tx.commit().await.unwrap();
    Ok(out)
}

#[tokio::test]
async fn a_replay_must_name_the_same_interaction_and_keys_are_per_env() {
    let c = setup().await;
    let (x, y) = (Uuid::new_v4(), Uuid::new_v4());
    assert_eq!(
        post_set(&c.pool, &set_with(Env::Simulated, "k", Some(x))).await,
        Ok(PostOutcome::Posted)
    );
    assert_eq!(
        post_set(&c.pool, &set_with(Env::Simulated, "k", Some(x))).await,
        Ok(PostOutcome::Replayed)
    );
    // Same postings for another interaction (or none) is not a replay.
    for other in [Some(y), None] {
        assert_eq!(
            post_set(&c.pool, &set_with(Env::Simulated, "k", other)).await,
            Err("idempotency_key_reused".to_owned())
        );
    }
    // The same key in another environment is a different posting set.
    assert_eq!(
        post_set(&c.pool, &set_with(Env::ArcTestnet, "k", Some(x))).await,
        Ok(PostOutcome::Posted)
    );
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(rows, 4);
}
