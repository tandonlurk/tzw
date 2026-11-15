use std::env;
use std::error::Error;
use std::net::SocketAddr;

use aen_server::{DEFAULT_CORS_ORIGIN, MIGRATOR, app};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let database_url = env::var("DATABASE_URL").map_err(|_| "DATABASE_URL must be set")?;
    let port: u16 = match env::var("PORT") {
        Ok(p) => p.parse().map_err(|_| "PORT must be a port number")?,
        Err(_) => 8080,
    };
    let cors_origin = env::var("CORS_ORIGIN")
        .unwrap_or_else(|_| DEFAULT_CORS_ORIGIN.to_owned())
        .parse()
        .map_err(|_| "CORS_ORIGIN is not a valid origin")?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;
    MIGRATOR.run(&pool).await?;
    tracing::info!("migrations applied");

    // No auth yet (D-9), so listen on loopback only.
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app(pool, cors_origin)).await?;
    Ok(())
}
