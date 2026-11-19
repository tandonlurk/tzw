//! Agent multiplier (D-73) against a real Postgres (`TEST_DATABASE_URL`,
//! default `aen_test`): ledger correctness, limits, variance, caps and the
//! sibling rule. Fake market and a passed-in clock, as in `lifecycle.rs`.

mod common;

use aen_server::lifecycle;
use aen_server::state::AppState;
use aen_server::{DEFAULT_CORS_ORIGIN, MIGRATOR, app_with_state};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration, SubsecRound, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};
use tower::ServiceExt;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

const USDC: i64 = 1_000_000;

struct Ctx {
    app: Router,
    state: AppState,
    pool: PgPool,
    fake: Arc<aen_server::market::fake::FakeMarket>,
    t0: DateTime<Utc>,
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
    let state = AppState::offline(pool.clone());
    let fake = state.fake_market.clone().expect("fake market");
    Ctx {
        app: app_with_state(state.clone(), DEFAULT_CORS_ORIGIN.parse().unwrap()),
        state,
        pool,
        fake,
        t0: Utc::now().trunc_subsecs(0),
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

async fn get(app: &Router, path: &str) -> Value {
    let (status, body) = send(app, "GET", path, None).await;
    assert_eq!(status, StatusCode::OK, "GET {path}: {body}");
    body
}

async fn post(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(app, "POST", path, Some(body)).await
}

async fn ok(app: &Router, path: &str, body: Value) -> Value {
    let (status, out) = post(app, path, body).await;
    assert!(status.is_success(), "POST {path}: {status} {out}");
    out
}

async fn deposit(c: &Ctx, usdc: i64) {
    ok(
        &c.app,
        "/api/wallet/simulated-deposit",
        json!({ "amount": (usdc * USDC).to_string() }),
    )
    .await;
}

/// An agent from a starter template with `funds` USDC allocated.
async fn from_template(c: &Ctx, template: &str, funds: i64) -> String {
    let templates = get(&c.app, "/api/templates").await;
    let t = templates
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == template)
        .unwrap();
    let agent = ok(
        &c.app,
        "/api/agents",
        json!({ "name": format!("{template} (simulated)"), "description": "test",
                "config": t["config"], "policy": t["policy"], "graph": t["graph"] }),
    )
    .await;
    let id = agent["id"].as_str().unwrap().to_owned();
    deposit(c, funds).await;
    ok(
        &c.app,
        &format!("/api/agents/{id}/allocate"),
        json!({ "amount": (funds * USDC).to_string() }),
    )
    .await;
    id
}

async fn wallet(c: &Ctx) -> Value {
    get(&c.app, "/api/wallet").await
}

fn units(v: &Value) -> i64 {
    v.as_str().unwrap().parse().unwrap()
}

async fn agent_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM agents")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every posting set balances, nothing but `external` is negative, and the
/// owner's simulated money (unallocated + every agent's balances) equals the
/// simulated deposits.
async fn assert_ledger_sound(pool: &PgPool) {
    let total: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(total, 0, "ledger does not sum to zero");
    let unbalanced: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (SELECT 1 FROM ledger_entries GROUP BY idempotency_key \
         HAVING SUM(delta) <> 0) x",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(unbalanced, 0);
    let negative: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (SELECT 1 FROM ledger_entries WHERE account <> 'external' \
         GROUP BY account HAVING SUM(delta) < 0) x",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(negative, 0);
}

async fn external(pool: &PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries WHERE account = 'external'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn multiply_moves_owner_money_to_clones_and_creates_none() {
    let c = setup().await;
    let bull = from_template(&c, "Momentum bull", 10).await;
    deposit(&c, 100).await;
    let external_before = external(&c.pool).await;
    assert_eq!(units(&wallet(&c).await["unallocated"]), 100 * USDC);

    // 55 USDC over 10 clones = 5.50 each, nothing left over.
    let report = ok(
        &c.app,
        &format!("/api/agents/{bull}/multiply"),
        json!({ "count": 10, "variance_bps": 1000, "budget": "55000000", "seed": 7 }),
    )
    .await;
    assert_eq!(report["dry_run"], false);
    assert_eq!(report["env"], "simulated");
    assert_eq!(report["per_clone"], "5500000");
    assert_eq!(report["total"], "55000000");
    assert_eq!(report["remainder"], "0");
    assert_eq!(report["family_id"], bull.as_str());
    let clones = report["clones"].as_array().unwrap();
    assert_eq!(clones.len(), 10);
    assert_eq!(clones[0]["name"], "Momentum bull #1 (simulated)");
    assert_eq!(clones[9]["name"], "Momentum bull #10 (simulated)");

    let w = wallet(&c).await;
    assert_eq!(units(&w["unallocated"]), 45 * USDC);
    assert_eq!(units(&w["allocated"]), 65 * USDC);
    // No money entered from outside: deposits are unchanged.
    assert_eq!(external(&c.pool).await, external_before);
    for clone in clones {
        let a = get(
            &c.app,
            &format!("/api/agents/{}", clone["agent_id"].as_str().unwrap()),
        )
        .await;
        assert_eq!(a["balances"]["available"], "5500000");
        assert_eq!(a["family_id"], bull.as_str());
        assert_eq!(a["clone"]["source_agent_id"], bull.as_str());
        assert_eq!(a["clone"]["seed"], 7);
        assert_eq!(a["env"], "simulated");
        assert_eq!(a["version"], 1);
    }
    let root = get(&c.app, &format!("/api/agents/{bull}")).await;
    assert_eq!(root["family_id"], bull.as_str());
    assert_eq!(root["clone"], Value::Null);
    assert_eq!(root["balances"]["available"], "10000000");

    // An uneven split leaves the remainder unallocated.
    let report = ok(
        &c.app,
        &format!("/api/agents/{bull}/multiply"),
        json!({ "count": 3, "variance_bps": 0, "budget": "10000000" }),
    )
    .await;
    assert_eq!(report["per_clone"], "3333333");
    assert_eq!(report["remainder"], "1");
    assert_eq!(report["clones"][0]["name"], "Momentum bull #11 (simulated)");
    assert_eq!(
        units(&wallet(&c).await["unallocated"]),
        45 * USDC - 9_999_999
    );
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn insufficient_funds_are_refused_and_nothing_is_written() {
    let c = setup().await;
    let bull = from_template(&c, "Momentum bull", 10).await;
    deposit(&c, 20).await;
    let agents = agent_count(&c.pool).await;
    let (status, body) = post(
        &c.app,
        &format!("/api/agents/{bull}/multiply"),
        json!({ "count": 10, "variance_bps": 500, "budget": "20000010" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "insufficient_funds");
    assert_eq!(agent_count(&c.pool).await, agents);
    assert_eq!(units(&wallet(&c).await["unallocated"]), 20 * USDC);

    // A dry run reports the shortfall instead of failing, and writes nothing.
    let plan = ok(
        &c.app,
        &format!("/api/agents/{bull}/multiply"),
        json!({ "count": 10, "variance_bps": 500, "budget": "20000010", "dry_run": true }),
    )
    .await;
    assert_eq!(plan["funds_ok"], false);
    assert_eq!(plan["multiplication_id"], Value::Null);
    assert_eq!(plan["unallocated"], "20000000");
    assert_eq!(agent_count(&c.pool).await, agents);

    // Exactly the balance is fine.
    ok(
        &c.app,
        &format!("/api/agents/{bull}/multiply"),
        json!({ "count": 10, "variance_bps": 500, "budget": "20000000" }),
    )
    .await;
    assert_eq!(units(&wallet(&c).await["unallocated"]), 0);
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn variance_never_loosens_limits_and_is_reproducible() {
    let c = setup().await;
    let bull = from_template(&c, "Momentum bull", 10).await;
    deposit(&c, 100).await;
    let source = get(&c.app, &format!("/api/agents/{bull}")).await;
    let body = json!({ "count": 25, "variance_bps": 2500, "budget": "25000000", "seed": 1234 });

    let mut plan_body = body.clone();
    plan_body["dry_run"] = json!(true);
    let plan = ok(&c.app, &format!("/api/agents/{bull}/multiply"), plan_body).await;
    let made = ok(&c.app, &format!("/api/agents/{bull}/multiply"), body).await;
    // Same seed and indexes: the dry run predicted exactly what was made.
    for (p, m) in plan["clones"]
        .as_array()
        .unwrap()
        .iter()
        .zip(made["clones"].as_array().unwrap())
    {
        assert_eq!(p["params"], m["params"]);
        assert_eq!(p["name"], m["name"]);
    }

    let money = |v: &Value| units(v);
    let sp = &source["policy"];
    let mut distinct = std::collections::HashSet::new();
    for clone in made["clones"].as_array().unwrap() {
        let a = get(
            &c.app,
            &format!("/api/agents/{}", clone["agent_id"].as_str().unwrap()),
        )
        .await;
        let p = &a["policy"];
        assert!(money(&p["max_per_interaction"]) <= money(&sp["max_per_interaction"]));
        assert!(money(&p["max_aggregate_exposure"]) <= money(&sp["max_aggregate_exposure"]));
        assert!(money(&p["approval_threshold"]) <= money(&sp["approval_threshold"]));
        assert_eq!(p["stop_loss"], sp["stop_loss"]);
        assert_eq!(
            p["allowed_interaction_types"],
            sp["allowed_interaction_types"]
        );
        assert!(p["min_conviction_bps"].as_i64() >= sp["min_conviction_bps"].as_i64());
        // Money-valued graph params are copied as they are.
        let stake = |agent: &Value| {
            agent["graph"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["type"] == "cond.stake_lte")
                .unwrap()["params"]
                .clone()
        };
        assert_eq!(stake(&a), stake(&source));
        // The recorded variance is what the stored version holds, within 25 %.
        for change in a["clone"]["params"].as_array().unwrap() {
            let (from, to) = (
                change["from"].as_i64().unwrap(),
                change["to"].as_i64().unwrap(),
            );
            assert!(
                change["offset_bps"].as_i64().unwrap().abs() <= 2500,
                "{change}"
            );
            assert!(to * 4 >= from * 3 - 4 && to * 4 <= from * 5 + 4, "{change}");
        }
        let threshold = a["graph"]["nodes"][0]["params"]["threshold_bps"].clone();
        distinct.insert(threshold.to_string());
    }
    assert!(
        distinct.len() > 1,
        "25 clones at 25 % should not all be identical"
    );
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn caps_and_refusals() {
    let c = setup().await;
    let bull = from_template(&c, "Momentum bull", 1).await;
    deposit(&c, 1000).await;
    let path = format!("/api/agents/{bull}/multiply");
    for (body, field) in [
        (
            json!({ "count": 0, "variance_bps": 0, "budget": "1000000" }),
            "count",
        ),
        (
            json!({ "count": 101, "variance_bps": 0, "budget": "1000000000" }),
            "count",
        ),
        (
            json!({ "count": 10, "variance_bps": 2501, "budget": "1000000" }),
            "variance_bps",
        ),
        (
            json!({ "count": 10, "variance_bps": 0, "budget": "9" }),
            "budget",
        ),
        (
            json!({ "count": 10, "variance_bps": 0, "budget": "0" }),
            "budget",
        ),
    ] {
        let (status, out) = post(&c.app, &path, body.clone()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body} {out}");
        assert_eq!(out["error"]["details"][0]["path"], field, "{out}");
    }
    let (status, _) = post(
        &c.app,
        "/api/agents/00000000-0000-4000-8000-000000000999/multiply",
        json!({ "count": 1, "variance_bps": 0, "budget": "1000000" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Family cap: 200 clones, counted across requests and across sources.
    let first = ok(
        &c.app,
        &path,
        json!({ "count": 100, "variance_bps": 0, "budget": "100000000" }),
    )
    .await;
    let clone = first["clones"][0]["agent_id"].as_str().unwrap().to_owned();
    // Multiplying a clone adds to the root's family.
    let second = ok(
        &c.app,
        &format!("/api/agents/{clone}/multiply"),
        json!({ "count": 100, "variance_bps": 0, "budget": "100000000" }),
    )
    .await;
    assert_eq!(second["family_id"], bull.as_str());
    assert_eq!(second["clones"][0]["index"], 101);
    let (status, out) = post(
        &c.app,
        &path,
        json!({ "count": 1, "variance_bps": 0, "budget": "1000000" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{out}");
    assert_eq!(out["error"]["code"], "family_full");

    // A paused source is refused.
    let skeptic = from_template(&c, "Skeptic", 1).await;
    ok(
        &c.app,
        &format!("/api/agents/{skeptic}/status"),
        json!({ "status": "paused" }),
    )
    .await;
    let (status, out) = post(
        &c.app,
        &format!("/api/agents/{skeptic}/multiply"),
        json!({ "count": 1, "variance_bps": 0, "budget": "1000000" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{out}");
    assert_eq!(out["error"]["code"], "source_paused");
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn siblings_never_challenge_each_other() {
    let c = setup().await;
    let bull = from_template(&c, "Momentum bull", 20).await;
    deposit(&c, 100).await;
    ok(
        &c.app,
        &format!("/api/agents/{bull}/multiply"),
        json!({ "count": 3, "variance_bps": 0, "budget": "60000000" }),
    )
    .await;
    // Nothing but the bull's own family: nobody to challenge.
    c.fake.set_move_bps(300);
    c.fake.set_yes_price(Some(5500));
    let report = lifecycle::tick(&c.state, c.t0).await.unwrap();
    assert_eq!(report.proposed, 0, "{report:?}");
    let list = get(&c.app, "/api/interactions").await;
    assert!(list.as_array().unwrap().is_empty(), "{list}");

    // A skeptic family of the same size: every challenge crosses families.
    let skeptic = from_template(&c, "Skeptic", 20).await;
    deposit(&c, 60).await;
    ok(
        &c.app,
        &format!("/api/agents/{skeptic}/multiply"),
        json!({ "count": 3, "variance_bps": 0, "budget": "60000000" }),
    )
    .await;
    let agents = get(&c.app, "/api/agents").await;
    let family: HashMap<String, String> = agents
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a["id"].as_str().unwrap().to_owned(),
                a["family_id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    // Past the no-counterparty cooldown of the first tick.
    let later = c.t0 + Duration::seconds(700);
    let report = lifecycle::tick(&c.state, later).await.unwrap();
    assert_eq!(report.proposed, 4, "{report:?}");
    let list = get(&c.app, "/api/interactions").await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 4);
    let mut counterparties = std::collections::HashSet::new();
    for i in list {
        let (a, b) = (
            i["proposer_id"].as_str().unwrap(),
            i["counterparty_id"].as_str().unwrap(),
        );
        assert_eq!(family[a], bull, "{i}");
        assert_eq!(family[b], skeptic, "{i}");
        // One open challenge per agent per market still holds.
        assert!(counterparties.insert(b.to_owned()), "{b} twice");
    }
    assert_ledger_sound(&c.pool).await;
}
