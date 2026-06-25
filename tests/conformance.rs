#![cfg(unix)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use usedu::protocol::{build_scan_envelope, EnvelopeMode, EnvelopeOptions, ScanEnvelope};
use usedu::scanner::{scan_recursive, ScanOptions, SortKey};

#[test]
fn cli_json_v2_matches_protocol_conversion_for_same_fixture() {
    let fixture = Fixture::new("conformance-json-v2");
    fs::create_dir(fixture.path("child")).unwrap();
    write_file(&fixture.path("child/a.txt"), b"a");
    write_file(&fixture.path("root.txt"), b"root");

    let scan_options = ScanOptions {
        include_files_in_output: true,
        retained_tree_depth: 1,
        top_files_limit: 5,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &scan_options).unwrap();
    let direct = build_scan_envelope(
        &scan,
        &EnvelopeOptions {
            mode: EnvelopeMode::Report,
            depth: 1,
            top: 5,
            include_files: true,
            summarize: false,
            dirs_only: false,
            sort_key: SortKey::Used,
            show_errors: true,
            fast: false,
            cross_file_systems: false,
            jobs: None,
            max_output_entries: None,
            redact_paths: false,
        },
    );

    let output = Command::new(env!("CARGO_BIN_EXE_usedu"))
        .args([
            "report",
            fixture.root().to_str().unwrap(),
            "--format",
            "json-v2",
            "--depth",
            "1",
            "--top",
            "5",
            "--files",
            "--errors",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cli: ScanEnvelope = serde_json::from_slice(&output.stdout).unwrap();

    assert_eq!(cli.schema_version, direct.schema_version);
    assert_eq!(cli.semantics, direct.semantics);
    assert_eq!(cli.root.path_ref, direct.root.path_ref);
    assert_eq!(
        cli.root.counts.regular_files,
        direct.root.counts.regular_files
    );
    assert_eq!(cli.root.counts.directories, direct.root.counts.directories);
    assert_eq!(cli.entries.len(), direct.entries.len());
    assert_eq!(
        cli.entries
            .iter()
            .map(|entry| &entry.path_ref)
            .collect::<Vec<_>>(),
        direct
            .entries
            .iter()
            .map(|entry| &entry.path_ref)
            .collect::<Vec<_>>()
    );
}

#[test]
fn snapshot_envelope_round_trips_through_json() {
    let fixture = Fixture::new("conformance-snapshot");
    write_file(&fixture.path("file.txt"), b"file");
    let scan_options = ScanOptions {
        include_files_in_output: true,
        retained_tree_depth: 1,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &scan_options).unwrap();
    let envelope = build_scan_envelope(
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
            redact_paths: false,
        },
    );

    let encoded = serde_json::to_string_pretty(&envelope).unwrap();
    let decoded: ScanEnvelope = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded.schema_version, envelope.schema_version);
    assert_eq!(decoded.scan_id, envelope.scan_id);
    assert_eq!(decoded.root.path_ref, envelope.root.path_ref);
    assert_eq!(decoded.entries.len(), envelope.entries.len());
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

    fn root(&self) -> &Path {
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
