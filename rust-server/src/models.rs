use serde::Deserialize;

/// Represents the nested snapshot_stats in the incoming JSON payload.
#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
pub struct SummaryPayload {
    pub task: String,
    pub time: String,            // ISO8601 timestamp as string
    pub event: String,
    pub repo: String,
    pub plan: String,
    pub snapshot: String,
    pub error: Option<String>,
    pub snapshot_stats: Option<SnapshotStats>,
}
