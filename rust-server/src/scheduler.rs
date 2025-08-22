use chrono::{DateTime, Local, Utc, Duration as ChronoDuration};
use cron::Schedule;
use reqwest::Client;
use std::{fs, str::FromStr};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, error};
use crate::{
    fail, ok,
    config::Config,
    email::{EmailClient},
    utils::{container_id_from_hostname, format_local_datetime},
};


/// Spawns a cron job that calls the `/generate-and-send-email-report` endpoint.
pub async fn spawn_email_report_cron(cfg: Config) {
    // clone once for the async task
    let cfg = cfg.clone();
    let http = Client::new();
    
    tokio::spawn(async move {
        // 1) Preview next run via `cron::Schedule`
        let schedule = Schedule::from_str(&cfg.email_frequency)
        .expect("Invalid cron expression in EMAIL_FREQUENCY");
        
        // Pull the next run in UTC
        let next_utc: DateTime<Utc> = schedule
        .upcoming(Utc)
        .next()
        .expect("Unable to compute next schedule");
        
        // Convert to local zone for display
        let next_local: DateTime<Local> = next_utc.with_timezone(&Local);
        
        ok!(
            cfg,
            "System online. Next email report is at {}",
            next_local.format("%a, %b %e %Y at %I:%M:%S %p %:z")
        );
        
        // 2) Only send a startup email if configured to do so
        if cfg.send_startup_email {
            // Attempt to build an EmailClient from your SMTP config
            match EmailClient::from_config(&cfg) {
                Ok(client) => {
                    // Read the startup email HTML template
                    let mut html = match fs::read_to_string("html/startup_email.html") {
                        Ok(s) => s,
                        Err(err) => {
                            error!("Failed to read HTML template: {}", err);
                            return;
                        }
                    };
                    
                    // Replace any placeholders e.g. timestamp, URLs, etc.
                    html = html.replace("{{HOSTNAME}}", &container_id_from_hostname());
                    html = html.replace("{{NEXT_REPORT}}", &next_local.format("%a, %b %e %Y at %I:%M:%S %p %:z").to_string());
                    
                    html = html.replace("{{TIMESTAMP}}", &format_local_datetime(Local::now()));
                    html = html.replace(
                        "{{BACKREST_URL}}",
                        &cfg.backrest_url.clone().unwrap_or_default(),
                    );
                    html = html.replace(
                        "{{PGADMIN_URL}}",
                        &cfg.pgadmin_url.clone().unwrap_or_default(),
                    );
                    html = html.replace("{{VERSION}}", &cfg.version.to_string());

                    // Build and send the email
                    if let Err(err) = client.send_html("🎉 Server Startup", html, &cfg).await {
                        error!("Failed to send startup email: {:?}", err);
                    } else {
                        info!(
                            "Startup email successfully sent to {}",
                            cfg.email_to.as_deref().unwrap_or("<unset>")
                        );
                    }
                }
                Err(err) => {
                    error!(
                        "Unable to construct EmailClient for startup email: {:?}",
                        err
                    );
                }
            }
        }
        
        
        // 3) Build the actual scheduler
        let mut sched = JobScheduler::new();
        
        // 4) Define and add the job
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
        
        // 5) Start the scheduler loop
        sched.start().await.expect("Scheduler failed to start");
    });
}

/// Spawns a daily-at-midnight cron job that calls the `/update-storage-statistics` endpoint,
/// and also triggers one immediate run on startup.
pub async fn spawn_storage_update_cron(cfg: Config) {
    // clone for the task
    let cfg = cfg.clone();
    let http = Client::new();
    
    tokio::spawn(async move {
        let port = cfg.listen_addr.port();
        let url = format!("http://localhost:{}/update-storage-statistics", port);
        let ts_fmt = "%a, %b %e %Y at %I:%M:%S %p %:z";
        
        // Immediate storage stats update
        {
            let api_key = cfg.auth_key.clone();
            let now = Local::now().format(ts_fmt).to_string();
            
            match http
            .get(&url)
            .header("X-API-Key", api_key)
            .send()
            .await
            {
                Ok(resp) if resp.status().is_success() => {
                    info!("Startup storage stats update succeeded at {}", now);
                }
                Ok(resp) => error!(
                    "Startup storage stats update returned {} at {}",
                    resp.status(),
                    now
                ),
                Err(e) => error!(
                    "Startup storage stats update failed at {}: {}",
                    now, e
                ),
            }
        }
        
        // Preview next run
        let expr = "0 0 0 * * *"; // quartz 6-field: sec=0, min=0, hour=0, daily
        let schedule = Schedule::from_str(expr)
        .expect("Invalid cron expression for storage update");
        
        // Pull the next run in UTC
        let next_utc: DateTime<Utc> = schedule
        .upcoming(Utc)
        .next()
        .expect("Unable to compute next schedule");
        
        // Convert to local zone for display
        let next_local: DateTime<Local> = next_utc.with_timezone(&Local);
        
        info!(
            "Next storage stats update is at {}",
            next_local.format(ts_fmt)
        );
        
        // Build scheduler
        let mut sched = JobScheduler::new();
        
        // Build and add the job
        let job = Job::new_async(expr, move |_uuid, _l| {
            // clone inside closure
            let api_key = cfg.auth_key.clone();
            let http    = http.clone();
            let url     = url.clone();
            
            Box::pin(async move {
                let now = Local::now().format(&ts_fmt).to_string();
                match http
                .get(&url)
                .header("X-API-Key", api_key)
                .send()
                .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        info!("Storage stats update succeeded at {}", now)
                    },
                    Ok(resp) => error!(
                        "Storage stats update returned {} at {}",
                        resp.status(),
                        now
                    ),
                    Err(e) => error!(
                        "Storage stats update POST failed at {}: {}",
                        now, e
                    ),
                }
            })
        })
        .expect("Invalid cron expression for storage update");
        
        sched.add(job).expect("Failed to add storage update cron job");
        
        // Start the scheduler loop
        sched.start().await.expect("Scheduler failed to start");
    });
}