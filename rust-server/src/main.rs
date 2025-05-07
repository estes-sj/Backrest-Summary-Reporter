// src/main.rs

mod config;
mod db;
mod handlers;
mod models;

use axum::{Router, routing::get, routing::post};
use config::Config;
use db::init_db;
use handlers::{summary_handler,get_stats_handler,test_email_handler};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber::{fmt, EnvFilter};

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
        .with_state((pool, cfg));

    // serve
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
