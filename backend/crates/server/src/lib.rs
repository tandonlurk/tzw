//! HTTP API over the core domain logic. See docs/api.md for the contract.

pub mod agents;
pub mod demo;
pub mod error;
pub mod extract;
pub mod graph_exec;
pub mod grok;
pub mod interactions;
pub mod ledger_db;
pub mod lifecycle;
pub mod market;
pub mod misc;
pub mod models;
pub mod reasoner;
pub mod state;

use axum::Router;
use axum::http::HeaderValue;
use axum::routing::{get, post};
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub const DEFAULT_CORS_ORIGIN: &str = "http://localhost:3000";

/// The API with no network dependencies: deterministic fake market data and
/// the rule-based reasoner. Used by tests and offline demos.
pub fn app(pool: PgPool, cors_origin: HeaderValue) -> Router {
    app_with_state(AppState::offline(pool), cors_origin)
}

/// The whole API, mounted under `/api`.
pub fn app_with_state(state: AppState, cors_origin: HeaderValue) -> Router {
    let mut api = Router::new()
        .route("/health", get(misc::health))
        .route("/agents", get(agents::list).post(agents::create))
        .route("/agents/{id}", get(agents::get).put(agents::update))
        .route("/agents/{id}/status", post(agents::set_status))
        .route("/agents/{id}/versions", get(agents::versions))
        .route("/agents/{id}/ledger", get(agents::ledger_entries))
        .route("/agents/{id}/allocate", post(agents::allocate))
        .route("/agents/{id}/deallocate", post(agents::deallocate))
        .route("/templates", get(misc::templates))
        .route("/graph/blocks", get(misc::blocks))
        .route("/graph/validate", post(misc::validate_graph))
        .route("/wallet", get(misc::wallet))
        .route("/wallet/simulated-deposit", post(misc::simulated_deposit))
        .route("/activity", get(misc::activity))
        .route("/interactions", get(interactions::list))
        .route("/interactions/{id}", get(interactions::get))
        .route("/approvals", get(interactions::approvals))
        .route("/approvals/{id}/approve", post(interactions::approve))
        .route("/approvals/{id}/reject", post(interactions::reject))
        .route("/stats/portfolio", get(interactions::portfolio));
    if state.settings.dev_endpoints {
        api = api
            .route("/dev/tick", post(interactions::dev_tick))
            .route("/dev/fake-market", post(interactions::dev_fake_market))
            .route("/dev/seed-demo", post(demo::seed_demo));
    }

    Router::new()
        .nest("/api", api)
        .fallback(misc::not_found)
        .layer(
            CorsLayer::new()
                .allow_origin(cors_origin)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
