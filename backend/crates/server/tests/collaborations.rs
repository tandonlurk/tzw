//! Collaboration MVP against a real Postgres (`TEST_DATABASE_URL`, default
//! `aen_test`): states, authorization at every step, the payout split
//! through the double-entry ledger, and the failure paths.

mod common;

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

const USDC: i64 = 1_000_000;

struct Ctx {
    app: Router,
    pool: PgPool,
    _guard: MutexGuard<'static, ()>,
}

async fn setup() -> Ctx {
    let guard = DB_LOCK.lock().await;
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| match std::env::var("USER") {
        Ok(user) => format!("postgres://{user}@localhost/aen_test"),
        Err(_) => "postgres://localhost/aen_test".to_owned(),
    });
    let pool = PgPool::connect(&url).await.expect("connect to test db");
    MIGRATOR.run(&pool).await.expect("run migrations");
    common::wipe(&pool).await;
    Ctx {
        app: app(pool.clone(), DEFAULT_CORS_ORIGIN.parse().unwrap()),
        pool,
        _guard: guard,
    }
}

async fn send(app: &Router, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    let body = match body {
        Some(v) => {
            req = req.header("content-type", "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn post(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(app, "POST", path, Some(body)).await
}

async fn ok(app: &Router, path: &str, body: Value) -> Value {
    let (s, out) = post(app, path, body).await;
    assert!(s.is_success(), "POST {path}: {s} {out}");
    out
}

async fn get(app: &Router, path: &str) -> Value {
    let (s, out) = send(app, "GET", path, None).await;
    assert_eq!(s, StatusCode::OK, "GET {path}: {out}");
    out
}

/// An agent allowed to collaborate: max 10 USDC per interaction, 20 in
/// total, automatic up to 5, funded with `funds` simulated USDC.
async fn agent(c: &Ctx, name: &str, funds: i64, policy: Value) -> String {
    let mut p = json!({
        "max_per_interaction": (10 * USDC).to_string(),
        "max_aggregate_exposure": (20 * USDC).to_string(),
        "approval_threshold": (5 * USDC).to_string(),
        "allowed_interaction_types": ["challenge", "collaboration"],
        "min_conviction_bps": 6000, "stop_loss": null,
    });
    for (k, v) in policy.as_object().unwrap() {
        p[k] = v.clone();
    }
    let input = json!({
        "name": name, "description": "collaboration test",
        "config": {"goals": "g", "strategy": "s", "categories": ["crypto"],
                   "horizon_secs": 3600, "risk_tolerance": "medium", "schedule_secs": 60},
        "policy": p,
        "graph": {"schema_version": 1,
            "nodes": [{"id": "n1", "type": "trigger.schedule", "params": {"every_secs": 60}},
                      {"id": "n2", "type": "reason.evaluate_opportunity", "params": {}},
                      {"id": "n3", "type": "action.request_approval", "params": {}}],
            "edges": [{"from": "n1", "from_port": "out", "to": "n2", "to_port": "in"},
                      {"from": "n2", "from_port": "out", "to": "n3", "to_port": "in"}]},
    });
    let a = ok(&c.app, "/api/agents", input).await;
    let id = a["id"].as_str().unwrap().to_owned();
    if funds > 0 {
        let amount = json!({ "amount": (funds * USDC).to_string() });
        ok(&c.app, "/api/wallet/simulated-deposit", amount.clone()).await;
        ok(&c.app, &format!("/api/agents/{id}/allocate"), amount).await;
    }
    id
}

async fn balance(pool: &PgPool, account: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries WHERE account = $1",
    )
    .bind(account)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn available(pool: &PgPool, agent: &str) -> i64 {
    balance(pool, &format!("agent:{agent}:available")).await
}

async fn committed(pool: &PgPool, agent: &str) -> i64 {
    balance(pool, &format!("agent:{agent}:committed")).await
}

async fn ledger_rows(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Sums to zero overall and per set; nothing but `external` is negative.
async fn assert_ledger_sound(pool: &PgPool) {
    let total: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(total, 0);
    let negative: Vec<String> = sqlx::query_scalar(
        "SELECT account FROM ledger_entries WHERE account <> 'external' GROUP BY account \
         HAVING SUM(delta) < 0",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(negative.is_empty(), "negative accounts: {negative:?}");
}

fn proposal(a: &str, b: &str, bps: u32, ca: i64, cb: i64) -> Value {
    json!({
        "proposer_id": a, "partner_id": b,
        "objective": "Research ETH funding rates and share the findings",
        "proposer_share_bps": bps,
        "proposer_contribution": ca.to_string(),
        "partner_contribution": cb.to_string(),
    })
}

async fn create(c: &Ctx, body: Value) -> String {
    let (s, out) = post(&c.app, "/api/collaborations", body).await;
    assert_eq!(s, StatusCode::CREATED, "{out}");
    out["id"].as_str().unwrap().to_owned()
}

fn step(id: &str, action: &str) -> String {
    format!("/api/collaborations/{id}/{action}")
}

async fn kinds(c: &Ctx, id: &str) -> Vec<String> {
    get(&c.app, &format!("/api/collaborations/{id}")).await["activity"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["kind"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn happy_path_pays_out_the_agreed_split() {
    let c = setup().await;
    let a = agent(&c, "Researcher", 50, json!({})).await;
    let b = agent(&c, "Analyst", 50, json!({})).await;
    let rows = ledger_rows(&c.pool).await;

    let (s, created) = post(
        &c.app,
        "/api/collaborations",
        proposal(&a, &b, 6000, 3 * USDC, 5 * USDC),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{created}");
    let id = created["id"].as_str().unwrap().to_owned();
    assert_eq!(created["state"], "proposed");
    assert_eq!(created["env"], "simulated");
    assert_eq!(created["actions"], json!(["accept", "reject", "cancel"]));
    assert_eq!(created["pool"], "8000000");
    assert_eq!(created["proposer"]["name"], "Researcher");
    assert_eq!(created["proposer"]["share_bps"], 6000);
    assert_eq!(created["partner"]["share_bps"], 4000);
    assert_eq!(created["proposer"]["expected_payout"], "4800000");
    assert_eq!(created["partner"]["expected_payout"], "3200000");
    assert_eq!(created["proposer"]["payout"], Value::Null);
    assert_eq!(created["resolution"], "owner_declared");
    assert_eq!(ledger_rows(&c.pool).await, rows, "proposing moves no money");

    let accepted = ok(&c.app, &step(&id, "accept"), json!({})).await;
    assert_eq!(accepted["state"], "accepted");
    assert_eq!(accepted["actions"], json!(["fund", "cancel"]));

    let funded = ok(&c.app, &step(&id, "fund"), json!({})).await;
    assert_eq!(funded["state"], "funded");
    assert!(funded["funded_at"].is_string());
    assert_eq!(available(&c.pool, &a).await, 47 * USDC);
    assert_eq!(committed(&c.pool, &a).await, 3 * USDC);
    assert_eq!(committed(&c.pool, &b).await, 5 * USDC);
    // The agent views show the contribution as committed capital.
    let view = get(&c.app, &format!("/api/agents/{a}")).await;
    assert_eq!(view["balances"]["committed"], "3000000");

    let done = ok(&c.app, &step(&id, "complete"), json!({})).await;
    assert_eq!(done["state"], "completed");
    assert_eq!(done["actions"], json!([]));
    assert_eq!(done["proposer"]["payout"], "4800000");
    assert_eq!(done["partner"]["payout"], "3200000");
    assert_eq!(done["proposer"]["net"], "1800000");
    assert_eq!(done["partner"]["net"], "-1800000");
    assert_eq!(available(&c.pool, &a).await, 47 * USDC + 4_800_000);
    assert_eq!(available(&c.pool, &b).await, 45 * USDC + 3_200_000);
    assert_eq!(
        committed(&c.pool, &a).await + committed(&c.pool, &b).await,
        0
    );
    assert_eq!(balance(&c.pool, &format!("escrow:{id}")).await, 0);
    assert_ledger_sound(&c.pool).await;

    let detail = get(&c.app, &format!("/api/collaborations/{id}")).await;
    let reasons: Vec<&str> = detail["ledger"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["reason"].as_str().unwrap())
        .collect();
    assert!(reasons.contains(&"collab_commit") && reasons.contains(&"collab_payout"));
    assert!(
        detail["ledger"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["collaboration_id"] == id.as_str() && e["interaction_id"].is_null())
    );
    assert_eq!(
        kinds(&c, &id).await,
        [
            "collaboration_proposed",
            "collaboration_accepted",
            "collaboration_funded",
            "collaboration_funded",
            "collaboration_completed",
            "collaboration_completed"
        ]
    );
    let completed = &detail["activity"][4];
    assert!(
        completed["summary"]
            .as_str()
            .unwrap()
            .contains("received 4.80 USDC (simulated) for a 3.00 USDC (simulated) contribution (net +1.80 USDC (simulated))"),
        "{completed}"
    );
    // The general activity feed carries it too, filterable by agent or collaboration.
    let feed = get(&c.app, &format!("/api/activity?collaboration_id={id}")).await;
    assert_eq!(feed.as_array().unwrap().len(), 6);
    let mine = get(&c.app, &format!("/api/activity?agent_id={b}")).await;
    assert!(
        mine.as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "collaboration_completed" && i["collaboration_id"] == id.as_str())
    );
    // Listing and filters.
    let list = get(
        &c.app,
        &format!("/api/collaborations?agent_id={b}&state=completed"),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let none = get(&c.app, "/api/collaborations?state=funded").await;
    assert!(none.as_array().unwrap().is_empty());
    let (s, _) = send(&c.app, "GET", "/api/collaborations?state=nope", None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Challenge P&L is unaffected by collaboration payouts.
    assert_eq!(view["stats"]["realized_pnl"], "0");
}

#[tokio::test]
async fn the_split_rounds_without_creating_or_losing_money() {
    let c = setup().await;
    let a = agent(&c, "A", 50, json!({})).await;
    let b = agent(&c, "B", 50, json!({})).await;
    // (proposer bps, contributions in minor units, expected proposer payout)
    for (bps, ca, cb, expect_a) in [
        (5000, 1, 2, 1),               // pool 3: 1.5 -> 1, partner gets 2
        (3333, 1_000_001, 1, 333_300), // floor(1_000_002 * 0.3333)
        (6667, 2, 1, 2),               // floor(3 * 0.6667) = 2
        (0, 7, 3, 0),                  // proposer gets nothing: no posting for it
        (10_000, 7, 3, 10),            // proposer gets all
    ] {
        let before = available(&c.pool, &a).await + available(&c.pool, &b).await;
        let id = create(&c, proposal(&a, &b, bps, ca, cb)).await;
        ok(&c.app, &step(&id, "accept"), json!({})).await;
        ok(&c.app, &step(&id, "fund"), json!({})).await;
        let done = ok(&c.app, &step(&id, "complete"), json!({})).await;
        let pa: i64 = done["proposer"]["payout"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let pb: i64 = done["partner"]["payout"].as_str().unwrap().parse().unwrap();
        assert_eq!(pa, expect_a, "bps {bps}");
        assert_eq!(pa + pb, ca + cb, "bps {bps}: money created or lost");
        let after = available(&c.pool, &a).await + available(&c.pool, &b).await;
        assert_eq!(after, before, "bps {bps}: total available changed");
        assert_eq!(
            committed(&c.pool, &a).await + committed(&c.pool, &b).await,
            0
        );
        assert_eq!(balance(&c.pool, &format!("escrow:{id}")).await, 0);
    }
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn over_limit_contributions_are_denied_at_every_step() {
    let c = setup().await;
    let a = agent(&c, "A", 50, json!({})).await;
    let b = agent(&c, "B", 50, json!({})).await;
    let rows = ledger_rows(&c.pool).await;

    // Proposer above its per-interaction limit (10): nothing is stored.
    let (s, out) = post(
        &c.app,
        "/api/collaborations",
        proposal(&a, &b, 5000, 11 * USDC, USDC),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{out}");
    assert_eq!(out["error"]["code"], "authorization_denied");
    assert_eq!(out["error"]["details"][0]["path"], "proposer");
    assert_eq!(
        out["error"]["details"][0]["code"],
        "exceeds_max_per_interaction"
    );
    assert!(
        get(&c.app, "/api/collaborations")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    let feed = get(&c.app, &format!("/api/activity?agent_id={a}")).await;
    assert_eq!(feed[0]["kind"], "collaboration_denied");

    // Partner above its limit: accept is denied, state stays proposed.
    let id = create(&c, proposal(&a, &b, 5000, USDC, 12 * USDC)).await;
    let (s, out) = post(&c.app, &step(&id, "accept"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{out}");
    assert_eq!(out["error"]["details"][0]["path"], "partner");
    let detail = get(&c.app, &format!("/api/collaborations/{id}")).await;
    assert_eq!(detail["collaboration"]["state"], "proposed");
    assert!(
        detail["activity"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["kind"] == "collaboration_denied")
    );

    // Balances are re-checked at funding: the partner spent its funds after
    // accepting, so funding is denied and neither side is committed.
    let id = create(&c, proposal(&a, &b, 5000, 2 * USDC, 3 * USDC)).await;
    ok(&c.app, &step(&id, "accept"), json!({})).await;
    ok(
        &c.app,
        &format!("/api/agents/{b}/deallocate"),
        json!({ "amount": (49 * USDC).to_string() }),
    )
    .await;
    let (s, out) = post(&c.app, &step(&id, "fund"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{out}");
    assert_eq!(out["error"]["details"][0]["path"], "partner");
    assert_eq!(
        out["error"]["details"][0]["code"],
        "insufficient_available_balance"
    );
    assert_eq!(committed(&c.pool, &a).await, 0);
    assert_eq!(
        get(&c.app, &format!("/api/collaborations/{id}")).await["collaboration"]["state"],
        "accepted"
    );
    // Only the deallocation was posted since the start.
    assert_eq!(ledger_rows(&c.pool).await, rows + 2);
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn funded_collaborations_count_toward_aggregate_exposure() {
    let c = setup().await;
    let a = agent(
        &c,
        "A",
        50,
        json!({ "max_aggregate_exposure": (12 * USDC).to_string() }),
    )
    .await;
    let b = agent(&c, "B", 50, json!({})).await;
    let first = create(&c, proposal(&a, &b, 5000, 4 * USDC, 4 * USDC)).await;
    ok(&c.app, &step(&first, "accept"), json!({})).await;
    ok(&c.app, &step(&first, "fund"), json!({})).await;
    let second = create(&c, proposal(&a, &b, 5000, 4 * USDC, USDC)).await;
    ok(&c.app, &step(&second, "accept"), json!({})).await;
    // A third would take A to 12: still fine; a 5 USDC one to 13 is not.
    let third = create(&c, proposal(&a, &b, 5000, 5 * USDC, USDC)).await;
    ok(&c.app, &step(&third, "accept"), json!({})).await;
    ok(&c.app, &step(&second, "fund"), json!({})).await; // exposure 8
    let (s, out) = post(&c.app, &step(&third, "fund"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{out}");
    assert_eq!(
        out["error"]["details"][0]["code"],
        "exceeds_aggregate_exposure"
    );
    // Completing one frees the room.
    ok(&c.app, &step(&first, "complete"), json!({})).await;
    ok(&c.app, &step(&third, "fund"), json!({})).await;
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn contributions_above_the_threshold_need_the_owner() {
    let c = setup().await;
    let a = agent(&c, "A", 50, json!({})).await;
    let b = agent(&c, "B", 50, json!({})).await;
    let id = create(&c, proposal(&a, &b, 5000, 6 * USDC, 2 * USDC)).await;
    let proposed = get(&c.app, &format!("/api/collaborations/{id}")).await;
    assert!(
        proposed["activity"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("funding will need the owner's approval")
    );
    ok(&c.app, &step(&id, "accept"), json!({})).await;
    let (s, out) = post(&c.app, &step(&id, "fund"), json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT, "{out}");
    assert_eq!(out["error"]["code"], "approval_required");
    assert_eq!(out["error"]["details"][0]["path"], "proposer");
    assert_eq!(committed(&c.pool, &a).await, 0);
    let (s, _) = post(
        &c.app,
        &step(&id, "fund"),
        json!({ "owner_approved": "yes" }),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let funded = ok(
        &c.app,
        &step(&id, "fund"),
        json!({ "owner_approved": true }),
    )
    .await;
    assert_eq!(funded["state"], "funded");
    assert_eq!(committed(&c.pool, &a).await, 6 * USDC);
    let detail = get(&c.app, &format!("/api/collaborations/{id}")).await;
    let approved: Vec<bool> = detail["activity"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["kind"] == "collaboration_funded")
        .map(|a| a["detail"]["owner_approved"].as_bool().unwrap())
        .collect();
    assert_eq!(approved, [true, false]);
}

#[tokio::test]
async fn steps_in_the_wrong_state_are_refused() {
    let c = setup().await;
    let a = agent(&c, "A", 50, json!({})).await;
    let b = agent(&c, "B", 50, json!({})).await;
    let id = create(&c, proposal(&a, &b, 5000, USDC, USDC)).await;
    for action in ["fund", "complete"] {
        let (s, out) = post(&c.app, &step(&id, action), json!({})).await;
        assert_eq!(s, StatusCode::CONFLICT, "{action}: {out}");
        assert_eq!(out["error"]["code"], "invalid_state");
    }
    ok(&c.app, &step(&id, "accept"), json!({})).await;
    for action in ["accept", "reject", "complete"] {
        let (s, out) = post(&c.app, &step(&id, action), json!({})).await;
        assert_eq!(s, StatusCode::CONFLICT, "{action}: {out}");
    }
    // Rejected and cancelled are final.
    let rejected = create(&c, proposal(&a, &b, 5000, USDC, USDC)).await;
    let out = ok(&c.app, &step(&rejected, "reject"), json!({})).await;
    assert_eq!(out["state"], "rejected");
    assert!(out["closed_at"].is_string());
    for action in ["accept", "fund", "complete", "cancel"] {
        let (s, _) = post(&c.app, &step(&rejected, action), json!({})).await;
        assert_eq!(s, StatusCode::CONFLICT, "{action}");
    }
    // Unknown id, bad id, missing JSON.
    let (s, _) = post(
        &c.app,
        &step("00000000-0000-4000-8000-000000000000", "accept"),
        json!({}),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = post(&c.app, &step("nope", "accept"), json!({})).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = send(&c.app, "POST", &step(&id, "fund"), None).await;
    assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    // Invalid terms.
    let (s, out) = post(
        &c.app,
        "/api/collaborations",
        proposal(&a, &a, 10_001, 0, USDC),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let paths: Vec<&str> = out["error"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        ["proposer_share_bps", "proposer_contribution", "partner_id"]
    );
}

#[tokio::test]
async fn a_collaboration_completes_once_even_under_concurrency() {
    let c = setup().await;
    let a = agent(&c, "A", 50, json!({})).await;
    let b = agent(&c, "B", 50, json!({})).await;
    let id = create(&c, proposal(&a, &b, 7000, 2 * USDC, 2 * USDC)).await;
    ok(&c.app, &step(&id, "accept"), json!({})).await;
    ok(&c.app, &step(&id, "fund"), json!({})).await;
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let (app, path) = (c.app.clone(), step(&id, "complete"));
        tasks.push(tokio::spawn(
            async move { post(&app, &path, json!({})).await.0 },
        ));
    }
    let mut statuses = Vec::new();
    for t in tasks {
        statuses.push(t.await.unwrap());
    }
    assert_eq!(
        statuses.iter().filter(|s| **s == StatusCode::OK).count(),
        1,
        "{statuses:?}"
    );
    assert!(
        statuses
            .iter()
            .all(|s| *s == StatusCode::OK || *s == StatusCode::CONFLICT)
    );
    // And sequentially: a second completion or a cancel after completion is 409.
    for action in ["complete", "cancel"] {
        let (s, out) = post(&c.app, &step(&id, action), json!({})).await;
        assert_eq!(s, StatusCode::CONFLICT, "{action}");
        assert_eq!(out["error"]["code"], "invalid_state");
    }
    let payouts: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT idempotency_key) FROM ledger_entries WHERE reason = 'collab_payout'",
    )
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(payouts, 1);
    assert_eq!(available(&c.pool, &a).await, 48 * USDC + 2_800_000);
    assert_eq!(available(&c.pool, &b).await, 48 * USDC + 1_200_000);
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn cancelling_a_funded_collaboration_refunds_both_sides() {
    let c = setup().await;
    let a = agent(&c, "A", 50, json!({})).await;
    let b = agent(&c, "B", 50, json!({})).await;
    let id = create(&c, proposal(&a, &b, 5000, 3 * USDC, 4 * USDC)).await;
    ok(&c.app, &step(&id, "accept"), json!({})).await;
    ok(&c.app, &step(&id, "fund"), json!({})).await;
    let out = ok(
        &c.app,
        &step(&id, "cancel"),
        json!({ "reason": "objective dropped" }),
    )
    .await;
    assert_eq!(out["state"], "cancelled");
    assert_eq!(out["status_reason"], "objective dropped");
    assert_eq!(available(&c.pool, &a).await, 50 * USDC);
    assert_eq!(available(&c.pool, &b).await, 50 * USDC);
    assert_eq!(
        committed(&c.pool, &a).await + committed(&c.pool, &b).await,
        0
    );
    let k = kinds(&c, &id).await;
    assert_eq!(
        k.iter().filter(|k| *k == "collaboration_cancelled").count(),
        2
    );
    // An unfunded one is cancelled without touching the ledger.
    let rows = ledger_rows(&c.pool).await;
    let other = create(&c, proposal(&a, &b, 5000, USDC, USDC)).await;
    ok(&c.app, &step(&other, "cancel"), json!({})).await;
    assert_eq!(ledger_rows(&c.pool).await, rows);
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn policy_and_status_are_enforced() {
    let c = setup().await;
    let a = agent(
        &c,
        "A",
        50,
        json!({ "allowed_interaction_types": ["challenge"] }),
    )
    .await;
    let b = agent(&c, "B", 50, json!({})).await;
    let (s, out) = post(
        &c.app,
        "/api/collaborations",
        proposal(&a, &b, 5000, USDC, USDC),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        out["error"]["details"][0]["code"],
        "interaction_type_not_allowed"
    );

    // A partner whose policy disallows collaborations cannot accept.
    let id = create(&c, proposal(&b, &a, 5000, USDC, USDC)).await;
    let (s, out) = post(&c.app, &step(&id, "accept"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(out["error"]["details"][0]["path"], "partner");
    // A paused partner cannot accept either; a paused proposer cannot fund.
    let d = agent(&c, "D", 50, json!({})).await;
    let id = create(&c, proposal(&b, &d, 5000, USDC, USDC)).await;
    ok(
        &c.app,
        &format!("/api/agents/{d}/status"),
        json!({ "status": "paused" }),
    )
    .await;
    let (s, out) = post(&c.app, &step(&id, "accept"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(out["error"]["details"][0]["code"], "agent_paused");
    ok(
        &c.app,
        &format!("/api/agents/{d}/status"),
        json!({ "status": "active" }),
    )
    .await;
    ok(&c.app, &step(&id, "accept"), json!({})).await;
    ok(
        &c.app,
        &format!("/api/agents/{b}/status"),
        json!({ "status": "paused" }),
    )
    .await;
    let (s, out) = post(&c.app, &step(&id, "fund"), json!({})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(out["error"]["details"][0]["path"], "proposer");
    assert_eq!(out["error"]["details"][0]["code"], "agent_paused");
    ok(
        &c.app,
        &format!("/api/agents/{b}/status"),
        json!({ "status": "active" }),
    )
    .await;
    ok(&c.app, &step(&id, "fund"), json!({})).await;
    // Once funded, a pause or a policy change does not trap the money: the
    // payout follows the funded terms.
    ok(
        &c.app,
        &format!("/api/agents/{d}/status"),
        json!({ "status": "paused" }),
    )
    .await;
    let mut input = get(&c.app, &format!("/api/agents/{d}")).await;
    input["policy"]["allowed_interaction_types"] = json!(["challenge"]);
    let body = json!({
        "name": input["name"], "description": input["description"], "config": input["config"],
        "policy": input["policy"], "graph": input["graph"],
    });
    let (s, out) = send(&c.app, "PUT", &format!("/api/agents/{d}"), Some(body)).await;
    assert_eq!(s, StatusCode::OK, "{out}");
    let done = ok(&c.app, &step(&id, "complete"), json!({})).await;
    assert_eq!(done["state"], "completed");
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn the_seeded_demo_agents_can_collaborate() {
    let c = setup().await;
    let seed = ok(&c.app, "/api/dev/seed-demo", json!({})).await;
    let a = seed["agents"][0]["id"].as_str().unwrap().to_owned();
    let b = seed["agents"][1]["id"].as_str().unwrap().to_owned();
    let id = create(&c, proposal(&a, &b, 6000, 3 * USDC, 5 * USDC)).await;
    ok(&c.app, &step(&id, "accept"), json!({})).await;
    // The skeptic's automatic limit is 1.00 USDC: its 5.00 needs the owner.
    let (s, out) = post(&c.app, &step(&id, "fund"), json!({})).await;
    assert_eq!(s, StatusCode::CONFLICT, "{out}");
    assert_eq!(out["error"]["details"][0]["path"], "partner");
    ok(
        &c.app,
        &step(&id, "fund"),
        json!({ "owner_approved": true }),
    )
    .await;
    let done = ok(&c.app, &step(&id, "complete"), json!({})).await;
    assert_eq!(done["proposer"]["net"], "1800000");
    assert_eq!(available(&c.pool, &a).await, 100 * USDC + 1_800_000);
    assert_eq!(available(&c.pool, &b).await, 100 * USDC - 1_800_000);
    assert_ledger_sound(&c.pool).await;
}
