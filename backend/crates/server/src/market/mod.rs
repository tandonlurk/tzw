//! Market data from the Gemini crypto exchange (never Google's AI models).
//!
//! `MarketData` returns only fields observed in live responses (see
//! `docs/research/gemini-prediction-markets.md`). `discover` and `observe`
//! are built on the trait, so the deterministic fake exercises the same
//! selection and resolution code as the real client.
//!
//! A market data API is not an oracle: resolution read from here is
//! backend-administered.

pub mod fake;
pub mod gemini;

use std::future::Future;
use std::pin::Pin;

use aen_core::challenge::{FormulaId, MarketRef};
use serde::Serialize;

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MarketError {
    #[error("market data request timed out")]
    Timeout,
    #[error("market data request failed: {0}")]
    Transport(String),
    #[error("market data source answered HTTP {0}")]
    Http(u16),
    #[error("market not found")]
    NotFound,
    #[error("unexpected market data response: {0}")]
    Decode(String),
    #[error("no usable market data: {0}")]
    NoData(String),
}

impl MarketError {
    /// Whether trying again may help. 4xx answers (other than 429) will not.
    pub fn is_retryable(&self) -> bool {
        match self {
            MarketError::Timeout | MarketError::Transport(_) => true,
            MarketError::Http(status) => *status == 429 || *status >= 500,
            MarketError::NotFound | MarketError::Decode(_) | MarketError::NoData(_) => false,
        }
    }
}

/// `GET /v1/pubticker/{symbol}`. Prices are micro-USD (6 decimals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpotTicker {
    pub bid: i64,
    pub ask: i64,
    pub last: i64,
}

/// One row of `GET /v2/candles/{symbol}/{time_frame}`, micro-USD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candle {
    pub ts_ms: i64,
    pub open: i64,
    pub high: i64,
    pub low: i64,
    pub close: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeFrame {
    OneMinute,
    OneHour,
}

impl TimeFrame {
    pub const fn as_str(self) -> &'static str {
        match self {
            TimeFrame::OneMinute => "1m",
            TimeFrame::OneHour => "1hr",
        }
    }
}

/// A prediction-market event (`/v1/prediction-markets/events`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PmEvent {
    pub ticker: String,
    pub title: String,
    pub category: String,
    pub status: String,
    /// Unix seconds.
    pub expiry_ts: Option<i64>,
    pub contracts: Vec<PmContract>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PmContract {
    pub instrument_symbol: String,
    pub label: String,
    pub status: String,
    pub market_state: Option<String>,
    pub expiry_ts: Option<i64>,
    /// `prices.lastTradePrice` in basis points of 1.00; absent when the
    /// contract has not traded or is settled.
    pub last_trade_price_bps: Option<i64>,
    /// `"yes"` or `"no"` once the contract is settled.
    pub resolution_side: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventQuery {
    pub search: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub limit: u32,
}

pub trait MarketData: Send + Sync {
    /// `"gemini"` or `"fake"`; stored with observations so simulated data is
    /// never mistaken for exchange data.
    fn name(&self) -> &'static str;
    fn spot_ticker<'a>(&'a self, symbol: &'a str) -> BoxFut<'a, Result<SpotTicker, MarketError>>;
    /// Newest first.
    fn spot_candles<'a>(
        &'a self,
        symbol: &'a str,
        frame: TimeFrame,
    ) -> BoxFut<'a, Result<Vec<Candle>, MarketError>>;
    fn list_events<'a>(
        &'a self,
        query: &'a EventQuery,
    ) -> BoxFut<'a, Result<Vec<PmEvent>, MarketError>>;
    fn get_event<'a>(&'a self, ticker: &'a str) -> BoxFut<'a, Result<PmEvent, MarketError>>;
}

/// What an agent sees when it looks for an opportunity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarketView {
    pub market: MarketRef,
    pub title: String,
    /// Lower-case, e.g. `crypto`.
    pub category: String,
    /// Current observation in the unit of the market's formula.
    pub value: i64,
    /// Signed move of the underlying spot price over `window_secs`, in basis
    /// points. A discovery signal only; never used for resolution.
    pub move_bps: i64,
    pub window_secs: i64,
    pub spot_symbol: String,
    pub source: &'static str,
    /// Set when no prediction-market contract was usable and the view fell
    /// back to the spot price.
    pub fallback_reason: Option<String>,
}

/// The value a challenge resolves on, with what it was read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Observation {
    pub value: i64,
    pub source: &'static str,
    pub detail: String,
}

/// A contract is worth disagreeing about only while its YES price is away
/// from both ends.
const MIN_OPEN_PRICE_BPS: i64 = 500;
const MAX_OPEN_PRICE_BPS: i64 = 9500;
const EVENT_PAGE: u32 = 20;

/// Signed move of the latest close against the close `window_secs` earlier.
pub fn move_bps(candles: &[Candle], window_secs: i64) -> Result<i64, MarketError> {
    let latest = candles
        .iter()
        .max_by_key(|c| c.ts_ms)
        .ok_or_else(|| MarketError::NoData("no candles".into()))?;
    let cutoff = latest.ts_ms - window_secs.saturating_mul(1000);
    // The newest candle at or before the cutoff, else the oldest we have.
    let reference = candles
        .iter()
        .filter(|c| c.ts_ms <= cutoff)
        .max_by_key(|c| c.ts_ms)
        .or_else(|| candles.iter().min_by_key(|c| c.ts_ms))
        .ok_or_else(|| MarketError::NoData("no candles".into()))?;
    if reference.close <= 0 {
        return Err(MarketError::NoData("reference price is zero".into()));
    }
    let diff = i128::from(latest.close) - i128::from(reference.close);
    Ok((diff * 10_000 / i128::from(reference.close)) as i64)
}

/// `BTCUSD` -> `BTC`.
fn base_asset(symbol: &str) -> String {
    let upper = symbol.to_uppercase();
    upper.strip_suffix("USD").unwrap_or(&upper).to_owned()
}

/// Looks at `symbol`: the spot move over the window, then the most contested
/// open prediction-market contract on the same asset that expires after
/// `min_expiry_ts`. Falls back to the spot price (and says so) when no
/// contract is usable.
pub async fn discover(
    market: &dyn MarketData,
    symbol: &str,
    window_secs: i64,
    min_expiry_ts: i64,
) -> Result<MarketView, MarketError> {
    let frame = if window_secs <= 86_400 {
        TimeFrame::OneMinute
    } else {
        TimeFrame::OneHour
    };
    let candles = market.spot_candles(symbol, frame).await?;
    let moved = move_bps(&candles, window_secs)?;
    let base = base_asset(symbol);

    let query = EventQuery {
        search: Some(base.clone()),
        category: Some("Crypto".into()),
        status: Some("active".into()),
        limit: EVENT_PAGE,
    };
    let fallback_reason = match market.list_events(&query).await {
        Ok(events) => {
            let mut usable: Vec<&PmEvent> = events
                .iter()
                .filter(|e| e.ticker.starts_with(&base) && e.status == "active")
                .filter(|e| e.expiry_ts.is_none_or(|t| t >= min_expiry_ts))
                .collect();
            usable.sort_by_key(|e| (e.expiry_ts.unwrap_or(i64::MAX), e.ticker.clone()));
            let pick = usable.iter().find_map(|event| {
                event
                    .contracts
                    .iter()
                    .filter(|c| c.status == "active" && c.resolution_side.is_none())
                    .filter_map(|c| Some((c, c.last_trade_price_bps?)))
                    .filter(|(_, p)| (MIN_OPEN_PRICE_BPS..=MAX_OPEN_PRICE_BPS).contains(p))
                    .min_by_key(|(c, p)| ((p - 5000).abs(), c.instrument_symbol.clone()))
                    .map(|(c, p)| (*event, c, p))
            });
            match pick {
                Some((event, contract, price)) => {
                    return Ok(MarketView {
                        market: MarketRef::GeminiPredictionContract {
                            event_ticker: event.ticker.clone(),
                            instrument_symbol: contract.instrument_symbol.clone(),
                        },
                        title: format!("{}: {}", event.title, contract.label),
                        category: event.category.to_lowercase(),
                        value: price,
                        move_bps: moved,
                        window_secs,
                        spot_symbol: symbol.to_lowercase(),
                        source: market.name(),
                        fallback_reason: None,
                    });
                }
                None => format!("no contested open prediction-market contract for {base}"),
            }
        }
        Err(e) => format!("prediction markets unavailable: {e}"),
    };

    let ticker = market.spot_ticker(symbol).await?;
    Ok(MarketView {
        market: MarketRef::GeminiSpot {
            symbol: symbol.to_lowercase(),
        },
        title: format!("{} spot price", symbol.to_uppercase()),
        category: "crypto".into(),
        value: ticker.last,
        move_bps: moved,
        window_secs,
        spot_symbol: symbol.to_lowercase(),
        source: market.name(),
        fallback_reason: Some(fallback_reason),
    })
}

/// Reads the resolving observation for `market` under `formula`. Errors are
/// returned, never replaced by a default value.
pub async fn observe(
    source: &dyn MarketData,
    market: &MarketRef,
    formula: FormulaId,
) -> Result<Observation, MarketError> {
    match (market, formula) {
        (
            MarketRef::GeminiPredictionContract {
                event_ticker,
                instrument_symbol,
            },
            FormulaId::PmYesPriceBpsV1,
        ) => {
            let event = source.get_event(event_ticker).await?;
            let contract = event
                .contracts
                .iter()
                .find(|c| &c.instrument_symbol == instrument_symbol)
                .ok_or_else(|| {
                    MarketError::NoData(format!("contract {instrument_symbol} not in event"))
                })?;
            let (value, detail) = match contract.resolution_side.as_deref() {
                Some("yes") => (10_000, "contract resolved yes".to_owned()),
                Some("no") => (0, "contract resolved no".to_owned()),
                Some(other) => {
                    return Err(MarketError::NoData(format!(
                        "unknown resolution side {other}"
                    )));
                }
                None => match contract.last_trade_price_bps {
                    Some(p) => (p, "last trade price of the open contract".to_owned()),
                    None => {
                        return Err(MarketError::NoData(
                            "contract has no last trade price".into(),
                        ));
                    }
                },
            };
            Ok(Observation {
                value,
                source: source.name(),
                detail,
            })
        }
        (MarketRef::GeminiSpot { symbol }, FormulaId::SpotLastPriceMicrosV1) => {
            let ticker = source.spot_ticker(symbol).await?;
            Ok(Observation {
                value: ticker.last,
                source: source.name(),
                detail: "spot last price".to_owned(),
            })
        }
        _ => Err(MarketError::NoData(
            "formula does not apply to this market".into(),
        )),
    }
}
