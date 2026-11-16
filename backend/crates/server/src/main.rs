use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use aen_server::cocaptain::{CoCaptain, RuleBasedCoCaptain};
use aen_server::grok::GrokReasoner;
use aen_server::grok_cocaptain::GrokCoCaptain;
use aen_server::market::MarketData;
use aen_server::market::fake::FakeMarket;
use aen_server::market::gemini::GeminiClient;
use aen_server::reasoner::{Reasoner, RuleBasedReasoner};
use aen_server::state::{AppState, Settings};
use aen_server::{DEFAULT_CORS_ORIGIN, MIGRATOR, app_with_state, lifecycle};
use sqlx::postgres::PgPoolOptions;

/// An optional environment variable, parsed; unset means `default`.
fn var<T: FromStr>(name: &str, default: T) -> Result<T, String> {
    match env::var(name) {
        Ok(text) => text
            .trim()
            .parse()
            .map_err(|_| format!("{name} has an invalid value")),
        Err(_) => Ok(default),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let database_url = env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set")?;
    let port: u16 = var("PORT", 8080)?;
    let cors_origin = env::var("CORS_ORIGIN")
        .unwrap_or_else(|_| DEFAULT_CORS_ORIGIN.to_owned())
        .parse()
        .map_err(|_| "CORS_ORIGIN is not a valid origin")?;

    let defaults = Settings::default();
    let settings = Settings {
        max_negotiation_rounds: var("MAX_NEGOTIATION_ROUNDS", defaults.max_negotiation_rounds)?,
        funding_window_secs: var("FUNDING_WINDOW_SECS", defaults.funding_window_secs)?,
        resolution_grace_secs: var("RESOLUTION_GRACE_SECS", defaults.resolution_grace_secs)?,
        proposal_cooldown_secs: var("PROPOSAL_COOLDOWN_SECS", defaults.proposal_cooldown_secs)?,
        default_symbol: var("DEFAULT_SYMBOL", defaults.default_symbol)?,
        default_window_secs: defaults.default_window_secs,
        dev_endpoints: var("DEV_ENDPOINTS", defaults.dev_endpoints)?,
    };
    if settings.max_negotiation_rounds < 1
        || settings.funding_window_secs < 1
        || settings.resolution_grace_secs < 0
        || settings.proposal_cooldown_secs < 0
    {
        return Err("lifecycle settings must be positive".into());
    }
    let tick_interval_secs: u64 = var("TICK_INTERVAL_SECS", 15)?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;
    MIGRATOR.run(&pool).await?;
    tracing::info!("migrations applied");

    // "gemini" is the Gemini crypto exchange's public market data API.
    let source: String = var("MARKET_DATA", "gemini".to_owned())?;
    let (market, fake_market): (Arc<dyn MarketData>, _) = match source.as_str() {
        "gemini" => {
            let base: String = var("GEMINI_BASE_URL", GeminiClient::DEFAULT_BASE_URL.to_owned())?;
            (Arc::new(GeminiClient::new(&base)?), None)
        }
        "fake" => {
            let fake = Arc::new(FakeMarket::demo());
            (fake.clone(), Some(fake))
        }
        _ => return Err("MARKET_DATA must be `gemini` or `fake`".into()),
    };
    // Grok is used only when a key is present; both uses fall back to the
    // rules on any error. The key is never logged.
    let (reasoner, cocaptain): (Arc<dyn Reasoner>, Arc<dyn CoCaptain>) =
        match env::var("XAI_API_KEY") {
            Ok(key) if !key.trim().is_empty() => {
                let base: String = var("XAI_BASE_URL", GrokReasoner::DEFAULT_BASE_URL.to_owned())?;
                let model: String = var("XAI_MODEL", GrokReasoner::DEFAULT_MODEL.to_owned())?;
                let timeout = GrokReasoner::TIMEOUT;
                (
                    Arc::new(GrokReasoner::new(&base, &model, key.trim(), timeout)?),
                    Arc::new(GrokCoCaptain::new(&base, &model, key.trim(), timeout)?),
                )
            }
            _ => (Arc::new(RuleBasedReasoner), Arc::new(RuleBasedCoCaptain)),
        };
    tracing::info!(market_data = %source, reasoner = reasoner.name(), cocaptain = cocaptain.name(), "runtime configured");

    let state = AppState {
        pool,
        market,
        fake_market,
        reasoner,
        cocaptain,
        settings,
    };

    if tick_interval_secs > 0 {
        let state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(tick_interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                match lifecycle::tick(&state, chrono::Utc::now()).await {
                    Ok(report) if !report.errors.is_empty() => {
                        tracing::warn!(errors = ?report.errors, "tick finished with problems");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::error!(code = e.code, message = %e.message, "tick failed"),
                }
            }
        });
        tracing::info!(tick_interval_secs, "scheduler started");
    } else {
        tracing::info!("scheduler disabled (TICK_INTERVAL_SECS=0); use POST /api/dev/tick");
    }

    // No auth yet (D-9), so listen on loopback only.
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app_with_state(state, cors_origin)).await?;
    Ok(())
}
