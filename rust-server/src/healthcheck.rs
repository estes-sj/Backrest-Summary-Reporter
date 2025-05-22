use reqwest::Client;
use tracing::{info, warn, error};

/// Healthcheck States
#[derive(Debug, Clone, Copy)]
pub enum HealthStatus {
    Start,
    Success,
    Fail,
}

/// Fire off a non‐blocking POST to your healthcheck URL (if any).
///
/// - `hc_url`:  `cfg.healthcheck_url`
/// - `status`:  START, SUCCESS or FAIL
/// - `message`: optional body text (falls back to a default per status)
///
pub fn ping_healthcheck(
    hc_url: &Option<String>,
    status: HealthStatus,
    message: Option<&str>,
) {
    // only do anything if a URL was configured
    if let Some(base) = hc_url {
        // build the final URL
        let url = match status {
            HealthStatus::Start   => format!("{}/start", base.trim_end_matches('/')),
            HealthStatus::Fail    => format!("{}/fail",  base.trim_end_matches('/')),
            HealthStatus::Success => base.clone(),
        };

        // pick your body text
        let body = match status {
            HealthStatus::Start   => message.unwrap_or("Start").to_string(),
            HealthStatus::Success => message.unwrap_or("Success").to_string(),
            HealthStatus::Fail    => message.unwrap_or("Failure").to_string(),
        };

        // spawn a task so it doesn’t block
        let url_clone = url.clone();
        tokio::spawn(async move {
            let client = Client::new();
            match client
                .post(&url_clone)
                .header("Content-Type", "text/plain")
                .body(body.clone())
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    info!("Healthcheck {} ping to {} succeeded", format!("{:?}", status), url_clone);
                }
                Ok(resp) => {
                    warn!(
                        "Healthcheck {:?} ping to {} returned {}",
                        status, url_clone, resp.status()
                    );
                }
                Err(e) => {
                    error!("Healthcheck {:?} ping to {} failed: {}", status, url_clone, e);
                }
            }
        });
    }
}