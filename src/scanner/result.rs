use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Used,
    Name,
    Files,
    Dirs,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            Self::Used => Self::Name,
            Self::Name => Self::Files,
            Self::Files => Self::Dirs,
            Self::Dirs => Self::Used,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Used => "used",
            Self::Name => "name",
            Self::Files => "files",
            Self::Dirs => "dirs",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScanMetrics {
    pub elapsed: Duration,
    pub entries_seen: u64,
    pub files_seen: u64,
    pub dirs_seen: u64,
    pub errors_seen: u64,
}

impl ScanMetrics {
    pub fn entries_per_second(&self) -> u64 {
        let elapsed = self.elapsed.as_secs_f64();
        if elapsed <= f64::EPSILON {
            self.entries_seen
        } else {
            (self.entries_seen as f64 / elapsed).round() as u64
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanErrorRecord {
    pub path: PathBuf,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct DirSummary {
    pub path: PathBuf,
    pub name: OsString,
    pub used_bytes: u64,
    pub own_bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub errors: Vec<ScanErrorRecord>,
    pub children: Vec<EntrySummary>,
}

#[derive(Debug, Clone)]
pub enum EntrySummary {
    Dir(DirSummary),
    File(FileSummary),
    Symlink(FileSummary),
    Other(FileSummary),
}

#[derive(Debug, Clone)]
pub struct FileSummary {
    pub path: PathBuf,
    pub name: OsString,
    pub used_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub root: DirSummary,
    pub metrics: ScanMetrics,
    pub top_files: Vec<FileSummary>,
}

#[derive(Debug, Clone)]
pub struct CurrentLevelScan {
    pub root: DirSummary,
    pub metrics: ScanMetrics,
    pub rows: Vec<EntrySummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

impl EntrySummary {
    pub fn kind(&self) -> EntryKind {
        match self {
            Self::Dir(_) => EntryKind::Dir,
            Self::File(_) => EntryKind::File,
            Self::Symlink(_) => EntryKind::Symlink,
            Self::Other(_) => EntryKind::Other,
        }
    }

    pub fn kind_label(&self) -> &'static str {
        match self.kind() {
            EntryKind::Dir => "dir",
            EntryKind::File => "file",
            EntryKind::Symlink => "symlink",
            EntryKind::Other => "other",
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Dir(_))
    }

    pub fn as_dir(&self) -> Option<&DirSummary> {
        match self {
            Self::Dir(summary) => Some(summary),
            _ => None,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Dir(summary) => &summary.path,
            Self::File(summary) | Self::Symlink(summary) | Self::Other(summary) => &summary.path,
        }
    }

    pub fn name(&self) -> &OsStr {
        match self {
            Self::Dir(summary) => &summary.name,
            Self::File(summary) | Self::Symlink(summary) | Self::Other(summary) => &summary.name,
        }
    }

    pub fn used_bytes(&self) -> u64 {
        match self {
            Self::Dir(summary) => summary.used_bytes,
            Self::File(summary) | Self::Symlink(summary) | Self::Other(summary) => {
                summary.used_bytes
            }
        }
    }

    pub fn file_count(&self) -> u64 {
        match self {
            Self::Dir(summary) => summary.file_count,
            Self::File(_) | Self::Symlink(_) | Self::Other(_) => 1,
        }
    }

    pub fn dir_count(&self) -> u64 {
        match self {
            Self::Dir(summary) => summary.dir_count,
            Self::File(_) | Self::Symlink(_) | Self::Other(_) => 0,
        }
    }
}

impl DirSummary {
    pub fn errors_count(&self) -> usize {
        self.errors.len()
    }
}
