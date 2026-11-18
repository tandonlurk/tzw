//! Arc testnet wiring against a local anvil chain with the real compiled
//! contracts (`forge build` in contracts/) and a real Postgres
//! (`TEST_DATABASE_URL`, default `aen_test`). Skips when anvil is missing.
//!
//! anvil starts ten minutes in the past so its block times (which run ahead
//! of the wall clock by a second per block when blocks come fast) stay behind
//! the wall-clock funding deadlines; tests move it forward explicitly. The
//! backend reaches anvil through a local proxy that tests can switch off to
//! simulate an RPC outage.

mod common;

use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use aen_chain::testkit::{ANVIL_MNEMONIC, deploy, mint};
use aen_chain::{
    Chain, ChainConfig, EscrowStatus, EvmAddress, OwnerWallet, TEST_OWNER_INDEX, challenge_id,
    derive_signer,
};
use aen_server::chain::ChainState;
use aen_server::lifecycle::{self, TickReport};
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
use uuid::Uuid;

static DB_LOCK: Mutex<()> = Mutex::const_new(());

const USDC: u64 = 1_000_000;
/// Seconds from proposal to funding deadline, and from there to `end_ts`.
const FUNDING_WINDOW: i64 = 8;
const HORIZON: i64 = 3;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct AnvilProc(Child);

impl Drop for AnvilProc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn anvil_bin() -> Option<String> {
    let home = std::env::var("HOME").unwrap_or_default();
    ["anvil".to_owned(), format!("{home}/.foundry/bin/anvil")]
        .into_iter()
        .find(|b| {
            Command::new(b)
                .arg("--version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok()
        })
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn rpc(url: &str, method: &str, params: Value) -> Value {
    let res: Value = reqwest::Client::new()
        .post(url)
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(res.get("error").is_none(), "{method}: {res}");
    res["result"].clone()
}

async fn spawn_anvil() -> Option<(AnvilProc, String)> {
    let bin = anvil_bin()?;
    let port = free_port();
    let genesis = Utc::now().timestamp() - 600;
    let child = Command::new(bin)
        .args([
            "--port",
            &port.to_string(),
            "--silent",
            "--chain-id",
            "31337",
        ])
        .args(["--timestamp", &genesis.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let proc = AnvilProc(child);
    let url = format!("http://127.0.0.1:{port}");
    for _ in 0..100 {
        if reqwest::Client::new()
            .post(&url)
            .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": [] }))
            .send()
            .await
            .is_ok()
        {
            return Some((proc, url));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

/// A TCP proxy to `upstream` that drops every connection while `up` is false.
async fn spawn_proxy(upstream: String, up: Arc<AtomicBool>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, _) = tokio::sync::broadcast::channel::<()>(16);
    let kill = tx.clone();
    let flag = up.clone();
    // Closes open (pooled) connections as soon as the proxy goes down.
    tokio::spawn(async move {
        let mut was_up = true;
        loop {
            let now_up = flag.load(Ordering::SeqCst);
            if was_up && !now_up {
                let _ = kill.send(());
            }
            was_up = now_up;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    tokio::spawn(async move {
        loop {
            let Ok((mut inbound, _)) = listener.accept().await else {
                return;
            };
            if !up.load(Ordering::SeqCst) {
                drop(inbound);
                continue;
            }
            let upstream = upstream.clone();
            let mut stop = tx.subscribe();
            tokio::spawn(async move {
                let Ok(mut outbound) = tokio::net::TcpStream::connect(upstream).await else {
                    return;
                };
                tokio::select! {
                    _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound) => {}
                    _ = stop.recv() => {}
                }
            });
        }
    });
    format!("http://{addr}")
}

struct Ctx {
    app: Router,
    state: AppState,
    pool: PgPool,
    chain: Chain,
    owner: OwnerWallet,
    /// Direct to anvil (owner calls, mint, time travel).
    cfg: ChainConfig,
    anvil_url: String,
    rpc_up: Arc<AtomicBool>,
    _anvil: AnvilProc,
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

async fn setup() -> Option<Ctx> {
    let guard = DB_LOCK.lock().await;
    let Some((anvil, anvil_url)) = spawn_anvil().await else {
        eprintln!("SKIP: anvil not installed");
        return None;
    };
    let deployed = deploy(&anvil_url, ANVIL_MNEMONIC).await.expect("deploy");
    let cfg = deployed.cfg;
    let rpc_up = Arc::new(AtomicBool::new(true));
    let proxy = spawn_proxy(
        anvil_url.trim_start_matches("http://").to_owned(),
        rpc_up.clone(),
    )
    .await;
    let backend_cfg = ChainConfig {
        rpc_url: proxy,
        explorer_url: Some("https://explorer.example".into()),
        ..cfg.clone()
    };
    let chain = Chain::connect(backend_cfg, ANVIL_MNEMONIC)
        .await
        .expect("connect");
    let owner = OwnerWallet::connect(
        cfg.clone(),
        derive_signer(ANVIL_MNEMONIC, TEST_OWNER_INDEX).unwrap(),
    )
    .await
    .unwrap();
    mint(&cfg, ANVIL_MNEMONIC, owner.address(), 1_000 * USDC)
        .await
        .unwrap();

    let pool = pool().await;
    let mut state = AppState::offline(pool.clone());
    state.chain = Some(Arc::new(ChainState::new(
        chain.clone(),
        "https://rpc.public.example".into(),
        None,
    )));
    state.settings.funding_window_secs = FUNDING_WINDOW;
    Some(Ctx {
        app: app_with_state(state.clone(), DEFAULT_CORS_ORIGIN.parse().unwrap()),
        state,
        pool,
        chain,
        owner,
        cfg,
        anvil_url,
        rpc_up,
        _anvil: anvil,
        _guard: guard,
    })
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

/// An agent from a seeded template with a short horizon (no funds).
async fn agent(c: &Ctx, template: &str) -> Uuid {
    let templates = get(&c.app, "/api/templates").await;
    let t = templates
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == template)
        .unwrap()
        .clone();
    let mut config = t["config"].clone();
    config["horizon_secs"] = json!(HORIZON);
    let input = json!({
        "name": template, "description": t["description"], "config": config,
        "policy": t["policy"], "graph": t["graph"],
    });
    post(&c.app, "/api/agents", input).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Creates a vault for `agent` as the test owner, links it through the API
/// and funds it with `usdc` through a synced transfer. Returns the vault.
async fn vault_agent(c: &Ctx, agent: Uuid, usdc: u64) -> EvmAddress {
    let (vault, tx) = c
        .owner
        .create_vault(agent, c.chain.operator(), 10 * USDC, 15 * USDC)
        .await
        .unwrap();
    let view = post(
        &c.app,
        &format!("/api/agents/{agent}/vault"),
        json!({ "tx_hash": format!("{tx:#x}") }),
    )
    .await;
    assert_eq!(view["address"], vault.to_string());
    let a = get(&c.app, &format!("/api/agents/{agent}")).await;
    assert_eq!(a["env"], "arc_testnet");
    assert_eq!(a["vault_address"], format!("{vault:#x}"));
    let fund = c.owner.transfer_usdc(vault, usdc * USDC).await.unwrap();
    let synced = post(
        &c.app,
        &format!("/api/agents/{agent}/vault/sync"),
        json!({ "tx_hash": format!("{fund:#x}") }),
    )
    .await;
    assert_eq!(synced["credited"].as_array().unwrap().len(), 1, "{synced}");
    assert_eq!(synced["vault"]["drift"], "0");
    vault
}

async fn tick(c: &Ctx) -> TickReport {
    lifecycle::tick(&c.state, Utc::now()).await.unwrap()
}

async fn only_interaction(c: &Ctx) -> Value {
    let list = get(&c.app, "/api/interactions").await;
    assert_eq!(list.as_array().unwrap().len(), 1, "{list}");
    list[0].clone()
}

async fn rows(pool: &PgPool, interaction: Uuid, reason: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM ledger_entries WHERE interaction_id = $1 AND reason = $2",
    )
    .bind(interaction)
    .bind(reason)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The vault view of `agent` says the ledger equals the chain.
async fn assert_no_drift(c: &Ctx, agent: Uuid) -> Value {
    let v = get(&c.app, &format!("/api/agents/{agent}/vault")).await;
    assert_eq!(v["drift"], "0", "{v}");
    assert_eq!(v["onchain_exposure"], "0", "{v}");
    assert_eq!(v["ledger_committed"], "0", "{v}");
    v
}

async fn increase_chain_time(c: &Ctx, secs: i64) {
    rpc(&c.anvil_url, "evm_increaseTime", json!([secs])).await;
    rpc(&c.anvil_url, "evm_mine", json!([])).await;
}

async fn sleep_until(t: chrono::DateTime<Utc>) {
    let wait = (t - Utc::now()).num_milliseconds();
    if wait > 0 {
        tokio::time::sleep(Duration::from_millis(wait as u64 + 200)).await;
    }
}

/// Bull and skeptic on vaults with 10 USDC each, then the first tick: the
/// bull proposes 5.00 and the skeptic counters 2.50.
async fn negotiated(c: &Ctx) -> (Uuid, Uuid, EvmAddress, EvmAddress, Uuid) {
    let (a, b) = (agent(c, "Momentum bull").await, agent(c, "Skeptic").await);
    let va = vault_agent(c, a, 10).await;
    let vb = vault_agent(c, b, 10).await;
    let r = tick(c).await;
    assert_eq!((r.proposed, r.countered), (1, 1), "{r:?}");
    let i = only_interaction(c).await;
    assert_eq!(i["env"], "arc_testnet");
    assert_eq!(i["stake"], "2500000");
    let id = i["id"].as_str().unwrap().parse().unwrap();
    (a, b, va, vb, id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn challenge_settles_onchain_and_the_ledger_matches_the_vaults() {
    let Some(c) = setup().await else { return };
    let (a, b, va, vb, id) = negotiated(&c).await;

    // Tick 2: accepted, reserved in the ledger, staked onchain, active.
    let r = tick(&c).await;
    assert_eq!((r.accepted, r.funded, r.activated), (1, 1, 1), "{r:?}");
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let i = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(i["state"], "active");
    for tx in ["create_tx", "proposer_stake_tx", "counterparty_stake_tx"] {
        assert!(i["escrow"][tx].as_str().is_some(), "{tx}: {i}");
    }
    let onchain = c.chain.get_challenge(challenge_id(id)).await.unwrap();
    assert_eq!(onchain.status, EscrowStatus::Active);
    assert_eq!((onchain.party_a, onchain.party_b), (va, vb));
    let v = get(&c.app, &format!("/api/agents/{a}/vault")).await;
    assert_eq!(v["onchain_balance"], "7500000");
    assert_eq!(v["onchain_exposure"], "2500000");
    assert_eq!(v["ledger_available"], "7500000");
    assert_eq!(v["ledger_committed"], "2500000");
    assert_eq!(v["drift"], "0");

    // Never before end_ts, whatever the dev clock says.
    let end: chrono::DateTime<Utc> = i["end_at"].as_str().unwrap().parse().unwrap();
    let r = lifecycle::tick(&c.state, end + chrono::Duration::hours(2))
        .await
        .unwrap();
    assert_eq!(r.resolving, 0, "{r:?}");

    post(
        &c.app,
        "/api/dev/fake-market",
        json!({ "yes_price_bps": 7000 }),
    )
    .await;
    sleep_until(end).await;

    // The RPC goes down mid-resolution: decided, nothing moved, retried.
    c.rpc_up.store(false, Ordering::SeqCst);
    let r = tick(&c).await;
    assert_eq!((r.resolving, r.settled), (1, 0), "{r:?}");
    assert!(
        r.errors.iter().any(|e| e.starts_with("chain_unavailable")),
        "{:?}",
        r.errors
    );
    let i = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(i["state"], "resolving");
    assert_eq!(rows(&c.pool, id, "settle").await, 0);

    c.rpc_up.store(true, Ordering::SeqCst);
    let r = tick(&c).await;
    assert_eq!(r.settled, 1, "{r:?}");
    assert!(r.errors.is_empty(), "{:?}", r.errors);
    let i = get(&c.app, &format!("/api/interactions/{id}")).await;
    assert_eq!(i["state"], "settled");
    assert_eq!(i["outcome"], "a");
    assert!(i["escrow"]["settle_tx"].as_str().is_some());
    let onchain = c.chain.get_challenge(challenge_id(id)).await.unwrap();
    assert_eq!(
        (onchain.status, onchain.winner),
        (EscrowStatus::Resolved, va)
    );

    // Nothing is posted twice, however often it ticks.
    tick(&c).await;
    assert_eq!(rows(&c.pool, id, "commit_stake").await, 4);
    assert_eq!(rows(&c.pool, id, "settle").await, 6);
    let settle_tx: Option<String> = sqlx::query_scalar(
        "SELECT DISTINCT tx_hash FROM ledger_entries WHERE interaction_id = $1 AND reason = 'settle'",
    )
    .bind(id)
    .fetch_one(&c.pool)
    .await
    .unwrap();
    assert_eq!(settle_tx.as_deref(), i["escrow"]["settle_tx"].as_str());

    // Ledger equals the vaults: winner 12.50, loser 7.50, exposure released.
    let va_view = assert_no_drift(&c, a).await;
    assert_eq!(va_view["onchain_balance"], "12500000");
    assert_eq!(va_view["ledger_available"], "12500000");
    let vb_view = assert_no_drift(&c, b).await;
    assert_eq!(vb_view["onchain_balance"], "7500000");
    let agents = get(&c.app, "/api/agents").await;
    let bull = agents
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] == a.to_string())
        .unwrap();
    assert_eq!(bull["stats"]["realized_pnl"], "2500000");
    assert_eq!(bull["balances"]["available"], "12500000");
    assert_eq!(c.chain.vault_info(vb).await.unwrap().exposure, 0);

    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM activity WHERE interaction_id = $1 ORDER BY seq")
            .bind(id)
            .fetch_all(&c.pool)
            .await
            .unwrap();
    for kind in [
        "stake_reserved",
        "escrow_created",
        "stake_deposited",
        "stake_committed",
        "challenge_settled",
    ] {
        assert!(kinds.iter().any(|k| k == kind), "{kind} missing: {kinds:?}");
    }
}

#[tokio::test]
async fn funding_deadline_after_an_rpc_outage_voids_and_refunds() {
    let Some(c) = setup().await else { return };
    let (a, b, _va, _vb, id) = negotiated(&c).await;

    // Accepted and reserved, but the chain is unreachable: stays funded.
    c.rpc_up.store(false, Ordering::SeqCst);
    let r = tick(&c).await;
    assert_eq!((r.accepted, r.funded, r.activated), (1, 1, 0), "{r:?}");
    assert!(
        r.errors.iter().any(|e| e.starts_with("chain_unavailable")),
        "{:?}",
        r.errors
    );
    assert_eq!(only_interaction(&c).await["state"], "funded");
    assert_eq!(rows(&c.pool, id, "commit_stake").await, 4);

    // The funding deadline passes (chain clock) before the RPC comes back.
    increase_chain_time(&c, 900).await;
    c.rpc_up.store(true, Ordering::SeqCst);
    let r = tick(&c).await;
    assert_eq!(r.voided, 1, "{r:?}");
    let i = only_interaction(&c).await;
    assert_eq!(i["state"], "cancelled");
    assert_eq!(i["outcome"], "void");
    assert!(i["escrow"]["create_tx"].is_null());
    assert_eq!(
        c.chain
            .get_challenge(challenge_id(id))
            .await
            .unwrap()
            .status,
        EscrowStatus::None
    );
    tick(&c).await;
    assert_eq!(rows(&c.pool, id, "refund").await, 4);
    for agent in [a, b] {
        let v = assert_no_drift(&c, agent).await;
        assert_eq!(v["ledger_available"], "10000000");
    }
}

#[tokio::test]
async fn paused_vault_cannot_stake_so_the_challenge_is_voided_onchain() {
    let Some(c) = setup().await else { return };
    let (a, b, va, vb, id) = negotiated(&c).await;
    c.owner.vault_set_paused(vb, true).await.unwrap();

    let r = tick(&c).await;
    assert_eq!((r.accepted, r.funded, r.voided), (1, 1, 1), "{r:?}");
    let i = only_interaction(&c).await;
    assert_eq!(i["state"], "cancelled");
    assert!(
        i["status_reason"].as_str().unwrap().contains("paused"),
        "{i}"
    );
    assert!(i["escrow"]["proposer_stake_tx"].as_str().is_some());
    assert!(i["escrow"]["counterparty_stake_tx"].is_null());
    assert!(i["escrow"]["settle_tx"].as_str().is_some());
    assert_eq!(
        c.chain
            .get_challenge(challenge_id(id))
            .await
            .unwrap()
            .status,
        EscrowStatus::Voided
    );
    // The bull's stake came back to its vault and its exposure was released.
    assert_eq!(c.chain.vault_info(va).await.unwrap().balance, 10 * USDC);
    for agent in [a, b] {
        let v = assert_no_drift(&c, agent).await;
        assert_eq!(v["ledger_available"], "10000000");
    }
    tick(&c).await;
    assert_eq!(rows(&c.pool, id, "refund").await, 4);
}

#[tokio::test]
async fn sync_is_idempotent_and_never_invents_entries() {
    let Some(c) = setup().await else { return };
    let a = agent(&c, "Momentum bull").await;
    let vault = vault_agent(&c, a, 10).await;

    // A deposit synced twice is credited once.
    let tx = c.owner.transfer_usdc(vault, 4 * USDC).await.unwrap();
    let body = json!({ "tx_hash": format!("{tx:#x}") });
    let path = format!("/api/agents/{a}/vault/sync");
    let first = post(&c.app, &path, body.clone()).await;
    assert_eq!(first["credited"][0]["delta"], "4000000");
    assert_eq!(first["credited"][0]["reason"], "vault_deposit");
    assert_eq!(first["credited"][0]["tx_hash"], format!("{tx:#x}"));
    let again = post(&c.app, &path, body).await;
    assert_eq!(again["credited"], json!([]));
    assert_eq!(again["vault"]["ledger_available"], "14000000");
    assert_eq!(again["vault"]["drift"], "0");

    // A withdrawal is a negative credit.
    let w = c
        .owner
        .vault_withdraw(vault, c.owner.address(), 3 * USDC)
        .await
        .unwrap();
    let out = post(&c.app, &path, json!({ "tx_hash": format!("{w:#x}") })).await;
    assert_eq!(out["credited"][0]["delta"], "-3000000");
    assert_eq!(out["credited"][0]["reason"], "vault_withdraw");
    assert_eq!(out["vault"]["ledger_available"], "11000000");

    // Funds the ledger never saw arrive cannot leave it: the withdrawal is
    // skipped and the disagreement shows as drift.
    c.owner.transfer_usdc(vault, 5 * USDC).await.unwrap(); // not synced
    let w = c
        .owner
        .vault_withdraw(vault, c.owner.address(), 16 * USDC)
        .await
        .unwrap();
    let out = post(&c.app, &path, json!({ "tx_hash": format!("{w:#x}") })).await;
    assert_eq!(out["credited"], json!([]));
    assert_eq!(out["skipped"][0]["reason"], "insufficient_ledger_available");
    assert_eq!(out["vault"]["onchain_balance"], "0");
    assert_eq!(out["vault"]["ledger_available"], "11000000");
    assert_eq!(out["vault"]["drift"], "-11000000");

    // A transaction that does not touch the vault posts nothing; garbage is 422.
    let other = c
        .owner
        .transfer_usdc(c.chain.operator(), USDC)
        .await
        .unwrap();
    let out = post(&c.app, &path, json!({ "tx_hash": format!("{other:#x}") })).await;
    assert_eq!(out["credited"], json!([]));
    let (status, err) = send(&c.app, "POST", &path, Some(json!({ "tx_hash": "0x12" }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(err["error"]["code"], "invalid_vault_tx");
    let unknown = format!("0x{}", "ab".repeat(32));
    let (status, err) = send(&c.app, "POST", &path, Some(json!({ "tx_hash": unknown }))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
}

#[tokio::test]
async fn link_refuses_foreign_factory_agent_and_operator() {
    let Some(c) = setup().await else { return };
    let a = agent(&c, "Momentum bull").await;
    let path = format!("/api/agents/{a}/vault");
    let refused = |tx: aen_chain::TxHash| {
        let app = c.app.clone();
        let path = path.clone();
        async move {
            let (status, err) = send(
                &app,
                "POST",
                &path,
                Some(json!({ "tx_hash": format!("{tx:#x}") })),
            )
            .await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{err}");
            assert_eq!(err["error"]["code"], "invalid_vault_tx");
            err["error"]["message"].as_str().unwrap().to_owned()
        }
    };

    // Another deployment's factory.
    let other = deploy(&c.anvil_url, ANVIL_MNEMONIC).await.unwrap();
    let other_owner = OwnerWallet::connect(
        other.cfg.clone(),
        derive_signer(ANVIL_MNEMONIC, TEST_OWNER_INDEX).unwrap(),
    )
    .await
    .unwrap();
    let (_, tx) = other_owner
        .create_vault(a, c.chain.operator(), USDC, USDC)
        .await
        .unwrap();
    assert!(refused(tx).await.contains("factory"));

    // Our factory, but created for another agent.
    let (_, tx) = c
        .owner
        .create_vault(Uuid::new_v4(), c.chain.operator(), USDC, USDC)
        .await
        .unwrap();
    assert!(refused(tx).await.contains("another agent"));

    // Our factory and agent, but someone else operates it.
    let (_, tx) = c
        .owner
        .create_vault(a, c.owner.address(), USDC, USDC)
        .await
        .unwrap();
    assert!(refused(tx).await.contains("operator"));
    assert_eq!(
        get(&c.app, &format!("/api/agents/{a}")).await["env"],
        "simulated"
    );
    let (status, err) = send(&c.app, "GET", &path, None).await;
    assert_eq!(
        (status, err["error"]["code"].as_str()),
        (StatusCode::NOT_FOUND, Some("vault_not_linked"))
    );

    // The right one links once.
    vault_agent(&c, a, 1).await;
    let (_, tx) = c
        .owner
        .create_vault(a, c.chain.operator(), USDC, USDC)
        .await
        .unwrap();
    let (status, err) = send(
        &c.app,
        "POST",
        &path,
        Some(json!({ "tx_hash": format!("{tx:#x}") })),
    )
    .await;
    assert_eq!(
        (status, err["error"]["code"].as_str()),
        (StatusCode::CONFLICT, Some("vault_already_linked"))
    );

    // GET /chain: config only, the public RPC URL, never the backend's.
    let info = get(&c.app, "/api/chain").await;
    assert_eq!(info["enabled"], true);
    assert_eq!(info["chain_id"], 31337);
    assert_eq!(info["rpc_url"], "https://rpc.public.example");
    assert_eq!(info["usdc_decimals"], 6);
    assert_eq!(info["operator_address"], c.chain.operator().to_string());
    assert_eq!(info["escrow_address"], c.cfg.escrow.to_string());
    let text = info.to_string();
    assert!(!text.contains("127.0.0.1"), "{text}");
}

#[tokio::test]
async fn without_chain_access_everything_stays_simulated() {
    let guard = DB_LOCK.lock().await;
    let pool = pool().await;
    let state = AppState::offline(pool.clone());
    let app = app_with_state(state, DEFAULT_CORS_ORIGIN.parse().unwrap());
    assert_eq!(get(&app, "/api/chain").await, json!({ "enabled": false }));
    let a: Uuid = {
        let templates = get(&app, "/api/templates").await;
        let t = templates[0].clone();
        let input = json!({
            "name": "x", "description": "", "config": t["config"], "policy": t["policy"],
            "graph": t["graph"],
        });
        post(&app, "/api/agents", input).await["id"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap()
    };
    let tx = json!({ "tx_hash": format!("0x{}", "11".repeat(32)) });
    let (status, err) = send(&app, "POST", &format!("/api/agents/{a}/vault"), Some(tx)).await;
    assert_eq!(
        (status, err["error"]["code"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("chain_unavailable"))
    );
    // An agent linked earlier, while the chain is now off: 503, not a guess.
    sqlx::query(
        "UPDATE agents SET env = 'arc_testnet', vault_address = $2, vault_owner = $2 WHERE id = $1",
    )
    .bind(a)
    .bind(format!("0x{}", "22".repeat(20)))
    .execute(&pool)
    .await
    .unwrap();
    let (status, _) = send(&app, "GET", &format!("/api/agents/{a}/vault"), None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    drop(guard);
}
