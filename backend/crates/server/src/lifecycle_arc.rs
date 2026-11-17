//! The onchain half of `arc_testnet` challenges.
//!
//! `lifecycle::try_commit` authorizes both stakes and reserves them in the
//! ledger exactly as for simulated funds, then leaves the challenge `Funded`.
//! From there, each tick:
//!
//! 1. **Fund**: reads the escrow, registers the challenge if it is missing
//!    (resolver key), stakes each vault that has not deposited (operator key),
//!    and moves the challenge to `Active` once the escrow says `Active`. A
//!    vault that cannot stake (paused, capped, short of funds, operator
//!    revoked, or the escrow refuses) or a passed funding deadline voids the
//!    challenge: `resolveVoid` onchain if anything is deposited, then both
//!    ledger reservations are refunded and the challenge is cancelled.
//! 2. **Resolve**: decides the outcome from market data once (kept in
//!    `pending_outcome`), calls `resolve(winner vault)` or `resolveVoid`, and
//!    only then settles the ledger with the same posting sets as simulated
//!    challenges. The escrow's final state wins over the decision if they
//!    ever differ (e.g. someone used the timeout refund).
//! 3. **Release**: frees both vaults' onchain exposure for finished challenges.
//!
//! Every step reads the escrow before acting, so a step that failed halfway
//! (RPC error, crash) is simply retried on the next tick and never repeats a
//! transaction or a ledger posting. RPC errors are "retry next tick"; only a
//! revert is treated as the chain refusing. Deadlines are checked against the
//! wall clock and the chain's own clock, whichever is later.

use aen_chain::{ChainError, ChallengeView, EscrowStatus, EvmAddress, VaultInfo, challenge_id};
use aen_core::challenge::Outcome;
use aen_core::interaction::InteractionState;
use aen_core::ledger::{self, Env};
use chrono::Utc;
use serde_json::json;
use sqlx::PgPool;
use std::str::FromStr;
use uuid::Uuid;

use crate::chain::{ChainState, hex, is_revert, money, rpc_failure};
use crate::error::ApiError;
use crate::ledger_db;
use crate::lifecycle::{
    Decided, InteractionRow, TickReport, apply_outcome, decide_outcome, ids, lock_interaction,
    names, record, set_state, stake_text, step,
};
use crate::market;
use crate::models::display_usdc;
use crate::state::AppState;

/// A stake is not attempted this close to the funding deadline: it could be
/// mined after it and revert.
const FUNDING_MARGIN_SECS: u64 = 2;

fn waiting_for_chain(report: &mut TickReport, n: usize) {
    report.errors.push(format!(
        "chain_unavailable: {n} Arc testnet challenge(s) wait for chain access (set \
         AEN_TESTNET_MNEMONIC and the ARC_* settings)"
    ));
}

/// Phase after commit: stakes `Funded` arc_testnet challenges into the escrow.
pub(crate) async fn fund_phase(state: &AppState, report: &mut TickReport) -> Result<(), ApiError> {
    let funded = ids(
        &state.pool,
        "SELECT id FROM interactions WHERE state = 'funded' AND env = 'arc_testnet' \
         ORDER BY created_at, id",
        None,
    )
    .await?;
    if funded.is_empty() {
        return Ok(());
    }
    let Some(cs) = state.chain.as_deref() else {
        waiting_for_chain(report, funded.len());
        return Ok(());
    };
    let _one_at_a_time = cs.tx_lock.lock().await;
    for id in funded {
        let outcome = fund_one(state, cs, id, report).await;
        step(report, outcome);
    }
    Ok(())
}

/// Phase after resolution starts: resolves arc_testnet challenges onchain and
/// settles them, then releases vault exposure of finished ones.
pub(crate) async fn resolve_phase(
    state: &AppState,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let resolving = ids(
        &state.pool,
        "SELECT id FROM interactions WHERE state = 'resolving' AND env = 'arc_testnet' \
         ORDER BY created_at, id",
        None,
    )
    .await?;
    // Finished challenges that reached funding and whose vaults still carry
    // the stake as exposure.
    let to_release = ids(
        &state.pool,
        "SELECT i.id FROM interactions i WHERE i.env = 'arc_testnet' \
           AND i.state IN ('settled', 'cancelled') AND i.vaults_released_at IS NULL \
           AND EXISTS (SELECT 1 FROM ledger_entries e \
                       WHERE e.interaction_id = i.id AND e.reason = 'commit_stake') \
         ORDER BY i.created_at, i.id",
        None,
    )
    .await?;
    if resolving.is_empty() && to_release.is_empty() {
        return Ok(());
    }
    let Some(cs) = state.chain.as_deref() else {
        waiting_for_chain(report, resolving.len() + to_release.len());
        return Ok(());
    };
    let _one_at_a_time = cs.tx_lock.lock().await;
    for id in resolving {
        let outcome = resolve_one(state, cs, id, report).await;
        step(report, outcome);
    }
    // Includes what was settled or voided just above.
    let to_release = ids(
        &state.pool,
        "SELECT i.id FROM interactions i WHERE i.env = 'arc_testnet' \
           AND i.state IN ('settled', 'cancelled') AND i.vaults_released_at IS NULL \
           AND EXISTS (SELECT 1 FROM ledger_entries e \
                       WHERE e.interaction_id = i.id AND e.reason = 'commit_stake') \
         ORDER BY i.created_at, i.id",
        None,
    )
    .await?;
    for id in to_release {
        let outcome = release_one(state, cs, id).await;
        step(report, outcome);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

async fn read_row(pool: &PgPool, id: Uuid) -> Result<Option<InteractionRow>, ApiError> {
    Ok(sqlx::query_as("SELECT * FROM interactions WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?)
}

/// The vaults of (proposer, counterparty), if both are linked.
async fn vaults(
    pool: &PgPool,
    row: &InteractionRow,
) -> Result<Option<(EvmAddress, EvmAddress)>, ApiError> {
    let rows: Vec<(Uuid, Option<String>)> =
        sqlx::query_as("SELECT id, vault_address FROM agents WHERE id = ANY($1)")
            .bind(vec![row.proposer_id, row.counterparty_id])
            .fetch_all(pool)
            .await?;
    let of = |agent: Uuid| -> Result<Option<EvmAddress>, ApiError> {
        rows.iter()
            .find(|(id, _)| *id == agent)
            .and_then(|(_, v)| v.as_deref())
            .map(|v| {
                EvmAddress::from_str(v)
                    .map_err(|_| ApiError::internal("stored vault address is unreadable"))
            })
            .transpose()
    };
    Ok(match (of(row.proposer_id)?, of(row.counterparty_id)?) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    })
}

fn stake_units(row: &InteractionRow) -> Result<u64, ApiError> {
    u64::try_from(row.stake).map_err(|_| ApiError::internal("negative stake"))
}

/// Stores a confirmed transaction hash. Columns are a fixed set.
async fn save_tx(
    pool: &PgPool,
    id: Uuid,
    column: &'static str,
    tx: aen_chain::TxHash,
) -> Result<(), ApiError> {
    let sql = match column {
        "escrow_create_tx" => "UPDATE interactions SET escrow_create_tx = $2 WHERE id = $1",
        "proposer_stake_tx" => "UPDATE interactions SET proposer_stake_tx = $2 WHERE id = $1",
        "counterparty_stake_tx" => {
            "UPDATE interactions SET counterparty_stake_tx = $2 WHERE id = $1"
        }
        "escrow_settle_tx" => "UPDATE interactions SET escrow_settle_tx = $2 WHERE id = $1",
        _ => return Err(ApiError::internal("unknown tx column")),
    };
    sqlx::query(sql)
        .bind(id)
        .bind(hex(tx))
        .execute(pool)
        .await?;
    Ok(())
}

async fn get_challenge(cs: &ChainState, id: Uuid) -> Result<ChallengeView, ApiError> {
    cs.chain
        .get_challenge(challenge_id(id))
        .await
        .map_err(|e| rpc_failure(&e))
}

/// The later of the wall clock and the chain's clock, in unix seconds.
async fn now_secs(cs: &ChainState) -> Result<u64, ApiError> {
    let chain = cs
        .chain
        .block_timestamp()
        .await
        .map_err(|e| rpc_failure(&e))?;
    let wall = u64::try_from(Utc::now().timestamp()).unwrap_or_default();
    Ok(chain.max(wall))
}

/// Why `vault` cannot stake `stake` right now, judged from its onchain state.
fn vault_problem(info: &VaultInfo, operator: EvmAddress, stake: u64) -> Option<String> {
    let usdc = |units: u64| {
        money(units)
            .map(display_usdc)
            .unwrap_or_else(|_| format!("{units} units"))
    };
    if info.paused {
        Some("is paused by its owner".into())
    } else if info.operator != operator {
        Some("no longer lets this backend operate it".into())
    } else if stake > info.max_per_challenge {
        Some(format!(
            "caps one challenge at {} onchain",
            usdc(info.max_per_challenge)
        ))
    } else if info.exposure.saturating_add(stake) > info.max_exposure {
        Some(format!(
            "would exceed its onchain exposure cap of {}",
            usdc(info.max_exposure)
        ))
    } else if info.balance < stake {
        Some(format!("holds only {}", usdc(info.balance)))
    } else {
        None
    }
}

fn revert_text(e: &ChainError) -> &'static str {
    match e {
        ChainError::Reverted { .. } => "the transaction reverted",
        _ => "the contract refused the call",
    }
}

// ---------------------------------------------------------------------------
// Fund
// ---------------------------------------------------------------------------

async fn fund_one(
    state: &AppState,
    cs: &ChainState,
    id: Uuid,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let pool = &state.pool;
    let chain = &cs.chain;
    let Some(row) = read_row(pool, id).await? else {
        return Ok(());
    };
    if row.state()? != InteractionState::Funded || row.env()? != Env::ArcTestnet {
        return Ok(());
    }
    let mut c = get_challenge(cs, id).await?;
    let Some((va, vb)) = vaults(pool, &row).await? else {
        return void_funding(
            state,
            cs,
            &row,
            c.status,
            "an agent no longer has a vault",
            report,
        )
        .await;
    };
    let cid = challenge_id(id);
    let stake = stake_units(&row)?;
    let deadline = u64::try_from(row.start_ts.timestamp()).unwrap_or_default();
    let late = |now: u64| now + FUNDING_MARGIN_SECS >= deadline;
    let (a_name, b_name) = {
        let mut conn = pool.acquire().await?;
        names(&mut conn, &row).await?
    };

    if c.status == EscrowStatus::None {
        if late(now_secs(cs).await?) {
            return void_funding(
                state,
                cs,
                &row,
                c.status,
                "the funding deadline passed before the escrow was set up",
                report,
            )
            .await;
        }
        // The escrow refunds anyone after resolveDeadline + 1 h if the backend
        // never resolves; give the backend its own grace period first.
        let resolve_deadline =
            u64::try_from(row.end_ts.timestamp() + state.settings.resolution_grace_secs.max(0))
                .unwrap_or_default()
                .max(deadline + 1);
        match chain
            .create_challenge(cid, va, vb, stake, deadline, resolve_deadline)
            .await
        {
            Ok(tx) => {
                save_tx(pool, id, "escrow_create_tx", tx).await?;
                let mut conn = pool.acquire().await?;
                record(
                    &mut conn,
                    Some(row.proposer_id),
                    Some(id),
                    "escrow_created",
                    format!(
                        "The challenge between \"{a_name}\" and \"{b_name}\" was registered in \
                         the Arc testnet escrow: {} per side, funding open until {}.",
                        stake_text(row.stake()?, Env::ArcTestnet),
                        row.start_ts.format("%Y-%m-%d %H:%M:%S UTC")
                    ),
                    json!({
                        "env": Env::ArcTestnet, "tx_hash": hex(tx),
                        "explorer_url": chain.explorer_tx(tx),
                        "escrow": chain.config().escrow.to_string(),
                    }),
                )
                .await?;
            }
            Err(e) if is_revert(&e) => {
                tracing::warn!(error = %e, %id, "escrow create reverted");
                let reason = format!("the escrow refused the challenge ({})", revert_text(&e));
                return void_funding(state, cs, &row, EscrowStatus::None, &reason, report).await;
            }
            Err(e) => return Err(rpc_failure(&e)),
        }
        c = get_challenge(cs, id).await?;
    }

    if c.status == EscrowStatus::Created {
        if (c.party_a, c.party_b, c.stake) != (va, vb, stake) {
            return void_funding(
                state,
                cs,
                &row,
                c.status,
                "the escrow holds different terms than this challenge",
                report,
            )
            .await;
        }
        for (vault, agent, name, column) in [
            (va, row.proposer_id, &a_name, "proposer_stake_tx"),
            (vb, row.counterparty_id, &b_name, "counterparty_stake_tx"),
        ] {
            if c.deposited(vault) {
                continue;
            }
            if late(now_secs(cs).await?) {
                return void_funding(
                    state,
                    cs,
                    &row,
                    c.status,
                    "the funding deadline passed before both vaults staked",
                    report,
                )
                .await;
            }
            let info = chain.vault_info(vault).await.map_err(|e| rpc_failure(&e))?;
            if let Some(problem) = vault_problem(&info, chain.operator(), stake) {
                let reason = format!("\"{name}\"'s vault {problem}");
                return void_funding(state, cs, &row, c.status, &reason, report).await;
            }
            match chain.vault_stake(vault, cid).await {
                Ok(tx) => {
                    save_tx(pool, id, column, tx).await?;
                    let mut conn = pool.acquire().await?;
                    record(
                        &mut conn,
                        Some(agent),
                        Some(id),
                        "stake_deposited",
                        format!(
                            "\"{name}\"'s vault staked {} into the Arc testnet escrow.",
                            stake_text(row.stake()?, Env::ArcTestnet)
                        ),
                        json!({
                            "env": Env::ArcTestnet, "tx_hash": hex(tx), "vault": vault.to_string(),
                            "explorer_url": chain.explorer_tx(tx),
                        }),
                    )
                    .await?;
                }
                Err(e) if is_revert(&e) => {
                    tracing::warn!(error = %e, %id, "vault stake reverted");
                    let reason = format!(
                        "\"{name}\"'s vault rejected the stake ({})",
                        revert_text(&e)
                    );
                    return void_funding(state, cs, &row, c.status, &reason, report).await;
                }
                Err(e) => return Err(rpc_failure(&e)),
            }
        }
        c = get_challenge(cs, id).await?;
    }

    match c.status {
        EscrowStatus::Active => activate(pool, &row, (&a_name, &b_name), report).await,
        EscrowStatus::Voided | EscrowStatus::Refunded => {
            void_funding(
                state,
                cs,
                &row,
                c.status,
                "the escrow returned the stakes onchain",
                report,
            )
            .await
        }
        // Still waiting for a deposit (a retry follows next tick).
        EscrowStatus::None | EscrowStatus::Created => Ok(()),
        EscrowStatus::Resolved => Err(ApiError::internal(format!(
            "escrow challenge {id} is resolved but the challenge was never active"
        ))),
    }
}

/// Both stakes are in the escrow: the challenge is live.
async fn activate(
    pool: &PgPool,
    seen: &InteractionRow,
    (a_name, b_name): (&String, &String),
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let Some(mut row) = lock_interaction(&mut tx, seen.id).await? else {
        return Ok(());
    };
    if row.state()? != InteractionState::Funded {
        return Ok(());
    }
    set_state(&mut tx, &mut row, InteractionState::Active, None, now).await?;
    let terms = row.terms()?;
    let stake = row.stake()?;
    for (agent, name, party) in [
        (row.proposer_id, a_name, aen_core::challenge::Party::A),
        (row.counterparty_id, b_name, aen_core::challenge::Party::B),
    ] {
        record(
            &mut tx,
            Some(agent),
            Some(row.id),
            "stake_committed",
            format!(
                "\"{name}\" committed {} to its challenge (locked in the Arc testnet escrow) and \
                 holds \"{} {}\". The challenge is active until {}.",
                stake_text(stake, Env::ArcTestnet),
                crate::lifecycle::side_phrase(terms.side_of(party)),
                crate::lifecycle::format_value(terms.formula, terms.threshold),
                row.end_ts.format("%Y-%m-%d %H:%M UTC")
            ),
            json!({ "stake": stake, "env": Env::ArcTestnet, "side": terms.side_of(party) }),
        )
        .await?;
    }
    tx.commit().await?;
    report.activated += 1;
    Ok(())
}

/// Voids a `Funded` challenge: returns any deposit onchain first, then
/// refunds both ledger reservations and cancels. `status` is the escrow
/// status just read.
async fn void_funding(
    state: &AppState,
    cs: &ChainState,
    seen: &InteractionRow,
    status: EscrowStatus,
    reason: &str,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let pool = &state.pool;
    let id = seen.id;
    let mut void_tx = seen.escrow_settle_tx.clone();
    if matches!(status, EscrowStatus::Created | EscrowStatus::Active) {
        match cs.chain.resolve_void(challenge_id(id)).await {
            Ok(tx) => {
                save_tx(pool, id, "escrow_settle_tx", tx).await?;
                void_tx = Some(hex(tx));
            }
            // A revert means the status moved on (e.g. already void): read it.
            Err(e) if is_revert(&e) => {
                if !get_challenge(cs, id).await?.status.is_terminal() {
                    return Err(ApiError::internal(format!(
                        "resolveVoid({id}) reverted: {e}"
                    )));
                }
            }
            Err(e) => return Err(rpc_failure(&e)),
        }
    }

    let now = Utc::now();
    let mut tx = pool.begin().await?;
    let Some(mut row) = lock_interaction(&mut tx, id).await? else {
        return Ok(());
    };
    if row.state()? != InteractionState::Funded {
        return Ok(());
    }
    let (a_name, b_name) = names(&mut tx, &row).await?;
    let stake = row.stake()?;
    for agent in [row.proposer_id, row.counterparty_id] {
        let set = ledger::refund(Env::ArcTestnet, agent, id, stake)?;
        let set = match &void_tx {
            Some(h) => set.with_tx_hash(h),
            None => set,
        };
        ledger_db::post(&mut tx, &set).await?;
    }
    sqlx::query("UPDATE interactions SET outcome = 'void', turn = NULL WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let text = format!("Voided before it started: {reason}. Both reserved stakes were returned.");
    set_state(
        &mut tx,
        &mut row,
        InteractionState::Cancelled,
        Some(&text),
        now,
    )
    .await?;
    for (agent, name) in [(row.proposer_id, &a_name), (row.counterparty_id, &b_name)] {
        record(
            &mut tx,
            Some(agent),
            Some(id),
            "challenge_voided",
            format!(
                "The challenge between \"{a_name}\" and \"{b_name}\" was voided before it \
                 started: {reason}. {} was returned to \"{name}\".",
                stake_text(stake, Env::ArcTestnet)
            ),
            json!({
                "refund": stake, "env": Env::ArcTestnet, "tx_hash": void_tx,
                "explorer_url": void_tx.as_deref().and_then(|h| {
                    aen_chain::TxHash::from_str(h).ok().and_then(|t| cs.chain.explorer_tx(t))
                }),
            }),
        )
        .await?;
    }
    tx.commit().await?;
    report.voided += 1;
    Ok(())
}

// ---------------------------------------------------------------------------
// Resolve
// ---------------------------------------------------------------------------

async fn resolve_one(
    state: &AppState,
    cs: &ChainState,
    id: Uuid,
    report: &mut TickReport,
) -> Result<(), ApiError> {
    let pool = &state.pool;
    let chain = &cs.chain;
    let Some(row) = read_row(pool, id).await? else {
        return Ok(());
    };
    if row.state()? != InteractionState::Resolving || row.env()? != Env::ArcTestnet {
        return Ok(());
    }
    let names = {
        let mut conn = pool.acquire().await?;
        names(&mut conn, &row).await?
    };

    // 1. Decide once from market data; a retry reuses the stored decision.
    let mut decided: Decided = match &row.pending_outcome {
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| ApiError::internal(format!("stored outcome is unreadable: {e}")))?,
        None => {
            let terms = row.terms()?;
            let observation =
                market::observe(state.market.as_ref(), &terms.market, terms.formula).await;
            if let Err(e) = &observation {
                report.errors.push(format!("market data: {e}"));
            }
            let mut tx = pool.begin().await?;
            let Some(locked) = lock_interaction(&mut tx, id).await? else {
                return Ok(());
            };
            if locked.state()? != InteractionState::Resolving || locked.pending_outcome.is_some() {
                return Ok(());
            }
            let decision = decide_outcome(
                &mut tx,
                &state.settings,
                &locked,
                &observation,
                &names,
                Utc::now(),
            )
            .await?;
            let Some(decided) = decision else {
                tx.commit().await?;
                return Ok(());
            };
            sqlx::query("UPDATE interactions SET pending_outcome = $2 WHERE id = $1")
                .bind(id)
                .bind(serde_json::to_value(&decided).map_err(ApiError::internal)?)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            decided
        }
    };

    // 2. Move the funds onchain (or find they already moved).
    let Some((va, vb)) = vaults(pool, &row).await? else {
        return Err(ApiError::internal(format!(
            "challenge {id} is resolving but an agent has no vault"
        )));
    };
    let cid = challenge_id(id);
    let c = get_challenge(cs, id).await?;
    let mut settle_tx = row.escrow_settle_tx.clone();
    match c.status {
        EscrowStatus::Active => {
            let sent = match decided.outcome {
                Outcome::A => chain.resolve(cid, va).await,
                Outcome::B => chain.resolve(cid, vb).await,
                Outcome::Void => chain.resolve_void(cid).await,
            };
            // Any failure (RPC or revert) is retried next tick, which reads
            // the escrow again first.
            let tx = sent.map_err(|e| rpc_failure(&e))?;
            save_tx(pool, id, "escrow_settle_tx", tx).await?;
            settle_tx = Some(hex(tx));
        }
        EscrowStatus::Resolved => {
            // The escrow is final: the ledger follows it.
            let onchain = if c.winner == va {
                Outcome::A
            } else if c.winner == vb {
                Outcome::B
            } else {
                return Err(ApiError::internal(format!(
                    "escrow challenge {id} was paid to an unknown winner"
                )));
            };
            if onchain != decided.outcome {
                tracing::error!(%id, ?onchain, decided = ?decided.outcome, "escrow winner differs");
                decided.outcome = onchain;
                decided.void_reason.clear();
            }
        }
        EscrowStatus::Voided | EscrowStatus::Refunded => {
            if decided.outcome != Outcome::Void {
                decided.outcome = Outcome::Void;
                decided.void_reason = "the escrow returned both stakes onchain".into();
            }
        }
        EscrowStatus::None | EscrowStatus::Created => {
            return Err(ApiError::internal(format!(
                "escrow challenge {id} is not active although the challenge is resolving"
            )));
        }
    }

    // 3. Settle the ledger with the same posting sets as simulated funds.
    let mut tx = pool.begin().await?;
    let Some(mut locked) = lock_interaction(&mut tx, id).await? else {
        return Ok(());
    };
    if locked.state()? != InteractionState::Resolving {
        return Ok(());
    }
    let settled = apply_outcome(
        &mut tx,
        &mut locked,
        &decided,
        &names,
        settle_tx.as_deref(),
        Utc::now(),
    )
    .await?;
    tx.commit().await?;
    match settled {
        Some(Outcome::Void) => report.voided += 1,
        Some(_) => report.settled += 1,
        None => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

async fn release_one(state: &AppState, cs: &ChainState, id: Uuid) -> Result<(), ApiError> {
    let pool = &state.pool;
    let Some(row) = read_row(pool, id).await? else {
        return Ok(());
    };
    if row.vaults_released_at.is_some() {
        return Ok(());
    }
    let status = get_challenge(cs, id).await?.status;
    // `release` reverts until the escrow is final. A challenge cancelled
    // before the escrow existed has nothing staked (release is a no-op).
    if !status.is_terminal() && status != EscrowStatus::None {
        return Err(ApiError::internal(format!(
            "challenge {id} is closed but its escrow is still {status:?}"
        )));
    }
    if let Some((va, vb)) = vaults(pool, &row).await? {
        for vault in [va, vb] {
            cs.chain
                .vault_release(vault, challenge_id(id))
                .await
                .map_err(|e| rpc_failure(&e))?;
        }
    }
    sqlx::query("UPDATE interactions SET vaults_released_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
