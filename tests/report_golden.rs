#![cfg(unix)]

use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;
use usedu::output::{render_report, ReportOptions};
use usedu::scanner::{DirSummary, EntryCounts, ScanErrorRecord, ScanMetrics, ScanResult, SortKey};

#[test]
fn current_summary_report_matches_golden_baseline() {
    let report = render_report(
        &report_fixture(),
        &ReportOptions {
            depth: 0,
            top: 0,
            include_files: false,
            summarize: true,
            dirs_only: false,
            sort_key: SortKey::Used,
            show_errors: false,
        },
    );
    let expected = include_str!("../fixtures/report/current-summary.golden.txt");

    assert_eq!(report, expected);
}

fn report_fixture() -> ScanResult {
    ScanResult {
        root: DirSummary {
            path: PathBuf::from("/fixture"),
            name: OsString::from("fixture"),
            used_bytes: 32_768,
            own_bytes: 4_096,
            file_count: 7,
            dir_count: 2,
            counts: EntryCounts {
                regular_files: 5,
                directories: 2,
                symlinks: 1,
                other: 1,
            },
            errors: vec![
                ScanErrorRecord {
                    path: PathBuf::from("/fixture/locked"),
                    kind: "PermissionDenied".to_string(),
                    message: "Permission denied (os error 13)".to_string(),
                },
                ScanErrorRecord {
                    path: PathBuf::from("/fixture/race"),
                    kind: "NotFound".to_string(),
                    message: "No such file or directory (os error 2)".to_string(),
                },
            ],
            children: Vec::new(),
        },
        metrics: ScanMetrics {
            elapsed: Duration::from_millis(1_200),
            entries_seen: 9,
            files_seen: 7,
            dirs_seen: 2,
            errors_seen: 2,
        },
        top_files: Vec::new(),
    }
}
