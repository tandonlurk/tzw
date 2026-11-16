//! Ledger persistence. The only code that writes `ledger_entries`.
//!
//! A posting set is written inside the caller's transaction. Before the
//! balance check, a transaction-scoped advisory lock is taken on the
//! idempotency key and on every account the set touches, so two concurrent
//! writers can never both pass the check against the same balance.

use std::collections::BTreeMap;

use aen_core::ledger::{Account, Env, PostingSet};
use aen_core::money::SignedMoney;
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgExecutor};
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::LedgerEntry;

pub const IDEMPOTENCY_HEADER: &str = "idempotency-key";
const MAX_KEY_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostOutcome {
    Posted,
    /// The same set was already written under this idempotency key.
    Replayed,
}

/// The only caller today: the single owner of this deployment (no user
/// accounts yet, D-9). When there are several, this becomes the
/// authenticated caller's id.
pub const OWNER_CALLER: &str = "owner";

/// The caller's `Idempotency-Key` header (or a fresh key if absent), scoped
/// to the caller as `client:{caller}:{key}` (SEC-09). The lifecycle's own
/// keys (`settle:…`, `commit_stake:…`, `demo-seed:…`) never start with
/// `client:`, so a client cannot pre-claim them, and one caller cannot claim
/// another's keys. Keys are also unique per env only (see `post`).
pub fn idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    Ok(format!(
        "client:{OWNER_CALLER}:{}",
        raw_idempotency_key(headers)?
    ))
}

fn raw_idempotency_key(headers: &HeaderMap) -> Result<String, ApiError> {
    let Some(value) = headers.get(IDEMPOTENCY_HEADER) else {
        return Ok(Uuid::new_v4().to_string());
    };
    let key = value
        .to_str()
        .map(str::trim)
        .map_err(|_| ApiError::bad_request("Idempotency-Key must be ASCII"))?;
    if key.is_empty() || key.len() > MAX_KEY_LEN {
        return Err(ApiError::bad_request(format!(
            "Idempotency-Key must be 1-{MAX_KEY_LEN} characters"
        )));
    }
    Ok(key.to_owned())
}

async fn lock(conn: &mut PgConnection, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(name)
        .execute(conn)
        .await?;
    Ok(())
}

/// Writes `set` within the open transaction `conn`, after checking that no
/// account would go negative. Replaying an idempotency key writes nothing.
pub async fn post(conn: &mut PgConnection, set: &PostingSet) -> Result<PostOutcome, ApiError> {
    let env = set.env().as_str();
    let key = set.idempotency_key();

    // Locks are always taken in the same order (key, then accounts sorted),
    // so concurrent posts cannot deadlock.
    lock(conn, &format!("idem:{env}:{key}")).await?;

    // Keys are scoped per env (SEC-09); a replay must match every field that
    // was written, including the interaction it belongs to.
    let existing: Vec<(String, i64, String, Option<Uuid>)> = sqlx::query_as(
        "SELECT account, delta, reason, interaction_id FROM ledger_entries \
         WHERE env = $1 AND idempotency_key = $2 ORDER BY line",
    )
    .bind(env)
    .bind(key)
    .fetch_all(&mut *conn)
    .await?;
    if !existing.is_empty() {
        let same = existing.len() == set.postings().len()
            && existing.iter().zip(set.postings()).all(|(row, p)| {
                row.0 == p.account.to_string()
                    && row.1 == p.delta.minor()
                    && row.2 == set.reason().as_str()
                    && row.3 == set.interaction_id()
            });
        return if same {
            Ok(PostOutcome::Replayed)
        } else {
            Err(ApiError::conflict(
                "idempotency_key_reused",
                "this Idempotency-Key was already used for a different request",
            ))
        };
    }

    let accounts: Vec<String> = set
        .net_deltas()
        .keys()
        .filter(|a| !a.may_go_negative())
        .map(Account::to_string)
        .collect();
    for account in &accounts {
        lock(conn, &format!("acct:{env}:{account}")).await?;
    }

    // Read after locking: READ COMMITTED gives this statement a fresh snapshot.
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT account, COALESCE(SUM(delta), 0)::BIGINT FROM ledger_entries \
         WHERE env = $1 AND account = ANY($2) GROUP BY account",
    )
    .bind(env)
    .bind(&accounts)
    .fetch_all(&mut *conn)
    .await?;
    let mut balances: BTreeMap<Account, SignedMoney> = BTreeMap::new();
    for (account, balance) in rows {
        let account: Account = account
            .parse()
            .map_err(|()| ApiError::internal("unparseable account in ledger"))?;
        balances.insert(account, SignedMoney::from_minor(balance));
    }
    set.check_balances(&balances)?;

    for (line, posting) in set.postings().iter().enumerate() {
        sqlx::query(
            "INSERT INTO ledger_entries \
             (env, idempotency_key, line, account, agent_id, delta, reason, interaction_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(env)
        .bind(key)
        .bind(line as i32)
        .bind(posting.account.to_string())
        .bind(posting.account.agent_id())
        .bind(posting.delta.minor())
        .bind(set.reason().as_str())
        .bind(set.interaction_id())
        .execute(&mut *conn)
        .await?;
    }
    Ok(PostOutcome::Posted)
}

#[derive(sqlx::FromRow)]
struct LedgerRow {
    id: Uuid,
    ts: DateTime<Utc>,
    env: String,
    account: String,
    delta: i64,
    reason: String,
    interaction_id: Option<Uuid>,
    tx_hash: Option<String>,
}

impl From<LedgerRow> for LedgerEntry {
    fn from(r: LedgerRow) -> Self {
        LedgerEntry {
            id: r.id,
            ts: r.ts,
            env: r.env,
            account: r.account,
            delta: SignedMoney::from_minor(r.delta),
            reason: r.reason,
            interaction_id: r.interaction_id,
            tx_hash: r.tx_hash,
        }
    }
}

const ENTRY_LIMIT: i64 = 500;

/// An agent's entries, newest first.
pub async fn agent_entries<'e>(
    ex: impl PgExecutor<'e>,
    agent_id: Uuid,
) -> Result<Vec<LedgerEntry>, sqlx::Error> {
    let rows: Vec<LedgerRow> = sqlx::query_as(
        "SELECT id, ts, env, account, delta, reason, interaction_id, tx_hash \
         FROM ledger_entries WHERE agent_id = $1 ORDER BY seq DESC LIMIT $2",
    )
    .bind(agent_id)
    .bind(ENTRY_LIMIT)
    .fetch_all(ex)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Entries on one account, newest first.
pub async fn account_entries<'e>(
    ex: impl PgExecutor<'e>,
    env: Env,
    account: Account,
) -> Result<Vec<LedgerEntry>, sqlx::Error> {
    let rows: Vec<LedgerRow> = sqlx::query_as(
        "SELECT id, ts, env, account, delta, reason, interaction_id, tx_hash \
         FROM ledger_entries WHERE env = $1 AND account = $2 ORDER BY seq DESC LIMIT $3",
    )
    .bind(env.as_str())
    .bind(account.to_string())
    .bind(ENTRY_LIMIT)
    .fetch_all(ex)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Every entry of one interaction, oldest first.
pub async fn interaction_entries<'e>(
    ex: impl PgExecutor<'e>,
    interaction_id: Uuid,
) -> Result<Vec<LedgerEntry>, sqlx::Error> {
    let rows: Vec<LedgerRow> = sqlx::query_as(
        "SELECT id, ts, env, account, delta, reason, interaction_id, tx_hash \
         FROM ledger_entries WHERE interaction_id = $1 ORDER BY seq",
    )
    .bind(interaction_id)
    .fetch_all(ex)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}
