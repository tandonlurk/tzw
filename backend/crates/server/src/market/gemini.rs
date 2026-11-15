//! Client for the Gemini crypto exchange's public, unauthenticated market
//! data API. Only endpoints and fields observed in live responses are used;
//! see `docs/research/gemini-prediction-markets.md`.
//!
//! Every request has a timeout and is retried with backoff on transient
//! failures. Errors are returned to the caller, never turned into a value.

use std::time::Duration;

use aen_core::challenge::parse_decimal_scaled;
use chrono::DateTime;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::{
    BoxFut, Candle, EventQuery, MarketData, MarketError, PmContract, PmEvent, SpotTicker, TimeFrame,
};

/// Spot prices are kept in micro-USD.
const PRICE_DECIMALS: u32 = 6;
/// Contract prices are fractions of 1.00, kept in basis points.
const BPS_DECIMALS: u32 = 4;

#[derive(Debug, Clone)]
pub struct GeminiClient {
    http: reqwest::Client,
    base_url: String,
    attempts: u32,
    backoff: Duration,
}

impl GeminiClient {
    pub const DEFAULT_BASE_URL: &'static str = "https://api.gemini.com";
    const TIMEOUT: Duration = Duration::from_secs(5);
    const ATTEMPTS: u32 = 3;
    const BACKOFF: Duration = Duration::from_millis(250);

    pub fn new(base_url: &str) -> Result<GeminiClient, MarketError> {
        Self::with_options(base_url, Self::TIMEOUT, Self::ATTEMPTS, Self::BACKOFF)
    }

    /// `backoff` doubles after every failed attempt.
    pub fn with_options(
        base_url: &str,
        timeout: Duration,
        attempts: u32,
        backoff: Duration,
    ) -> Result<GeminiClient, MarketError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent("aen-server")
            .build()
            .map_err(|e| MarketError::Transport(e.to_string()))?;
        Ok(GeminiClient {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            attempts: attempts.max(1),
            backoff,
        })
    }

    async fn get_once(&self, path: &str, query: &[(&str, String)]) -> Result<String, MarketError> {
        let response = self
            .http
            .get(format!("{}{path}", self.base_url))
            .query(query)
            .send()
            .await
            .map_err(transport)?;
        let status = response.status().as_u16();
        if status == 404 {
            return Err(MarketError::NotFound);
        }
        if !(200..300).contains(&status) {
            return Err(MarketError::Http(status));
        }
        response.text().await.map_err(transport)
    }

    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, MarketError> {
        let mut delay = self.backoff;
        let mut attempt = 1;
        let body = loop {
            match self.get_once(path, query).await {
                Ok(body) => break body,
                Err(e) if e.is_retryable() && attempt < self.attempts => {
                    tracing::warn!(%path, attempt, error = %e, "gemini request failed, retrying");
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        };
        serde_json::from_str(&body).map_err(|e| MarketError::Decode(e.to_string()))
    }
}

fn transport(e: reqwest::Error) -> MarketError {
    if e.is_timeout() {
        MarketError::Timeout
    } else {
        MarketError::Transport(e.without_url().to_string())
    }
}

/// Symbols and tickers go into the URL path, so only plain identifiers pass.
fn path_segment(value: &str) -> Result<&str, MarketError> {
    let ok = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(value)
    } else {
        Err(MarketError::NoData(format!(
            "invalid market identifier {value:?}"
        )))
    }
}

fn price(text: &str, what: &str) -> Result<i64, MarketError> {
    parse_decimal_scaled(text, PRICE_DECIMALS)
        .ok_or_else(|| MarketError::Decode(format!("{what} is not a decimal: {text:?}")))
}

/// `{"bid":"65016.06000","ask":"65016.07000","last":"65029.83000",...}`
#[derive(Deserialize)]
struct RawTicker {
    bid: String,
    ask: String,
    last: String,
}

pub fn parse_ticker(body: &str) -> Result<SpotTicker, MarketError> {
    let raw: RawTicker =
        serde_json::from_str(body).map_err(|e| MarketError::Decode(e.to_string()))?;
    ticker(raw)
}

fn ticker(raw: RawTicker) -> Result<SpotTicker, MarketError> {
    Ok(SpotTicker {
        bid: price(&raw.bid, "bid")?,
        ask: price(&raw.ask, "ask")?,
        last: price(&raw.last, "last")?,
    })
}

/// Rows are `[ts_ms, open, high, low, close, volume]` as JSON numbers. They
/// are converted through their decimal text, not through floats. Candles are
/// a discovery signal only and never resolve a challenge.
type RawCandles = Vec<Vec<serde_json::Number>>;

pub fn parse_candles(body: &str) -> Result<Vec<Candle>, MarketError> {
    let raw: RawCandles =
        serde_json::from_str(body).map_err(|e| MarketError::Decode(e.to_string()))?;
    candles(raw)
}

fn candles(raw: RawCandles) -> Result<Vec<Candle>, MarketError> {
    raw.iter()
        .map(|row| {
            if row.len() < 5 {
                return Err(MarketError::Decode("candle row is too short".into()));
            }
            let ts_ms = row[0]
                .as_i64()
                .ok_or_else(|| MarketError::Decode("candle time is not an integer".into()))?;
            let field = |i: usize| price(&row[i].to_string(), "candle price");
            Ok(Candle {
                ts_ms,
                open: field(1)?,
                high: field(2)?,
                low: field(3)?,
                close: field(4)?,
            })
        })
        .collect()
}

#[derive(Deserialize)]
struct RawEventList {
    data: Vec<RawEvent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawEvent {
    ticker: String,
    title: String,
    category: String,
    status: String,
    expiry_date: Option<String>,
    #[serde(default)]
    contracts: Vec<RawContract>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawContract {
    instrument_symbol: String,
    label: String,
    status: String,
    market_state: Option<String>,
    expiry_date: Option<String>,
    #[serde(default)]
    prices: RawPrices,
    resolution_side: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawPrices {
    last_trade_price: Option<String>,
}

fn timestamp(text: Option<&str>) -> Result<Option<i64>, MarketError> {
    text.map(|t| {
        DateTime::parse_from_rfc3339(t)
            .map(|d| d.timestamp())
            .map_err(|_| MarketError::Decode(format!("not a timestamp: {t:?}")))
    })
    .transpose()
}

fn event(raw: RawEvent) -> Result<PmEvent, MarketError> {
    let contracts = raw
        .contracts
        .into_iter()
        .map(|c| {
            let last = c
                .prices
                .last_trade_price
                .as_deref()
                .map(|p| {
                    parse_decimal_scaled(p, BPS_DECIMALS)
                        .filter(|bps| (0..=10_000).contains(bps))
                        .ok_or_else(|| {
                            MarketError::Decode(format!("contract price out of range: {p:?}"))
                        })
                })
                .transpose()?;
            Ok(PmContract {
                instrument_symbol: c.instrument_symbol,
                label: c.label,
                status: c.status,
                market_state: c.market_state,
                expiry_ts: timestamp(c.expiry_date.as_deref())?,
                last_trade_price_bps: last,
                resolution_side: c.resolution_side,
            })
        })
        .collect::<Result<_, MarketError>>()?;
    Ok(PmEvent {
        ticker: raw.ticker,
        title: raw.title,
        category: raw.category,
        status: raw.status,
        expiry_ts: timestamp(raw.expiry_date.as_deref())?,
        contracts,
    })
}

pub fn parse_event(body: &str) -> Result<PmEvent, MarketError> {
    let raw: RawEvent =
        serde_json::from_str(body).map_err(|e| MarketError::Decode(e.to_string()))?;
    event(raw)
}

pub fn parse_event_list(body: &str) -> Result<Vec<PmEvent>, MarketError> {
    let raw: RawEventList =
        serde_json::from_str(body).map_err(|e| MarketError::Decode(e.to_string()))?;
    raw.data.into_iter().map(event).collect()
}

impl MarketData for GeminiClient {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn spot_ticker<'a>(&'a self, symbol: &'a str) -> BoxFut<'a, Result<SpotTicker, MarketError>> {
        Box::pin(async move {
            let symbol = path_segment(symbol)?.to_lowercase();
            ticker(self.get(&format!("/v1/pubticker/{symbol}"), &[]).await?)
        })
    }

    fn spot_candles<'a>(
        &'a self,
        symbol: &'a str,
        frame: TimeFrame,
    ) -> BoxFut<'a, Result<Vec<Candle>, MarketError>> {
        Box::pin(async move {
            let symbol = path_segment(symbol)?.to_lowercase();
            let path = format!("/v2/candles/{symbol}/{}", frame.as_str());
            candles(self.get(&path, &[]).await?)
        })
    }

    fn list_events<'a>(
        &'a self,
        query: &'a EventQuery,
    ) -> BoxFut<'a, Result<Vec<PmEvent>, MarketError>> {
        Box::pin(async move {
            let mut params = vec![("limit", query.limit.clamp(1, 100).to_string())];
            for (name, value) in [
                ("search", &query.search),
                ("category", &query.category),
                ("status", &query.status),
            ] {
                if let Some(value) = value {
                    params.push((name, value.clone()));
                }
            }
            let list: RawEventList = self.get("/v1/prediction-markets/events", &params).await?;
            list.data.into_iter().map(event).collect()
        })
    }

    fn get_event<'a>(&'a self, ticker: &'a str) -> BoxFut<'a, Result<PmEvent, MarketError>> {
        Box::pin(async move {
            let ticker = path_segment(ticker)?;
            event(
                self.get(&format!("/v1/prediction-markets/events/{ticker}"), &[])
                    .await?,
            )
        })
    }
}
