mod config;
mod db;
mod email;
mod handlers;
mod html_report;
mod models;

use std::net::SocketAddr;
use std::str::FromStr;

use axum::{
    routing::{get, post},
    Router, serve,
};
use chrono::{Utc, DateTime, Duration as ChronoDuration, Local};
use cron::Schedule;
use reqwest::Client;
use tokio::net::TcpListener;
use tokio_cron_scheduler::{Job, JobScheduler};
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

    // Load config & DB
    let cfg = Config::from_env()?;
    let pool = init_db(&cfg.database_url).await?;

    // Spawn the cron scheduler in a background task
    {
        let cfg = cfg.clone();
        let client = Client::new();
        tokio::spawn(async move {

            // Build the cron::Schedule from the same expr
            let schedule = Schedule::from_str(&cfg.email_frequency)
                .expect("Invalid cron expression in EMAIL_FREQUENCY");

            // Compute the next upcoming time in the local timezone
            let next_local: DateTime<Local> = schedule
                .upcoming(Local)
                .next()
                .expect("Unable to compute next schedule");
            
            tracing::info!(
                "System online. Next email report is at {}",
                next_local.format("%a, %b %e %Y at %I:%M:%S %p %:z")
            );

            // Build the JobScheduler and add the job
            let mut sched = JobScheduler::new();

            sched
                .add(
                    Job::new_async(&cfg.email_frequency, move |_uuid, _l| {
                        // Clone what we need _before_ the async block
                        let api_key = cfg.auth_key.clone();
                        let http    = client.clone();
                        let port    = cfg.listen_addr.port();
                        let interval = cfg.stats_interval;

                        Box::pin(async move {
                            // Compute the time window
                            let end = Utc::now();
                            let start = end - ChronoDuration::hours(interval);

                            // Build JSON body
                            let body = serde_json::json!({
                                "start_date": start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                                "end_date"  : end.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                            });

                            // Compose URL
                            let url = format!(
                                "http://localhost:{}/generate-and-send-email-report",
                                port
                            );

                            // Fire & forget
                            if let Err(err) = http
                                .post(&url)
                                .header("X-API-Key", api_key)
                                .json(&body)
                                .send()
                                .await
                            {
                                tracing::error!(
                                    "Scheduled report POST failed at {}: {}",
                                    Local::now().to_rfc3339(),
                                    err
                                );
                            } else {
                                tracing::info!(
                                    "Scheduled report triggered at {}",
                                    Local::now().to_rfc3339()
                                );
                            }
                        })
                    })
                    .expect("Invalid cron expression"),
                )
                .expect("Failed to add cron job");

            // Kick off the scheduler loop
            let _ = sched.start();
        });
    }

    // Bind TCP listener
    let listener = TcpListener::bind(cfg.listen_addr).await?;
    tracing::info!("Listening on {}", cfg.listen_addr);

    // Build the application router
    let app = Router::new()
        .route("/add-event", post(add_event_handler))
        .route(
            "/generate-and-send-email-report",
            post(generate_and_send_email_report),
        )
        .route(
            "/get-events-and-storage-stats",
            post(get_events_and_storage_stats_handler),
        )
        .route("/get-events-in-range", post(get_events_in_range_handler))
        .route(
            "/get-events-in-range-totals",
            post(get_events_in_range_totals_handler),
        )
        .route(
            "/get-latest-storage-stats",
            get(get_latest_storage_stats_handler),
        )
        .route("/send-test-email", get(send_test_email_handler))
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
