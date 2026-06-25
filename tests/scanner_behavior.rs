#![cfg(unix)]

use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use usedu::scanner::{
    allocated_bytes, scan_current_level, scan_recursive, EntrySummary, ScanBudget,
    ScanCancellation, ScanOptions, ScanProgress, ScannerError,
};

#[test]
fn hidden_files_are_included() {
    let fixture = Fixture::new("hidden-files");
    write_file(&fixture.path(".hidden"), b"hidden");

    let scan = scan_recursive(fixture.root(), &ScanOptions::default()).unwrap();

    assert!(scan
        .root
        .children
        .iter()
        .any(|entry| entry.name().to_string_lossy() == ".hidden"));
    assert_eq!(scan.root.file_count, 1);
}

#[test]
fn symlinked_directories_are_not_followed() {
    let fixture = Fixture::new("symlinked-directories");
    fs::create_dir(fixture.path("target")).unwrap();
    write_file(&fixture.path("target/nested.txt"), b"nested");
    std::os::unix::fs::symlink(fixture.path("target"), fixture.path("link")).unwrap();

    let scan = scan_recursive(fixture.root(), &ScanOptions::default()).unwrap();
    let link = scan
        .root
        .children
        .iter()
        .find(|entry| entry.name().to_string_lossy() == "link")
        .unwrap();

    assert!(matches!(link, EntrySummary::Symlink(_)));
    assert_eq!(scan.root.dir_count, 2);
    assert_eq!(scan.root.file_count, 2);
}

#[test]
fn directory_totals_include_descendant_allocated_size() {
    let fixture = Fixture::new("directory-aggregation");
    fs::create_dir(fixture.path("child")).unwrap();
    write_file(&fixture.path("child/file.bin"), &[7; 8192]);

    let scan = scan_recursive(fixture.root(), &ScanOptions::default()).unwrap();
    let child = scan
        .root
        .children
        .iter()
        .find_map(|entry| entry.as_dir())
        .unwrap();
    let file_allocated =
        allocated_bytes(&fs::symlink_metadata(fixture.path("child/file.bin")).unwrap());

    assert!(child.used_bytes >= file_allocated);
    assert_eq!(child.file_count, 1);
}

#[test]
fn hard_links_are_counted_once() {
    let fixture = Fixture::new("hard-links");
    write_file(&fixture.path("original.bin"), &[1; 1024 * 1024]);
    fs::hard_link(fixture.path("original.bin"), fixture.path("alias.bin")).unwrap();

    let scan = scan_recursive(fixture.root(), &ScanOptions::default()).unwrap();
    let allocated = allocated_bytes(&fs::symlink_metadata(fixture.path("original.bin")).unwrap());

    if allocated > 0 {
        let own = allocated_bytes(&fs::symlink_metadata(fixture.root()).unwrap());
        assert!(scan.root.used_bytes >= own + allocated);
        assert!(scan.root.used_bytes < own + allocated.saturating_mul(2));
    }
    assert_eq!(scan.root.file_count, 2);
}

#[test]
fn hard_links_across_sibling_directories_have_stable_owner() {
    let fixture = Fixture::new("hard-links-siblings");
    fs::create_dir(fixture.path("a")).unwrap();
    fs::create_dir(fixture.path("b")).unwrap();
    write_file(&fixture.path("a/original.bin"), &[1; 1024 * 1024]);
    fs::hard_link(fixture.path("a/original.bin"), fixture.path("b/alias.bin")).unwrap();
    let allocated = allocated_bytes(&fs::symlink_metadata(fixture.path("a/original.bin")).unwrap());

    for _ in 0..8 {
        let scan = scan_recursive(fixture.root(), &ScanOptions::default()).unwrap();
        let a = scan
            .root
            .children
            .iter()
            .find_map(|entry| entry.as_dir().filter(|dir| dir.name == "a"))
            .unwrap();
        let b = scan
            .root
            .children
            .iter()
            .find_map(|entry| entry.as_dir().filter(|dir| dir.name == "b"))
            .unwrap();

        assert!(a.used_bytes >= allocated);
        assert!(b.used_bytes < allocated);
    }
}

#[test]
fn current_level_contains_only_direct_children_with_recursive_directory_sizes() {
    let fixture = Fixture::new("current-level");
    fs::create_dir(fixture.path("child")).unwrap();
    fs::create_dir(fixture.path("child/grandchild")).unwrap();
    write_file(&fixture.path("child/grandchild/file.bin"), &[2; 4096]);
    write_file(&fixture.path("direct.bin"), b"direct");

    let scan = scan_current_level(fixture.root(), &ScanOptions::default()).unwrap();

    assert_eq!(scan.rows.len(), 2);
    assert!(scan
        .rows
        .iter()
        .any(|entry| entry.name().to_string_lossy() == "child" && entry.is_dir()));
    assert!(scan
        .rows
        .iter()
        .all(|entry| entry.name().to_string_lossy() != "grandchild"));
}

#[test]
fn many_direct_children_scan_without_retaining_grandchildren() {
    let fixture = Fixture::new("many-direct-children");
    for index in 0..300 {
        let dir = fixture.path(format!("child-{index:03}"));
        fs::create_dir(&dir).unwrap();
        write_file(&dir.join("file.txt"), b"file");
    }

    let scan = scan_current_level(fixture.root(), &ScanOptions::default()).unwrap();

    assert_eq!(scan.rows.len(), 300);
    assert!(scan
        .rows
        .iter()
        .all(|entry| entry.as_dir().is_some_and(|dir| dir.children.is_empty())));
}

#[test]
fn nested_file_nodes_are_not_retained_when_not_report_relevant() {
    let fixture = Fixture::new("retained-file-depth");
    fs::create_dir(fixture.path("child")).unwrap();
    fs::create_dir(fixture.path("child/grandchild")).unwrap();
    write_file(&fixture.path("child/grandchild/deep.bin"), &[3; 4096]);

    let options = ScanOptions {
        include_files_in_output: false,
        retained_tree_depth: 0,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &options).unwrap();
    let child = scan
        .root
        .children
        .iter()
        .find_map(|entry| entry.as_dir())
        .unwrap();

    assert_eq!(child.file_count, 1);
    assert_eq!(child.dir_count, 2);
    assert!(child.children.is_empty());
}

#[test]
fn retained_tree_depth_keeps_files_needed_for_tree_output() {
    let fixture = Fixture::new("retained-tree-depth");
    fs::create_dir(fixture.path("child")).unwrap();
    write_file(&fixture.path("child/file.bin"), &[4; 4096]);

    let options = ScanOptions {
        include_files_in_output: true,
        retained_tree_depth: 2,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &options).unwrap();
    let child = scan
        .root
        .children
        .iter()
        .find_map(|entry| entry.as_dir())
        .unwrap();

    assert!(child
        .children
        .iter()
        .any(|entry| entry.name().to_string_lossy() == "file.bin"));
}

#[test]
fn fast_scan_keeps_counts_and_symlink_boundaries() {
    let fixture = Fixture::new("fast-scan");
    fs::create_dir(fixture.path("child")).unwrap();
    write_file(&fixture.path("child/file.bin"), &[1; 4096]);
    write_file(&fixture.path(".hidden"), b"hidden");
    symlink("child", fixture.path("child-link")).unwrap();

    let options = ScanOptions {
        fast: true,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &options).unwrap();

    assert_eq!(scan.root.file_count, 3);
    assert_eq!(scan.root.dir_count, 2);
    assert!(scan
        .root
        .children
        .iter()
        .any(|entry| matches!(entry, EntrySummary::Symlink(_))));
}

#[test]
fn fast_scan_aggregates_unretained_subtrees() {
    let fixture = Fixture::new("fast-unretained-subtree");
    fs::create_dir(fixture.path("child")).unwrap();
    fs::create_dir(fixture.path("child/grandchild")).unwrap();
    write_file(&fixture.path("child/grandchild/a.bin"), &[1; 4096]);
    write_file(&fixture.path("child/grandchild/b.bin"), &[2; 4096]);
    symlink("a.bin", fixture.path("child/grandchild/a-link")).unwrap();

    let options = ScanOptions {
        fast: true,
        retained_tree_depth: 0,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &options).unwrap();
    let child = scan
        .root
        .children
        .iter()
        .find_map(|entry| entry.as_dir())
        .unwrap();

    assert_eq!(child.file_count, 3);
    assert_eq!(child.dir_count, 2);
    assert!(child.children.is_empty());
}

#[test]
fn fast_summary_scan_does_not_retain_root_children() {
    let fixture = Fixture::new("fast-summary");
    fs::create_dir(fixture.path("child")).unwrap();
    write_file(&fixture.path("child/file.bin"), &[1; 4096]);

    let options = ScanOptions {
        fast: true,
        retained_tree_depth: 0,
        retain_root_children: false,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &options).unwrap();

    assert_eq!(scan.root.file_count, 1);
    assert_eq!(scan.root.dir_count, 2);
    assert!(scan.root.children.is_empty());
}

#[test]
fn top_files_are_collected_without_retaining_all_file_nodes() {
    let fixture = Fixture::new("top-files");
    fs::create_dir(fixture.path("child")).unwrap();
    write_file(&fixture.path("child/small.bin"), &[1; 4096]);
    write_file(&fixture.path("child/medium.bin"), &[2; 8192]);
    write_file(&fixture.path("child/large.bin"), &[3; 16 * 1024]);

    let options = ScanOptions {
        include_files_in_output: true,
        top_files_limit: 2,
        retained_tree_depth: 0,
        ..Default::default()
    };
    let scan = scan_recursive(fixture.root(), &options).unwrap();

    assert_eq!(scan.top_files.len(), 2);
    assert!(scan
        .top_files
        .windows(2)
        .all(|files| files[0].used_bytes >= files[1].used_bytes));
    assert!(scan.top_files.iter().all(|file| file.used_bytes > 0));
}

#[test]
fn progress_reports_scan_counts() {
    let fixture = Fixture::new("progress");
    write_file(&fixture.path("file.txt"), b"progress");
    let progress = ScanProgress::new();
    let options = ScanOptions {
        progress: Some(progress.clone()),
        ..Default::default()
    };

    let scan = scan_recursive(fixture.root(), &options).unwrap();
    let snapshot = progress.snapshot();

    assert!(snapshot.done);
    assert_eq!(snapshot.entries_seen, scan.metrics.entries_seen);
    assert_eq!(snapshot.files_seen, scan.metrics.files_seen);
    assert_eq!(snapshot.errors_seen, scan.metrics.errors_seen);
}

#[test]
fn cancellation_stops_scan() {
    let fixture = Fixture::new("cancellation");
    write_file(&fixture.path("file.txt"), b"cancel");
    let cancellation = ScanCancellation::default();
    cancellation.cancel();
    let options = ScanOptions {
        cancellation: Some(cancellation),
        ..Default::default()
    };

    let error = scan_recursive(fixture.root(), &options).unwrap_err();

    assert!(matches!(
        error.downcast_ref::<ScannerError>(),
        Some(ScannerError::Cancelled)
    ));
}

#[test]
fn scan_budget_stops_after_max_entries() {
    let fixture = Fixture::new("budget-max-entries");
    write_file(&fixture.path("a.txt"), b"a");
    write_file(&fixture.path("b.txt"), b"b");
    let options = ScanOptions {
        budget: ScanBudget {
            max_entries: Some(1),
            max_duration: None,
        },
        ..Default::default()
    };

    let error = scan_recursive(fixture.root(), &options).unwrap_err();

    assert!(matches!(
        error.downcast_ref::<ScannerError>(),
        Some(ScannerError::ResourceLimitReached("max_entries"))
    ));
}

#[test]
fn scan_budget_stops_after_max_duration() {
    let fixture = Fixture::new("budget-max-duration");
    write_file(&fixture.path("a.txt"), b"a");
    let options = ScanOptions {
        budget: ScanBudget {
            max_entries: None,
            max_duration: Some(Duration::from_nanos(0)),
        },
        ..Default::default()
    };

    let error = scan_recursive(fixture.root(), &options).unwrap_err();

    assert!(matches!(
        error.downcast_ref::<ScannerError>(),
        Some(ScannerError::ResourceLimitReached("max_duration"))
    ));
}

#[test]
fn permission_errors_do_not_abort_scan() {
    let fixture = Fixture::new("permission-errors");
    fs::create_dir(fixture.path("locked")).unwrap();
    write_file(&fixture.path("visible.txt"), b"visible");
    fs::set_permissions(fixture.path("locked"), fs::Permissions::from_mode(0o000)).unwrap();

    let scan = scan_recursive(fixture.root(), &ScanOptions::default()).unwrap();

    fs::set_permissions(fixture.path("locked"), fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(scan.root.file_count, 1);
    assert!(scan.metrics.errors_seen >= 1);
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
        let _ = fs::set_permissions(self.path("locked"), fs::Permissions::from_mode(0o700));
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_file(path: &Path, bytes: &[u8]) {
    let mut file = File::create(path).unwrap();
    file.write_all(bytes).unwrap();
    file.sync_all().unwrap();
}
