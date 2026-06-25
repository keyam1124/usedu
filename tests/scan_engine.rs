#![cfg(unix)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use usedu::scanner::ScanOptions;
use usedu_core::engine::{Collector, ScanEngine, ScanRequest, SummaryCollector, TopKCollector};

#[test]
fn scan_engine_runs_core_scan_and_collectors() {
    let fixture = Fixture::new("scan-engine");
    write_file(&fixture.path("small.txt"), b"small");
    write_file(&fixture.path("large.txt"), &[1; 16 * 1024]);
    let engine = ScanEngine;
    let outcome = engine
        .scan(ScanRequest {
            root: fixture.root().to_path_buf(),
            options: ScanOptions {
                include_files_in_output: true,
                top_files_limit: 2,
                retained_tree_depth: 1,
                ..Default::default()
            },
        })
        .unwrap();

    let summary = SummaryCollector.collect(&outcome);
    let top_files = TopKCollector { limit: 1 }.collect(&outcome);

    assert_eq!(summary.regular_file_count, 2);
    assert_eq!(summary.directory_count, 1);
    assert_eq!(top_files.len(), 1);
    assert!(top_files[0].used_bytes > 0);
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
