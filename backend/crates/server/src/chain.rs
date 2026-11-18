//! Arc testnet: chain configuration, linking an agent to its onchain
//! `AgentVault` (D-57), syncing vault deposits and withdrawals into the ledger,
//! and a dev-only helper that plays the owner with the test-owner key.
//!
//! Rules kept here:
//! - Keys stay inside `aen-chain`. This module only ever holds a `Chain` and,
//!   when explicitly enabled for dev, an `OwnerWallet`; neither exposes a key.
//! - The ledger is credited from verified receipts only: a transaction hash
//!   from a client is a pointer, never a claim. Each transfer log is posted at
//!   most once (`chain:{tx}:{log_index}`).
//! - Disagreement between the vault and the ledger is reported as `drift`,
//!   never papered over with an invented entry.

use std::str::FromStr;

use aen_chain::{
    ARC_TESTNET_CHAIN_ID, ARC_TESTNET_EXPLORER, ARC_TESTNET_RPC, ARC_TESTNET_USDC, Chain,
    ChainConfig, ChainError, EvmAddress, OwnerWallet, TEST_OWNER_INDEX, TxHash, VaultInfo,
    agent_ref, derive_signer,
};
use aen_core::ledger::{self, Env};
use aen_core::money::{Money, SignedMoney, UNITS_PER_USDC};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::agents::{self, record_activity};
use crate::demo::{DEMO_BULL_ID, DEMO_SKEPTIC_ID};
use crate::error::ApiError;
use crate::extract::{ApiJson, ApiPath};
use crate::ledger_db::{self, PostOutcome};
use crate::models::{LedgerEntry, display_usdc};
use crate::state::AppState;

/// Chain access shared by the API and the lifecycle.
pub struct ChainState {
    pub chain: Chain,
    /// The RPC URL given to browsers. The backend's own `ARC_RPC_URL` may
    /// carry a provider key, so it is never sent to the frontend.
    pub public_rpc_url: String,
    /// Dev only (`AEN_DEV_CHAIN_OWNER=1`): the test owner's wallet (HD index
    /// 2), standing in for an owner's browser wallet.
    pub dev_owner: Option<OwnerWallet>,
    /// The lifecycle sends one escrow / vault transaction at a time (D-59).
    pub(crate) tx_lock: tokio::sync::Mutex<()>,
}

impl ChainState {
    pub fn new(chain: Chain, public_rpc_url: String, dev_owner: Option<OwnerWallet>) -> Self {
        ChainState {
            chain,
            public_rpc_url,
            dev_owner,
            tx_lock: tokio::sync::Mutex::new(()),
        }
    }
}

impl std::fmt::Debug for ChainState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainState")
            .field("chain", &self.chain)
            .field(
                "dev_owner",
                &self.dev_owner.as_ref().map(OwnerWallet::address),
            )
            .finish()
    }
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn env_address(name: &str, default: Option<&str>) -> Result<EvmAddress, String> {
    let text = env_var(name)
        .or_else(|| default.map(str::to_owned))
        .ok_or_else(|| format!("{name} must be set when AEN_TESTNET_MNEMONIC is set"))?;
    EvmAddress::from_str(&text).map_err(|_| format!("{name} is not an address"))
}

/// Chain access from the environment. `None` when `AEN_TESTNET_MNEMONIC` is
/// unset: the server then runs exactly as before, simulated only. With the
/// mnemonic, `ARC_RPC_URL`, `ARC_ESCROW_ADDRESS` and
/// `ARC_VAULT_FACTORY_ADDRESS` are required and the server refuses to start
/// if the chain cannot be reached or is not ours (see `Chain::connect`).
pub async fn from_env(dev_endpoints: bool) -> Result<Option<ChainState>, String> {
    let Some(mnemonic) = env_var("AEN_TESTNET_MNEMONIC") else {
        return Ok(None);
    };
    let rpc_url =
        env_var("ARC_RPC_URL").ok_or("ARC_RPC_URL must be set when AEN_TESTNET_MNEMONIC is set")?;
    let cfg = ChainConfig {
        rpc_url,
        usdc: env_address("ARC_USDC_ADDRESS", Some(ARC_TESTNET_USDC))?,
        escrow: env_address("ARC_ESCROW_ADDRESS", None)?,
        factory: env_address("ARC_VAULT_FACTORY_ADDRESS", None)?,
        explorer_url: Some(env_var("ARC_EXPLORER_URL").unwrap_or(ARC_TESTNET_EXPLORER.into())),
    };
    let chain = Chain::connect(cfg.clone(), &mnemonic)
        .await
        .map_err(|e| format!("Arc testnet: {e}"))?;
    let dev_owner = if dev_endpoints
        && matches!(
            env_var("AEN_DEV_CHAIN_OWNER").as_deref(),
            Some("1" | "true")
        ) {
        let signer = derive_signer(&mnemonic, TEST_OWNER_INDEX).map_err(|e| e.to_string())?;
        Some(
            OwnerWallet::connect(cfg, signer)
                .await
                .map_err(|e| format!("test owner: {e}"))?,
        )
    } else {
        None
    };
    let public_rpc_url = env_var("ARC_PUBLIC_RPC_URL").unwrap_or(ARC_TESTNET_RPC.into());
    Ok(Some(ChainState::new(chain, public_rpc_url, dev_owner)))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Lowercase `0x…` form, as stored in the database.
pub(crate) fn hex<T: std::fmt::LowerHex>(value: T) -> String {
    format!("{value:#x}")
}

pub(crate) fn money(units: u64) -> Result<Money, ApiError> {
    i64::try_from(units)
        .ok()
        .and_then(Money::from_minor)
        .ok_or_else(|| ApiError::internal("onchain amount out of range"))
}

pub(crate) fn chain_unavailable(message: impl Into<String>) -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "chain_unavailable",
        message,
    )
}

/// An RPC failure: logged in full, reported without details (an error string
/// can carry the RPC URL, which may hold a provider key).
pub(crate) fn rpc_failure(e: &ChainError) -> ApiError {
    tracing::warn!(error = %e, "chain call failed");
    chain_unavailable("the Arc testnet RPC call failed; try again")
}

/// Whether the chain refused the call (a revert), as opposed to the RPC
/// being unreachable. Gas estimation reports a revert as an RPC error.
pub(crate) fn is_revert(e: &ChainError) -> bool {
    match e {
        ChainError::Reverted { .. } => true,
        ChainError::Rpc(m) => m.to_ascii_lowercase().contains("revert"),
        _ => false,
    }
}

fn require_chain(state: &AppState) -> Result<&ChainState, ApiError> {
    state
        .chain
        .as_deref()
        .ok_or_else(|| chain_unavailable("this server is not connected to Arc testnet"))
}

fn invalid_tx(message: impl Into<String>) -> ApiError {
    ApiError::unprocessable("invalid_vault_tx", message)
}

fn parse_tx(text: &str) -> Result<TxHash, ApiError> {
    let t = text.trim();
    let ok = t.len() == 66 && t.starts_with("0x") && t[2..].bytes().all(|b| b.is_ascii_hexdigit());
    if !ok {
        return Err(invalid_tx(
            "tx_hash must be 0x followed by 64 hexadecimal characters",
        ));
    }
    TxHash::from_str(t).map_err(|_| invalid_tx("tx_hash is not a transaction hash"))
}

/// Receipt lookups: an unknown or reverted transaction is the caller's
/// problem (422), an unreachable RPC is ours (503).
fn receipt_error(e: ChainError) -> ApiError {
    match e {
        ChainError::Verify(_) => {
            invalid_tx("no receipt for this transaction on Arc testnet (unknown, or not mined yet)")
        }
        ChainError::Reverted { .. } => invalid_tx("this transaction reverted"),
        other => rpc_failure(&other),
    }
}

fn vault_not_linked() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        "vault_not_linked",
        "this agent has no Arc testnet vault",
    )
}

/// `(name, vault)` of an agent; 404 if the agent does not exist.
async fn agent_link(
    conn: &mut PgConnection,
    id: Uuid,
) -> Result<(String, Option<EvmAddress>), ApiError> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT name, vault_address FROM agents WHERE id = $1")
            .bind(id)
            .fetch_optional(conn)
            .await?;
    let (name, vault) = row.ok_or_else(|| ApiError::not_found("agent"))?;
    let vault = vault
        .map(|v| EvmAddress::from_str(&v))
        .transpose()
        .map_err(|_| ApiError::internal("stored vault address is unreadable"))?;
    Ok((name, vault))
}

/// Whether the agent has any interaction or collaboration still open.
async fn has_open_commitments(conn: &mut PgConnection, id: Uuid) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM interactions \
                        WHERE state NOT IN ('settled', 'cancelled', 'failed', 'disputed') \
                          AND (proposer_id = $1 OR counterparty_id = $1)) \
             OR EXISTS (SELECT 1 FROM collaborations \
                        WHERE state IN ('proposed', 'accepted', 'funded') \
                          AND (proposer_id = $1 OR partner_id = $1))",
    )
    .bind(id)
    .fetch_one(conn)
    .await
}

fn open_commitments() -> ApiError {
    ApiError::conflict(
        "agent_has_open_interactions",
        "the agent has open challenges or collaborations; wait until they are settled or \
         cancelled",
    )
}

// ---------------------------------------------------------------------------
// GET /chain
// ---------------------------------------------------------------------------

/// `GET /api/chain`: what the web app needs to talk to the contracts. Never
/// any key, and never the backend's own RPC URL.
pub async fn info(State(state): State<AppState>) -> Json<Value> {
    let Some(cs) = state.chain.as_deref() else {
        return Json(json!({ "enabled": false }));
    };
    let chain = &cs.chain;
    let cfg = chain.config();
    let chain_name = if chain.chain_id() == ARC_TESTNET_CHAIN_ID {
        "Arc Testnet"
    } else {
        "Local dev chain"
    };
    Json(json!({
        "enabled": true,
        "chain_id": chain.chain_id(),
        "chain_name": chain_name,
        "rpc_url": cs.public_rpc_url,
        "explorer_url": cfg.explorer_url,
        "usdc_address": cfg.usdc.to_string(),
        "usdc_decimals": 6,
        "escrow_address": cfg.escrow.to_string(),
        "vault_factory_address": cfg.factory.to_string(),
        "operator_address": chain.operator().to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Vault view
// ---------------------------------------------------------------------------

/// An agent's vault onchain next to its `arc_testnet` ledger balances.
#[derive(Debug, Serialize)]
pub struct VaultView {
    pub address: String,
    pub owner: String,
    pub operator: String,
    pub paused: bool,
    pub max_per_challenge: Money,
    pub max_exposure: Money,
    /// USDC held by the vault contract now.
    pub onchain_balance: Money,
    /// Stakes the vault has in escrow challenges not yet released.
    pub onchain_exposure: Money,
    pub ledger_available: Money,
    pub ledger_committed: Money,
    /// `onchain_balance + onchain_exposure - (ledger_available +
    /// ledger_committed)`. Zero when the ledger matches the vault; non-zero
    /// means a transfer is not synced yet (or a payout is not yet released).
    pub drift: SignedMoney,
    pub explorer_url: Option<String>,
}

pub(crate) async fn ledger_balances(pool: &PgPool, agent: Uuid) -> Result<(i64, i64), ApiError> {
    Ok(sqlx::query_as(
        "SELECT \
           COALESCE(SUM(delta) FILTER (WHERE account = 'agent:' || $1 || ':available'), 0)::BIGINT, \
           COALESCE(SUM(delta) FILTER (WHERE account = 'agent:' || $1 || ':committed'), 0)::BIGINT \
         FROM ledger_entries WHERE env = 'arc_testnet' AND agent_id = $1",
    )
    .bind(agent)
    .fetch_one(pool)
    .await?)
}

async fn vault_view(
    pool: &PgPool,
    cs: &ChainState,
    agent: Uuid,
    vault: EvmAddress,
) -> Result<VaultView, ApiError> {
    let info: VaultInfo = cs
        .chain
        .vault_info(vault)
        .await
        .map_err(|e| rpc_failure(&e))?;
    let (available, committed) = ledger_balances(pool, agent).await?;
    let onchain = i128::from(info.balance) + i128::from(info.exposure);
    let drift = i64::try_from(onchain - i128::from(available) - i128::from(committed))
        .map_err(|_| ApiError::internal("drift out of range"))?;
    let ledger_money = |units: i64| {
        Money::from_minor(units).ok_or_else(|| ApiError::internal("negative ledger balance"))
    };
    Ok(VaultView {
        address: vault.to_string(),
        owner: info.owner.to_string(),
        operator: info.operator.to_string(),
        paused: info.paused,
        max_per_challenge: money(info.max_per_challenge)?,
        max_exposure: money(info.max_exposure)?,
        onchain_balance: money(info.balance)?,
        onchain_exposure: money(info.exposure)?,
        ledger_available: ledger_money(available)?,
        ledger_committed: ledger_money(committed)?,
        drift: SignedMoney::from_minor(drift),
        explorer_url: cs
            .chain
            .config()
            .explorer_url
            .as_ref()
            .map(|base| format!("{}/address/{vault}", base.trim_end_matches('/'))),
    })
}

/// `GET /api/agents/{id}/vault`.
pub async fn get_vault(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<VaultView>, ApiError> {
    let mut conn = state.pool.acquire().await?;
    let (_, vault) = agent_link(&mut conn, id).await?;
    drop(conn);
    let vault = vault.ok_or_else(vault_not_linked)?;
    let cs = require_chain(&state)?;
    Ok(Json(vault_view(&state.pool, cs, id, vault).await?))
}

// ---------------------------------------------------------------------------
// POST /agents/{id}/vault
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TxBody {
    pub tx_hash: String,
}

/// `POST /api/agents/{id}/vault`: links the vault created in `tx_hash`.
pub async fn link_vault(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TxBody>,
) -> Result<Json<VaultView>, ApiError> {
    let cs = require_chain(&state)?;
    let tx = parse_tx(&body.tx_hash)?;
    link(&state, cs, id, tx).await.map(Json)
}

/// Verifies that `tx` created, through our factory, a vault for this agent
/// with our operator, then switches the agent to `arc_testnet`.
async fn link(
    state: &AppState,
    cs: &ChainState,
    id: Uuid,
    tx: TxHash,
) -> Result<VaultView, ApiError> {
    let pool = &state.pool;
    let chain = &cs.chain;
    {
        let mut conn = pool.acquire().await?;
        if agent_link(&mut conn, id).await?.1.is_some() {
            return Err(already_linked());
        }
    }

    // Only logs emitted by our factory are returned.
    let created = chain.vaults_created(tx).await.map_err(receipt_error)?;
    if created.is_empty() {
        return Err(invalid_tx(
            "the transaction did not create a vault through this network's vault factory",
        ));
    }
    let wanted = agent_ref(id);
    let mine: Vec<_> = created.iter().filter(|v| v.agent_ref == wanted).collect();
    let created = match mine.as_slice() {
        [one] => *one,
        [] => {
            return Err(invalid_tx(
                "the vault in this transaction was created for another agent (agentRef does \
                 not match this agent's id)",
            ));
        }
        _ => {
            return Err(invalid_tx(
                "the transaction created several vaults for this agent; link one created alone",
            ));
        }
    };
    if created.operator != chain.operator() {
        return Err(invalid_tx(format!(
            "the vault's operator is {}, not this backend's operator {}",
            created.operator,
            chain.operator()
        )));
    }
    // The owner may have changed the operator since: check the vault now.
    let info = chain
        .vault_info(created.vault)
        .await
        .map_err(|e| rpc_failure(&e))?;
    if info.operator != chain.operator() || info.escrow != chain.config().escrow {
        return Err(invalid_tx(
            "the vault no longer lets this backend operate it (operator changed or revoked)",
        ));
    }

    let mut db = pool.begin().await?;
    // Discovery takes the same lock before it writes a proposal, and commits
    // take the agent lock, so the env cannot change under either.
    for lock in ["aen:propose".to_owned(), format!("aen:agent:{id}")] {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock)
            .execute(&mut *db)
            .await?;
    }
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT name, vault_address FROM agents WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *db)
            .await?;
    let (name, existing) = row.ok_or_else(|| ApiError::not_found("agent"))?;
    if existing.is_some() {
        return Err(already_linked());
    }
    if has_open_commitments(&mut db, id).await? {
        return Err(open_commitments());
    }
    let updated = sqlx::query(
        "UPDATE agents SET env = 'arc_testnet', vault_address = $2, vault_owner = $3, \
         updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(hex(created.vault))
    .bind(hex(created.owner))
    .execute(&mut *db)
    .await;
    if let Err(sqlx::Error::Database(e)) = &updated
        && e.is_unique_violation()
    {
        return Err(already_linked());
    }
    updated?;
    record_activity(
        &mut db,
        Some(id),
        "vault_linked",
        format!(
            "\"{name}\" is now linked to its Arc testnet vault {} (owned by {}). Its stakes now \
             come from that vault; any simulated balance stays in the simulated ledger.",
            created.vault, created.owner
        ),
        Some(json!({
            "env": Env::ArcTestnet, "vault": created.vault.to_string(),
            "owner": created.owner.to_string(), "tx_hash": hex(tx),
            "explorer_url": chain.explorer_tx(tx),
        })),
    )
    .await?;
    db.commit().await?;
    vault_view(pool, cs, id, created.vault).await
}

fn already_linked() -> ApiError {
    ApiError::conflict(
        "vault_already_linked",
        "this agent is already linked to a vault",
    )
}

// ---------------------------------------------------------------------------
// POST /agents/{id}/vault/sync
// ---------------------------------------------------------------------------

/// A transfer in the transaction that was not posted.
#[derive(Debug, Serialize)]
pub struct SkippedTransfer {
    pub log_index: u64,
    pub amount: Money,
    pub reason: &'static str,
}

#[derive(Debug, Serialize)]
pub struct SyncReport {
    /// The agent's ledger lines posted by this call (empty on a replay):
    /// deposits positive, withdrawals negative.
    pub credited: Vec<LedgerEntry>,
    /// Vault transfers in the transaction that were not posted.
    pub skipped: Vec<SkippedTransfer>,
    pub vault: VaultView,
}

/// `POST /api/agents/{id}/vault/sync`: posts the USDC transfers into or out
/// of the agent's vault found in `tx_hash`'s receipt.
pub async fn sync_vault(
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(body): ApiJson<TxBody>,
) -> Result<Json<SyncReport>, ApiError> {
    let mut conn = state.pool.acquire().await?;
    let (_, vault) = agent_link(&mut conn, id).await?;
    drop(conn);
    let vault = vault.ok_or_else(vault_not_linked)?;
    let cs = require_chain(&state)?;
    let tx = parse_tx(&body.tx_hash)?;
    sync(&state, cs, id, vault, tx).await.map(Json)
}

async fn sync(
    state: &AppState,
    cs: &ChainState,
    id: Uuid,
    vault: EvmAddress,
    tx: TxHash,
) -> Result<SyncReport, ApiError> {
    let chain = &cs.chain;
    let transfers = chain.usdc_transfers(tx).await.map_err(receipt_error)?;
    let escrow = chain.config().escrow;
    let tx_text = hex(tx);

    let mut db = state.pool.begin().await?;
    let (name, linked) = agent_link(&mut db, id).await?;
    if linked != Some(vault) {
        return Err(vault_not_linked());
    }
    let mut keys = Vec::new();
    let mut skipped = Vec::new();
    for t in &transfers {
        // Stakes and payouts move between the vault and the escrow; the
        // lifecycle accounts for those. Only owner-side flows are synced here.
        let set = if t.to == vault && t.from != escrow {
            ledger::vault_deposit(id, money(t.value)?, &tx_text, t.log_index)
        } else if t.from == vault && t.to != escrow {
            ledger::vault_withdraw(id, money(t.value)?, &tx_text, t.log_index)
        } else {
            continue;
        };
        let set = match set {
            Ok(set) => set,
            Err(ledger::LedgerError::ZeroAmount) => continue,
            Err(e) => return Err(e.into()),
        };
        let deposit = t.to == vault;
        match ledger_db::post(&mut db, &set).await {
            Ok(PostOutcome::Posted) => {
                let amount = money(t.value)?;
                let (kind, summary) = if deposit {
                    (
                        "vault_deposit",
                        format!(
                            "{} (Arc testnet USDC) arrived in \"{name}\"'s vault and is \
                             available to it.",
                            display_usdc(amount)
                        ),
                    )
                } else {
                    (
                        "vault_withdraw",
                        format!(
                            "{} (Arc testnet USDC) was withdrawn from \"{name}\"'s vault by its \
                             owner.",
                            display_usdc(amount)
                        ),
                    )
                };
                record_activity(
                    &mut db,
                    Some(id),
                    kind,
                    summary,
                    Some(json!({
                        "amount": amount, "env": Env::ArcTestnet, "tx_hash": tx_text,
                        "log_index": t.log_index, "explorer_url": chain.explorer_tx(tx),
                    })),
                )
                .await?;
                keys.push(set.idempotency_key().to_owned());
            }
            Ok(PostOutcome::Replayed) => {}
            // The ledger cannot follow a withdrawal of funds it never saw
            // arrive: report it (it shows as drift), invent nothing.
            Err(e) if e.code == "insufficient_funds" => skipped.push(SkippedTransfer {
                log_index: t.log_index,
                amount: money(t.value)?,
                reason: "insufficient_ledger_available",
            }),
            Err(e) => return Err(e),
        }
    }
    db.commit().await?;
    Ok(SyncReport {
        credited: ledger_db::agent_entries_by_keys(&state.pool, Env::ArcTestnet, id, &keys).await?,
        skipped,
        vault: vault_view(&state.pool, cs, id, vault).await?,
    })
}

// ---------------------------------------------------------------------------
// Dev: test-owner vaults for the demo agents
// ---------------------------------------------------------------------------

const DEFAULT_DEMO_VAULT_FUNDS: u64 = 5 * UNITS_PER_USDC as u64;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupBody {
    /// Target vault balance per demo agent (and both onchain caps).
    #[serde(default)]
    pub amount: Option<Money>,
}

fn owner_tx_failed(e: &ChainError) -> ApiError {
    tracing::warn!(error = %e, "test owner transaction failed");
    ApiError::conflict(
        "dev_owner_tx_failed",
        "the test owner's transaction failed (is its wallet funded with testnet USDC?)",
    )
}

/// `POST /api/dev/chain/setup-demo-vaults` (dev only, `AEN_DEV_CHAIN_OWNER=1`).
/// For the two seed-demo agents: creates a vault with the test-owner key if
/// the agent has none, links it through the same verification as
/// `POST /agents/{id}/vault`, tops the vault up to `amount` and syncs the
/// transfer through the same path as `POST /agents/{id}/vault/sync`.
pub async fn dev_setup_demo_vaults(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<SetupBody>,
) -> Result<Json<Value>, ApiError> {
    let cs = require_chain(&state)?;
    let owner = cs
        .dev_owner
        .as_ref()
        .ok_or_else(|| ApiError::not_found("dev chain owner"))?;
    let amount = body
        .amount
        .map_or(DEFAULT_DEMO_VAULT_FUNDS, |m| m.minor() as u64);
    if amount == 0 {
        return Err(ApiError::unprocessable(
            "invalid_amount",
            "amount must be positive",
        ));
    }
    let chain = &cs.chain;
    let mut out = Vec::new();
    for id in [DEMO_BULL_ID, DEMO_SKEPTIC_ID] {
        let mut conn = state.pool.acquire().await?;
        let (_, linked) = agent_link(&mut conn, id).await.map_err(|e| {
            if e.status == StatusCode::NOT_FOUND {
                ApiError::conflict("demo_not_seeded", "run POST /api/dev/seed-demo first")
            } else {
                e
            }
        })?;
        let mut link_tx = None;
        let vault = match linked {
            Some(vault) => vault,
            None => {
                if has_open_commitments(&mut conn, id).await? {
                    return Err(open_commitments());
                }
                drop(conn);
                let (vault, tx) = owner
                    .create_vault(id, chain.operator(), amount, amount)
                    .await
                    .map_err(|e| owner_tx_failed(&e))?;
                link(&state, cs, id, tx).await?;
                link_tx = Some(hex(tx));
                vault
            }
        };
        rename_for_testnet(&state.pool, id).await?;

        let info = chain.vault_info(vault).await.map_err(|e| rpc_failure(&e))?;
        let mut funding_tx = None;
        let mut credited = Vec::new();
        if info.balance < amount {
            let tx = owner
                .transfer_usdc(vault, amount - info.balance)
                .await
                .map_err(|e| owner_tx_failed(&e))?;
            credited = sync(&state, cs, id, vault, tx).await?.credited;
            funding_tx = Some(hex(tx));
        }
        out.push(json!({
            "id": id,
            "link_tx": link_tx,
            "funding_tx": funding_tx,
            "credited": credited,
            "vault": vault_view(&state.pool, cs, id, vault).await?,
        }));
    }
    Ok(Json(json!({
        "env": Env::ArcTestnet,
        "owner": owner.address().to_string(),
        "agents": out,
        "note": "Arc testnet USDC (testnet asset, no real value). The test owner key stands in \
                 for the owner's browser wallet.",
    })))
}

/// The seed names say "(simulated)"; once on a vault that label is wrong.
async fn rename_for_testnet(pool: &PgPool, id: Uuid) -> Result<(), ApiError> {
    let mut db = pool.begin().await?;
    let Some(mut input) = agents::current_input(&mut db, id).await? else {
        return Ok(());
    };
    if !input.name.contains("(simulated)") {
        return Ok(());
    }
    input.name = input.name.replace("(simulated)", "(Arc testnet)");
    input.description = input
        .description
        .replace("on simulated funds", "on Arc testnet USDC");
    agents::apply_update(&mut db, id, input).await?;
    db.commit().await?;
    Ok(())
}
