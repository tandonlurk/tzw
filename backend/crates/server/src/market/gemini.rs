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

#[cfg(test)]
mod tests {
    //! No test here reaches the internet: the client talks to a local server
    //! that replays bodies captured from the live API on 2026-11-15 (trimmed
    //! to the fields this client reads, plus a few it must ignore).

    use std::collections::HashMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use aen_core::challenge::{FormulaId, MarketRef};
    use axum::Router;
    use axum::extract::{Path, Query, State};
    use axum::http::StatusCode;
    use axum::routing::get;

    use super::*;
    use crate::market::{discover, observe};

    const TICKER: &str = r#"{"bid":"65016.06000","ask":"65016.07000","last":"65029.83000","volume":{"BTC":"306.59686938","USD":"19937942.2943136054","timestamp":1794775740000}}"#;
    const CANDLES: &str = "[[1794775680000,64993.3,65033.76,64993.3,65033.76,0.09677628],[1794775620000,64962.62,64993.3,64962.62,64993.3,0.00747382],[1794772080000,63758.59,63760,63750.1,63758.59,0.5]]";
    const OPEN_EVENT: &str = r##"{"id":"461869","title":"HYPE price on November 15","slug":"hype-price-on-november-15","type":"categorical","category":"Crypto","ticker":"HYPE2611152200","status":"active","createdAt":"2026-11-14T20:56:28.719Z","contracts":[{"id":"461869-1148971","label":"$59 or above","description":{"data":{},"content":[]},"prices":{"buy":{"yes":"0.99","no":"0.05"},"sell":{"yes":"0.95","no":"0.01"},"bestBid":"0.95","lastTradePrice":"0.99"},"status":"active","ticker":"HI59","instrumentSymbol":"GEMI-HYPE2611152200-HI59","marketState":"open","expiryDate":"2026-11-15T22:00:00.000Z","strike":{"type":"above","value":"59"},"source":"LS-LRRR-HYPE22USD","isCombo":false},{"id":"461869-1148977","label":"$65 or above","prices":{"buy":{"yes":"0.58","no":"0.45"},"sell":{"yes":"0.55","no":"0.42"},"bestBid":"0.55","bestAsk":"0.58","lastTradePrice":"0.56"},"status":"active","ticker":"HI65","instrumentSymbol":"GEMI-HYPE2611152200-HI65","marketState":"open","expiryDate":"2026-11-15T22:00:00.000Z","strike":{"type":"above","value":"65"}}],"isLive":true,"volume":"30028.86","expiryDate":"2026-11-15T22:00:00.000Z","startTime":"2026-11-14T21:00:00.000Z","sourceDetails":{"agency":"lukka","index":"LS-LRRR-HYPE22USD"}}"##;
    const SETTLED_EVENT: &str = r##"{"id":"466649","title":"XRP price today at 3:45pm EST","type":"binary","category":"Crypto","ticker":"XRP15M2611152045","status":"settled","contracts":[{"id":"466649-1164264","label":"Up","prices":{"buy":{},"sell":{}},"status":"settled","ticker":"UP","instrumentSymbol":"GEMI-XRP15M2611152045-UP","marketState":"closed","expiryDate":"2026-11-15T20:45:00.000Z","resolutionSide":"yes","resolvedAt":"2026-11-15T20:45:05.897Z","strike":{"type":"reference","value":"0.95996973755","availableAt":"2026-11-15T20:30:00.000Z"},"settlementValue":"0.9625122158833334"}],"resolvedAt":"2026-11-15T20:45:05.897Z","expiryDate":"2026-11-15T20:45:00.000Z","settlement":{"value":"0.9625122158833334"}}"##;

    #[derive(Default)]
    struct Hits {
        flaky: AtomicU32,
        down: AtomicU32,
        missing: AtomicU32,
        last_query: std::sync::Mutex<HashMap<String, String>>,
    }

    async fn ticker_route(
        State(hits): State<Arc<Hits>>,
        Path(symbol): Path<String>,
    ) -> (StatusCode, String) {
        match symbol.as_str() {
            "btcusd" => (StatusCode::OK, TICKER.into()),
            // Fails twice, then answers.
            "flaky" => {
                if hits.flaky.fetch_add(1, Ordering::SeqCst) < 2 {
                    (StatusCode::INTERNAL_SERVER_ERROR, "oops".into())
                } else {
                    (StatusCode::OK, TICKER.into())
                }
            }
            "down" => {
                hits.down.fetch_add(1, Ordering::SeqCst);
                (StatusCode::SERVICE_UNAVAILABLE, "maintenance".into())
            }
            "slow" => {
                tokio::time::sleep(Duration::from_millis(400)).await;
                (StatusCode::OK, TICKER.into())
            }
            "garbage" => (StatusCode::OK, "<html>not json</html>".into()),
            "badprice" => (
                StatusCode::OK,
                r#"{"bid":"1","ask":"1","last":"NaN"}"#.into(),
            ),
            other => {
                hits.missing.fetch_add(1, Ordering::SeqCst);
                // The live API answers plain text here.
                (
                    StatusCode::NOT_FOUND,
                    format!("'{other}' does not have available data yet"),
                )
            }
        }
    }

    async fn events_route(
        State(hits): State<Arc<Hits>>,
        Query(query): Query<HashMap<String, String>>,
    ) -> String {
        *hits.last_query.lock().unwrap() = query;
        format!(r#"{{"data":[{OPEN_EVENT}],"pagination":{{"limit":20,"offset":0,"total":1}}}}"#)
    }

    async fn event_route(Path(ticker): Path<String>) -> (StatusCode, String) {
        match ticker.as_str() {
            "HYPE2611152200" => (StatusCode::OK, OPEN_EVENT.into()),
            "XRP15M2611152045" => (StatusCode::OK, SETTLED_EVENT.into()),
            _ => (
                StatusCode::NOT_FOUND,
                r#"{"error": "Event not found"}"#.into(),
            ),
        }
    }

    /// Starts the stand-in server and a client with short timings.
    async fn server() -> (GeminiClient, Arc<Hits>) {
        let hits = Arc::new(Hits::default());
        let app = Router::new()
            .route("/v1/pubticker/{symbol}", get(ticker_route))
            .route("/v2/candles/{symbol}/{frame}", get(|| async { CANDLES }))
            .route("/v1/prediction-markets/events", get(events_route))
            .route("/v1/prediction-markets/events/{ticker}", get(event_route))
            .with_state(hits.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let client = GeminiClient::with_options(
            &base,
            Duration::from_millis(150),
            3,
            Duration::from_millis(5),
        )
        .unwrap();
        (client, hits)
    }

    #[test]
    fn parses_observed_shapes_without_floats() {
        let t = parse_ticker(TICKER).unwrap();
        assert_eq!(
            (t.bid, t.ask, t.last),
            (65_016_060_000, 65_016_070_000, 65_029_830_000)
        );

        let c = parse_candles(CANDLES).unwrap();
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].ts_ms, 1_794_775_680_000);
        assert_eq!((c[0].open, c[0].close), (64_993_300_000, 65_033_760_000));

        let e = parse_event(OPEN_EVENT).unwrap();
        assert_eq!(
            (e.ticker.as_str(), e.category.as_str(), e.status.as_str()),
            ("HYPE2611152200", "Crypto", "active")
        );
        assert_eq!(e.expiry_ts, Some(1_794_780_000));
        assert_eq!(e.contracts.len(), 2);
        assert_eq!(e.contracts[0].instrument_symbol, "GEMI-HYPE2611152200-HI59");
        assert_eq!(e.contracts[0].last_trade_price_bps, Some(9900));
        assert_eq!(e.contracts[0].market_state.as_deref(), Some("open"));
        assert_eq!(e.contracts[1].last_trade_price_bps, Some(5600));

        let s = parse_event(SETTLED_EVENT).unwrap();
        assert_eq!(s.contracts[0].resolution_side.as_deref(), Some("yes"));
        // A settled contract has empty price objects: no price, not zero.
        assert_eq!(s.contracts[0].last_trade_price_bps, None);

        let list = format!(r#"{{"data":[{OPEN_EVENT},{SETTLED_EVENT}]}}"#);
        assert_eq!(parse_event_list(&list).unwrap().len(), 2);
    }

    #[test]
    fn malformed_bodies_are_decode_errors() {
        for bad in [
            "",
            "[]",
            r#"{"bid":"1"}"#,
            r#"{"bid":"1","ask":"1","last":"1e5"}"#,
        ] {
            assert!(
                matches!(parse_ticker(bad), Err(MarketError::Decode(_))),
                "{bad}"
            );
        }
        assert!(parse_candles("[[1,2]]").is_err());
        assert!(parse_candles(r#"[["a",1,2,3,4,5]]"#).is_err());
        assert!(parse_event(r#"{"ticker":"X"}"#).is_err());
        let over = OPEN_EVENT.replace(r#""lastTradePrice":"0.99""#, r#""lastTradePrice":"1.50""#);
        assert!(matches!(parse_event(&over), Err(MarketError::Decode(_))));
        let bad_date = OPEN_EVENT.replace("2026-11-15T22:00:00.000Z", "tomorrow");
        assert!(parse_event(&bad_date).is_err());
    }

    #[tokio::test]
    async fn reads_spot_and_prediction_markets_over_http() {
        let (client, hits) = server().await;
        assert_eq!(
            client.spot_ticker("BTCUSD").await.unwrap().last,
            65_029_830_000
        );
        assert_eq!(
            client
                .spot_candles("btcusd", TimeFrame::OneMinute)
                .await
                .unwrap()
                .len(),
            3
        );

        let query = EventQuery {
            search: Some("HYPE".into()),
            category: Some("Crypto".into()),
            status: Some("active".into()),
            limit: 20,
        };
        let events = client.list_events(&query).await.unwrap();
        assert_eq!(events.len(), 1);
        let sent = hits.last_query.lock().unwrap().clone();
        assert_eq!(sent["search"], "HYPE");
        assert_eq!(sent["category"], "Crypto");
        assert_eq!(sent["status"], "active");
        assert_eq!(sent["limit"], "20");
        assert!(!sent.contains_key("offset"));

        let event = client.get_event("XRP15M2611152045").await.unwrap();
        assert_eq!(event.status, "settled");
    }

    #[tokio::test]
    async fn discovery_and_resolution_work_on_real_shapes() {
        let (client, _) = server().await;
        // The contested contract (0.56) is chosen over the one-sided one (0.99).
        let view = discover(&client, "hypeusd", 3600, 1_794_700_000)
            .await
            .unwrap();
        assert_eq!(
            view.market,
            MarketRef::GeminiPredictionContract {
                event_ticker: "HYPE2611152200".into(),
                instrument_symbol: "GEMI-HYPE2611152200-HI65".into(),
            }
        );
        assert_eq!(view.title, "HYPE price on November 15: $65 or above");
        assert_eq!((view.value, view.source.as_str()), (5600, "gemini"));
        // 63758.59 -> 65033.76 over the hour.
        assert_eq!(view.move_bps, 199);
        assert_eq!(view.fallback_reason, None);

        // An event that expires before the challenge could be funded is not
        // used: discovery falls back to the spot price and says why.
        let view = discover(&client, "btcusd", 3600, 1_794_780_001)
            .await
            .unwrap();
        assert_eq!(
            view.market,
            MarketRef::GeminiSpot {
                symbol: "btcusd".into()
            }
        );
        assert_eq!(view.value, 65_029_830_000);
        assert!(view.fallback_reason.is_some());
    }

    #[tokio::test]
    async fn observes_open_price_and_published_resolution() {
        let (client, _) = server().await;
        let open = MarketRef::GeminiPredictionContract {
            event_ticker: "HYPE2611152200".into(),
            instrument_symbol: "GEMI-HYPE2611152200-HI65".into(),
        };
        let obs = observe(&client, &open, FormulaId::PmYesPriceBpsV1)
            .await
            .unwrap();
        assert_eq!((obs.value, obs.source), (5600, "gemini"));

        let settled = MarketRef::GeminiPredictionContract {
            event_ticker: "XRP15M2611152045".into(),
            instrument_symbol: "GEMI-XRP15M2611152045-UP".into(),
        };
        let obs = observe(&client, &settled, FormulaId::PmYesPriceBpsV1)
            .await
            .unwrap();
        assert_eq!(obs.value, 10_000);

        let unknown_contract = MarketRef::GeminiPredictionContract {
            event_ticker: "HYPE2611152200".into(),
            instrument_symbol: "GEMI-HYPE2611152200-NOPE".into(),
        };
        let err = observe(&client, &unknown_contract, FormulaId::PmYesPriceBpsV1)
            .await
            .unwrap_err();
        assert!(matches!(err, MarketError::NoData(_)));

        let spot = MarketRef::GeminiSpot {
            symbol: "btcusd".into(),
        };
        let obs = observe(&client, &spot, FormulaId::SpotLastPriceMicrosV1)
            .await
            .unwrap();
        assert_eq!(obs.value, 65_029_830_000);
    }

    #[tokio::test]
    async fn transient_failures_are_retried_with_backoff() {
        let (client, hits) = server().await;
        // Two 500s, then success on the third and last attempt.
        assert_eq!(
            client.spot_ticker("flaky").await.unwrap().last,
            65_029_830_000
        );
        assert_eq!(hits.flaky.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn persistent_failures_surface_after_the_last_attempt() {
        let (client, hits) = server().await;
        assert_eq!(
            client.spot_ticker("down").await.unwrap_err(),
            MarketError::Http(503)
        );
        assert_eq!(hits.down.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn not_found_is_not_retried() {
        let (client, hits) = server().await;
        assert_eq!(
            client.spot_ticker("nopeusd").await.unwrap_err(),
            MarketError::NotFound
        );
        assert_eq!(hits.missing.load(Ordering::SeqCst), 1);
        assert_eq!(
            client.get_event("NOPE123").await.unwrap_err(),
            MarketError::NotFound
        );
    }

    #[tokio::test]
    async fn slow_and_malformed_answers_are_errors() {
        let (client, _) = server().await;
        assert_eq!(
            client.spot_ticker("slow").await.unwrap_err(),
            MarketError::Timeout
        );
        assert!(matches!(
            client.spot_ticker("garbage").await,
            Err(MarketError::Decode(_))
        ));
        assert!(matches!(
            client.spot_ticker("badprice").await,
            Err(MarketError::Decode(_))
        ));
    }

    #[tokio::test]
    async fn unreachable_host_and_bad_identifiers_are_errors() {
        // Port 9 (discard) on loopback: connection refused, no internet.
        let client = GeminiClient::with_options(
            "http://127.0.0.1:9",
            Duration::from_millis(150),
            2,
            Duration::from_millis(1),
        )
        .unwrap();
        let err = client.spot_ticker("btcusd").await.unwrap_err();
        assert!(
            matches!(err, MarketError::Transport(_) | MarketError::Timeout),
            "{err:?}"
        );

        let (client, _) = server().await;
        for bad in ["", "btc/usd", "../v1/order", "btc usd", "btc?x=1"] {
            assert!(
                matches!(client.spot_ticker(bad).await, Err(MarketError::NoData(_))),
                "{bad}"
            );
            assert!(
                matches!(client.get_event(bad).await, Err(MarketError::NoData(_))),
                "{bad}"
            );
        }
    }
}
