use axum::{
    extract::{ConnectInfo, Json, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use chrono::{DateTime, Local, Utc};
use sqlx::Row;
use std::net::SocketAddr;

use crate::models::SummaryPayload;
use sqlx::PgPool;

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
        "INSERT INTO summaries (created_at, task, time, event, repo, plan, snapshot)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, created_at"
    )
    .bind(&created_at)
    .bind(&payload.task)
    .bind(&payload.time)
    .bind(&payload.event)
    .bind(&payload.repo)
    .bind(&payload.plan)
    .bind(&payload.snapshot)
    .fetch_one(&pool)
    .await;

    let (summary_id, created): (i32, DateTime<Utc>) = match row {
        Ok(r) => (r.get("id"), r.get("created_at")),
        Err(e) => {
            eprintln!("DB insert summary error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    // Insert into snapshot_stats
    let stats = payload.snapshot_stats;
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

    tracing::info!("Inserted summary {} at {} from {}", summary_id, created.to_rfc3339(), addr);
    (StatusCode::OK, "ok")
}
