//! Market listings for the approach editor: which categories exist, and how
//! many live prediction-market contracts a market filter matches right now.
//! Read-only; never used for resolution.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use aen_core::approach::{MarketFilter, MarketStructure, base_symbol, market_matches};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{ApiError, Detail};
use crate::extract::ApiJson;
use crate::graph_exec::{self, TRIGGER_MARKET_MOVE};
use crate::lifecycle::load_agent;
use crate::market::{
    self, EventQuery, MarketData, MarketError, PmEvent, TimeFrame, contract_facts, ticker_symbol,
};
use crate::state::AppState;

/// How long a category listing is reused.
const CATEGORY_TTL: Duration = Duration::from_secs(300);
/// One listing page (the largest accepted live).
const EVENT_PAGE: u32 = 100;
/// Most spot symbols whose price move a preview computes.
const MAX_MOVE_SYMBOLS: usize = 5;
const MAX_SAMPLE: usize = 10;

/// Gemini's top-level categories (observed 2026-11-14), used when the
/// listing is unavailable.
pub const FALLBACK_CATEGORIES: [&str; 12] = [
    "Crypto",
    "Commodities",
    "Economics",
    "Finance",
    "Politics",
    "Sports",
    "Tech",
    "Business",
    "Weather",
    "Culture",
    "Media",
    "Other",
];

#[derive(Debug, Clone, Serialize)]
pub struct Categories {
    pub categories: Vec<String>,
    /// Category -> subcategory names seen on active events.
    pub subcategories: BTreeMap<String, Vec<String>>,
    pub structures: Vec<&'static str>,
    /// `gemini`, `fake` or `fallback`.
    pub source: String,
}

static CATEGORY_CACHE: Mutex<Option<(Instant, &'static str, Categories)>> = Mutex::new(None);

async fn active_events(market: &dyn MarketData) -> Result<Vec<PmEvent>, MarketError> {
    let query = EventQuery {
        search: None,
        category: None,
        status: Some("active".into()),
        limit: EVENT_PAGE,
    };
    Ok(market
        .list_events(&query)
        .await?
        .into_iter()
        .filter(|e| e.status == "active")
        .collect())
}

fn structures() -> Vec<&'static str> {
    MarketStructure::ALL.iter().map(|s| s.as_str()).collect()
}

pub async fn categories(State(state): State<AppState>) -> Json<Categories> {
    let source = state.market.name();
    if let Ok(cache) = CATEGORY_CACHE.lock()
        && let Some((at, name, value)) = cache.as_ref()
        && *name == source
        && at.elapsed() < CATEGORY_TTL
    {
        return Json(value.clone());
    }
    let value = match active_events(state.market.as_ref()).await {
        Ok(events) => {
            let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for e in &events {
                let subs = map.entry(e.category.clone()).or_default();
                // Slug and name are both listed in meta; the name is last.
                if let Some(name) = e.meta.subcategories.last() {
                    subs.insert(name.clone());
                }
            }
            Categories {
                categories: map.keys().cloned().collect(),
                subcategories: map
                    .into_iter()
                    .map(|(k, v)| (k, v.into_iter().collect()))
                    .collect(),
                structures: structures(),
                source: source.to_owned(),
            }
        }
        // Not cached: the next request tries the source again.
        Err(e) => {
            tracing::warn!(error = %e, "market categories unavailable; using the fallback list");
            return Json(Categories {
                categories: FALLBACK_CATEGORIES
                    .iter()
                    .map(|c| (*c).to_owned())
                    .collect(),
                subcategories: BTreeMap::new(),
                structures: structures(),
                source: "fallback".into(),
            });
        }
    };
    if let Ok(mut cache) = CATEGORY_CACHE.lock() {
        *cache = Some((Instant::now(), source, value.clone()));
    }
    Json(value)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewBody {
    pub markets: MarketFilter,
    /// Uses this agent's market-move window for the price move.
    #[serde(default)]
    pub agent_id: Option<Uuid>,
}

/// One matching contract.
#[derive(Debug, Serialize)]
pub struct PreviewMarket {
    /// `gemini_pm:{instrument}`, the same key challenges use.
    pub market_id: String,
    pub title: String,
    pub category: String,
    pub subcategory: Option<String>,
    pub structure: Option<MarketStructure>,
    pub symbol: Option<String>,
    pub yes_price_bps: Option<i64>,
    /// Signed spot move over the window; `null` when not computed.
    pub move_bps: Option<i64>,
    pub expiry_ts: Option<i64>,
    pub volume_delta_24h_bps: Option<i64>,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct Preview {
    pub count: usize,
    pub sample: Vec<PreviewMarket>,
    pub as_of: DateTime<Utc>,
    pub source: String,
    /// Seconds of the price-move window used.
    pub window_secs: i64,
}

async fn window_for(state: &AppState, agent: Option<Uuid>) -> Result<i64, ApiError> {
    let default = state.settings.default_window_secs;
    let Some(id) = agent else {
        return Ok(default);
    };
    let mut conn = state.pool.acquire().await?;
    let agent = load_agent(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("agent"))?;
    Ok(graph_exec::find_trigger(&agent.graph, TRIGGER_MARKET_MOVE)
        .and_then(|n| n.params.get("window_secs").and_then(Value::as_i64))
        .filter(|w| *w > 0)
        .unwrap_or(default))
}

pub async fn preview(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<PreviewBody>,
) -> Result<Json<Preview>, ApiError> {
    let details: Vec<Detail> = body
        .markets
        .validate()
        .into_iter()
        .map(|(path, message)| Detail::new(path, "invalid", message))
        .collect();
    if !details.is_empty() {
        return Err(ApiError::validation(details));
    }
    let window = window_for(&state, body.agent_id).await?;
    let source = state.market.name();
    let events = active_events(state.market.as_ref()).await.map_err(|e| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "market_data_unavailable",
            format!("market listings are unavailable: {e}"),
        )
    })?;

    // The spot move is only fetched when the filter has a move range, for a
    // few distinct assets (others count as unknown and fail a move bound).
    let mut moves: BTreeMap<String, i64> = BTreeMap::new();
    if !body.markets.ranges.move_bps.is_open() {
        let frame = if window <= 86_400 {
            TimeFrame::OneMinute
        } else {
            TimeFrame::OneHour
        };
        let symbols: BTreeSet<String> = events
            .iter()
            .filter_map(|e| ticker_symbol(&e.ticker))
            .collect();
        for sym in symbols.into_iter().take(MAX_MOVE_SYMBOLS) {
            let pair = format!("{}usd", sym.to_lowercase());
            if let Ok(candles) = state.market.spot_candles(&pair, frame).await
                && let Ok(m) = market::move_bps(&candles, window)
            {
                moves.insert(sym, m);
            }
        }
    }

    let now = Utc::now();
    let mut count = 0;
    let mut sample = Vec::new();
    for event in &events {
        let symbol = ticker_symbol(&event.ticker);
        let moved = symbol.as_ref().and_then(|s| moves.get(s)).copied();
        for contract in event
            .contracts
            .iter()
            .filter(|c| c.status == "active" && c.resolution_side.is_none())
        {
            let facts = contract_facts(event, contract, moved, now.timestamp());
            if market_matches(&body.markets, &facts).is_err() {
                continue;
            }
            count += 1;
            if sample.len() < MAX_SAMPLE {
                sample.push(PreviewMarket {
                    market_id: aen_core::challenge::MarketRef::GeminiPredictionContract {
                        event_ticker: event.ticker.clone(),
                        instrument_symbol: contract.instrument_symbol.clone(),
                    }
                    .key(),
                    title: facts.title,
                    category: event.category.clone(),
                    subcategory: event.meta.subcategories.last().cloned(),
                    structure: event.meta.structure,
                    symbol: symbol.as_deref().map(base_symbol),
                    yes_price_bps: contract.quote_bps(),
                    move_bps: moved,
                    expiry_ts: contract.expiry_ts.or(event.expiry_ts),
                    volume_delta_24h_bps: event.meta.volume_delta_24h_bps,
                    source: source.to_owned(),
                });
            }
        }
    }
    Ok(Json(Preview {
        count,
        sample,
        as_of: now,
        source: source.to_owned(),
        window_secs: window,
    }))
}
