use super::bulk::{self, BulkEntryKind};
use super::errors::ScannerError;
use super::metadata::{allocated_bytes, device_id, inode_id, link_count};
use super::options::ScanOptions;
use super::progress::ScanProgress;
use super::result::{
    CurrentLevelScan, DirSummary, EntryCounts, EntryKind, EntrySummary, FileSummary,
    ScanErrorRecord, ScanMetrics, ScanResult,
};
use anyhow::{anyhow, Context, Result};
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

static THREAD_POOLS: OnceLock<Mutex<HashMap<usize, Arc<ThreadPool>>>> = OnceLock::new();

const NESTED_PARALLEL_MIN_DIRS: usize = 96;
const HARD_LINK_SHARDS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FileIdentity {
    dev: u64,
    ino: u64,
}

struct DirTotals {
    used_bytes: u64,
    file_count: u64,
    dir_count: u64,
    counts: EntryCounts,
    errors: Vec<ScanErrorRecord>,
}

#[derive(Clone)]
struct ScanState {
    options: ScanOptions,
    root_dev: u64,
    shared: Arc<SharedScanState>,
}

struct SharedScanState {
    errors: Mutex<Vec<ScanErrorRecord>>,
    seen_hard_links: Vec<Mutex<HashSet<FileIdentity>>>,
    top_files: Mutex<Vec<FileSummary>>,
    top_file_floor: AtomicU64,
}

struct ProgressGuard {
    progress: Option<ScanProgress>,
}

pub fn scan_recursive(path: impl AsRef<Path>, options: &ScanOptions) -> Result<ScanResult> {
    let started = Instant::now();
    let path = make_absolute(path.as_ref())?;
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let root_dev = device_id(&metadata);
    let state = ScanState::new(options.clone(), root_dev);
    let _progress_guard = ProgressGuard::new(state.options.progress.clone());
    state.increment_entries(1);

    let mut root = if state.options.fast && metadata.is_dir() && !metadata.file_type().is_symlink()
    {
        scan_dir_fast(&path, &state, 0)?
    } else if metadata.is_dir() && !metadata.file_type().is_symlink() {
        scan_dir(&path, &metadata, &state, 0)?
    } else {
        let entry = scan_leaf(&path, display_name(&path), &metadata, &state)?;
        pseudo_root_for_leaf(&path, allocated_bytes(&metadata), entry)
    };

    let elapsed = started.elapsed();
    root.errors = state.errors();
    let metrics = state.final_metrics(&root, elapsed);
    state.publish_final_metrics(&metrics);

    Ok(ScanResult {
        root,
        metrics,
        top_files: state.top_files(),
    })
}

pub fn scan_current_level(
    path: impl AsRef<Path>,
    options: &ScanOptions,
) -> Result<CurrentLevelScan> {
    let started = Instant::now();
    let path = make_absolute(path.as_ref())?;
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ScannerError::NotDirectory(path).into());
    }

    let root_dev = device_id(&metadata);
    let state = ScanState::new(options.clone(), root_dev);
    let _progress_guard = ProgressGuard::new(state.options.progress.clone());
    state.increment_entries(1);
    state.increment_dirs(1);

    let mut root = if state.options.fast {
        let mut root = empty_dir_summary_fast(&path);
        read_children_fast_into(&mut root, &state, 0)?;
        root
    } else {
        let mut root = empty_dir_summary(&path, &metadata);
        read_children_into(&mut root, &state, 0)?;
        root
    };
    let elapsed = started.elapsed();
    root.errors = state.errors();
    let rows = root.children.clone();
    let metrics = state.final_metrics(&root, elapsed);
    state.publish_final_metrics(&metrics);

    Ok(CurrentLevelScan {
        root,
        metrics,
        rows,
    })
}

fn make_absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

impl ScanState {
    fn new(options: ScanOptions, root_dev: u64) -> Self {
        if let Some(progress) = &options.progress {
            progress.reset();
        }
        Self {
            options,
            root_dev,
            shared: Arc::new(SharedScanState {
                errors: Mutex::new(Vec::new()),
                seen_hard_links: (0..HARD_LINK_SHARDS)
                    .map(|_| Mutex::new(HashSet::new()))
                    .collect(),
                top_files: Mutex::new(Vec::new()),
                top_file_floor: AtomicU64::new(0),
            }),
        }
    }

    fn check_cancelled(&self) -> Result<()> {
        if self
            .options
            .cancellation
            .as_ref()
            .is_some_and(|cancellation| cancellation.is_cancelled())
        {
            return Err(ScannerError::Cancelled.into());
        }
        Ok(())
    }

    fn increment_entries(&self, count: u64) {
        if let Some(progress) = &self.options.progress {
            progress.increment_entries(count);
        }
    }

    fn increment_files(&self, count: u64) {
        if let Some(progress) = &self.options.progress {
            progress.increment_files(count);
        }
    }

    fn increment_dirs(&self, count: u64) {
        if let Some(progress) = &self.options.progress {
            progress.increment_dirs(count);
        }
    }

    fn increment_errors(&self, count: u64) {
        if let Some(progress) = &self.options.progress {
            progress.increment_errors(count);
        }
    }

    fn final_metrics(&self, root: &DirSummary, elapsed: Duration) -> ScanMetrics {
        ScanMetrics {
            elapsed,
            entries_seen: root.file_count.saturating_add(root.dir_count),
            files_seen: root.file_count,
            dirs_seen: root.dir_count,
            errors_seen: root.errors.len() as u64,
        }
    }

    fn publish_final_metrics(&self, metrics: &ScanMetrics) {
        if let Some(progress) = &self.options.progress {
            progress.publish(metrics);
        }
    }

    fn errors(&self) -> Vec<ScanErrorRecord> {
        self.shared
            .errors
            .lock()
            .expect("scan errors mutex poisoned")
            .clone()
    }

    fn top_files(&self) -> Vec<FileSummary> {
        self.shared
            .top_files
            .lock()
            .expect("scan top files mutex poisoned")
            .clone()
    }

    fn record_error(
        &self,
        path: impl Into<PathBuf>,
        kind: impl Into<String>,
        message: impl Into<String>,
    ) -> ScanErrorRecord {
        let record = ScanErrorRecord {
            path: path.into(),
            kind: kind.into(),
            message: message.into(),
        };
        self.increment_errors(1);
        self.shared
            .errors
            .lock()
            .expect("scan errors mutex poisoned")
            .push(record.clone());
        record
    }

    fn record_io_error(&self, path: impl Into<PathBuf>, error: &io::Error) -> ScanErrorRecord {
        self.record_error(path, format!("{:?}", error.kind()), error.to_string())
    }

    fn should_skip_cross_fs(&self, metadata: &Metadata) -> bool {
        self.should_skip_cross_fs_device(device_id(metadata))
    }

    fn should_skip_cross_fs_device(&self, dev: u64) -> bool {
        !self.options.cross_file_systems && dev != 0 && dev != self.root_dev
    }

    fn leaf_used_bytes(&self, metadata: &Metadata) -> u64 {
        self.leaf_used_bytes_from_parts(
            metadata.is_file(),
            link_count(metadata),
            device_id(metadata),
            inode_id(metadata),
            allocated_bytes(metadata),
        )
    }

    fn leaf_used_bytes_from_parts(
        &self,
        is_file: bool,
        link_count: u64,
        dev: u64,
        ino: u64,
        used_bytes: u64,
    ) -> u64 {
        if is_file && link_count > 1 {
            let id = FileIdentity { dev, ino };
            let shard_index = (id.ino as usize) % self.shared.seen_hard_links.len();
            let mut seen = self
                .shared
                .seen_hard_links
                .get(shard_index)
                .expect("hard link shard index out of range")
                .lock()
                .expect("scan hard link shard mutex poisoned");
            if !seen.insert(id) {
                return 0;
            }
        }
        used_bytes
    }

    fn record_top_file(&self, summary: &FileSummary, metadata: &Metadata) {
        if !metadata.is_file() || !self.should_consider_top_file(summary.used_bytes) {
            return;
        }
        self.record_top_file_candidate(summary.clone());
    }

    fn record_top_file_candidate(&self, summary: FileSummary) {
        if !self.should_consider_top_file(summary.used_bytes) {
            return;
        }
        let mut top_files = self
            .shared
            .top_files
            .lock()
            .expect("scan top files mutex poisoned");
        let limit = self.options.top_files_limit;
        if top_files.len() >= limit
            && top_files
                .last()
                .is_some_and(|smallest| summary.used_bytes <= smallest.used_bytes)
        {
            return;
        }

        top_files.push(summary);
        top_files.sort_by_key(|file| Reverse(file.used_bytes));
        if top_files.len() > limit {
            top_files.truncate(limit);
        }
        let floor = if top_files.len() >= limit {
            top_files.last().map_or(0, |file| file.used_bytes)
        } else {
            0
        };
        self.shared.top_file_floor.store(floor, Ordering::Relaxed);
    }

    fn should_consider_top_file(&self, used_bytes: u64) -> bool {
        self.options.include_files_in_output
            && self.options.top_files_limit > 0
            && used_bytes > 0
            && {
                let floor = self.shared.top_file_floor.load(Ordering::Relaxed);
                floor == 0 || used_bytes > floor
            }
    }

    fn worker_count(&self) -> usize {
        self.options.jobs.unwrap_or(1).max(1)
    }

    fn should_parallelize_dirs(&self, current_depth: usize, dir_count: usize) -> bool {
        if self.worker_count() <= 1 || dir_count <= 1 {
            return false;
        }
        self.options.fast || current_depth == 0 || dir_count >= NESTED_PARALLEL_MIN_DIRS
    }

    fn should_retain_dir(&self, current_depth: usize) -> bool {
        (current_depth == 0 && self.options.retain_root_children)
            || current_depth < self.options.retained_tree_depth
    }

    fn should_retain_leaf(&self, current_depth: usize) -> bool {
        (current_depth == 0 && self.options.retain_root_children)
            || (self.options.include_files_in_output
                && current_depth < self.options.retained_tree_depth)
    }
}

fn scanner_thread_pool(worker_count: usize) -> Result<Arc<ThreadPool>> {
    let pools = THREAD_POOLS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(pool) = pools
        .lock()
        .expect("scanner thread pool cache mutex poisoned")
        .get(&worker_count)
        .cloned()
    {
        return Ok(pool);
    }

    let pool = Arc::new(
        ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .thread_name(|idx| format!("usedu-scanner-{idx}"))
            .build()
            .context("failed to create scanner thread pool")?,
    );
    let mut pools = pools
        .lock()
        .expect("scanner thread pool cache mutex poisoned");
    Ok(pools.entry(worker_count).or_insert_with(|| pool).clone())
}

impl ProgressGuard {
    fn new(progress: Option<ScanProgress>) -> Self {
        Self { progress }
    }
}

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        if let Some(progress) = &self.progress {
            progress.mark_done();
        }
    }
}

fn scan_dir(
    path: &Path,
    metadata: &Metadata,
    state: &ScanState,
    current_depth: usize,
) -> Result<DirSummary> {
    state.check_cancelled()?;
    state.increment_dirs(1);
    let mut summary = empty_dir_summary(path, metadata);
    read_children_into(&mut summary, state, current_depth)?;
    Ok(summary)
}

fn empty_dir_summary(path: &Path, metadata: &Metadata) -> DirSummary {
    empty_dir_summary_with_own(path, allocated_bytes(metadata))
}

fn empty_dir_summary_with_own(path: &Path, own_bytes: u64) -> DirSummary {
    DirSummary {
        path: path.to_path_buf(),
        name: display_name(path),
        used_bytes: own_bytes,
        own_bytes,
        file_count: 0,
        dir_count: 1,
        counts: EntryCounts::directory(),
        errors: Vec::new(),
        children: Vec::new(),
    }
}

fn empty_dir_summary_fast(path: &Path) -> DirSummary {
    DirSummary {
        path: path.to_path_buf(),
        name: display_name(path),
        used_bytes: 0,
        own_bytes: 0,
        file_count: 0,
        dir_count: 1,
        counts: EntryCounts::directory(),
        errors: Vec::new(),
        children: Vec::new(),
    }
}

fn read_children_into(
    summary: &mut DirSummary,
    state: &ScanState,
    current_depth: usize,
) -> Result<()> {
    state.check_cancelled()?;
    let entries = match fs::read_dir(&summary.path) {
        Ok(entries) => entries,
        Err(error) => {
            let record = state.record_io_error(summary.path.clone(), &error);
            summary.errors.push(record);
            return Ok(());
        }
    };

    let mut dir_children = Vec::new();

    for entry_result in entries {
        state.check_cancelled()?;
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                let record = state.record_io_error(summary.path.clone(), &error);
                summary.errors.push(record);
                continue;
            }
        };

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                let child_path = entry.path();
                let record = state.record_io_error(child_path, &error);
                summary.errors.push(record);
                continue;
            }
        };

        state.increment_entries(1);
        if state.should_skip_cross_fs(&metadata) {
            let child_path = entry.path();
            let record = state.record_error(
                child_path,
                "cross_file_system",
                "skipped entry on a different filesystem; pass --cross-file-systems to include it",
            );
            summary.errors.push(record);
            continue;
        }

        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            let child_path = entry.path();
            dir_children.push((child_path, metadata));
        } else {
            let retain_child = state.should_retain_leaf(current_depth);
            if retain_child {
                let child_path = entry.path();
                let child =
                    scan_leaf_after_cancel_check(&child_path, entry.file_name(), &metadata, state);
                add_child_to_summary(summary, child, retain_child);
            } else {
                let (used_bytes, kind) = scan_unretained_leaf_after_cancel_check(&metadata, state);
                if metadata.is_file() && state.should_consider_top_file(used_bytes) {
                    let child_path = entry.path();
                    state.record_top_file_candidate(FileSummary {
                        path: child_path,
                        name: entry.file_name(),
                        used_bytes,
                    });
                }
                add_leaf_to_summary(summary, used_bytes, counts_for_kind(kind));
            }
        }
    }

    let dir_entries = scan_dir_children(dir_children, state, current_depth)?;
    for child in dir_entries {
        add_child_to_summary(summary, child, state.should_retain_dir(current_depth));
    }
    Ok(())
}

fn scan_dir_fast(path: &Path, state: &ScanState, current_depth: usize) -> Result<DirSummary> {
    state.check_cancelled()?;
    state.increment_dirs(1);
    let mut summary = empty_dir_summary_fast(path);
    read_children_fast_into(&mut summary, state, current_depth)?;
    Ok(summary)
}

fn read_children_fast_into(
    summary: &mut DirSummary,
    state: &ScanState,
    current_depth: usize,
) -> Result<()> {
    state.check_cancelled()?;
    if read_children_fast_bulk_into(summary, state, current_depth)? {
        return Ok(());
    }

    let entries = match fs::read_dir(&summary.path) {
        Ok(entries) => entries,
        Err(error) => {
            let record = state.record_io_error(summary.path.clone(), &error);
            summary.errors.push(record);
            return Ok(());
        }
    };

    let mut dir_children = Vec::new();

    for entry_result in entries {
        state.check_cancelled()?;
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                let record = state.record_io_error(summary.path.clone(), &error);
                summary.errors.push(record);
                continue;
            }
        };

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                let child_path = entry.path();
                let record = state.record_io_error(child_path, &error);
                summary.errors.push(record);
                continue;
            }
        };

        state.increment_entries(1);
        if file_type.is_dir() && !file_type.is_symlink() {
            dir_children.push(entry.path());
        } else if file_type.is_symlink() {
            let retain_child = state.should_retain_leaf(current_depth);
            if retain_child {
                add_child_to_summary(
                    summary,
                    EntrySummary::Symlink(FileSummary {
                        path: entry.path(),
                        name: entry.file_name(),
                        used_bytes: 0,
                    }),
                    retain_child,
                );
            } else {
                add_leaf_to_summary(summary, 0, EntryCounts::symlink());
            }
        } else {
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    let child_path = entry.path();
                    let record = state.record_io_error(child_path, &error);
                    summary.errors.push(record);
                    continue;
                }
            };
            let retain_child = state.should_retain_leaf(current_depth);
            let used_bytes = allocated_bytes(&metadata);
            if retain_child {
                let file_summary = FileSummary {
                    path: entry.path(),
                    name: entry.file_name(),
                    used_bytes,
                };
                state.record_top_file(&file_summary, &metadata);
                let child = if metadata.is_file() {
                    EntrySummary::File(file_summary)
                } else {
                    EntrySummary::Other(file_summary)
                };
                add_child_to_summary(summary, child, retain_child);
            } else {
                if metadata.is_file() && state.should_consider_top_file(used_bytes) {
                    state.record_top_file_candidate(FileSummary {
                        path: entry.path(),
                        name: entry.file_name(),
                        used_bytes,
                    });
                }
                add_leaf_to_summary(summary, used_bytes, counts_for_metadata(&metadata));
            }
        }
    }

    add_fast_dir_children_to_summary(summary, dir_children, state, current_depth)?;
    Ok(())
}

fn read_children_fast_bulk_into(
    summary: &mut DirSummary,
    state: &ScanState,
    current_depth: usize,
) -> Result<bool> {
    let entries = match bulk::read_dir_fast(&summary.path) {
        Ok(Some(entries)) => entries,
        Ok(None) => return Ok(false),
        Err(error) => {
            let record = state.record_io_error(summary.path.clone(), &error);
            summary.errors.push(record);
            return Ok(true);
        }
    };

    let mut dir_children = Vec::new();
    for entry in entries {
        state.check_cancelled()?;
        if let Some(error) = entry.error {
            let record = state.record_io_error(entry.path, &error);
            summary.errors.push(record);
            continue;
        }

        state.increment_entries(1);
        match entry.kind {
            BulkEntryKind::Dir => dir_children.push(entry.path),
            BulkEntryKind::Symlink => {
                let retain_child = state.should_retain_leaf(current_depth);
                if retain_child {
                    add_child_to_summary(
                        summary,
                        EntrySummary::Symlink(FileSummary {
                            path: entry.path,
                            name: entry.name,
                            used_bytes: 0,
                        }),
                        true,
                    );
                } else {
                    add_leaf_to_summary(summary, 0, EntryCounts::symlink());
                }
            }
            BulkEntryKind::File | BulkEntryKind::Other => {
                let retain_child = state.should_retain_leaf(current_depth);
                let is_file = entry.kind == BulkEntryKind::File;
                if retain_child {
                    let file_summary = FileSummary {
                        path: entry.path,
                        name: entry.name,
                        used_bytes: entry.used_bytes,
                    };
                    if is_file {
                        state.record_top_file_candidate(file_summary.clone());
                    }
                    let child = if is_file {
                        EntrySummary::File(file_summary)
                    } else {
                        EntrySummary::Other(file_summary)
                    };
                    add_child_to_summary(summary, child, true);
                } else {
                    if is_file && state.should_consider_top_file(entry.used_bytes) {
                        state.record_top_file_candidate(FileSummary {
                            path: entry.path,
                            name: entry.name,
                            used_bytes: entry.used_bytes,
                        });
                    }
                    add_leaf_to_summary(
                        summary,
                        entry.used_bytes,
                        counts_for_bulk_kind(entry.kind),
                    );
                }
            }
        }
    }

    add_fast_dir_children_to_summary(summary, dir_children, state, current_depth)?;
    Ok(true)
}

fn add_fast_dir_children_to_summary(
    summary: &mut DirSummary,
    dir_children: Vec<PathBuf>,
    state: &ScanState,
    current_depth: usize,
) -> Result<()> {
    if state.should_retain_dir(current_depth) {
        let dir_entries = scan_dir_children_fast(dir_children, state, current_depth)?;
        for child in dir_entries {
            add_child_to_summary(summary, child, true);
        }
    } else {
        let dir_totals = scan_unretained_dir_children_fast(dir_children, state)?;
        for child in dir_totals {
            add_dir_totals_to_summary(summary, child);
        }
    }
    Ok(())
}

fn scan_dir_children(
    dir_children: Vec<(PathBuf, Metadata)>,
    state: &ScanState,
    current_depth: usize,
) -> Result<Vec<EntrySummary>> {
    if !state.should_parallelize_dirs(current_depth, dir_children.len()) {
        return dir_children
            .into_iter()
            .map(|(child_path, metadata)| {
                scan_dir(&child_path, &metadata, state, current_depth + 1).map(EntrySummary::Dir)
            })
            .collect();
    }

    if current_depth == 0 {
        scan_root_dir_children(dir_children, state, current_depth)
    } else {
        scan_nested_dir_children(&dir_children, state, current_depth)
    }
}

fn scan_root_dir_children(
    dir_children: Vec<(PathBuf, Metadata)>,
    state: &ScanState,
    current_depth: usize,
) -> Result<Vec<EntrySummary>> {
    state.check_cancelled()?;
    let worker_count = state.worker_count().min(dir_children.len());
    let next_child = AtomicUsize::new(0);
    std::thread::scope(|scope| -> Result<Vec<EntrySummary>> {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let state = state.clone();
            let dir_children = &dir_children;
            let next_child = &next_child;
            handles.push(scope.spawn(move || -> Result<Vec<EntrySummary>> {
                let mut entries = Vec::new();
                loop {
                    let index = next_child.fetch_add(1, Ordering::Relaxed);
                    let Some((child_path, metadata)) = dir_children.get(index) else {
                        break;
                    };
                    entries.push(
                        scan_dir(child_path, metadata, &state, current_depth + 1)
                            .map(EntrySummary::Dir)?,
                    );
                }
                Ok(entries)
            }));
        }

        let mut entries = Vec::with_capacity(dir_children.len());
        for handle in handles {
            entries.extend(
                handle
                    .join()
                    .map_err(|_| anyhow!("scanner worker panicked"))??,
            );
        }
        Ok(entries)
    })
}

fn scan_nested_dir_children(
    dir_children: &[(PathBuf, Metadata)],
    state: &ScanState,
    current_depth: usize,
) -> Result<Vec<EntrySummary>> {
    state.check_cancelled()?;
    scanner_thread_pool(state.worker_count())?.install(|| {
        dir_children
            .par_iter()
            .map(|(child_path, metadata)| {
                scan_dir(child_path, metadata, state, current_depth + 1).map(EntrySummary::Dir)
            })
            .collect()
    })
}

fn scan_dir_children_fast(
    dir_children: Vec<PathBuf>,
    state: &ScanState,
    current_depth: usize,
) -> Result<Vec<EntrySummary>> {
    if !state.should_parallelize_dirs(current_depth, dir_children.len()) {
        return dir_children
            .into_iter()
            .map(|child_path| {
                scan_dir_fast(&child_path, state, current_depth + 1).map(EntrySummary::Dir)
            })
            .collect();
    }

    scan_nested_dir_children_fast(&dir_children, state, current_depth)
}

fn scan_nested_dir_children_fast(
    dir_children: &[PathBuf],
    state: &ScanState,
    current_depth: usize,
) -> Result<Vec<EntrySummary>> {
    state.check_cancelled()?;
    scanner_thread_pool(state.worker_count())?.install(|| {
        dir_children
            .par_iter()
            .map(|child_path| {
                scan_dir_fast(child_path, state, current_depth + 1).map(EntrySummary::Dir)
            })
            .collect()
    })
}

fn scan_unretained_dir_fast(path: &Path, state: &ScanState) -> Result<DirTotals> {
    state.check_cancelled()?;
    state.increment_dirs(1);
    let mut totals = DirTotals {
        used_bytes: 0,
        file_count: 0,
        dir_count: 1,
        counts: EntryCounts::directory(),
        errors: Vec::new(),
    };
    if scan_unretained_dir_fast_bulk_into(&mut totals, path, state)? {
        return Ok(totals);
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            let record = state.record_io_error(path.to_path_buf(), &error);
            totals.errors.push(record);
            return Ok(totals);
        }
    };

    let mut dir_children = Vec::new();
    for entry_result in entries {
        state.check_cancelled()?;
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                let record = state.record_io_error(path.to_path_buf(), &error);
                totals.errors.push(record);
                continue;
            }
        };

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                let record = state.record_io_error(entry.path(), &error);
                totals.errors.push(record);
                continue;
            }
        };

        state.increment_entries(1);
        if file_type.is_dir() && !file_type.is_symlink() {
            dir_children.push(entry.path());
        } else if file_type.is_symlink() {
            add_leaf_to_totals(&mut totals, 0, EntryCounts::symlink());
        } else {
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    let record = state.record_io_error(entry.path(), &error);
                    totals.errors.push(record);
                    continue;
                }
            };
            let used_bytes = allocated_bytes(&metadata);
            if metadata.is_file() && state.should_consider_top_file(used_bytes) {
                state.record_top_file_candidate(FileSummary {
                    path: entry.path(),
                    name: entry.file_name(),
                    used_bytes,
                });
            }
            add_leaf_to_totals(&mut totals, used_bytes, counts_for_metadata(&metadata));
        }
    }

    let dir_totals = scan_unretained_dir_children_fast(dir_children, state)?;
    for child in dir_totals {
        add_dir_totals_to_totals(&mut totals, child);
    }
    Ok(totals)
}

fn scan_unretained_dir_fast_bulk_into(
    totals: &mut DirTotals,
    path: &Path,
    state: &ScanState,
) -> Result<bool> {
    if !state.options.include_files_in_output {
        return scan_unretained_dir_fast_bulk_aggregate_into(totals, path, state);
    }

    let entries = match bulk::read_dir_fast(path) {
        Ok(Some(entries)) => entries,
        Ok(None) => return Ok(false),
        Err(error) => {
            let record = state.record_io_error(path.to_path_buf(), &error);
            totals.errors.push(record);
            return Ok(true);
        }
    };

    let mut dir_children = Vec::new();
    for entry in entries {
        state.check_cancelled()?;
        if let Some(error) = entry.error {
            let record = state.record_io_error(entry.path, &error);
            totals.errors.push(record);
            continue;
        }

        state.increment_entries(1);
        match entry.kind {
            BulkEntryKind::Dir => dir_children.push(entry.path),
            BulkEntryKind::Symlink => add_leaf_to_totals(totals, 0, EntryCounts::symlink()),
            BulkEntryKind::File | BulkEntryKind::Other => {
                let is_file = entry.kind == BulkEntryKind::File;
                if is_file && state.should_consider_top_file(entry.used_bytes) {
                    state.record_top_file_candidate(FileSummary {
                        path: entry.path,
                        name: entry.name,
                        used_bytes: entry.used_bytes,
                    });
                }
                add_leaf_to_totals(totals, entry.used_bytes, counts_for_bulk_kind(entry.kind));
            }
        }
    }

    let dir_totals = scan_unretained_dir_children_fast(dir_children, state)?;
    for child in dir_totals {
        add_dir_totals_to_totals(totals, child);
    }
    Ok(true)
}

fn scan_unretained_dir_fast_bulk_aggregate_into(
    totals: &mut DirTotals,
    path: &Path,
    state: &ScanState,
) -> Result<bool> {
    let aggregate = match bulk::read_dir_fast_aggregate(path) {
        Ok(Some(aggregate)) => aggregate,
        Ok(None) => return Ok(false),
        Err(error) => {
            let record = state.record_io_error(path.to_path_buf(), &error);
            totals.errors.push(record);
            return Ok(true);
        }
    };

    for error in aggregate.errors {
        let record = state.record_io_error(error.path, &error.error);
        totals.errors.push(record);
    }
    state.increment_entries(aggregate.entries_seen);
    totals.used_bytes = totals.used_bytes.saturating_add(aggregate.used_bytes);
    totals.file_count = totals.file_count.saturating_add(aggregate.file_count);
    totals.counts.saturating_add(aggregate.counts);

    let dir_totals = scan_unretained_dir_children_fast(aggregate.dir_children, state)?;
    for child in dir_totals {
        add_dir_totals_to_totals(totals, child);
    }
    Ok(true)
}

fn scan_unretained_dir_children_fast(
    dir_children: Vec<PathBuf>,
    state: &ScanState,
) -> Result<Vec<DirTotals>> {
    if state.worker_count() <= 1 || dir_children.len() <= 1 {
        return dir_children
            .into_iter()
            .map(|child_path| scan_unretained_dir_fast(&child_path, state))
            .collect();
    }

    scanner_thread_pool(state.worker_count())?.install(|| {
        dir_children
            .par_iter()
            .map(|child_path| scan_unretained_dir_fast(child_path, state))
            .collect()
    })
}

fn add_child_to_summary(summary: &mut DirSummary, child: EntrySummary, retain_child: bool) {
    summary.used_bytes = summary.used_bytes.saturating_add(child.used_bytes());
    summary.file_count = summary.file_count.saturating_add(child.file_count());
    summary.dir_count = summary.dir_count.saturating_add(child.dir_count());
    summary.counts.saturating_add(child.counts());
    if let Some(child_dir) = child
        .as_dir()
        .filter(|child_dir| !child_dir.errors.is_empty())
    {
        summary.errors.extend(child_dir.errors.iter().cloned());
    }
    if retain_child {
        summary.children.push(child);
    }
}

fn add_dir_totals_to_summary(summary: &mut DirSummary, child: DirTotals) {
    summary.used_bytes = summary.used_bytes.saturating_add(child.used_bytes);
    summary.file_count = summary.file_count.saturating_add(child.file_count);
    summary.dir_count = summary.dir_count.saturating_add(child.dir_count);
    summary.counts.saturating_add(child.counts);
    summary.errors.extend(child.errors);
}

fn add_dir_totals_to_totals(totals: &mut DirTotals, child: DirTotals) {
    totals.used_bytes = totals.used_bytes.saturating_add(child.used_bytes);
    totals.file_count = totals.file_count.saturating_add(child.file_count);
    totals.dir_count = totals.dir_count.saturating_add(child.dir_count);
    totals.counts.saturating_add(child.counts);
    totals.errors.extend(child.errors);
}

fn scan_leaf(
    path: &Path,
    name: OsString,
    metadata: &Metadata,
    state: &ScanState,
) -> Result<EntrySummary> {
    state.check_cancelled()?;
    Ok(scan_leaf_after_cancel_check(path, name, metadata, state))
}

fn scan_leaf_after_cancel_check(
    path: &Path,
    name: OsString,
    metadata: &Metadata,
    state: &ScanState,
) -> EntrySummary {
    state.increment_files(1);
    let summary = FileSummary {
        path: path.to_path_buf(),
        name,
        used_bytes: state.leaf_used_bytes(metadata),
    };
    state.record_top_file(&summary, metadata);

    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        EntrySummary::Symlink(summary)
    } else if metadata.is_file() {
        EntrySummary::File(summary)
    } else {
        EntrySummary::Other(summary)
    }
}

fn scan_unretained_leaf_after_cancel_check(
    metadata: &Metadata,
    state: &ScanState,
) -> (u64, EntryKind) {
    state.increment_files(1);
    (state.leaf_used_bytes(metadata), kind_for_metadata(metadata))
}

fn add_leaf_to_summary(summary: &mut DirSummary, used_bytes: u64, counts: EntryCounts) {
    summary.used_bytes = summary.used_bytes.saturating_add(used_bytes);
    summary.file_count = summary.file_count.saturating_add(1);
    summary.counts.saturating_add(counts);
}

fn add_leaf_to_totals(totals: &mut DirTotals, used_bytes: u64, counts: EntryCounts) {
    totals.used_bytes = totals.used_bytes.saturating_add(used_bytes);
    totals.file_count = totals.file_count.saturating_add(1);
    totals.counts.saturating_add(counts);
}

fn pseudo_root_for_leaf(path: &Path, own_bytes: u64, entry: EntrySummary) -> DirSummary {
    DirSummary {
        path: path.to_path_buf(),
        name: display_name(path),
        used_bytes: entry.used_bytes(),
        own_bytes,
        file_count: 1,
        dir_count: 0,
        counts: entry.counts(),
        errors: Vec::new(),
        children: vec![entry],
    }
}

fn kind_for_metadata(metadata: &Metadata) -> EntryKind {
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        EntryKind::Symlink
    } else if metadata.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    }
}

fn counts_for_metadata(metadata: &Metadata) -> EntryCounts {
    counts_for_kind(kind_for_metadata(metadata))
}

fn counts_for_kind(kind: EntryKind) -> EntryCounts {
    match kind {
        EntryKind::Dir => EntryCounts::directory(),
        EntryKind::File => EntryCounts::regular_file(),
        EntryKind::Symlink => EntryCounts::symlink(),
        EntryKind::Other => EntryCounts::other(),
    }
}

fn counts_for_bulk_kind(kind: BulkEntryKind) -> EntryCounts {
    match kind {
        BulkEntryKind::Dir => EntryCounts::directory(),
        BulkEntryKind::File => EntryCounts::regular_file(),
        BulkEntryKind::Symlink => EntryCounts::symlink(),
        BulkEntryKind::Other => EntryCounts::other(),
    }
}

fn display_name(path: &Path) -> OsString {
    path.file_name()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.as_os_str().to_owned())
}
