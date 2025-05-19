use chrono::{DateTime, Local, Utc, Duration as ChronoDuration};
use cron::Schedule;
use reqwest::Client;
use std::str::FromStr;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, error};

use crate::config::Config;

/// Spawns a cron job that calls the `/generate-and-send-email-report` endpoint.
pub async fn spawn_email_report_cron(cfg: Config) {
    // clone once for the async task
    let cfg = cfg.clone();
    let http = Client::new();

    tokio::spawn(async move {
        // 1) Preview next run via `cron::Schedule`
        let schedule = Schedule::from_str(&cfg.email_frequency)
            .expect("Invalid cron expression in EMAIL_FREQUENCY");
        let next_local: DateTime<Local> = schedule
            .upcoming(Local)
            .next()
            .expect("Unable to compute next schedule");
        info!(
            "System online. Next email report is at {}",
            next_local.format("%a, %b %e %Y at %I:%M:%S %p %:z")
        );

        // 2) Build the actual scheduler
        let mut sched = JobScheduler::new();

        // 3) Define and add the job
        let job = Job::new_async(&cfg.email_frequency, move |_uuid, _l| {
            // Clone what we need before the async block
            let api_key  = cfg.auth_key.clone();
            let http     = http.clone();
            let port     = cfg.listen_addr.port();
            let interval = cfg.stats_interval;

            Box::pin(async move {
                // Compute the time window
                let end   = Utc::now();
                let start = end - ChronoDuration::hours(interval);

                let body = serde_json::json!({
                    "start_date": start.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    "end_date":   end  .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                });

                let url = format!(
                    "http://localhost:{}/generate-and-send-email-report",
                    port
                );

                match http
                    .post(&url)
                    .header("X-API-Key", api_key)
                    .json(&body)
                    .send()
                    .await
                {
                    Ok(_) => info!("Scheduled report triggered at {}", Local::now().to_rfc3339()),
                    Err(e) => error!(
                        "Scheduled report POST failed at {}: {}",
                        Local::now().to_rfc3339(),
                        e
                    ),
                }
            })
        })
        .expect("Invalid cron expression");

        sched.add(job).expect("Failed to add email cron job");

        // 4) Start the scheduler loop
        sched.start().await.expect("Scheduler failed to start");
    });
}

/// Spawns a daily-at-midnight cron job that calls the `/update-storage-statistics` endpoint.
pub async fn spawn_storage_update_cron(cfg: Config) {
    // clone for the task
    let cfg = cfg.clone();
    let http = Client::new();

    tokio::spawn(async move {
        // 1) Preview next run
        let expr = "0 0 0 * * *"; // quartz 6-field: sec=0, min=0, hour=0, daily
        let schedule = Schedule::from_str(expr)
            .expect("Invalid cron expression for storage update");
        let next_local: DateTime<Local> = schedule
            .upcoming(Local)
            .next()
            .expect("Cannot compute next storage update run");
        info!(
            "Next storage stats update is at {}",
            next_local.format("%a, %b %e %Y at %I:%M:%S %p %:z")
        );

        // 2) Build scheduler
        let mut sched = JobScheduler::new();

        // 3) Build and add the job
        let job = Job::new_async(expr, move |_uuid, _l| {
            // clone inside closure
            let api_key = cfg.auth_key.clone();
            let http    = http.clone();
            let port    = cfg.listen_addr.port();

            Box::pin(async move {
                let url = format!(
                    "http://localhost:{}/update-storage-statistics",
                    port
                );

                match http
                    .get(&url)
                    .header("X-API-Key", api_key)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        info!("Storage stats update succeeded at {}", Local::now().to_rfc3339())
                    },
                    Ok(resp) => error!(
                        "Storage stats update returned {} at {}",
                        resp.status(),
                        Local::now().to_rfc3339()
                    ),
                    Err(e) => error!(
                        "Storage stats update POST failed at {}: {}",
                        Local::now().to_rfc3339(),
                        e
                    ),
                }
            })
        })
        .expect("Invalid cron expression for storage update");

        sched.add(job).expect("Failed to add storage update cron job");

        // 4) Start the scheduler loop
        sched.start().await.expect("Scheduler failed to start");
    });
}