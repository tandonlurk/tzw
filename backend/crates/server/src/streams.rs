//! QuickNode Streams as an independent confirmation of the Arc testnet
//! lifecycle (D-74).
//!
//! The webhook (`quicknode.rs`) stores every log once. Logs from our
//! `ChallengeEscrow` and `VaultFactory` are then decoded here, matched to the
//! challenge (escrow id = interaction id) or agent (`agentRef` = agent id)
//! they belong to, and compared with what the server itself confirmed from
//! its own transaction receipts:
//!
//! - same transaction and values: `confirmed`, shown as "confirmed by
//!   QuickNode Streams" with the block and the block-to-delivery latency;
//! - the server has not recorded that step yet: `pending`, rechecked after
//!   every confirmed transaction and on every tick;
//! - different transaction, parties, amount or winner: `mismatch`, flagged
//!   in activity and the log;
//! - a real event the server did not cause (timeout refund, deferred payout,
//!   claim): `observed`;
//! - no challenge or agent here has that id: `unmatched`.
//!
//! A stream event never moves interaction state and never credits or debits
//! the ledger: the server's receipt reads and escrow reads stay the source of
//! truth, and the escrow is re-read before every step anyway.

use aen_chain::events::{self, ContractEvent, Source};
use aen_core::ledger::Env;
use aen_core::money::Money;
use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::error::ApiError;
use crate::lifecycle::{record, stake_text};
use crate::state::AppState;

/// Contracts whose logs are decoded (lowercase addresses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamContracts {
    pub escrow: String,
    pub factory: String,
    pub explorer_url: Option<String>,
}

/// Pending events are rechecked for this long after delivery.
const RECHECK_WINDOW_HOURS: i64 = 24;
const RECHECK_BATCH: i64 = 200;

/// The configured override (tests), else the connected chain's contracts.
pub fn contracts(state: &AppState) -> Option<StreamContracts> {
    if let Some(c) = state
        .settings
        .quicknode
        .as_ref()
        .and_then(|q| q.contracts.clone())
    {
        return Some(c);
    }
    let cfg = state.chain.as_deref()?.chain.config();
    Some(StreamContracts {
        escrow: format!("{:#x}", cfg.escrow),
        factory: format!("{:#x}", cfg.factory),
        explorer_url: cfg.explorer_url.clone(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Confirmed,
    Mismatch,
    Observed,
    Unmatched,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Confirmed => "confirmed",
            Status::Mismatch => "mismatch",
            Status::Observed => "observed",
            Status::Unmatched => "unmatched",
        }
    }
}

/// What the server knows about the challenge an escrow event names.
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct Known {
    pub id: Uuid,
    pub stake: i64,
    pub proposer_id: Uuid,
    pub counterparty_id: Uuid,
    pub proposer_name: String,
    pub counterparty_name: String,
    pub proposer_vault: Option<String>,
    pub counterparty_vault: Option<String>,
    pub escrow_create_tx: Option<String>,
    pub proposer_stake_tx: Option<String>,
    pub counterparty_stake_tx: Option<String>,
    pub escrow_settle_tx: Option<String>,
    pub winner_id: Option<Uuid>,
    pub outcome: Option<String>,
}

/// What the server knows about the agent a `VaultCreated` names.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct KnownAgent {
    pub id: Uuid,
    pub name: String,
    pub vault_address: Option<String>,
}

fn same(a: &Option<String>, b: &str) -> bool {
    a.as_deref() == Some(b)
}

/// Compares the server's own transaction for a step with the event's.
fn tx_check(server: &Option<String>, tx: &str, step: &str) -> (Status, String) {
    match server {
        None => (
            Status::Pending,
            format!("the server has not confirmed the {step} transaction yet"),
        ),
        Some(s) if s == tx => (
            Status::Confirmed,
            format!("matches the server's verified {step} receipt"),
        ),
        Some(s) => (
            Status::Mismatch,
            format!("the server confirmed a different {step} transaction ({s})"),
        ),
    }
}

fn parties_check(k: &Known, a: &str, b: &str) -> Option<String> {
    (!(same(&k.proposer_vault, a) && same(&k.counterparty_vault, b)))
        .then(|| format!("the escrow names vaults {a} and {b}, not the two agents' linked vaults"))
}

fn amount_check(got: &str, want: i64, what: &str) -> Option<String> {
    (got != want.to_string()).then(|| format!("{what} is {got}, the server expects {want}"))
}

/// Pure comparison of one escrow event with the challenge it names.
pub fn evaluate(ev: &ContractEvent, tx: &str, k: &Known) -> (Status, String) {
    let mismatch = |note: String| (Status::Mismatch, note);
    match ev {
        ContractEvent::Created {
            party_a,
            party_b,
            stake,
            ..
        } => {
            if let Some(n) = parties_check(k, party_a, party_b) {
                return mismatch(n);
            }
            if let Some(n) = amount_check(stake, k.stake, "the stake per side") {
                return mismatch(n);
            }
            tx_check(&k.escrow_create_tx, tx, "escrow create")
        }
        ContractEvent::Deposited { party, amount, .. } => {
            let server = if same(&k.proposer_vault, party) {
                &k.proposer_stake_tx
            } else if same(&k.counterparty_vault, party) {
                &k.counterparty_stake_tx
            } else {
                return mismatch(format!(
                    "the deposit came from {party}, which is neither agent's vault"
                ));
            };
            if let Some(n) = amount_check(amount, k.stake, "the deposit") {
                return mismatch(n);
            }
            tx_check(server, tx, "stake")
        }
        ContractEvent::Activated {
            party_a,
            party_b,
            total_staked,
            ..
        } => {
            if let Some(n) = parties_check(k, party_a, party_b) {
                return mismatch(n);
            }
            if let Some(n) = amount_check(total_staked, k.stake * 2, "the total staked") {
                return mismatch(n);
            }
            if same(&k.proposer_stake_tx, tx) || same(&k.counterparty_stake_tx, tx) {
                (
                    Status::Confirmed,
                    "matches the server's verified stake receipt".into(),
                )
            } else if k.proposer_stake_tx.is_none() || k.counterparty_stake_tx.is_none() {
                (
                    Status::Pending,
                    "the server has not confirmed both stake transactions yet".into(),
                )
            } else {
                mismatch("activated by a transaction the server did not send".into())
            }
        }
        ContractEvent::Resolved {
            winner,
            loser,
            payout,
            ..
        } => {
            let (a, b) = (&k.proposer_vault, &k.counterparty_vault);
            if !((same(a, winner) && same(b, loser)) || (same(b, winner) && same(a, loser))) {
                return mismatch(format!(
                    "the escrow paid {winner}, which is not one of the two agents' vaults"
                ));
            }
            if let Some(n) = amount_check(payout, k.stake * 2, "the payout") {
                return mismatch(n);
            }
            let (status, note) = tx_check(&k.escrow_settle_tx, tx, "resolve");
            if status != Status::Confirmed {
                return (status, note);
            }
            let expected = match k.winner_id {
                None if k.outcome.as_deref() == Some("void") => {
                    return mismatch(
                        "the escrow paid a winner but the server settled the challenge as void"
                            .into(),
                    );
                }
                None => {
                    return (
                        Status::Pending,
                        "the server has not settled the ledger yet".into(),
                    );
                }
                Some(w) if w == k.proposer_id => a,
                Some(_) => b,
            };
            if same(expected, winner) {
                (Status::Confirmed, note)
            } else {
                mismatch(format!(
                    "the escrow paid {winner} but the server settled for the other agent"
                ))
            }
        }
        ContractEvent::Voided {
            party_a, party_b, ..
        } => {
            if let Some(n) = parties_check(k, party_a, party_b) {
                return mismatch(n);
            }
            if k.winner_id.is_some() {
                return mismatch(
                    "the escrow voided the challenge but the server settled it with a winner"
                        .into(),
                );
            }
            tx_check(&k.escrow_settle_tx, tx, "void")
        }
        ContractEvent::Refunded { .. } => (
            Status::Observed,
            "a timeout refund the server did not send; it re-reads the escrow on its next tick"
                .into(),
        ),
        ContractEvent::PayoutDeferred { party, .. } => (
            Status::Observed,
            format!("the payout to {party} could not be pushed and waits in the escrow"),
        ),
        ContractEvent::Claimed { party, .. } => (
            Status::Observed,
            format!("a deferred payout was claimed for {party}"),
        ),
        ContractEvent::VaultCreated { .. } => (Status::Unmatched, String::new()),
    }
}

/// `VaultCreated` against the agent its `agentRef` names.
pub fn evaluate_vault(vault: &str, agent: &KnownAgent) -> (Status, String) {
    match &agent.vault_address {
        None => (
            Status::Pending,
            "the owner has not linked this vault to the agent yet".into(),
        ),
        Some(v) if v == vault => (
            Status::Confirmed,
            "matches the vault the server verified and linked".into(),
        ),
        Some(v) => (
            Status::Observed,
            format!("the agent is linked to another vault ({v})"),
        ),
    }
}

fn decoded_json(ev: &ContractEvent) -> Value {
    let id = ev.challenge().map(|c| format!("{c:#x}"));
    let fields = match ev {
        ContractEvent::Created {
            party_a,
            party_b,
            stake,
            fund_deadline,
            resolve_deadline,
            ..
        } => json!({"party_a": party_a, "party_b": party_b, "stake": stake,
                    "fund_deadline": fund_deadline, "resolve_deadline": resolve_deadline}),
        ContractEvent::Deposited { party, amount, .. }
        | ContractEvent::PayoutDeferred { party, amount, .. }
        | ContractEvent::Claimed { party, amount, .. } => {
            json!({"party": party, "amount": amount})
        }
        ContractEvent::Activated {
            party_a,
            party_b,
            total_staked,
            ..
        } => json!({"party_a": party_a, "party_b": party_b, "total_staked": total_staked}),
        ContractEvent::Resolved {
            winner,
            loser,
            payout,
            ..
        } => json!({"winner": winner, "loser": loser, "payout": payout}),
        ContractEvent::Voided {
            party_a,
            party_b,
            refund_a,
            refund_b,
            ..
        }
        | ContractEvent::Refunded {
            party_a,
            party_b,
            refund_a,
            refund_b,
            ..
        } => json!({"party_a": party_a, "party_b": party_b,
                    "refund_a": refund_a, "refund_b": refund_b}),
        ContractEvent::VaultCreated {
            vault,
            owner,
            operator,
            agent_ref,
            max_per_challenge,
            max_exposure,
        } => json!({"vault": vault, "owner": owner, "operator": operator,
                    "agent_ref": format!("{agent_ref:#x}"),
                    "max_per_challenge": max_per_challenge, "max_exposure": max_exposure}),
    };
    let mut v = fields;
    if let (Some(id), Some(o)) = (id, v.as_object_mut()) {
        o.insert("challenge_id".into(), json!(id));
    }
    v
}

#[derive(Debug, sqlx::FromRow)]
struct EventRow {
    id: Uuid,
    tx_hash: String,
    log_index: i64,
    block_number: i64,
    block_timestamp: Option<DateTime<Utc>>,
    address: String,
    topics: Value,
    data: String,
    removed: bool,
    received_at: DateTime<Utc>,
    match_status: Option<String>,
}

fn latency_ms(row: &EventRow) -> Option<i64> {
    row.block_timestamp
        .map(|b| (row.received_at - b).num_milliseconds())
}

fn explorer_tx(c: &StreamContracts, tx: &str) -> Option<String> {
    c.explorer_url
        .as_ref()
        .map(|base| format!("{}/tx/{tx}", base.trim_end_matches('/')))
}

fn short(addr: &str) -> String {
    format!(
        "{}…{}",
        &addr[..8.min(addr.len())],
        &addr[addr.len().saturating_sub(4)..]
    )
}

fn usdc(units: &str) -> String {
    units
        .parse::<i64>()
        .ok()
        .and_then(Money::from_minor)
        .map_or_else(
            || format!("{units} base units"),
            |m| stake_text(m, Env::ArcTestnet),
        )
}

fn seen_text(row: &EventRow) -> String {
    match latency_ms(row) {
        Some(ms) if ms >= 0 => format!(
            "block {}, {:.1} s after the block",
            row.block_number,
            ms as f64 / 1000.0
        ),
        _ => format!("block {}", row.block_number),
    }
}

/// The readable sentence for a confirmed event.
fn confirmed_text(ev: &ContractEvent, k: Option<&Known>, agent: Option<&str>) -> String {
    let name_of = |vault: &str| -> String {
        k.and_then(|k| {
            if same(&k.proposer_vault, vault) {
                Some(k.proposer_name.clone())
            } else if same(&k.counterparty_vault, vault) {
                Some(k.counterparty_name.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| short(vault))
    };
    match ev {
        ContractEvent::Created { stake, .. } => match k {
            Some(k) => format!(
                "Escrow registered on Arc for \"{}\" vs \"{}\" ({} per side)",
                k.proposer_name,
                k.counterparty_name,
                usdc(stake)
            ),
            None => format!("Escrow registered on Arc ({} per side)", usdc(stake)),
        },
        ContractEvent::Deposited { party, amount, .. } => format!(
            "Escrow funded on Arc: \"{}\"'s vault deposited {}",
            name_of(party),
            usdc(amount)
        ),
        ContractEvent::Activated { total_staked, .. } => format!(
            "Both stakes are locked in the Arc escrow ({} in total)",
            usdc(total_staked)
        ),
        ContractEvent::Resolved { winner, payout, .. } => format!(
            "Escrow paid {} to \"{}\"'s vault on Arc",
            usdc(payout),
            name_of(winner)
        ),
        ContractEvent::Voided { .. } => "Escrow returned both stakes on Arc (void)".into(),
        ContractEvent::Refunded { .. } => "Escrow refunded both stakes on Arc (timeout)".into(),
        ContractEvent::PayoutDeferred { party, amount, .. } => format!(
            "The escrow could not push {} to \"{}\"'s vault; it is held for a claim",
            usdc(amount),
            name_of(party)
        ),
        ContractEvent::Claimed { party, amount, .. } => format!(
            "A deferred payout of {} was claimed for \"{}\"'s vault",
            usdc(amount),
            name_of(party)
        ),
        ContractEvent::VaultCreated { vault, .. } => format!(
            "Vault {} for \"{}\" created on Arc",
            short(vault),
            agent.unwrap_or("an agent")
        ),
    }
}

async fn known(conn: &mut PgConnection, id: Uuid) -> Result<Option<Known>, sqlx::Error> {
    sqlx::query_as(
        "SELECT i.id, i.stake, i.proposer_id, i.counterparty_id, \
                pa.name AS proposer_name, pb.name AS counterparty_name, \
                pa.vault_address AS proposer_vault, pb.vault_address AS counterparty_vault, \
                i.escrow_create_tx, i.proposer_stake_tx, i.counterparty_stake_tx, \
                i.escrow_settle_tx, i.winner_id, i.outcome \
         FROM interactions i \
         JOIN agents pa ON pa.id = i.proposer_id \
         JOIN agents pb ON pb.id = i.counterparty_id \
         WHERE i.id = $1 AND i.env = 'arc_testnet'",
    )
    .bind(id)
    .fetch_optional(conn)
    .await
}

/// Decodes (if needed), matches and records one stored event. Locks the row,
/// so concurrent callers record each transition once. Returns the status.
pub async fn check_event(
    conn: &mut PgConnection,
    contracts: &StreamContracts,
    event_id: Uuid,
) -> Result<Option<Status>, ApiError> {
    let Some(row): Option<EventRow> = sqlx::query_as(
        "SELECT id, tx_hash, log_index, block_number, block_timestamp, address, topics, data, \
                removed, received_at, match_status \
         FROM chain_events WHERE id = $1 FOR UPDATE",
    )
    .bind(event_id)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    if row.removed || matches!(row.match_status.as_deref(), Some(s) if s != "pending") {
        return Ok(None);
    }
    let source = if row.address == contracts.escrow {
        Source::Escrow
    } else if row.address == contracts.factory {
        Source::Factory
    } else {
        return Ok(None);
    };
    let topics: Vec<String> = serde_json::from_value(row.topics.clone()).unwrap_or_default();
    let Some(ev) = events::decode(source, &topics, &row.data) else {
        return Ok(None);
    };

    let mut interaction_id = None;
    let mut agent_id = None;
    let mut k = None;
    let mut agent_name = None;
    let (status, note) = match &ev {
        ContractEvent::VaultCreated {
            vault, agent_ref, ..
        } => {
            let agent: Option<KnownAgent> = match events::interaction_of(*agent_ref) {
                Some(id) => {
                    sqlx::query_as("SELECT id, name, vault_address FROM agents WHERE id = $1")
                        .bind(id)
                        .fetch_optional(&mut *conn)
                        .await?
                }
                None => None,
            };
            match agent {
                Some(a) => {
                    agent_id = Some(a.id);
                    agent_name = Some(a.name.clone());
                    evaluate_vault(vault, &a)
                }
                None => (
                    Status::Unmatched,
                    "no agent on this server has this agentRef".into(),
                ),
            }
        }
        _ => {
            let found = match ev.challenge().and_then(events::interaction_of) {
                Some(id) => known(&mut *conn, id).await?,
                None => None,
            };
            match found {
                Some(found) => {
                    interaction_id = Some(found.id);
                    let r = evaluate(&ev, &row.tx_hash, &found);
                    k = Some(found);
                    r
                }
                None => (
                    Status::Unmatched,
                    "no Arc testnet challenge on this server has this escrow id".into(),
                ),
            }
        }
    };

    let now = Utc::now();
    sqlx::query(
        "UPDATE chain_events SET event_name = $2, decoded = $3, interaction_id = $4, \
         agent_id = $5, match_status = $6, match_note = $7, checked_at = $8 WHERE id = $1",
    )
    .bind(row.id)
    .bind(ev.name())
    .bind(decoded_json(&ev))
    .bind(interaction_id)
    .bind(agent_id)
    .bind(status.as_str())
    .bind(&note)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    if status == Status::Pending {
        return Ok(Some(status));
    }

    // One activity row per final status (the row lock and the status filter
    // above make this once per event).
    let actor = match (&ev, &k) {
        (ContractEvent::Deposited { party, .. }, Some(k))
        | (ContractEvent::Resolved { winner: party, .. }, Some(k)) => {
            if same(&k.counterparty_vault, party) {
                Some(k.counterparty_id)
            } else {
                Some(k.proposer_id)
            }
        }
        (_, Some(k)) => Some(k.proposer_id),
        (_, None) => agent_id,
    };
    let what = confirmed_text(&ev, k.as_ref(), agent_name.as_deref());
    let (kind, summary) = match status {
        Status::Confirmed => (
            "stream_confirmed",
            format!(
                "{what}, confirmed by QuickNode Streams ({}).",
                seen_text(&row)
            ),
        ),
        Status::Mismatch => (
            "stream_mismatch",
            format!(
                "QuickNode Streams reported \"{}\" ({}) but it does not match what the server \
                 confirmed: {note}. Nothing was changed; the server's own receipt and escrow \
                 reads stay authoritative.",
                ev.name(),
                seen_text(&row)
            ),
        ),
        Status::Observed => (
            "stream_observed",
            format!(
                "{what}, seen by QuickNode Streams ({}): {note}.",
                seen_text(&row)
            ),
        ),
        _ => (
            "chain_event",
            format!(
                "Arc testnet {} event seen via QuickNode Streams ({}) for a {} this server does \
                 not know. Recorded only.",
                ev.name(),
                seen_text(&row),
                if source == Source::Factory {
                    "vault"
                } else {
                    "challenge"
                }
            ),
        ),
    };
    if status == Status::Mismatch {
        tracing::warn!(event = ev.name(), tx = %row.tx_hash, note = %note, "stream event disagrees with the server");
    }
    record(
        &mut *conn,
        actor,
        interaction_id,
        kind,
        summary,
        json!({
            "chain_event_id": row.id, "source": "quicknode_streams", "env": "arc_testnet",
            "event": ev.name(), "status": status.as_str(), "note": note,
            "tx_hash": row.tx_hash, "log_index": row.log_index,
            "block_number": row.block_number, "block_timestamp": row.block_timestamp,
            "received_at": row.received_at, "latency_ms": latency_ms(&row),
            "explorer_url": explorer_tx(contracts, &row.tx_hash),
            "address": row.address,
        }),
    )
    .await?;
    Ok(Some(status))
}

/// Rechecks pending events (all, or one challenge's). Errors are logged:
/// confirmation is never on the critical path of the lifecycle.
pub async fn recheck(state: &AppState, interaction: Option<Uuid>) {
    if let Err(e) = try_recheck(state, interaction).await {
        tracing::warn!(error = %e.message, "stream confirmation recheck failed");
    }
}

async fn try_recheck(state: &AppState, interaction: Option<Uuid>) -> Result<usize, ApiError> {
    let Some(contracts) = contracts(state) else {
        return Ok(0);
    };
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM chain_events WHERE match_status = 'pending' AND NOT removed \
           AND received_at > now() - make_interval(hours => $1) \
           AND ($2::uuid IS NULL OR interaction_id = $2) \
         ORDER BY seq LIMIT $3",
    )
    .bind(RECHECK_WINDOW_HOURS as i32)
    .bind(interaction)
    .bind(RECHECK_BATCH)
    .fetch_all(&state.pool)
    .await?;
    let mut changed = 0;
    for id in ids {
        let mut tx = state.pool.begin().await?;
        if check_event(&mut tx, &contracts, id).await? != Some(Status::Pending) {
            changed += 1;
        }
        tx.commit().await?;
    }
    Ok(changed)
}

// ---------------------------------------------------------------------------
// Read side
// ---------------------------------------------------------------------------

/// One decoded stream event, as shown on a challenge.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct StreamEvent {
    pub id: Uuid,
    pub event: String,
    pub status: String,
    pub note: Option<String>,
    pub tx_hash: String,
    pub log_index: i64,
    pub block_number: i64,
    pub block_timestamp: Option<DateTime<Utc>>,
    pub received_at: DateTime<Utc>,
    /// `received_at - block_timestamp`; block timestamps are whole seconds.
    pub latency_ms: Option<i64>,
    pub removed: bool,
    #[sqlx(skip)]
    pub explorer_url: Option<String>,
}

pub async fn for_interaction(
    state: &AppState,
    pool: &PgPool,
    id: Uuid,
) -> Result<Vec<StreamEvent>, ApiError> {
    let mut rows: Vec<StreamEvent> = sqlx::query_as(
        "SELECT id, event_name AS event, match_status AS status, match_note AS note, tx_hash, \
                log_index, block_number, block_timestamp, received_at, \
                (EXTRACT(EPOCH FROM (received_at - block_timestamp)) * 1000)::bigint \
                  AS latency_ms, removed \
         FROM chain_events WHERE interaction_id = $1 AND event_name IS NOT NULL \
         ORDER BY block_number, log_index",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    if let Some(c) = contracts(state) {
        for r in &mut rows {
            r.explorer_url = explorer_tx(&c, &r.tx_hash);
        }
    }
    Ok(rows)
}

#[derive(Debug, Serialize)]
pub struct StreamsStatus {
    /// The webhook route is mounted (`QUICKNODE_STREAMS_SECRET` set).
    pub webhook_enabled: bool,
    /// Escrow and factory addresses are known, so events can be matched.
    pub matching_enabled: bool,
    pub events_total: i64,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub last_block_number: Option<i64>,
    /// Median block-to-delivery latency of the last 50 events that carry a block timestamp.
    pub median_latency_ms: Option<i64>,
    pub confirmed: i64,
    pub pending: i64,
    pub mismatch: i64,
    pub observed: i64,
    pub unmatched: i64,
}

/// `GET /api/streams`: QuickNode Streams health for the UI.
pub async fn status(State(state): State<AppState>) -> Result<Json<StreamsStatus>, ApiError> {
    #[allow(clippy::type_complexity)]
    let (total, last, block, confirmed, pending, mismatch, observed, unmatched): (
        i64,
        Option<DateTime<Utc>>,
        Option<i64>,
        i64,
        i64,
        i64,
        i64,
        i64,
    ) = sqlx::query_as(
        "SELECT COUNT(*), MAX(received_at), MAX(block_number), \
                COUNT(*) FILTER (WHERE match_status = 'confirmed' AND NOT removed), \
                COUNT(*) FILTER (WHERE match_status = 'pending' AND NOT removed), \
                COUNT(*) FILTER (WHERE match_status = 'mismatch' AND NOT removed), \
                COUNT(*) FILTER (WHERE match_status = 'observed' AND NOT removed), \
                COUNT(*) FILTER (WHERE match_status = 'unmatched' AND NOT removed) \
         FROM chain_events",
    )
    .fetch_one(&state.pool)
    .await?;
    let median: Option<f64> = sqlx::query_scalar(
        "SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY ms) FROM ( \
           SELECT EXTRACT(EPOCH FROM (received_at - block_timestamp)) * 1000 AS ms \
           FROM chain_events WHERE block_timestamp IS NOT NULL ORDER BY seq DESC LIMIT 50) t",
    )
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(StreamsStatus {
        webhook_enabled: state.settings.quicknode.is_some(),
        matching_enabled: contracts(&state).is_some(),
        events_total: total,
        last_delivery_at: last,
        last_block_number: block,
        median_latency_ms: median.map(|m| m.round() as i64),
        confirmed,
        pending,
        mismatch,
        observed,
        unmatched,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VA: &str = "0xf78499779bcc48f4e96d34a06d6ae4869cc5b37f";
    const VB: &str = "0xcac9240abc5563bbf7a8fb32be8c5fd3b1c3d0b6";
    const TX: &str = "0x792b93c846b895f583082956e78853cb0ec71c444907e9a3e1e9e713c83f840f";
    const OTHER: &str = "0x6f087c7b1c925365f9148506e2612f006d13d06049645ccb8a648dfc0b8ef2b0";

    fn known() -> Known {
        Known {
            stake: 1_500_000,
            proposer_id: Uuid::from_u128(1),
            counterparty_id: Uuid::from_u128(2),
            proposer_vault: Some(VA.into()),
            counterparty_vault: Some(VB.into()),
            ..Known::default()
        }
    }

    fn deposit(party: &str, amount: &str) -> ContractEvent {
        ContractEvent::Deposited {
            id: Default::default(),
            party: party.into(),
            amount: amount.into(),
        }
    }

    #[test]
    fn a_deposit_is_pending_then_confirmed_or_flagged() {
        let mut k = known();
        let ev = deposit(VA, "1500000");
        assert_eq!(evaluate(&ev, TX, &k).0, Status::Pending);
        k.proposer_stake_tx = Some(TX.into());
        assert_eq!(evaluate(&ev, TX, &k).0, Status::Confirmed);
        k.proposer_stake_tx = Some(OTHER.into());
        assert_eq!(evaluate(&ev, TX, &k).0, Status::Mismatch);
        // Wrong amount or a stranger's vault are mismatches whatever the tx.
        k.proposer_stake_tx = Some(TX.into());
        assert_eq!(evaluate(&deposit(VA, "1"), TX, &k).0, Status::Mismatch);
        let stranger = "0x1111111111111111111111111111111111111111";
        assert_eq!(
            evaluate(&deposit(stranger, "1500000"), TX, &k).0,
            Status::Mismatch
        );
    }

    #[test]
    fn a_resolution_must_pay_the_winner_the_server_settled_for() {
        let mut k = known();
        let ev = ContractEvent::Resolved {
            id: Default::default(),
            winner: VA.into(),
            loser: VB.into(),
            payout: "3000000".into(),
        };
        assert_eq!(evaluate(&ev, TX, &k).0, Status::Pending);
        k.escrow_settle_tx = Some(TX.into());
        // Tx confirmed but the ledger not settled yet.
        assert_eq!(evaluate(&ev, TX, &k).0, Status::Pending);
        k.winner_id = Some(k.proposer_id);
        assert_eq!(evaluate(&ev, TX, &k).0, Status::Confirmed);
        k.winner_id = Some(k.counterparty_id);
        assert_eq!(evaluate(&ev, TX, &k).0, Status::Mismatch);
        k.winner_id = None;
        k.outcome = Some("void".into());
        assert_eq!(evaluate(&ev, TX, &k).0, Status::Mismatch);
    }

    #[test]
    fn refunds_are_observed_and_vaults_match_by_agent() {
        let ev = ContractEvent::Refunded {
            id: Default::default(),
            party_a: VA.into(),
            party_b: VB.into(),
            refund_a: "1".into(),
            refund_b: "1".into(),
        };
        assert_eq!(evaluate(&ev, TX, &known()).0, Status::Observed);
        let mut a = KnownAgent {
            id: Uuid::nil(),
            name: "A".into(),
            vault_address: None,
        };
        assert_eq!(evaluate_vault(VA, &a).0, Status::Pending);
        a.vault_address = Some(VA.into());
        assert_eq!(evaluate_vault(VA, &a).0, Status::Confirmed);
        assert_eq!(evaluate_vault(VB, &a).0, Status::Observed);
    }
}
