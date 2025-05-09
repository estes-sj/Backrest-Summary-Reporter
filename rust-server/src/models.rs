use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Represents the nested snapshot_stats in the incoming JSON payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotStats {
    pub message_type: String,
    pub error: Option<String>,
    pub during: String,
    pub item: String,
    pub files_new: i64,
    pub files_changed: i64,
    pub files_unmodified: i64,
    pub dirs_new: i64,
    pub dirs_changed: i64,
    pub dirs_unmodified: i64,
    pub data_blobs: i64,
    pub tree_blobs: i64,
    pub data_added: i64,
    pub total_files_processed: i64,
    pub total_bytes_processed: i64,
    pub total_duration: f64,
    pub snapshot_id: String,
    pub percent_done: i64,
    pub total_files: i64,
    pub files_done: i64,
    pub total_bytes: i64,
    pub bytes_done: i64,
    pub current_files: Option<i64>,
}

/// Represents the top-level summary payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryPayload {
    pub task: String,
    pub time: DateTime<Utc>,
    pub event: String,
    pub repo: String,
    pub plan: String,
    pub snapshot: String,
    pub error: Option<String>,
    pub snapshot_stats: Option<SnapshotStats>,
}

/// New request type for stats
#[derive(Debug, Serialize, Deserialize)]
pub struct StatsRequest {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
}

/// Combined response type
#[derive(Debug, Serialize, FromRow)]
pub struct CombinedStats {
    pub summary_id: i32,
    pub created_at: DateTime<Utc>,
    pub task: String,
    pub time: DateTime<Utc>,
    pub event: String,
    pub repo: String,
    pub plan: String,
    pub snapshot: String,
    pub error: Option<String>,

    // All the optional snapshot fields
    pub message_type: Option<String>,
    pub ss_error: Option<String>,
    pub during: Option<String>,
    pub item: Option<String>,
    pub files_new: Option<i64>,
    pub files_changed: Option<i64>,
    pub files_unmodified: Option<i64>,
    pub dirs_new: Option<i64>,
    pub dirs_changed: Option<i64>,
    pub dirs_unmodified: Option<i64>,
    pub data_blobs: Option<i64>,
    pub tree_blobs: Option<i64>,
    pub data_added: Option<i64>,
    pub total_files_processed: Option<i64>,
    pub total_bytes_processed: Option<i64>,
    pub total_duration: Option<f64>,
    pub ss_snapshot: Option<String>,
    pub percent_done: Option<i64>,
    pub total_files: Option<i64>,
    pub files_done: Option<i64>,
    pub total_bytes: Option<i64>,
    pub bytes_done: Option<i64>,
    pub current_files: Option<i64>,
}

/// Storing the stats for a storage
#[derive(Serialize)]
pub struct StorageReport {
    pub location:    String,
    pub nickname:    Option<String>,
    pub used_bytes:  u64,
    pub total_bytes: u64,
}

/// The JSON shape returned for the current storage report
#[derive(Serialize)]
pub struct CurrentStorageStats {
    pub location:     String,
    pub nickname:     Option<String>,
    pub used_bytes:   i64,
    pub free_bytes:   i64,
    pub total_bytes:  i64,
    pub percent_used: f64,
    pub time_added:   DateTime<Utc>,
}

/// Structure matching exactly the columns pulled from the DB
#[derive(FromRow)]
pub struct DbStorageRow {
    pub storage_location:    String,
    pub storage_nickname:    Option<String>,
    pub storage_used_bytes:  i64,
    pub storage_total_bytes: i64,
    pub time_added:          DateTime<Utc>,
}