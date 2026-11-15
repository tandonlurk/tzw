//! HTTP API over the core domain logic. See docs/api.md for the contract.

pub mod agents;
pub mod error;
pub mod extract;
pub mod ledger_db;
pub mod market;
pub mod misc;
pub mod models;
pub mod reasoner;

use axum::Router;
use axum::http::HeaderValue;
use axum::routing::{get, post};
use sqlx::PgPool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub const DEFAULT_CORS_ORIGIN: &str = "http://localhost:3000";

/// The whole API, mounted under `/api`.
pub fn app(pool: PgPool, cors_origin: HeaderValue) -> Router {
    let api = Router::new()
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
        .route("/activity", get(misc::activity));

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
        .with_state(pool)
}
