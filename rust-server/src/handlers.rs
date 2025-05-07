use axum::{
    extract::{ConnectInfo, Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Local, Utc};
use sqlx::{Row, PgPool};
use std::{env, net::SocketAddr};
use lettre::{
    Message,
    transport::smtp::{AsyncSmtpTransport, authentication::Credentials},
    Tokio1Executor,
    Transport,            // for sync .send, though you won’t need this here
    AsyncTransport,       // <-- import this to get async .send(...)
};
use crate::models::{SummaryPayload, StatsRequest, CombinedStats};

/// HTTP handler for `/summary` endpoint.
/// Validates API key, inserts summary and snapshot_stats into the database.
pub async fn summary_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, auth_key)): State<(PgPool, String)>,
    headers: HeaderMap,
    Json(payload): Json<SummaryPayload>,
) -> impl IntoResponse {

    // Validate API Key
    if let Err(e) = validate_api_key_with_ip(&headers, &auth_key, addr) {
        return e;
    }

    // Determine created_at timestamp in server's local timezone
    let created_at = Local::now().with_timezone(&Utc); // Use DateTime<Utc> for SQL compatibility

    // Insert into summaries, returning id and created_at
    let row = sqlx::query(
        "INSERT INTO summaries (created_at, task, time, event, repo, plan, snapshot, error)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, created_at"
    )
    .bind(&created_at)
    .bind(&payload.task)
    .bind(&payload.time)
    .bind(&payload.event)
    .bind(&payload.repo)
    .bind(&payload.plan)
    .bind(&payload.snapshot)
    .bind(&payload.error)
    .fetch_one(&pool)
    .await;

    let (summary_id, created): (i32, DateTime<Utc>) = match row {
        Ok(r) => (r.get("id"), r.get("created_at")),
        Err(e) => {
            eprintln!("DB insert summary error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    // Insert snapshot_stats if present
    if let Some(stats) = payload.snapshot_stats {
        if let Err(e) = sqlx::query(
            "INSERT INTO snapshot_stats (
                summary_id, message_type, error, during, item,
                files_new, files_changed, files_unmodified,
                dirs_new, dirs_changed, dirs_unmodified,
                data_blobs, tree_blobs, data_added,
                total_files_processed, total_bytes_processed,
                total_duration, snapshot_id, percent_done,
                total_files, files_done, total_bytes, bytes_done, current_files
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9, $10, $11,
                $12, $13, $14,
                $15, $16,
                $17, $18, $19,
                $20, $21, $22, $23, $24
            )"
        )
        .bind(summary_id)
        .bind(&stats.message_type)
        .bind(&stats.error)
        .bind(&stats.during)
        .bind(&stats.item)
        .bind(stats.files_new)
        .bind(stats.files_changed)
        .bind(stats.files_unmodified)
        .bind(stats.dirs_new)
        .bind(stats.dirs_changed)
        .bind(stats.dirs_unmodified)
        .bind(stats.data_blobs)
        .bind(stats.tree_blobs)
        .bind(stats.data_added)
        .bind(stats.total_files_processed)
        .bind(stats.total_bytes_processed)
        .bind(stats.total_duration)
        .bind(&stats.snapshot_id)
        .bind(stats.percent_done)
        .bind(stats.total_files)
        .bind(stats.files_done)
        .bind(stats.total_bytes)
        .bind(stats.bytes_done)
        .bind(stats.current_files)
        .execute(&pool)
        .await
        {
            eprintln!("DB insert stats error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    }

    tracing::info!("Event with ID {} at {} from {}", summary_id, created.to_rfc3339(), addr);
    (StatusCode::OK, "ok")
}

/// HTTP handler for `/get_stats` endpoint.
/// Validates API key, takes in a start_date and end_date, returns the queried data between the provided times
pub async fn get_stats_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, auth_key)): State<(PgPool, String)>,
    headers: HeaderMap,
    Json(req): Json<StatsRequest>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {

    // Validate API Key
    validate_api_key_with_ip(&headers, &auth_key, addr)?;

    // Dynamic query_as, mapping into CombinedStats (FromRow)
    let rows: Vec<CombinedStats> = sqlx::query_as::<_, CombinedStats>(
        r#"
        SELECT
          s.id AS summary_id,
          s.created_at,
          s.task, s.time, s.event, s.repo, s.plan, s.snapshot, s.error,
          ss.message_type, ss.error       AS ss_error, ss.during, ss.item,
          ss.files_new, ss.files_changed, ss.files_unmodified,
          ss.dirs_new, ss.dirs_changed, ss.dirs_unmodified,
          ss.data_blobs, ss.tree_blobs, ss.data_added,
          ss.total_files_processed, ss.total_bytes_processed,
          ss.total_duration, ss.snapshot_id AS ss_snapshot,
          ss.percent_done, ss.total_files, ss.files_done,
          ss.total_bytes, ss.bytes_done, ss.current_files
        FROM summaries s
        LEFT JOIN snapshot_stats ss ON ss.summary_id = s.id
        WHERE s.time BETWEEN $1 AND $2
        "#
    )
    .bind(req.start_date)
    .bind(req.end_date)
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        eprintln!("DB query error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "db error")
    })?;

    Ok((StatusCode::OK, Json(rows)))
}

///
/// HELPER METHODS
/// 

pub fn validate_api_key_with_ip(
    headers: &HeaderMap,
    expected_key: &str,
    addr: SocketAddr,
) -> Result<(), (StatusCode, &'static str)> {
    let provided = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if provided != expected_key {
        tracing::warn!(
            "Unauthorized request from {}: provided API key '{}'",
            addr,
            provided
        );
        return Err((StatusCode::UNAUTHORIZED, "unauthorized"));
    }

    Ok(())
}


/// GET or POST /test_email
pub async fn test_email_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((_, auth_key)): State<(PgPool, String)>,
    headers: HeaderMap,
) -> impl IntoResponse {
    // 1) API key + IP check
    if let Err(err) = validate_api_key_with_ip(&headers, &auth_key, addr) {
        return err;
    }

    // 2) Load SMTP settings from env
    let smtp_host = match env::var("SMTP_HOST") {
        Ok(h) => h,
        Err(_) => {
            tracing::error!("SMTP_HOST not set");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Missing SMTP_HOST");
        }
    };
    let smtp_username = env::var("SMTP_USERNAME").unwrap_or_default();
    let smtp_password = env::var("SMTP_PASSWORD").unwrap_or_default();
    let from_addr = match env::var("EMAIL_FROM") {
        Ok(a) => a.parse().expect("EMAIL_FROM is not a valid address"),
        Err(_) => {
            tracing::error!("EMAIL_FROM not set");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Missing EMAIL_FROM");
        }
    };
    let to_addr = match env::var("EMAIL_TO") {
        Ok(a) => a.parse().expect("EMAIL_TO is not a valid address"),
        Err(_) => {
            tracing::error!("EMAIL_TO not set");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Missing EMAIL_TO");
        }
    };

    // 3) Build the email
    let email = match Message::builder()
        .from(from_addr)
        .to(to_addr)
        .subject("🚀 Test email from Axum + Lettre")
        .body(String::from("This is a test email sent from your /test_email endpoint."))
    {
        Ok(msg) => msg,
        Err(e) => {
            tracing::error!("Failed to build email: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Email build error");
        }
    };

    // 4) Configure and send via async SMTP transport
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_host)
        .and_then(|builder| {
            let creds = Credentials::new(smtp_username.clone(), smtp_password.clone());
            Ok(builder.credentials(creds).build())
        })
        .map_err(|e| {
            tracing::error!("SMTP relay setup failed: {}", e);
            e
        });

    let mailer = match mailer {
        Ok(m) => m,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "SMTP config error"),
    };

    match mailer.send(email).await {
        Ok(_) => (StatusCode::OK, "Test email sent successfully"),
        Err(e) => {
            tracing::error!("Failed to send test email: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to send email")
        }
    }
}