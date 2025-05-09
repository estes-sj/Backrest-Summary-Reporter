use anyhow::Result;
use sqlx::{Executor, PgPool};

/// Initialize the database connection and ensure required tables exist.
/// Returns a configured PgPool.
pub async fn init_db(database_url: &str) -> Result<PgPool> {
    // Connect to Postgres
    let pool = PgPool::connect(database_url).await?;

    // Create summaries table with created_at timestamp column
    pool.execute(r#"
        CREATE TABLE IF NOT EXISTS summaries (
          id             SERIAL PRIMARY KEY,
          created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
          task           TEXT NOT NULL,
          time           TIMESTAMPTZ NOT NULL,
          event          TEXT NOT NULL,
          repo           TEXT NOT NULL,
          plan           TEXT NOT NULL,
          snapshot       TEXT NOT NULL,
          error          TEXT
        );
    "#).await?;

    // Create snapshot_stats table
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

    // Create storage statistics table
    pool.execute(r#"
        CREATE TABLE IF NOT EXISTS storage (
          id                     SERIAL PRIMARY KEY,
          storage_location       TEXT NOT NULL,
          storage_nickname       TEXT,
          time_added             TIMESTAMPTZ NOT NULL DEFAULT now(),
          storage_used_bytes     BIGINT,
          storage_total_bytes    BIGINT
        );
    "#).await?;

    Ok(pool)
}
