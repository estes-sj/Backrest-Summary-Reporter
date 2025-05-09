mod config;
mod db;
mod handlers;
mod models;

use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber::{fmt, EnvFilter};
use axum::{
    routing::{get, post},
    Router,
};
use config::Config;
use db::init_db;
use handlers::{get_stats_handler, summary_handler, test_email_handler, storage_stats_handler};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(false)
        .init();

    // Load configuration (DATABASE_URL, AUTH_KEY, TZ)
    let cfg = Config::from_env()?;

    // Initialize database and create tables
    let pool = init_db(&cfg.database_url).await?;

    // use cfg to bind first
    let listener = TcpListener::bind(cfg.listen_addr).await?;
    tracing::info!("Listening on {}", cfg.listen_addr);

    // now move cfg into the router
    let app = Router::new()
        .route("/summary", post(summary_handler))
        .route("/get_stats", post(get_stats_handler))
        .route("/test_email", get(test_email_handler))
        .route("/storage_stats", post(storage_stats_handler))
        .with_state((pool, cfg));

    // serve
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
