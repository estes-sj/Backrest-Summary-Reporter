use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Represents the nested snapshot_stats in the incoming JSON payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotStats {
    pub message_type:           String,
    pub error:                  Option<String>,
    pub during:                 String,
    pub item:                   String,
    pub files_new:              i64,
    pub files_changed:          i64,
    pub files_unmodified:       i64,
    pub dirs_new:               i64,
    pub dirs_changed:           i64,
    pub dirs_unmodified:        i64,
    pub data_blobs:             i64,
    pub tree_blobs:             i64,
    pub data_added:             i64,
    pub total_files_processed:  i64,
    pub total_bytes_processed:  i64,
    pub total_duration:         f64,
    pub snapshot_id:            String,
    pub percent_done:           i64,
    pub total_files:            i64,
    pub files_done:             i64,
    pub total_bytes:            i64,
    pub bytes_done:             i64,
    pub current_files:          Option<i64>,
}

/// Represents the top-level summary payload.
#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryPayload {
    pub task:            String,
    pub time:            DateTime<Utc>,
    pub event:           String,
    pub repo:            String,
    pub plan:            String,
    pub snapshot:        String,
    pub error:           Option<String>,
    pub snapshot_stats:  Option<SnapshotStats>,
}

/// New request type for stats
#[derive(Debug, Serialize, Deserialize)]
pub struct StatsRequest {
    pub start_date: DateTime<Utc>,
    pub end_date:   DateTime<Utc>,
}

/// Combined response type
#[derive(Debug, Serialize, FromRow)]
pub struct CombinedStats {
    pub summary_id:             i32,
    pub created_at:             DateTime<Utc>,
    pub task:                   String,
    pub time:                   DateTime<Utc>,
    pub event:                  String,
    pub repo:                   String,
    pub plan:                   String,
    pub snapshot:               String,
    pub error:                  Option<String>,

    // All the optional snapshot fields
    pub message_type:           Option<String>,
    pub ss_error:               Option<String>,
    pub during:                 Option<String>,
    pub item:                   Option<String>,
    pub files_new:              Option<i64>,
    pub files_changed:          Option<i64>,
    pub files_unmodified:       Option<i64>,
    pub dirs_new:               Option<i64>,
    pub dirs_changed:           Option<i64>,
    pub dirs_unmodified:        Option<i64>,
    pub data_blobs:             Option<i64>,
    pub tree_blobs:             Option<i64>,
    pub data_added:             Option<i64>,
    pub total_files_processed:  Option<i64>,
    pub total_bytes_processed:  Option<i64>,
    pub total_duration:         Option<f64>,
    pub ss_snapshot:            Option<String>,
    pub percent_done:           Option<i64>,
    pub total_files:            Option<i64>,
    pub files_done:             Option<i64>,
    pub total_bytes:            Option<i64>,
    pub bytes_done:             Option<i64>,
    pub current_files:          Option<i64>,
}

/// Aggregated event totals over a time range
#[derive(Serialize, FromRow)]
pub struct EventTotals {
    pub start_date:              DateTime<Utc>,
    pub end_date:                DateTime<Utc>,

    pub total_events:            i64,
    pub total_snapshot_success:  i64,
    pub total_snapshot_error:    i64,
    pub total_forget_success:    i64,
    pub total_forget_error:      i64,

    pub total_files_new:         i64,
    pub total_files_changed:     i64,
    pub total_files_unmodified:  i64,

    pub total_dirs_new:          i64,
    pub total_dirs_changed:      i64,
    pub total_dirs_unmodified:   i64,

    pub total_data_blobs:        i64,
    pub total_tree_blobs:        i64,
    pub total_data_added:        i64,
    pub total_files_processed:   i64,
    pub total_bytes_processed:   i64,
    pub total_duration:          i64
}

// Event Totals for the polled date, the prior day, prior week, and prior month
#[derive(Serialize)]
pub struct EventTotalsReport {
    pub current:        EventTotals,
    pub previous_day:   Option<EventTotals>,
    pub previous_week:  Option<EventTotals>,
    pub previous_month: Option<EventTotals>,
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
// Period statistics grouping
#[derive(Serialize)]
pub struct PeriodStats {
    pub used_bytes:   i64,
    pub free_bytes:   i64,
    pub total_bytes : i64,
    pub percent_used: f64,
    pub time_added:   DateTime<Utc>,
}

// Main response shape with nested periods
#[derive(Serialize)]
pub struct CurrentStorageStats {
    pub location:       String,
    pub nickname:       Option<String>,
    pub current:        PeriodStats,
    pub previous_day:   Option<PeriodStats>,
    pub previous_week:  Option<PeriodStats>,
    pub previous_month: Option<PeriodStats>,
}

/// Combined report of event totals, summary statistics, and current storage stats
#[derive(Serialize)]
pub struct GenerateReport {
    pub event_totals:       EventTotalsReport,
    pub snapshot_summaries: Vec<CombinedStats>,
    pub storage_statistics: Vec<CurrentStorageStats>,
}

/// Structure matching exactly the columns pulled from the DB
#[derive(Clone, FromRow)]
pub struct DbStorageRow {
    pub storage_location:    String,
    pub storage_nickname:    Option<String>,
    pub storage_used_bytes:  i64,
    pub storage_total_bytes: i64,
    pub time_added:          DateTime<Utc>,
}