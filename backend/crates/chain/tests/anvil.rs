//! Full escrow + vault lifecycle against a local anvil chain and the real compiled contracts.
//! Skips (with a message) when anvil is not installed.

use aen_chain::testkit::{ANVIL_MNEMONIC, deploy, mint, spawn_anvil};
use aen_chain::{
    Chain, ChainError, EscrowStatus, OwnerWallet, TEST_OWNER_INDEX, agent_ref, challenge_id,
    derive_signer,
};
use uuid::Uuid;

const USDC: u64 = 1_000_000;

#[tokio::test]
async fn escrow_vault_lifecycle_on_anvil() {
    let Some(anvil) = spawn_anvil().await else {
        eprintln!("SKIP: anvil not installed");
        return;
    };
    let d = deploy(&anvil.url, ANVIL_MNEMONIC).await.expect("deploy");
    let chain = Chain::connect(d.cfg.clone(), ANVIL_MNEMONIC).await.expect("connect");
    let owner = OwnerWallet::connect(
        d.cfg.clone(),
        derive_signer(ANVIL_MNEMONIC, TEST_OWNER_INDEX).unwrap(),
    )
    .await
    .unwrap();

    // Owner creates two agent vaults with the backend operator and caps, and funds them.
    let (agent_a, agent_b) = (Uuid::new_v4(), Uuid::new_v4());
    let (va, tx_a) = owner
        .create_vault(agent_a, chain.operator(), 10 * USDC, 15 * USDC)
        .await
        .unwrap();
    let (vb, _) = owner
        .create_vault(agent_b, chain.operator(), 10 * USDC, 15 * USDC)
        .await
        .unwrap();
    let created = chain.vaults_created(tx_a).await.unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].vault, va);
    assert_eq!(created[0].owner, owner.address());
    assert_eq!(created[0].operator, chain.operator());
    assert_eq!(created[0].agent_ref, agent_ref(agent_a));
    assert!(chain.is_vault(va).await.unwrap());

    mint(&d.cfg, ANVIL_MNEMONIC, owner.address(), 100 * USDC).await.unwrap();
    let fund_tx = owner.transfer_usdc(va, 40 * USDC).await.unwrap();
    owner.transfer_usdc(vb, 40 * USDC).await.unwrap();
    let transfers = chain.usdc_transfers(fund_tx).await.unwrap();
    assert_eq!(transfers.len(), 1);
    assert_eq!((transfers[0].from, transfers[0].to, transfers[0].value), (owner.address(), va, 40 * USDC));

    // Resolver registers, operator stakes both vaults, resolver pays A.
    let id = challenge_id(Uuid::new_v4());
    let now = chain.block_timestamp().await.unwrap();
    chain
        .create_challenge(id, va, vb, 5 * USDC, now + 300, now + 900)
        .await
        .unwrap();
    assert_eq!(chain.get_challenge(id).await.unwrap().status, EscrowStatus::Created);
    chain.vault_stake(va, id).await.unwrap();
    chain.vault_stake(vb, id).await.unwrap();
    let c = chain.get_challenge(id).await.unwrap();
    assert_eq!(c.status, EscrowStatus::Active);
    assert!(c.deposited(va) && c.deposited(vb));
    assert_eq!(chain.vault_info(va).await.unwrap().exposure, 5 * USDC);

    chain.resolve(id, va).await.unwrap();
    assert_eq!(chain.get_challenge(id).await.unwrap().status, EscrowStatus::Resolved);
    assert!(chain.vault_release(va, id).await.unwrap().is_some());
    assert!(chain.vault_release(vb, id).await.unwrap().is_some());
    assert!(chain.vault_release(vb, id).await.unwrap().is_none(), "second release is a no-op");
    let (ia, ib) = (chain.vault_info(va).await.unwrap(), chain.vault_info(vb).await.unwrap());
    assert_eq!((ia.balance, ia.exposure), (45 * USDC, 0));
    assert_eq!((ib.balance, ib.exposure), (35 * USDC, 0));

    // Caps are enforced onchain: an 11 USDC challenge is over the 10 USDC per-challenge cap.
    let big = challenge_id(Uuid::new_v4());
    chain
        .create_challenge(big, va, vb, 11 * USDC, now + 300, now + 900)
        .await
        .unwrap();
    assert!(chain.vault_stake(va, big).await.is_err());
    chain.resolve_void(big).await.unwrap();

    // A paused vault refuses to stake; the owner can always withdraw.
    owner.vault_set_paused(va, true).await.unwrap();
    let p = challenge_id(Uuid::new_v4());
    chain
        .create_challenge(p, va, vb, USDC, now + 300, now + 900)
        .await
        .unwrap();
    assert!(chain.vault_stake(va, p).await.is_err());
    let w = owner.vault_withdraw(va, owner.address(), 45 * USDC).await.unwrap();
    let t = chain.usdc_transfers(w).await.unwrap();
    assert_eq!((t[0].from, t[0].value), (va, 45 * USDC));
    assert_eq!(chain.vault_info(va).await.unwrap().balance, 0);
}

#[tokio::test]
async fn refuses_escrow_with_foreign_resolver() {
    let Some(anvil) = spawn_anvil().await else {
        eprintln!("SKIP: anvil not installed");
        return;
    };
    let d = deploy(&anvil.url, ANVIL_MNEMONIC).await.expect("deploy");
    // Same contracts, different mnemonic: our resolver key is not the escrow's resolver.
    let other = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    match Chain::connect(d.cfg, other).await {
        Err(ChainError::Config(msg)) => assert!(msg.contains("resolver"), "{msg}"),
        other => panic!("expected config error, got {other:?}"),
    }
}
