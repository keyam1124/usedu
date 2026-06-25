use crate::scanner::{EntrySummary, ScanErrorRecord, ScanResult};
use crate::util::path::display_path;
use anyhow::Result;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonReport {
    path: String,
    used_bytes: u64,
    file_count: u64,
    dir_count: u64,
    errors_count: u64,
    metrics: JsonMetrics,
    children: Vec<JsonEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<JsonError>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonMetrics {
    elapsed_ms: u128,
    entries_per_second: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonEntry {
    kind: String,
    name: String,
    path: String,
    used_bytes: u64,
    file_count: u64,
    dir_count: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonError {
    path: String,
    kind: String,
    message: String,
}

pub fn render_json(scan: &ScanResult, show_errors: bool) -> Result<String> {
    let report = JsonReport {
        path: display_path(&scan.root.path),
        used_bytes: scan.root.used_bytes,
        file_count: scan.root.file_count,
        dir_count: scan.root.dir_count,
        errors_count: scan.metrics.errors_seen,
        metrics: JsonMetrics {
            elapsed_ms: scan.metrics.elapsed.as_millis(),
            entries_per_second: scan.metrics.entries_per_second(),
        },
        children: scan.root.children.iter().map(json_entry).collect(),
        errors: show_errors.then(|| scan.root.errors.iter().map(json_error).collect()),
    };

    Ok(serde_json::to_string_pretty(&report)?)
}

fn json_entry(entry: &EntrySummary) -> JsonEntry {
    JsonEntry {
        kind: entry.kind_label().to_string(),
        name: entry.name().to_string_lossy().into_owned(),
        path: display_path(entry.path()),
        used_bytes: entry.used_bytes(),
        file_count: entry.file_count(),
        dir_count: entry.dir_count(),
    }
}

fn json_error(error: &ScanErrorRecord) -> JsonError {
    JsonError {
        path: display_path(&error.path),
        kind: error.kind.clone(),
        message: error.message.clone(),
    }
}
