//! Approach block and firms against a real Postgres (`TEST_DATABASE_URL`,
//! default `aen_test`): the API, the approach gates in the lifecycle, the
//! scout -> lead -> analyst flow, task fees and the profit share through the
//! ledger. Market data is the deterministic fake.

mod common;

use std::sync::Arc;

use aen_core::challenge::Direction;
use aen_server::lifecycle::{self, CommitOutcome};
use aen_server::market::BoxFut;
use aen_server::market::fake::FakeMarket;
use aen_server::reasoner::{
    ReasonRequest, Reasoner, ReasonerError, ReasonerOutput, RuleBasedReasoner, Task,
};
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
    fake: Arc<FakeMarket>,
    t0: DateTime<Utc>,
    _guard: MutexGuard<'static, ()>,
}

async fn setup() -> Ctx {
    setup_with(|s| s).await
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
    let (s, out) = send(app, "GET", path, None).await;
    assert_eq!(s, StatusCode::OK, "GET {path}: {out}");
    out
}

async fn post(app: &Router, path: &str, body: Value) -> (StatusCode, Value) {
    send(app, "POST", path, Some(body)).await
}

async fn ok(app: &Router, path: &str, body: Value) -> Value {
    let (s, out) = post(app, path, body).await;
    assert!(s.is_success(), "POST {path}: {s} {out}");
    out
}

fn approach(collab: u32, ambition: u32, role: &str, markets: Value) -> Value {
    json!({"id": "approach", "type": "approach.profile",
           "params": {"stance": {"collaboration_bps": collab, "ambition_bps": ambition},
                      "firm_role": role, "markets": markets}})
}

/// A template graph without its approach node (D-69), so a test controls it.
fn without_approach(graph: &Value) -> Value {
    let mut graph = graph.clone();
    graph["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["type"] != "approach.profile");
    graph
}

/// An agent from a seeded template, optionally with an approach node and
/// policy overrides, funded with `funds` simulated USDC.
async fn agent(
    c: &Ctx,
    template: &str,
    name: &str,
    ap: Option<Value>,
    policy: Value,
    funds: i64,
) -> Uuid {
    let templates = get(&c.app, "/api/templates").await;
    let t = templates
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == template)
        .unwrap()
        .clone();
    let mut graph = without_approach(&t["graph"]);
    if let Some(node) = ap {
        graph["nodes"].as_array_mut().unwrap().push(node);
    }
    let mut p = t["policy"].clone();
    for (k, v) in policy.as_object().unwrap() {
        p[k] = v.clone();
    }
    let input = json!({"name": name, "description": "firm test", "config": t["config"],
                       "policy": p, "graph": graph});
    let a = ok(&c.app, "/api/agents", input).await;
    let id: Uuid = a["id"].as_str().unwrap().parse().unwrap();
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

async fn available(pool: &PgPool, agent: Uuid) -> i64 {
    balance(pool, &format!("agent:{agent}:available")).await
}

async fn assert_ledger_sound(pool: &PgPool) {
    let total: i64 =
        sqlx::query_scalar("SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(total, 0);
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

async fn kinds(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT kind FROM activity ORDER BY seq")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn tick(c: &Ctx, at: DateTime<Utc>) -> lifecycle::TickReport {
    lifecycle::tick(&c.state, at).await.unwrap()
}

fn lead_ap() -> Value {
    // Collaborative enough to lead, ambitious: min conviction 50 %, stake
    // target 100 % of max, up to 5 open deals.
    approach(5000, 10_000, "lead", json!({}))
}

/// Joins firms, never initiates challenges itself (collaboration > 70 %).
fn member_ap() -> Value {
    approach(8000, 5000, "member", json!({}))
}

fn firm_body(lead: Uuid) -> Value {
    json!({"lead_agent_id": lead, "name": "BTC desk", "mandate": "Hourly BTC contracts",
           "task_fee": "100000"})
}

fn detail_paths(err: &Value) -> Vec<String> {
    err["error"]["details"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap().to_owned())
        .collect()
}

// ---------------------------------------------------------------------------

#[tokio::test]
async fn approach_block_in_catalog_and_validation() {
    let c = setup().await;
    let blocks = get(&c.app, "/api/graph/blocks").await;
    let ap = blocks
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["type"] == "approach.profile")
        .expect("approach.profile in the catalog");
    assert_eq!(ap["kind"], "approach");
    assert_eq!(ap["inputs"], json!([]));
    assert_eq!(ap["outputs"], json!([]));

    // Bad params name the field under graph.nodes.<id>.params.
    let templates = get(&c.app, "/api/templates").await;
    let t = &templates[0];
    let mut graph = without_approach(&t["graph"]);
    graph["nodes"].as_array_mut().unwrap().push(approach(
        5000,
        10_001,
        "none",
        json!({"ranges": {"move_bps": {"min": 9, "max": 1}}}),
    ));
    let (s, err) = post(
        &c.app,
        "/api/agents",
        json!({"name": "x", "description": "", "config": t["config"], "policy": t["policy"],
               "graph": graph}),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
    let paths = detail_paths(&err);
    assert!(
        paths.contains(&"graph.nodes.approach.params.stance.ambition_bps".to_owned()),
        "{paths:?}"
    );
    assert!(
        paths.contains(&"graph.nodes.approach.params.markets.ranges.move_bps".to_owned()),
        "{paths:?}"
    );
}

#[tokio::test]
async fn firm_api_membership_rules() {
    let c = setup().await;
    let plain = agent(&c, "Momentum bull", "Plain", None, json!({}), 10).await;
    let lead = agent(&c, "Momentum bull", "Lead", Some(lead_ap()), json!({}), 10).await;
    let m1 = agent(&c, "Momentum bull", "M1", Some(member_ap()), json!({}), 0).await;
    let m2 = agent(&c, "Momentum bull", "M2", Some(member_ap()), json!({}), 0).await;
    let m3 = agent(&c, "Momentum bull", "M3", Some(member_ap()), json!({}), 0).await;

    // Only an approach with firm_role "lead" may found a firm.
    let (s, err) = post(&c.app, "/api/firms", firm_body(plain)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["error"]["code"], "not_a_lead");
    assert_eq!(detail_paths(&err), ["lead_agent_id"]);

    // Field errors carry the form field.
    let mut bad = firm_body(lead);
    bad["name"] = json!(" ");
    bad["lead_share_bps"] = json!(4000);
    bad["roles"] = json!([{"role": "scout", "slots": 1, "weight_bps": 5000}]);
    let (s, err) = post(&c.app, "/api/firms", bad).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["error"]["code"], "validation_failed");
    assert_eq!(detail_paths(&err), ["name", "lead_share_bps", "roles"]);

    let (s, firm) = post(&c.app, "/api/firms", firm_body(lead)).await;
    assert_eq!(s, StatusCode::CREATED, "{firm}");
    assert_eq!(firm["env"], "simulated");
    assert_eq!(firm["lead_share_bps"], 8000);
    assert_eq!(firm["member_pool_bps"], 2000);
    assert_eq!(firm["task_fee"], "100000");
    assert_eq!(firm["state"], "open");
    assert_eq!(firm["actions"], json!(["join", "close"]));
    assert_eq!(
        firm["roles"][0],
        json!({"role": "scout", "slots": 2, "weight_bps": 6000, "filled": 0})
    );
    assert_eq!(firm["markets"], Value::Null);
    let id = firm["id"].as_str().unwrap().to_owned();

    let (s, err) = post(&c.app, "/api/firms", firm_body(lead)).await;
    assert_eq!(
        (s, err["error"]["code"].clone()),
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            json!("already_leads_firm")
        )
    );

    let join = |agent: Uuid, role: &str| json!({"agent_id": agent, "role": role});
    let path = format!("/api/firms/{id}/join");
    ok(&c.app, &path, join(m1, "analyst")).await;
    ok(&c.app, &path, join(m2, "analyst")).await;
    // Analyst slots are full.
    let (s, err) = post(&c.app, &path, join(m3, "analyst")).await;
    assert_eq!(
        (s, err["error"]["code"].clone()),
        (StatusCode::CONFLICT, json!("no_free_slot"))
    );
    assert_eq!(detail_paths(&err), ["role"]);
    let (s, err) = post(&c.app, &path, join(m1, "scout")).await;
    assert_eq!(
        (s, err["error"]["code"].clone()),
        (StatusCode::CONFLICT, json!("already_member"))
    );
    assert_eq!(detail_paths(&err), ["agent_id"]);
    let (s, err) = post(&c.app, &path, join(lead, "scout")).await;
    assert_eq!(
        (s, err["error"]["code"].clone()),
        (StatusCode::UNPROCESSABLE_ENTITY, json!("is_lead"))
    );
    // The owner may add an agent whatever its approach.
    let firm = ok(&c.app, &path, join(plain, "scout")).await;
    assert_eq!(firm["members"].as_array().unwrap().len(), 3);
    assert_eq!(firm["actions"], json!(["join", "leave", "close"]));

    let firm = ok(
        &c.app,
        &format!("/api/firms/{id}/leave"),
        json!({"agent_id": m2}),
    )
    .await;
    let left = firm["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["agent_id"] == m2.to_string())
        .unwrap()
        .clone();
    assert!(left["left_at"].is_string());
    let (s, _) = post(
        &c.app,
        &format!("/api/firms/{id}/leave"),
        json!({"agent_id": m2}),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    let firm = ok(&c.app, &format!("/api/firms/{id}/close"), json!({})).await;
    assert_eq!(firm["state"], "closed");
    assert_eq!(firm["actions"], json!([]));
    let (s, _) = post(&c.app, &path, join(m3, "scout")).await;
    assert_eq!(s, StatusCode::CONFLICT);
    // After closing, the lead may found another firm and members are free.
    let (s, _) = post(&c.app, "/api/firms", firm_body(lead)).await;
    assert_eq!(s, StatusCode::CREATED);

    let list = get(&c.app, "/api/firms").await;
    assert_eq!(list.as_array().unwrap().len(), 2);
    let k = kinds(&c.pool).await;
    for kind in ["firm_created", "firm_joined", "firm_left", "firm_closed"] {
        assert!(k.contains(&kind.to_owned()), "{kind}");
    }
    let acts = get(&c.app, &format!("/api/activity?firm_id={id}")).await;
    assert!(
        acts.as_array()
            .unwrap()
            .iter()
            .all(|a| a["firm_id"] == id && a["detail"]["firm_id"] == id)
    );
}

#[tokio::test]
async fn members_join_on_their_own_preferring_the_bigger_pool() {
    let c = setup().await;
    let a = agent(
        &c,
        "Momentum bull",
        "Lead A",
        Some(lead_ap()),
        json!({}),
        10,
    )
    .await;
    let b = agent(
        &c,
        "Momentum bull",
        "Lead B",
        Some(lead_ap()),
        json!({}),
        10,
    )
    .await;
    let energy = approach(
        8000,
        5000,
        "member",
        json!({"scope": "specific", "categories": ["Commodities"]}),
    );
    let off = agent(
        &c,
        "Momentum bull",
        "Energy only",
        Some(energy),
        json!({}),
        0,
    )
    .await;
    let crypto = approach(
        8000,
        5000,
        "member",
        json!({"scope": "specific", "categories": ["crypto"]}),
    );
    let m = agent(&c, "Momentum bull", "Joiner", Some(crypto), json!({}), 0).await;
    // Too competitive to join.
    let lone = agent(
        &c,
        "Momentum bull",
        "Lone",
        Some(approach(2000, 5000, "member", json!({}))),
        json!({}),
        0,
    )
    .await;

    let crypto_firm = json!({"scope": "specific", "categories": ["Crypto"]});
    let mut fa = firm_body(a);
    fa["markets"] = crypto_firm.clone();
    let fa = ok(&c.app, "/api/firms", fa).await;
    let mut fb = firm_body(b);
    fb["lead_share_bps"] = json!(7000);
    fb["markets"] = crypto_firm;
    let fb = ok(&c.app, "/api/firms", fb).await;

    let report = tick(&c, c.t0).await;
    assert_eq!(report.firm_joined, 1, "{report:?}");
    let fb = get(
        &c.app,
        &format!("/api/firms/{}", fb["id"].as_str().unwrap()),
    )
    .await;
    let fa = get(
        &c.app,
        &format!("/api/firms/{}", fa["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(fb["members"][0]["agent_id"], m.to_string());
    assert_eq!(fb["members"][0]["role"], "scout");
    assert!(fa["members"].as_array().unwrap().is_empty());
    for id in [off, lone] {
        let k: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM firm_members WHERE agent_id = $1")
            .bind(id)
            .fetch_one(&c.pool)
            .await
            .unwrap();
        assert_eq!(k, 0);
    }
    // Members are not added twice.
    assert_eq!(tick(&c, c.t0 + Duration::seconds(1)).await.firm_joined, 0);
}

#[tokio::test]
async fn approach_markets_gate_discovery_and_incoming_challenges() {
    let c = setup().await;
    let energy = json!({"scope": "specific", "categories": ["Commodities"]});
    let picky = agent(
        &c,
        "Momentum bull",
        "Picky",
        Some(approach(3000, 10_000, "none", energy.clone())),
        json!({}),
        100,
    )
    .await;
    let _skeptic = agent(&c, "Skeptic", "Skeptic", None, json!({}), 100).await;
    let report = tick(&c, c.t0).await;
    assert_eq!(report.proposed, 0, "{report:?}");
    let k = kinds(&c.pool).await;
    assert_eq!(k.iter().filter(|k| *k == "approach_filtered").count(), 1);
    // Rate-limited: the same note is not repeated.
    tick(&c, c.t0 + Duration::seconds(1)).await;
    assert_eq!(
        kinds(&c.pool)
            .await
            .iter()
            .filter(|k| *k == "approach_filtered")
            .count(),
        1
    );
    let _ = picky;

    // An incoming challenge outside the responder's approach is rejected.
    common::wipe(&c.pool).await;
    let _bull = agent(&c, "Momentum bull", "Bull", None, json!({}), 100).await;
    let mut skeptic_graph_ap = approach(3000, 5000, "none", energy);
    skeptic_graph_ap["id"] = json!("ap");
    let _choosy = agent(
        &c,
        "Skeptic",
        "Choosy",
        Some(skeptic_graph_ap),
        json!({}),
        100,
    )
    .await;
    let report = tick(&c, c.t0).await;
    assert_eq!((report.proposed, report.rejected), (1, 1), "{report:?}");
    let reason: Option<String> = sqlx::query_scalar(
        "SELECT detail->>'reason' FROM activity WHERE kind = 'challenge_rejected'",
    )
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(reason.as_deref(), Some("outside_approach"));
}

#[tokio::test]
async fn competitive_or_capped_agents_do_not_initiate() {
    let c = setup().await;
    let _collab = agent(
        &c,
        "Momentum bull",
        "Collaborator",
        Some(approach(7001, 10_000, "none", json!({}))),
        json!({}),
        100,
    )
    .await;
    let _skeptic = agent(&c, "Skeptic", "Skeptic", None, json!({}), 100).await;
    assert_eq!(tick(&c, c.t0).await.proposed, 0);
}

#[tokio::test]
async fn ambition_caps_the_stake() {
    let c = setup_with(|mut s| {
        s.reasoner = Arc::new(WithDoubters);
        s
    })
    .await;
    // Cautious: stake target 20 % of 10 USDC = 2 USDC (the bull would stake 5).
    let cautious = approach(3000, 0, "none", json!({}));
    let _a = agent(
        &c,
        "Momentum bull",
        "Sure bull",
        Some(cautious),
        json!({}),
        100,
    )
    .await;
    let _s = agent(&c, "Skeptic", "Skeptic", None, json!({}), 100).await;
    let report = tick(&c, c.t0).await;
    assert_eq!(report.proposed, 1, "{report:?}");
    // The proposal (round 1), before any counter-offer.
    let proposed: String = sqlx::query_scalar(
        "SELECT terms->>'stake' FROM negotiation_rounds WHERE seq = 1 AND action = 'propose'",
    )
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(proposed, (2 * USDC).to_string());
}

/// Wraps the rule reasoner; agents whose name starts with "Doubter" disagree
/// strongly with whatever the market did, "Sure..." agents are very sure.
struct WithDoubters;

impl Reasoner for WithDoubters {
    fn name(&self) -> &'static str {
        "rules"
    }
    fn reason<'a>(
        &'a self,
        req: &'a ReasonRequest<'a>,
    ) -> BoxFut<'a, Result<ReasonerOutput, ReasonerError>> {
        Box::pin(async move {
            let mut out = RuleBasedReasoner::decide(req)?;
            if req.agent.name.starts_with("Doubter") && req.task == Task::EvaluateOpportunity {
                out.stance = match out.stance {
                    Direction::Above => Direction::Below,
                    Direction::Below => Direction::Above,
                };
                out.conviction_bps = 9500;
            }
            if req.agent.name.starts_with("Sure") {
                out.conviction_bps = 9900;
            }
            Ok(out)
        })
    }
}

#[tokio::test]
async fn analysts_can_stop_a_draft() {
    let c = setup_with(|mut s| {
        s.reasoner = Arc::new(WithDoubters);
        s
    })
    .await;
    let lead = agent(&c, "Momentum bull", "Lead", Some(lead_ap()), json!({}), 100).await;
    let doubter = agent(
        &c,
        "Momentum bull",
        "Doubter",
        Some(member_ap()),
        json!({}),
        0,
    )
    .await;
    let _skeptic = agent(&c, "Skeptic", "Skeptic", None, json!({}), 100).await;
    let firm = ok(&c.app, "/api/firms", firm_body(lead)).await;
    let id = firm["id"].as_str().unwrap();
    ok(
        &c.app,
        &format!("/api/firms/{id}/join"),
        json!({"agent_id": doubter, "role": "analyst"}),
    )
    .await;

    let report = tick(&c, c.t0).await;
    assert_eq!(report.proposed, 0, "{report:?}");
    assert_eq!(report.firm_contributions, 1);
    assert!(
        kinds(&c.pool)
            .await
            .contains(&"firm_draft_dropped".to_owned())
    );
    let contribs = get(&c.app, &format!("/api/firms/{id}/contributions")).await;
    assert_eq!(contribs[0]["kind"], "analyst_signal");
    assert_eq!(contribs[0]["used"], false);
    assert_eq!(contribs[0]["conviction_bps"], 500);
    assert_eq!(available(&c.pool, doubter).await, 0);
}

#[tokio::test]
async fn firm_challenge_pays_fees_and_shares_the_profit() {
    let c = setup().await;
    let scout = agent(
        &c,
        "Momentum bull",
        "Scout",
        Some(member_ap()),
        json!({}),
        0,
    )
    .await;
    let analyst = agent(
        &c,
        "Momentum bull",
        "Analyst",
        Some(member_ap()),
        json!({}),
        0,
    )
    .await;
    let skeptic = agent(&c, "Skeptic", "Skeptic", None, json!({}), 100).await;
    // Created last, so its scout has looked before it discovers.
    let lead = agent(
        &c,
        "Momentum bull",
        "Lead",
        Some(lead_ap()),
        json!({"approval_threshold": "10000000"}),
        100,
    )
    .await;
    let firm = ok(&c.app, "/api/firms", firm_body(lead)).await;
    let fid = firm["id"].as_str().unwrap().to_owned();
    ok(
        &c.app,
        &format!("/api/firms/{fid}/join"),
        json!({"agent_id": scout, "role": "scout"}),
    )
    .await;
    ok(
        &c.app,
        &format!("/api/firms/{fid}/join"),
        json!({"agent_id": analyst, "role": "analyst"}),
    )
    .await;

    let report = tick(&c, c.t0).await;
    assert_eq!(report.proposed, 1, "{report:?}");
    assert_eq!(report.firm_contributions, 2, "{report:?}");
    let list = get(&c.app, &format!("/api/interactions?agent_id={lead}")).await;
    let i = list
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["proposer_id"] == lead.to_string())
        .unwrap()
        .clone();
    assert_eq!(i["firm_id"], fid);
    let iid: Uuid = i["id"].as_str().unwrap().parse().unwrap();

    // Both contributions fed the challenge and earned the 0.1 USDC fee.
    let contribs = get(&c.app, &format!("/api/firms/{fid}/contributions")).await;
    assert_eq!(contribs.as_array().unwrap().len(), 2);
    for x in contribs.as_array().unwrap() {
        assert_eq!(x["used"], true, "{x}");
        assert_eq!(x["interaction_id"], iid.to_string());
        assert_eq!(x["fee_paid"], "100000");
    }
    assert_eq!(available(&c.pool, scout).await, 100_000);
    assert_eq!(available(&c.pool, analyst).await, 100_000);

    // Make sure the challenge is live, whatever the skeptic answered.
    let state: String = sqlx::query_scalar("SELECT state FROM interactions WHERE id = $1")
        .bind(iid)
        .fetch_one(&c.pool)
        .await
        .unwrap();
    if matches!(state.as_str(), "proposed" | "negotiating") {
        sqlx::query("UPDATE interactions SET state = 'accepted', turn = NULL WHERE id = $1")
            .bind(iid)
            .execute(&c.pool)
            .await
            .unwrap();
    }
    let state: String = sqlx::query_scalar("SELECT state FROM interactions WHERE id = $1")
        .bind(iid)
        .fetch_one(&c.pool)
        .await
        .unwrap();
    if state == "accepted" {
        let mut tx = c.pool.begin().await.unwrap();
        let out = lifecycle::try_commit(&mut tx, iid, c.t0).await.unwrap();
        tx.commit().await.unwrap();
        if out != CommitOutcome::Funded {
            // The bull asks its owner above its automatic stake: approve.
            assert!(
                matches!(out, CommitOutcome::AwaitingApproval { .. }),
                "{out:?}"
            );
            for a in get(&c.app, "/api/approvals").await.as_array().unwrap() {
                let path = format!("/api/approvals/{}/approve", a["id"].as_str().unwrap());
                ok(&c.app, &path, json!({})).await;
            }
        }
    }
    let state: String = sqlx::query_scalar("SELECT state FROM interactions WHERE id = $1")
        .bind(iid)
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert!(matches!(state.as_str(), "funded" | "active"), "{state}");
    let (stake, end, direction): (i64, DateTime<Utc>, String) =
        sqlx::query_as("SELECT stake, end_ts, terms->>'direction' FROM interactions WHERE id = $1")
            .bind(iid)
            .fetch_one(&c.pool)
            .await
            .unwrap();

    // The lead's side wins.
    c.fake
        .set_yes_price(Some(if direction == "above" { 9900 } else { 100 }));
    // Closing stops new work; payouts on open firm challenges still settle.
    ok(&c.app, &format!("/api/firms/{fid}/close"), json!({})).await;
    tick(&c, end + Duration::seconds(1)).await;
    let state: String = sqlx::query_scalar("SELECT state FROM interactions WHERE id = $1")
        .bind(iid)
        .fetch_one(&c.pool)
        .await
        .unwrap();
    assert_eq!(state, "settled");

    // pool = 20 % of the profit; scouts weigh 60 %, analysts 40 %.
    let pool = stake * 2000 / 10_000;
    let (to_scout, to_analyst) = (pool * 6000 / 10_000, pool * 4000 / 10_000);
    assert_eq!(available(&c.pool, scout).await, 100_000 + to_scout);
    assert_eq!(available(&c.pool, analyst).await, 100_000 + to_analyst);
    // On this challenge the lead got its stake back plus the profit, less
    // the shares.
    let lead_net: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries WHERE interaction_id = $1 \
         AND account = $2 AND reason IN ('settle', 'firm_payout')",
    )
    .bind(iid)
    .bind(format!("agent:{lead}:available"))
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(lead_net, 2 * stake - to_scout - to_analyst);
    assert_ledger_sound(&c.pool).await;

    let firm = get(&c.app, &format!("/api/firms/{fid}")).await;
    assert_eq!(firm["stats"]["challenges"], 1);
    assert_eq!(firm["stats"]["settled"], 1);
    let paid = 200_000 + to_scout + to_analyst;
    assert_eq!(firm["stats"]["paid_to_members"], paid.to_string());
    assert_eq!(
        firm["stats"]["lead_realized_pnl"],
        (stake - paid).to_string()
    );
    let scout_row = firm["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["agent_id"] == scout.to_string())
        .unwrap()
        .clone();
    assert_eq!(scout_row["earned"], (100_000 + to_scout).to_string());
    assert_eq!(scout_row["used"], 1);

    // P&L: the lead's is net of what it paid; members show firm income only.
    let agents = get(&c.app, "/api/agents").await;
    let find = |id: Uuid| {
        agents
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id.to_string())
            .unwrap()
            .clone()
    };
    assert_eq!(
        find(lead)["stats"]["realized_pnl"],
        (stake - paid).to_string()
    );
    assert_eq!(find(scout)["stats"]["realized_pnl"], "0");
    assert_eq!(
        find(scout)["stats"]["firm_income"],
        (100_000 + to_scout).to_string()
    );
    assert_eq!(find(skeptic)["stats"]["realized_pnl"], (-stake).to_string());

    let k = kinds(&c.pool).await;
    assert_eq!(k.iter().filter(|k| *k == "firm_task_fee").count(), 2);
    assert_eq!(k.iter().filter(|k| *k == "firm_payout").count(), 3);
    let payouts: Vec<Value> = sqlx::query_scalar(
        "SELECT detail FROM activity WHERE kind IN ('firm_payout', 'firm_task_fee')",
    )
    .fetch_all(&c.pool)
    .await
    .unwrap();
    assert!(
        payouts
            .iter()
            .all(|d| d["firm_id"] == fid && d["amount"].is_string())
    );

    // Settling again pays nothing twice.
    tick(&c, end + Duration::seconds(60)).await;
    assert_eq!(available(&c.pool, scout).await, 100_000 + to_scout);
}

#[tokio::test]
async fn a_losing_firm_challenge_pays_no_share_and_a_poor_lead_pays_no_fee() {
    let c = setup().await;
    let scout = agent(
        &c,
        "Momentum bull",
        "Scout",
        Some(member_ap()),
        json!({}),
        0,
    )
    .await;
    let _skeptic = agent(&c, "Skeptic", "Skeptic", None, json!({}), 100).await;
    // The lead can stake but its fee (above the per-interaction max) is refused.
    let lead = agent(&c, "Momentum bull", "Lead", Some(lead_ap()), json!({}), 100).await;
    let mut body = firm_body(lead);
    body["task_fee"] = json!("20000000");
    let firm = ok(&c.app, "/api/firms", body).await;
    let fid = firm["id"].as_str().unwrap().to_owned();
    ok(
        &c.app,
        &format!("/api/firms/{fid}/join"),
        json!({"agent_id": scout, "role": "scout"}),
    )
    .await;
    let report = tick(&c, c.t0).await;
    assert_eq!(report.proposed, 1, "{report:?}");
    assert!(kinds(&c.pool).await.contains(&"firm_fee_denied".to_owned()));
    let contribs = get(&c.app, &format!("/api/firms/{fid}/contributions")).await;
    assert_eq!(contribs[0]["used"], true);
    assert_eq!(contribs[0]["fee_paid"], "0");
    assert_eq!(available(&c.pool, scout).await, 0);
    assert_ledger_sound(&c.pool).await;
}

#[tokio::test]
async fn market_categories_and_preview() {
    let c = setup().await;
    let cats = get(&c.app, "/api/markets/categories").await;
    assert_eq!(cats["source"], "fake");
    assert_eq!(
        cats["categories"],
        json!(["Commodities", "Crypto", "Economics", "Sports"])
    );
    assert_eq!(
        cats["subcategories"]["Crypto"],
        json!(["Bitcoin", "Ethereum"])
    );
    assert_eq!(
        cats["subcategories"]["Commodities"],
        json!(["Energy", "Metals"])
    );
    assert_eq!(
        cats["structures"],
        json!(["binary_up_down", "price_ladder", "categorical"])
    );

    let app = &c.app;
    let preview = |markets: Value| post(app, "/api/markets/preview", json!({"markets": markets}));
    // The fake catalog: BTC 1 + ETH 3 + WTI 3 + natural gas 1 + gold 1 + Fed 3 + final 2.
    let (s, p) = preview(json!({"scope": "any"})).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    assert_eq!(p["count"], 14);
    assert_eq!(p["source"], "fake");
    assert!(p["sample"][0]["title"].as_str().unwrap().contains("BTC"));
    assert_eq!(p["sample"][0]["category"], "Crypto");
    let count = |v: &Value| v["count"].as_i64().unwrap();
    assert_eq!(
        count(
            &preview(json!({"scope": "specific", "categories": ["commodities"]}))
                .await
                .1
        ),
        5
    );
    // Category OR subcategory.
    assert_eq!(count(&preview(json!({"scope": "specific", "categories": ["commodities"], "subcategories": ["bitcoin"]})).await.1), 6);
    // An energy market with a price near even money.
    let (_, energy) = preview(json!({"scope": "specific", "subcategories": ["energy"], "keywords": ["oil"], "ranges": {"yes_price_bps": {"min": 4000, "max": 6000}}})).await;
    assert_eq!(count(&energy), 1, "{energy}");
    assert_eq!(
        energy["sample"][0]["market_id"],
        "gemini_pm:FAKE-WTI-FRI-80"
    );
    assert_eq!(energy["sample"][0]["subcategory"], "Energy");
    assert_eq!(energy["sample"][0]["structure"], "price_ladder");
    assert_eq!(
        count(
            &preview(json!({"scope": "specific", "structures": ["categorical"], "categories": ["economics"]}))
                .await
                .1
        ),
        3
    );
    assert_eq!(
        count(
            &preview(
                json!({"scope": "specific", "structures": ["binary_up_down"], "symbols": ["BTC"]})
            )
            .await
            .1
        ),
        1
    );
    assert_eq!(
        count(
            &preview(json!({"scope": "specific", "keywords": ["hourly"]}))
                .await
                .1
        ),
        1
    );
    // The fake moved 3 %; only BTC and ETH have a fake spot price, so other
    // markets have no move and fail a move bound.
    assert_eq!(
        count(
            &preview(
                json!({"scope": "specific", "ranges": {"move_bps": {"min": 250, "max": null}}})
            )
            .await
            .1
        ),
        4
    );
    assert_eq!(
        count(
            &preview(
                json!({"scope": "specific", "ranges": {"move_bps": {"min": 301, "max": null}}})
            )
            .await
            .1
        ),
        0
    );
    let (s, err) = preview(json!({"ranges": {"yes_price_bps": {"min": 9, "max": 1}}})).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(detail_paths(&err), ["markets.ranges.yes_price_bps"]);
}

/// The `backend/README.md` "Firm demo" script, entirely over the HTTP router.
#[tokio::test]
async fn seed_firm_demo_walks_the_whole_script() {
    const LEAD: &str = "00000000-0000-4000-8000-0000000000f4";
    const SCOUT: &str = "00000000-0000-4000-8000-0000000000f1";
    const ANALYST: &str = "00000000-0000-4000-8000-0000000000f2";
    const RIVAL: &str = "00000000-0000-4000-8000-0000000000f3";
    let c = setup().await;

    let seeded = ok(&c.app, "/api/dev/seed-firm-demo", json!({})).await;
    assert_eq!(seeded["env"], "simulated");
    assert_eq!(seeded["firm_created"], true);
    assert_eq!(seeded["funds_per_agent"], "100000000");
    assert_eq!(seeded["agents"].as_array().unwrap().len(), 4);
    assert!(
        seeded["agents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["created"] == true && a["name"].as_str().unwrap().ends_with("(simulated)"))
    );
    let fid = seeded["firm_id"].as_str().unwrap().to_owned();
    let firm = get(&c.app, &format!("/api/firms/{fid}")).await;
    assert_eq!(firm["lead_agent_id"], LEAD);
    assert_eq!(firm["lead_share_bps"], 8000);
    assert_eq!(firm["member_pool_bps"], 2000);
    assert_eq!(firm["task_fee"], "100000");
    assert_eq!(firm["members"].as_array().unwrap().len(), 2);

    // Idempotent: nothing changes the second time.
    let counts = |pool: PgPool| async move {
        let n = |sql: &'static str| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, i64>(sql)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
            }
        };
        (
            n("SELECT COUNT(*) FROM ledger_entries").await,
            n("SELECT COUNT(*) FROM agents").await,
            n("SELECT COUNT(*) FROM firms").await,
            n("SELECT COUNT(*) FROM firm_members").await,
            n("SELECT COUNT(*) FROM activity").await,
        )
    };
    let before = counts(c.pool.clone()).await;
    let again = ok(&c.app, "/api/dev/seed-firm-demo", json!({})).await;
    assert_eq!(again["firm_created"], false);
    assert!(
        again["agents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["created"] == false)
    );
    assert_eq!(counts(c.pool.clone()).await, before);

    ok(
        &c.app,
        "/api/dev/fake-market",
        json!({"move_bps": 300, "yes_price_bps": 5500, "resolution_side": "open"}),
    )
    .await;
    let t1 = ok(&c.app, "/api/dev/tick", json!({})).await;
    assert_eq!(t1["proposed"], 1, "{t1}");
    assert_eq!(t1["firm_contributions"], 2, "{t1}");
    assert_eq!(t1["accepted"], 1, "{t1}");
    assert_eq!(t1["approvals_requested"], 1, "{t1}");
    ok(&c.app, "/api/dev/tick", json!({})).await;
    ok(&c.app, "/api/dev/tick", json!({})).await;

    // The rival's owner approves its 5.00 USDC stake.
    let approvals = get(&c.app, "/api/approvals").await;
    let approvals = approvals.as_array().unwrap();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0]["agent_id"], RIVAL);
    assert_eq!(approvals[0]["stake"], "5000000");
    let path = format!(
        "/api/approvals/{}/approve",
        approvals[0]["id"].as_str().unwrap()
    );
    ok(&c.app, &path, json!({})).await;

    let list = get(&c.app, "/api/interactions").await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["state"], "active", "{}", list[0]);
    assert_eq!(list[0]["firm_id"], fid);
    let end: DateTime<Utc> = list[0]["end_at"].as_str().unwrap().parse().unwrap();

    ok(
        &c.app,
        "/api/dev/fake-market",
        json!({"yes_price_bps": 7000}),
    )
    .await;
    let now = (end + Duration::seconds(60)).to_rfc3339();
    let t4 = ok(&c.app, "/api/dev/tick", json!({ "now": now })).await;
    assert_eq!(t4["settled"], 1, "{t4}");
    let list = get(&c.app, "/api/interactions").await;
    assert_eq!(list[0]["state"], "settled");
    assert_eq!(list[0]["winner_id"], LEAD);

    // Profit 5.00: pool 20 % = 1.00 (scout 0.60, analyst 0.40), plus a 0.10 task
    // fee each; the lead keeps 5.00 - 1.00 - 0.20 = 3.80.
    let firm = get(&c.app, &format!("/api/firms/{fid}")).await;
    assert_eq!(firm["stats"]["lead_realized_pnl"], "3800000");
    assert_eq!(firm["stats"]["paid_to_members"], "1200000");
    let earned = |id: &str| {
        firm["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["agent_id"] == id)
            .unwrap()["earned"]
            .clone()
    };
    assert_eq!(earned(SCOUT), "700000");
    assert_eq!(earned(ANALYST), "500000");
    let stats = |id: &str| {
        let id = id.to_owned();
        let app = c.app.clone();
        async move { get(&app, &format!("/api/agents/{id}")).await }
    };
    let lead = stats(LEAD).await;
    assert_eq!(lead["stats"]["realized_pnl"], "3800000");
    assert_eq!(lead["balances"]["available"], "103800000");
    assert_eq!(stats(SCOUT).await["stats"]["firm_income"], "700000");
    assert_eq!(stats(SCOUT).await["balances"]["available"], "100700000");
    assert_eq!(stats(ANALYST).await["stats"]["firm_income"], "500000");
    assert_eq!(stats(ANALYST).await["balances"]["available"], "100500000");
    let rival = stats(RIVAL).await;
    assert_eq!(rival["stats"]["realized_pnl"], "-5000000");
    assert_eq!(rival["balances"]["available"], "95000000");
    assert_ledger_sound(&c.pool).await;
}
