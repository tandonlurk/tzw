//! Test-only: a local anvil chain with MockUSDC, ChallengeEscrow and VaultFactory deployed from the
//! Foundry build output (`contracts/out`). Uses anvil's well-known dev mnemonic, never a real key.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{Address, Bytes, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::sol_types::SolValue;

use crate::{ChainConfig, ChainError, Result, derive_signer, rpc};

/// Anvil's default dev mnemonic. Its accounts are pre-funded with native ETH on anvil only.
pub const ANVIL_MNEMONIC: &str = "test test test test test test test test test test test junk";

alloy::sol! {
    #[sol(rpc)]
    interface IMockUSDC {
        function mint(address to, uint256 amount) external;
    }
}

pub struct Anvil {
    child: tokio::process::Child,
    pub url: String,
}

impl Drop for Anvil {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn anvil_bin() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let candidates = [
        Some(PathBuf::from("anvil")),
        home.map(|h| h.join(".foundry/bin/anvil")),
    ];
    candidates.into_iter().flatten().find(|p| {
        std::process::Command::new(p)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok()
    })
}

/// Starts anvil on a free port, or `None` if anvil is not installed.
pub async fn spawn_anvil() -> Option<Anvil> {
    let bin = anvil_bin()?;
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .ok()?
        .local_addr()
        .ok()?
        .port();
    let child = tokio::process::Command::new(bin)
        .args([
            "--port",
            &port.to_string(),
            "--silent",
            "--chain-id",
            "31337",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let url = format!("http://127.0.0.1:{port}");
    let provider = ProviderBuilder::new().connect_http(url.parse().ok()?);
    for _ in 0..100 {
        if provider.get_chain_id().await.is_ok() {
            return Some(Anvil { child, url });
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    None
}

fn artifact(file: &str, contract: &str) -> Result<Bytes> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../contracts/out")
        .join(file)
        .join(format!("{contract}.json"));
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        ChainError::Config(format!(
            "{}: {e} (run `forge build` in contracts/)",
            path.display()
        ))
    })?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| ChainError::Config(e.to_string()))?;
    let hex = v["bytecode"]["object"]
        .as_str()
        .ok_or_else(|| ChainError::Config(format!("no bytecode in {}", path.display())))?;
    hex.parse::<Bytes>()
        .map_err(|e| ChainError::Config(e.to_string()))
}

pub struct Deployed {
    pub cfg: ChainConfig,
}

/// Deploys MockUSDC, ChallengeEscrow (resolver = `mnemonic` index 0) and VaultFactory.
pub async fn deploy(url: &str, mnemonic: &str) -> Result<Deployed> {
    let deployer = derive_signer(mnemonic, crate::TEST_OWNER_INDEX)?;
    let resolver = derive_signer(mnemonic, crate::RESOLVER_INDEX)?.address();
    let provider = crate::signing_provider(EthereumWallet::new(deployer), url)?;

    async fn create<P: Provider>(p: &P, code: Bytes, args: Vec<u8>) -> Result<Address> {
        let mut data = code.to_vec();
        data.extend(args);
        let tx = TransactionRequest::default().with_deploy_code(Bytes::from(data));
        let r = p
            .send_transaction(tx)
            .await
            .map_err(rpc)?
            .get_receipt()
            .await
            .map_err(rpc)?;
        r.contract_address
            .ok_or_else(|| ChainError::Verify("no contract address".into()))
    }

    let usdc = create(&provider, artifact("MockTokens.sol", "MockUSDC")?, vec![]).await?;
    let escrow = create(
        &provider,
        artifact("ChallengeEscrow.sol", "ChallengeEscrow")?,
        (usdc, resolver).abi_encode_params(),
    )
    .await?;
    let factory = create(
        &provider,
        artifact("AgentVault.sol", "VaultFactory")?,
        (usdc, escrow).abi_encode_params(),
    )
    .await?;
    Ok(Deployed {
        cfg: ChainConfig {
            rpc_url: url.to_owned(),
            usdc,
            escrow,
            factory,
            explorer_url: None,
        },
    })
}

/// Mints mock USDC (6 decimals) to `to`. MockUSDC's mint is open to anyone.
pub async fn mint(cfg: &ChainConfig, mnemonic: &str, to: Address, amount: u64) -> Result<()> {
    let signer = derive_signer(mnemonic, crate::TEST_OWNER_INDEX)?;
    let provider = crate::signing_provider(EthereumWallet::new(signer), &cfg.rpc_url)?;
    IMockUSDC::new(cfg.usdc, &provider)
        .mint(to, U256::from(amount))
        .send()
        .await
        .map_err(rpc)?
        .get_receipt()
        .await
        .map_err(rpc)?;
    Ok(())
}
