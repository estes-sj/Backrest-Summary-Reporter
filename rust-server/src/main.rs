mod config;
mod db;
mod email;
mod handlers;
mod healthcheck;
mod html_report;
mod models;
mod scheduler;
mod utils;

use std::net::SocketAddr;

use axum::{
    routing::{get, post},
    Router, serve,
};
use tokio::net::TcpListener;
use tracing_subscriber::{fmt, EnvFilter};

use config::Config;
use db::init_db;
use handlers::{
    add_event_handler,
    generate_and_send_email_report,
    get_events_and_storage_stats_handler,
    get_events_in_range_handler,
    get_events_in_range_totals_handler,
    get_latest_storage_stats_handler,
    get_storage_stats_handler,
    send_test_email_handler,
    update_storage_statistics_handler,
};
use scheduler::{
    spawn_email_report_cron,
    spawn_storage_update_cron,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_ansi(false)
        .init();

    // Load config & DB
    let cfg = Config::from_env()?;
    let pool = init_db(&cfg.database_url).await?;

    // Spawn the cron scheduler in a background task
    spawn_email_report_cron(cfg.clone()).await;

    // Kick off storage update scheduler
    spawn_storage_update_cron(cfg.clone()).await;

    // Bind TCP listener
    let listener = TcpListener::bind(cfg.listen_addr).await?;
    tracing::info!("Listening on {}", cfg.listen_addr);

    // Build the application router
    let app = Router::new()
        .route(
            "/add-event",
            post(add_event_handler))
        .route(
            "/generate-and-send-email-report",
            post(generate_and_send_email_report),
        )
        .route(
            "/get-events-and-storage-stats",
            post(get_events_and_storage_stats_handler),
        )
        .route(
            "/get-events-in-range",
            post(get_events_in_range_handler))
        .route(
            "/get-events-in-range-totals",
            post(get_events_in_range_totals_handler),
        )
        .route(
            "/get-latest-storage-stats",
            get(get_latest_storage_stats_handler),
        )
        .route(
            "/get-storage-stats",
            post(get_storage_stats_handler),
        )
        .route(
            "/send-test-email",
            get(send_test_email_handler))
        .route(
            "/update-storage-statistics",
            get(update_storage_statistics_handler),
        )
        .with_state((pool, cfg.clone()));

    // Serve the app with connect-info
    serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
