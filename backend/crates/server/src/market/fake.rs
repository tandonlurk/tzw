//! Deterministic in-memory market for tests and offline demos. Everything it
//! returns is labelled `fake`; it never touches the network.

use std::sync::Mutex;

use serde::Deserialize;

use super::{
    BoxFut, Candle, EventQuery, MarketData, MarketError, PmContract, PmEvent, SpotTicker, TimeFrame,
};

pub const FAKE_EVENT_TICKER: &str = "BTC-FAKE-1H";
pub const FAKE_INSTRUMENT: &str = "FAKE-BTC-1H-UP";

/// Reference spot price: 65,000 USD in micro-USD.
const REFERENCE_PRICE: i64 = 65_000_000_000;
const LATEST_TS_MS: i64 = 2_000_000_000_000;
/// Older than any trigger window, so it is always the reference candle.
const REFERENCE_AGE_MS: i64 = 400 * 86_400 * 1000;

#[derive(Debug)]
struct State {
    move_bps: i64,
    yes_price_bps: Option<i64>,
    resolution_side: Option<String>,
    prediction_markets: bool,
    fail: bool,
    calls: u64,
}

#[derive(Debug)]
pub struct FakeMarket {
    state: Mutex<State>,
}

/// Body of `POST /api/dev/fake-market`: only the fields present change.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FakeMarketUpdate {
    /// Spot move over any window, in basis points.
    pub move_bps: Option<i64>,
    /// YES price of the fake contract in basis points (0-10000).
    pub yes_price_bps: Option<i64>,
    /// `"yes"` / `"no"` settles the fake contract; `"open"` reopens it.
    pub resolution_side: Option<String>,
    /// `false` removes the fake event so discovery falls back to spot.
    pub prediction_markets: Option<bool>,
    /// `true` makes every call fail.
    pub fail: Option<bool>,
}

impl FakeMarket {
    /// BTC up 3% over the window; one open contract trading at 0.55.
    pub fn demo() -> FakeMarket {
        FakeMarket {
            state: Mutex::new(State {
                move_bps: 300,
                yes_price_bps: Some(5500),
                resolution_side: None,
                prediction_markets: true,
                fail: false,
                calls: 0,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Applies an update; `Err` names the invalid field.
    pub fn update(&self, update: &FakeMarketUpdate) -> Result<(), &'static str> {
        if update
            .move_bps
            .is_some_and(|m| !(-9000..=100_000).contains(&m))
        {
            return Err("move_bps");
        }
        if update
            .yes_price_bps
            .is_some_and(|p| !(0..=10_000).contains(&p))
        {
            return Err("yes_price_bps");
        }
        let side = match update.resolution_side.as_deref() {
            None => None,
            Some("open") => Some(None),
            Some(s @ ("yes" | "no")) => Some(Some(s.to_owned())),
            Some(_) => return Err("resolution_side"),
        };
        let mut state = self.lock();
        if let Some(m) = update.move_bps {
            state.move_bps = m;
        }
        if let Some(p) = update.yes_price_bps {
            state.yes_price_bps = Some(p);
        }
        if let Some(side) = side {
            state.resolution_side = side;
        }
        if let Some(pm) = update.prediction_markets {
            state.prediction_markets = pm;
        }
        if let Some(fail) = update.fail {
            state.fail = fail;
        }
        Ok(())
    }

    pub fn set_move_bps(&self, move_bps: i64) {
        self.lock().move_bps = move_bps;
    }

    /// `None` = the contract has no last trade price.
    pub fn set_yes_price(&self, bps: Option<i64>) {
        self.lock().yes_price_bps = bps;
    }

    pub fn set_resolution(&self, side: Option<&str>) {
        self.lock().resolution_side = side.map(str::to_owned);
    }

    pub fn set_prediction_markets(&self, on: bool) {
        self.lock().prediction_markets = on;
    }

    pub fn set_fail(&self, fail: bool) {
        self.lock().fail = fail;
    }

    /// Number of calls made, including failed ones.
    pub fn calls(&self) -> u64 {
        self.lock().calls
    }

    fn enter(&self) -> Result<std::sync::MutexGuard<'_, State>, MarketError> {
        let mut state = self.lock();
        state.calls += 1;
        if state.fail {
            Err(MarketError::Transport("fake outage".into()))
        } else {
            Ok(state)
        }
    }

    fn last_price(state: &State) -> i64 {
        REFERENCE_PRICE + REFERENCE_PRICE / 10_000 * state.move_bps
    }

    fn event(state: &State) -> PmEvent {
        let settled = state.resolution_side.is_some();
        PmEvent {
            ticker: FAKE_EVENT_TICKER.into(),
            title: "[fake] BTC price in one hour".into(),
            category: "Crypto".into(),
            status: if settled { "settled" } else { "active" }.into(),
            expiry_ts: None,
            contracts: vec![PmContract {
                instrument_symbol: FAKE_INSTRUMENT.into(),
                label: "Up".into(),
                status: if settled { "settled" } else { "active" }.into(),
                market_state: Some(if settled { "closed" } else { "open" }.into()),
                expiry_ts: None,
                last_trade_price_bps: if settled { None } else { state.yes_price_bps },
                resolution_side: state.resolution_side.clone(),
            }],
            meta: super::MarketMeta {
                subcategories: vec!["bitcoin".into(), "Bitcoin".into()],
                tags: vec!["BTC".into(), "Hourly".into()],
                structure: Some(aen_core::approach::MarketStructure::BinaryUpDown),
                expiry_ts: None,
                volume_delta_24h_bps: None,
            },
        }
    }
}

impl MarketData for FakeMarket {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn spot_ticker<'a>(&'a self, _symbol: &'a str) -> BoxFut<'a, Result<SpotTicker, MarketError>> {
        Box::pin(async move {
            let state = self.enter()?;
            let last = Self::last_price(&state);
            Ok(SpotTicker {
                bid: last,
                ask: last,
                last,
            })
        })
    }

    fn spot_candles<'a>(
        &'a self,
        _symbol: &'a str,
        _frame: TimeFrame,
    ) -> BoxFut<'a, Result<Vec<Candle>, MarketError>> {
        Box::pin(async move {
            let state = self.enter()?;
            let flat = |ts_ms, price| Candle {
                ts_ms,
                open: price,
                high: price,
                low: price,
                close: price,
            };
            Ok(vec![
                flat(LATEST_TS_MS, Self::last_price(&state)),
                flat(LATEST_TS_MS - REFERENCE_AGE_MS, REFERENCE_PRICE),
            ])
        })
    }

    fn list_events<'a>(
        &'a self,
        _query: &'a EventQuery,
    ) -> BoxFut<'a, Result<Vec<PmEvent>, MarketError>> {
        Box::pin(async move {
            let state = self.enter()?;
            Ok(if state.prediction_markets {
                vec![Self::event(&state)]
            } else {
                Vec::new()
            })
        })
    }

    fn get_event<'a>(&'a self, ticker: &'a str) -> BoxFut<'a, Result<PmEvent, MarketError>> {
        Box::pin(async move {
            let state = self.enter()?;
            if state.prediction_markets && ticker == FAKE_EVENT_TICKER {
                Ok(Self::event(&state))
            } else {
                Err(MarketError::NotFound)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use aen_core::challenge::{FormulaId, MarketRef};

    use super::*;
    use crate::market::{discover, observe};

    fn contract() -> MarketRef {
        MarketRef::GeminiPredictionContract {
            event_ticker: FAKE_EVENT_TICKER.into(),
            instrument_symbol: FAKE_INSTRUMENT.into(),
        }
    }

    #[tokio::test]
    async fn discover_prefers_the_prediction_market_contract() {
        let fake = FakeMarket::demo();
        let view = discover(&fake, "BTCUSD", 3600, 0).await.unwrap();
        assert_eq!(view.market, contract());
        assert_eq!(view.value, 5500);
        assert_eq!(view.move_bps, 300);
        assert_eq!(view.source, "fake");
        assert_eq!(view.fallback_reason, None);
    }

    #[tokio::test]
    async fn discover_falls_back_to_spot_and_says_so() {
        let fake = FakeMarket::demo();
        fake.set_prediction_markets(false);
        let view = discover(&fake, "BTCUSD", 3600, 0).await.unwrap();
        assert_eq!(
            view.market,
            MarketRef::GeminiSpot {
                symbol: "btcusd".into()
            }
        );
        assert_eq!(view.value, 66_950_000_000);
        assert!(view.fallback_reason.is_some());

        // A contract priced at an extreme is not worth a challenge either.
        fake.set_prediction_markets(true);
        fake.set_yes_price(Some(9900));
        let view = discover(&fake, "BTCUSD", 3600, 0).await.unwrap();
        assert!(matches!(view.market, MarketRef::GeminiSpot { .. }));
    }

    #[tokio::test]
    async fn observe_reads_price_then_resolution() {
        let fake = FakeMarket::demo();
        let formula = FormulaId::PmYesPriceBpsV1;
        assert_eq!(
            observe(&fake, &contract(), formula).await.unwrap().value,
            5500
        );
        fake.set_resolution(Some("yes"));
        assert_eq!(
            observe(&fake, &contract(), formula).await.unwrap().value,
            10_000
        );
        fake.set_resolution(Some("no"));
        assert_eq!(observe(&fake, &contract(), formula).await.unwrap().value, 0);
    }

    #[tokio::test]
    async fn missing_data_is_an_error_not_a_default() {
        let fake = FakeMarket::demo();
        fake.set_yes_price(None);
        let err = observe(&fake, &contract(), FormulaId::PmYesPriceBpsV1)
            .await
            .unwrap_err();
        assert!(matches!(err, MarketError::NoData(_)), "{err:?}");

        fake.set_fail(true);
        let err = observe(&fake, &contract(), FormulaId::PmYesPriceBpsV1)
            .await
            .unwrap_err();
        assert!(matches!(err, MarketError::Transport(_)), "{err:?}");
        assert!(discover(&fake, "BTCUSD", 3600, 0).await.is_err());

        // A formula that does not fit the market is refused.
        fake.set_fail(false);
        let err = observe(&fake, &contract(), FormulaId::SpotLastPriceMicrosV1)
            .await
            .unwrap_err();
        assert!(matches!(err, MarketError::NoData(_)), "{err:?}");
    }

    #[test]
    fn update_validates_fields() {
        let fake = FakeMarket::demo();
        let bad = FakeMarketUpdate {
            yes_price_bps: Some(10_001),
            ..Default::default()
        };
        assert_eq!(fake.update(&bad), Err("yes_price_bps"));
        let bad = FakeMarketUpdate {
            resolution_side: Some("maybe".into()),
            ..Default::default()
        };
        assert_eq!(fake.update(&bad), Err("resolution_side"));
    }
}
