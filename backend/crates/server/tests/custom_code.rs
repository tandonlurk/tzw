//! `custom.code` agents end to end against a real Postgres
//! (`TEST_DATABASE_URL`, default `aen_test`): the sandbox's intents pass
//! through the same planning and `authorize` gate as built-in blocks, and
//! failures are recorded with no funds moving. Tests that need the sandbox
//! skip when its guest module is not built (`crates/sandbox/build-guest.sh`).

mod common;

use std::time::Instant;

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
use tokio::sync::{Mutex, MutexGuard};
use tower::ServiceExt;
use uuid::Uuid;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

const USDC: i64 = 1_000_000;

struct Ctx {
    app: Router,
    state: AppState,
    pool: PgPool,
    t0: DateTime<Utc>,
    _guard: MutexGuard<'static, ()>,
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
        t0: Utc::now().trunc_subsecs(0),
        _guard: guard,
    }
}

/// `None` (test skipped) when the sandbox guest is not built.
async fn setup() -> Option<Ctx> {
    let c = setup_with(|s| s).await;
    if c.state.sandbox.is_none() {
        eprintln!("SKIP: sandbox guest not built; run crates/sandbox/build-guest.sh");
        return None;
    }
    Some(c)
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

fn code_graph(trigger: &str, source: &str) -> Value {
    let params = if trigger == "trigger.schedule" {
        json!({"every_secs": 60})
    } else {
        json!({})
    };
    json!({
        "schema_version": 1,
        "nodes": [
            {"id": "t", "type": trigger, "params": params},
            {"id": "code", "type": "custom.code", "params": {"source": source}}
        ],
        "edges": [{"from": "t", "from_port": "out", "to": "code", "to_port": "in"}]
    })
}

/// Policy: 10 USDC per interaction, approval above 5, 50 in total.
fn policy(overrides: Value) -> Value {
    let mut p = json!({
        "max_per_interaction": "10000000", "max_aggregate_exposure": "50000000",
        "approval_threshold": "5000000", "allowed_interaction_types": ["challenge"],
        "min_conviction_bps": 6000, "stop_loss": null,
    });
    for (k, v) in overrides.as_object().unwrap() {
        p[k] = v.clone();
    }
    p
}

fn agent_input(name: &str, graph: Value, policy: Value) -> Value {
    json!({
        "name": name, "description": "custom code test",
        "config": {"goals": "test", "strategy": "Follow momentum.", "categories": ["crypto"],
                   "horizon_secs": 3600, "risk_tolerance": "medium", "schedule_secs": 60},
        "policy": policy, "graph": graph,
    })
}

async fn fund(c: &Ctx, id: Uuid, usdc: i64) {
    let amount = json!({ "amount": (usdc * USDC).to_string() });
    post(&c.app, "/api/wallet/simulated-deposit", amount.clone()).await;
    post(&c.app, &format!("/api/agents/{id}/allocate"), amount).await;
}

async fn create(c: &Ctx, input: Value, usdc: i64) -> Uuid {
    let agent = post(&c.app, "/api/agents", input).await;
    let id = uuid(&agent["id"]);
    fund(c, id, usdc).await;
    id
}

/// Code that behaves on the dry run's sample market and runs `body` (the
/// body of `decide(ctx)`) on a real one: the save-time check is a usability
/// aid, not a security boundary, so the runtime must cope with this.
fn evasive(body: &str) -> String {
    format!("function decide(ctx) {{ if (ctx.market.source === 'sample') return []; {body} }}")
}

/// A scheduled agent whose whole flow is `source`.
async fn proposer(c: &Ctx, name: &str, source: &str) -> Uuid {
    let graph = code_graph("trigger.schedule", source);
    create(c, agent_input(name, graph, policy(json!({}))), 100).await
}

/// Answers every challenge by accepting the stake on the table; generous
/// limits so its own policy never gets in the way.
async fn accepter(c: &Ctx) -> Uuid {
    let graph = code_graph(
        "trigger.challenge_received",
        "function decide(ctx) { return [{ type: 'accept_challenge', stake: ctx.incoming.terms.stake }]; }",
    );
    let p = policy(json!({"max_per_interaction": "30000000", "approval_threshold": "30000000"}));
    create(c, agent_input("Accepter", graph, p), 100).await
}

async fn template_agent(c: &Ctx, name: &str) -> Uuid {
    let templates = get(&c.app, "/api/templates").await;
    let t = templates
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == name)
        .unwrap_or_else(|| panic!("template {name}"))
        .clone();
    let input = json!({
        "name": name, "description": t["description"], "config": t["config"],
        "policy": t["policy"], "graph": t["graph"],
    });
    create(c, input, 100).await
}

async fn tick(c: &Ctx, at: DateTime<Utc>) -> lifecycle::TickReport {
    lifecycle::tick(&c.state, at).await.unwrap()
}

async fn activity(c: &Ctx, agent: Uuid, kind: &str) -> Vec<Value> {
    let items = get(&c.app, &format!("/api/activity?agent_id={agent}")).await;
    items
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["kind"] == kind)
        .cloned()
        .collect()
}

async fn scalar(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Every row that records money, a decision or a commitment.
async fn footprint(pool: &PgPool) -> [i64; 6] {
    let mut out = [0; 6];
    for (i, table) in [
        "ledger_entries",
        "activity",
        "agent_runs",
        "interactions",
        "approvals",
        "negotiation_rounds",
    ]
    .iter()
    .enumerate()
    {
        out[i] = scalar(pool, &format!("SELECT COUNT(*) FROM {table}")).await;
    }
    out
}

async fn available(pool: &PgPool, agent: Uuid) -> i64 {
    scalar(
        pool,
        &format!(
            "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries \
             WHERE account = 'agent:{agent}:available'"
        ),
    )
    .await
}

async fn committed(pool: &PgPool, agent: Uuid) -> i64 {
    scalar(
        pool,
        &format!(
            "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries \
             WHERE account = 'agent:{agent}:committed'"
        ),
    )
    .await
}

const PROPOSE_20: &str =
    "function decide(ctx) { return [{ type: 'propose_challenge', stake: '20000000' }]; }";
const PROPOSE_8: &str =
    "function decide(ctx) { return [{ type: 'propose_challenge', stake: '8000000' }]; }";

// ---------------------------------------------------------------------------

#[tokio::test]
async fn custom_code_is_limited_by_policy_and_approval() {
    let Some(c) = setup().await else { return };
    let p = proposer(&c, "Coder", PROPOSE_20).await;
    let _r = accepter(&c).await;

    // 20 USDC is over the 10 USDC per-interaction limit: denied, nothing moves.
    let report = tick(&c, c.t0).await;
    assert_eq!((report.proposed, report.denied), (0, 1), "{report:?}");
    let denied = activity(&c, p, "proposal_denied").await;
    assert_eq!(denied.len(), 1);
    assert_eq!(
        denied[0]["detail"]["reasons"],
        json!(["exceeds_max_per_interaction"])
    );
    assert!(
        get(&c.app, "/api/interactions")
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(available(&c.pool, p).await, 100 * USDC);
    let engine: String =
        sqlx::query_scalar("SELECT reasoner FROM agent_runs WHERE agent_id = $1 LIMIT 1")
            .bind(p)
            .fetch_one(&c.pool)
            .await
            .unwrap();
    assert_eq!(engine, "custom_code");

    // The owner edits the code to 8 USDC: within the limit, above the 5 USDC
    // approval threshold. Proposed, accepted, then held for the owner.
    let mut input = get(&c.app, &format!("/api/agents/{p}")).await;
    input["graph"] = code_graph("trigger.schedule", PROPOSE_8);
    let input = json!({
        "name": input["name"], "description": input["description"], "config": input["config"],
        "policy": input["policy"], "graph": input["graph"],
    });
    let (status, body) = send(&c.app, "PUT", &format!("/api/agents/{p}"), Some(input)).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let report = tick(&c, c.t0 + Duration::seconds(60)).await;
    assert_eq!(
        (
            report.proposed,
            report.accepted,
            report.approvals_requested,
            report.funded
        ),
        (1, 1, 1, 0),
        "{report:?}"
    );
    let list = get(&c.app, "/api/interactions").await;
    assert_eq!(list[0]["state"], "awaiting_approval");
    assert_eq!(list[0]["stake"], "8000000");
    let approvals = get(&c.app, "/api/approvals").await;
    assert_eq!(approvals.as_array().unwrap().len(), 1);
    assert_eq!(approvals[0]["agent_name"], "Coder");
    assert_eq!(
        committed(&c.pool, p).await,
        0,
        "nothing moves before approval"
    );
    let proposed = activity(&c, p, "challenge_proposed").await;
    assert!(
        proposed[0]["summary"]
            .as_str()
            .unwrap()
            .contains("asked to approve")
    );
}

#[tokio::test]
async fn malicious_code_cannot_stall_a_tick_or_affect_other_agents() {
    let Some(c) = setup().await else { return };
    let looper = proposer(&c, "Looper", &evasive("for (;;) {}")).await;
    let bomber = proposer(
        &c,
        "Bomber",
        &evasive("const a = []; for (;;) a.push(new ArrayBuffer(1 << 20));"),
    )
    .await;
    let deep = proposer(
        &c,
        "Recurser",
        &evasive("const f = (n) => f(n + 1) + 1; return [f(0)];"),
    )
    .await;
    let bull = template_agent(&c, "Momentum bull").await;
    let _skeptic = template_agent(&c, "Skeptic").await;
    let before = [
        available(&c.pool, looper).await,
        available(&c.pool, bomber).await,
        available(&c.pool, deep).await,
    ];

    let t = Instant::now();
    let report = tick(&c, c.t0).await;
    let took = t.elapsed();
    assert!(took.as_secs() < 10, "tick took {took:?}");
    // The well-behaved agent still proposed in the same tick.
    assert_eq!(report.proposed, 1, "{report:?}");
    let list = get(&c.app, "/api/interactions").await;
    assert_eq!(list[0]["proposer_id"], json!(bull));

    for (agent, want) in [
        (looper, "too much CPU"),
        (bomber, "too much"),
        (deep, "recursed too deeply"),
    ] {
        let failed = activity(&c, agent, "custom_code_failed").await;
        assert_eq!(failed.len(), 1, "{agent}");
        let summary = failed[0]["summary"].as_str().unwrap();
        assert!(summary.contains(want), "{summary}");
        assert!(summary.contains("no funds moved"), "{summary}");
        eprintln!("{summary}");
    }
    let after = [
        available(&c.pool, looper).await,
        available(&c.pool, bomber).await,
        available(&c.pool, deep).await,
    ];
    assert_eq!(before, after);

    // A persisting failure is logged once, and the next tick still works.
    tick(&c, c.t0 + Duration::seconds(60)).await;
    assert_eq!(activity(&c, looper, "custom_code_failed").await.len(), 1);
}

#[tokio::test]
async fn bad_output_is_recorded_and_nothing_moves() {
    let Some(c) = setup().await else { return };
    let cases = [
        (
            "return [{ type: 'propose_challenge', stake: 5 }];",
            "invalid intents",
        ),
        (
            "return [{ type: 'propose_challenge', stake: '1', to: '0xdead' }];",
            "invalid intents",
        ),
        (
            "return [{ type: 'commit_funds', interaction_type: 'challenge', stake: '1' }];",
            "committed by the runtime",
        ),
        (
            "return [{ type: 'propose_challenge', stake: '0' }];",
            "invalid terms",
        ),
    ];
    let mut agents = Vec::new();
    for (i, (body, _)) in cases.iter().enumerate() {
        agents.push(proposer(&c, &format!("Bad {i}"), &evasive(body)).await);
    }
    let _r = accepter(&c).await;
    let ledger_before = scalar(&c.pool, "SELECT COUNT(*) FROM ledger_entries").await;

    let report = tick(&c, c.t0).await;
    assert_eq!((report.proposed, report.denied), (0, 0), "{report:?}");
    for (agent, (_, want)) in agents.iter().zip(cases) {
        let failed = activity(&c, *agent, "custom_code_failed").await;
        assert_eq!(failed.len(), 1);
        let summary = failed[0]["summary"].as_str().unwrap();
        assert!(summary.contains(want), "{summary}");
        assert!(failed[0]["detail"]["code"]["error"].is_string());
        let (status, engine): (String, String) = sqlx::query_as(
            "SELECT status, reasoner FROM agent_runs WHERE agent_id = $1 ORDER BY ts DESC LIMIT 1",
        )
        .bind(agent)
        .fetch_one(&c.pool)
        .await
        .unwrap();
        assert_eq!((status.as_str(), engine.as_str()), ("error", "custom_code"));
    }
    assert_eq!(
        scalar(&c.pool, "SELECT COUNT(*) FROM interactions").await,
        0
    );
    assert_eq!(
        scalar(&c.pool, "SELECT COUNT(*) FROM ledger_entries").await,
        ledger_before
    );
}

#[tokio::test]
async fn test_run_reports_verdicts_and_has_no_effect() {
    let Some(c) = setup().await else { return };
    let source = r#"function decide(ctx) {
        console.log("available", ctx.balances.available, "move", ctx.market.move_bps);
        return [
          { type: "propose_challenge", stake: "20000000" },
          { type: "propose_challenge", stake: "8000000" },
          { type: "propose_challenge", stake: "2000000" },
        ];
      }"#;
    let a = proposer(&c, "Tester", source).await;
    let path = format!("/api/agents/{a}/code/test-run");
    let before = footprint(&c.pool).await;

    let out = post(&c.app, &path, json!({})).await;
    assert_eq!(out["ok"], true, "{out}");
    let verdicts: Vec<&Value> = out["intents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| &i["decision"]["result"])
        .collect();
    assert_eq!(verdicts, ["deny", "require_approval", "allow"]);
    assert_eq!(
        out["intents"][0]["decision"]["reasons"],
        json!(["exceeds_max_per_interaction"])
    );
    assert_eq!(
        out["intents"][1]["intent"],
        json!({"type": "propose_challenge", "stake": "8000000"})
    );
    assert_eq!(out["logs"], json!(["available 100000000 move 300"]));
    assert!(out["fuel_used"].as_u64().unwrap() > 0);
    assert_eq!(out["ctx"]["balances"]["available"], "100000000");
    assert!(!out["ctx"].to_string().contains("max_per_interaction"));
    assert_eq!(out["module_sha256"].as_str().unwrap().len(), 64);

    // Unsaved code, and code that fails.
    let out = post(
        &c.app,
        &path,
        json!({"source": "function decide() { return [{type: 'reject'}]; }"}),
    )
    .await;
    assert_eq!(out["intents"][0]["intent"], json!({"type": "reject"}));
    let out = post(
        &c.app,
        &path,
        json!({"source": "function decide() { for (;;) {} }"}),
    )
    .await;
    assert_eq!(out["ok"], false);
    assert_eq!(out["error"]["code"], "out_of_fuel");

    assert_eq!(footprint(&c.pool).await, before, "test-run wrote something");

    // An agent without custom code needs `source`; oversize source is refused.
    let bull = template_agent(&c, "Momentum bull").await;
    let (status, body) = send(
        &c.app,
        "POST",
        &format!("/api/agents/{bull}/code/test-run"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "no_custom_code");
    let (status, _) = send(
        &c.app,
        "POST",
        &path,
        Some(json!({"source": "x".repeat(70_000)})),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = send(
        &c.app,
        "POST",
        &format!("/api/agents/{}/code/test-run", Uuid::new_v4()),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn saving_code_that_does_not_run_is_rejected() {
    let Some(c) = setup().await else { return };
    for (source, want) in [
        ("function decide( {", "strategy error"),
        ("function decide() { throw new Error('nope'); }", "nope"),
        ("const x = 1;", "not defined"),
        ("function decide() { return 5; }", "invalid intents"),
        ("function decide() { for (;;) {} }", "too much CPU"),
    ] {
        let input = agent_input(
            "Broken",
            code_graph("trigger.schedule", source),
            policy(json!({})),
        );
        let (status, body) = send(&c.app, "POST", "/api/agents", Some(input)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{source}: {body}");
        let detail = &body["error"]["details"][0];
        assert_eq!(detail["path"], "graph.nodes.code");
        assert_eq!(detail["code"], "custom_code_failed");
        assert!(
            detail["message"].as_str().unwrap().contains(want),
            "{source}: {detail}"
        );
    }
    assert_eq!(scalar(&c.pool, "SELECT COUNT(*) FROM agents").await, 0);

    // An edit to broken code is refused and the version stays.
    let a = proposer(&c, "Fine", "function decide() { return []; }").await;
    let mut input = agent_input(
        "Fine",
        code_graph("trigger.schedule", "function decide() { throw 1; }"),
        policy(json!({})),
    );
    let (status, _) = send(
        &c.app,
        "PUT",
        &format!("/api/agents/{a}"),
        Some(input.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(get(&c.app, &format!("/api/agents/{a}")).await["version"], 1);
    input["graph"] = code_graph("trigger.schedule", "function decide() { return []; }");
    let (status, body) = send(&c.app, "PUT", &format!("/api/agents/{a}"), Some(input)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], 2);
}

#[tokio::test]
async fn without_a_sandbox_custom_code_agents_record_it_and_nothing_moves() {
    let c = setup_with(|s| AppState { sandbox: None, ..s }).await;
    // Saving is allowed (the dry run is skipped) so the config is not lost.
    let a = proposer(&c, "Coder", PROPOSE_8).await;
    let _r = accepter(&c).await;
    let ledger_before = scalar(&c.pool, "SELECT COUNT(*) FROM ledger_entries").await;

    let report = tick(&c, c.t0).await;
    assert_eq!(report.proposed, 0);
    let failed = activity(&c, a, "custom_code_failed").await;
    assert_eq!(failed.len(), 1);
    assert!(
        failed[0]["summary"]
            .as_str()
            .unwrap()
            .contains("sandbox is not available"),
        "{}",
        failed[0]["summary"]
    );
    assert_eq!(
        scalar(&c.pool, "SELECT COUNT(*) FROM ledger_entries").await,
        ledger_before
    );
    let (status, body) = send(
        &c.app,
        "POST",
        &format!("/api/agents/{a}/code/test-run"),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["error"]["code"], "sandbox_unavailable");
}
