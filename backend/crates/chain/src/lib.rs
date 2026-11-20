//! Arc testnet access for the agent economy: `ChallengeEscrow` and `AgentVault` calls, and
//! receipt verification for owner transactions made in the browser.
//!
//! Custody (D-57): owner funds live in per-agent `AgentVault`s owned by the owner's own wallet.
//! This crate holds only two testnet-only keys, derived from `AEN_TESTNET_MNEMONIC` (D-58):
//! - index 0, the escrow **resolver**: registers, resolves and voids challenges;
//! - index 1, the vault **operator**: may only stake a vault's funds into an escrow challenge that
//!   names that vault, within the caps the owner set onchain, and release finished challenges.
//!
//! Neither key can send owner funds anywhere else. Keys never leave this crate: no getter returns
//! them and `Chain`'s `Debug` prints addresses only.
//!
//! Refuses to start on any chain but Arc testnet (5042002) or a local dev chain (31337).

use std::fmt;
use std::time::Duration;

use alloy::network::EthereumWallet;
use alloy::primitives::{Address, B256, U256};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::types::TransactionReceipt;
use alloy::signers::local::{MnemonicBuilder, PrivateKeySigner, coins_bip39::English};
use alloy::sol;
use alloy::sol_types::SolEvent;
use uuid::Uuid;

pub use alloy::primitives::{Address as EvmAddress, TxHash};

pub mod events;

pub const ARC_TESTNET_CHAIN_ID: u64 = 5_042_002;
pub const LOCAL_CHAIN_ID: u64 = 31_337;
/// Arc testnet USDC (ERC-20 interface, 6 decimals). Source: docs/research/sponsors.md.
pub const ARC_TESTNET_USDC: &str = "0x3600000000000000000000000000000000000000";
pub const ARC_TESTNET_RPC: &str = "https://rpc.testnet.arc.io";
pub const ARC_TESTNET_EXPLORER: &str = "https://explorer.testnet.arc.io";

/// HD index of each backend key in the testnet mnemonic.
pub const RESOLVER_INDEX: u32 = 0;
pub const OPERATOR_INDEX: u32 = 1;
/// Stands in for an owner's browser wallet in automated tests and the dev test-owner endpoints.
pub const TEST_OWNER_INDEX: u32 = 2;

sol! {
    #[sol(rpc)]
    interface IChallengeEscrow {
        enum Status { None, Created, Active, Resolved, Voided, Refunded }
        struct Challenge {
            address partyA;
            uint64 fundDeadline;
            Status status;
            bool depositedA;
            bool depositedB;
            address partyB;
            uint64 resolveDeadline;
            uint256 stake;
            address winner;
        }
        function create(bytes32 id, address partyA, address partyB, uint256 stake, uint64 fundDeadline, uint64 resolveDeadline) external;
        function resolve(bytes32 id, address winner) external;
        function resolveVoid(bytes32 id) external;
        function refund(bytes32 id) external;
        function getChallenge(bytes32 id) external view returns (Challenge memory);
        function RESOLVER() external view returns (address);
        function USDC() external view returns (address);
    }

    #[sol(rpc)]
    interface IAgentVault {
        function stake(bytes32 id) external;
        function release(bytes32 id) external;
        function withdraw(address to, uint256 amount) external;
        function setOperator(address operator_) external;
        function setLimits(uint256 maxPerChallenge_, uint256 maxExposure_) external;
        function setPaused(bool paused_) external;
        function OWNER() external view returns (address);
        function ESCROW() external view returns (address);
        function operator() external view returns (address);
        function paused() external view returns (bool);
        function maxPerChallenge() external view returns (uint256);
        function maxExposure() external view returns (uint256);
        function exposure() external view returns (uint256);
        function staked(bytes32 id) external view returns (uint256);
        function balance() external view returns (uint256);
    }

    #[sol(rpc)]
    interface IVaultFactory {
        event VaultCreated(address indexed vault, address indexed owner, address indexed operator, bytes32 agentRef, uint256 maxPerChallenge, uint256 maxExposure);
        function createVault(address operator, uint256 maxPerChallenge, uint256 maxExposure, bytes32 agentRef) external returns (address);
        function isVault(address vault) external view returns (bool);
        function ESCROW() external view returns (address);
    }

    #[sol(rpc)]
    interface IERC20 {
        event Transfer(address indexed from, address indexed to, uint256 value);
        function transfer(address to, uint256 amount) external returns (bool);
        function balanceOf(address who) external view returns (uint256);
        function decimals() external view returns (uint8);
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ChainError {
    #[error("chain config: {0}")]
    Config(String),
    #[error("refusing chain id {0}: only Arc testnet (5042002) or a local chain (31337)")]
    WrongChain(u64),
    #[error("rpc: {0}")]
    Rpc(String),
    #[error("transaction {tx} reverted")]
    Reverted { tx: TxHash },
    #[error("verification failed: {0}")]
    Verify(String),
}

fn rpc<E: fmt::Display>(e: E) -> ChainError {
    ChainError::Rpc(e.to_string())
}

pub type Result<T> = std::result::Result<T, ChainError>;

/// Where the contracts are. Addresses only: the mnemonic is passed separately.
#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub rpc_url: String,
    pub usdc: Address,
    pub escrow: Address,
    pub factory: Address,
    pub explorer_url: Option<String>,
}

/// On-chain status of an escrow challenge. Mirrors `ChallengeEscrow.Status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EscrowStatus {
    None,
    Created,
    Active,
    Resolved,
    Voided,
    Refunded,
}

impl EscrowStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Resolved | Self::Voided | Self::Refunded)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChallengeView {
    pub status: EscrowStatus,
    pub party_a: Address,
    pub party_b: Address,
    pub deposited_a: bool,
    pub deposited_b: bool,
    pub stake: u64,
    pub fund_deadline: u64,
    pub resolve_deadline: u64,
    pub winner: Address,
}

impl ChallengeView {
    pub fn deposited(&self, party: Address) -> bool {
        (party == self.party_a && self.deposited_a) || (party == self.party_b && self.deposited_b)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VaultInfo {
    pub vault: Address,
    pub owner: Address,
    pub operator: Address,
    pub escrow: Address,
    pub paused: bool,
    pub max_per_challenge: u64,
    pub max_exposure: u64,
    pub exposure: u64,
    pub balance: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct VaultCreatedLog {
    pub vault: Address,
    pub owner: Address,
    pub operator: Address,
    pub agent_ref: B256,
    pub max_per_challenge: u64,
    pub max_exposure: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TransferLog {
    pub log_index: u64,
    pub from: Address,
    pub to: Address,
    pub value: u64,
}

/// The escrow id of a backend interaction: its UUID, left-padded to 32 bytes.
pub fn challenge_id(interaction: Uuid) -> B256 {
    B256::left_padding_from(interaction.as_bytes())
}

/// The `agentRef` a vault is created with: the agent's UUID, left-padded to 32 bytes.
pub fn agent_ref(agent: Uuid) -> B256 {
    B256::left_padding_from(agent.as_bytes())
}

pub fn derive_signer(mnemonic: &str, index: u32) -> Result<PrivateKeySigner> {
    MnemonicBuilder::<English>::default()
        .phrase(mnemonic.trim())
        .index(index)
        .map_err(|e| ChainError::Config(format!("mnemonic index: {e}")))?
        .build()
        .map_err(|_| ChainError::Config("invalid mnemonic".into()))
}

/// A provider that signs with `wallet` and reads the pending nonce from the node for every
/// transaction. alloy's default nonce cache never resynchronises, so a failed send or another
/// process using the same key would wedge it; we send one transaction at a time, so the extra
/// `eth_getTransactionCount` is cheap.
///
/// Arc's public RPC rate-limits bursts (`-32005 rate limit exceeded`), so requests that hit a rate
/// limit are retried with backoff. Arc confirms in under a second, so receipts of remote chains
/// are polled every second instead of alloy's 7 s default.
pub(crate) fn signing_provider(wallet: EthereumWallet, rpc_url: &str) -> Result<DynProvider> {
    let url = rpc_url
        .parse()
        .map_err(|e| ChainError::Config(format!("rpc url: {e}")))?;
    let client = alloy::rpc::client::ClientBuilder::default()
        .layer(alloy::transports::layers::RetryBackoffLayer::new(10, 1_000, 100))
        .http(url);
    if !client.is_local() {
        client.set_poll_interval(Duration::from_secs(1));
    }
    Ok(ProviderBuilder::new()
        .disable_recommended_fillers()
        .with_gas_estimation()
        .with_simple_nonce_management()
        .fetch_chain_id()
        .wallet(wallet)
        .connect_client(client)
        .erased())
}

fn to_u64(v: U256, what: &str) -> Result<u64> {
    u64::try_from(v).map_err(|_| ChainError::Verify(format!("{what} does not fit in u64")))
}

/// Testnet chain access. Holds the resolver and operator signers.
#[derive(Clone)]
pub struct Chain {
    provider: DynProvider,
    chain_id: u64,
    resolver: Address,
    operator: Address,
    cfg: ChainConfig,
}

impl fmt::Debug for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Chain")
            .field("chain_id", &self.chain_id)
            .field("resolver", &self.resolver)
            .field("operator", &self.operator)
            .field("cfg", &self.cfg)
            .finish()
    }
}

impl Chain {
    /// Connects, checks the chain id and that the escrow's resolver is our resolver key.
    pub async fn connect(cfg: ChainConfig, mnemonic: &str) -> Result<Self> {
        let resolver = derive_signer(mnemonic, RESOLVER_INDEX)?;
        let operator = derive_signer(mnemonic, OPERATOR_INDEX)?;
        Self::connect_with(cfg, resolver, operator).await
    }

    pub async fn connect_with(
        cfg: ChainConfig,
        resolver: PrivateKeySigner,
        operator: PrivateKeySigner,
    ) -> Result<Self> {
        let (resolver_addr, operator_addr) = (resolver.address(), operator.address());
        let mut wallet = EthereumWallet::new(resolver);
        wallet.register_signer(operator);
        let provider = signing_provider(wallet, &cfg.rpc_url)?;
        let chain_id = provider.get_chain_id().await.map_err(rpc)?;
        if chain_id != ARC_TESTNET_CHAIN_ID && chain_id != LOCAL_CHAIN_ID {
            return Err(ChainError::WrongChain(chain_id));
        }
        let chain = Self {
            provider,
            chain_id,
            resolver: resolver_addr,
            operator: operator_addr,
            cfg,
        };
        let escrow = IChallengeEscrow::new(chain.cfg.escrow, &chain.provider);
        let on_chain_resolver = escrow.RESOLVER().call().await.map_err(rpc)?;
        if on_chain_resolver != resolver_addr {
            return Err(ChainError::Config(format!(
                "escrow {} has resolver {on_chain_resolver}, our resolver key is {resolver_addr}",
                chain.cfg.escrow
            )));
        }
        let factory = IVaultFactory::new(chain.cfg.factory, &chain.provider);
        if factory.ESCROW().call().await.map_err(rpc)? != chain.cfg.escrow {
            return Err(ChainError::Config("vault factory is bound to another escrow".into()));
        }
        Ok(chain)
    }

    pub fn chain_id(&self) -> u64 {
        self.chain_id
    }
    pub fn resolver(&self) -> Address {
        self.resolver
    }
    pub fn operator(&self) -> Address {
        self.operator
    }
    pub fn config(&self) -> &ChainConfig {
        &self.cfg
    }
    pub fn provider(&self) -> &DynProvider {
        &self.provider
    }

    pub async fn block_timestamp(&self) -> Result<u64> {
        let block = self
            .provider
            .get_block_by_number(alloy::eips::BlockNumberOrTag::Latest)
            .await
            .map_err(rpc)?
            .ok_or_else(|| ChainError::Rpc("no latest block".into()))?;
        Ok(block.header.timestamp)
    }

    pub async fn usdc_balance(&self, who: Address) -> Result<u64> {
        let usdc = IERC20::new(self.cfg.usdc, &self.provider);
        to_u64(usdc.balanceOf(who).call().await.map_err(rpc)?, "balance")
    }

    // ------------------------------------------------------------------ escrow

    pub async fn get_challenge(&self, id: B256) -> Result<ChallengeView> {
        let escrow = IChallengeEscrow::new(self.cfg.escrow, &self.provider);
        let c = escrow.getChallenge(id).call().await.map_err(rpc)?;
        let status = match c.status as u8 {
            0 => EscrowStatus::None,
            1 => EscrowStatus::Created,
            2 => EscrowStatus::Active,
            3 => EscrowStatus::Resolved,
            4 => EscrowStatus::Voided,
            5 => EscrowStatus::Refunded,
            n => return Err(ChainError::Verify(format!("unknown escrow status {n}"))),
        };
        Ok(ChallengeView {
            status,
            party_a: c.partyA,
            party_b: c.partyB,
            deposited_a: c.depositedA,
            deposited_b: c.depositedB,
            stake: to_u64(c.stake, "stake")?,
            fund_deadline: c.fundDeadline,
            resolve_deadline: c.resolveDeadline,
            winner: c.winner,
        })
    }

    pub async fn create_challenge(
        &self,
        id: B256,
        party_a: Address,
        party_b: Address,
        stake: u64,
        fund_deadline: u64,
        resolve_deadline: u64,
    ) -> Result<TxHash> {
        let escrow = IChallengeEscrow::new(self.cfg.escrow, &self.provider);
        let call = escrow
            .create(id, party_a, party_b, U256::from(stake), fund_deadline, resolve_deadline)
            .from(self.resolver);
        self.confirm(call.send().await.map_err(rpc)?).await
    }

    pub async fn resolve(&self, id: B256, winner: Address) -> Result<TxHash> {
        let escrow = IChallengeEscrow::new(self.cfg.escrow, &self.provider);
        let call = escrow.resolve(id, winner).from(self.resolver);
        self.confirm(call.send().await.map_err(rpc)?).await
    }

    pub async fn resolve_void(&self, id: B256) -> Result<TxHash> {
        let escrow = IChallengeEscrow::new(self.cfg.escrow, &self.provider);
        let call = escrow.resolveVoid(id).from(self.resolver);
        self.confirm(call.send().await.map_err(rpc)?).await
    }

    // ------------------------------------------------------------------ vaults

    /// Operator: deposit `vault`'s stake into challenge `id`. The vault enforces party and caps.
    pub async fn vault_stake(&self, vault: Address, id: B256) -> Result<TxHash> {
        let v = IAgentVault::new(vault, &self.provider);
        let call = v.stake(id).from(self.operator);
        self.confirm(call.send().await.map_err(rpc)?).await
    }

    /// Frees the vault's exposure for a finished challenge. Moves no funds. No-op if nothing staked.
    pub async fn vault_release(&self, vault: Address, id: B256) -> Result<Option<TxHash>> {
        let v = IAgentVault::new(vault, &self.provider);
        if v.staked(id).call().await.map_err(rpc)?.is_zero() {
            return Ok(None);
        }
        let call = v.release(id).from(self.operator);
        Ok(Some(self.confirm(call.send().await.map_err(rpc)?).await?))
    }

    pub async fn vault_info(&self, vault: Address) -> Result<VaultInfo> {
        let v = IAgentVault::new(vault, &self.provider);
        Ok(VaultInfo {
            vault,
            owner: v.OWNER().call().await.map_err(rpc)?,
            operator: v.operator().call().await.map_err(rpc)?,
            escrow: v.ESCROW().call().await.map_err(rpc)?,
            paused: v.paused().call().await.map_err(rpc)?,
            max_per_challenge: to_u64(v.maxPerChallenge().call().await.map_err(rpc)?, "cap")?,
            max_exposure: to_u64(v.maxExposure().call().await.map_err(rpc)?, "cap")?,
            exposure: to_u64(v.exposure().call().await.map_err(rpc)?, "exposure")?,
            balance: self.usdc_balance(vault).await?,
        })
    }

    pub async fn is_vault(&self, vault: Address) -> Result<bool> {
        let f = IVaultFactory::new(self.cfg.factory, &self.provider);
        f.isVault(vault).call().await.map_err(rpc)
    }

    // ------------------------------------------------------------------ receipts

    /// A successful receipt, or an error if the tx is unknown or reverted.
    pub async fn receipt(&self, tx: TxHash) -> Result<TransactionReceipt> {
        let r = self
            .provider
            .get_transaction_receipt(tx)
            .await
            .map_err(rpc)?
            .ok_or_else(|| ChainError::Verify(format!("no receipt for {tx}")))?;
        if !r.status() {
            return Err(ChainError::Reverted { tx });
        }
        Ok(r)
    }

    /// `VaultCreated` events emitted by our factory in `tx`.
    pub async fn vaults_created(&self, tx: TxHash) -> Result<Vec<VaultCreatedLog>> {
        let r = self.receipt(tx).await?;
        let mut out = Vec::new();
        for log in r.inner.logs() {
            if log.address() != self.cfg.factory {
                continue;
            }
            if let Ok(ev) = IVaultFactory::VaultCreated::decode_log(&log.inner) {
                out.push(VaultCreatedLog {
                    vault: ev.vault,
                    owner: ev.owner,
                    operator: ev.operator,
                    agent_ref: ev.agentRef,
                    max_per_challenge: to_u64(ev.maxPerChallenge, "cap")?,
                    max_exposure: to_u64(ev.maxExposure, "cap")?,
                });
            }
        }
        Ok(out)
    }

    /// USDC `Transfer` events in `tx`, with their log index.
    pub async fn usdc_transfers(&self, tx: TxHash) -> Result<Vec<TransferLog>> {
        let r = self.receipt(tx).await?;
        let mut out = Vec::new();
        for log in r.inner.logs() {
            if log.address() != self.cfg.usdc {
                continue;
            }
            if let Ok(ev) = IERC20::Transfer::decode_log(&log.inner) {
                out.push(TransferLog {
                    log_index: log.log_index.unwrap_or_default(),
                    from: ev.from,
                    to: ev.to,
                    value: to_u64(ev.value, "transfer value")?,
                });
            }
        }
        Ok(out)
    }

    pub fn explorer_tx(&self, tx: TxHash) -> Option<String> {
        self.cfg
            .explorer_url
            .as_ref()
            .map(|base| format!("{}/tx/{tx}", base.trim_end_matches('/')))
    }

    async fn confirm<N: alloy::network::Network>(
        &self,
        pending: alloy::providers::PendingTransactionBuilder<N>,
    ) -> Result<TxHash> {
        let tx = *pending.tx_hash();
        let receipt = pending
            .with_timeout(Some(Duration::from_secs(60)))
            .get_receipt()
            .await
            .map_err(rpc)?;
        if !alloy::network::ReceiptResponse::status(&receipt) {
            return Err(ChainError::Reverted { tx });
        }
        tracing::info!(%tx, "chain tx confirmed");
        Ok(tx)
    }
}

// ---------------------------------------------------------------------- owner-side helpers

/// Owner-side calls, normally made by the owner's browser wallet. The backend uses these only for
/// the test owner (`TEST_OWNER_INDEX`) in automated tests and dev endpoints; real owners sign in
/// their own wallet and the backend only verifies the receipts.
pub struct OwnerWallet {
    provider: DynProvider,
    address: Address,
    cfg: ChainConfig,
}

impl OwnerWallet {
    pub async fn connect(cfg: ChainConfig, signer: PrivateKeySigner) -> Result<Self> {
        let address = signer.address();
        let provider = signing_provider(EthereumWallet::new(signer), &cfg.rpc_url)?;
        let chain_id = provider.get_chain_id().await.map_err(rpc)?;
        if chain_id != ARC_TESTNET_CHAIN_ID && chain_id != LOCAL_CHAIN_ID {
            return Err(ChainError::WrongChain(chain_id));
        }
        Ok(Self { provider, address, cfg })
    }

    pub fn address(&self) -> Address {
        self.address
    }

    async fn confirm(
        &self,
        pending: alloy::providers::PendingTransactionBuilder<alloy::network::Ethereum>,
    ) -> Result<TransactionReceipt> {
        let tx = *pending.tx_hash();
        let r = pending
            .with_timeout(Some(Duration::from_secs(60)))
            .get_receipt()
            .await
            .map_err(rpc)?;
        if !r.status() {
            return Err(ChainError::Reverted { tx });
        }
        Ok(r)
    }

    /// Creates a vault for `agent` with the given operator and caps. Returns (vault, tx).
    pub async fn create_vault(
        &self,
        agent: Uuid,
        operator: Address,
        max_per_challenge: u64,
        max_exposure: u64,
    ) -> Result<(Address, TxHash)> {
        let f = IVaultFactory::new(self.cfg.factory, &self.provider);
        let pending = f
            .createVault(
                operator,
                U256::from(max_per_challenge),
                U256::from(max_exposure),
                agent_ref(agent),
            )
            .send()
            .await
            .map_err(rpc)?;
        let r = self.confirm(pending).await?;
        let vault = r
            .inner
            .logs()
            .iter()
            .filter(|l| l.address() == self.cfg.factory)
            .find_map(|l| IVaultFactory::VaultCreated::decode_log(&l.inner).ok())
            .map(|ev| ev.vault)
            .ok_or_else(|| ChainError::Verify("no VaultCreated event".into()))?;
        Ok((vault, r.transaction_hash))
    }

    pub async fn transfer_usdc(&self, to: Address, amount: u64) -> Result<TxHash> {
        let usdc = IERC20::new(self.cfg.usdc, &self.provider);
        let pending = usdc
            .transfer(to, U256::from(amount))
            .send()
            .await
            .map_err(rpc)?;
        Ok(self.confirm(pending).await?.transaction_hash)
    }

    pub async fn vault_withdraw(&self, vault: Address, to: Address, amount: u64) -> Result<TxHash> {
        let v = IAgentVault::new(vault, &self.provider);
        let pending = v
            .withdraw(to, U256::from(amount))
            .send()
            .await
            .map_err(rpc)?;
        Ok(self.confirm(pending).await?.transaction_hash)
    }

    pub async fn vault_set_paused(&self, vault: Address, paused: bool) -> Result<TxHash> {
        let v = IAgentVault::new(vault, &self.provider);
        let pending = v.setPaused(paused).send().await.map_err(rpc)?;
        Ok(self.confirm(pending).await?.transaction_hash)
    }
}

#[cfg(feature = "testkit")]
pub mod testkit;
