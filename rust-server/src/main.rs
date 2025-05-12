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
use handlers::{
    add_event_handler,
    get_events_and_storage_stats_handler,
    get_events_in_range_handler,
    get_events_in_range_totals_handler,
    get_latest_storage_stats_handler,
    send_test_email_handler,
    update_storage_statistics_handler,
};

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
        .route("/add-event", post(add_event_handler))
        .route("/get-events-and-storage-stats", post(get_events_and_storage_stats_handler))
        .route("/get-events-in-range", post(get_events_in_range_handler))
        .route("/get-events-in-range-totals", post(get_events_in_range_totals_handler))
        .route("/get-latest-storage-stats", get(get_latest_storage_stats_handler))
        .route("/send-test-email", get(send_test_email_handler))
        .route("/update-storage-statistics", get(update_storage_statistics_handler))
        .with_state((pool, cfg));

    // serve
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
