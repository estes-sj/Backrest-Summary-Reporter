use axum::{
    extract::{ConnectInfo, Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Local, Utc};
use sqlx::{Row, PgPool};
use std::net::SocketAddr;

use crate::models::{SummaryPayload, StatsRequest, CombinedStats};

/// HTTP handler for `/summary` endpoint.
/// Validates API key, inserts summary and snapshot_stats into the database.
pub async fn summary_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, auth_key)): State<(PgPool, String)>,
    headers: HeaderMap,
    Json(payload): Json<SummaryPayload>,
) -> impl IntoResponse {
    // API key validation
    let provided = headers.get("X-API-Key").and_then(|v| v.to_str().ok()).unwrap_or("");
    if provided != auth_key {
        tracing::warn!("Unauthorized request from {}: provided API key '{}'", addr, provided);
        return (StatusCode::UNAUTHORIZED, "unauthorized");
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

pub async fn get_stats_handler(
    State((pool, auth_key)): State<(PgPool, String)>,
    headers: HeaderMap,
    Json(req): Json<StatsRequest>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    // API key validation
    let provided = headers.get("X-API-Key").and_then(|v| v.to_str().ok()).unwrap_or("");
    if provided != auth_key {
        tracing::warn!("Unauthorized request from {}: provided API key '{}'", addr, provided);
        return (StatusCode::UNAUTHORIZED, "unauthorized");
    }

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
        WHERE s.created_at BETWEEN $1 AND $2
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