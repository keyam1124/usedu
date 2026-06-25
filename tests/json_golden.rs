#![cfg(unix)]

use serde_json::Value;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;
use usedu::output::json::render_json;
use usedu::scanner::{
    DirSummary, EntryCounts, EntrySummary, FileSummary, ScanErrorRecord, ScanMetrics, ScanResult,
};

#[test]
fn current_json_report_with_errors_matches_golden_baseline() {
    let actual = render_json(&current_json_fixture(), true).unwrap();
    let expected = include_str!("../fixtures/json/current-report-with-errors.golden.json");

    assert_json_matches(&actual, expected);
}

#[test]
fn current_json_report_without_errors_matches_golden_baseline() {
    let actual = render_json(&current_json_fixture(), false).unwrap();
    let expected = include_str!("../fixtures/json/current-report-without-errors.golden.json");

    assert_json_matches(&actual, expected);
}

#[test]
fn current_json_uses_lossy_display_strings_for_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt;

    let name = OsString::from_vec(b"bad-\xff-name".to_vec());
    let child = EntrySummary::File(FileSummary {
        path: PathBuf::from("/fixture").join(&name),
        name,
        used_bytes: 1,
    });
    let scan = ScanResult {
        root: DirSummary {
            path: PathBuf::from("/fixture"),
            name: OsString::from("fixture"),
            used_bytes: 1,
            own_bytes: 0,
            file_count: 1,
            dir_count: 1,
            counts: EntryCounts {
                regular_files: 1,
                directories: 1,
                symlinks: 0,
                other: 0,
            },
            errors: Vec::new(),
            children: vec![child],
        },
        metrics: ScanMetrics {
            elapsed: Duration::from_millis(1),
            entries_seen: 2,
            files_seen: 1,
            dirs_seen: 1,
            errors_seen: 0,
        },
        top_files: Vec::new(),
    };

    let rendered: Value = serde_json::from_str(&render_json(&scan, false).unwrap()).unwrap();

    assert_eq!(rendered["children"][0]["name"], "bad-\u{FFFD}-name");
    assert_eq!(
        rendered["children"][0]["path"],
        "/fixture/bad-\u{FFFD}-name"
    );
}

fn current_json_fixture() -> ScanResult {
    let root = PathBuf::from("/fixture");
    ScanResult {
        root: DirSummary {
            path: root.clone(),
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
                    path: root.join("locked"),
                    kind: "PermissionDenied".to_string(),
                    message: "Permission denied (os error 13)".to_string(),
                },
                ScanErrorRecord {
                    path: root.join("race"),
                    kind: "NotFound".to_string(),
                    message: "No such file or directory (os error 2)".to_string(),
                },
            ],
            children: vec![
                EntrySummary::Dir(DirSummary {
                    path: root.join("alpha"),
                    name: OsString::from("alpha"),
                    used_bytes: 16_384,
                    own_bytes: 4_096,
                    file_count: 3,
                    dir_count: 1,
                    counts: EntryCounts {
                        regular_files: 3,
                        directories: 1,
                        symlinks: 0,
                        other: 0,
                    },
                    errors: Vec::new(),
                    children: Vec::new(),
                }),
                EntrySummary::File(FileSummary {
                    path: root.join(".hidden"),
                    name: OsString::from(".hidden"),
                    used_bytes: 4_096,
                }),
                EntrySummary::Symlink(FileSummary {
                    path: root.join("link\nname"),
                    name: OsString::from("link\nname"),
                    used_bytes: 512,
                }),
                EntrySummary::Other(FileSummary {
                    path: root.join("control\tname"),
                    name: OsString::from("control\tname"),
                    used_bytes: 256,
                }),
                EntrySummary::File(FileSummary {
                    path: root.join("hard-link-alias"),
                    name: OsString::from("hard-link-alias"),
                    used_bytes: 4_096,
                }),
            ],
        },
        metrics: ScanMetrics {
            elapsed: Duration::from_millis(1_200),
            entries_seen: 9,
            files_seen: 7,
            dirs_seen: 2,
            errors_seen: 2,
        },
        top_files: vec![
            FileSummary {
                path: root.join("alpha/big.bin"),
                name: OsString::from("big.bin"),
                used_bytes: 8_192,
            },
            FileSummary {
                path: root.join("hard-link-alias"),
                name: OsString::from("hard-link-alias"),
                used_bytes: 4_096,
            },
        ],
    }
}

fn assert_json_matches(actual: &str, expected: &str) {
    let actual_value: Value = serde_json::from_str(actual).unwrap();
    let expected_value: Value = serde_json::from_str(expected).unwrap();

    assert_eq!(actual_value, expected_value, "actual JSON:\n{actual}");
    assert_eq!(actual.trim_end(), expected.trim_end());
}
