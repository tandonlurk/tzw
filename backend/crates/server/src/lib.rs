//! HTTP API over the core domain logic. See docs/api.md for the contract.

pub mod agents;
pub mod chain;
pub mod cocaptain;
pub mod collaborations;
pub mod custom_code;
pub mod demo;
pub mod demo_firm;
pub mod error;
pub mod extract;
pub mod firms;
pub mod graph_exec;
pub mod grok;
pub mod grok_cocaptain;
pub mod guard;
pub mod interactions;
pub mod ledger_db;
pub mod lifecycle;
pub mod lifecycle_arc;
pub mod market;
pub mod markets;
pub mod misc;
pub mod models;
pub mod quicknode;
pub mod reasoner;
pub mod state;
pub mod streams;
pub mod suggestions;

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
        .route("/agents/{id}/code/test-run", post(custom_code::test_run))
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
        .route("/stats/portfolio", get(interactions::portfolio))
        .route(
            "/collaborations",
            get(collaborations::list).post(collaborations::create),
        )
        .route("/collaborations/{id}", get(collaborations::get))
        .route("/collaborations/{id}/accept", post(collaborations::accept))
        .route("/collaborations/{id}/reject", post(collaborations::reject))
        .route("/collaborations/{id}/fund", post(collaborations::fund))
        .route(
            "/collaborations/{id}/complete",
            post(collaborations::complete),
        )
        .route("/collaborations/{id}/cancel", post(collaborations::cancel))
        .route("/markets/categories", get(markets::categories))
        .route("/markets/preview", post(markets::preview))
        .route("/firms", get(firms::list).post(firms::create))
        .route("/firms/{id}", get(firms::get))
        .route("/firms/{id}/join", post(firms::join))
        .route("/firms/{id}/leave", post(firms::leave))
        .route("/firms/{id}/close", post(firms::close))
        .route("/firms/{id}/contributions", get(firms::contributions))
        .route("/suggestions", get(suggestions::list))
        .route("/suggestions/generate", post(suggestions::generate_now))
        .route("/suggestions/{id}/accept", post(suggestions::accept))
        .route("/suggestions/{id}/dismiss", post(suggestions::dismiss))
        .route("/chain", get(chain::info))
        .route("/streams", get(streams::status))
        .route(
            "/agents/{id}/vault",
            get(chain::get_vault).post(chain::link_vault),
        )
        .route("/agents/{id}/vault/sync", post(chain::sync_vault));
    if state.settings.quicknode.is_some() {
        api = api.route("/webhooks/quicknode", post(quicknode::webhook));
    }
    if state.settings.dev_endpoints {
        api = api
            .route("/dev/tick", post(interactions::dev_tick))
            .route("/dev/fake-market", post(interactions::dev_fake_market))
            .route("/dev/seed-demo", post(demo::seed_demo))
            .route("/dev/seed-firm-demo", post(demo_firm::seed_firm_demo));
        // Signs with the test-owner key: only when explicitly enabled.
        if state.chain.as_ref().is_some_and(|c| c.dev_owner.is_some()) {
            api = api.route(
                "/dev/chain/setup-demo-vaults",
                post(chain::dev_setup_demo_vaults),
            );
        }
    }

    let guard = guard::Guard {
        config: state.settings.guard.clone(),
        origin: cors_origin.clone(),
    };
    Router::new()
        .nest("/api", api)
        .fallback(misc::not_found)
        // Inside CORS, so preflights are answered first and refusals still
        // carry CORS headers for the allowed origin.
        .layer(axum::middleware::from_fn_with_state(guard, guard::guard))
        .layer(
            CorsLayer::new()
                .allow_origin(cors_origin)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
