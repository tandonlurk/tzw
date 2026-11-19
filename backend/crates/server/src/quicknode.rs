//! QuickNode Streams webhook ingestion (local only).
//!
//! `POST /api/webhooks/quicknode` is mounted only when
//! `QUICKNODE_STREAMS_SECRET` is set. A delivery is accepted only with a
//! valid HMAC-SHA256 signature over `nonce + timestamp + raw body` and a
//! timestamp within `QUICKNODE_MAX_SKEW_SECS` of the server clock. Each log
//! is stored once, keyed on `(tx_hash, log_index)`, so duplicate and
//! reordered deliveries are no-ops, and is recorded as activity.
//!
//! Header names (`X-QN-Signature`, `X-QN-Timestamp`, `X-QN-Nonce`), the
//! signed message (hex HMAC-SHA256 of nonce + timestamp + decompressed body,
//! keyed with the stream's security token) and the `{data, metadata}` log
//! fields match QuickNode's docs as read 2026-11-18 (docs/research/sponsors.md).
//! Still UNVERIFIED until one real delivery is captured. Header names stay
//! configurable; the assumptions are documented in docs/api.md. Nothing here
//! acts on an event: there is no chain executor, and nothing signs or sends.

use std::fmt;

use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::Sha256;
use uuid::Uuid;

use crate::error::{ApiError, Detail};
use crate::extract::is_json;
use crate::state::AppState;

pub const PATH: &str = "/api/webhooks/quicknode";
pub const DEFAULT_SIGNATURE_HEADER: &str = "x-qn-signature";
pub const DEFAULT_TIMESTAMP_HEADER: &str = "x-qn-timestamp";
pub const DEFAULT_NONCE_HEADER: &str = "x-qn-nonce";
pub const DEFAULT_MAX_SKEW_SECS: i64 = 300;
/// Logs accepted in one delivery.
pub const MAX_LOGS: usize = 5_000;
/// Activity rows written per delivery; the rest are summarized in one row.
pub const MAX_ACTIVITY_PER_DELIVERY: usize = 50;
const MAX_DATA_HEX: usize = 64 * 1024;

/// The stream's security token. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq)]
pub struct StreamSecret(String);

impl StreamSecret {
    pub fn new(secret: &str) -> Option<StreamSecret> {
        let s = secret.trim();
        (!s.is_empty()).then(|| StreamSecret(s.to_owned()))
    }
}

impl fmt::Debug for StreamSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("StreamSecret(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickNodeConfig {
    pub secret: StreamSecret,
    /// Lowercase header names.
    pub signature_header: String,
    pub timestamp_header: String,
    pub nonce_header: String,
    pub max_skew_secs: i64,
}

impl QuickNodeConfig {
    pub fn new(secret: StreamSecret) -> QuickNodeConfig {
        QuickNodeConfig {
            secret,
            signature_header: DEFAULT_SIGNATURE_HEADER.into(),
            timestamp_header: DEFAULT_TIMESTAMP_HEADER.into(),
            nonce_header: DEFAULT_NONCE_HEADER.into(),
            max_skew_secs: DEFAULT_MAX_SKEW_SECS,
        }
    }
}

/// Lowercase hex of HMAC-SHA256(`secret`, `nonce + timestamp + body`). Used by
/// the handler (through `verify`) and by tests to sign fixtures.
pub fn sign(secret: &str, nonce: &str, timestamp: &str, body: &[u8]) -> String {
    mac(secret, nonce, timestamp, body)
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn mac(secret: &str, nonce: &str, timestamp: &str, body: &[u8]) -> Hmac<Sha256> {
    // HMAC accepts keys of any length, so this cannot fail.
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .unwrap_or_else(|_| unreachable!("HMAC takes any key length"));
    mac.update(nonce.as_bytes());
    mac.update(timestamp.as_bytes());
    mac.update(body);
    mac
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    let s = s.strip_prefix("sha256=").unwrap_or(s);
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Constant-time check of a hex signature.
fn verify(secret: &str, nonce: &str, timestamp: &str, body: &[u8], signature: &str) -> bool {
    let Some(sig) = from_hex(signature) else {
        return false;
    };
    mac(secret, nonce, timestamp, body)
        .verify_slice(&sig)
        .is_ok()
}

/// Seconds since the epoch; a value above 10^12 is taken as milliseconds.
fn parse_timestamp(s: &str) -> Option<i64> {
    let n: i64 = s.trim().parse().ok()?;
    Some(if n > 1_000_000_000_000 { n / 1000 } else { n })
}

fn unauthorized(code: &'static str, message: &str) -> ApiError {
    ApiError::new(StatusCode::UNAUTHORIZED, code, message)
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

// ---------------------------------------------------------------------------
// Payload (UNVERIFIED shape, see docs/api.md)
// ---------------------------------------------------------------------------

/// One validated log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Log {
    pub tx_hash: String,
    pub log_index: i64,
    pub block_number: i64,
    pub block_hash: Option<String>,
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub removed: bool,
}

/// `0x`-prefixed hex quantity or a JSON number.
fn quantity(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().filter(|n| *n >= 0),
        Value::String(s) => {
            let hex = s.strip_prefix("0x")?;
            if hex.is_empty() || hex.len() > 15 {
                return None;
            }
            i64::from_str_radix(hex, 16).ok()
        }
        _ => None,
    }
}

/// Lowercased `0x` + exactly `len` hex digits.
fn hex_id(v: Option<&Value>, len: usize) -> Option<String> {
    let s = v?.as_str()?.to_ascii_lowercase();
    let hex = s.strip_prefix("0x")?;
    (hex.len() == len && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(s)
}

fn hex_data(v: Option<&Value>) -> Option<String> {
    let s = v
        .and_then(Value::as_str)
        .unwrap_or("0x")
        .to_ascii_lowercase();
    let hex = s.strip_prefix("0x")?;
    (hex.len() <= MAX_DATA_HEX && hex.len() % 2 == 0 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then_some(s)
}

fn parse_log(v: &Value) -> Option<Log> {
    let o = v.as_object()?;
    let topics = o
        .get("topics")
        .and_then(Value::as_array)
        .map(|t| {
            t.iter()
                .map(|x| hex_id(Some(x), 64))
                .collect::<Option<Vec<_>>>()
        })
        .unwrap_or(Some(Vec::new()))?;
    if topics.len() > 4 {
        return None;
    }
    Some(Log {
        tx_hash: hex_id(o.get("transactionHash"), 64)?,
        log_index: quantity(o.get("logIndex"))?,
        block_number: quantity(o.get("blockNumber"))?,
        block_hash: match o.get("blockHash") {
            None | Some(Value::Null) => None,
            v => Some(hex_id(v, 64)?),
        },
        address: hex_id(o.get("address"), 40)?,
        topics,
        data: hex_data(o.get("data"))?,
        removed: o.get("removed").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Everything in a delivery: `{data: [...], metadata: {...}}` or a bare
/// array. `data` may hold logs or arrays of logs (one array per block); a
/// `{logs: [...]}` element (receipt-shaped) is opened too.
#[derive(Debug, Default)]
pub struct Batch {
    pub logs: Vec<Log>,
    pub rejected: usize,
    pub network: Option<String>,
    pub stream_id: Option<String>,
}

fn collect(v: &Value, depth: u8, out: &mut Batch) {
    match v {
        Value::Array(items) if depth < 3 => {
            for item in items {
                collect(item, depth + 1, out);
            }
        }
        Value::Object(o) if o.contains_key("transactionHash") => match parse_log(v) {
            Some(log) => out.logs.push(log),
            None => out.rejected += 1,
        },
        Value::Object(o) if depth < 3 && o.get("logs").is_some_and(Value::is_array) => {
            collect(&o["logs"], depth + 1, out);
        }
        _ => out.rejected += 1,
    }
}

pub fn parse_batch(body: &Value) -> Result<Batch, ApiError> {
    let mut batch = Batch::default();
    let data = match body {
        Value::Array(_) => body,
        Value::Object(o) => {
            let meta = o.get("metadata");
            let text = |k: &str| {
                meta.and_then(|m| m.get(k))
                    .and_then(Value::as_str)
                    .map(|s| s.chars().take(100).collect())
            };
            batch.network = text("network");
            batch.stream_id = text("stream_id");
            o.get("data").ok_or_else(|| {
                ApiError::validation(vec![Detail::new("data", "missing", "`data` is required")])
            })?
        }
        _ => {
            return Err(ApiError::validation(vec![Detail::new(
                "",
                "invalid_value",
                "expected an object with `data` or an array",
            )]));
        }
    };
    if !data.is_array() {
        return Err(ApiError::validation(vec![Detail::new(
            "data",
            "invalid_value",
            "`data` must be an array",
        )]));
    }
    collect(data, 0, &mut batch);
    if batch.logs.len() > MAX_LOGS {
        return Err(ApiError::validation(vec![Detail::new(
            "data",
            "too_many_logs",
            format!("at most {MAX_LOGS} logs per delivery"),
        )]));
    }
    Ok(batch)
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub received: usize,
    pub inserted: usize,
    pub duplicates: usize,
    pub removed: usize,
    pub rejected: usize,
}

fn short(hash: &str) -> String {
    format!(
        "{}…{}",
        &hash[..10.min(hash.len())],
        &hash[hash.len().saturating_sub(4)..]
    )
}

pub async fn webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Receipt>, ApiError> {
    // Mounted only when configured; this is a second line of defence.
    let Some(cfg) = state.settings.quicknode.as_ref() else {
        return Err(ApiError::not_found("route"));
    };
    // QuickNode can gzip deliveries and signs the decompressed body; this
    // endpoint reads plain JSON only, so say so instead of failing the
    // signature check.
    if header(&headers, "content-encoding")
        .is_some_and(|e| !e.trim().eq_ignore_ascii_case("identity"))
    {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "compressed deliveries are not accepted: turn compression off on the stream's webhook destination",
        ));
    }
    let signature = header(&headers, &cfg.signature_header)
        .ok_or_else(|| unauthorized("missing_signature", "the signature header is missing"))?;
    let timestamp = header(&headers, &cfg.timestamp_header)
        .ok_or_else(|| unauthorized("missing_signature", "the timestamp header is missing"))?;
    let nonce = header(&headers, &cfg.nonce_header).unwrap_or("");
    if !verify(&cfg.secret.0, nonce, timestamp, &body, signature) {
        return Err(unauthorized(
            "invalid_signature",
            "the signature does not match",
        ));
    }
    // Checked after the signature, so the timestamp itself is authenticated.
    let now = Utc::now().timestamp();
    match parse_timestamp(timestamp) {
        Some(ts) if (now - ts).abs() <= cfg.max_skew_secs => {}
        _ => {
            return Err(unauthorized(
                "stale_timestamp",
                "the delivery timestamp is missing, malformed or too far from the server clock",
            ));
        }
    }
    if !is_json(&headers) {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "Content-Type must be application/json (uncompressed)",
        ));
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|e| ApiError::bad_request(format!("invalid JSON: {e}")))?;
    let batch = parse_batch(&value)?;
    let received_at: DateTime<Utc> = Utc::now();

    let mut tx = state.pool.begin().await?;
    let mut receipt = Receipt {
        received: batch.logs.len(),
        inserted: 0,
        duplicates: 0,
        removed: 0,
        rejected: batch.rejected,
    };
    let mut activity_rows = 0usize;
    let mut unlogged = 0usize;
    for log in &batch.logs {
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "INSERT INTO chain_events (source, network, stream_id, tx_hash, log_index, \
             block_number, block_hash, address, topics, data, removed, received_at) \
             VALUES ('quicknode_streams', $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
             ON CONFLICT (tx_hash, log_index) DO NOTHING RETURNING id",
        )
        .bind(&batch.network)
        .bind(&batch.stream_id)
        .bind(&log.tx_hash)
        .bind(log.log_index)
        .bind(log.block_number)
        .bind(&log.block_hash)
        .bind(&log.address)
        .bind(json!(log.topics))
        .bind(&log.data)
        .bind(log.removed)
        .bind(received_at)
        .fetch_optional(&mut *tx)
        .await?;
        let (id, kind, summary) = match inserted {
            Some(id) => {
                receipt.inserted += 1;
                (
                    id,
                    "chain_event",
                    format!(
                        "Arc testnet event seen via QuickNode Streams: log {} of transaction {} \
                         from contract {} in block {}{}. Recorded only; nothing acts on it yet.",
                        log.log_index,
                        short(&log.tx_hash),
                        short(&log.address),
                        log.block_number,
                        if log.removed {
                            " (already marked removed by a reorg)"
                        } else {
                            ""
                        }
                    ),
                )
            }
            None if log.removed => {
                // A known log reported as removed (reorg): mark it once.
                let marked: Option<Uuid> = sqlx::query_scalar(
                    "UPDATE chain_events SET removed = true \
                     WHERE tx_hash = $1 AND log_index = $2 AND NOT removed RETURNING id",
                )
                .bind(&log.tx_hash)
                .bind(log.log_index)
                .fetch_optional(&mut *tx)
                .await?;
                let Some(id) = marked else {
                    receipt.duplicates += 1;
                    continue;
                };
                receipt.removed += 1;
                (
                    id,
                    "chain_event_removed",
                    format!(
                        "Arc testnet event removed by a chain reorganization: log {} of \
                         transaction {}.",
                        log.log_index,
                        short(&log.tx_hash)
                    ),
                )
            }
            None => {
                receipt.duplicates += 1;
                continue;
            }
        };
        if activity_rows < MAX_ACTIVITY_PER_DELIVERY {
            activity_rows += 1;
            sqlx::query("INSERT INTO activity (kind, summary, detail) VALUES ($1, $2, $3)")
                .bind(kind)
                .bind(summary)
                .bind(json!({
                    "chain_event_id": id, "source": "quicknode_streams", "env": "arc_testnet",
                    "network": batch.network, "tx_hash": log.tx_hash, "log_index": log.log_index,
                    "block_number": log.block_number, "address": log.address,
                    "topic0": log.topics.first(),
                }))
                .execute(&mut *tx)
                .await?;
        } else {
            unlogged += 1;
        }
    }
    if unlogged > 0 {
        sqlx::query("INSERT INTO activity (kind, summary, detail) VALUES ($1, $2, $3)")
            .bind("chain_events")
            .bind(format!(
                "{unlogged} more Arc testnet events were recorded from the same QuickNode \
                 Streams delivery."
            ))
            .bind(json!({ "count": unlogged, "source": "quicknode_streams" }))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    tracing::info!(
        received = receipt.received,
        inserted = receipt.inserted,
        duplicates = receipt.duplicates,
        removed = receipt.removed,
        rejected = receipt.rejected,
        "quicknode streams delivery"
    );
    Ok(Json(receipt))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_verify_only_for_the_exact_input() {
        let sig = sign("s3cret", "n1", "1700000000", b"{\"data\":[]}");
        assert_eq!(sig.len(), 64);
        assert!(verify("s3cret", "n1", "1700000000", b"{\"data\":[]}", &sig));
        assert!(verify(
            "s3cret",
            "n1",
            "1700000000",
            b"{\"data\":[]}",
            &format!("sha256={}", sig.to_uppercase())
        ));
        assert!(!verify("other", "n1", "1700000000", b"{\"data\":[]}", &sig));
        assert!(!verify(
            "s3cret",
            "n2",
            "1700000000",
            b"{\"data\":[]}",
            &sig
        ));
        assert!(!verify(
            "s3cret",
            "n1",
            "1700000001",
            b"{\"data\":[]}",
            &sig
        ));
        assert!(!verify(
            "s3cret",
            "n1",
            "1700000000",
            b"{\"data\":[1]}",
            &sig
        ));
        assert!(!verify(
            "s3cret",
            "n1",
            "1700000000",
            b"{\"data\":[]}",
            "zz"
        ));
        assert!(!verify("s3cret", "n1", "1700000000", b"{\"data\":[]}", ""));
    }

    #[test]
    fn timestamps_accept_seconds_and_milliseconds() {
        assert_eq!(parse_timestamp("1700000000"), Some(1_700_000_000));
        assert_eq!(parse_timestamp("1700000000123"), Some(1_700_000_000));
        assert_eq!(parse_timestamp("soon"), None);
    }

    #[test]
    fn logs_are_validated_and_normalized() {
        let tx = format!("0x{}", "AB".repeat(32));
        let good = json!({
            "address": format!("0x{}", "Cd".repeat(20)), "topics": [format!("0x{}", "1".repeat(64))],
            "data": "0x00ff", "blockNumber": "0x1a", "transactionHash": tx, "logIndex": "0x2",
            "blockHash": null, "removed": false,
        });
        let log = parse_log(&good).unwrap();
        assert_eq!(log.tx_hash, tx.to_ascii_lowercase());
        assert_eq!((log.block_number, log.log_index), (26, 2));
        assert!(log.address.starts_with("0xcdcd"));
        for (field, bad) in [
            ("transactionHash", json!("0x12")),
            ("logIndex", json!("12")),
            ("logIndex", json!(-1)),
            ("address", json!("0xnothex")),
            ("data", json!("0x0")),
            ("topics", json!(["0x01"])),
        ] {
            let mut v = good.clone();
            v[field] = bad;
            assert!(parse_log(&v).is_none(), "{field}");
        }
        // Wrapped, nested per block, receipt-shaped, and junk counted as rejected.
        let batch = parse_batch(&json!({
            "data": [[good.clone()], {"logs": [good.clone()]}, 7],
            "metadata": {"network": "arc-testnet", "stream_id": "s1"},
        }))
        .unwrap();
        assert_eq!((batch.logs.len(), batch.rejected), (2, 1));
        assert_eq!(batch.network.as_deref(), Some("arc-testnet"));
        assert!(parse_batch(&json!({"metadata": {}})).is_err());
        assert!(parse_batch(&json!("x")).is_err());
        assert_eq!(parse_batch(&json!([good])).unwrap().logs.len(), 1);
    }
}
