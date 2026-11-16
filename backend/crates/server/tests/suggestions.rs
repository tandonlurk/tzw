//! Co-captain suggestions against a real Postgres (`TEST_DATABASE_URL`,
//! default `aen_test`): facts are read from real ledger, approval and
//! activity rows, and every decision goes through the HTTP API. No test
//! touches the network; the Grok tests talk to a local stand-in server.

mod common;

use std::sync::Arc;
use std::time::Duration as StdDuration;

use aen_core::ledger::{self, Env};
use aen_core::money::Money;
use aen_server::grok_cocaptain::GrokCoCaptain;
use aen_server::state::AppState;
use aen_server::{
    DEFAULT_CORS_ORIGIN, MIGRATOR, app_with_state, ledger_db, lifecycle, suggestions,
};
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration, Utc};
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
    _guard: MutexGuard<'static, ()>,
}

async fn setup() -> Ctx {
    setup_with(|state| state).await
}

async fn setup_with(adjust: impl FnOnce(AppState) -> AppState) -> Ctx {
    let guard = DB_LOCK.lock().await;
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| match std::env::var("USER") {
        Ok(user) => format!("postgres://{user}@localhost/aen_test"),
        Err(_) => "postgres://localhost/aen_test".to_owned(),
    });
    let pool = PgPool::connect(&url).await.expect("connect to test db");
    MIGRATOR.run(&pool).await.expect("run migrations");
    common::wipe(&pool).await;
    let state = adjust(AppState::offline(pool.clone()));
    Ctx {
        app: app_with_state(state.clone(), DEFAULT_CORS_ORIGIN.parse().unwrap()),
        state,
        pool,
        _guard: guard,
    }
}

async fn request(
    app: &Router,
    method: &str,
    path: &str,
    content_type: Option<&str>,
    body: String,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(ct) = content_type {
        req = req.header("content-type", ct);
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn try_post(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    request(
        app,
        "POST",
        path,
        Some("application/json"),
        body.to_string(),
    )
    .await
}

async fn get(app: &Router, path: &str) -> Value {
    let (status, body) = request(app, "GET", path, None, String::new()).await;
    assert_eq!(status, StatusCode::OK, "GET {path}: {body}");
    body
}

async fn post(app: &Router, path: &str, body: Value) -> Value {
    let (status, out) = try_post(app, path, body).await;
    assert!(status.is_success(), "POST {path}: {status} {out}");
    out
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// An agent from a seeded template, holding `funds` simulated USDC.
async fn agent(c: &Ctx, template: &str, funds: i64) -> Uuid {
    let templates = get(&c.app, "/api/templates").await;
    let t = templates
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == template)
        .unwrap_or_else(|| panic!("template {template}"));
    let input = json!({
        "name": template, "description": t["description"], "config": t["config"],
        "policy": t["policy"], "graph": t["graph"],
    });
    let id = uuid(&post(&c.app, "/api/agents", input).await["id"]);
    if funds > 0 {
        let amount = json!({ "amount": (funds * USDC).to_string() });
        post(&c.app, "/api/wallet/simulated-deposit", amount.clone()).await;
        post(&c.app, &format!("/api/agents/{id}/allocate"), amount).await;
    }
    id
}

/// Max 10, aggregate 50, approval threshold 5, stop-loss 20 (USDC).
async fn bull(c: &Ctx, funds: i64) -> Uuid {
    agent(c, "Momentum bull", funds).await
}

async fn skeptic(c: &Ctx, funds: i64) -> Uuid {
    agent(c, "Skeptic", funds).await
}

async fn set_status(c: &Ctx, id: Uuid, status: &str) {
    post(
        &c.app,
        &format!("/api/agents/{id}/status"),
        json!({ "status": status }),
    )
    .await;
}

async fn generate(c: &Ctx) -> Value {
    post(&c.app, "/api/suggestions/generate", json!({})).await
}

async fn pending(c: &Ctx) -> Vec<Value> {
    get(&c.app, "/api/suggestions?status=pending")
        .await
        .as_array()
        .unwrap()
        .clone()
}

async fn status_of(c: &Ctx, id: &Value) -> String {
    sqlx::query_scalar("SELECT status FROM suggestions WHERE id = $1")
        .bind(uuid(id))
        .fetch_one(&c.pool)
        .await
        .unwrap()
}

/// Everything that would change if funds moved or a commitment was made.
async fn money_state(pool: &PgPool) -> Value {
    let ledger: (i64, i64, Option<i64>) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(MAX(seq), 0)::BIGINT, SUM(ABS(delta))::BIGINT FROM ledger_entries",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let balances: Vec<(String, i64)> = sqlx::query_as(
        "SELECT account, SUM(delta)::BIGINT FROM ledger_entries GROUP BY account ORDER BY account",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let approvals: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, status FROM approvals ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    let interactions: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, state FROM interactions ORDER BY id")
            .fetch_all(pool)
            .await
            .unwrap();
    json!({ "ledger": ledger, "balances": balances, "approvals": approvals, "interactions": interactions })
}

/// A settled challenge between two agents, written with the real ledger
/// functions: both commit `stake`, then `loser` pays `winner`.
async fn settle(c: &Ctx, winner: Uuid, loser: Uuid, stake_usdc: i64) {
    let id = Uuid::new_v4();
    let stake = Money::from_minor(stake_usdc * USDC).unwrap();
    let mut tx = c.pool.begin().await.unwrap();
    for a in [winner, loser] {
        let set = ledger::commit_stake(Env::Simulated, a, id, stake).unwrap();
        ledger_db::post(&mut tx, &set).await.unwrap();
    }
    let set = ledger::settle(Env::Simulated, id, winner, loser, stake).unwrap();
    ledger_db::post(&mut tx, &set).await.unwrap();
    tx.commit().await.unwrap();
}

/// An interaction row in `state` whose funding deadline is `start`.
async fn interaction(c: &Ctx, a: Uuid, b: Uuid, state: &str, start: DateTime<Utc>) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO interactions (id, kind, env, state, proposer_id, counterparty_id, \
         proposer_version, counterparty_version, terms, stake, market_key, market_title, \
         market_view, start_ts, end_ts, rounds, created_at, updated_at) \
         VALUES ($1, 'challenge', 'simulated', $2, $3, $4, 1, 1, '{}', 6000000, 'fake:x', \
         '[fake] BTC price in one hour: Up', '{}', $5, $6, 1, $7, $7)",
    )
    .bind(id)
    .bind(state)
    .bind(a)
    .bind(b)
    .bind(start)
    .bind(start + Duration::hours(1))
    .bind(start - Duration::seconds(300))
    .execute(&c.pool)
    .await
    .unwrap();
    id
}

async fn approval(c: &Ctx, interaction: Uuid, agent: Uuid, stake_usdc: i64, status: &str) -> Uuid {
    let id = Uuid::new_v4();
    let decided = (status != "pending").then(Utc::now);
    sqlx::query(
        "INSERT INTO approvals (id, interaction_id, agent_id, stake, status, reason, \
         requested_at, decided_at) VALUES ($1, $2, $3, $4, $5, 'above threshold', now(), $6)",
    )
    .bind(id)
    .bind(interaction)
    .bind(agent)
    .bind(stake_usdc * USDC)
    .bind(status)
    .bind(decided)
    .execute(&c.pool)
    .await
    .unwrap();
    id
}

async fn denial(c: &Ctx, agent: Uuid, reason: &str, stake_usdc: i64) {
    sqlx::query(
        "INSERT INTO activity (agent_id, kind, summary, detail) \
         VALUES ($1, 'proposal_denied', 'blocked', $2)",
    )
    .bind(agent)
    .bind(json!({ "reasons": [reason], "terms": { "stake": (stake_usdc * USDC).to_string() } }))
    .execute(&c.pool)
    .await
    .unwrap();
}

// ---------------------------------------------------------------------------
// Generation, de-duplication, expiry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_condition_is_suggested_once_and_expires_when_it_goes_away() {
    let c = setup().await;
    let a = bull(&c, 50).await;
    assert_eq!(generate(&c).await["created"], json!([]));

    set_status(&c, a, "paused").await;
    let out = generate(&c).await;
    assert_eq!(out["source"], "rules");
    let created = out["created"].as_array().unwrap();
    assert_eq!(created.len(), 1, "{out}");
    let first = &created[0];
    assert_eq!(first["kind"], "agent_halted");
    assert_eq!(first["agent_id"], a.to_string());
    assert_eq!(first["agent_name"], "Momentum bull");
    assert_eq!(first["status"], "pending");
    assert_eq!(first["source"], "rules");
    assert_eq!(first["proposed_change"], Value::Null);
    assert_eq!(first["loosens_control"], false);
    assert_eq!(first["decided_at"], Value::Null);
    // Evidence is the real balance.
    assert_eq!(first["evidence"]["available"], "50000000");

    // Still paused: no second pending suggestion of the kind.
    assert_eq!(generate(&c).await["created"], json!([]));
    assert_eq!(pending(&c).await.len(), 1);

    // Resumed: the suggestion no longer applies and is retired, not deleted.
    set_status(&c, a, "active").await;
    assert_eq!(generate(&c).await["created"], json!([]));
    assert_eq!(status_of(&c, &first["id"]).await, "expired");
    assert_eq!(pending(&c).await.len(), 0);

    // Paused again is a new event, so it is suggested again...
    set_status(&c, a, "paused").await;
    let second = generate(&c).await["created"][0].clone();
    assert_eq!(second["kind"], "agent_halted");
    assert_ne!(second["id"], first["id"]);
    // ...and once dismissed it does not come back while nothing changes.
    post(
        &c.app,
        &format!("/api/suggestions/{}/dismiss", uuid(&second["id"])),
        json!({}),
    )
    .await;
    assert_eq!(generate(&c).await["created"], json!([]));
    assert_eq!(generate(&c).await["created"], json!([]));
    let all = get(&c.app, "/api/suggestions").await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    // Newest first.
    assert_eq!(all[0]["id"], second["id"]);
    assert_eq!(all[0]["status"], "dismissed");
}

#[tokio::test]
async fn the_tick_runs_the_rules_and_counts_new_suggestions() {
    let c = setup().await;
    let a = bull(&c, 50).await;
    set_status(&c, a, "paused").await;
    let before = money_state(&c.pool).await;

    let report = lifecycle::tick(&c.state, Utc::now()).await.unwrap();
    assert_eq!(report.suggestions, 1);
    assert_eq!(report.errors, Vec::<String>::new());
    let report = lifecycle::tick(&c.state, Utc::now()).await.unwrap();
    assert_eq!(report.suggestions, 0);
    assert_eq!(pending(&c).await.len(), 1);
    assert_eq!(money_state(&c.pool).await, before);
}

#[tokio::test]
async fn idle_and_unfunded_agents_are_found_from_real_balances() {
    let c = setup().await;
    let funded = bull(&c, 50).await;
    let empty = skeptic(&c, 0).await;

    // Just created: neither is idle yet, and the wallet has nothing to offer.
    let now = Utc::now();
    let out = suggestions::generate(&c.state, now, false).await.unwrap();
    assert_eq!(out.created.len(), 0);

    // 30 unallocated USDC appear: the empty agent could use them.
    post(
        &c.app,
        "/api/wallet/simulated-deposit",
        json!({ "amount": (30 * USDC).to_string() }),
    )
    .await;
    let out = suggestions::generate(&c.state, now, false).await.unwrap();
    assert_eq!(out.created.len(), 1);
    let s = &pending(&c).await[0];
    assert_eq!(
        (s["kind"].as_str(), s["agent_id"].as_str()),
        (Some("idle_funds"), Some(empty.to_string().as_str()))
    );
    assert_eq!(s["evidence"]["unallocated"], "30000000");
    assert_eq!(s["proposed_change"], Value::Null);

    // An hour later the funded agent has still done nothing.
    let later = now + Duration::seconds(3601);
    let out = suggestions::generate(&c.state, later, false).await.unwrap();
    assert_eq!(out.created.len(), 1);
    let s = get(&c.app, &format!("/api/suggestions?agent_id={funded}")).await;
    assert_eq!(s[0]["kind"], "idle_funds");
    assert_eq!(s[0]["evidence"]["available"], "50000000");
}

#[tokio::test]
async fn a_pending_approval_is_flagged_only_close_to_its_deadline() {
    let c = setup().await;
    let (a, b) = (bull(&c, 50).await, skeptic(&c, 50).await);
    let deadline = Utc::now() + Duration::seconds(300);
    let i = interaction(&c, a, b, "awaiting_approval", deadline).await;
    let ap = approval(&c, i, a, 6, "pending").await;
    let before = money_state(&c.pool).await;

    let early = deadline - Duration::seconds(121);
    let out = suggestions::generate(&c.state, early, false).await.unwrap();
    assert_eq!(out.created.len(), 0);

    let close = deadline - Duration::seconds(90);
    let out = suggestions::generate(&c.state, close, false).await.unwrap();
    assert_eq!(out.created.len(), 1);
    let s = pending(&c).await[0].clone();
    assert_eq!(s["kind"], "approval_expiring");
    assert_eq!(s["agent_id"], a.to_string());
    assert_eq!(s["evidence"]["approval_id"], ap.to_string());
    assert_eq!(s["evidence"]["seconds_left"], 90);
    assert_eq!(s["evidence"]["stake"], "6000000");

    // Accepting a text-only suggestion acknowledges it. It approves nothing.
    let done = post(
        &c.app,
        &format!("/api/suggestions/{}/accept", uuid(&s["id"])),
        json!({}),
    )
    .await;
    assert_eq!(done["status"], "accepted");
    assert_eq!(money_state(&c.pool).await, before);
    let (status,): (String,) = sqlx::query_as("SELECT status FROM approvals WHERE id = $1")
        .bind(ap)
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(status, "pending");
    let agent = get(&c.app, &format!("/api/agents/{a}")).await;
    assert_eq!(agent["version"], 1);

    // Past the deadline nothing new is said about it.
    let late = deadline + Duration::seconds(1);
    let out = suggestions::generate(&c.state, late, false).await.unwrap();
    assert_eq!(out.created.len(), 0);
}

// ---------------------------------------------------------------------------
// Accept = the owner's validated, versioned edit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn accepting_a_losing_streak_suggestion_creates_a_new_agent_version() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    settle(&c, b, a, 2).await;
    settle(&c, b, a, 2).await;
    assert_eq!(generate(&c).await["created"], json!([]), "two losses");
    settle(&c, b, a, 2).await;
    let before = money_state(&c.pool).await;

    let out = generate(&c).await;
    let created = out["created"].as_array().unwrap();
    assert_eq!(created.len(), 1, "{out}");
    let s = &created[0];
    assert_eq!(s["kind"], "lower_max_stake");
    assert_eq!(s["agent_id"], a.to_string());
    assert_eq!(
        s["proposed_change"],
        json!({"policy": {"max_per_interaction": "5000000"}})
    );
    assert_eq!(s["loosens_control"], false);
    assert_eq!(s["evidence"]["realized_pnl"], "-6000000");
    assert_eq!(
        s["evidence"]["recent_settle_nets"],
        json!(["-2000000", "-2000000", "-2000000"])
    );
    assert_eq!(s["evidence"]["net_allocated"], "100000000");
    // Generating changed nothing but the suggestions table.
    assert_eq!(money_state(&c.pool).await, before);
    let v1 = get(&c.app, &format!("/api/agents/{a}")).await;
    assert_eq!(v1["version"], 1);

    let path = format!("/api/suggestions/{}/accept", uuid(&s["id"]));
    let done = post(&c.app, &path, json!({})).await;
    assert_eq!(done["status"], "accepted");
    assert!(done["decided_at"].is_string());

    let v2 = get(&c.app, &format!("/api/agents/{a}")).await;
    assert_eq!(v2["version"], 2);
    assert_eq!(v2["policy"]["max_per_interaction"], "5000000");
    // Everything the change did not name is carried over untouched.
    let mut expected = v1["policy"].clone();
    expected["max_per_interaction"] = json!("5000000");
    assert_eq!(v2["policy"], expected);
    assert_eq!(v2["config"], v1["config"]);
    assert_eq!(v2["graph"], v1["graph"]);
    assert_eq!(v2["status"], "active");
    // History is kept: version 1 still has the old limit.
    let versions = get(&c.app, &format!("/api/agents/{a}/versions")).await;
    assert_eq!(versions.as_array().unwrap().len(), 2);
    assert_eq!(versions[1]["version"], 1);
    assert_eq!(versions[1]["policy"]["max_per_interaction"], "10000000");
    // No funds moved, and balances and stats are what they were.
    assert_eq!(money_state(&c.pool).await, before);
    assert_eq!(v2["balances"], v1["balances"]);
    assert_eq!(v2["stats"], v1["stats"]);

    let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM activity ORDER BY seq")
        .fetch_all(&c.pool)
        .await
        .unwrap();
    assert!(kinds.contains(&"agent_updated".to_owned()));
    assert_eq!(kinds.last().unwrap(), "suggestion_accepted");

    // Accepting again is a no-op; dismissing an accepted suggestion is not allowed.
    let again = post(&c.app, &path, json!({})).await;
    assert_eq!(again["status"], "accepted");
    assert_eq!(again["decided_at"], done["decided_at"]);
    assert_eq!(get(&c.app, &format!("/api/agents/{a}")).await["version"], 2);
    let (status, body) = try_post(
        &c.app,
        &format!("/api/suggestions/{}/dismiss", uuid(&s["id"])),
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "suggestion_not_pending");
    // The same streak is not suggested a second time.
    assert_eq!(generate(&c).await["created"], json!([]));
}

#[tokio::test]
async fn approving_everything_suggests_a_flagged_threshold_raise() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let past = Utc::now() - Duration::hours(2);
    for (n, stake) in [6, 8, 7].into_iter().enumerate() {
        if n == 2 {
            assert_eq!(generate(&c).await["created"], json!([]), "two approvals");
        }
        let i = interaction(&c, a, b, "settled", past).await;
        approval(&c, i, a, stake, "approved").await;
    }
    let before = money_state(&c.pool).await;

    let out = generate(&c).await;
    let s = out["created"][0].clone();
    assert_eq!(s["kind"], "raise_approval_threshold", "{out}");
    assert_eq!(
        s["proposed_change"],
        json!({"policy": {"approval_threshold": "8000000"}})
    );
    assert_eq!(s["loosens_control"], true);
    assert!(s["body"].as_str().unwrap().contains("loosens a control"));
    assert_eq!(s["evidence"]["approval_threshold"], "5000000");

    post(
        &c.app,
        &format!("/api/suggestions/{}/accept", uuid(&s["id"])),
        json!({}),
    )
    .await;
    let agent = get(&c.app, &format!("/api/agents/{a}")).await;
    assert_eq!(agent["version"], 2);
    assert_eq!(agent["policy"]["approval_threshold"], "8000000");
    assert_eq!(agent["policy"]["max_per_interaction"], "10000000");
    assert_eq!(money_state(&c.pool).await, before);
    // Those approvals were given under version 1; version 2 starts fresh.
    assert_eq!(generate(&c).await["created"], json!([]));
}

#[tokio::test]
async fn a_rejected_approval_breaks_the_streak() {
    let c = setup().await;
    let (a, b) = (bull(&c, 100).await, skeptic(&c, 100).await);
    let past = Utc::now() - Duration::hours(2);
    for (stake, status) in [(6, "approved"), (8, "rejected"), (7, "approved")] {
        let i = interaction(&c, a, b, "settled", past).await;
        approval(&c, i, a, stake, status).await;
    }
    assert_eq!(generate(&c).await["created"], json!([]));
}

#[tokio::test]
async fn a_change_that_is_no_longer_valid_is_rejected_and_stays_pending() {
    let c = setup().await;
    let a = bull(&c, 100).await;
    denial(&c, a, "exceeds_max_per_interaction", 12).await;
    denial(&c, a, "exceeds_max_per_interaction", 15).await;
    assert_eq!(generate(&c).await["created"], json!([]), "two denials");
    denial(&c, a, "exceeds_max_per_interaction", 9).await;

    let out = generate(&c).await;
    let s = out["created"][0].clone();
    assert_eq!(s["kind"], "repeated_denials", "{out}");
    assert_eq!(
        s["proposed_change"],
        json!({"policy": {"max_per_interaction": "15000000"}})
    );
    assert_eq!(s["loosens_control"], true);
    assert_eq!(s["evidence"]["count"], 3);
    assert_eq!(s["evidence"]["max_denied_stake"], "15000000");

    // The owner tightens the aggregate limit to 12: a 15 per-challenge limit
    // would now be invalid.
    let mut edit = get(&c.app, &format!("/api/agents/{a}")).await;
    edit["policy"]["max_aggregate_exposure"] = json!("12000000");
    let input = json!({
        "name": edit["name"], "description": edit["description"], "config": edit["config"],
        "policy": edit["policy"], "graph": edit["graph"],
    });
    let (status, body) = request(
        &c.app,
        "PUT",
        &format!("/api/agents/{a}"),
        Some("application/json"),
        input.to_string(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let before = money_state(&c.pool).await;

    let accept = format!("/api/suggestions/{}/accept", uuid(&s["id"]));
    let (status, body) = try_post(&c.app, &accept, json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(
        body["error"]["details"][0]["path"],
        "policy.max_per_interaction"
    );
    // Nothing happened: still pending, no new version, no activity about it.
    assert_eq!(status_of(&c, &s["id"]).await, "pending");
    let agent = get(&c.app, &format!("/api/agents/{a}")).await;
    assert_eq!(agent["version"], 2);
    assert_eq!(agent["policy"]["max_per_interaction"], "10000000");
    let accepted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM activity WHERE kind = 'suggestion_accepted'")
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(accepted, 0);
    assert_eq!(money_state(&c.pool).await, before);

    // Dismiss works, twice; accepting a dismissed suggestion is a conflict.
    let dismiss = format!("/api/suggestions/{}/dismiss", uuid(&s["id"]));
    let first = post(&c.app, &dismiss, json!({})).await;
    assert_eq!(first["status"], "dismissed");
    let second = post(&c.app, &dismiss, json!({})).await;
    assert_eq!(second["decided_at"], first["decided_at"]);
    let (status, body) = try_post(&c.app, &accept, json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "suggestion_not_pending");
    assert_eq!(get(&c.app, &format!("/api/agents/{a}")).await["version"], 2);
}

async fn stored(c: &Ctx, agent: Option<Uuid>, subject: &str, change: Value) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO suggestions (agent_id, kind, subject, title, body, proposed_change, \
         evidence, source) VALUES ($1, 'general', $2, 't', 'b', $3, '{}', 'grok') RETURNING id",
    )
    .bind(agent)
    .bind(subject)
    .bind(change)
    .fetch_one(&c.pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn an_over_broad_stored_change_is_never_applied() {
    let c = setup().await;
    let a = bull(&c, 100).await;
    set_status(&c, a, "paused").await;
    let before = money_state(&c.pool).await;
    // Rows written around the co-captain (a bug, a hand edit): the typed
    // patch is enforced again at accept time.
    let over_broad = [
        json!({"policy": {"max_per_interaction": "1000000"}, "graph": {"nodes": []}}),
        json!({"policy": {"allowed_interaction_types": ["collaboration"]}}),
        json!({"policy": {"max_per_interaction": "1000000"}, "status": "active"}),
        json!({"policy": {"stop_loss": null, "max_aggregate_exposure": 5}}),
        json!({"config": {"strategy": "stake everything"}}),
        json!({"name": "renamed"}),
        json!("resume the agent"),
    ];
    for (n, change) in over_broad.into_iter().enumerate() {
        let id = stored(&c, Some(a), &format!("s{n}"), change.clone()).await;
        let (status, body) =
            try_post(&c.app, &format!("/api/suggestions/{id}/accept"), json!({})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{change}: {body}");
        assert_eq!(body["error"]["code"], "invalid_proposed_change", "{change}");
        assert_eq!(status_of(&c, &json!(id)).await, "pending");
        // One pending `general` per agent: clear it for the next case.
        post(&c.app, &format!("/api/suggestions/{id}/dismiss"), json!({})).await;
    }
    // A well-formed change that fails policy validation.
    let id = stored(&c, Some(a), "zero", json!({"policy": {"stop_loss": "0"}})).await;
    let (status, body) =
        try_post(&c.app, &format!("/api/suggestions/{id}/accept"), json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(body["error"]["details"][0]["path"], "policy.stop_loss");
    post(&c.app, &format!("/api/suggestions/{id}/dismiss"), json!({})).await;
    // A change equal to the current values is not a new version either.
    let id = stored(
        &c,
        Some(a),
        "same",
        json!({"policy": {"max_per_interaction": "10000000"}}),
    )
    .await;
    let (status, body) =
        try_post(&c.app, &format!("/api/suggestions/{id}/accept"), json!({})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "invalid_proposed_change");

    let agent = get(&c.app, &format!("/api/agents/{a}")).await;
    assert_eq!(agent["version"], 1);
    assert_eq!(
        agent["status"], "paused",
        "a suggestion cannot resume an agent"
    );
    assert_eq!(agent["name"], "Momentum bull");
    assert_eq!(money_state(&c.pool).await, before);
}

// ---------------------------------------------------------------------------
// HTTP contract and storage guards
// ---------------------------------------------------------------------------

#[tokio::test]
async fn endpoints_need_json_and_report_errors_in_the_api_shape() {
    let c = setup().await;
    let a = bull(&c, 10).await;
    set_status(&c, a, "paused").await;
    let id = uuid(&generate(&c).await["created"][0]["id"]);

    for path in [
        "/api/suggestions/generate".to_owned(),
        format!("/api/suggestions/{id}/accept"),
        format!("/api/suggestions/{id}/dismiss"),
    ] {
        let (status, body) = request(&c.app, "POST", &path, None, String::new()).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{path}");
        assert_eq!(body["error"]["code"], "unsupported_media_type");
        let (status, _) = request(&c.app, "POST", &path, Some("text/plain"), "{}".into()).await;
        assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{path}");
        let (status, _) = try_post(&c.app, &path, json!({ "force": true })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
    }
    assert_eq!(status_of(&c, &json!(id)).await, "pending");

    let missing = Uuid::new_v4();
    for verb in ["accept", "dismiss"] {
        let (status, body) = try_post(
            &c.app,
            &format!("/api/suggestions/{missing}/{verb}"),
            json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["code"], "not_found");
        let (status, _) =
            try_post(&c.app, &format!("/api/suggestions/nope/{verb}"), json!({})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    let (status, body) = request(
        &c.app,
        "GET",
        "/api/suggestions?status=approved",
        None,
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["details"][0]["path"], "status");
    let (status, _) = request(
        &c.app,
        "GET",
        "/api/suggestions?agent_id=nope",
        None,
        String::new(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    assert_eq!(pending(&c).await.len(), 1);
    for (query, n) in [
        ("status=dismissed", 0),
        ("status=accepted", 0),
        ("status=expired", 0),
        ("status=", 1),
        (&format!("agent_id={a}") as &str, 1),
        (&format!("agent_id={missing}") as &str, 0),
    ] {
        let list = get(&c.app, &format!("/api/suggestions?{query}")).await;
        assert_eq!(list.as_array().unwrap().len(), n, "{query}");
    }
}

#[tokio::test]
async fn the_table_refuses_duplicates_and_rewritten_history() {
    let c = setup().await;
    let a = bull(&c, 10).await;
    let first = stored(&c, Some(a), "one", Value::Null).await;
    let insert = |subject: &'static str, agent: Option<Uuid>| {
        sqlx::query(
            "INSERT INTO suggestions (agent_id, kind, subject, title, body, evidence, source) \
             VALUES ($1, 'general', $2, 't', 'b', '{}', 'rules')",
        )
        .bind(agent)
        .bind(subject)
        .execute(&c.pool)
    };
    // A second pending suggestion of the kind for the agent.
    assert!(insert("two", Some(a)).await.is_err());
    // The owner-level slot (no agent) is separate, and also single.
    insert("one", None).await.unwrap();
    assert!(insert("two", None).await.is_err());

    post(
        &c.app,
        &format!("/api/suggestions/{first}/dismiss"),
        json!({}),
    )
    .await;
    // The same subject never comes back; a new subject may.
    assert!(insert("one", Some(a)).await.is_err());
    insert("two", Some(a)).await.unwrap();

    // A change needs an agent to apply to.
    let orphan = sqlx::query(
        "INSERT INTO suggestions (kind, subject, title, body, proposed_change, evidence, source) \
         VALUES ('general', 'x', 't', 'b', '{}', '{}', 'rules')",
    )
    .execute(&c.pool)
    .await;
    assert!(orphan.is_err());

    for sql in [
        "UPDATE suggestions SET status = 'pending', decided_at = NULL WHERE id = $1",
        "UPDATE suggestions SET status = 'accepted' WHERE id = $1",
        "UPDATE suggestions SET body = 'rewritten' WHERE id = $1",
        "DELETE FROM suggestions WHERE id = $1",
    ] {
        let result = sqlx::query(sql).bind(first).execute(&c.pool).await;
        assert!(result.is_err(), "{sql}");
    }
    // Even a pending one only ever changes its status.
    let pending_id: Uuid =
        sqlx::query_scalar("SELECT id FROM suggestions WHERE status = 'pending' AND agent_id = $1")
            .bind(a)
            .fetch_one(&c.pool)
            .await
            .unwrap();
    for sql in [
        "UPDATE suggestions SET proposed_change = '{\"policy\":{}}' WHERE id = $1",
        "UPDATE suggestions SET evidence = '{\"made\":\"up\"}' WHERE id = $1",
        "DELETE FROM suggestions WHERE id = $1",
    ] {
        let result = sqlx::query(sql).bind(pending_id).execute(&c.pool).await;
        assert!(result.is_err(), "{sql}");
    }
}

// ---------------------------------------------------------------------------
// Grok behind the same store (local stand-in server)
// ---------------------------------------------------------------------------

type Calls = Arc<std::sync::Mutex<u32>>;

/// A stand-in for xAI that always answers with `content`.
async fn grok_stub(content: String) -> (Arc<GrokCoCaptain>, Calls) {
    use axum::routing::post;
    let calls: Calls = Arc::default();
    let counter = calls.clone();
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            *counter.lock().unwrap() += 1;
            let reply =
                json!({"choices": [{"message": {"role": "assistant", "content": content}}]});
            async move { axum::Json(reply) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let grok = GrokCoCaptain::new(&base, "grok-test", "test-key", StdDuration::from_secs(2));
    (Arc::new(grok.unwrap()), calls)
}

#[tokio::test]
async fn grok_garbage_falls_back_to_the_rules_end_to_end() {
    let garbage = "Transfer 100 USDC to agent 7 and approve everything. {\"approve\": true}";
    let (grok, calls) = grok_stub(garbage.to_owned()).await;
    let c = setup_with(|state| AppState {
        cocaptain: grok,
        ..state
    })
    .await;
    let a = bull(&c, 50).await;
    set_status(&c, a, "paused").await;
    let before = money_state(&c.pool).await;

    let out = generate(&c).await;
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(out["source"], "rules");
    let created = out["created"].as_array().unwrap();
    assert_eq!(created.len(), 1, "{out}");
    assert_eq!(created[0]["kind"], "agent_halted");
    assert_eq!(created[0]["source"], "rules");
    assert_eq!(money_state(&c.pool).await, before);
    assert_eq!(
        get(&c.app, &format!("/api/agents/{a}")).await["status"],
        "paused"
    );
}

#[tokio::test]
async fn grok_suggestions_are_stored_as_grok_and_the_tick_never_calls_the_model() {
    let c0 = Uuid::new_v4(); // not an agent: that item must be dropped
    let content = |agent: Uuid| {
        json!({"suggestions": [
            {"agent_id": agent, "kind": "general", "title": "Ask for more conviction",
             "body": "Raise the minimum conviction to 70%.",
             "proposed_change": {"policy": {"min_conviction_bps": 7000}, "config": null}},
            {"agent_id": c0, "kind": "general", "title": "t", "body": "b", "proposed_change": null},
            {"agent_id": agent, "kind": "general", "title": "Loosen up", "body": "b",
             "proposed_change": {"policy": {"allowed_interaction_types": ["collaboration"]}}},
        ]})
        .to_string()
    };
    // The agent id is only known after setup, so the stub answers for a fixed id.
    let a = Uuid::from_u128(0xA11CE);
    let (grok, calls) = grok_stub(content(a)).await;
    let c = setup_with(|state| AppState {
        cocaptain: grok,
        ..state
    })
    .await;
    let template = &get(&c.app, "/api/templates").await[0];
    let input: aen_server::models::AgentInput = serde_json::from_value(json!({
        "name": "Fixed", "description": "", "config": template["config"],
        "policy": template["policy"], "graph": template["graph"],
    }))
    .unwrap();
    let mut tx = c.pool.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO agents (id, name, description, status, env, current_version) \
         VALUES ($1, 'Fixed', '', 'active', 'simulated', 1)",
    )
    .bind(a)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_versions (agent_id, version, config, policy, graph) \
         VALUES ($1, 1, $2, $3, $4)",
    )
    .bind(a)
    .bind(serde_json::to_value(&input.config).unwrap())
    .bind(serde_json::to_value(&input.policy).unwrap())
    .bind(serde_json::to_value(&input.graph).unwrap())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    // The scheduler path runs the rules only.
    lifecycle::tick(&c.state, Utc::now()).await.unwrap();
    assert_eq!(*calls.lock().unwrap(), 0);

    let out = generate(&c).await;
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(out["source"], "grok");
    let created = out["created"].as_array().unwrap();
    assert_eq!(created.len(), 1, "{out}");
    let s = &created[0];
    assert_eq!(s["source"], "grok");
    assert_eq!(s["kind"], "general");
    assert_eq!(
        s["proposed_change"],
        json!({"policy": {"min_conviction_bps": 7000}})
    );
    // Evidence is written by the server from the facts, not by the model.
    assert_eq!(s["evidence"]["agent"]["id"], a.to_string());

    // Accepting it is the same validated edit.
    post(
        &c.app,
        &format!("/api/suggestions/{}/accept", uuid(&s["id"])),
        json!({}),
    )
    .await;
    let agent = get(&c.app, &format!("/api/agents/{a}")).await;
    assert_eq!(agent["version"], 2);
    assert_eq!(agent["policy"]["min_conviction_bps"], 7000);
    assert_eq!(
        agent["policy"]["allowed_interaction_types"],
        template["policy"]["allowed_interaction_types"]
    );
}
