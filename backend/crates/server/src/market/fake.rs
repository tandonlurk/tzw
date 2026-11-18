//! Deterministic in-memory market for tests and offline demos. Everything it
//! returns is labelled `fake`; it never touches the network.
//!
//! One controllable BTC event (`/api/dev/fake-market` moves it) plus a fixed
//! catalog across categories (crypto, commodities/energy, metals, economics,
//! sports) so the approach market filter can be shown offline. Catalog
//! prices never change and catalog events never settle.

use std::sync::Mutex;

use aen_core::approach::MarketStructure;
use serde::Deserialize;

use super::{
    BoxFut, Candle, EventQuery, MarketData, MarketError, PmContract, PmEvent, SpotTicker, TimeFrame,
};

pub const FAKE_EVENT_TICKER: &str = "BTC-FAKE-1H";
pub const FAKE_INSTRUMENT: &str = "FAKE-BTC-1H-UP";

/// Reference spot price: 65,000 USD in micro-USD.
const REFERENCE_PRICE: i64 = 65_000_000_000;
/// ETH reference spot price: 3,200 USD in micro-USD.
const ETH_REFERENCE_PRICE: i64 = 3_200_000_000;
const LATEST_TS_MS: i64 = 2_000_000_000_000;
/// Older than any trigger window, so it is always the reference candle.
const REFERENCE_AGE_MS: i64 = 400 * 86_400 * 1000;

/// One fixed catalog event: the contracts are `(instrument, label, YES price bps)`.
struct Listing {
    ticker: &'static str,
    title: &'static str,
    category: &'static str,
    /// Slug and name, as Gemini lists them.
    subcategory: (&'static str, &'static str),
    tags: &'static [&'static str],
    structure: Option<MarketStructure>,
    volume_delta_24h_bps: Option<i64>,
    contracts: &'static [(&'static str, &'static str, i64)],
}

/// Fixed fake listings besides the controllable BTC event. Every price is
/// inside the 0.05-0.95 band discovery accepts.
const CATALOG: [Listing; 6] = [
    Listing {
        ticker: "ETH-FAKE-FRI",
        title: "[fake] ETH price on Friday",
        category: "Crypto",
        subcategory: ("ethereum", "Ethereum"),
        tags: &["ETH", "Weekly"],
        structure: Some(MarketStructure::PriceLadder),
        volume_delta_24h_bps: Some(800),
        contracts: &[
            ("FAKE-ETH-FRI-3400", "$3,400 or above", 6200),
            ("FAKE-ETH-FRI-3500", "$3,500 or above", 4800),
            ("FAKE-ETH-FRI-3600", "$3,600 or above", 2900),
        ],
    },
    Listing {
        ticker: "WTI-FAKE-FRI",
        title: "[fake] WTI crude oil on Friday",
        category: "Commodities",
        subcategory: ("energy", "Energy"),
        tags: &["Oil", "WTI", "Energy"],
        structure: Some(MarketStructure::PriceLadder),
        volume_delta_24h_bps: Some(1250),
        contracts: &[
            ("FAKE-WTI-FRI-78", "$78 or above", 6600),
            ("FAKE-WTI-FRI-80", "$80 or above", 4500),
            ("FAKE-WTI-FRI-82", "$82 or above", 2100),
        ],
    },
    Listing {
        ticker: "NG-FAKE-DEC",
        title: "[fake] Natural gas in December",
        category: "Commodities",
        subcategory: ("energy", "Energy"),
        tags: &["Natural gas", "Energy", "Monthly"],
        structure: Some(MarketStructure::BinaryUpDown),
        volume_delta_24h_bps: Some(-300),
        contracts: &[("FAKE-NG-DEC-UP", "Up", 5300)],
    },
    Listing {
        ticker: "GOLD-FAKE-DEC",
        title: "[fake] Gold at month end",
        category: "Commodities",
        subcategory: ("metals", "Metals"),
        tags: &["Gold", "Metals", "Monthly"],
        structure: Some(MarketStructure::PriceLadder),
        volume_delta_24h_bps: None,
        contracts: &[("FAKE-GOLD-DEC-2700", "$2,700 or above", 5800)],
    },
    Listing {
        ticker: "FED-FAKE-DEC",
        title: "[fake] Fed decision in December",
        category: "Economics",
        subcategory: ("fed-rates", "Fed rates"),
        tags: &["Fed", "Rates"],
        structure: Some(MarketStructure::Categorical),
        volume_delta_24h_bps: Some(2000),
        contracts: &[
            ("FAKE-FED-DEC-CUT25", "Cut 25 bps", 5200),
            ("FAKE-FED-DEC-HOLD", "Hold", 4300),
            ("FAKE-FED-DEC-HIKE", "Hike", 600),
        ],
    },
    Listing {
        ticker: "FINAL-FAKE-CHAMP",
        title: "[fake] Championship final winner",
        category: "Sports",
        subcategory: ("basketball", "Basketball"),
        tags: &["Final", "Basketball"],
        structure: Some(MarketStructure::Categorical),
        volume_delta_24h_bps: None,
        contracts: &[
            ("FAKE-FINAL-HOME", "Home team", 5700),
            ("FAKE-FINAL-AWAY", "Away team", 4400),
        ],
    },
];

impl Listing {
    fn event(&self) -> PmEvent {
        PmEvent {
            ticker: self.ticker.into(),
            title: self.title.into(),
            category: self.category.into(),
            status: "active".into(),
            expiry_ts: None,
            contracts: self
                .contracts
                .iter()
                .map(|(symbol, label, price)| PmContract {
                    instrument_symbol: (*symbol).into(),
                    label: (*label).into(),
                    status: "active".into(),
                    market_state: Some("open".into()),
                    expiry_ts: None,
                    last_trade_price_bps: Some(*price),
                    best_bid_bps: None,
                    best_ask_bps: None,
                    resolution_side: None,
                })
                .collect(),
            meta: super::MarketMeta {
                subcategories: vec![self.subcategory.0.into(), self.subcategory.1.into()],
                tags: self.tags.iter().map(|t| (*t).into()).collect(),
                structure: self.structure,
                expiry_ts: None,
                volume_delta_24h_bps: self.volume_delta_24h_bps,
            },
        }
    }
}

/// Whether `event` fits the listing query: category equal and search a
/// substring of ticker or title, both case-insensitive. Status is not
/// filtered (callers check it).
fn fits(event: &PmEvent, query: &EventQuery) -> bool {
    let category_ok = query
        .category
        .as_deref()
        .is_none_or(|c| event.category.eq_ignore_ascii_case(c));
    let search_ok = query.search.as_deref().is_none_or(|s| {
        let s = s.to_lowercase();
        event.ticker.to_lowercase().contains(&s) || event.title.to_lowercase().contains(&s)
    });
    category_ok && search_ok
}

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

    /// Reference price of a known spot symbol (`btcusd`, `ethusd`); both move
    /// by the same `move_bps`. Other symbols have no fake spot market.
    fn reference_price(symbol: &str) -> Result<i64, MarketError> {
        match symbol.to_ascii_lowercase().as_str() {
            "btcusd" => Ok(REFERENCE_PRICE),
            "ethusd" => Ok(ETH_REFERENCE_PRICE),
            _ => Err(MarketError::NotFound),
        }
    }

    fn last_price(state: &State, reference: i64) -> i64 {
        reference + reference / 10_000 * state.move_bps
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
                best_bid_bps: None,
                best_ask_bps: None,
                resolution_side: state.resolution_side.clone(),
            }],
            meta: super::MarketMeta {
                subcategories: vec!["bitcoin".into(), "Bitcoin".into()],
                tags: vec!["BTC".into(), "Hourly".into()],
                structure: Some(MarketStructure::BinaryUpDown),
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

    fn spot_ticker<'a>(&'a self, symbol: &'a str) -> BoxFut<'a, Result<SpotTicker, MarketError>> {
        Box::pin(async move {
            let state = self.enter()?;
            let last = Self::last_price(&state, Self::reference_price(symbol)?);
            Ok(SpotTicker {
                bid: last,
                ask: last,
                last,
            })
        })
    }

    fn spot_candles<'a>(
        &'a self,
        symbol: &'a str,
        _frame: TimeFrame,
    ) -> BoxFut<'a, Result<Vec<Candle>, MarketError>> {
        Box::pin(async move {
            let state = self.enter()?;
            let reference = Self::reference_price(symbol)?;
            let flat = |ts_ms, price| Candle {
                ts_ms,
                open: price,
                high: price,
                low: price,
                close: price,
            };
            Ok(vec![
                flat(LATEST_TS_MS, Self::last_price(&state, reference)),
                flat(LATEST_TS_MS - REFERENCE_AGE_MS, reference),
            ])
        })
    }

    fn list_events<'a>(
        &'a self,
        query: &'a EventQuery,
    ) -> BoxFut<'a, Result<Vec<PmEvent>, MarketError>> {
        Box::pin(async move {
            let state = self.enter()?;
            if !state.prediction_markets {
                return Ok(Vec::new());
            }
            Ok(std::iter::once(Self::event(&state))
                .chain(CATALOG.iter().map(Listing::event))
                .filter(|e| fits(e, query))
                .collect())
        })
    }

    fn get_event<'a>(&'a self, ticker: &'a str) -> BoxFut<'a, Result<PmEvent, MarketError>> {
        Box::pin(async move {
            let state = self.enter()?;
            if !state.prediction_markets {
                return Err(MarketError::NotFound);
            }
            if ticker == FAKE_EVENT_TICKER {
                return Ok(Self::event(&state));
            }
            CATALOG
                .iter()
                .find(|l| l.ticker == ticker)
                .map(Listing::event)
                .ok_or(MarketError::NotFound)
        })
    }
}

#[cfg(test)]
mod tests {
    use aen_core::challenge::{FormulaId, MarketRef};

    use super::*;
    use crate::market::{MAX_OPEN_PRICE_BPS, MIN_OPEN_PRICE_BPS, discover, observe};

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

    #[tokio::test]
    async fn catalog_spans_categories_and_honours_the_query() {
        let fake = FakeMarket::demo();
        let all = fake.list_events(&EventQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1 + CATALOG.len());
        let categories: std::collections::BTreeSet<&str> =
            all.iter().map(|e| e.category.as_str()).collect();
        assert_eq!(
            categories.into_iter().collect::<Vec<_>>(),
            ["Commodities", "Crypto", "Economics", "Sports"]
        );
        // Every catalog contract is open, inside the discovery band, and unique.
        let mut symbols = std::collections::BTreeSet::new();
        for c in all.iter().flat_map(|e| &e.contracts) {
            assert!(symbols.insert(c.instrument_symbol.clone()), "{c:?}");
            let p = c.last_trade_price_bps.unwrap();
            assert!(
                (MIN_OPEN_PRICE_BPS..=MAX_OPEN_PRICE_BPS).contains(&p),
                "{c:?}"
            );
        }

        let energy = fake
            .list_events(&EventQuery {
                category: Some("commodities".into()),
                search: Some("oil".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(energy.len(), 1);
        assert_eq!(energy[0].ticker, "WTI-FAKE-FRI");
        assert_eq!(energy[0].meta.subcategories, ["energy", "Energy"]);

        // Discovery's own query still sees only the BTC event.
        let btc = fake
            .list_events(&EventQuery {
                search: Some("BTC".into()),
                category: Some("Crypto".into()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(btc.len(), 1);
        assert_eq!(btc[0].ticker, FAKE_EVENT_TICKER);

        fake.set_prediction_markets(false);
        assert!(
            fake.list_events(&EventQuery::default())
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            fake.get_event("WTI-FAKE-FRI").await.unwrap_err(),
            MarketError::NotFound
        );
    }

    #[tokio::test]
    async fn catalog_contracts_are_discoverable_and_observable() {
        let fake = FakeMarket::demo();
        // ETH: the ladder rung closest to 0.50, on its own spot price.
        let view = discover(&fake, "ETHUSD", 3600, 0).await.unwrap();
        assert_eq!(
            view.market,
            MarketRef::GeminiPredictionContract {
                event_ticker: "ETH-FAKE-FRI".into(),
                instrument_symbol: "FAKE-ETH-FRI-3500".into(),
            }
        );
        assert_eq!(view.value, 4800);
        assert_eq!(view.move_bps, 300);
        assert_eq!(view.title, "[fake] ETH price on Friday: $3,500 or above");
        assert_eq!(
            fake.spot_ticker("ethusd").await.unwrap().last,
            3_296_000_000
        );
        // BTC discovery is unchanged by the catalog.
        let view = discover(&fake, "BTCUSD", 3600, 0).await.unwrap();
        assert_eq!(view.market, contract());

        let oil = MarketRef::GeminiPredictionContract {
            event_ticker: "WTI-FAKE-FRI".into(),
            instrument_symbol: "FAKE-WTI-FRI-80".into(),
        };
        let obs = observe(&fake, &oil, FormulaId::PmYesPriceBpsV1)
            .await
            .unwrap();
        assert_eq!((obs.value, obs.source), (4500, "fake"));
        // The dev controls move only the BTC contract.
        fake.set_resolution(Some("yes"));
        let obs = observe(&fake, &oil, FormulaId::PmYesPriceBpsV1)
            .await
            .unwrap();
        assert_eq!(obs.value, 4500);

        // No fake spot market for anything but BTC and ETH.
        assert_eq!(
            fake.spot_ticker("wtiusd").await.unwrap_err(),
            MarketError::NotFound
        );
        assert!(discover(&fake, "WTIUSD", 3600, 0).await.is_err());
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
