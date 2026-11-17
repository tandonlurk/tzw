//! Arc testnet smoke run: one full create -> stake -> resolve -> release -> withdraw lifecycle with
//! real transactions against the deployed `ChallengeEscrow` and `VaultFactory` (D-60), plus one
//! `resolveVoid` path. Proves `aen-chain` works on Arc, not only on anvil.
//!
//! Spends real testnet USDC (gas is paid in USDC on Arc): about 0.1-0.2 USDC per run. Vault funds
//! (2 x 0.5 USDC) go back to the test owner at the end.
//!
//! Run from `backend/`:
//!     AEN_ENV_FILE=/path/to/backend/.env cargo run -p aen-chain --example arc_smoke
//! The mnemonic comes from `AEN_TESTNET_MNEMONIC` in the environment, or else from the
//! `AEN_TESTNET_MNEMONIC=` line of `AEN_ENV_FILE` (default `backend/.env`). It is never printed.
//! Optional overrides: `ARC_RPC_URL`, `ARC_ESCROW_ADDRESS`, `ARC_VAULT_FACTORY_ADDRESS`.
//! Resume after an interrupted run (reuses the vaults, tops them up, ends by withdrawing them):
//! `SMOKE_VAULT_A=0x.. SMOKE_VAULT_B=0x..`.
//! Refuses to run on any chain but Arc testnet.

use std::time::Instant;

use aen_chain::{
    ARC_TESTNET_CHAIN_ID, ARC_TESTNET_EXPLORER, ARC_TESTNET_RPC, ARC_TESTNET_USDC, Chain,
    ChainConfig, EscrowStatus, EvmAddress, OwnerWallet, TEST_OWNER_INDEX, TxHash, agent_ref,
    challenge_id, derive_signer,
};
use alloy::primitives::B256;
use uuid::Uuid;

/// D-60 deployment.
const ESCROW: &str = "0x4179f2e314410D10C2906B1914bAAe8DD93b5698";
const FACTORY: &str = "0x4EA6ac6A9F7b8fcf01ba04b177F0A098d9664C45";

const MICRO: u64 = 1_000_000; // 1 USDC in 6-decimal ERC-20 units
const FUND: u64 = MICRO / 2; // 0.5 USDC into each vault
const STAKE: u64 = MICRO / 10; // 0.1 USDC per challenge
const MAX_PER_CHALLENGE: u64 = MICRO / 5; // 0.2 USDC
const MAX_EXPOSURE: u64 = MICRO * 3 / 10; // 0.3 USDC

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn usdc(v: u64) -> String {
    format!("{}.{:06} USDC", v / MICRO, v % MICRO)
}

fn mnemonic() -> Res<String> {
    if let Ok(m) = std::env::var("AEN_TESTNET_MNEMONIC") {
        return Ok(m);
    }
    let path = std::env::var("AEN_ENV_FILE")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env").to_string());
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("AEN_TESTNET_MNEMONIC not set and cannot read {path}: {e}"))?;
    text.lines()
        .find_map(|l| l.trim().strip_prefix("AEN_TESTNET_MNEMONIC="))
        .map(|v| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("no AEN_TESTNET_MNEMONIC= line in {path}").into())
}

fn env_addr(key: &str, default: &str) -> Res<EvmAddress> {
    Ok(std::env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .parse()?)
}

struct Log {
    txs: Vec<(String, TxHash)>,
    explorer: String,
    started: Instant,
}

impl Log {
    fn tx(&mut self, what: &str, tx: TxHash) {
        println!(
            "  [{:>5.1}s] {what}: {tx}\n           {}/tx/{tx}",
            self.started.elapsed().as_secs_f32(),
            self.explorer
        );
        self.txs.push((what.to_string(), tx));
    }
}

async fn show_vault(chain: &Chain, label: &str, vault: EvmAddress) -> Res<()> {
    let v = chain.vault_info(vault).await?;
    println!(
        "  vault {label} {vault}: balance {}, exposure {}, caps {}/{}, paused {}",
        usdc(v.balance),
        usdc(v.exposure),
        usdc(v.max_per_challenge),
        usdc(v.max_exposure),
        v.paused
    );
    Ok(())
}

async fn show_challenge(chain: &Chain, id: B256) -> Res<EscrowStatus> {
    let c = chain.get_challenge(id).await?;
    println!(
        "  escrow {id}: {:?}, deposited A={} B={}, stake {}, winner {}",
        c.status,
        c.deposited_a,
        c.deposited_b,
        usdc(c.stake),
        c.winner
    );
    Ok(c.status)
}

fn expect<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> Res<()> {
    if got != want {
        return Err(format!("{what}: expected {want:?}, got {got:?}").into());
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Res<()> {
    let cfg = ChainConfig {
        rpc_url: std::env::var("ARC_RPC_URL").unwrap_or_else(|_| ARC_TESTNET_RPC.into()),
        usdc: ARC_TESTNET_USDC.parse()?,
        escrow: env_addr("ARC_ESCROW_ADDRESS", ESCROW)?,
        factory: env_addr("ARC_VAULT_FACTORY_ADDRESS", FACTORY)?,
        explorer_url: Some(ARC_TESTNET_EXPLORER.into()),
    };
    let mnemonic = mnemonic()?;

    // Connect checks the chain id, that escrow.RESOLVER() is our idx 0 and factory.ESCROW() is the escrow.
    let chain = Chain::connect(cfg.clone(), &mnemonic).await?;
    if chain.chain_id() != ARC_TESTNET_CHAIN_ID {
        return Err(format!("not Arc testnet: chain id {}", chain.chain_id()).into());
    }
    let owner =
        OwnerWallet::connect(cfg.clone(), derive_signer(&mnemonic, TEST_OWNER_INDEX)?).await?;
    drop(mnemonic);

    let mut log = Log {
        txs: Vec::new(),
        explorer: ARC_TESTNET_EXPLORER.into(),
        started: Instant::now(),
    };
    let accounts = [
        ("resolver (idx 0)", chain.resolver()),
        ("operator (idx 1)", chain.operator()),
        ("owner    (idx 2)", owner.address()),
    ];
    let mut before = Vec::new();
    println!("Arc testnet {} via {}", chain.chain_id(), cfg.rpc_url);
    println!(
        "escrow {} / factory {} / usdc {}",
        cfg.escrow, cfg.factory, cfg.usdc
    );
    for (name, a) in accounts {
        let b = chain.usdc_balance(a).await?;
        println!("  {name} {a}: {}", usdc(b));
        before.push(b);
    }
    if before[2] < 2 * FUND + MICRO / 2 {
        return Err("test owner needs at least 1.5 USDC".into());
    }

    let (va, vb) = match (
        std::env::var("SMOKE_VAULT_A"),
        std::env::var("SMOKE_VAULT_B"),
    ) {
        (Ok(a), Ok(b)) => {
            // Resume mode: reuse vaults from an earlier (interrupted) run instead of creating new ones.
            let (va, vb): (EvmAddress, EvmAddress) = (a.parse()?, b.parse()?);
            println!("\n1. reusing vaults A {va} and B {vb}");
            for v in [va, vb] {
                let i = chain.vault_info(v).await?;
                expect("isVault", chain.is_vault(v).await?, true)?;
                expect("vault owner", i.owner, owner.address())?;
                expect("vault operator", i.operator, chain.operator())?;
                expect("vault exposure", i.exposure, 0)?;
            }
            (va, vb)
        }
        _ => {
            println!("\n1. owner creates two vaults (operator = idx 1)");
            let (agent_a, agent_b) = (Uuid::new_v4(), Uuid::new_v4());
            let (va, tx) = owner
                .create_vault(agent_a, chain.operator(), MAX_PER_CHALLENGE, MAX_EXPOSURE)
                .await?;
            log.tx("createVault A", tx);
            let ev = chain.vaults_created(tx).await?;
            expect("VaultCreated events", ev.len(), 1)?;
            expect("VaultCreated.owner", ev[0].owner, owner.address())?;
            expect("VaultCreated.operator", ev[0].operator, chain.operator())?;
            expect("VaultCreated.agentRef", ev[0].agent_ref, agent_ref(agent_a))?;
            let (vb, tx) = owner
                .create_vault(agent_b, chain.operator(), MAX_PER_CHALLENGE, MAX_EXPOSURE)
                .await?;
            log.tx("createVault B", tx);
            expect("isVault(A)", chain.is_vault(va).await?, true)?;
            expect("isVault(B)", chain.is_vault(vb).await?, true)?;
            (va, vb)
        }
    };

    println!("\n2. owner funds each vault up to {}", usdc(FUND));
    for (label, v) in [("A", va), ("B", vb)] {
        let bal = chain.usdc_balance(v).await?;
        if bal >= FUND {
            println!("  vault {label} already holds {}", usdc(bal));
            continue;
        }
        let tx = owner.transfer_usdc(v, FUND - bal).await?;
        log.tx(&format!("fund {label}"), tx);
        let t = chain.usdc_transfers(tx).await?;
        let t: Vec<_> = t.iter().map(|t| (t.from, t.to, t.value)).collect();
        expect(
            "fund transfer log",
            t,
            vec![(owner.address(), v, FUND - bal)],
        )?;
    }
    show_vault(&chain, "A", va).await?;
    show_vault(&chain, "B", vb).await?;
    let (a0, b0) = (chain.usdc_balance(va).await?, chain.usdc_balance(vb).await?);

    println!(
        "\n3. resolver creates challenge 1 (A vs B, stake {})",
        usdc(STAKE)
    );
    let id1 = challenge_id(Uuid::new_v4());
    let now = chain.block_timestamp().await?;
    log.tx(
        "escrow.create #1",
        chain
            .create_challenge(id1, va, vb, STAKE, now + 300, now + 600)
            .await?,
    );
    expect(
        "status",
        show_challenge(&chain, id1).await?,
        EscrowStatus::Created,
    )?;

    println!("\n4. operator stakes from both vaults");
    log.tx("vault A stake #1", chain.vault_stake(va, id1).await?);
    expect(
        "status after A",
        show_challenge(&chain, id1).await?,
        EscrowStatus::Created,
    )?;
    log.tx("vault B stake #1", chain.vault_stake(vb, id1).await?);
    expect(
        "status after B",
        show_challenge(&chain, id1).await?,
        EscrowStatus::Active,
    )?;
    show_vault(&chain, "A", va).await?;
    show_vault(&chain, "B", vb).await?;
    expect(
        "escrow holds stakes",
        chain.usdc_balance(cfg.escrow).await? >= 2 * STAKE,
        true,
    )?;

    println!("\n5. resolver resolves challenge 1 to vault A");
    log.tx("escrow.resolve #1 -> A", chain.resolve(id1, va).await?);
    expect(
        "status",
        show_challenge(&chain, id1).await?,
        EscrowStatus::Resolved,
    )?;

    println!("\n6. operator releases both vaults' exposure");
    log.tx(
        "vault A release #1",
        chain
            .vault_release(va, id1)
            .await?
            .ok_or("A had nothing staked")?,
    );
    log.tx(
        "vault B release #1",
        chain
            .vault_release(vb, id1)
            .await?
            .ok_or("B had nothing staked")?,
    );
    expect(
        "second release is a no-op",
        chain.vault_release(vb, id1).await?,
        None,
    )?;
    show_vault(&chain, "A", va).await?;
    show_vault(&chain, "B", vb).await?;
    let (ia, ib) = (chain.vault_info(va).await?, chain.vault_info(vb).await?);
    expect("A after win", (ia.balance, ia.exposure), (a0 + STAKE, 0))?;
    expect("B after loss", (ib.balance, ib.exposure), (b0 - STAKE, 0))?;

    println!("\n7. void path: challenge 2, only A stakes, resolver voids, A is refunded");
    let id2 = challenge_id(Uuid::new_v4());
    let now = chain.block_timestamp().await?;
    log.tx(
        "escrow.create #2",
        chain
            .create_challenge(id2, va, vb, STAKE, now + 300, now + 600)
            .await?,
    );
    log.tx("vault A stake #2", chain.vault_stake(va, id2).await?);
    show_challenge(&chain, id2).await?;
    show_vault(&chain, "A", va).await?;
    log.tx("escrow.resolveVoid #2", chain.resolve_void(id2).await?);
    expect(
        "status",
        show_challenge(&chain, id2).await?,
        EscrowStatus::Voided,
    )?;
    log.tx(
        "vault A release #2",
        chain
            .vault_release(va, id2)
            .await?
            .ok_or("A had nothing staked")?,
    );
    let ia = chain.vault_info(va).await?;
    expect("A after void", (ia.balance, ia.exposure), (a0 + STAKE, 0))?;
    show_vault(&chain, "A", va).await?;

    println!("\n8. owner withdraws everything back to idx 2");
    for (label, v) in [("A", va), ("B", vb)] {
        let bal = chain.vault_info(v).await?.balance;
        log.tx(
            &format!("vault {label} withdraw {}", usdc(bal)),
            owner.vault_withdraw(v, owner.address(), bal).await?,
        );
        expect(
            &format!("vault {label} empty"),
            chain.usdc_balance(v).await?,
            0,
        )?;
    }

    println!(
        "\nDone in {:.1}s, {} transactions.",
        log.started.elapsed().as_secs_f32(),
        log.txs.len()
    );
    println!("Balance change per account (gas + vault flows; USDC, 6-decimal ERC-20 view):");
    for ((name, a), b0) in accounts.iter().zip(&before) {
        let b1 = chain.usdc_balance(*a).await?;
        let (sign, d) = if b1 >= *b0 {
            ("+", b1 - b0)
        } else {
            ("-", b0 - b1)
        };
        println!(
            "  {name}: {} -> {} ({sign}{})",
            usdc(*b0),
            usdc(b1),
            usdc(d)
        );
    }
    println!("vaults: A {va}  B {vb}");
    println!("challenges: #1 {id1}  #2 {id2}");
    println!("\n| step | tx |\n|---|---|");
    for (what, tx) in &log.txs {
        println!("| {what} | [{tx}]({}/tx/{tx}) |", log.explorer);
    }
    Ok(())
}
