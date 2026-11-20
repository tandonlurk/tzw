//! QuickNode Streams webhook ingestion, fixture-driven (no network): HMAC
//! signature, stale timestamps, idempotency on (tx hash, log index) under
//! duplicate and reordered deliveries, activity, and the request guard.

mod common;

use aen_server::guard::ApiToken;
use aen_server::quicknode::{QuickNodeConfig, StreamSecret, sign};
use aen_server::state::AppState;
use aen_server::streams::{self, StreamContracts};
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
    state: AppState,
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
        app: app_with_state(state.clone(), DEFAULT_CORS_ORIGIN.parse().unwrap()),
        pool,
        state,
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
async fn gzip_deliveries_get_a_clear_415() {
    let c = setup().await;
    let req = delivery(FIXTURE, SECRET, &now(), &[("content-encoding", "gzip")]);
    let (status, body) = call(&c.app, req).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{body}");
    assert!(events(&c.pool).await.is_empty());
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

// ---------------------------------------------------------------------------
// Streams as a confirmation of the Arc testnet lifecycle (D-74). The fixture
// is the real delivery of one challenge's lifecycle on Arc testnet
// (2026-11-19): two VaultCreated, Created, two Deposited, Activated,
// Resolved; the interaction fixture is that challenge's row on the server
// that ran it.
// ---------------------------------------------------------------------------

const ARC: &str = include_str!("fixtures/quicknode/arc_escrow_lifecycle.json");
const ARC_ROW: &str = include_str!("fixtures/quicknode/arc_lifecycle_interaction.json");
const CHALLENGE: &str = "838ca4b6-2e3b-46f3-9cad-6b81b1fb760c";
const BULL: &str = "00000000-0000-4000-8000-0000000000d1";
const BULL_VAULT: &str = "0xf78499779bcc48f4e96d34a06d6ae4869cc5b37f";
const SKEPTIC_VAULT: &str = "0xcac9240abc5563bbf7a8fb32be8c5fd3b1c3d0b6";
const OWNER: &str = "0x1f0eb73c714ae0bd1a97c1c8e0f34513073393fb";

fn arc_contracts() -> StreamContracts {
    StreamContracts {
        escrow: "0x4179f2e314410d10c2906b1914baae8dd93b5698".into(),
        factory: "0x4ea6ac6a9f7b8fcf01ba04b177f0a098d9664c45".into(),
        explorer_url: Some("https://explorer.testnet.arc.io".into()),
    }
}

async fn arc_setup() -> Ctx {
    setup_with(|s| s.settings.quicknode.as_mut().unwrap().contracts = Some(arc_contracts())).await
}

fn fixture_arc() -> Value {
    serde_json::from_str(ARC).unwrap()
}

/// The two demo agents with the vaults they had, and (optionally) the
/// challenge row exactly as the server that ran it stored it.
async fn seed_arc(c: &Ctx, with_challenge: bool) {
    let (s, out) = call(
        &c.app,
        Request::post("/api/dev/seed-demo")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{out}");
    for (suffix, vault) in [("d1", BULL_VAULT), ("d2", SKEPTIC_VAULT)] {
        sqlx::query(
            "UPDATE agents SET env = 'arc_testnet', vault_address = $2, vault_owner = $3 \
             WHERE id = $1::uuid",
        )
        .bind(format!("00000000-0000-4000-8000-0000000000{suffix}"))
        .bind(vault)
        .bind(OWNER)
        .execute(&c.pool)
        .await
        .unwrap();
    }
    if with_challenge {
        sqlx::query(
            "INSERT INTO interactions SELECT * FROM jsonb_populate_record(NULL::interactions, $1)",
        )
        .bind(serde_json::from_str::<Value>(ARC_ROW).unwrap())
        .execute(&c.pool)
        .await
        .unwrap();
    }
}

async fn statuses(pool: &PgPool) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT event_name, match_status FROM chain_events WHERE event_name IS NOT NULL \
         ORDER BY block_number, log_index",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn stream_activity(pool: &PgPool) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT kind, summary, interaction_id::text FROM activity \
         WHERE detail->>'source' = 'quicknode_streams' ORDER BY seq",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

async fn get_json(app: &Router, uri: &str) -> Value {
    let (s, v) = call(app, Request::get(uri).body(Body::empty()).unwrap()).await;
    assert_eq!(s, StatusCode::OK, "{uri}: {v}");
    v
}

#[tokio::test]
async fn the_real_arc_delivery_confirms_the_whole_lifecycle() {
    let c = arc_setup().await;
    seed_arc(&c, true).await;
    let (s, out) = call(&c.app, delivery(ARC, SECRET, &now(), &[])).await;
    assert_eq!(s, StatusCode::OK, "{out}");
    assert_eq!(
        out,
        json!({"received": 7, "inserted": 7, "duplicates": 0, "removed": 0, "rejected": 0})
    );
    assert_eq!(
        statuses(&c.pool).await,
        pairs(&[
            ("VaultCreated", "confirmed"),
            ("VaultCreated", "confirmed"),
            ("Created", "confirmed"),
            ("Deposited", "confirmed"),
            ("Deposited", "confirmed"),
            ("Activated", "confirmed"),
            ("Resolved", "confirmed"),
        ])
    );
    // blockTimestamp is kept, so the block-to-delivery latency is known.
    let stamped: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM chain_events WHERE block_timestamp = to_timestamp(1795139565)",
    )
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(stamped, 1);

    let acts = stream_activity(&c.pool).await;
    assert_eq!(acts.len(), 7);
    assert!(acts.iter().all(|a| a.0 == "stream_confirmed"), "{acts:?}");
    let funded = &acts[3];
    assert!(
        funded.1.starts_with(
            "Escrow funded on Arc: \"Demo Bull (simulated)\"'s vault deposited 1.50 USDC \
             (Arc testnet USDC), confirmed by QuickNode Streams (block 73112834, "
        ),
        "{}",
        funded.1
    );
    assert_eq!(funded.2.as_deref(), Some(CHALLENGE));
    assert!(
        acts[6]
            .1
            .contains("Escrow paid 3.00 USDC (Arc testnet USDC) to \"Demo Bull"),
        "{}",
        acts[6].1
    );
    assert!(
        acts[0].1.starts_with("Vault 0xf78499…b37f for \"Demo Bull"),
        "{}",
        acts[0].1
    );

    // The challenge page shows them with explorer links.
    let detail = get_json(&c.app, &format!("/api/interactions/{CHALLENGE}")).await;
    let evs = detail["stream_events"].as_array().unwrap();
    assert_eq!(evs.len(), 5);
    assert_eq!(evs[1]["event"], "Deposited");
    assert_eq!(evs[1]["status"], "confirmed");
    assert_eq!(evs[1]["block_number"], 73112834);
    assert!(evs[1]["latency_ms"].is_i64());
    assert_eq!(
        evs[1]["explorer_url"],
        "https://explorer.testnet.arc.io/tx/0x792b93c846b895f583082956e78853cb0ec71c444907e9a3e1e9e713c83f840f"
    );
    // A stream event never touches the challenge or the ledger.
    let (state, winner): (String, String) =
        sqlx::query_as("SELECT state, winner_id::text FROM interactions WHERE id = $1::uuid")
            .bind(CHALLENGE)
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!((state.as_str(), winner.as_str()), ("settled", BULL));
    let ledger: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries WHERE env = 'arc_testnet'")
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(ledger, 0);

    let st = get_json(&c.app, "/api/streams").await;
    assert_eq!(
        (
            st["events_total"].clone(),
            st["confirmed"].clone(),
            st["webhook_enabled"].clone(),
            st["matching_enabled"].clone(),
            st["last_block_number"].clone()
        ),
        (
            json!(7),
            json!(7),
            json!(true),
            json!(true),
            json!(73113135)
        )
    );

    // The same delivery again: nothing new, no second activity.
    let (_, out) = call(&c.app, delivery(ARC, SECRET, &now(), &[])).await;
    assert_eq!(out["duplicates"], 7);
    assert_eq!(stream_activity(&c.pool).await.len(), 7);
}

#[tokio::test]
async fn events_ahead_of_the_server_wait_and_are_confirmed_once() {
    let c = arc_setup().await;
    seed_arc(&c, true).await;
    // As if the stream delivered before the server stored its receipts.
    sqlx::query(
        "UPDATE interactions SET escrow_create_tx = NULL, proposer_stake_tx = NULL, \
         counterparty_stake_tx = NULL, escrow_settle_tx = NULL, winner_id = NULL, outcome = NULL \
         WHERE id = $1::uuid",
    )
    .bind(CHALLENGE)
    .execute(&c.pool)
    .await
    .unwrap();
    let row: Value = serde_json::from_str(ARC_ROW).unwrap();
    let saved = |k: &str| row[k].as_str().map(str::to_owned);
    call(&c.app, delivery(ARC, SECRET, &now(), &[])).await;
    let st = statuses(&c.pool).await;
    assert_eq!(
        st.iter().filter(|(_, s)| s == "pending").count(),
        5,
        "{st:?}"
    );
    // Only the two vault confirmations are in activity so far.
    assert_eq!(stream_activity(&c.pool).await.len(), 2);

    // The server confirms its funding transactions; a recheck picks them up.
    sqlx::query(
        "UPDATE interactions SET escrow_create_tx = $2, proposer_stake_tx = $3, \
         counterparty_stake_tx = $4 WHERE id = $1::uuid",
    )
    .bind(CHALLENGE)
    .bind(saved("escrow_create_tx"))
    .bind(saved("proposer_stake_tx"))
    .bind(saved("counterparty_stake_tx"))
    .execute(&c.pool)
    .await
    .unwrap();
    // Concurrent rechecks still record each confirmation once.
    let (a, b) = (c.state.clone(), c.state.clone());
    tokio::join!(streams::recheck(&a, None), streams::recheck(&b, None));
    let st = statuses(&c.pool).await;
    assert_eq!(st[6], ("Resolved".into(), "pending".into()));
    assert_eq!(
        st.iter().filter(|(_, s)| s == "confirmed").count(),
        6,
        "{st:?}"
    );
    assert_eq!(stream_activity(&c.pool).await.len(), 6);

    // Settled: the resolve transaction and the winner.
    sqlx::query(
        "UPDATE interactions SET escrow_settle_tx = $2, winner_id = $3::uuid, outcome = 'a' \
         WHERE id = $1::uuid",
    )
    .bind(CHALLENGE)
    .bind(saved("escrow_settle_tx"))
    .bind(BULL)
    .execute(&c.pool)
    .await
    .unwrap();
    streams::recheck(&c.state, Some(CHALLENGE.parse().unwrap())).await;
    streams::recheck(&c.state, None).await;
    assert!(
        statuses(&c.pool)
            .await
            .iter()
            .all(|(_, s)| s == "confirmed")
    );
    assert_eq!(stream_activity(&c.pool).await.len(), 7);
}

#[tokio::test]
async fn a_disagreeing_event_is_flagged_and_moves_nothing() {
    let c = arc_setup().await;
    seed_arc(&c, true).await;
    // The server believes the skeptic won, with another resolve transaction.
    let other_tx = format!("0x{}", "ab".repeat(32));
    sqlx::query(
        "UPDATE interactions SET escrow_settle_tx = $2, \
         winner_id = '00000000-0000-4000-8000-0000000000d2', outcome = 'b' WHERE id = $1::uuid",
    )
    .bind(CHALLENGE)
    .bind(&other_tx)
    .execute(&c.pool)
    .await
    .unwrap();
    call(&c.app, delivery(ARC, SECRET, &now(), &[])).await;
    let st = statuses(&c.pool).await;
    assert_eq!(st[6], ("Resolved".into(), "mismatch".into()));
    let acts = stream_activity(&c.pool).await;
    let flagged: Vec<_> = acts.iter().filter(|a| a.0 == "stream_mismatch").collect();
    assert_eq!(flagged.len(), 1);
    assert!(
        flagged[0]
            .1
            .contains("does not match what the server confirmed")
            && flagged[0].1.contains(&other_tx)
            && flagged[0].1.contains("Nothing was changed"),
        "{}",
        flagged[0].1
    );
    // Interaction unchanged; a recheck does not flip a final status.
    streams::recheck(&c.state, None).await;
    let (winner, settle): (String, String) = sqlx::query_as(
        "SELECT winner_id::text, escrow_settle_tx FROM interactions WHERE id = $1::uuid",
    )
    .bind(CHALLENGE)
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(
        (winner.as_str(), settle),
        ("00000000-0000-4000-8000-0000000000d2", other_tx)
    );
    assert_eq!(stream_activity(&c.pool).await.len(), acts.len());
}

#[tokio::test]
async fn a_reorg_withdraws_a_confirmation_and_unknown_ids_stay_unmatched() {
    let c = arc_setup().await;
    // Agents linked, but this server never ran the challenge.
    seed_arc(&c, false).await;
    call(&c.app, delivery(ARC, SECRET, &now(), &[])).await;
    let st = statuses(&c.pool).await;
    assert_eq!(&st[0].1, "confirmed");
    assert!(st[2..].iter().all(|(_, s)| s == "unmatched"), "{st:?}");
    let acts = stream_activity(&c.pool).await;
    assert_eq!(acts.iter().filter(|a| a.0 == "chain_event").count(), 5);
    assert!(
        acts[2]
            .1
            .contains("for a challenge this server does not know"),
        "{}",
        acts[2].1
    );

    // The first VaultCreated is reorged out.
    let mut gone = fixture_arc()["data"][0].clone();
    gone["removed"] = json!(true);
    let body = json!({ "data": [gone] }).to_string();
    let (_, out) = call(&c.app, delivery(&body, SECRET, &now(), &[])).await;
    assert_eq!(out["removed"], 1);
    let st = get_json(&c.app, "/api/streams").await;
    assert_eq!(
        (st["confirmed"].clone(), st["unmatched"].clone()),
        (json!(1), json!(5))
    );
    let removed: Vec<(String,)> =
        sqlx::query_as("SELECT summary FROM activity WHERE kind = 'chain_event_removed'")
            .fetch_all(&c.pool)
            .await
            .unwrap();
    assert_eq!(removed.len(), 1);
    assert!(
        removed[0].0.contains("VaultCreated event removed") && removed[0].0.contains("withdrawn"),
        "{}",
        removed[0].0
    );
}

/// AgentVault logs reach the backend only once the owner adds the vault
/// addresses to the stream filter (docs/setup/quicknode-streams-filter.md).
/// The fixture is the two real `Released` logs of the same challenge, taken
/// from their Arc testnet receipts (not from a stream delivery).
#[tokio::test]
async fn vault_releases_are_confirmed_once_their_vaults_are_streamed() {
    let c = arc_setup().await;
    seed_arc(&c, true).await;
    let body = include_str!("fixtures/quicknode/arc_vault_release.json");
    let (s, out) = call(&c.app, delivery(body, SECRET, &now(), &[])).await;
    assert_eq!(s, StatusCode::OK, "{out}");
    assert_eq!(
        statuses(&c.pool).await,
        pairs(&[("Released", "confirmed"), ("Released", "confirmed")])
    );
    let acts = stream_activity(&c.pool).await;
    assert!(
        acts[1]
            .1
            .starts_with("\"Demo Skeptic (simulated)\"'s vault released its 1.50 USDC"),
        "{}",
        acts[1].1
    );
    // A vault no agent is linked to stays a plain recorded log.
    sqlx::query("UPDATE agents SET vault_address = NULL, vault_owner = NULL")
        .execute(&c.pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM chain_events")
        .execute(&c.pool)
        .await
        .unwrap();
    call(&c.app, delivery(body, SECRET, &now(), &[])).await;
    assert!(statuses(&c.pool).await.is_empty());
}
