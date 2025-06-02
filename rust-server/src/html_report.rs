use axum::{
    http::StatusCode,
};
use chrono::{DateTime, Local, Offset, TimeZone};
use std::{fs, path::Path};

use crate::{
    config::Config,
    models::{CurrentStorageStats, EventTotals, GenerateReport},
};

/// Renders the full report email, replacing placeholders in the template.
pub fn render_report_html(cfg: &Config, report: &GenerateReport) -> Result<String, &'static str> {
    
    // 1) Load all the snapshot templates once:
    let mut html      = fs::read_to_string("html/report_variabled.html")
        .map_err(|_| "Failed to read HTML template")?;
    let success_tmpl = fs::read_to_string("html/success_snapshot.html")
        .map_err(|_| "Failed to read success snapshot template")?;
    let warn_tmpl = fs::read_to_string("html/warn_snapshot.html")
        .map_err(|_| "Failed to read warn snapshot template")?;
    let error_tmpl   = fs::read_to_string("html/error_snapshot.html")
        .map_err(|_| "Failed to read error snapshot template")?;
    let row_tmpl     = fs::read_to_string("html/snapshot_status_row.html")
        .map_err(|_| "Failed to read snapshot status row template")?;
    let table_tmpl   = fs::read_to_string("html/snapshot_status_table.html")
        .map_err(|_| "Failed to read snapshot status table template")?;
    let storage_entry_tmpl    = fs::read_to_string("html/storage_entry.html")
        .map_err(|_| "Failed to read storage entry template")?;

    // 2) Render each summary into its per‐entry HTML
    let mut entries: Vec<String> = Vec::new();
    for summary in &report.snapshot_summaries {
        let tpl = match summary.event.as_str() {
            "snapshot success" => &success_tmpl,
            "snapshot warning" => &warn_tmpl,
            "snapshot error"   => &error_tmpl,
            _                  => continue, // skip other events
        };

        // assume success_tmpl and error_tmpl have placeholders like {{ID}}, {{TIME}}, {{MESSAGE}}
        let mut entry = tpl.clone();
        entry = entry.replace("{{SNAPSHOT_PLAN}}", &summary.plan.to_string());
        entry = entry.replace("{{SNAPSHOT_SUMMARY_ID}}", &summary.snapshot.to_string()[..10]);
        entry = entry.replace("{{SNAPSHOT_TIME}}", &format_local_datetime(summary.created_at));
        entry = entry.replace(
            "{{SNAPSHOT_NEW_FILES}}",
            &summary.files_new
                .map_or("-".to_string(), |v| v.to_string()),
        );
        entry = entry.replace(
            "{{SNAPSHOT_CHANGED_FILES}}",
            &summary.files_changed
                .map_or("-".to_string(), |v| v.to_string()),
        );
        entry = entry.replace(
            "{{SNAPSHOT_UNMODIFIED_FILES}}",
            &summary.files_unmodified
                .map_or("-".to_string(), |v| v.to_string()),
        );
        entry = entry.replace(
            "{{SNAPSHOT_TOTAL_PROCESSED}}",
            &summary.total_bytes_processed
                .map_or("-".to_string(), |v| format_bytes(v as u64)),
        );
        entry = entry.replace("{{SNAPSHOT_TOTAL_DURATION}}", &format_duration_secs(summary.total_duration.unwrap_or(0.0) as i64));
        entry = entry.replace("{{STORAGE_ERROR}}", summary.error.as_deref().unwrap_or("N/A"));

        entries.push(entry);
    }

    // 3) Group into rows of two entries each
    let mut rows_html = String::new();
    for chunk in entries.chunks(2) {
        // join 1 or 2 entries back-to-back
        let joined = chunk.join("");
        // inject into row template
        let row_html = row_tmpl.replace("{{SNAPSHOT}}", &joined);
        rows_html.push_str(&row_html);
    }

    // 4) Build the full table by splicing in all the rows
    let snapshot_table_html = table_tmpl.replace("{{SNAPSHOT_ROW}}", &rows_html);

    // Build storage rows
    let storage_html = report.storage_statistics
        .iter()
        .map(|stat| render_storage_entry(&storage_entry_tmpl, stat))
        .collect::<String>();

    // Prepare replacements
    let mut replacements = Vec::new();
    let now = Local::now();

    // Config fields
    replacements.push(("{{SERVER_NAME}}", cfg.server_name.clone().unwrap_or_default()));
    replacements.push(("{{BACKREST_URL}}", cfg.backrest_url.clone().unwrap_or_default()));
    replacements.push(("{{PGADMIN_URL}}", cfg.pgadmin_url.clone().unwrap_or_default()));
    replacements.push((
        "{{START_DATE}}",
        format_pretty_datetime(report.event_totals.current.start_date),
    ));
    replacements.push((
        "{{END_DATE}}",
        format_pretty_datetime(report.event_totals.current.end_date),
    ));
    replacements.push((
        "{{DATE_RANGE_TIMEZONE}}",
        format_local_offset(report.event_totals.current.start_date),
    ));
    replacements.push(("{{REPORT_GENERATION_DATE}}", format_local_datetime(now)));

    // Event totals (current)
    let et = &report.event_totals.current;
    replacements.push(("{{TOTAL_EVENTS}}",           et.total_events.to_string()));
    replacements.push(("{{TOTAL_SNAPSHOT_SUCCESS}}", et.total_snapshot_success.to_string()));
    replacements.push(("{{TOTAL_SNAPSHOT_ERROR}}",   et.total_snapshot_error.to_string()));
    replacements.push(("{{TOTAL_FORGET_SUCCESS}}",   et.total_forget_success.to_string()));
    replacements.push(("{{TOTAL_FORGET_ERROR}}",     et.total_forget_error.to_string()));

    // Data processed
    let cur_bytes = et.total_bytes_processed as u64;
    let prev_day_bytes  = report.event_totals.previous_day.as_ref().map(|e| e.total_bytes_processed as u64);
    let prev_week_bytes = report.event_totals.previous_week.as_ref().map(|e| e.total_bytes_processed as u64);
    let prev_month_bytes= report.event_totals.previous_month.as_ref().map(|e| e.total_bytes_processed as u64);

    replacements.push(("{{TOTAL_DATA_PROCESSED}}", format_bytes(cur_bytes)));
    replacements.push(("{{TOTAL_DATA_PROCESSED_PREVIOUS_DAY}}",  prev_day_bytes.map_or_else(|| "–".to_string(), |b| format_bytes(b))));
    replacements.push(("{{TOTAL_DATA_PROCESSED_PREVIOUS_WEEK}}", prev_week_bytes.map_or_else(|| "–".to_string(), |b| format_bytes(b))));
    replacements.push(("{{TOTAL_DATA_PROCESSED_PREVIOUS_MONTH}}", prev_month_bytes.map_or_else(|| "–".to_string(), |b| format_bytes(b))));

    replacements.push(("{{PERCENT_TOTAL_DATA_PROCESSED_PREVIOUS_DAY}}",  fmt_bytes_change_pct(cur_bytes, prev_day_bytes)));
    replacements.push(("{{PERCENT_TOTAL_DATA_PROCESSED_PREVIOUS_WEEK}}", fmt_bytes_change_pct(cur_bytes, prev_week_bytes)));
    replacements.push(("{{PERCENT_TOTAL_DATA_PROCESSED_PREVIOUS_MONTH}}", fmt_bytes_change_pct(cur_bytes, prev_month_bytes)));

    // Duration
    let cur_dur = et.total_duration;
    let prev_day_dur   = report.event_totals.previous_day.as_ref().map(|e| e.total_duration);
    let prev_week_dur  = report.event_totals.previous_week.as_ref().map(|e| e.total_duration);
    let prev_month_dur = report.event_totals.previous_month.as_ref().map(|e| e.total_duration);

    replacements.push(("{{TOTAL_DURATION}}", format_duration_secs(cur_dur)));
    replacements.push(("{{TOTAL_DURATION_PREVIOUS_DAY}}",   prev_day_dur.map_or_else(|| "–".to_string(), format_duration_secs)));
    replacements.push(("{{TOTAL_DURATION_PREVIOUS_WEEK}}",  prev_week_dur.map_or_else(|| "–".to_string(), format_duration_secs)));
    replacements.push(("{{TOTAL_DURATION_PREVIOUS_MONTH}}", prev_month_dur.map_or_else(|| "–".to_string(), format_duration_secs)));

    replacements.push(("{{PERCENT_TOTAL_DURATION_PREVIOUS_DAY}}",   fmt_duration_change_pct(cur_dur, prev_day_dur)));
    replacements.push(("{{PERCENT_TOTAL_DURATION_PREVIOUS_WEEK}}",  fmt_duration_change_pct(cur_dur, prev_week_dur)));
    replacements.push(("{{PERCENT_TOTAL_DURATION_PREVIOUS_MONTH}}", fmt_duration_change_pct(cur_dur, prev_month_dur)));

    // Files new
    let cur_new = et.total_files_new;
    replacements.push(("{{TOTAL_FILES_NEW}}", get_formatted_files_new(&report.event_totals.current)));
    replacements.push(("{{TOTAL_FILES_NEW_PREVIOUS_DAY}}",  get_formatted_files_new(&report.event_totals.previous_day)));
    replacements.push(("{{PERCENT_TOTAL_FILES_NEW_PREVIOUS_DAY}}",  get_files_new_change_pct(cur_new, &report.event_totals.previous_day)));
    replacements.push(("{{TOTAL_FILES_NEW_PREVIOUS_WEEK}}", get_formatted_files_new(&report.event_totals.previous_week)));
    replacements.push(("{{PERCENT_TOTAL_FILES_NEW_PREVIOUS_WEEK}}", get_files_new_change_pct(cur_new, &report.event_totals.previous_week)));
    replacements.push(("{{TOTAL_FILES_NEW_PREVIOUS_MONTH}}", get_formatted_files_new(&report.event_totals.previous_month)));
    replacements.push(("{{PERCENT_TOTAL_FILES_NEW_PREVIOUS_MONTH}}", get_files_new_change_pct(cur_new, &report.event_totals.previous_month)));

    // Files changed
    let cur_changed = et.total_files_changed;
    replacements.push(("{{TOTAL_FILES_CHANGED}}", get_formatted_files_changed(&report.event_totals.current)));
    replacements.push(("{{TOTAL_FILES_CHANGED_PREVIOUS_DAY}}", get_formatted_files_changed(&report.event_totals.previous_day)));
    replacements.push(("{{PERCENT_TOTAL_FILES_CHANGED_PREVIOUS_DAY}}", get_files_changed_change_pct(cur_changed, &report.event_totals.previous_day)));
    replacements.push(("{{TOTAL_FILES_CHANGED_PREVIOUS_WEEK}}", get_formatted_files_changed(&report.event_totals.previous_week)));
    replacements.push(("{{PERCENT_TOTAL_FILES_CHANGED_PREVIOUS_WEEK}}", get_files_changed_change_pct(cur_changed, &report.event_totals.previous_week)));
    replacements.push(("{{TOTAL_FILES_CHANGED_PREVIOUS_MONTH}}", get_formatted_files_changed(&report.event_totals.previous_month)));
    replacements.push(("{{PERCENT_TOTAL_FILES_CHANGED_PREVIOUS_MONTH}}", get_files_changed_change_pct(cur_changed, &report.event_totals.previous_month)));

    // Files unmodified
    let cur_unmod = et.total_files_unmodified;
    replacements.push(("{{TOTAL_FILES_UNMODIFIED}}", get_formatted_files_unmodified(&report.event_totals.current)));
    replacements.push(("{{TOTAL_FILES_UNMODIFIED_PREVIOUS_DAY}}", get_formatted_files_unmodified(&report.event_totals.previous_day)));
    replacements.push(("{{PERCENT_TOTAL_FILES_UNMODIFIED_PREVIOUS_DAY}}", get_files_unmodified_change_pct(cur_unmod, &report.event_totals.previous_day)));
    replacements.push(("{{TOTAL_FILES_UNMODIFIED_PREVIOUS_WEEK}}", get_formatted_files_unmodified(&report.event_totals.previous_week)));
    replacements.push(("{{PERCENT_TOTAL_FILES_UNMODIFIED_PREVIOUS_WEEK}}", get_files_unmodified_change_pct(cur_unmod, &report.event_totals.previous_week)));
    replacements.push(("{{TOTAL_FILES_UNMODIFIED_PREVIOUS_MONTH}}", get_formatted_files_unmodified(&report.event_totals.previous_month)));
    replacements.push(("{{PERCENT_TOTAL_FILES_UNMODIFIED_PREVIOUS_MONTH}}", get_files_unmodified_change_pct(cur_unmod, &report.event_totals.previous_month)));

    // Dirs new
    let cur_new = et.total_dirs_new;
    replacements.push(("{{TOTAL_DIRS_NEW}}", get_formatted_dirs_new(&report.event_totals.current)));
    replacements.push(("{{TOTAL_DIRS_NEW_PREVIOUS_DAY}}",  get_formatted_dirs_new(&report.event_totals.previous_day)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_NEW_PREVIOUS_DAY}}",  get_dirs_new_change_pct(cur_new, &report.event_totals.previous_day)));
    replacements.push(("{{TOTAL_DIRS_NEW_PREVIOUS_WEEK}}", get_formatted_dirs_new(&report.event_totals.previous_week)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_NEW_PREVIOUS_WEEK}}", get_dirs_new_change_pct(cur_new, &report.event_totals.previous_week)));
    replacements.push(("{{TOTAL_DIRS_NEW_PREVIOUS_MONTH}}", get_formatted_dirs_new(&report.event_totals.previous_month)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_NEW_PREVIOUS_MONTH}}", get_dirs_new_change_pct(cur_new, &report.event_totals.previous_month)));

    // Dirs changed
    let cur_changed = et.total_dirs_changed;
    replacements.push(("{{TOTAL_DIRS_CHANGED}}", get_formatted_dirs_changed(&report.event_totals.current)));
    replacements.push(("{{TOTAL_DIRS_CHANGED_PREVIOUS_DAY}}", get_formatted_dirs_changed(&report.event_totals.previous_day)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_CHANGED_PREVIOUS_DAY}}", get_dirs_changed_change_pct(cur_changed, &report.event_totals.previous_day)));
    replacements.push(("{{TOTAL_DIRS_CHANGED_PREVIOUS_WEEK}}", get_formatted_dirs_changed(&report.event_totals.previous_week)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_CHANGED_PREVIOUS_WEEK}}", get_dirs_changed_change_pct(cur_changed, &report.event_totals.previous_week)));
    replacements.push(("{{TOTAL_DIRS_CHANGED_PREVIOUS_MONTH}}", get_formatted_dirs_changed(&report.event_totals.previous_month)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_CHANGED_PREVIOUS_MONTH}}", get_dirs_changed_change_pct(cur_changed, &report.event_totals.previous_month)));

    // Dirs unmodified
    let cur_unmod = et.total_dirs_unmodified;
    replacements.push(("{{TOTAL_DIRS_UNMODIFIED}}", get_formatted_dirs_unmodified(&report.event_totals.current)));
    replacements.push(("{{TOTAL_DIRS_UNMODIFIED_PREVIOUS_DAY}}", get_formatted_dirs_unmodified(&report.event_totals.previous_day)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_UNMODIFIED_PREVIOUS_DAY}}", get_dirs_unmodified_change_pct(cur_unmod, &report.event_totals.previous_day)));
    replacements.push(("{{TOTAL_DIRS_UNMODIFIED_PREVIOUS_WEEK}}", get_formatted_dirs_unmodified(&report.event_totals.previous_week)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_UNMODIFIED_PREVIOUS_WEEK}}", get_dirs_unmodified_change_pct(cur_unmod, &report.event_totals.previous_week)));
    replacements.push(("{{TOTAL_DIRS_UNMODIFIED_PREVIOUS_MONTH}}", get_formatted_dirs_unmodified(&report.event_totals.previous_month)));
    replacements.push(("{{PERCENT_TOTAL_DIRS_UNMODIFIED_PREVIOUS_MONTH}}", get_dirs_unmodified_change_pct(cur_unmod, &report.event_totals.previous_month)));

    // Insert snapshot summaries
    replacements.push(("{{SNAPSHOT_TABLE}}", snapshot_table_html));

    // Insert storage rows HTML
    replacements.push(("{{STORAGE_STATISTICS}}", storage_html));

    // Apply all replacements
    for (ph, val) in replacements {
        html = html.replace(ph, &val);
    }

    Ok(html)
}

/// Writes the rendered HTML to the specified file path.
///
/// * Ensures the parent directory exists, creating it if necessary.
/// * Writes the given HTML content to the file system.
///
/// # Arguments
/// * `path` - File path where the HTML should be saved
/// * `html` - The rendered HTML content
///
/// # Errors
/// Returns an error tuple if directory creation or file write fails.
///
fn render_storage_entry(template: &str, stat: &CurrentStorageStats) -> String {
    let mut entry = template.to_string();
    let nickname = stat.nickname.as_deref().filter(|s| !s.is_empty()).unwrap_or(&stat.location);
    let pairs = vec![
        ("{{STORAGE_NICKNAME}}",   nickname.to_string()),
        ("{{TIME_ADDED}}",         format_local_datetime(stat.current.time_added)),
        ("{{PERCENT_USED_COLOR}}", percent_used_color(stat.current.percent_used).to_string()),
        ("{{PERCENT_USED_CURRENT}}", format!("{:.2}", stat.current.percent_used)),
        ("{{PERCENT_FREE_CURRENT}}", format!("{:.2}", 100.0 - stat.current.percent_used)),
        ("{{USED_SPACE_CURRENT}}",   format_bytes(stat.current.used_bytes as u64)),
        ("{{TOTAL_SPACE_CURRENT}}",  format_bytes(stat.current.total_bytes as u64)),
        // percent changes… arrow and value
        ("{{STORAGE_USED_PREVIOUS_DAY_PERCENT_INCREASE}}",   fmt_percent_change(stat.current.percent_used, stat.previous_day.as_ref().map(|p| p.percent_used))),
        ("{{STORAGE_USED_PREVIOUS_WEEK_PERCENT_INCREASE}}",  fmt_percent_change(stat.current.percent_used, stat.previous_week.as_ref().map(|p| p.percent_used))),
        ("{{STORAGE_USED_PREVIOUS_MONTH_PERCENT_INCREASE}}", fmt_percent_change(stat.current.percent_used, stat.previous_month.as_ref().map(|p| p.percent_used))),
    ];
    for (ph, val) in pairs {
        entry = entry.replace(ph, &val);
    }
    entry
}

/// Writes the rendered HTML to disk.
pub fn write_report_html(path: &str, html: &str) -> Result<(), (StatusCode, &'static str)> {
    if let Some(dir) = Path::new(path).parent() {
        fs::create_dir_all(dir).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create dirs"))?;
    }
    fs::write(path, html).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write report"))
}

/// Deletes oldest files in `dir_path` until only `max_files` remain.
/// Expects HTML files (or whatever) in that directory.
pub fn prune_old_reports(dir_path: &str, max_files: usize) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir_path)?
        .filter_map(|res| res.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|s| s.eq_ignore_ascii_case("html"))
                .unwrap_or(false)
        })
        .collect();

    // Sort by file modified time, oldest first
    entries.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    // Delete the oldest ones if over limit
    while entries.len() > max_files {
        if let Some(entry) = entries.first() {
            let _ = std::fs::remove_file(entry.path());
            entries.remove(0);
        }
    }
    Ok(())
}

///
/// HELPER AND FORMATTING METHODS
/// 

/// Returns a HEX color based on the used‐percentage thresholds:
/// - 90–100 → greenish  (`#b02020`)
/// - 80–90  → yellowish (`#e0c020`)
/// - 0–80   → redish    (`#80c080`)
fn percent_used_color(pct: f64) -> &'static str {
    match pct {
        pct if pct >= 90.0 => "#b02020",
        pct if pct >= 80.0 => "#e0c020",
        _                  => "#80c080",
    }
}

/// Converts a byte count into a human‑readable string with 1 decimal place:
/// - ≥ 1 TB → “1.8 TB”
/// - ≥ 1 GB → “400.5 GB”
/// - ≥ 1 MB → “20.0 MB”
/// - ≥ 1 KB → “512.0 KB”
/// - else   → “123 B”
fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.1} TB", b / TB)
    } else if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.1} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Formats any DateTime into a local time string as:
/// "MM/DD/YYYY at hh:mm:ss AM/PM ZZZ"
fn format_local_datetime<Tz: TimeZone>(dt: DateTime<Tz>) -> String {
    dt.with_timezone(&Local)
      .format("%m/%d/%Y at %I:%M:%S %p %Z")
      .to_string()
}

/// Formats any DateTime into a string like:
/// "May 2, 2025, 11:13 AM"
fn format_pretty_datetime<Tz: TimeZone>(dt: DateTime<Tz>) -> String {
    dt.with_timezone(&Local)
      .format("%B %-d, %Y, %-I:%M %p")
      .to_string()
}

/// Returns the local UTC offset as a string like:
/// "(UTC -4)"    // if offset is exactly -4 hours
/// "(UTC +5:30)" // if offset has minutes
fn format_local_offset<Tz: TimeZone>(dt: DateTime<Tz>) -> String {
    // convert to Local so we get the correct offset for the current locale
    let local_dt = dt.with_timezone(&Local);
    // get a FixedOffset, then total seconds east of UTC
    let secs = local_dt.offset().fix().local_minus_utc();
    let hours = secs / 3600;
    let mins = (secs.abs() % 3600) / 60;

    if secs % 3600 == 0 {
        // whole hours only
        format!("(UTC {:+})", hours)
    } else {
        // include minutes
        format!("(UTC {:+}:{:02})", hours, mins)
    }
}

/// Formats a start and end time like:
/// `2025-05-01 13:13 to 2025-05-16 13:13 -04:00`
pub fn format_range_iso_with_offset<Tz: TimeZone>(
    start: DateTime<Tz>,
    end: DateTime<Tz>,
) -> String {
    let start_local = start.with_timezone(&Local);
    let end_local = end.with_timezone(&Local);
    let offset = end_local.offset(); // both have same offset in same timezone

    format!(
        "{} to {} {}",
        start_local.format("%Y-%m-%d %H:%M"),
        end_local.format("%Y-%m-%d %H:%M"),
        offset
    )
}

/// Formats a percentage change between a current and optional previous value:
/// - Returns "↑x.xx%" if increased
/// - Returns "↓x.xx%" if decreased
/// - Returns "–" if no previous value is available
fn fmt_percent_change(current: f64, previous_opt: Option<f64>) -> String {
    if let Some(prev) = previous_opt {
        let diff = current - prev;
        let arrow = if diff >= 0.0 { "↑" } else { "↓" };
        format!("{}{:.2}%", arrow, diff.abs())
    } else {
        "-".to_string()
    }
}

/// Formats a percentage change between current and optional previous byte counts:
/// - Returns "↑x.xx%" or "↓x.xx%" if previous exists and is non-zero
/// - Returns "–" if no previous value or if previous is 0
fn fmt_bytes_change_pct(current: u64, previous_opt: Option<u64>) -> String {
    if let Some(prev) = previous_opt {
        if prev == 0 {
            return "–".into();
        }
        let diff = current as f64 - prev as f64;
        let pct  = diff / prev as f64 * 100.0;
        let arrow = if pct >= 0.0 { "↑" } else { "↓" };
        format!("{}{:.2}%", arrow, pct.abs())
    } else {
        "-".into()
    }
}

/// Formats a percentage change between current and optional previous durations (seconds):
/// - Returns "↑x.xx%" or "↓x.xx%" if previous exists and > 0
/// - Returns "–" if no previous value or previous ≤ 0 prior is zero
fn fmt_duration_change_pct(current: i64, previous_opt: Option<i64>) -> String {
    if let Some(prev) = previous_opt {
        if prev <= 0 {
            return "–".into();
        }
        let diff = current as f64 - prev as f64;
        let pct = diff / prev as f64 * 100.0;
        let arrow = if pct >= 0.0 { "↑" } else { "↓" };
        format!("{}{:.2}%", arrow, pct.abs())
    } else {
        "–".into()
    }
}

/// Converts a duration in seconds to hh:mm:ss format.
/// Returns "00:00:00" if input is zero or negative.
fn format_duration_secs(secs: i64) -> String {
    let total = if secs > 0 { secs } else { 0 };
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

/// A trait for "something I can borrow as an optional `&EventTotals`."
pub trait AsEventOpt {
    fn as_event_opt(&self) -> Option<&EventTotals>;
}

impl AsEventOpt for Option<EventTotals> {
    fn as_event_opt(&self) -> Option<&EventTotals> {
        self.as_ref()
    }
}

impl AsEventOpt for EventTotals {
    fn as_event_opt(&self) -> Option<&EventTotals> {
        Some(self)
    }
}

/// Formats a total count value extracted from EventTotals (or an Option).
/// Returns the value as a string, or "–" if not available.
/// Uses scientific notation if the number exceeds 99,999.
fn get_formatted_total<T, F>(opt: &T, extractor: F) -> String
where
    T: AsEventOpt,
    F: Fn(&EventTotals) -> i64,
{
    opt.as_event_opt()
        .map(|e| {
            let val = extractor(e);
            if val.abs() > 99_999 {
                format!("{:.2e}", val as f64)
            } else {
                val.to_string()
            }
        })
        .unwrap_or_else(|| "–".to_string())
}

/// Computes the percent change between a current value and a previous value.
/// `opt` can be an Option<EventTotals> or an EventTotals directly.
/// Returns the percentage as a string, or "–" if not available or invalid.
fn get_change_pct<T, F>(cur: i64, opt: &T, extractor: F) -> String
where
    T: AsEventOpt,
    F: Fn(&EventTotals) -> i64,
{
    if cur < 0 {
        return "–".into();
    }

    match opt.as_event_opt().map(|e| extractor(e)) {
        Some(prev) if prev >= 0 => fmt_bytes_change_pct(cur as u64, Some(prev as u64)),
        _ => "–".into(),
    }
}

///
/// FILE COUNTERS
/// 

/// Returns the total number of new files as a string, or "–" if not available.
fn get_formatted_files_new<T: AsEventOpt>(opt: &T) -> String {
    get_formatted_total(opt, |e| e.total_files_new)
}

/// Returns the % change in new files compared to previous, or "–" if not available.
fn get_files_new_change_pct<T: AsEventOpt>(cur: i64, opt: &T) -> String {
    get_change_pct(cur, opt, |e| e.total_files_new)
}

/// Returns the total number of changed files as a string, or "–" if not available.
fn get_formatted_files_changed<T: AsEventOpt>(opt: &T) -> String {
    get_formatted_total(opt, |e| e.total_files_changed)
}

/// Returns the % change in changed files compared to previous, or "–" if not available.
fn get_files_changed_change_pct<T: AsEventOpt>(cur: i64, opt: &T) -> String {
    get_change_pct(cur, opt, |e| e.total_files_changed)
}

/// Returns the total number of unmodified files as a string, or "–" if not available.
fn get_formatted_files_unmodified<T: AsEventOpt>(opt: &T) -> String {
    get_formatted_total(opt, |e| e.total_files_unmodified)
}

/// Returns the % change in unmodified files compared to previous, or "–" if not available.
fn get_files_unmodified_change_pct<T: AsEventOpt>(cur: i64, opt: &T) -> String {
    get_change_pct(cur, opt, |e| e.total_files_unmodified)
}

///
/// DIRECTORY COUNTERS
/// 

/// Returns the total number of new dirs as a string, or "–" if not available.
fn get_formatted_dirs_new<T: AsEventOpt>(opt: &T) -> String {
    get_formatted_total(opt, |e| e.total_dirs_new)
}

/// Returns the % change in new dirs compared to previous, or "–" if not available.
fn get_dirs_new_change_pct<T: AsEventOpt>(cur: i64, opt: &T) -> String {
    get_change_pct(cur, opt, |e| e.total_dirs_new)
}

/// Returns the total number of changed dirs as a string, or "–" if not available.
fn get_formatted_dirs_changed<T: AsEventOpt>(opt: &T) -> String {
    get_formatted_total(opt, |e| e.total_dirs_changed)
}

/// Returns the % change in changed dirs compared to previous, or "–" if not available.
fn get_dirs_changed_change_pct<T: AsEventOpt>(cur: i64, opt: &T) -> String {
    get_change_pct(cur, opt, |e| e.total_dirs_changed)
}

/// Returns the total number of unmodified dirs as a string, or "–" if not available.
fn get_formatted_dirs_unmodified<T: AsEventOpt>(opt: &T) -> String {
    get_formatted_total(opt, |e| e.total_dirs_unmodified)
}

/// Returns the % change in unmodified dirs compared to previous, or "–" if not available.
fn get_dirs_unmodified_change_pct<T: AsEventOpt>(cur: i64, opt: &T) -> String {
    get_change_pct(cur, opt, |e| e.total_dirs_unmodified)
}