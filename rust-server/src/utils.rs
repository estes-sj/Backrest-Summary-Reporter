use axum::http::StatusCode;
use std::fs;

/// Log a failure, ping healthcheck(fail) with a combined message of
/// **static** + **formatted** details, but return only the static part
/// to the HTTP handler.
///
/// # Parameters
/// - `cfg`
///     Your application `Config`, so we can grab `cfg.healthcheck_url`.
/// - `static_msg`
///     A **`&'static str`** that will be returned to the HTTP caller.
/// - `format_args...`
///     A `format!`-style string plus any arguments to build the **detailed** part.
///
/// # Usage
/// ```ignore
/// .map_err(|e| {
///     fail!(
///         cfg,
///         "db error",                      // <-- static to client
///         "DB insert {} failed: {}",       // <-- detailed for logs/ping
///         path,
///         e
///     )
/// })?
/// ```
#[macro_export]
macro_rules! fail {
    ($cfg:expr, $static_msg:expr, $fmt:expr, $($arg:tt)+) => {{
        // build detailed message
        let detail = format!($fmt, $($arg)+);
        // combine with the static prefix
        let combined = format!("{}: {}", $static_msg, detail);
        tracing::error!("{}", combined);
        crate::healthcheck::ping_healthcheck(
            &$cfg.healthcheck_url,
            crate::healthcheck::HealthStatus::Fail,
            Some(&combined),
        );
        // return only the static part for the HTTP response
        (StatusCode::INTERNAL_SERVER_ERROR, $static_msg)
    }};
}

/// Log a warning, ping healthcheck(fail) with a combined message of
/// **static** + **formatted** details. Returns nothing, unlike fail().
///
/// # Parameters
/// - `healthcheck_url`
///     The healthcheck URL to ping.
/// - `static_msg`
///     A **`&'static str`** that will be returned to the HTTP caller.
/// - `format_args...`
///     A `format!`-style string plus any arguments to build the **detailed** part.
///
/// # Usage
/// ```ignore
/// .map_err(|e| {
///     fail!(
///         healthcheck_url,
///         "db error",                      // <-- static to client
///         "DB insert {} failed: {}",       // <-- detailed for logs/ping
///         path,
///         e
///     )
/// })?
/// ```
#[macro_export]
macro_rules! warn {
    ($healthcheck_url:expr, $static_msg:expr, $fmt:expr, $($arg:tt)+) => {{
        // build detailed message
        let detail = format!($fmt, $($arg)+);
        // combine with the static prefix
        let combined = format!("{}: {}", $static_msg, detail);
        tracing::error!("{}", combined);
        crate::healthcheck::ping_healthcheck(
            &$healthcheck_url,
            crate::healthcheck::HealthStatus::Fail,
            Some(&combined),
        );
    }};
}

/// Log an info, ping healthcheck(success) with a message of
/// **formatted** details, but return nothing.
///
/// # Parameters
/// - `cfg`
///     Your application `Config`.
/// - `format_args...`
///     A `format!`-style string plus any arguments to build the message.
///
/// # Usage
/// ```ignore
/// ok!(cfg, "processed {} items in {:.2}s", count, elapsed_secs);
/// ```
#[macro_export]
macro_rules! ok {
    ($cfg:expr, $fmt:expr, $($arg:tt)+) => {{
        // build detailed message
        let detail = format!($fmt, $($arg)+);
        // combine with a fixed "Success" prefix
        let combined = format!("Success: {}", detail);
        tracing::info!("{}", combined);
        crate::healthcheck::ping_healthcheck(
            &$cfg.healthcheck_url,
            crate::healthcheck::HealthStatus::Success,
            Some(&combined),
        );
    }};
}

/// Ping healthcheck(success) with a message of
/// **formatted** details, but return nothing.
///
/// # Parameters
/// - `cfg`
///     Your application `Config`.
/// - `format_args...`
///     A `format!`-style string plus any arguments to build the message.
///
/// # Usage
/// ```ignore
/// start!(cfg, "starting report process for {} to {}", start_date, end_date);
/// ```
#[macro_export]
macro_rules! start {
    ($cfg:expr, $fmt:expr, $($arg:tt)+) => {{
        // build detailed message
        let detail = format!($fmt, $($arg)+);
        // combine with a fixed "Start" prefix
        let combined = format!("Start: {}", detail);
        tracing::info!("{}", combined);
        crate::healthcheck::ping_healthcheck(
            &$cfg.healthcheck_url,
            crate::healthcheck::HealthStatus::Start,
            Some(&combined),
        );
    }};
}

/// Try to read the container ID from /etc/hostname,
/// or return `"Unknown"` if that fails.
pub fn container_id_from_hostname() -> String {
    fs::read_to_string("/etc/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Unknown".into())
}