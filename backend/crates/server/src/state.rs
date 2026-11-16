//! Shared application state and runtime settings.

use std::sync::Arc;

use axum::extract::FromRef;
use sqlx::PgPool;

use crate::market::MarketData;
use crate::market::fake::FakeMarket;
use crate::reasoner::{Reasoner, RuleBasedReasoner};

/// Lifecycle defaults. All of them are financial or timing defaults the owner
/// may overturn; they are read from the environment in `main`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Offers allowed in one negotiation, counting the proposal.
    pub max_negotiation_rounds: i32,
    /// Seconds a new proposal leaves for both stakes to be committed.
    pub funding_window_secs: i64,
    /// Seconds after `end_ts` to keep retrying market data before a
    /// challenge is voided and refunded.
    pub resolution_grace_secs: i64,
    /// Seconds an agent waits after proposing on a market before it may
    /// propose on that market again, whatever became of the first proposal.
    /// `0` disables the cooldown.
    pub proposal_cooldown_secs: i64,
    /// Market looked at by `trigger.schedule` flows (which name no symbol).
    pub default_symbol: String,
    pub default_window_secs: i64,
    /// Whether `/api/dev/*` is mounted.
    pub dev_endpoints: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            max_negotiation_rounds: 3,
            funding_window_secs: 300,
            resolution_grace_secs: 600,
            proposal_cooldown_secs: 600,
            default_symbol: "BTCUSD".into(),
            default_window_secs: 3600,
            dev_endpoints: true,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub market: Arc<dyn MarketData>,
    /// Present only when `market` is the deterministic fake; enables the
    /// fake-market controls and the simulated clock on `/api/dev/tick`.
    pub fake_market: Option<Arc<FakeMarket>>,
    pub reasoner: Arc<dyn Reasoner>,
    pub settings: Settings,
}

impl AppState {
    /// Fake market data and the rule-based reasoner: no network at all.
    pub fn offline(pool: PgPool) -> AppState {
        let fake = Arc::new(FakeMarket::demo());
        AppState {
            pool,
            market: fake.clone(),
            fake_market: Some(fake),
            reasoner: Arc::new(RuleBasedReasoner),
            settings: Settings::default(),
        }
    }
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> PgPool {
        state.pool.clone()
    }
}
