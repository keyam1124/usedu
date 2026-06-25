#![cfg(unix)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use usedu::output::{render_report, ReportOptions};
use usedu::scanner::{scan_recursive, ScanOptions, SortKey};
use usedu::util::path::escape_untrusted_text;

#[test]
fn human_report_escapes_control_characters_in_file_names() {
    let fixture = Fixture::new("control-output");
    write_file(&fixture.path("line\nbreak.txt"), b"line");
    write_file(&fixture.path("tab\tname.txt"), b"tab");
    write_file(&fixture.path("ansi\u{1b}[31m.txt"), b"ansi");

    let scan_options = ScanOptions {
        include_files_in_output: true,
        retained_tree_depth: 1,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &scan_options).unwrap();
    let report = render_report(
        &scan,
        &ReportOptions {
            depth: 1,
            top: 10,
            include_files: true,
            summarize: false,
            dirs_only: false,
            sort_key: SortKey::Name,
            show_errors: true,
        },
    );

    assert!(report.contains("line\\nbreak.txt"));
    assert!(report.contains("tab\\tname.txt"));
    assert!(report.contains("ansi\\x1b[31m.txt"));
    assert!(!report.contains("line\nbreak.txt"));
    assert!(!report.contains("tab\tname.txt"));
    assert!(!report.contains("ansi\u{1b}[31m.txt"));
}

#[test]
fn untrusted_text_escape_is_visible_and_deterministic() {
    assert_eq!(
        escape_untrusted_text("a\nb\rc\td\u{1b}e\u{7}"),
        "a\\nb\\rc\\td\\x1be\\u{7}"
    );
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
