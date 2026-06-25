use super::progress::{ScanCancellation, ScanProgress};

const DEFAULT_JOBS_PER_LOGICAL_CPU: usize = 8;
const MAX_DEFAULT_JOBS: usize = 80;

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub cross_file_systems: bool,
    pub jobs: Option<usize>,
    pub include_files_in_output: bool,
    pub top_files_limit: usize,
    pub retained_tree_depth: usize,
    pub retain_root_children: bool,
    pub fast: bool,
    pub progress: Option<ScanProgress>,
    pub cancellation: Option<ScanCancellation>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            cross_file_systems: false,
            jobs: default_jobs(),
            include_files_in_output: false,
            top_files_limit: 30,
            retained_tree_depth: 2,
            retain_root_children: true,
            fast: false,
            progress: None,
            cancellation: None,
        }
    }
}

fn default_jobs() -> Option<usize> {
    std::thread::available_parallelism()
        .ok()
        .map(|logical_cpus| {
            usize::from(logical_cpus)
                .saturating_mul(DEFAULT_JOBS_PER_LOGICAL_CPU)
                .clamp(1, MAX_DEFAULT_JOBS)
        })
}
