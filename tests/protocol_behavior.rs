#![cfg(unix)]

use serde_json::Value;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use usedu::protocol::{
    build_scan_envelope, diff_snapshots, json_v2_schema, EnvelopeMode, EnvelopeOptions,
    ScanEnvelope, ScanStateDto, SCAN_SCHEMA_VERSION,
};
use usedu::scanner::{
    scan_recursive, DirSummary, EntryCounts, EntrySummary, FileSummary, ScanBudget, ScanMetrics,
    ScanOptions, ScanResult, SortKey,
};

#[test]
fn json_v2_envelope_reflects_report_options_and_separate_counts() {
    let fixture = Fixture::new("json-v2-options");
    fs::create_dir(fixture.path("small-dir")).unwrap();
    fs::create_dir(fixture.path("large-dir")).unwrap();
    write_file(&fixture.path("small-dir/a.txt"), b"a");
    write_file(&fixture.path("large-dir/big.bin"), &[1; 16 * 1024]);
    write_file(&fixture.path("top-level.txt"), b"top");
    symlink("top-level.txt", fixture.path("top-link")).unwrap();

    let scan_options = ScanOptions {
        include_files_in_output: true,
        retained_tree_depth: 2,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &scan_options).unwrap();
    let envelope = build_scan_envelope(
        &scan,
        &EnvelopeOptions {
            mode: EnvelopeMode::Report,
            depth: 2,
            top: 1,
            include_files: false,
            summarize: false,
            dirs_only: true,
            sort_key: SortKey::Used,
            show_errors: false,
            fast: false,
            cross_file_systems: false,
            jobs: Some(1),
            max_output_entries: None,
            max_output_bytes: None,
            redact_paths: false,
        },
    );

    assert_eq!(envelope.schema_version, SCAN_SCHEMA_VERSION);
    assert_eq!(envelope.effective_options.top, 1);
    assert!(envelope.effective_options.dirs_only);
    assert_eq!(
        envelope.effective_options.sort,
        usedu::protocol::ProtocolSort::Used
    );
    assert_eq!(envelope.entries.len(), 1);
    assert_eq!(
        envelope.entries[0].kind,
        usedu::protocol::EntryKindDto::Directory
    );
    assert_eq!(envelope.root.counts.directories, 3);
    assert_eq!(envelope.root.counts.regular_files, 3);
    assert_eq!(envelope.root.counts.symlinks, 1);
    assert_eq!(envelope.root.counts.other, 0);
    assert!(envelope.next_cursor.is_some());
}

#[test]
fn json_v2_reports_limit_reached_when_output_bytes_are_capped() {
    let fixture = Fixture::new("json-v2-output-byte-limit");
    write_file(&fixture.path("a.txt"), b"a");
    write_file(&fixture.path("b.txt"), b"b");
    let scan_options = ScanOptions {
        include_files_in_output: true,
        retained_tree_depth: 1,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &scan_options).unwrap();
    let mut options = default_report_options();
    options.max_output_bytes = Some(1);

    let envelope = build_scan_envelope(&scan, &options);

    assert_eq!(envelope.status.state, ScanStateDto::LimitReached);
    assert!(envelope
        .status
        .partial_reasons
        .iter()
        .any(|reason| reason == "maxOutputBytes"));
}

#[test]
fn json_v2_reports_scan_resource_limits_as_structured_issues() {
    let fixture = Fixture::new("json-v2-scan-budget");
    write_file(&fixture.path("a.txt"), b"a");
    write_file(&fixture.path("b.txt"), b"b");
    let scan_options = ScanOptions {
        budget: ScanBudget {
            max_entries: Some(1),
            max_duration: None,
        },
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &scan_options).unwrap();

    let envelope = build_scan_envelope(&scan, &default_report_options());

    assert_eq!(envelope.status.state, ScanStateDto::Partial);
    assert!(envelope
        .status
        .partial_reasons
        .iter()
        .any(|reason| reason == "resourceLimitReached"));
    assert!(envelope
        .issues
        .iter()
        .any(|issue| issue.code == "RESOURCE_LIMIT_REACHED"));
}

#[test]
fn json_v2_path_ref_is_reversible_for_non_utf8_paths() {
    use std::os::unix::ffi::OsStringExt;

    let raw_name = std::ffi::OsString::from_vec(b"bad-\xff-name".to_vec());
    let child = EntrySummary::File(FileSummary {
        path: PathBuf::from("/fixture").join(&raw_name),
        name: raw_name,
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
    let envelope = build_scan_envelope(&scan, &default_report_options());
    let entry = envelope
        .entries
        .iter()
        .find(|entry| entry.display_name.contains("bad-"))
        .unwrap();

    assert!(entry.display_name.contains('\u{FFFD}'));
    assert!(entry.path_ref.bytes_hex.ends_with("6261642dff2d6e616d65"));
}

#[test]
fn json_v2_reports_limit_reached_when_output_entries_are_capped() {
    let fixture = Fixture::new("json-v2-output-limit");
    write_file(&fixture.path("a.txt"), b"a");
    write_file(&fixture.path("b.txt"), b"b");
    let scan_options = ScanOptions {
        include_files_in_output: true,
        retained_tree_depth: 1,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &scan_options).unwrap();
    let mut options = default_report_options();
    options.max_output_entries = Some(1);

    let envelope = build_scan_envelope(&scan, &options);

    assert_eq!(envelope.status.state, ScanStateDto::LimitReached);
    assert_eq!(envelope.entries.len(), 1);
    assert!(envelope
        .status
        .partial_reasons
        .iter()
        .any(|reason| reason == "maxOutputEntries"));
    assert!(envelope.next_cursor.is_some());
}

#[test]
fn snapshot_diff_marks_added_removed_and_changed_entries() {
    let before = snapshot_fixture(
        "/fixture",
        &[("same", 4096), ("grow", 4096), ("remove", 4096)],
    );
    let after = snapshot_fixture(
        "/fixture",
        &[("same", 4096), ("grow", 16 * 1024), ("add", 8192)],
    );

    let diff = diff_snapshots(&before, &after);
    let changes: Vec<_> = diff
        .changes
        .iter()
        .map(|change| change.change.as_str())
        .collect();

    assert!(diff.status.exact);
    assert_eq!(diff.summary.added, 1);
    assert_eq!(diff.summary.removed, 1);
    assert_eq!(diff.summary.grown, 2);
    assert_eq!(diff.summary.unchanged, 1);
    assert!(changes.contains(&"added"));
    assert!(changes.contains(&"removed"));
    assert!(changes.contains(&"grown"));
}

#[test]
fn json_v2_schema_is_valid_json() {
    let schema: Value = serde_json::from_str(json_v2_schema()).unwrap();

    assert_eq!(
        schema["properties"]["schemaVersion"]["const"],
        SCAN_SCHEMA_VERSION
    );
}

fn default_report_options() -> EnvelopeOptions {
    EnvelopeOptions {
        mode: EnvelopeMode::Report,
        depth: 1,
        top: 30,
        include_files: true,
        summarize: false,
        dirs_only: false,
        sort_key: SortKey::Used,
        show_errors: true,
        fast: false,
        cross_file_systems: false,
        jobs: None,
        max_output_entries: None,
        max_output_bytes: None,
        redact_paths: false,
    }
}

fn snapshot_fixture(root: &str, entries: &[(&str, u64)]) -> ScanEnvelope {
    let fixture = Fixture::new("snapshot-diff");
    for (name, size) in entries {
        write_file(&fixture.path(name), &vec![1; *size as usize]);
    }
    let scan_options = ScanOptions {
        include_files_in_output: true,
        retained_tree_depth: 1,
        ..Default::default()
    };
    let mut scan = scan_recursive(fixture.root(), &scan_options).unwrap();
    scan.root.path = PathBuf::from(root);
    for entry in &mut scan.root.children {
        let relative = entry.path().file_name().unwrap().to_owned();
        match entry {
            usedu::scanner::EntrySummary::File(file)
            | usedu::scanner::EntrySummary::Symlink(file)
            | usedu::scanner::EntrySummary::Other(file) => {
                file.path = PathBuf::from(root).join(relative);
            }
            usedu::scanner::EntrySummary::Dir(dir) => {
                dir.path = PathBuf::from(root).join(relative);
            }
        }
    }
    build_scan_envelope(
        &scan,
        &EnvelopeOptions {
            mode: EnvelopeMode::Snapshot,
            depth: 1,
            top: 0,
            include_files: true,
            summarize: false,
            dirs_only: false,
            sort_key: SortKey::Used,
            show_errors: true,
            fast: false,
            cross_file_systems: false,
            jobs: None,
            max_output_entries: None,
            max_output_bytes: None,
            redact_paths: false,
        },
    )
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("usedu-{name}-{nonce}"));
        fs::create_dir(&root).unwrap();
        Self { root }
    }

    fn root(&self) -> &PathBuf {
        &self.root
    }

    fn path(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_file(path: &Path, bytes: &[u8]) {
    let mut file = File::create(path).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
