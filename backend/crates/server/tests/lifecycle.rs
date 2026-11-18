//! Challenge lifecycle tests against a real Postgres (`TEST_DATABASE_URL`,
//! default `aen_test`). Market data is the deterministic fake and the clock is
//! passed in, so nothing here touches the network or sleeps.

mod common;

use std::sync::Arc;

use aen_core::challenge::{ChallengeTerms, Direction, FormulaId, MarketRef, Outcome};
use aen_core::ledger;
use aen_core::money::Money;
use aen_server::lifecycle::{self, CommitOutcome};
use aen_server::market::fake::{FAKE_EVENT_TICKER, FAKE_INSTRUMENT, FakeMarket};
use aen_server::market::{BoxFut, MarketError, Observation};
use aen_server::reasoner::{ReasonRequest, Reasoner, ReasonerError, ReasonerOutput};
use aen_server::state::AppState;
use aen_server::{DEFAULT_CORS_ORIGIN, MIGRATOR, app_with_state, ledger_db};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration, SubsecRound, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::{Mutex, MutexGuard};
use tower::ServiceExt;
use uuid::Uuid;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

const USDC: i64 = 1_000_000;

struct Ctx {
    app: Router,
    state: AppState,
    pool: PgPool,
    fake: Arc<FakeMarket>,
    /// Test clock origin.
    t0: DateTime<Utc>,
    _guard: MutexGuard<'static, ()>,
}

async fn pool() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| match std::env::var("USER") {
        Ok(user) => format!("postgres://{user}@localhost/aen_test"),
        Err(_) => "postgres://localhost/aen_test".to_owned(),
    });
    let pool = PgPool::connect(&url).await.expect("connect to test db");
    MIGRATOR.run(&pool).await.expect("run migrations");
    common::wipe(&pool).await;
    pool
}

async fn setup() -> Ctx {
    setup_with(|state| state).await
}

async fn setup_with(adjust: impl FnOnce(AppState) -> AppState) -> Ctx {
    let guard = DB_LOCK.lock().await;
    let pool = pool().await;
    let state = adjust(AppState::offline(pool.clone()));
    let fake = state
        .fake_market
        .clone()
        .expect("offline state has a fake market");
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

async fn post(app: &Router, path: &str, body: Value) -> Value {
    let (status, out) = send(app, "POST", path, Some(body)).await;
    assert!(status.is_success(), "POST {path}: {status} {out}");
    out
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// Creates an agent from a seeded template, with policy overrides, and
/// allocates `funds` USDC of simulated money to it.
async fn agent_from_template(c: &Ctx, template: &str, policy: Value, funds: i64) -> Uuid {
    let templates = get(&c.app, "/api/templates").await;
    let t = templates
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == template)
        .unwrap_or_else(|| panic!("template {template}"));
    let mut merged = t["policy"].clone();
    for (k, v) in policy.as_object().unwrap() {
        merged[k] = v.clone();
    }
    let input = json!({
        "name": template, "description": t["description"], "config": t["config"],
        "policy": merged, "graph": t["graph"],
    });
    let agent = post(&c.app, "/api/agents", input).await;
    let id = uuid(&agent["id"]);
    fund(c, id, funds).await;
    id
}

async fn fund(c: &Ctx, id: Uuid, usdc: i64) {
    if usdc == 0 {
        return;
    }
    let amount = json!({ "amount": (usdc * USDC).to_string() });
    post(&c.app, "/api/wallet/simulated-deposit", amount.clone()).await;
    post(&c.app, &format!("/api/agents/{id}/allocate"), amount).await;
}

async fn bull(c: &Ctx, funds: i64) -> Uuid {
    agent_from_template(c, "Momentum bull", json!({}), funds).await
}

async fn skeptic(c: &Ctx, funds: i64) -> Uuid {
    agent_from_template(c, "Skeptic", json!({}), funds).await
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

async fn available(pool: &PgPool, agent: Uuid) -> i64 {
    balance(pool, &format!("agent:{agent}:available")).await
}

async fn committed(pool: &PgPool, agent: Uuid) -> i64 {
    balance(pool, &format!("agent:{agent}:committed")).await
}

/// The ledger as a whole sums to zero, every posting set sums to zero, and
/// no account except `external` is negative.
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
    assert_eq!(unbalanced, 0, "a posting set is unbalanced");
    let negative: Vec<String> = sqlx::query_scalar(
        "SELECT account FROM ledger_entries WHERE account <> 'external' GROUP BY account \
         HAVING SUM(delta) < 0",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert!(negative.is_empty(), "negative accounts: {negative:?}");
}

async fn ledger_rows(pool: &PgPool, interaction: Uuid, reason: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM ledger_entries WHERE interaction_id = $1 AND reason = $2",
    )
    .bind(interaction)
    .bind(reason)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn state_of(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar("SELECT state FROM interactions WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn activity_kinds(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT kind FROM activity ORDER BY seq")
        .fetch_all(pool)
        .await
        .unwrap()
}

fn count(kinds: &[String], kind: &str) -> usize {
    kinds.iter().filter(|k| *k == kind).count()
}

fn terms(c: &Ctx, stake_usdc: i64, market: MarketRef) -> ChallengeTerms {
    let formula = FormulaId::for_market(&market);
    ChallengeTerms {
        threshold: if formula == FormulaId::PmYesPriceBpsV1 {
            5500
        } else {
            65_000_000_000
        },
        market,
        direction: Direction::Above,
        start_ts: (c.t0 + Duration::seconds(300)).timestamp(),
        end_ts: (c.t0 + Duration::seconds(3900)).timestamp(),
        stake: Money::from_minor(stake_usdc * USDC).unwrap(),
        resolution_source: formula.source(),
        formula,
    }
}

fn fake_contract() -> MarketRef {
    MarketRef::GeminiPredictionContract {
        event_ticker: FAKE_EVENT_TICKER.into(),
        instrument_symbol: FAKE_INSTRUMENT.into(),
    }
}

fn spot(symbol: &str) -> MarketRef {
    MarketRef::GeminiSpot {
        symbol: symbol.into(),
    }
}

/// Inserts an interaction directly in `state`, as if negotiation had already
/// happened. Lets a test change the world between acceptance and commit.
async fn fixture(c: &Ctx, a: Uuid, b: Uuid, t: &ChallengeTerms, state: &str) -> Uuid {
    let id = Uuid::new_v4();
    let view = json!({
        "market": t.market, "title": "[fake] BTC price in one hour: Up", "category": "crypto",
        "value": 5500, "move_bps": 300, "window_secs": 3600, "spot_symbol": "btcusd",
        "source": "fake", "fallback_reason": null,
    });
    let turn = (state == "proposed").then_some(b);
    sqlx::query(
        "INSERT INTO interactions (id, kind, env, state, proposer_id, counterparty_id, \
         proposer_version, counterparty_version, terms, stake, market_key, market_title, \
         market_view, start_ts, end_ts, turn, rounds, created_at, updated_at) \
         VALUES ($1, 'challenge', 'simulated', $2, $3, $4, 1, 1, $5, $6, $7, \
         '[fake] BTC price in one hour: Up', $8, $9, $10, $11, 1, $12, $12)",
    )
    .bind(id)
    .bind(state)
    .bind(a)
    .bind(b)
    .bind(serde_json::to_value(t).unwrap())
    .bind(t.stake.minor())
    .bind(t.market.key())
    .bind(view)
    .bind(DateTime::from_timestamp(t.start_ts, 0).unwrap())
    .bind(DateTime::from_timestamp(t.end_ts, 0).unwrap())
    .bind(turn)
    .bind(c.t0)
    .execute(&c.pool)
    .await
    .unwrap();
    id
}

async fn try_commit(c: &Ctx, id: Uuid) -> CommitOutcome {
    let mut tx = c.pool.begin().await.unwrap();
    let outcome = lifecycle::try_commit(&mut tx, id, c.t0).await.unwrap();
    tx.commit().await.unwrap();
    outcome
}

async fn tick(c: &Ctx, at: DateTime<Utc>) -> lifecycle::TickReport {
    lifecycle::tick(&c.state, at).await.unwrap()
}

async fn only_interaction(c: &Ctx) -> Value {
    let list = get(&c.app, "/api/interactions").await;
    assert_eq!(list.as_array().unwrap().len(), 1, "{list}");
    list[0].clone()
}

async fn deny_reasons(pool: &PgPool, interaction: Uuid) -> Vec<String> {
    let details: Vec<Value> = sqlx::query_scalar(
        "SELECT detail FROM activity WHERE interaction_id = $1 AND kind = 'commit_denied'",
    )
    .bind(interaction)
    .fetch_all(pool)
    .await
    .unwrap();
    details
        .iter()
        .flat_map(|d| d["reasons"].as_array().cloned().unwrap_or_default())
        .map(|r| r.as_str().unwrap().to_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// Authorization at execution time
// ---------------------------------------------------------------------------

#[tokio::test]
async fn commit_is_denied_above_the_per_interaction_limit() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    // Template limit is 10 USDC per interaction; these terms ask for 20.
    let id = fixture(&c, a, b, &terms(&c, 20, fake_contract()), "accepted").await;

    assert_eq!(try_commit(&c, id).await, CommitOutcome::Denied);
    assert_eq!(state_of(&c.pool, id).await, "cancelled");
    assert_eq!(ledger_rows(&c.pool, id, "commit_stake").await, 0);
    let reasons = deny_reasons(&c.pool, id).await;
    assert_eq!(
        reasons,
        ["exceeds_max_per_interaction", "exceeds_max_per_interaction"]
    );
    for agent in [a, b] {
        assert_eq!(available(&c.pool, agent).await, 100 * USDC);
        assert_eq!(committed(&c.pool, agent).await, 0);
    }
    // Idempotent: a second attempt finds nothing to do.
    assert_eq!(try_commit(&c, id).await, CommitOutcome::Skipped);
}

#[tokio::test]
async fn aggregate_exposure_holds_across_two_simultaneous_challenges() {
    let c = setup().await;
    // 15 USDC total exposure: one 10 USDC challenge fits, two do not.
    let limits = json!({ "max_aggregate_exposure": "15000000", "approval_threshold": "10000000" });
    let a = agent_from_template(&c, "Momentum bull", limits, 100).await;
    let b = agent_from_template(
        &c,
        "Skeptic",
        json!({ "max_aggregate_exposure": "50000000" }),
        100,
    )
    .await;
    let first = fixture(&c, a, b, &terms(&c, 10, fake_contract()), "accepted").await;
    let second = fixture(&c, a, b, &terms(&c, 10, spot("btcusd")), "accepted").await;

    // Both commits race; the agent locks make them take turns.
    let (r1, r2) = tokio::join!(try_commit(&c, first), try_commit(&c, second));
    let mut outcomes = [r1, r2];
    outcomes.sort_by_key(|o| format!("{o:?}"));
    assert_eq!(outcomes, [CommitOutcome::Denied, CommitOutcome::Funded]);

    assert_eq!(committed(&c.pool, a).await, 10 * USDC);
    assert_eq!(committed(&c.pool, b).await, 10 * USDC);
    let (funded, denied) = if r1 == CommitOutcome::Funded {
        (first, second)
    } else {
        (second, first)
    };
    assert_eq!(state_of(&c.pool, funded).await, "active");
    assert_eq!(state_of(&c.pool, denied).await, "cancelled");
    assert_eq!(
        deny_reasons(&c.pool, denied).await,
        ["exceeds_aggregate_exposure"]
    );
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn insufficient_balance_denies_and_commits_neither_side() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 3).await);
    let id = fixture(&c, a, b, &terms(&c, 5, fake_contract()), "accepted").await;

    assert_eq!(try_commit(&c, id).await, CommitOutcome::Denied);
    assert_eq!(
        deny_reasons(&c.pool, id).await,
        ["insufficient_available_balance"]
    );
    // The side that could pay was not charged either.
    assert_eq!(available(&c.pool, a).await, 100 * USDC);
    assert_eq!(committed(&c.pool, a).await, 0);
    assert_eq!(ledger_rows(&c.pool, id, "commit_stake").await, 0);
}

#[tokio::test]
async fn paused_agent_cannot_commit() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = fixture(&c, a, b, &terms(&c, 5, fake_contract()), "accepted").await;
    // The owner pauses the agent after it agreed, before funds move.
    post(
        &c.app,
        &format!("/api/agents/{b}/status"),
        json!({ "status": "paused" }),
    )
    .await;

    assert_eq!(try_commit(&c, id).await, CommitOutcome::Denied);
    assert_eq!(deny_reasons(&c.pool, id).await, ["agent_paused"]);
    assert_eq!(committed(&c.pool, a).await, 0);
    assert_eq!(committed(&c.pool, b).await, 0);
}

#[tokio::test]
async fn policy_tightened_after_acceptance_is_enforced_at_commit() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = fixture(&c, a, b, &terms(&c, 5, fake_contract()), "accepted").await;
    // The owner lowers the limit below the agreed stake.
    let mut agent = get(&c.app, &format!("/api/agents/{a}")).await;
    agent["policy"]["max_per_interaction"] = json!("4000000");
    agent["policy"]["approval_threshold"] = json!("4000000");
    let (status, _) = send(&c.app, "PUT", &format!("/api/agents/{a}"), Some(agent)).await;
    assert_eq!(status, StatusCode::OK);

    assert_eq!(try_commit(&c, id).await, CommitOutcome::Denied);
    assert_eq!(
        deny_reasons(&c.pool, id).await,
        ["exceeds_max_per_interaction"]
    );
}

#[tokio::test]
async fn stale_approval_does_not_move_funds() {
    let c = setup().await;
    // Approval needed above 5 USDC; the stake is 8.
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = fixture(&c, a, b, &terms(&c, 8, fake_contract()), "accepted").await;
    assert_eq!(
        try_commit(&c, id).await,
        CommitOutcome::AwaitingApproval { newly_requested: 1 }
    );
    assert_eq!(state_of(&c.pool, id).await, "awaiting_approval");
    // Asking again does not create a second request.
    assert_eq!(
        try_commit(&c, id).await,
        CommitOutcome::AwaitingApproval { newly_requested: 0 }
    );
    let pending = get(&c.app, "/api/approvals").await;
    assert_eq!(pending.as_array().unwrap().len(), 1, "{pending}");
    assert_eq!(pending[0]["agent_id"], a.to_string());
    assert_eq!(pending[0]["stake"], "8000000");
    let approval = pending[0]["id"].as_str().unwrap().to_owned();

    // Between the request and the approval the owner withdraws the funds.
    post(
        &c.app,
        &format!("/api/agents/{a}/deallocate"),
        json!({ "amount": "95000000" }),
    )
    .await;

    // The approval is recorded, but authorization runs again and denies.
    let result = post(
        &c.app,
        &format!("/api/approvals/{approval}/approve"),
        json!({}),
    )
    .await;
    assert_eq!(result["approval"]["status"], "approved");
    assert_eq!(result["interaction"]["state"], "cancelled");
    assert_eq!(
        deny_reasons(&c.pool, id).await,
        ["insufficient_available_balance"]
    );
    assert_eq!(ledger_rows(&c.pool, id, "commit_stake").await, 0);
    assert_eq!(committed(&c.pool, a).await, 0);
    assert_eq!(committed(&c.pool, b).await, 0);

    // A decided approval cannot be used again.
    let (status, body) = send(
        &c.app,
        "POST",
        &format!("/api/approvals/{approval}/approve"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "approval_not_pending");
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn approval_after_the_funding_deadline_is_refused() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let mut t = terms(&c, 8, fake_contract());
    let id = fixture(&c, a, b, &t, "accepted").await;
    try_commit(&c, id).await;
    // Move the funding deadline into the past (the endpoint uses real time).
    t.start_ts = (c.t0 - Duration::seconds(10)).timestamp();
    sqlx::query("UPDATE interactions SET start_ts = $2 WHERE id = $1")
        .bind(id)
        .bind(DateTime::from_timestamp(t.start_ts, 0).unwrap())
        .execute(&c.pool)
        .await
        .unwrap();
    let approval = get(&c.app, "/api/approvals").await[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let (status, body) = send(
        &c.app,
        "POST",
        &format!("/api/approvals/{approval}/approve"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "approval_expired");
    assert_eq!(state_of(&c.pool, id).await, "cancelled");
    assert_eq!(
        get(&c.app, "/api/approvals?status=expired")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(committed(&c.pool, a).await, 0);
}

#[tokio::test]
async fn approval_endpoints_need_json_and_reject_cancels() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = fixture(&c, a, b, &terms(&c, 8, fake_contract()), "accepted").await;
    try_commit(&c, id).await;
    let approval = get(&c.app, "/api/approvals").await[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // A cross-site form post (no JSON content type) must not approve (SEC-01).
    let (status, _) = send(
        &c.app,
        "POST",
        &format!("/api/approvals/{approval}/approve"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(state_of(&c.pool, id).await, "awaiting_approval");

    let (status, _) = send(
        &c.app,
        "POST",
        &format!("/api/approvals/{}/approve", Uuid::new_v4()),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let result = post(
        &c.app,
        &format!("/api/approvals/{approval}/reject"),
        json!({}),
    )
    .await;
    assert_eq!(result["approval"]["status"], "rejected");
    assert_eq!(result["interaction"]["state"], "cancelled");
    assert_eq!(committed(&c.pool, a).await, 0);
    assert!(
        activity_kinds(&c.pool)
            .await
            .contains(&"approval_rejected".to_owned())
    );
}

// ---------------------------------------------------------------------------
// Atomic two-sided commit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn both_stakes_commit_together() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = fixture(&c, a, b, &terms(&c, 5, fake_contract()), "accepted").await;

    assert_eq!(try_commit(&c, id).await, CommitOutcome::Funded);
    assert_eq!(state_of(&c.pool, id).await, "active");
    for agent in [a, b] {
        assert_eq!(available(&c.pool, agent).await, 95 * USDC);
        assert_eq!(committed(&c.pool, agent).await, 5 * USDC);
    }
    assert_eq!(ledger_rows(&c.pool, id, "commit_stake").await, 4);
    // Committing again is a no-op.
    assert_eq!(try_commit(&c, id).await, CommitOutcome::Skipped);
    assert_eq!(ledger_rows(&c.pool, id, "commit_stake").await, 4);
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn a_failure_on_the_second_stake_rolls_back_the_first() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = fixture(&c, a, b, &terms(&c, 5, fake_contract()), "accepted").await;
    // Sabotage: a foreign row already claims B's stake for this interaction,
    // so B's posting hits the one-commit-per-interaction index after A's
    // posting has been written in the same transaction. (Both rows in one
    // transaction: the database refuses an unbalanced set at commit.)
    let mut rogue = c.pool.begin().await.unwrap();
    for (line, account, delta) in [
        (0, format!("agent:{b}:available"), -USDC),
        (1, format!("agent:{b}:committed"), USDC),
    ] {
        sqlx::query(
            "INSERT INTO ledger_entries (env, idempotency_key, line, account, agent_id, delta, \
             reason, interaction_id) VALUES ('simulated', 'rogue', $1, $2, $3, $4, \
             'commit_stake', $5)",
        )
        .bind(line)
        .bind(account)
        .bind(b)
        .bind(delta)
        .bind(id)
        .execute(&mut *rogue)
        .await
        .unwrap();
    }
    rogue.commit().await.unwrap();

    let mut tx = c.pool.begin().await.unwrap();
    let result = lifecycle::try_commit(&mut tx, id, c.t0).await;
    assert!(result.is_err(), "second stake should have failed");
    drop(tx); // rolled back

    assert_eq!(state_of(&c.pool, id).await, "accepted");
    // A's stake was written inside the failed transaction and is gone.
    assert_eq!(available(&c.pool, a).await, 100 * USDC);
    assert_eq!(committed(&c.pool, a).await, 0);
    assert_eq!(ledger_rows(&c.pool, id, "commit_stake").await, 2); // only the rogue set
    assert_ledger_sound(&c.pool).await;
}

// ---------------------------------------------------------------------------
// Settlement
// ---------------------------------------------------------------------------

async fn funded(c: &Ctx, a: Uuid, b: Uuid, stake: i64, market: MarketRef) -> Uuid {
    let id = fixture(c, a, b, &terms(c, stake, market), "accepted").await;
    assert_eq!(try_commit(c, id).await, CommitOutcome::Funded);
    id
}

fn after_end(c: &Ctx) -> DateTime<Utc> {
    c.t0 + Duration::seconds(3901)
}

#[tokio::test]
async fn settlement_pays_the_winner_and_keeps_the_ledger_balanced() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = funded(&c, a, b, 5, fake_contract()).await;

    // Not due yet: nothing happens.
    let report = tick(&c, c.t0 + Duration::seconds(3899)).await;
    assert_eq!((report.resolving, report.settled), (0, 0));
    assert_eq!(state_of(&c.pool, id).await, "active");

    // YES price 0.40 < 0.55 threshold: "above" (A) loses, B wins.
    c.fake.set_yes_price(Some(4000));
    let report = tick(&c, after_end(&c)).await;
    assert_eq!((report.resolving, report.settled, report.voided), (1, 1, 0));
    assert_eq!(state_of(&c.pool, id).await, "settled");

    // Winner +stake, loser -stake, committed back to zero.
    assert_eq!(available(&c.pool, b).await, 105 * USDC);
    assert_eq!(available(&c.pool, a).await, 95 * USDC);
    assert_eq!(committed(&c.pool, a).await, 0);
    assert_eq!(committed(&c.pool, b).await, 0);
    assert_eq!(balance(&c.pool, &format!("escrow:{id}")).await, 0);
    assert_ledger_sound(&c.pool).await;

    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(detail["outcome"], "b");
    assert_eq!(detail["winner_id"], b.to_string());
    assert_eq!(detail["observed"], 4000);
    assert_eq!(detail["resolution"], "backend_administered");

    // Realized P&L counts the settlement only: deposits, allocations and
    // stake commits are transfers, not profit.
    let agents = get(&c.app, "/api/agents").await;
    let stat = |id: Uuid, field: &str| {
        let agent = agents
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == id.to_string())
            .unwrap();
        agent["stats"][field].clone()
    };
    assert_eq!(stat(b, "realized_pnl"), "5000000");
    assert_eq!(stat(a, "realized_pnl"), "-5000000");
    assert_eq!((stat(b, "settled"), stat(b, "wins")), (json!(1), json!(1)));
    assert_eq!((stat(a, "settled"), stat(a, "wins")), (json!(1), json!(0)));

    let portfolio = get(&c.app, "/api/stats/portfolio").await;
    let totals = &portfolio["totals"];
    assert_eq!(totals["realized_pnl"], "0"); // same owner on both sides
    assert_eq!(totals["allocated"], "200000000");
    assert_eq!(totals["committed"], "0");
    assert_eq!(totals["unrealized_pnl"], Value::Null);
    assert_eq!(portfolio["outcomes"]["settled"], 1);
    assert_eq!(portfolio["pnl_over_time"].as_array().unwrap().len(), 2);
    assert_eq!(portfolio["pnl_over_time"][1]["portfolio_cumulative"], "0");
    let perf = portfolio["agents"].as_array().unwrap();
    let winner = perf
        .iter()
        .find(|p| p["agent_id"] == b.to_string())
        .unwrap();
    assert_eq!(winner["win_rate_bps"], 10_000);

    // One environment per response: nothing simulated shows up as testnet.
    assert_eq!(
        get(&c.app, "/api/stats/portfolio?env=simulated").await["totals"],
        portfolio["totals"]
    );
    let arc = get(&c.app, "/api/stats/portfolio?env=arc_testnet").await;
    assert_eq!(arc["env"], "arc_testnet");
    assert_eq!(arc["totals"]["allocated"], "0");
    assert_eq!(arc["outcomes"]["settled"], 0);
    assert!(arc["pnl_over_time"].as_array().unwrap().is_empty());
    assert!(arc["agents"].as_array().unwrap().is_empty());
    let (status, _) = send(&c.app, "GET", "/api/stats/portfolio?env=mainnet", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // A later tick changes nothing.
    let report = tick(&c, after_end(&c) + Duration::seconds(60)).await;
    assert_eq!(report.settled, 0);
    assert_eq!(ledger_rows(&c.pool, id, "settle").await, 6);
}

#[tokio::test]
async fn exact_threshold_goes_to_the_above_side() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = funded(&c, a, b, 5, fake_contract()).await;
    c.fake.set_yes_price(Some(5500)); // exactly the threshold
    tick(&c, after_end(&c)).await;
    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(detail["outcome"], "a");
    assert_eq!(available(&c.pool, a).await, 105 * USDC);
}

#[tokio::test]
async fn settling_twice_is_rejected() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = funded(&c, a, b, 5, fake_contract()).await;
    // A second, unrelated challenge whose stakes a double settle would eat.
    let other = funded(&c, a, b, 5, spot("btcusd")).await;
    c.fake.set_yes_price(Some(9000));
    sqlx::query("UPDATE interactions SET state = 'resolving' WHERE id = $1")
        .bind(id)
        .execute(&c.pool)
        .await
        .unwrap();
    let observation = || -> Result<Observation, MarketError> {
        Ok(Observation {
            value: 9000,
            source: "fake",
            detail: "test".into(),
        })
    };
    let settle = |obs| async {
        let mut tx = c.pool.begin().await.unwrap();
        let out = lifecycle::settle_locked(&mut tx, &c.state.settings, id, obs, after_end(&c))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        out
    };

    // Two settlements race: the row lock lets exactly one through.
    let (first, second) = tokio::join!(settle(observation()), settle(observation()));
    let mut results = [first, second];
    results.sort_by_key(|r| r.is_none());
    assert_eq!(results, [Some(Outcome::A), None]);
    assert_eq!(settle(observation()).await, None);
    assert_eq!(ledger_rows(&c.pool, id, "settle").await, 6);

    // The ledger itself replays the derived key instead of paying again...
    let stake = Money::from_minor(5 * USDC).unwrap();
    let set = ledger::settle(ledger::Env::Simulated, id, a, b, stake).unwrap();
    let mut tx = c.pool.begin().await.unwrap();
    assert_eq!(
        ledger_db::post(&mut tx, &set).await.unwrap(),
        ledger_db::PostOutcome::Replayed
    );
    tx.commit().await.unwrap();

    // ...and a writer that invents its own key is stopped by the index.
    let rogue = sqlx::query(
        "INSERT INTO ledger_entries (env, idempotency_key, line, account, agent_id, delta, \
         reason, interaction_id) VALUES ('simulated', 'settle-again', 0, $1, $2, $3, 'settle', $4)",
    )
    .bind(format!("agent:{b}:committed"))
    .bind(b)
    .bind(-5 * USDC)
    .bind(id)
    .execute(&c.pool)
    .await;
    assert!(
        rogue.is_err(),
        "second settle of one interaction was accepted"
    );

    // The other challenge's stakes are intact.
    assert_eq!(state_of(&c.pool, other).await, "active");
    assert_eq!(committed(&c.pool, a).await, 5 * USDC);
    assert_eq!(committed(&c.pool, b).await, 5 * USDC);
    assert_eq!(available(&c.pool, a).await, 100 * USDC); // 100 - 5 - 5 + 10
    assert_eq!(available(&c.pool, b).await, 90 * USDC);
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn missing_market_data_voids_and_refunds_after_the_grace_period() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = funded(&c, a, b, 5, fake_contract()).await;
    c.fake.set_fail(true);

    // Within the grace period: keep the stakes locked and retry.
    for offset in [0, 60, 120] {
        let report = tick(&c, after_end(&c) + Duration::seconds(offset)).await;
        assert_eq!((report.settled, report.voided), (0, 0));
        assert!(!report.errors.is_empty(), "the failure must be surfaced");
    }
    assert_eq!(state_of(&c.pool, id).await, "resolving");
    assert_eq!(committed(&c.pool, a).await, 5 * USDC);
    let kinds = activity_kinds(&c.pool).await;
    assert_eq!(
        count(&kinds, "resolution_delayed"),
        1,
        "logged once, not per tick"
    );

    // Grace (600 s) over: void, refund both, no P&L.
    let report = tick(&c, after_end(&c) + Duration::seconds(600)).await;
    assert_eq!((report.settled, report.voided), (0, 1));
    assert_eq!(state_of(&c.pool, id).await, "cancelled");
    for agent in [a, b] {
        assert_eq!(available(&c.pool, agent).await, 100 * USDC);
        assert_eq!(committed(&c.pool, agent).await, 0);
    }
    assert_eq!(ledger_rows(&c.pool, id, "refund").await, 4);
    assert_eq!(ledger_rows(&c.pool, id, "settle").await, 0);
    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(detail["outcome"], "void");
    assert_eq!(detail["winner_id"], Value::Null);
    let portfolio = get(&c.app, "/api/stats/portfolio").await;
    assert_eq!(portfolio["outcomes"]["voided"], 1);
    assert_eq!(portfolio["outcomes"]["settled"], 0);
    assert!(portfolio["pnl_over_time"].as_array().unwrap().is_empty());
    for agent in portfolio["agents"].as_array().unwrap() {
        assert_eq!(agent["realized_pnl"], "0");
        assert_eq!(agent["settled"], 0);
        assert_eq!(agent["win_rate_bps"], Value::Null);
    }
    assert_ledger_sound(&c.pool).await;

    // Data coming back later cannot settle a voided challenge.
    c.fake.set_fail(false);
    let report = tick(&c, after_end(&c) + Duration::seconds(700)).await;
    assert_eq!((report.settled, report.voided), (0, 0));
}

#[tokio::test]
async fn a_contract_without_a_price_is_missing_data_not_zero() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = funded(&c, a, b, 5, fake_contract()).await;
    c.fake.set_yes_price(None);
    tick(&c, after_end(&c)).await;
    // No price is not "price 0" (which would hand the win to the below side).
    assert_eq!(state_of(&c.pool, id).await, "resolving");
    assert_eq!(available(&c.pool, b).await, 95 * USDC);

    // The contract resolving on the exchange gives a definite observation.
    c.fake.set_resolution(Some("yes"));
    tick(&c, after_end(&c) + Duration::seconds(30)).await;
    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(
        (detail["state"].clone(), detail["observed"].clone()),
        (json!("settled"), json!(10_000))
    );
    assert_eq!(detail["outcome"], "a");
}

// ---------------------------------------------------------------------------
// Ticks: idempotent and safe when concurrent
// ---------------------------------------------------------------------------

#[tokio::test]
async fn concurrent_ticks_create_and_settle_exactly_once() {
    let c = setup().await;
    c.fake.set_move_bps(400); // the skeptic accepts outright
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);

    let ticks = || async {
        tokio::join!(
            tick(&c, c.t0),
            tick(&c, c.t0),
            tick(&c, c.t0),
            tick(&c, c.t0)
        )
    };
    let (r1, r2, r3, r4) = ticks().await;
    assert_eq!(r1.proposed + r2.proposed + r3.proposed + r4.proposed, 1);
    assert_eq!(r1.funded + r2.funded + r3.funded + r4.funded, 1);
    let interaction = only_interaction(&c).await;
    let id = uuid(&interaction["id"]);
    assert_eq!(interaction["state"], "active");
    assert_eq!(ledger_rows(&c.pool, id, "commit_stake").await, 4);
    assert_eq!(committed(&c.pool, a).await, 5 * USDC);
    assert_eq!(committed(&c.pool, b).await, 5 * USDC);

    // Repeating the tick is a no-op.
    let again = tick(&c, c.t0 + Duration::seconds(1)).await;
    assert_eq!((again.proposed, again.funded, again.accepted), (0, 0, 0));
    assert_eq!(
        get(&c.app, "/api/interactions")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let end: DateTime<Utc> = interaction["end_at"].as_str().unwrap().parse().unwrap();
    let due = end + Duration::seconds(1);
    c.fake.set_yes_price(Some(8000));
    let (r1, r2, r3, r4) = tokio::join!(tick(&c, due), tick(&c, due), tick(&c, due), tick(&c, due));
    assert_eq!(r1.settled + r2.settled + r3.settled + r4.settled, 1);
    assert_eq!(ledger_rows(&c.pool, id, "settle").await, 6);
    assert_eq!(available(&c.pool, a).await, 105 * USDC);
    assert_eq!(available(&c.pool, b).await, 95 * USDC);
    assert_ledger_sound(&c.pool).await;
    // The win frees the agents to find the next opportunity (same tick or a
    // later one); the settled challenge itself is never touched again.
    assert_eq!(state_of(&c.pool, id).await, "settled");
}

// ---------------------------------------------------------------------------
// Negotiation
// ---------------------------------------------------------------------------

/// A graph that never accepts: it counters whatever it receives.
fn haggler_graph() -> Value {
    json!({
        "schema_version": 1,
        "nodes": [
            {"id": "t", "type": "trigger.challenge_received", "params": {}},
            {"id": "e", "type": "reason.evaluate_proposal", "params": {}},
            {"id": "c", "type": "cond.conviction_gte", "params": {"min_bps": 10000}},
            {"id": "yes", "type": "action.accept", "params": {}},
            {"id": "no", "type": "action.counter", "params": {}}
        ],
        "edges": [
            {"from": "t", "from_port": "out", "to": "e", "to_port": "in"},
            {"from": "e", "from_port": "out", "to": "c", "to_port": "in"},
            {"from": "c", "from_port": "true", "to": "yes", "to_port": "in"},
            {"from": "c", "from_port": "false", "to": "no", "to_port": "in"}
        ]
    })
}

async fn haggler(c: &Ctx, name: &str) -> Uuid {
    let input = json!({
        "name": name, "description": "never satisfied",
        "config": {"goals": "g", "strategy": "haggle", "categories": ["crypto"],
                   "horizon_secs": 3600, "risk_tolerance": "medium", "schedule_secs": 60},
        "policy": {"max_per_interaction": "10000000", "max_aggregate_exposure": "50000000",
                   "approval_threshold": "10000000", "allowed_interaction_types": ["challenge"],
                   "min_conviction_bps": 6000, "stop_loss": null},
        "graph": haggler_graph(),
    });
    let id = uuid(&post(&c.app, "/api/agents", input).await["id"]);
    fund(c, id, 100).await;
    id
}

#[tokio::test]
async fn negotiation_is_capped() {
    let c = setup().await;
    let (a, b) = (
        haggler(&c, "Haggler A").await,
        haggler(&c, "Haggler B").await,
    );
    let id = fixture(&c, a, b, &terms(&c, 8, fake_contract()), "proposed").await;

    // Offer 1 is the proposal. B counters (offer 2), A counters (offer 3).
    let r = tick(&c, c.t0).await;
    assert_eq!(r.countered, 1);
    assert_eq!(state_of(&c.pool, id).await, "negotiating");
    let r = tick(&c, c.t0 + Duration::seconds(1)).await;
    assert_eq!(r.countered, 1);
    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(
        (detail["rounds"].clone(), detail["max_rounds"].clone()),
        (json!(3), json!(3))
    );
    assert_eq!(detail["stake"], "2000000"); // 8 -> 4 -> 2
    assert_eq!(detail["turn"], b.to_string());

    // A fourth offer is not allowed: the challenge is cancelled instead.
    let r = tick(&c, c.t0 + Duration::seconds(2)).await;
    assert_eq!((r.countered, r.cancelled), (0, 1));
    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(detail["state"], "cancelled");
    assert_eq!(detail["rounds"], 3);
    let actions: Vec<&str> = detail["negotiation"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["counter", "counter"]); // the fixture skipped the proposal row
    assert!(
        activity_kinds(&c.pool)
            .await
            .contains(&"negotiation_cap_reached".to_owned())
    );
    // Nothing was ever staked.
    let moved: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries WHERE interaction_id = $1")
            .bind(id)
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(moved, 0);

    // And further ticks leave it alone.
    let r = tick(&c, c.t0 + Duration::seconds(3)).await;
    assert_eq!((r.countered, r.cancelled), (0, 0));
}

#[tokio::test]
async fn unfunded_challenges_expire_at_the_funding_deadline() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let id = fixture(&c, a, b, &terms(&c, 8, fake_contract()), "accepted").await;
    try_commit(&c, id).await; // awaiting approval
    let r = tick(&c, c.t0 + Duration::seconds(300)).await;
    assert_eq!(r.cancelled, 0); // counted in the activity log, not as a denial
    assert_eq!(state_of(&c.pool, id).await, "cancelled");
    assert_eq!(
        get(&c.app, "/api/approvals")
            .await
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert!(
        activity_kinds(&c.pool)
            .await
            .contains(&"challenge_expired".to_owned())
    );
    assert_eq!(committed(&c.pool, a).await, 0);
}

// ---------------------------------------------------------------------------
// Failure of the outside world
// ---------------------------------------------------------------------------

#[tokio::test]
async fn market_data_failure_stops_discovery_and_is_reported_once() {
    let c = setup().await;
    let (_a, _b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    c.fake.set_fail(true);
    for offset in 0..3 {
        let report = tick(&c, c.t0 + Duration::seconds(offset)).await;
        assert_eq!(report.proposed, 0);
        assert!(report.errors.iter().any(|e| e.contains("market data")));
    }
    assert!(
        get(&c.app, "/api/interactions")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    let kinds = activity_kinds(&c.pool).await;
    assert_eq!(count(&kinds, "market_data_error"), 1);

    // Recovery: the next tick proposes.
    c.fake.set_fail(false);
    assert_eq!(tick(&c, c.t0 + Duration::seconds(10)).await.proposed, 1);
}

/// A reasoner whose output breaks the contract (conviction out of range).
struct Broken;

impl Reasoner for Broken {
    fn name(&self) -> &'static str {
        "broken"
    }
    fn reason<'a>(
        &'a self,
        _req: &'a ReasonRequest<'a>,
    ) -> BoxFut<'a, Result<ReasonerOutput, ReasonerError>> {
        Box::pin(async {
            Ok(ReasonerOutput {
                conviction_bps: 1_000_000,
                stance: Direction::Above,
                matches_strategy: true,
                terms: None,
                summary: "trust me".into(),
            })
        })
    }
}

#[tokio::test]
async fn malformed_reasoner_output_never_becomes_an_intent() {
    let c = setup_with(|mut state| {
        state.reasoner = Arc::new(Broken);
        state
    })
    .await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);

    let report = tick(&c, c.t0).await;
    assert_eq!(report.proposed, 0);
    assert!(
        report.errors.iter().any(|e| e.contains("rejected")),
        "{report:?}"
    );
    assert!(
        get(&c.app, "/api/interactions")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    let (status, error): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM agent_runs WHERE agent_id = $1")
            .bind(a)
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(status, "error");
    assert!(error.unwrap().contains("conviction_bps"));
    assert!(
        activity_kinds(&c.pool)
            .await
            .contains(&"reasoning_failed".to_owned())
    );

    // The same reasoner answering a challenge: no answer is applied and the
    // challenge stays where it was, with no funds moved.
    let id = fixture(&c, a, b, &terms(&c, 5, spot("ethusd")), "proposed").await;
    tick(&c, c.t0 + Duration::seconds(1)).await;
    assert_eq!(state_of(&c.pool, id).await, "proposed");
    assert_eq!(committed(&c.pool, b).await, 0);
}

#[tokio::test]
async fn agents_without_funds_or_paused_do_not_propose() {
    let c = setup().await;
    let a = bull(&c, 0).await;
    let _b = skeptic(&c, 100).await;
    assert_eq!(tick(&c, c.t0).await.proposed, 0);
    fund(&c, a, 100).await;
    post(
        &c.app,
        &format!("/api/agents/{a}/status"),
        json!({ "status": "paused" }),
    )
    .await;
    assert_eq!(tick(&c, c.t0).await.proposed, 0);
    // A small move does not fire the 200 bps trigger either.
    post(
        &c.app,
        &format!("/api/agents/{a}/status"),
        json!({ "status": "active" }),
    )
    .await;
    c.fake.set_move_bps(150);
    assert_eq!(tick(&c, c.t0).await.proposed, 0);
    let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_runs")
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(runs, 0);
}

#[tokio::test]
async fn proposal_denied_by_policy_is_explained_in_activity() {
    let c = setup().await;
    // The owner disallows challenges after building the agent.
    let a = agent_from_template(
        &c,
        "Momentum bull",
        json!({ "allowed_interaction_types": ["collaboration"] }),
        100,
    )
    .await;
    let _b = skeptic(&c, 100).await;
    let cooldown = c.state.settings.proposal_cooldown_secs;
    // Denied once, then rate-limited like a proposal; denied again (and
    // still logged once) after the cooldown.
    for (offset, denied) in [(0, 1), (1, 0), (cooldown, 1)] {
        let report = tick(&c, c.t0 + Duration::seconds(offset)).await;
        assert_eq!(
            (report.proposed, report.denied),
            (0, denied),
            "at {offset} s"
        );
    }
    let items = get(&c.app, &format!("/api/activity?agent_id={a}")).await;
    let denied: Vec<&Value> = items
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == "proposal_denied")
        .collect();
    assert_eq!(denied.len(), 1, "a persistent denial is logged once");
    assert!(
        denied[0]["summary"]
            .as_str()
            .unwrap()
            .contains("challenges are not allowed by its policy")
    );
    assert_eq!(
        denied[0]["detail"]["reasons"],
        json!(["interaction_type_not_allowed"])
    );
}

async fn discovery_runs(pool: &PgPool, agent: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_runs WHERE agent_id = $1 AND trigger = 'trigger.market_move'",
    )
    .bind(agent)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn a_denied_proposal_is_rate_limited_until_the_owner_edits_the_agent() {
    let c = setup().await;
    let cooldown = c.state.settings.proposal_cooldown_secs;
    let a = agent_from_template(
        &c,
        "Momentum bull",
        json!({ "allowed_interaction_types": ["collaboration"] }),
        100,
    )
    .await;
    let _b = skeptic(&c, 100).await;
    assert_eq!(tick(&c, c.t0).await.denied, 1);
    assert_eq!(discovery_runs(&c.pool, a).await, 1);
    let blocked: Option<String> =
        sqlx::query_scalar("SELECT blocked FROM agent_runs WHERE agent_id = $1")
            .bind(a)
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(blocked.as_deref(), Some("denied"));

    // Inside the cooldown the agent does not even reason (no run, no model call).
    for secs in [15, 300, cooldown - 1] {
        let r = tick(&c, c.t0 + Duration::seconds(secs)).await;
        assert_eq!((r.proposed, r.denied), (0, 0), "after {secs} s");
    }
    assert_eq!(discovery_runs(&c.pool, a).await, 1);

    // An owner edit is a new version: the agent may try again at once.
    let mut input = get(&c.app, &format!("/api/agents/{a}")).await;
    input["policy"]["allowed_interaction_types"] = json!(["challenge", "collaboration"]);
    let body = json!({
        "name": input["name"], "description": input["description"], "config": input["config"],
        "policy": input["policy"], "graph": input["graph"],
    });
    let (status, out) = send(&c.app, "PUT", &format!("/api/agents/{a}"), Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{out}");
    assert_eq!(tick(&c, c.t0 + Duration::seconds(16)).await.proposed, 1);
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn no_counterparty_is_rate_limited_too() {
    let c = setup().await;
    let cooldown = c.state.settings.proposal_cooldown_secs;
    let a = bull(&c, 100).await;
    tick(&c, c.t0).await;
    assert_eq!(discovery_runs(&c.pool, a).await, 1);
    for secs in [15, cooldown - 1] {
        tick(&c, c.t0 + Duration::seconds(secs)).await;
    }
    assert_eq!(discovery_runs(&c.pool, a).await, 1);
    assert_eq!(count(&activity_kinds(&c.pool).await, "no_counterparty"), 1);
    tick(&c, c.t0 + Duration::seconds(cooldown)).await;
    assert_eq!(discovery_runs(&c.pool, a).await, 2);

    // Disabled cooldown: the old behaviour, one attempt per tick.
    drop(c);
    let c = setup_with(|mut state| {
        state.settings.proposal_cooldown_secs = 0;
        state
    })
    .await;
    let a = bull(&c, 100).await;
    for secs in [0, 15] {
        tick(&c, c.t0 + Duration::seconds(secs)).await;
    }
    assert_eq!(discovery_runs(&c.pool, a).await, 2);
}

// ---------------------------------------------------------------------------
// The whole thing through HTTP
// ---------------------------------------------------------------------------

async fn http_tick(c: &Ctx, at: DateTime<Utc>) -> Value {
    post(&c.app, "/api/dev/tick", json!({ "now": at.to_rfc3339() })).await
}

#[tokio::test]
async fn full_lifecycle_through_http() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);

    // Tick 1: the bull sees +3% and proposes; the skeptic counters with half.
    let report = http_tick(&c, c.t0).await;
    assert_eq!(
        (report["proposed"].clone(), report["countered"].clone()),
        (json!(1), json!(1))
    );
    let interaction = only_interaction(&c).await;
    let id = uuid(&interaction["id"]);
    assert_eq!(interaction["state"], "negotiating");
    assert_eq!(interaction["proposer_id"], a.to_string());
    assert_eq!(interaction["counterparty_id"], b.to_string());
    assert_eq!(interaction["stake"], "2500000");
    assert_eq!(interaction["turn"], a.to_string());
    assert_eq!(interaction["market_source"], "fake");
    assert_eq!(
        interaction["terms"]["market"]["kind"],
        "gemini_prediction_contract"
    );
    assert_eq!(interaction["terms"]["formula"], "pm_yes_price_bps_v1");
    assert_eq!(interaction["terms"]["direction"], "above");
    assert_eq!(interaction["terms"]["threshold"], 5500);

    // Tick 2: the bull accepts the counter; both stakes are authorized and
    // committed in the same tick.
    let report = http_tick(&c, c.t0 + Duration::seconds(15)).await;
    assert_eq!(
        (report["accepted"].clone(), report["funded"].clone()),
        (json!(1), json!(1))
    );
    let agents = get(&c.app, "/api/agents").await;
    for agent in agents.as_array().unwrap() {
        assert_eq!(agent["balances"]["available"], "97500000");
        assert_eq!(agent["balances"]["committed"], "2500000");
    }
    let wallet = get(&c.app, "/api/wallet").await;
    assert_eq!(wallet["committed"], "5000000");
    let portfolio = get(&c.app, "/api/stats/portfolio").await;
    assert_eq!(portfolio["totals"]["at_risk"], "5000000");
    assert_eq!(portfolio["outcomes"]["active"], 1);
    let listed = get(
        &c.app,
        &format!("/api/interactions?agent_id={b}&state=active"),
    )
    .await;
    assert_eq!(listed.as_array().unwrap().len(), 1);

    // The market moves to 0.70; at end_ts the challenge resolves and settles.
    post(
        &c.app,
        "/api/dev/fake-market",
        json!({ "yes_price_bps": 7000 }),
    )
    .await;
    let end: DateTime<Utc> = interaction["end_at"].as_str().unwrap().parse().unwrap();
    let report = http_tick(&c, end + Duration::seconds(1)).await;
    assert_eq!(
        (report["resolving"].clone(), report["settled"].clone()),
        (json!(1), json!(1))
    );

    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(detail["state"], "settled");
    assert_eq!(detail["outcome"], "a");
    assert_eq!(detail["winner_id"], a.to_string());
    assert_eq!(detail["observed"], 7000);
    assert_eq!(detail["observed_display"], "0.70 (YES price)");
    let actions: Vec<&str> = detail["negotiation"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["propose", "counter", "accept"]);
    // Three runs, each with a trace of decision summaries and its intents.
    let runs = detail["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 3);
    for run in runs {
        assert_eq!(run["status"], "ok");
        assert_eq!(run["reasoner"], "rule_based");
        assert!(!run["trace"].as_array().unwrap().is_empty());
        assert_eq!(run["intents"].as_array().unwrap().len(), 1);
    }
    assert_eq!(runs[0]["intents"][0]["intent"]["type"], "propose_challenge");
    assert_eq!(runs[1]["intents"][0]["intent"]["type"], "counter_offer");
    assert_eq!(runs[2]["intents"][0]["intent"]["type"], "accept_challenge");
    assert_eq!(detail["ledger"].as_array().unwrap().len(), 4 + 6);

    let agents = get(&c.app, "/api/agents").await;
    let find = |id: Uuid| {
        agents
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == id.to_string())
            .unwrap()
            .clone()
    };
    assert_eq!(find(a)["balances"]["available"], "102500000");
    assert_eq!(find(a)["stats"]["realized_pnl"], "2500000");
    assert_eq!(find(a)["stats"]["wins"], 1);
    assert_eq!(find(b)["balances"]["available"], "97500000");
    assert_eq!(find(b)["stats"]["realized_pnl"], "-2500000");
    for agent in [a, b] {
        assert_eq!(find(agent)["balances"]["committed"], "0");
    }
    assert_eq!(get(&c.app, "/api/wallet").await["committed"], "0");
    assert_ledger_sound(&c.pool).await;

    // Activity tells the story in sentences, linked to the interaction.
    let items = get(
        &c.app,
        &format!("/api/activity?interaction_id={id}&limit=50"),
    )
    .await;
    let kinds: Vec<&str> = items
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .map(|i| i["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "challenge_proposed",
            "challenge_countered",
            "challenge_accepted",
            "stake_committed",
            "stake_committed",
            "challenge_settled",
            "challenge_settled",
        ]
    );
    for item in items.as_array().unwrap() {
        let summary = item["summary"].as_str().unwrap();
        assert!(
            summary.contains("Momentum bull") || summary.contains("Skeptic"),
            "{summary}"
        );
        assert!(
            !summary.contains(&id.to_string()),
            "raw id in a summary: {summary}"
        );
    }
    let settled = items[0]["summary"].as_str().unwrap();
    assert!(settled.contains("2.50 USDC (simulated)"), "{settled}");

    // The simulated clock is refused when it is not valid JSON time.
    let (status, _) = send(
        &c.app,
        "POST",
        "/api/dev/tick",
        Some(json!({ "now": "soon" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = send(&c.app, "POST", "/api/dev/tick", None).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let (status, _) = send(
        &c.app,
        "GET",
        &format!("/api/interactions/{}", Uuid::new_v4()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn owner_approval_flow_through_http() {
    let c = setup().await;
    // "Cautious": approval threshold 0 and a flow ending in request_approval.
    // Its approach (ambition 20 %, D-69) caps the stake at 36 % of the 2.00
    // max per interaction: 0.72 instead of 1.00.
    let a = agent_from_template(&c, "Cautious", json!({}), 100).await;
    let b = skeptic(&c, 100).await;
    c.fake.set_move_bps(700); // enough conviction for both

    let report = http_tick(&c, c.t0).await;
    assert_eq!(report["proposed"], 1);
    assert_eq!(report["accepted"], 1);
    assert_eq!(report["approvals_requested"], 1);
    assert_eq!(report["funded"], 0);
    let interaction = only_interaction(&c).await;
    let id = uuid(&interaction["id"]);
    assert_eq!(interaction["state"], "awaiting_approval");
    assert_eq!(interaction["pending_approvals"], 1);
    assert_eq!(interaction["stake"], "720000");
    assert_eq!(committed(&c.pool, a).await, 0);
    assert_eq!(
        committed(&c.pool, b).await,
        0,
        "no side is funded while one waits"
    );

    // Ticks do not fund it without the owner.
    let report = http_tick(&c, c.t0 + Duration::seconds(15)).await;
    assert_eq!(
        (
            report["funded"].clone(),
            report["approvals_requested"].clone()
        ),
        (json!(0), json!(0))
    );

    let approvals = get(&c.app, "/api/approvals").await;
    assert_eq!(approvals.as_array().unwrap().len(), 1);
    assert_eq!(approvals[0]["agent_name"], "Cautious");
    assert_eq!(approvals[0]["status"], "pending");
    let approval = approvals[0]["id"].as_str().unwrap();

    let result = post(
        &c.app,
        &format!("/api/approvals/{approval}/approve"),
        json!({}),
    )
    .await;
    assert_eq!(result["approval"]["status"], "approved");
    assert_eq!(result["interaction"]["state"], "active");
    assert_eq!(committed(&c.pool, a).await, 720_000);
    assert_eq!(committed(&c.pool, b).await, 720_000);
    assert!(
        get(&c.app, "/api/approvals")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        get(&c.app, "/api/approvals?status=all")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let kinds = activity_kinds(&c.pool).await;
    assert_eq!(count(&kinds, "approval_requested"), 1);
    assert_eq!(count(&kinds, "approval_granted"), 1);
    let _ = id;
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn spot_fallback_is_used_and_labelled_when_no_contract_is_usable() {
    let c = setup().await;
    c.fake.set_prediction_markets(false);
    c.fake.set_move_bps(400);
    let (_a, _b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    http_tick(&c, c.t0).await;
    let interaction = only_interaction(&c).await;
    assert_eq!(interaction["state"], "active");
    assert_eq!(interaction["terms"]["market"]["kind"], "gemini_spot");
    assert_eq!(interaction["terms"]["formula"], "spot_last_price_micros_v1");
    assert_eq!(
        interaction["terms"]["resolution_source"],
        "gemini_spot_ticker"
    );
    let proposed = get(&c.app, "/api/activity?limit=50").await;
    let item = proposed
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["kind"] == "challenge_proposed")
        .unwrap()
        .clone();
    assert!(
        item["detail"]["fallback_reason"]
            .as_str()
            .unwrap()
            .contains("prediction-market")
    );
}

// ---------------------------------------------------------------------------
// Proposal cooldown, the dev clock guard and the demo seed
// ---------------------------------------------------------------------------

async fn set_status(c: &Ctx, agent: Uuid, status: &str) {
    post(
        &c.app,
        &format!("/api/agents/{agent}/status"),
        json!({ "status": status }),
    )
    .await;
}

/// Proposes at `t0`, then gets the challenge cancelled (the paused proposer
/// cannot answer the counter) and resumes the proposer.
async fn proposed_then_cancelled(c: &Ctx) -> Uuid {
    let (a, _b) = (bull(c, 100).await, skeptic(c, 100).await);
    assert_eq!(tick(c, c.t0).await.proposed, 1);
    set_status(c, a, "paused").await;
    assert_eq!(tick(c, c.t0 + Duration::seconds(1)).await.cancelled, 1);
    set_status(c, a, "active").await;
    a
}

#[tokio::test]
async fn an_agent_does_not_propose_again_on_a_market_during_the_cooldown() {
    let c = setup().await;
    let cooldown = c.state.settings.proposal_cooldown_secs;
    assert_eq!(cooldown, 600);
    let a = proposed_then_cancelled(&c).await;
    let runs = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_runs WHERE agent_id = $1 AND trigger = 'trigger.market_move'",
        )
        .bind(a)
        .fetch_one(&c.pool)
        .await
        .unwrap()
    };
    assert_eq!(runs().await, 1);

    // The market still shows the same opportunity, but the agent stays quiet
    // and does not even reason (no run, so no model call).
    for secs in [2, 15, 300, cooldown - 1] {
        let r = tick(&c, c.t0 + Duration::seconds(secs)).await;
        assert_eq!((r.proposed, r.denied), (0, 0), "after {secs} s");
    }
    assert_eq!(runs().await, 1);
    assert_eq!(
        get(&c.app, "/api/interactions")
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Once the cooldown has passed it may propose again.
    let r = tick(&c, c.t0 + Duration::seconds(cooldown)).await;
    assert_eq!(r.proposed, 1);
    assert_eq!(
        get(&c.app, "/api/interactions")
            .await
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn cooldown_is_per_market_and_can_be_disabled() {
    // Disabled: the old behaviour, a new proposal on the next tick.
    let c = setup_with(|mut state| {
        state.settings.proposal_cooldown_secs = 0;
        state
    })
    .await;
    proposed_then_cancelled(&c).await;
    assert_eq!(tick(&c, c.t0 + Duration::seconds(2)).await.proposed, 1);
    drop(c);

    // Enabled: another market is not affected by the cooldown on the first.
    let c = setup().await;
    proposed_then_cancelled(&c).await;
    c.fake.set_prediction_markets(false); // discovery falls back to spot
    let r = tick(&c, c.t0 + Duration::seconds(2)).await;
    assert_eq!(r.proposed, 1);
    let list = get(&c.app, "/api/interactions").await;
    let keys: Vec<String> =
        sqlx::query_scalar("SELECT market_key FROM interactions ORDER BY created_at")
            .fetch_all(&c.pool)
            .await
            .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 2);
    assert_ne!(keys[0], keys[1]);
}

#[tokio::test]
async fn a_simulated_clock_jump_past_a_funding_deadline_is_refused_unless_allowed() {
    let c = setup().await;
    let (_a, _b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    http_tick(&c, c.t0).await;
    let id = uuid(&only_interaction(&c).await["id"]);
    assert_eq!(state_of(&c.pool, id).await, "negotiating");

    // Jumping an hour ahead would silently expire the negotiation: refused.
    let jump = (c.t0 + Duration::hours(1)).to_rfc3339();
    let (status, body) = send(
        &c.app,
        "POST",
        "/api/dev/tick",
        Some(json!({ "now": jump })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "clock_jump_would_expire");
    assert_eq!(state_of(&c.pool, id).await, "negotiating");

    // A step that stays inside the funding window is fine and moves it on.
    let report = http_tick(&c, c.t0 + Duration::seconds(299)).await;
    assert_eq!(
        (report["accepted"].clone(), report["funded"].clone()),
        (json!(1), json!(1))
    );
    assert_eq!(state_of(&c.pool, id).await, "active");
    // Funded challenges have no funding deadline left: the jump is allowed.
    let report = http_tick(&c, c.t0 + Duration::hours(2)).await;
    assert_eq!(report["settled"], 1);
}

#[tokio::test]
async fn the_deadline_rule_itself_is_unchanged_by_the_clock_guard() {
    let c = setup().await;
    let (_a, _b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    http_tick(&c, c.t0).await;
    let id = uuid(&only_interaction(&c).await["id"]);
    let jump = (c.t0 + Duration::hours(1)).to_rfc3339();
    let report = post(
        &c.app,
        "/api/dev/tick",
        json!({ "now": jump, "allow_expiry": true }),
    )
    .await;
    assert_eq!(report["funded"], 0);
    assert_eq!(state_of(&c.pool, id).await, "cancelled");
    assert!(
        activity_kinds(&c.pool)
            .await
            .contains(&"challenge_expired".to_owned())
    );
    assert_ledger_sound(&c.pool).await;
}

fn field<'a>(v: &'a Value, name: &str) -> &'a str {
    v[name].as_str().unwrap_or_else(|| panic!("{name} in {v}"))
}

/// The README demo: seed, then a few ticks walk the whole lifecycle.
#[tokio::test]
async fn seeded_demo_walks_the_whole_lifecycle() {
    let c = setup().await;
    let seed = post(&c.app, "/api/dev/seed-demo", json!({})).await;
    assert_eq!(seed["env"], "simulated");
    let (bull, skeptic) = (
        uuid(&seed["agents"][0]["id"]),
        uuid(&seed["agents"][1]["id"]),
    );
    assert_eq!(seed["agents"][0]["created"], true);
    for agent in seed["agents"].as_array().unwrap() {
        assert!(field(agent, "name").contains("(simulated)"));
    }

    // Seeding again changes nothing: same agents, no second funding.
    let again = post(&c.app, "/api/dev/seed-demo", json!({})).await;
    assert_eq!(uuid(&again["agents"][0]["id"]), bull);
    assert_eq!(again["agents"][0]["created"], false);
    assert_eq!(
        get(&c.app, "/api/agents").await.as_array().unwrap().len(),
        2
    );
    assert_eq!(available(&c.pool, bull).await, 100 * USDC);
    assert_eq!(available(&c.pool, skeptic).await, 100 * USDC);
    let wallet = get(&c.app, "/api/wallet").await;
    assert_eq!(
        (field(&wallet, "env"), field(&wallet, "unallocated")),
        ("simulated", "0")
    );
    assert_eq!(
        get(&c.app, &format!("/api/agents/{bull}")).await["env"],
        "simulated"
    );

    // Tick 1: propose and counter.
    let report = http_tick(&c, c.t0).await;
    assert_eq!(
        (report["proposed"].clone(), report["countered"].clone()),
        (json!(1), json!(1))
    );
    let interaction = only_interaction(&c).await;
    let id = uuid(&interaction["id"]);
    assert_eq!(interaction["state"], "negotiating");
    assert_eq!(interaction["env"], "simulated");
    assert_eq!(uuid(&interaction["proposer_id"]), bull);
    assert_eq!(uuid(&interaction["counterparty_id"]), skeptic);

    // Tick 2: the bull accepts; the skeptic's stake is above its automatic
    // limit, so the permission engine asks the owner and nothing is funded.
    let report = http_tick(&c, c.t0 + Duration::seconds(15)).await;
    assert_eq!(
        (
            report["accepted"].clone(),
            report["approvals_requested"].clone(),
            report["funded"].clone()
        ),
        (json!(1), json!(1), json!(0))
    );
    assert_eq!(state_of(&c.pool, id).await, "awaiting_approval");
    assert_eq!(
        committed(&c.pool, bull).await + committed(&c.pool, skeptic).await,
        0
    );
    // More ticks never fund it without the owner.
    let report = http_tick(&c, c.t0 + Duration::seconds(30)).await;
    assert_eq!(report["funded"], 0);

    let approvals = get(&c.app, "/api/approvals").await;
    assert_eq!(approvals.as_array().unwrap().len(), 1);
    assert_eq!(uuid(&approvals[0]["agent_id"]), skeptic);
    assert_eq!(approvals[0]["stake"], "2500000");
    let approval = field(&approvals[0], "id");
    let result = post(
        &c.app,
        &format!("/api/approvals/{approval}/approve"),
        json!({}),
    )
    .await;
    assert_eq!(result["interaction"]["state"], "active");
    assert_eq!(committed(&c.pool, bull).await, 5 * USDC / 2);
    assert_eq!(committed(&c.pool, skeptic).await, 5 * USDC / 2);

    // The market moves the bull's way; after `end_at` the challenge resolves
    // and settles.
    post(
        &c.app,
        "/api/dev/fake-market",
        json!({ "yes_price_bps": 7000 }),
    )
    .await;
    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    let end_at: DateTime<Utc> = field(&detail, "end_at").parse().unwrap();
    let report = http_tick(&c, end_at + Duration::seconds(5)).await;
    assert_eq!(
        (report["resolving"].clone(), report["settled"].clone()),
        (json!(1), json!(1))
    );

    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(detail["state"], "settled");
    assert_eq!(detail["outcome"], "a");
    assert_eq!(uuid(&detail["winner_id"]), bull);
    assert_eq!(available(&c.pool, bull).await, 100 * USDC + 5 * USDC / 2);
    assert_eq!(available(&c.pool, skeptic).await, 100 * USDC - 5 * USDC / 2);
    assert_eq!(
        committed(&c.pool, bull).await + committed(&c.pool, skeptic).await,
        0
    );

    // What the owner reads: sentences with USDC amounts, labelled simulated,
    // and never raw minor units.
    let negotiation = detail["negotiation"].as_array().unwrap();
    let actions: Vec<&str> = negotiation.iter().map(|r| field(r, "action")).collect();
    assert_eq!(actions, ["propose", "counter", "accept"]);
    assert_eq!(
        field(&negotiation[0], "summary"),
        "I claim the market ends at or above 0.55 (YES price) and stake 5.00 USDC (simulated) on it."
    );
    assert!(field(&negotiation[1], "summary").contains("2.50 USDC (simulated)"));
    assert_eq!(negotiation[1]["terms"]["stake"], "2500000");
    for round in negotiation {
        let summary = field(round, "summary");
        assert!(
            !summary.contains("minor units") && !summary.contains("bps"),
            "{summary}"
        );
    }
    let kinds = activity_kinds(&c.pool).await;
    for kind in [
        "challenge_proposed",
        "challenge_countered",
        "challenge_accepted",
        "approval_requested",
        "approval_granted",
        "stake_committed",
        "challenge_settled",
    ] {
        assert!(count(&kinds, kind) >= 1, "missing {kind} in {kinds:?}");
    }
    assert_ledger_sound(&c.pool).await;

    // P&L over time is plotted on the lifecycle clock: the simulated `now`
    // of the settling tick, not the wall clock.
    let settled_at = end_at + Duration::seconds(5);
    let pnl = get(&c.app, "/api/stats/portfolio").await["pnl_over_time"].clone();
    assert_eq!(pnl.as_array().unwrap().len(), 2);
    for point in pnl.as_array().unwrap() {
        let ts: DateTime<Utc> = field(point, "ts").parse().unwrap();
        assert_eq!(ts, settled_at);
    }

    // Co-captain step: two more identical rounds, each won by the bull. The
    // skeptic's third straight loss yields a rule suggestion (F-21) to halve
    // its maximum stake.
    let mut at = settled_at;
    for round in 2..=3 {
        let report = demo_round(&c, at).await;
        assert_eq!(report["settled"], 1, "round {round}: {report}");
        at = field(&report, "now").parse().unwrap();
        if round == 2 {
            assert!(
                pending_suggestion(&c, skeptic, "lower_max_stake")
                    .await
                    .is_none()
            );
        }
    }
    assert_eq!(
        available(&c.pool, skeptic).await,
        100 * USDC - 3 * 5 * USDC / 2
    );
    let suggestion = pending_suggestion(&c, skeptic, "lower_max_stake")
        .await
        .expect("lower_max_stake suggestion after three losses");
    assert_eq!(suggestion["source"], "rules");
    assert_eq!(suggestion["loosens_control"], false);
    assert_eq!(
        suggestion["proposed_change"],
        json!({"policy": {"max_per_interaction": "5000000"}})
    );
    assert_eq!(
        suggestion["evidence"]["recent_settle_nets"],
        json!(["-2500000", "-2500000", "-2500000"])
    );
    let pnl = get(&c.app, "/api/stats/portfolio").await["pnl_over_time"].clone();
    let times: Vec<&str> = pnl
        .as_array()
        .unwrap()
        .iter()
        .map(|p| field(p, "ts"))
        .collect();
    assert_eq!(times.len(), 6);
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "{times:?}");

    // Accepting it is an owner edit: a new config version, history kept, no
    // funds moved.
    let before = get(&c.app, &format!("/api/agents/{skeptic}")).await;
    assert_eq!(before["version"], 1);
    let done = post(
        &c.app,
        &format!("/api/suggestions/{}/accept", field(&suggestion, "id")),
        json!({}),
    )
    .await;
    assert_eq!(done["status"], "accepted");
    let after = get(&c.app, &format!("/api/agents/{skeptic}")).await;
    assert_eq!(after["version"], 2);
    assert_eq!(after["policy"]["max_per_interaction"], "5000000");
    assert_eq!(after["balances"], before["balances"]);
    let versions = get(&c.app, &format!("/api/agents/{skeptic}/versions")).await;
    assert_eq!(versions.as_array().unwrap().len(), 2);
    assert_eq!(versions[1]["policy"]["max_per_interaction"], "10000000");
    assert_ledger_sound(&c.pool).await;
}

/// One more round of the README demo starting at simulated time `at`: the
/// contract back at 0.55, propose + counter, accept + approval request,
/// owner approves, contract to 0.70, settle. Returns the settling tick.
async fn demo_round(c: &Ctx, at: DateTime<Utc>) -> Value {
    post(
        &c.app,
        "/api/dev/fake-market",
        json!({ "yes_price_bps": 5500 }),
    )
    .await;
    let report = http_tick(c, at).await;
    assert_eq!(
        (report["proposed"].clone(), report["countered"].clone()),
        (json!(1), json!(1)),
        "{report}"
    );
    let report = http_tick(c, at + Duration::seconds(15)).await;
    assert_eq!(report["approvals_requested"], 1, "{report}");
    let approvals = get(&c.app, "/api/approvals").await;
    let approval = field(&approvals[0], "id");
    let result = post(
        &c.app,
        &format!("/api/approvals/{approval}/approve"),
        json!({}),
    )
    .await;
    assert_eq!(result["interaction"]["state"], "active");
    let id = field(&result["interaction"], "id").to_owned();
    post(
        &c.app,
        "/api/dev/fake-market",
        json!({ "yes_price_bps": 7000 }),
    )
    .await;
    let detail = get(&c.app, &format!("/api/interactions/{id}")).await;
    let end_at: DateTime<Utc> = field(&detail, "end_at").parse().unwrap();
    http_tick(c, end_at + Duration::seconds(5)).await
}

async fn pending_suggestion(c: &Ctx, agent: Uuid, kind: &str) -> Option<Value> {
    get(&c.app, "/api/suggestions?status=pending")
        .await
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["kind"] == kind && s["agent_id"] == agent.to_string())
        .cloned()
}

#[tokio::test]
async fn seed_demo_needs_json_and_is_dev_only() {
    let c = setup().await;
    let (status, _) = send(&c.app, "POST", "/api/dev/seed-demo", None).await;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let (status, _) = send(
        &c.app,
        "POST",
        "/api/dev/seed-demo",
        Some(json!({ "funds": "1" })),
    )
    .await;
    assert!(status.is_client_error());
    assert!(
        get(&c.app, "/api/agents")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    drop(c);

    let c = setup_with(|mut state| {
        state.settings.dev_endpoints = false;
        state
    })
    .await;
    let (status, _) = send(&c.app, "POST", "/api/dev/seed-demo", Some(json!({}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
