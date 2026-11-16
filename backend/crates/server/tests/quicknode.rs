//! QuickNode Streams webhook ingestion, fixture-driven (no network): HMAC
//! signature, stale timestamps, idempotency on (tx hash, log index) under
//! duplicate and reordered deliveries, activity, and the request guard.

mod common;

use aen_server::guard::ApiToken;
use aen_server::quicknode::{QuickNodeConfig, StreamSecret, sign};
use aen_server::state::AppState;
use aen_server::{DEFAULT_CORS_ORIGIN, MIGRATOR, app_with_state};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{Mutex, MutexGuard};
use tower::ServiceExt;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

const SECRET: &str = "qn-test-security-token";
const PATH: &str = "/api/webhooks/quicknode";
const FIXTURE: &str = include_str!("fixtures/quicknode/logs_batch.json");

struct Ctx {
    app: Router,
    pool: PgPool,
    _guard: MutexGuard<'static, ()>,
}

async fn setup_with(adjust: impl FnOnce(&mut AppState)) -> Ctx {
    let guard = DB_LOCK.lock().await;
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| match std::env::var("USER") {
        Ok(user) => format!("postgres://{user}@localhost/aen_test"),
        Err(_) => "postgres://localhost/aen_test".to_owned(),
    });
    let pool = PgPool::connect(&url).await.expect("connect to test db");
    MIGRATOR.run(&pool).await.expect("run migrations");
    common::wipe(&pool).await;
    let mut state = AppState::offline(pool.clone());
    state.settings.quicknode = Some(QuickNodeConfig::new(StreamSecret::new(SECRET).unwrap()));
    adjust(&mut state);
    Ctx {
        app: app_with_state(state, DEFAULT_CORS_ORIGIN.parse().unwrap()),
        pool,
        _guard: guard,
    }
}

async fn setup() -> Ctx {
    setup_with(|_| {}).await
}

fn now() -> String {
    Utc::now().timestamp().to_string()
}

/// A delivery as QuickNode would send it (header names are the UNVERIFIED
/// defaults), signed with `secret`.
fn delivery(body: &str, secret: &str, ts: &str, extra: &[(&str, &str)]) -> Request<Body> {
    let nonce = "8f6c0a2b";
    let mut req = Request::builder()
        .method("POST")
        .uri(PATH)
        .header("content-type", "application/json")
        .header("x-qn-nonce", nonce)
        .header("x-qn-timestamp", ts)
        .header("x-qn-signature", sign(secret, nonce, ts, body.as_bytes()));
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    req.body(Body::from(body.to_owned())).unwrap()
}

async fn call(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn events(pool: &PgPool) -> Vec<(String, i64, i64, bool)> {
    sqlx::query_as(
        "SELECT tx_hash, log_index, block_number, removed FROM chain_events \
         ORDER BY block_number, log_index",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn activity_kinds(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT kind FROM activity ORDER BY seq")
        .fetch_all(pool)
        .await
        .unwrap()
}

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

#[tokio::test]
async fn a_signed_delivery_is_stored_and_shown_as_activity() {
    let c = setup().await;
    let (s, out) = call(&c.app, delivery(FIXTURE, SECRET, &now(), &[])).await;
    assert_eq!(s, StatusCode::OK, "{out}");
    assert_eq!(
        out,
        json!({"received": 3, "inserted": 3, "duplicates": 0, "removed": 0, "rejected": 0})
    );
    let stored = events(&c.pool).await;
    assert_eq!(stored.len(), 3);
    assert_eq!((stored[0].1, stored[0].2), (0, 436));
    assert_eq!((stored[2].1, stored[2].2), (0, 437));
    let (network, stream): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT network, stream_id FROM chain_events LIMIT 1")
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(network.as_deref(), Some("arc-testnet"));
    assert!(stream.is_some());

    // Readable through the normal activity feed.
    let (s, feed) = call(
        &c.app,
        Request::get("/api/activity").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let items: Vec<&Value> = feed
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "chain_event")
        .collect();
    assert_eq!(items.len(), 3);
    let summary = items[0]["summary"].as_str().unwrap();
    assert!(
        summary.starts_with("Arc testnet event seen via QuickNode Streams")
            && summary.contains("Recorded only"),
        "{summary}"
    );
    assert_eq!(items[0]["detail"]["source"], "quicknode_streams");
    assert_eq!(items[0]["agent_id"], Value::Null);
}

#[tokio::test]
async fn duplicate_and_reordered_deliveries_are_safe() {
    let c = setup().await;
    let full = fixture();
    let blocks = full["data"].as_array().unwrap().clone();

    // The second block arrives first, then the whole batch, then the whole
    // batch again, then the first block on its own, reversed.
    let second = json!({ "data": [blocks[1].clone()], "metadata": full["metadata"] }).to_string();
    let mut first_rev = blocks[0].as_array().unwrap().clone();
    first_rev.reverse();
    let first = json!({ "data": first_rev }).to_string();
    let mut outcomes = Vec::new();
    for body in [second.as_str(), FIXTURE, FIXTURE, first.as_str()] {
        let (s, out) = call(&c.app, delivery(body, SECRET, &now(), &[])).await;
        assert_eq!(s, StatusCode::OK, "{out}");
        outcomes.push((out["inserted"].clone(), out["duplicates"].clone()));
    }
    assert_eq!(
        outcomes,
        [
            (json!(1), json!(0)),
            (json!(2), json!(1)),
            (json!(0), json!(3)),
            (json!(0), json!(2)),
        ]
    );
    assert_eq!(events(&c.pool).await.len(), 3);
    let kinds = activity_kinds(&c.pool).await;
    assert_eq!(kinds.iter().filter(|k| *k == "chain_event").count(), 3);

    // Concurrent duplicate deliveries: still one row per log.
    let mut tasks = Vec::new();
    for _ in 0..6 {
        let app = c.app.clone();
        tasks.push(tokio::spawn(async move {
            call(&app, delivery(FIXTURE, SECRET, &now(), &[])).await.0
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    assert_eq!(events(&c.pool).await.len(), 3);
}

#[tokio::test]
async fn a_reorg_removal_marks_the_event_once() {
    let c = setup().await;
    call(&c.app, delivery(FIXTURE, SECRET, &now(), &[])).await;
    let mut removed = fixture()["data"][1][0].clone();
    removed["removed"] = json!(true);
    let body = json!({ "data": [removed] }).to_string();
    for expected in [1, 0] {
        let (s, out) = call(&c.app, delivery(&body, SECRET, &now(), &[])).await;
        assert_eq!(s, StatusCode::OK, "{out}");
        assert_eq!(out["removed"], expected);
    }
    let stored = events(&c.pool).await;
    assert!(stored[2].3, "block 437 log is marked removed");
    assert!(!stored[0].3);
    let kinds = activity_kinds(&c.pool).await;
    assert_eq!(
        kinds.iter().filter(|k| *k == "chain_event_removed").count(),
        1
    );
}

#[tokio::test]
async fn bad_missing_or_stale_signatures_store_nothing() {
    let c = setup().await;
    let ts = now();
    let cases: Vec<(&str, Request<Body>, &str)> = vec![
        (
            "wrong secret",
            delivery(FIXTURE, "not-the-secret", &ts, &[]),
            "invalid_signature",
        ),
        (
            "tampered body",
            {
                let signed = delivery(FIXTURE, SECRET, &ts, &[]);
                let (parts, _) = signed.into_parts();
                Request::from_parts(parts, Body::from(FIXTURE.replace("0x1b5", "0x1b6")))
            },
            "invalid_signature",
        ),
        (
            "missing signature",
            Request::post(PATH)
                .header("content-type", "application/json")
                .header("x-qn-timestamp", ts.as_str())
                .body(Body::from(FIXTURE))
                .unwrap(),
            "missing_signature",
        ),
        (
            "missing timestamp",
            {
                let signed = delivery(FIXTURE, SECRET, &ts, &[]);
                let (mut parts, body) = signed.into_parts();
                parts.headers.remove("x-qn-timestamp");
                Request::from_parts(parts, body)
            },
            "missing_signature",
        ),
        (
            "stale (10 min old)",
            delivery(
                FIXTURE,
                SECRET,
                &(Utc::now().timestamp() - 600).to_string(),
                &[],
            ),
            "stale_timestamp",
        ),
        (
            "from the future",
            delivery(
                FIXTURE,
                SECRET,
                &(Utc::now().timestamp() + 600).to_string(),
                &[],
            ),
            "stale_timestamp",
        ),
        (
            "not a number",
            delivery(FIXTURE, SECRET, "yesterday", &[]),
            "stale_timestamp",
        ),
    ];
    for (name, req, code) in cases {
        let (s, out) = call(&c.app, req).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{name}: {out}");
        assert_eq!(out["error"]["code"], code, "{name}");
    }
    // Inside the window (4 min old, milliseconds) is fine.
    let ms = ((Utc::now().timestamp() - 240) * 1000).to_string();
    let (s, _) = call(&c.app, delivery(FIXTURE, SECRET, &ms, &[])).await;
    assert_eq!(s, StatusCode::OK);
    // Before that, nothing at all was stored.
    let (s, out) = call(&c.app, delivery("{\"data\": 5}", SECRET, &now(), &[])).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{out}");
    let (s, _) = call(&c.app, delivery("not json", SECRET, &now(), &[])).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(events(&c.pool).await.len(), 3);
}

#[tokio::test]
async fn malformed_logs_are_rejected_one_by_one() {
    let c = setup().await;
    let mut f = fixture();
    f["data"][0][0]["transactionHash"] = json!("0x1234");
    f["data"][0][1]["address"] = json!("not an address");
    let body = f.to_string();
    let (s, out) = call(&c.app, delivery(&body, SECRET, &now(), &[])).await;
    assert_eq!(s, StatusCode::OK, "{out}");
    assert_eq!(
        (out["inserted"].clone(), out["rejected"].clone()),
        (json!(1), json!(2))
    );
    assert_eq!(events(&c.pool).await.len(), 1);
}

#[tokio::test]
async fn header_names_are_configurable() {
    let c = setup_with(|s| {
        let q = s.settings.quicknode.as_mut().unwrap();
        q.signature_header = "x-custom-sig".into();
    })
    .await;
    // The default header name is no longer read.
    let (s, _) = call(&c.app, delivery(FIXTURE, SECRET, &now(), &[])).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let ts = now();
    let req = Request::post(PATH)
        .header("content-type", "application/json")
        .header("x-qn-nonce", "n")
        .header("x-qn-timestamp", ts.as_str())
        .header("x-custom-sig", sign(SECRET, "n", &ts, FIXTURE.as_bytes()))
        .body(Body::from(FIXTURE))
        .unwrap();
    let (s, out) = call(&c.app, req).await;
    assert_eq!(s, StatusCode::OK, "{out}");
}

#[tokio::test]
async fn the_route_is_absent_unless_a_secret_is_configured() {
    let c = setup_with(|s| s.settings.quicknode = None).await;
    let (s, _) = call(&c.app, delivery(FIXTURE, SECRET, &now(), &[])).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(events(&c.pool).await.is_empty());
}

/// Through a tunnel QuickNode's requests carry the tunnel's Host and no
/// Origin. The tunnel host must be in ALLOWED_HOSTS; with a token set, the
/// webhook needs only its HMAC while every other route needs the token.
#[tokio::test]
async fn the_guard_lets_signed_deliveries_through_a_tunnel_host() {
    let token = "t".repeat(40);
    let c = setup_with(|s| {
        s.settings.guard = s
            .settings
            .guard
            .clone()
            .with_extra_hosts("abc.tunnel.example");
        s.settings.guard.api_token = ApiToken::new(&token);
    })
    .await;
    let host = [("host", "abc.tunnel.example")];
    let (s, out) = call(&c.app, delivery(FIXTURE, SECRET, &now(), &host)).await;
    assert_eq!(s, StatusCode::OK, "{out}");
    // An unsigned delivery is still refused by the HMAC check.
    let (s, _) = call(&c.app, delivery(FIXTURE, "wrong", &now(), &host)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // The rest of the API through the tunnel host needs the token.
    let (s, _) = call(
        &c.app,
        Request::get("/api/wallet")
            .header("host", "abc.tunnel.example")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // A host that is not allowlisted is refused even with a valid signature.
    let (s, out) = call(
        &c.app,
        delivery(FIXTURE, SECRET, &now(), &[("host", "evil.example")]),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(out["error"]["code"], "host_not_allowed");
    // A browser on another origin cannot post to it either.
    let (s, _) = call(
        &c.app,
        delivery(
            FIXTURE,
            SECRET,
            &now(),
            &[
                ("host", "abc.tunnel.example"),
                ("origin", "https://evil.example"),
            ],
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}
