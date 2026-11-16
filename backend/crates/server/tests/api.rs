//! Integration tests against a real Postgres (`aen_test` by default, override
//! with `TEST_DATABASE_URL`). Tests share one database and the single demo
//! user's wallet, so they run one at a time and start from empty tables.

mod common;

use aen_core::graph::Graph;
use aen_core::policy::AgentPolicy;
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
    // The OS user is named explicitly: sqlx's own lookup can fail in sandboxes.
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| match std::env::var("USER") {
        Ok(user) => format!("postgres://{user}@localhost/aen_test"),
        Err(_) => "postgres://localhost/aen_test".to_owned(),
    });
    let pool = PgPool::connect(&url).await.expect("connect to test db");
    MIGRATOR.run(&pool).await.expect("run migrations");
    common::wipe(&pool).await;
    let origin = DEFAULT_CORS_ORIGIN.parse().unwrap();
    Ctx {
        app: app(pool.clone(), origin),
        pool,
        _guard: guard,
    }
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
    key: Option<&str>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(key) = key {
        req = req.header("Idempotency-Key", key);
    }
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
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

async fn get(app: &Router, path: &str) -> (StatusCode, Value) {
    send(app, "GET", path, None, None).await
}

async fn post(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(app, "POST", path, Some(body), None).await
}

fn graph() -> Value {
    json!({
        "schema_version": 1,
        "nodes": [
            {"id": "n1", "type": "trigger.schedule", "params": {"every_secs": 60}, "ui": {"x": 12.5, "y": 40.0}},
            {"id": "n2", "type": "reason.evaluate_opportunity", "params": {}},
            {"id": "n3", "type": "action.request_approval", "params": {}}
        ],
        "edges": [
            {"from": "n1", "from_port": "out", "to": "n2", "to_port": "in"},
            {"from": "n2", "from_port": "out", "to": "n3", "to_port": "in"}
        ]
    })
}

fn agent_input(name: &str) -> Value {
    json!({
        "name": name,
        "description": "test agent",
        "config": {
            "goals": "grow", "strategy": "momentum", "categories": ["crypto"],
            "horizon_secs": 3600, "risk_tolerance": "medium", "schedule_secs": 60
        },
        "policy": {
            "max_per_interaction": "10000000",
            "max_aggregate_exposure": "50000000",
            "approval_threshold": "5000000",
            "allowed_interaction_types": ["challenge"],
            "min_conviction_bps": 6000,
            "stop_loss": null
        },
        "graph": graph()
    })
}

async fn create_agent(app: &Router, name: &str) -> Value {
    let (status, agent) = post(app, "/api/agents", agent_input(name)).await;
    assert_eq!(status, StatusCode::CREATED, "{agent}");
    agent
}

fn id(agent: &Value) -> &str {
    agent["id"].as_str().unwrap()
}

async fn count(pool: &PgPool, table: &'static str) -> i64 {
    let sql = match table {
        "agents" => "SELECT COUNT(*) FROM agents",
        "agent_versions" => "SELECT COUNT(*) FROM agent_versions",
        "ledger_entries" => "SELECT COUNT(*) FROM ledger_entries",
        "activity" => "SELECT COUNT(*) FROM activity",
        _ => panic!("unknown table"),
    };
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

fn detail_paths(body: &Value) -> Vec<&str> {
    body["error"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn health_and_unknown_route() {
    let ctx = setup().await;
    assert_eq!(
        get(&ctx.app, "/api/health").await,
        (StatusCode::OK, json!({"status": "ok"}))
    );
    let (status, body) = get(&ctx.app, "/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn templates_are_seeded_and_all_valid() {
    let ctx = setup().await;
    let (status, body) = get(&ctx.app, "/api/templates").await;
    assert_eq!(status, StatusCode::OK);
    let templates = body.as_array().unwrap();
    assert_eq!(templates.len(), 3);
    for t in templates {
        let name = t["name"].as_str().unwrap();
        let graph: Graph = serde_json::from_value(t["graph"].clone()).unwrap();
        assert_eq!(aen_core::graph::validate(&graph), vec![], "{name}");
        let policy: AgentPolicy = serde_json::from_value(t["policy"].clone()).unwrap();
        assert_eq!(policy.validate(), vec![], "{name}");
        assert!(graph.nodes.iter().all(|n| n.ui.is_some()), "{name}");

        // A template can be used as-is to create an agent.
        let input = json!({
            "name": name, "description": t["description"],
            "config": t["config"], "policy": t["policy"], "graph": t["graph"]
        });
        let (status, agent) = post(&ctx.app, "/api/agents", input).await;
        assert_eq!(status, StatusCode::CREATED, "{name}: {agent}");
        // The validate endpoint agrees.
        let (_, v) = post(&ctx.app, "/api/graph/validate", t["graph"].clone()).await;
        assert_eq!(v, json!({"valid": true, "errors": []}), "{name}");
    }
}

#[tokio::test]
async fn block_catalog_and_graph_validation() {
    let ctx = setup().await;
    let (status, blocks) = get(&ctx.app, "/api/graph/blocks").await;
    assert_eq!(status, StatusCode::OK);
    let blocks = blocks.as_array().unwrap();
    assert_eq!(blocks.len(), 16);
    for key in [
        "type",
        "kind",
        "label",
        "description",
        "inputs",
        "outputs",
        "params_schema",
    ] {
        assert!(blocks[0].get(key).is_some(), "{key}");
    }

    // Invalid graphs are still a 200: the errors are the answer.
    let mut bad = graph();
    bad["nodes"][1]["type"] = json!("reason.unknown");
    let (status, body) = post(&ctx.app, "/api/graph/validate", bad).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
    assert_eq!(body["errors"][0]["code"], "unknown_node_type");
    assert_eq!(body["errors"][0]["node_id"], "n2");
    assert!(body["errors"][0]["message"].is_string());
}

#[tokio::test]
async fn create_get_and_list_agent() {
    let ctx = setup().await;
    let agent = create_agent(&ctx.app, "  Alpha  ").await;
    assert_eq!(agent["name"], "Alpha");
    assert_eq!(agent["status"], "active");
    assert_eq!(agent["env"], "simulated");
    assert_eq!(agent["wallet_address"], Value::Null);
    assert_eq!(agent["version"], 1);
    assert_eq!(agent["policy"], agent_input("x")["policy"]);
    assert_eq!(agent["config"], agent_input("x")["config"]);
    // `ui` is stored and returned untouched.
    assert_eq!(agent["graph"], graph());
    assert_eq!(
        agent["balances"],
        json!({"available": "0", "committed": "0"})
    );
    assert_eq!(
        agent["stats"],
        json!({"realized_pnl": "0", "settled": 0, "wins": 0})
    );
    assert!(agent["created_at"].as_str().unwrap().contains('T'));

    let (status, fetched) = get(&ctx.app, &format!("/api/agents/{}", id(&agent))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched, agent);

    create_agent(&ctx.app, "Beta").await;
    let (_, list) = get(&ctx.app, "/api/agents").await;
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["Alpha", "Beta"]);
}

#[tokio::test]
async fn unknown_agent_is_404_and_bad_id_is_400() {
    let ctx = setup().await;
    let missing = "/api/agents/00000000-0000-4000-8000-00000000dead";
    for path in [
        missing.to_owned(),
        format!("{missing}/versions"),
        format!("{missing}/ledger"),
    ] {
        let (status, body) = get(&ctx.app, &path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(body["error"]["code"], "not_found");
    }
    let (status, _) = send(&ctx.app, "PUT", missing, Some(agent_input("x")), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(
        &ctx.app,
        &format!("{missing}/status"),
        json!({"status": "paused"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(
        &ctx.app,
        &format!("{missing}/allocate"),
        json!({"amount": "1"}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, body) = get(&ctx.app, "/api/agents/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
}

#[tokio::test]
async fn update_creates_new_version_and_keeps_the_old_one() {
    let ctx = setup().await;
    let agent = create_agent(&ctx.app, "Alpha").await;
    let path = format!("/api/agents/{}", id(&agent));

    let mut input = agent_input("Alpha v2");
    input["policy"]["max_per_interaction"] = json!("20000000");
    input["config"]["strategy"] = json!("contrarian");
    let (status, updated) = send(&ctx.app, "PUT", &path, Some(input), None).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["version"], 2);
    assert_eq!(updated["name"], "Alpha v2");
    assert_eq!(updated["policy"]["max_per_interaction"], "20000000");
    assert_eq!(updated["created_at"], agent["created_at"]);

    let (status, versions) = get(&ctx.app, &format!("{path}/versions")).await;
    assert_eq!(status, StatusCode::OK);
    let versions = versions.as_array().unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["version"], 2);
    assert_eq!(versions[0]["config"]["strategy"], "contrarian");
    // Version 1 is exactly what was created.
    assert_eq!(versions[1]["version"], 1);
    assert_eq!(versions[1]["policy"], agent["policy"]);
    assert_eq!(versions[1]["config"], agent["config"]);
    assert_eq!(versions[1]["graph"], agent["graph"]);

    // The database itself refuses to rewrite history.
    let err = sqlx::query("UPDATE agent_versions SET policy = '{}'")
        .execute(&ctx.pool)
        .await;
    assert!(err.is_err());
    let err = sqlx::query("DELETE FROM agent_versions")
        .execute(&ctx.pool)
        .await;
    assert!(err.is_err());
}

#[tokio::test]
async fn invalid_policy_is_422_with_field_paths() {
    let ctx = setup().await;
    let mut input = agent_input("Bad");
    input["policy"]["approval_threshold"] = json!("10000001"); // > max_per_interaction
    input["policy"]["allowed_interaction_types"] = json!([]);
    input["policy"]["min_conviction_bps"] = json!(10001);
    let (status, body) = post(&ctx.app, "/api/agents", input).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
    assert_eq!(
        detail_paths(&body),
        [
            "policy.approval_threshold",
            "policy.allowed_interaction_types",
            "policy.min_conviction_bps"
        ]
    );
    let first = &body["error"]["details"][0];
    assert_eq!(
        first["code"],
        "approval_threshold_above_max_per_interaction"
    );
    assert!(first["message"].is_string());

    let mut input = agent_input("Bad");
    input["policy"]["max_per_interaction"] = json!("50000001"); // > aggregate
    let (status, body) = post(&ctx.app, "/api/agents", input).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail_paths(&body), ["policy.max_per_interaction"]);

    assert_eq!(count(&ctx.pool, "agents").await, 0);
}

#[tokio::test]
async fn malformed_bodies_are_rejected() {
    let ctx = setup().await;
    // Money must be a string of digits: numbers, decimals and negatives fail.
    for bad in [json!(10000000), json!("10.5"), json!("-1"), json!("abc")] {
        let mut input = agent_input("Bad");
        input["policy"]["max_per_interaction"] = bad.clone();
        let (status, body) = post(&ctx.app, "/api/agents", input).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        assert_eq!(detail_paths(&body), ["policy.max_per_interaction"], "{bad}");
        assert_eq!(body["error"]["details"][0]["code"], "invalid_value");
    }
    let mut input = agent_input("Bad");
    input["config"]["risk_tolerance"] = json!("yolo");
    let (status, body) = post(&ctx.app, "/api/agents", input).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail_paths(&body), ["config.risk_tolerance"]);

    let mut input = agent_input("Bad");
    input.as_object_mut().unwrap().remove("policy");
    let (status, _) = post(&ctx.app, "/api/agents", input).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let mut input = agent_input(" ");
    input["config"]["schedule_secs"] = json!(0);
    let (status, body) = post(&ctx.app, "/api/agents", input).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail_paths(&body), ["name", "config.schedule_secs"]);

    // Not JSON at all.
    let req = Request::builder()
        .method("POST")
        .uri("/api/agents")
        .header("content-type", "application/json")
        .body(Body::from("{not json"))
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    assert_eq!(count(&ctx.pool, "agents").await, 0);
}

#[tokio::test]
async fn invalid_graph_is_422_on_create_and_update() {
    let ctx = setup().await;
    let mut input = agent_input("Bad");
    input["graph"]["nodes"][0]["params"] = json!({"every_secs": 0});
    input["graph"]["edges"].as_array_mut().unwrap().remove(1); // n2 now leads nowhere, n3 is unreachable
    let (status, body) = post(&ctx.app, "/api/agents", input.clone()).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "validation_failed");
    let details = body["error"]["details"].as_array().unwrap();
    let got: Vec<(&str, &str)> = details
        .iter()
        .map(|d| (d["path"].as_str().unwrap(), d["code"].as_str().unwrap()))
        .collect();
    assert_eq!(
        got,
        [
            ("graph.nodes.n1", "invalid_params"),
            ("graph.nodes.n3", "unreachable_node"),
            ("graph.nodes.n2", "dead_end"),
        ]
    );

    let mut no_trigger = agent_input("Bad");
    no_trigger["graph"] = json!({"schema_version": 1, "nodes": [], "edges": []});
    let (status, body) = post(&ctx.app, "/api/agents", no_trigger).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail_paths(&body), ["graph"]);
    assert_eq!(count(&ctx.pool, "agents").await, 0);

    // An invalid update leaves the agent at its current version.
    let agent = create_agent(&ctx.app, "Good").await;
    let path = format!("/api/agents/{}", id(&agent));
    let (status, _) = send(&ctx.app, "PUT", &path, Some(input), None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, after) = get(&ctx.app, &path).await;
    assert_eq!(after, agent);
    assert_eq!(count(&ctx.pool, "agent_versions").await, 1);
}

#[tokio::test]
async fn pause_and_resume() {
    let ctx = setup().await;
    let agent = create_agent(&ctx.app, "Alpha").await;
    let path = format!("/api/agents/{}/status", id(&agent));

    let (status, paused) = post(&ctx.app, &path, json!({"status": "paused"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused["status"], "paused");
    assert_eq!(paused["version"], 1);

    let (_, resumed) = post(&ctx.app, &path, json!({"status": "active"})).await;
    assert_eq!(resumed["status"], "active");

    let (status, body) = post(&ctx.app, &path, json!({"status": "deleted"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail_paths(&body), ["status"]);
}

#[tokio::test]
async fn deposit_allocate_deallocate_balances() {
    let ctx = setup().await;
    let agent = create_agent(&ctx.app, "Alpha").await;
    let base = format!("/api/agents/{}", id(&agent));

    let (status, wallet) = get(&ctx.app, "/api/wallet").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        wallet,
        json!({"env": "simulated", "unallocated": "0", "allocated": "0", "committed": "0", "entries": []})
    );

    let (status, wallet) = post(
        &ctx.app,
        "/api/wallet/simulated-deposit",
        json!({"amount": "100000000"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{wallet}");
    assert_eq!(wallet["env"], "simulated");
    assert_eq!(wallet["unallocated"], "100000000");
    assert_eq!(wallet["allocated"], "0");

    let (status, a) = post(
        &ctx.app,
        &format!("{base}/allocate"),
        json!({"amount": "30000000"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(
        a["balances"],
        json!({"available": "30000000", "committed": "0"})
    );

    let (status, a) = post(
        &ctx.app,
        &format!("{base}/deallocate"),
        json!({"amount": "12000000"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["balances"]["available"], "18000000");
    // Moving funds is not profit.
    assert_eq!(
        a["stats"],
        json!({"realized_pnl": "0", "settled": 0, "wins": 0})
    );

    let (_, wallet) = get(&ctx.app, "/api/wallet").await;
    assert_eq!(wallet["unallocated"], "82000000");
    assert_eq!(wallet["allocated"], "18000000");
    assert_eq!(wallet["committed"], "0");
    // Newest first: deallocate, allocate, deposit.
    let entries = wallet["entries"].as_array().unwrap();
    let seen: Vec<(&str, &str)> = entries
        .iter()
        .map(|e| (e["reason"].as_str().unwrap(), e["delta"].as_str().unwrap()))
        .collect();
    assert_eq!(
        seen,
        [
            ("deallocate", "12000000"),
            ("allocate", "-30000000"),
            ("deposit", "100000000")
        ]
    );
    assert!(entries.iter().all(|e| e["env"] == "simulated"
        && e["account"] == "user:unallocated"
        && e["interaction_id"].is_null()
        && e["tx_hash"].is_null()));

    let (status, ledger) = get(&ctx.app, &format!("{base}/ledger")).await;
    assert_eq!(status, StatusCode::OK);
    let ledger = ledger.as_array().unwrap();
    assert_eq!(ledger.len(), 2);
    assert_eq!(ledger[0]["reason"], "deallocate");
    assert_eq!(ledger[0]["delta"], "-12000000");
    assert_eq!(
        ledger[0]["account"],
        format!("agent:{}:available", id(&agent))
    );
    assert_eq!(ledger[1]["delta"], "30000000");

    // Double entry: the whole ledger sums to zero.
    let total: i64 = sqlx::query_scalar("SELECT SUM(delta)::BIGINT FROM ledger_entries")
        .fetch_one(&ctx.pool)
        .await
        .unwrap();
    assert_eq!(total, 0);

    // Deallocating more than is available fails and changes nothing.
    let (status, body) = post(
        &ctx.app,
        &format!("{base}/deallocate"),
        json!({"amount": "18000001"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "insufficient_funds");
    // Exactly the available balance is fine.
    let (status, a) = post(
        &ctx.app,
        &format!("{base}/deallocate"),
        json!({"amount": "18000000"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(a["balances"]["available"], "0");
    let (_, wallet) = get(&ctx.app, "/api/wallet").await;
    assert_eq!(wallet["unallocated"], "100000000");
}

#[tokio::test]
async fn over_allocation_and_bad_amounts_are_rejected() {
    let ctx = setup().await;
    let agent = create_agent(&ctx.app, "Alpha").await;
    let path = format!("/api/agents/{}/allocate", id(&agent));
    post(
        &ctx.app,
        "/api/wallet/simulated-deposit",
        json!({"amount": "100"}),
    )
    .await;

    let (status, body) = post(&ctx.app, &path, json!({"amount": "101"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "insufficient_funds");

    for bad in [json!("0"), json!(5), json!("-5"), json!("1.5"), json!("")] {
        let (status, body) = post(&ctx.app, &path, json!({"amount": bad})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{bad}");
        assert_eq!(detail_paths(&body), ["amount"], "{bad}");
        let (status, _) = post(
            &ctx.app,
            "/api/wallet/simulated-deposit",
            json!({"amount": bad}),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "deposit {bad}");
    }
    // Nothing was written by any of the failures (the deposit is 2 rows).
    assert_eq!(count(&ctx.pool, "ledger_entries").await, 2);

    let (status, a) = post(&ctx.app, &path, json!({"amount": "100"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(a["balances"]["available"], "100");
    let (status, _) = post(&ctx.app, &path, json!({"amount": "1"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn idempotency_key_replay_does_not_double_post() {
    let ctx = setup().await;
    let agent = create_agent(&ctx.app, "Alpha").await;
    let path = format!("/api/agents/{}/allocate", id(&agent));

    let deposit = json!({"amount": "1000"});
    for _ in 0..3 {
        let (status, wallet) = send(
            &ctx.app,
            "POST",
            "/api/wallet/simulated-deposit",
            Some(deposit.clone()),
            Some("dep-1"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(wallet["unallocated"], "1000");
    }

    for _ in 0..3 {
        let (status, a) = send(
            &ctx.app,
            "POST",
            &path,
            Some(json!({"amount": "400"})),
            Some("alloc-1"),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(a["balances"]["available"], "400");
    }
    let (_, wallet) = get(&ctx.app, "/api/wallet").await;
    assert_eq!(wallet["unallocated"], "600");
    assert_eq!(count(&ctx.pool, "ledger_entries").await, 4);
    // Replays are not recorded as new activity: created + deposit + allocate.
    assert_eq!(count(&ctx.pool, "activity").await, 3);

    // The same key with a different request is a conflict, not a silent no-op.
    let (status, body) = send(
        &ctx.app,
        "POST",
        &path,
        Some(json!({"amount": "401"})),
        Some("alloc-1"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "idempotency_key_reused");

    // A new key posts again.
    let (_, a) = send(
        &ctx.app,
        "POST",
        &path,
        Some(json!({"amount": "400"})),
        Some("alloc-2"),
    )
    .await;
    assert_eq!(a["balances"]["available"], "800");

    // The database refuses to edit or delete entries.
    assert!(
        sqlx::query("UPDATE ledger_entries SET delta = 1")
            .execute(&ctx.pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM ledger_entries")
            .execute(&ctx.pool)
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_allocations_cannot_overdraw() {
    let ctx = setup().await;
    let a = create_agent(&ctx.app, "Alpha").await;
    let b = create_agent(&ctx.app, "Beta").await;
    post(
        &ctx.app,
        "/api/wallet/simulated-deposit",
        json!({"amount": "100"}),
    )
    .await;

    // 20 requests of 30 against a balance of 100: exactly 3 can succeed.
    let mut tasks = Vec::new();
    for i in 0..20 {
        let app = ctx.app.clone();
        let agent = if i % 2 == 0 { &a } else { &b };
        let path = format!("/api/agents/{}/allocate", id(agent));
        tasks.push(tokio::spawn(async move {
            post(&app, &path, json!({"amount": "30"})).await.0
        }));
    }
    let mut ok = 0;
    for task in tasks {
        match task.await.unwrap() {
            StatusCode::OK => ok += 1,
            StatusCode::UNPROCESSABLE_ENTITY => {}
            other => panic!("unexpected status {other}"),
        }
    }
    assert_eq!(ok, 3);

    let (_, wallet) = get(&ctx.app, "/api/wallet").await;
    assert_eq!(wallet["unallocated"], "10");
    assert_eq!(wallet["allocated"], "90");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_replays_of_one_key_post_once() {
    let ctx = setup().await;
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let app = ctx.app.clone();
        tasks.push(tokio::spawn(async move {
            send(
                &app,
                "POST",
                "/api/wallet/simulated-deposit",
                Some(json!({"amount": "50"})),
                Some("same-key"),
            )
            .await
            .0
        }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap(), StatusCode::OK);
    }
    let (_, wallet) = get(&ctx.app, "/api/wallet").await;
    assert_eq!(wallet["unallocated"], "50");
}

#[tokio::test]
async fn activity_is_recorded_newest_first_and_filterable() {
    let ctx = setup().await;
    let a = create_agent(&ctx.app, "Alpha").await;
    let b = create_agent(&ctx.app, "Beta").await;
    post(
        &ctx.app,
        "/api/wallet/simulated-deposit",
        json!({"amount": "25000000"}),
    )
    .await;
    post(
        &ctx.app,
        &format!("/api/agents/{}/allocate", id(&a)),
        json!({"amount": "1500000"}),
    )
    .await;
    post(
        &ctx.app,
        &format!("/api/agents/{}/status", id(&b)),
        json!({"status": "paused"}),
    )
    .await;

    let (status, all) = get(&ctx.app, "/api/activity").await;
    assert_eq!(status, StatusCode::OK);
    let kinds: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "agent_paused",
            "funds_allocated",
            "simulated_deposit",
            "agent_created",
            "agent_created"
        ]
    );
    assert_eq!(
        all[1]["summary"],
        "1.50 USDC (simulated) allocated to \"Alpha\""
    );
    assert_eq!(all[1]["detail"]["amount"], "1500000");
    assert_eq!(all[2]["agent_id"], Value::Null);
    assert_eq!(
        all[2]["summary"],
        "25.00 USDC of simulated funds added to the wallet"
    );

    let (_, only_a) = get(&ctx.app, &format!("/api/activity?agent_id={}", id(&a))).await;
    let only_a = only_a.as_array().unwrap();
    assert_eq!(only_a.len(), 2);
    assert!(only_a.iter().all(|i| i["agent_id"] == a["id"]));

    let (_, limited) = get(&ctx.app, "/api/activity?agent_id=&limit=2").await;
    assert_eq!(limited.as_array().unwrap().len(), 2);

    for bad in ["agent_id=nope", "limit=0", "limit=abc"] {
        let (status, _) = get(&ctx.app, &format!("/api/activity?{bad}")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
    }
}

#[tokio::test]
async fn cors_allows_the_frontend_origin() {
    let ctx = setup().await;
    let req = Request::builder()
        .method("OPTIONS")
        .uri("/api/agents")
        .header("origin", "http://localhost:3000")
        .header("access-control-request-method", "POST")
        .header(
            "access-control-request-headers",
            "content-type,idempotency-key",
        )
        .body(Body::empty())
        .unwrap();
    let res = ctx.app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        res.headers().get("access-control-allow-origin").unwrap(),
        "http://localhost:3000"
    );
}
