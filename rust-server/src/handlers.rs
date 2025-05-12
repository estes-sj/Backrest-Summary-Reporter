use axum::{
    extract::{ConnectInfo, Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Duration, Local, Utc};
use fs2::{free_space, total_space};
use std::{fs, net::SocketAddr, path::Path};
use sqlx::{PgPool, Row};
use lettre::{
    AsyncTransport,
    message::{header::ContentType, Message},
    transport::smtp::{authentication::Credentials, AsyncSmtpTransport},
    Tokio1Executor,
};
use crate::{
    config::{Config,StorageConfig},
    models::{CombinedStats, CurrentStorageStats, DbStorageRow, EventTotals, EventTotalsReport, GenerateReport, PeriodStats, StatsRequest, SummaryPayload, StorageReport},
};

///
/// HANDLER METHODS
/// 

/// POST `/add-event` endpoint.
/// Inserts snapshot summary and statistics into the database.
pub async fn add_event_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
    Json(payload): Json<SummaryPayload>,
) -> impl IntoResponse {
    // 1) Auth
    if let Err(e) = validate_api_key_with_ip(&headers, &cfg.auth_key, addr) {
        return e;
    }

    // 2) Delegate to helper
    match insert_summary_with_stats(&pool, &payload).await {
        Ok((summary_id, created)) => {
            tracing::info!(
                "Event with ID {} at {} from {}",
                summary_id,
                created.to_rfc3339(),
                addr
            );
            (StatusCode::OK, "ok")
        }
        Err(err) => err,
    }
}

/// POST `/get-events-in-range` endpoint.
/// Takes in a start_date and end_date, returns the queried data between the provided times
pub async fn get_events_in_range_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
    Json(req): Json<StatsRequest>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // 1) Auth
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // 2) Delegate to helper to fetch the combined stats between the request start and end date
    let rows = fetch_combined_stats(&pool, req.start_date, req.end_date).await?;

    // 3) Return the formatted JSON
    Ok((StatusCode::OK, Json(rows)))
}

/// POST `/get-events-in-range-totals` endpoint.
/// Takes in a start_date and end_date, returns the queried data between the provided times
pub async fn get_events_in_range_totals_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
    Json(req): Json<StatsRequest>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // 1) Auth
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // 2) Fetch the aggregated totals
    let totals = load_event_totals_report(&pool, req.start_date, req.end_date).await?;

    // 3) Return JSON
    Ok((StatusCode::OK, Json(totals)))
}

/// GET `/send-test-email` endpoint.
/// Send a test email using the configured SMTP settings.
pub async fn send_test_email_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((_, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // 1) Auth
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

/// GET `/update-storage-statistics` endpoint.
/// Updates the configured storage mounts with the latest statistics.
pub async fn update_storage_statistics_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // 1) Auth
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // 2) Delegate insertion & report generation
    let reports = load_and_insert_storage_stats(&pool, &cfg.storage_mounts).await?;

    // 3) Log & return
    tracing::info!("Storage statistics updated from {}.", addr);
    Ok((StatusCode::OK, Json(reports)))
}

/// GET `/get-latest-storage-stats` endpoint.
/// Retrieves the latest storage statistics and its previous day, week, and month.
pub async fn get_latest_storage_stats_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // 1) Auth
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // 2) Delegate to our new helper
    let stats = load_storage_stats(&pool, &cfg.storage_mounts).await?;

    // 3) Return JSON
    Ok((StatusCode::OK, Json(stats)))
}

/// POST `/get-events-and-storage-stats` endpoint.
/// Takes in a start_date and end_date, returns the event totals between the provided times,
/// the queried data between the provided times,
/// updates the configured storage mounts with the latest statistics, 
/// and retrieves the latest storage statistics and its previous day, week, and month.
pub async fn get_events_and_storage_stats_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, cfg)): State<(PgPool, Config)>,
    headers: HeaderMap,
    Json(req): Json<StatsRequest>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // 1) Auth
    validate_api_key_with_ip(&headers, &cfg.auth_key, addr)?;

    // 2) Fetch the combined event totals
    let event_totals = load_event_totals_report(&pool, req.start_date, req.end_date).await?;

    // 3) Delegate to helper to fetch the combined stats between the request start and end date
    let snapshot_summaries = fetch_combined_stats(&pool, req.start_date, req.end_date).await?;

    // 4) Trigger an update to update the storage statistics
    load_and_insert_storage_stats(&pool, &cfg.storage_mounts).await?;
    
    // 5) Get latest storage statistics
    let storage_statistics = load_storage_stats(&pool, &cfg.storage_mounts).await?;
    
    // 6) Return the combined report
    let payload = GenerateReport {
        event_totals,
        snapshot_summaries,
        storage_statistics,
    };
    Ok((StatusCode::OK, Json(payload)))
}

///
/// DATABASE QUERY METHODS
/// 

/// Inserts a new summary (and optional snapshot_stats), returning `(id, created_at)`.
pub async fn insert_summary_with_stats(
    pool: &PgPool,
    payload: &SummaryPayload,
) -> Result<(i32, DateTime<Utc>), (StatusCode, &'static str)> {
    // 1) Determine created_at in UTC
    let created_at = Local::now().with_timezone(&Utc);

    // 2) Insert into summaries dynamically
    let row = sqlx::query(
        r#"
        INSERT INTO summaries (
          created_at, task, time, event, repo, plan, snapshot, error
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, created_at
        "#
    )
    .bind(created_at)
    .bind(&payload.task)
    .bind(&payload.time)
    .bind(&payload.event)
    .bind(&payload.repo)
    .bind(&payload.plan)
    .bind(&payload.snapshot)
    .bind(&payload.error)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB insert summary error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "db error")
    })?;

    // Extract with dynamic get
    let summary_id: i32 = row.get("id");
    let created: DateTime<Utc> = row.get("created_at");

    // 3) Insert snapshot_stats if present
    if let Some(stats) = &payload.snapshot_stats {
        sqlx::query(
            r#"
            INSERT INTO snapshot_stats (
                summary_id, message_type, error, during, item,
                files_new, files_changed, files_unmodified,
                dirs_new, dirs_changed, dirs_unmodified,
                data_blobs, tree_blobs, data_added,
                total_files_processed, total_bytes_processed, total_duration,
                snapshot_id, percent_done,
                total_files, files_done, total_bytes, bytes_done, current_files
            ) VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8,
                $9, $10, $11,
                $12, $13, $14,
                $15, $16, $17,
                $18, $19,
                $20, $21, $22, $23, $24
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
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("DB insert snapshot_stats error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error")
        })?;
    }

    Ok((summary_id, created))
}

/// Fetches all `CombinedStats` between two instants, or returns a `(StatusCode, &str)` error.
pub async fn fetch_combined_stats(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<CombinedStats>, (StatusCode, &'static str)> {
    sqlx::query_as::<_, CombinedStats>(r#"
        SELECT
            s.id             AS summary_id,
            s.created_at,
            s.task, s.time, s.event, s.repo, s.plan, s.snapshot, s.error,

            ss.message_type,
            ss.error         AS ss_error,
            ss.during,
            ss.item,

            ss.files_new,
            ss.files_changed,
            ss.files_unmodified,

            ss.dirs_new,
            ss.dirs_changed,
            ss.dirs_unmodified,

            ss.data_blobs,
            ss.tree_blobs,
            ss.data_added,

            ss.total_files_processed,
            ss.total_bytes_processed,
            ss.total_duration,
            ss.snapshot_id   AS ss_snapshot,

            ss.percent_done,
            ss.total_files,
            ss.files_done,
            ss.total_bytes,
            ss.bytes_done,
            ss.current_files
        FROM summaries s
        LEFT JOIN snapshot_stats ss ON ss.summary_id = s.id
        WHERE s.time BETWEEN $1 AND $2
    "#)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB query error in fetch_combined_stats: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "db error")
    })
}

/// Fetches all of the aggregated counters between `start` and `end`.
pub async fn fetch_event_totals(
    pool: &PgPool,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<EventTotals, (StatusCode, &'static str)> {
    // We LEFT JOIN snapshot_stats so that events without stats still count.
    let row: EventTotals = sqlx::query_as::<_, EventTotals>(
        r#"
        SELECT
          $1::timestamptz  AS start_date,
          $2::timestamptz  AS end_date,

          COALESCE(COUNT(*)                                                           , 0)::BIGINT AS total_events,
          COALESCE(SUM(CASE WHEN s.event ILIKE '%snapshot success%' THEN 1 ELSE 0 END), 0)::BIGINT AS total_snapshot_success,
          COALESCE(SUM(CASE WHEN s.event ILIKE '%snapshot error%'   THEN 1 ELSE 0 END), 0)::BIGINT AS total_snapshot_error,
          COALESCE(SUM(CASE WHEN s.event ILIKE '%forget success%'   THEN 1 ELSE 0 END), 0)::BIGINT AS total_forget_success,
          COALESCE(SUM(CASE WHEN s.event ILIKE '%forget error%'     THEN 1 ELSE 0 END), 0)::BIGINT AS total_forget_error,
    
          COALESCE(SUM(ss.files_new)       , 0)::BIGINT AS total_files_new,
          COALESCE(SUM(ss.files_changed)   , 0)::BIGINT AS total_files_changed,
          COALESCE(SUM(ss.files_unmodified), 0)::BIGINT AS total_files_unmodified,

          COALESCE(SUM(ss.dirs_new)       , 0)::BIGINT AS total_dirs_new,
          COALESCE(SUM(ss.dirs_changed)   , 0)::BIGINT AS total_dirs_changed,
          COALESCE(SUM(ss.dirs_unmodified), 0)::BIGINT AS total_dirs_unmodified,

          COALESCE(SUM(ss.data_blobs)            , 0)::BIGINT AS total_data_blobs,
          COALESCE(SUM(ss.tree_blobs)            , 0)::BIGINT AS total_tree_blobs,
          COALESCE(SUM(ss.data_added)            , 0)::BIGINT AS total_data_added,
          COALESCE(SUM(ss.total_files_processed) , 0)::BIGINT AS total_files_processed,
          COALESCE(SUM(ss.total_bytes_processed) , 0)::BIGINT AS total_bytes_processed,
          COALESCE(SUM(ss.total_duration)        , 0)::BIGINT AS total_duration
    
        FROM summaries s
        LEFT JOIN snapshot_stats ss ON ss.summary_id = s.id
        WHERE s.time BETWEEN $1 AND $2
        "#
    )
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB aggregation error: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "db error")
    })?;

    Ok(row)
}

/// Build a report of event totals for current, previous day/week/month.
pub async fn load_event_totals_report(
    pool: &PgPool,
    start: DateTime<Utc>,
    end:   DateTime<Utc>,
) -> Result<EventTotalsReport, (StatusCode, &'static str)> {
    // 1) Current window
    let current = fetch_event_totals(pool, start, end).await?;

    // 2) Compute cutoffs
    let day_start   = start   - Duration::days(1);
    let day_end     = end     - Duration::days(1);
    let week_start  = start   - Duration::weeks(1);
    let week_end    = end     - Duration::weeks(1);
    let month_start = start   - Duration::days(30);
    let month_end   = end     - Duration::days(30);

    // 3) Previous windows
    let previous_day   = fetch_event_totals(pool, day_start, day_end).await.ok();
    let previous_week  = fetch_event_totals(pool, week_start, week_end).await.ok();
    let previous_month = fetch_event_totals(pool, month_start, month_end).await.ok();

    // 4) Assemble report
    Ok(EventTotalsReport {
        current,
        previous_day,
        previous_week,
        previous_month,
    })
}

/// Stats all mounts, inserts into the DB, and returns the summaries.
pub async fn load_and_insert_storage_stats(
    pool: &PgPool,
    mounts: &[StorageConfig],
) -> Result<Vec<StorageReport>, (StatusCode, &'static str)> {
    let mut reports = Vec::with_capacity(mounts.len());

    for mount in mounts {
        let path = &mount.path;
        let nickname = mount.nickname.clone();

        // filesystem stats
        let total_bytes = total_space(path).map_err(|e| {
            tracing::error!("stat total {} failed: {}", path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Filesystem stat error")
        })?;
        let free_bytes = free_space(path).map_err(|e| {
            tracing::error!("stat free {} failed: {}", path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Filesystem stat error")
        })?;
        let used_bytes = total_bytes.saturating_sub(free_bytes);

        // insert
        sqlx::query(
            r#"
            INSERT INTO storage (
                storage_location,
                storage_nickname,
                storage_used_bytes,
                storage_total_bytes
            ) VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(path)
        .bind(&nickname)
        .bind(used_bytes as i64)
        .bind(total_bytes as i64)
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("DB insert {} failed: {}", path, e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error")
        })?;

        reports.push(StorageReport {
            location:   path.clone(),
            nickname,
            used_bytes,
            total_bytes,
        });
    }

    Ok(reports)
}

/// Fetches *all* current + prior (day/week/month) stats for the given mounts.
pub async fn load_storage_stats(
    pool: &PgPool,
    mounts: &[StorageConfig],
) -> Result<Vec<CurrentStorageStats>, (StatusCode, &'static str)> {
    // 1) Grab the latest row for each mount
    let current_rows: Vec<DbStorageRow> = sqlx::query_as::<_, DbStorageRow>(
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
    .fetch_all(pool)
    .await
    .map_err(|e| {
        tracing::error!("DB query error for current storage: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "db error")
    })?;

    // 2) Prepare a helper for fetching the most recent before a cutoff
    async fn fetch_prior(
        pool: &PgPool,
        path: &str,
        cutoff: DateTime<Utc>,
    ) -> Result<Option<DbStorageRow>, sqlx::Error> {
        sqlx::query_as::<_, DbStorageRow>(
            r#"
            SELECT
                storage_location,
                storage_nickname,
                storage_used_bytes,
                storage_total_bytes,
                time_added
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

    // 3) Build the response vector
    let mut out = Vec::with_capacity(mounts.len());
    for mount in mounts {
        if let Some(cur) = current_rows.iter().find(|r| &r.storage_location == &mount.path) {
            // pack current
            let make_period = |r: &DbStorageRow| PeriodStats {
                used_bytes:   r.storage_used_bytes,
                free_bytes:   r.storage_total_bytes.saturating_sub(r.storage_used_bytes),
                total_bytes:  r.storage_total_bytes,
                percent_used: if r.storage_total_bytes > 0 {
                    (r.storage_used_bytes as f64 / r.storage_total_bytes as f64) * 100.0
                } else { 0.0 },
                time_added:   r.time_added,
            };
            let current = make_period(cur);

            // compute cutoffs
            let t = cur.time_added;
            let cuts = [
                ("day",   t - Duration::days(1)),
                ("week",  t - Duration::weeks(1)),
                ("month", t - Duration::days(30)),
            ];

            // fetch priors
            let mut priors = [None, None, None];
            for (i, &(_, cutoff)) in cuts.iter().enumerate() {
                priors[i] = fetch_prior(pool, &mount.path, cutoff)
                    .await
                    .map_err(|e| {
                        tracing::error!("DB fetch prior for {} cutoff {:?}: {}", mount.path, cutoff, e);
                        (StatusCode::INTERNAL_SERVER_ERROR, "db error")
                    })?;
            }

            let previous_day   = priors[0].as_ref().map(|r| make_period(r));
            let previous_week  = priors[1].as_ref().map(|r| make_period(r));
            let previous_month = priors[2].as_ref().map(|r| make_period(r));

            // use config nickname preferentially
            let nickname = mount.nickname.clone().or(cur.storage_nickname.clone());

            out.push(CurrentStorageStats {
                location:       mount.path.clone(),
                nickname,
                current,
                previous_day,
                previous_week,
                previous_month,
            });
        }
    }

    Ok(out)
}

///
/// HELPER METHODS
/// 

/// Validates the API key provided in the headers.
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