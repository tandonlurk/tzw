use std::env;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use aen_server::cocaptain::{CoCaptain, RuleBasedCoCaptain};
use aen_server::grok::GrokReasoner;
use aen_server::grok_cocaptain::GrokCoCaptain;
use aen_server::guard::{ApiToken, GuardConfig, MIN_TOKEN_LEN, startup_error};
use aen_server::market::MarketData;
use aen_server::market::fake::FakeMarket;
use aen_server::market::gemini::GeminiClient;
use aen_server::quicknode::{QuickNodeConfig, StreamSecret};
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
        guard: GuardConfig {
            api_token: match env::var("AEN_API_TOKEN") {
                Ok(t) if !t.trim().is_empty() => Some(ApiToken::new(&t).ok_or_else(|| {
                    format!("AEN_API_TOKEN must be at least {MIN_TOKEN_LEN} characters")
                })?),
                _ => None,
            },
            ..GuardConfig::default()
        }
        .with_extra_hosts(&env::var("ALLOWED_HOSTS").unwrap_or_default()),
        // Local ingestion only: mounted when the stream's secret is set.
        quicknode: match env::var("QUICKNODE_STREAMS_SECRET")
            .ok()
            .and_then(|s| StreamSecret::new(&s))
        {
            Some(secret) => {
                let d = QuickNodeConfig::new(secret);
                let name = |var: &str, default: String| {
                    env::var(var)
                        .ok()
                        .map(|v| v.trim().to_ascii_lowercase())
                        .filter(|v| !v.is_empty())
                        .unwrap_or(default)
                };
                Some(QuickNodeConfig {
                    signature_header: name("QUICKNODE_SIGNATURE_HEADER", d.signature_header),
                    timestamp_header: name("QUICKNODE_TIMESTAMP_HEADER", d.timestamp_header),
                    nonce_header: name("QUICKNODE_NONCE_HEADER", d.nonce_header),
                    max_skew_secs: var("QUICKNODE_MAX_SKEW_SECS", d.max_skew_secs)?,
                    secret: d.secret,
                })
            }
            None => None,
        },
    };
    if settings
        .quicknode
        .as_ref()
        .is_some_and(|q| q.max_skew_secs < 1)
    {
        return Err("QUICKNODE_MAX_SKEW_SECS must be positive".into());
    }
    // Loopback unless asked otherwise; anything else needs a token (SEC-02).
    let bind: IpAddr = var("BIND_ADDR", IpAddr::from([127, 0, 0, 1]))?;
    if let Some(problem) = startup_error(bind, &settings.guard) {
        return Err(problem.into());
    }
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

    let settings_token_required = settings.guard.api_token.is_some();
    let allowed_hosts = settings.guard.allowed_hosts.clone();
    let quicknode_on = settings.quicknode.is_some();
    // Compiled once here and shared; a missing guest module is logged and
    // only disables custom.code blocks.
    let sandbox = aen_server::custom_code::shared_sandbox();
    tracing::info!(
        sandbox = sandbox
            .as_ref()
            .map_or("unavailable", |s| s.module_sha256()),
        "custom code sandbox"
    );
    // Arc testnet only when AEN_TESTNET_MNEMONIC is set; otherwise every
    // agent stays simulated. The mnemonic is read here and dropped: keys live
    // inside aen-chain only and are never logged.
    let chain = aen_server::chain::from_env(settings.dev_endpoints).await?;
    match &chain {
        Some(c) => tracing::info!(
            chain_id = c.chain.chain_id(),
            escrow = %c.chain.config().escrow,
            factory = %c.chain.config().factory,
            operator = %c.chain.operator(),
            resolver = %c.chain.resolver(),
            dev_owner = ?c.dev_owner.as_ref().map(|o| o.address()),
            "Arc testnet connected"
        ),
        None => tracing::info!("chain disabled (no AEN_TESTNET_MNEMONIC): simulated funds only"),
    }
    let state = AppState {
        pool,
        market,
        fake_market,
        reasoner,
        cocaptain,
        sandbox,
        chain: chain.map(Arc::new),
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

    let addr = SocketAddr::from((bind, port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(
        %addr,
        token_required = settings_token_required,
        allowed_hosts = ?allowed_hosts,
        quicknode_webhook = quicknode_on,
        "listening"
    );
    axum::serve(listener, app_with_state(state, cors_origin)).await?;
    Ok(())
}
