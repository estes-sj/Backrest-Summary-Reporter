use axum::{
    extract::{ConnectInfo, Json, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use anyhow::Context;
use chrono::{DateTime, Local, Utc};
use dotenv::dotenv;
use serde::Deserialize;
use sqlx::{Executor, PgPool, Row};
use std::{env, net::SocketAddr};

// --- 1. Incoming JSON payload definitions ---
#[derive(Debug, Deserialize)]
struct SnapshotStats {
    message_type: String,
    error: Option<String>,
    during: String,
    item: String,
    files_new: i64,
    files_changed: i64,
    files_unmodified: i64,
    dirs_new: i64,
    dirs_changed: i64,
    dirs_unmodified: i64,
    data_blobs: i64,
    tree_blobs: i64,
    data_added: i64,
    total_files_processed: i64,
    total_bytes_processed: i64,
    total_duration: f64,
    snapshot_id: String,
    percent_done: i64,
    total_files: i64,
    files_done: i64,
    total_bytes: i64,
    bytes_done: i64,
    current_files: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SummaryPayload {
    task: String,
    time: String,
    event: String,
    repo: String,
    plan: String,
    snapshot: String,
    snapshot_stats: SnapshotStats,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let auth_key = env::var("AUTH_KEY").context("AUTH_KEY must be set")?;
    let pool = PgPool::connect(&database_url).await?;

    // Create tables with created_at column
    pool.execute(r#"
        CREATE TABLE IF NOT EXISTS summaries (
          id         SERIAL PRIMARY KEY,
          created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
          task       TEXT NOT NULL,
          time       TEXT NOT NULL,
          event      TEXT NOT NULL,
          repo       TEXT NOT NULL,
          plan       TEXT NOT NULL,
          snapshot   TEXT NOT NULL
        );
    "#).await?;

    pool.execute(r#"
        CREATE TABLE IF NOT EXISTS snapshot_stats (
          id                    SERIAL PRIMARY KEY,
          summary_id            INTEGER NOT NULL REFERENCES summaries(id) ON DELETE CASCADE,
          message_type          TEXT NOT NULL,
          error                 TEXT,
          during                TEXT,
          item                  TEXT,
          files_new             BIGINT,
          files_changed         BIGINT,
          files_unmodified      BIGINT,
          dirs_new              BIGINT,
          dirs_changed          BIGINT,
          dirs_unmodified       BIGINT,
          data_blobs            BIGINT,
          tree_blobs            BIGINT,
          data_added            BIGINT,
          total_files_processed BIGINT,
          total_bytes_processed BIGINT,
          total_duration        DOUBLE PRECISION,
          snapshot_id           TEXT NOT NULL,
          percent_done          BIGINT,
          total_files           BIGINT,
          files_done            BIGINT,
          total_bytes           BIGINT,
          bytes_done            BIGINT,
          current_files         BIGINT
        );
    "#).await?;

    let app = Router::new()
        .route("/summary", post(handle_summary))
        .with_state((pool, auth_key));

    let addr = SocketAddr::from(([0, 0, 0, 0], 2682));
    println!("Listening on {}", addr);

    axum::serve(
        tokio::net::TcpListener::bind(addr).await?,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}

async fn handle_summary(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((pool, auth_key)): State<(PgPool, String)>,
    headers: HeaderMap,
    Json(payload): Json<SummaryPayload>,
) -> (StatusCode, &'static str) {
    // API key validation
    let provided = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if provided != auth_key {
        return (StatusCode::UNAUTHORIZED, "unauthorized");
    }

    // Insert into summaries and return id, created_at
    let row = sqlx::query(
        "INSERT INTO summaries (task, time, event, repo, plan, snapshot)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, created_at::TEXT as created_at"
    )
    .bind(&payload.task)
    .bind(&payload.time)
    .bind(&payload.event)
    .bind(&payload.repo)
    .bind(&payload.plan)
    .bind(&payload.snapshot)
    .fetch_one(&pool)
    .await;

    let (summary_id, created_at): (i32, String) = match row {
        Ok(r) => (
            r.get::<i32, _>("id"),
            r.get::<String, _>("created_at"),
        ),
        Err(e) => {
            eprintln!("DB insert summary error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error");
        }
    };

    // Insert snapshot_stats
    let s = payload.snapshot_stats;
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
    .bind(&s.message_type)
    .bind(&s.error)
    .bind(&s.during)
    .bind(&s.item)
    .bind(s.files_new)
    .bind(s.files_changed)
    .bind(s.files_unmodified)
    .bind(s.dirs_new)
    .bind(s.dirs_changed)
    .bind(s.dirs_unmodified)
    .bind(s.data_blobs)
    .bind(s.tree_blobs)
    .bind(s.data_added)
    .bind(s.total_files_processed)
    .bind(s.total_bytes_processed)
    .bind(s.total_duration)
    .bind(&s.snapshot_id)
    .bind(s.percent_done)
    .bind(s.total_files)
    .bind(s.files_done)
    .bind(s.total_bytes)
    .bind(s.bytes_done)
    .bind(s.current_files)
    .execute(&pool)
    .await
    {
        eprintln!("DB insert stats error: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "db error");
    }

    println!(
        "{} - Inserted summary {} from {} (created_at: {})",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        summary_id,
        addr,
        created_at
    );
    (StatusCode::OK, "ok")
}
