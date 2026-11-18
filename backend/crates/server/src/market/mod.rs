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

use aen_core::approach::{MarketFacts, MarketStructure};
use aen_core::challenge::{FormulaId, MarketRef};
use serde::{Deserialize, Serialize};

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
    /// Optional listing fields the approach market filter reads.
    pub meta: MarketMeta,
}

/// What the approach market filter needs beyond the basics. Every field is
/// optional: Gemini fields were observed live but are not all documented,
/// so an unreadable one is left empty rather than failing the listing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MarketMeta {
    /// Subcategory slug and name, e.g. `["energy", "Energy"]`.
    pub subcategories: Vec<String>,
    /// `tags` and `eventTags` names.
    pub tags: Vec<String>,
    pub structure: Option<MarketStructure>,
    /// Unix seconds.
    pub expiry_ts: Option<i64>,
    /// `volumeDelta24hPct` in basis points (12.5 % -> 1250).
    pub volume_delta_24h_bps: Option<i64>,
}

/// `crypto-up-down` -> binary up/down, `prices-ladder` -> price ladder,
/// `type: categorical` -> categorical; anything else is unknown.
pub fn structure_of(event_type: Option<&str>, template: Option<&str>) -> Option<MarketStructure> {
    let template = template.unwrap_or_default().to_ascii_lowercase();
    if template.contains("up-down") {
        Some(MarketStructure::BinaryUpDown)
    } else if template.contains("ladder") {
        Some(MarketStructure::PriceLadder)
    } else if event_type.is_some_and(|t| t.eq_ignore_ascii_case("categorical")) {
        Some(MarketStructure::Categorical)
    } else {
        None
    }
}

/// The base asset an event is about: the leading letters of its ticker
/// (`BTC1H2511...` -> `BTC`, `OIL-...` -> `OIL`). A heuristic; `None` when the
/// ticker does not start with a letter.
pub fn ticker_symbol(ticker: &str) -> Option<String> {
    let base: String = ticker
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    (!base.is_empty()).then(|| base.to_ascii_uppercase())
}

/// One prediction-market contract as the approach filter sees it.
pub fn contract_facts(
    event: &PmEvent,
    contract: &PmContract,
    move_bps: Option<i64>,
    now: i64,
) -> MarketFacts {
    let expiry = contract
        .expiry_ts
        .or(event.expiry_ts)
        .or(event.meta.expiry_ts);
    MarketFacts {
        category: event.category.clone(),
        subcategories: event.meta.subcategories.clone(),
        structure: event.meta.structure,
        title: format!("{}: {}", event.title, contract.label),
        tickers: vec![event.ticker.clone(), contract.instrument_symbol.clone()],
        tags: event.meta.tags.clone(),
        symbol: ticker_symbol(&event.ticker),
        move_bps,
        yes_price_bps: contract.quote_bps(),
        expiry_secs: expiry.map(|t| t - now),
        volume_delta_24h_bps: event.meta.volume_delta_24h_bps,
    }
}

impl MarketView {
    /// The view as the approach filter sees it (discovery gate).
    pub fn facts(&self, now: i64) -> MarketFacts {
        let mut tickers = Vec::new();
        let mut yes_price_bps = None;
        match &self.market {
            MarketRef::GeminiPredictionContract {
                event_ticker,
                instrument_symbol,
            } => {
                tickers.push(event_ticker.clone());
                tickers.push(instrument_symbol.clone());
                yes_price_bps = Some(self.value);
            }
            MarketRef::GeminiSpot { symbol } => tickers.push(symbol.clone()),
        }
        MarketFacts {
            category: self.category.clone(),
            subcategories: self.meta.subcategories.clone(),
            structure: self.meta.structure,
            title: self.title.clone(),
            tickers,
            tags: self.meta.tags.clone(),
            symbol: Some(base_asset(&self.spot_symbol)),
            move_bps: Some(self.move_bps),
            yes_price_bps,
            expiry_secs: self.meta.expiry_ts.map(|t| t - now),
            volume_delta_24h_bps: self.meta.volume_delta_24h_bps,
        }
    }
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
    /// `prices.bestBid` / `prices.bestAsk` (YES side) in basis points; absent
    /// when that side of the book is empty.
    pub best_bid_bps: Option<i64>,
    pub best_ask_bps: Option<i64>,
    /// `"yes"` or `"no"` once the contract is settled.
    pub resolution_side: Option<String>,
}

impl PmContract {
    /// The YES price discovery and the approach filter use (D-67): the
    /// bid/ask mid (rounded down) when both sides are quoted and the book is
    /// not crossed, else the last trade. A last trade can be hours old on a
    /// thin contract; the live book is the market's current price. Resolution
    /// still reads the last trade (`PmYesPriceBpsV1`), unchanged.
    pub fn quote_bps(&self) -> Option<i64> {
        match (self.best_bid_bps, self.best_ask_bps) {
            (Some(bid), Some(ask)) if bid <= ask => Some((bid + ask) / 2),
            _ => self.last_trade_price_bps,
        }
    }
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// `gemini` or `fake`.
    pub source: String,
    /// Set when no prediction-market contract was usable and the view fell
    /// back to the spot price.
    pub fallback_reason: Option<String>,
    /// Listing details for the approach filter (empty for spot views and
    /// views stored before they existed).
    #[serde(default)]
    pub meta: MarketMeta,
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
pub(crate) const MIN_OPEN_PRICE_BPS: i64 = 500;
pub(crate) const MAX_OPEN_PRICE_BPS: i64 = 9500;
/// Listings come back soonest-expiring first and an asset has dozens of
/// 5 and 15 minute events, so a small page never reaches the longer-dated
/// contracts. 100 was accepted live; a BTC search returned 24 events.
const EVENT_PAGE: u32 = 100;

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
                    .filter_map(|c| Some((c, c.quote_bps()?)))
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
                        source: market.name().to_owned(),
                        fallback_reason: None,
                        meta: MarketMeta {
                            expiry_ts: contract.expiry_ts.or(event.expiry_ts),
                            ..event.meta.clone()
                        },
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
        source: market.name().to_owned(),
        fallback_reason: Some(fallback_reason),
        meta: MarketMeta::default(),
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

#[cfg(test)]
mod quote_tests {
    use super::PmContract;

    fn contract(last: Option<i64>, bid: Option<i64>, ask: Option<i64>) -> PmContract {
        PmContract {
            instrument_symbol: "X".into(),
            label: "X".into(),
            status: "active".into(),
            market_state: Some("open".into()),
            expiry_ts: None,
            last_trade_price_bps: last,
            best_bid_bps: bid,
            best_ask_bps: ask,
            resolution_side: None,
        }
    }

    #[test]
    fn quote_prefers_the_book_mid_over_a_stale_last_trade() {
        // The 2026-11-17 testnet run: last trade 0.14, book 0.34/0.41.
        assert_eq!(
            contract(Some(1400), Some(3400), Some(4100)).quote_bps(),
            Some(3750)
        );
        // Rounded down.
        assert_eq!(
            contract(None, Some(5500), Some(5800)).quote_bps(),
            Some(5650)
        );
        // One side missing, or a crossed book: the last trade (or nothing).
        assert_eq!(
            contract(Some(1400), Some(3400), None).quote_bps(),
            Some(1400)
        );
        assert_eq!(
            contract(Some(1400), None, Some(4100)).quote_bps(),
            Some(1400)
        );
        assert_eq!(
            contract(Some(1400), Some(4200), Some(4100)).quote_bps(),
            Some(1400)
        );
        assert_eq!(contract(None, None, None).quote_bps(), None);
    }
}
