use super::result::ScanMetrics;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ScanProgress {
    inner: Arc<ScanProgressInner>,
}

#[derive(Debug)]
struct ScanProgressInner {
    started: Mutex<Instant>,
    entries_seen: AtomicU64,
    files_seen: AtomicU64,
    dirs_seen: AtomicU64,
    errors_seen: AtomicU64,
    done: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanProgressSnapshot {
    pub elapsed: Duration,
    pub entries_seen: u64,
    pub files_seen: u64,
    pub dirs_seen: u64,
    pub errors_seen: u64,
    pub done: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ScanCancellation {
    cancelled: Arc<AtomicBool>,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ScanProgressInner {
                started: Mutex::new(Instant::now()),
                entries_seen: AtomicU64::new(0),
                files_seen: AtomicU64::new(0),
                dirs_seen: AtomicU64::new(0),
                errors_seen: AtomicU64::new(0),
                done: AtomicBool::new(false),
            }),
        }
    }

    pub fn reset(&self) {
        if let Ok(mut started) = self.inner.started.lock() {
            *started = Instant::now();
        }
        self.inner.entries_seen.store(0, Ordering::Relaxed);
        self.inner.files_seen.store(0, Ordering::Relaxed);
        self.inner.dirs_seen.store(0, Ordering::Relaxed);
        self.inner.errors_seen.store(0, Ordering::Relaxed);
        self.inner.done.store(false, Ordering::Relaxed);
    }

    pub fn publish(&self, metrics: &ScanMetrics) {
        self.inner
            .entries_seen
            .store(metrics.entries_seen, Ordering::Relaxed);
        self.inner
            .files_seen
            .store(metrics.files_seen, Ordering::Relaxed);
        self.inner
            .dirs_seen
            .store(metrics.dirs_seen, Ordering::Relaxed);
        self.inner
            .errors_seen
            .store(metrics.errors_seen, Ordering::Relaxed);
    }

    pub(crate) fn increment_entries(&self, count: u64) {
        self.inner.entries_seen.fetch_add(count, Ordering::Relaxed);
    }

    pub(crate) fn increment_files(&self, count: u64) {
        self.inner.files_seen.fetch_add(count, Ordering::Relaxed);
    }

    pub(crate) fn increment_dirs(&self, count: u64) {
        self.inner.dirs_seen.fetch_add(count, Ordering::Relaxed);
    }

    pub(crate) fn increment_errors(&self, count: u64) {
        self.inner.errors_seen.fetch_add(count, Ordering::Relaxed);
    }

    pub fn mark_done(&self) {
        self.inner.done.store(true, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> ScanProgressSnapshot {
        let started = self
            .inner
            .started
            .lock()
            .map(|started| *started)
            .unwrap_or_else(|_| Instant::now());
        ScanProgressSnapshot {
            elapsed: started.elapsed(),
            entries_seen: self.inner.entries_seen.load(Ordering::Relaxed),
            files_seen: self.inner.files_seen.load(Ordering::Relaxed),
            dirs_seen: self.inner.dirs_seen.load(Ordering::Relaxed),
            errors_seen: self.inner.errors_seen.load(Ordering::Relaxed),
            done: self.inner.done.load(Ordering::Relaxed),
        }
    }
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanCancellation {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}
