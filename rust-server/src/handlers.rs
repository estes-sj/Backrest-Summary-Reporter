use axum::{
    extract::{ConnectInfo, Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Duration, Local, Utc};
use fs2::{free_space, total_space};
use std::{collections::HashMap, fs, net::SocketAddr, path::Path};
use sqlx::{PgPool, Row};
use lettre::{
    AsyncTransport,
    message::{header::ContentType, Message},
    transport::smtp::{authentication::Credentials, AsyncSmtpTransport},
    Tokio1Executor,
};
use crate::{
    config::{Config,StorageConfig},
    models::{CombinedStats, CurrentStorageStats, DbStorageRow, PeriodStats, StatsRequest, SummaryPayload, StorageReport},
};

/// HTTP handler for `/summary` endpoint.
/// Validates API key, inserts summary and snapshot_stats into the database.
pub async fn summary_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
    Json(payload): Json<SummaryPayload>,
) -> impl IntoResponse {

    // Validate API Key
    if let Err(e) = validate_api_key_with_ip(&headers, &cfg.auth_key, addr) {
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
            r#"
            INSERT INTO snapshot_stats (
                -- General information
                summary_id,
                message_type,
                error,
                during,
                item,
    
                -- File stats
                files_new,
                files_changed,
                files_unmodified,
    
                -- Directory stats
                dirs_new,
                dirs_changed,
                dirs_unmodified,
    
                -- Blob stats
                data_blobs,
                tree_blobs,
                data_added,
    
                -- Processing stats
                total_files_processed,
                total_bytes_processed,
                total_duration,
    
                -- Snapshot info
                snapshot_id,
                percent_done,
    
                -- Progress info
                total_files,
                files_done,
                total_bytes,
                bytes_done,
                current_files
            ) VALUES (
                $1, $2, $3, $4, $5,      -- General information
                $6, $7, $8,              -- File stats
                $9, $10, $11,            -- Directory stats
                $12, $13, $14,           -- Blob stats
                $15, $16, $17,           -- Processing stats
                $18, $19,                -- Snapshot info
                $20, $21, $22, $23, $24  -- Progress info
            )
            "#
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
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
    Json(req): Json<StatsRequest>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {

    // Validate API Key
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // Dynamic query_as, mapping into CombinedStats (FromRow)
    let rows: Vec<CombinedStats> = sqlx::query_as::<_, CombinedStats>(
        r#"
        SELECT
            -- Summary fields
            s.id AS summary_id,
            s.created_at,
            s.task,
            s.time,
            s.event,
            s.repo,
            s.plan,
            s.snapshot,
            s.error,
    
            -- Snapshot stats fields
            ss.message_type,
            ss.error AS ss_error,
            ss.during,
            ss.item,
    
            -- File stats
            ss.files_new,
            ss.files_changed,
            ss.files_unmodified,
    
            -- Directory stats
            ss.dirs_new,
            ss.dirs_changed,
            ss.dirs_unmodified,
    
            -- Blob stats
            ss.data_blobs,
            ss.tree_blobs,
            ss.data_added,
    
            -- Processing stats
            ss.total_files_processed,
            ss.total_bytes_processed,
            ss.total_duration,
            ss.snapshot_id AS ss_snapshot,
    
            -- Progress
            ss.percent_done,
            ss.total_files,
            ss.files_done,
            ss.total_bytes,
            ss.bytes_done,
            ss.current_files
    
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

/// GET /test_email
pub async fn test_email_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((_, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // 1) API key + IP check
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // 2) Ensure we have all SMTP settings
    let host = cfg.smtp_host.as_deref().ok_or_else(|| {
        tracing::error!("SMTP_HOST not configured");
        (StatusCode::INTERNAL_SERVER_ERROR, "SMTP not configured")
    })?;
    let username = cfg.smtp_username.as_deref().unwrap_or_default();
    let password = cfg.smtp_password.as_deref().unwrap_or_default();
    let from = cfg.email_from.as_deref().ok_or_else(|| {
        tracing::error!("EMAIL_FROM not configured");
        (StatusCode::INTERNAL_SERVER_ERROR, "SMTP not configured")
    })?;
    let to = cfg.email_to.as_deref().ok_or_else(|| {
        tracing::error!("EMAIL_TO not configured");
        (StatusCode::INTERNAL_SERVER_ERROR, "SMTP not configured")
    })?;

    // 3) Read the test email HTML
    let html_path = Path::new("html/test_email.html");

    let mut html = fs::read_to_string(&html_path)
    .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "Failed to read HTML file"))?;

    // 4) Modify the timestamp
    let timestamp = Local::now().format("%d/%m/%Y at %I:%M %p").to_string();
    html = html.replace("{{TIMESTAMP}}", &timestamp);

    // 5) Build the email
    let email = Message::builder()
        .from(from.parse().expect("valid EMAIL_FROM"))
        .to(to.parse().expect("valid EMAIL_TO"))
        .header(ContentType::TEXT_HTML)
        .subject("🚀 Test Email")
        .body(html)
        .map_err(|e| {
            tracing::error!("Failed to build email: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Email build error")
        })?;

    // 6) Configure & send
    let creds = Credentials::new(username.into(), password.into());
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(host)
        .map_err(|e| {
            tracing::error!("SMTP relay config failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "SMTP config error")
        })?
        .credentials(creds)
        .build();

    mailer
        .send(email)
        .await
        .map(|_| {
            tracing::info!("Test email successfully sent to {}", to);
            (StatusCode::OK, "Test email sent")
        })
        .map_err(|e| {
            tracing::error!("Failed to send email: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to send email")
        })
}

/// POST /storage_stats
pub async fn storage_stats_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {

    // 1) API key + IP check
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // 2) Gather stats and insert
    let mut reports = Vec::new();

    for mount in &cfg.storage_mounts {
        let path = &mount.path;
        let nickname = mount.nickname.clone();

        let total_bytes = total_space(path).map_err(|e| {
            tracing::error!("stat total {} failed: {}", path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Filesystem stat error")
        })?;
        let free_bytes = free_space(path).map_err(|e| {
            tracing::error!("stat free {} failed: {}", path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Filesystem stat error")
        })?;
        let used_bytes = total_bytes.saturating_sub(free_bytes);

        // 3) Insert into DB
        sqlx::query(
            r#"
            INSERT INTO storage (
              storage_location,
              storage_nickname,
              storage_used_bytes,
              storage_total_bytes
            ) VALUES ($1, $2, $3, $4)
            "#
        )
        .bind(path)
        .bind(&nickname)
        .bind(used_bytes as i64)
        .bind(total_bytes as i64)
        .execute(&pool)
        .await
        .map_err(|e| {
            tracing::error!("DB insert {} failed: {}", path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error")
        })?;

        reports.push(StorageReport {
            location: path.clone(),
            nickname,
            used_bytes: used_bytes,
            total_bytes: total_bytes,
        });
    }

    // 4) Return JSON summary
    tracing::info!("Storage statistics updated from {}.", addr);
    Ok((StatusCode::OK, Json(reports)))
}

// GET /current_storage_stats
pub async fn get_current_storage_stats_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // 1) API key + IP check
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // 2) Map configured mounts
    let mount_map: HashMap<String, Option<String>> =
        cfg.storage_mounts.iter()
            .map(|m| (m.path.clone(), m.nickname.clone()))
            .collect();

    // 3) Fetch latest per mount
    let current_rows: Vec<DbStorageRow> = sqlx::query_as(
        r#"
        SELECT DISTINCT ON (storage_location)
            storage_location,
            storage_nickname,
            storage_used_bytes,
            storage_total_bytes,
            time_added
        FROM storage
        ORDER BY storage_location, time_added DESC
        "#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| {
        tracing::error!("DB query error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "db error")
    })?;

    // Helper to fetch prior row
    async fn fetch_prior(
        pool: &PgPool,
        path: &str,
        cutoff: DateTime<Utc>,
    ) -> Result<Option<DbStorageRow>, sqlx::Error> {
        sqlx::query_as(
            r#"
            SELECT storage_location, storage_nickname, storage_used_bytes,
                   storage_total_bytes, time_added
            FROM storage
            WHERE storage_location = $1
              AND time_added <= $2
            ORDER BY time_added DESC
            LIMIT 1
            "#,
        )
        .bind(path)
        .bind(cutoff)
        .fetch_optional(pool)
        .await
    }

    // 4) Build response
    let mut results = Vec::with_capacity(cfg.storage_mounts.len());
    for StorageConfig { path, .. } in &cfg.storage_mounts {
        if let Some(cur) = current_rows.iter().find(|r| &r.storage_location == path) {
            // pack a PeriodStats
            let make_period = |row: &DbStorageRow| PeriodStats {
                used_bytes: row.storage_used_bytes,
                free_bytes: row.storage_total_bytes.saturating_sub(row.storage_used_bytes),
                total_bytes: row.storage_total_bytes,
                percent_used: if row.storage_total_bytes > 0 {
                    (row.storage_used_bytes as f64 / row.storage_total_bytes as f64) * 100.0
                } else { 0.0 },
                time_added: row.time_added,
            };

            let current = make_period(cur);
            // cutoffs
            let t = cur.time_added;
            let d_cut = t - Duration::days(1);
            let w_cut = t - Duration::weeks(1);
            let m_cut = t - Duration::days(30);

            // fetch periods
            let d_row = fetch_prior(&pool, path, d_cut).await.map_err(|e| {
                tracing::error!("DB fetch prior day for {}: {}", path, e);
                (StatusCode::INTERNAL_SERVER_ERROR, "db error")
            })?;
            let w_row = fetch_prior(&pool, path, w_cut).await.map_err(|e| {
                tracing::error!("DB fetch prior week for {}: {}", path, e);
                (StatusCode::INTERNAL_SERVER_ERROR, "db error")
            })?;
            let m_row = fetch_prior(&pool, path, m_cut).await.map_err(|e| {
                tracing::error!("DB fetch prior month for {}: {}", path, e);
                (StatusCode::INTERNAL_SERVER_ERROR, "db error")
            })?;

            let previous_day   = d_row.as_ref().map(|r| make_period(r));
            let previous_week  = w_row.as_ref().map(|r| make_period(r));
            let previous_month = m_row.as_ref().map(|r| make_period(r));

            let nickname = mount_map.get(path).cloned().unwrap_or(cur.storage_nickname.clone());

            results.push(CurrentStorageStats {
                location: path.clone(),
                nickname,
                current,
                previous_day,
                previous_week,
                previous_month,
            });
        }
    }

    tracing::info!("Returned grouped storage stats to {}", addr);
    Ok((StatusCode::OK, Json(results)))
}

/// TODO: Move to utils class
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