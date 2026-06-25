use crate::output::json::render_json;
use crate::output::{render_report, ReportOptions};
use crate::scanner::{scan_recursive, ScanOptions, ScanProgress, SortKey};
use crate::tui;
use crate::util::path::display_path;
use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const LONG_ABOUT: &str = "Run usedu [PATH] to open the interactive TUI browser. Use usedu report [PATH] for a static report.\n\nusedu reports allocated file-system size. APFS clones, snapshots, compression, sparse files, and file-provider behavior can make reclaimable space differ from the displayed Used value.";

#[derive(Debug, Parser)]
#[command(name = "usedu")]
#[command(about = "Read-only macOS disk usage analyzer")]
#[command(long_about = LONG_ABOUT)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(value_name = "PATH", default_value = ".", help = "Directory to browse")]
    pub path: PathBuf,

    #[arg(
        long = "cross-file-systems",
        help = "Allow scanning across mounted filesystems"
    )]
    pub cross_file_systems: bool,

    #[arg(
        long = "fast",
        help = "Use faster approximate scanning; may over-count hard links and mounted filesystems"
    )]
    pub fast: bool,

    #[arg(
        long = "jobs",
        help = "Worker count for parallel scans; defaults to an I/O-optimized value"
    )]
    pub jobs: Option<usize>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Print a static disk usage report")]
    Report(ReportArgs),
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    #[arg(value_name = "PATH", default_value = ".", help = "Path to scan")]
    pub path: PathBuf,

    #[arg(
        short = 'd',
        long = "depth",
        default_value_t = 2,
        help = "Display tree depth"
    )]
    pub depth: usize,

    #[arg(
        short = 'n',
        long = "top",
        default_value_t = 30,
        help = "Show top N entries"
    )]
    pub top: usize,

    #[arg(long = "files", help = "Include top large files section")]
    pub files: bool,

    #[arg(long = "summarize", help = "Show only the total summary")]
    pub summarize: bool,

    #[arg(
        long = "fast",
        help = "Use faster approximate scanning; may over-count hard links and mounted filesystems"
    )]
    pub fast: bool,

    #[arg(long = "dirs-only", help = "Only show directories in ranking")]
    pub dirs_only: bool,

    #[arg(long = "sort", value_enum, default_value = "used", help = "Sort key")]
    pub sort: ReportSort,

    #[arg(long = "json", help = "Output JSON instead of rich text")]
    pub json: bool,

    #[arg(long = "errors", help = "Show error details")]
    pub errors: bool,

    #[arg(long = "no-progress", help = "Disable progress indicator")]
    pub no_progress: bool,

    #[arg(
        long = "cross-file-systems",
        help = "Allow scanning across mounted filesystems"
    )]
    pub cross_file_systems: bool,

    #[arg(
        long = "jobs",
        help = "Worker count for parallel scans; defaults to an I/O-optimized value"
    )]
    pub jobs: Option<usize>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ReportSort {
    Used,
    Files,
    Dirs,
}

impl From<ReportSort> for SortKey {
    fn from(value: ReportSort) -> Self {
        match value {
            ReportSort::Used => SortKey::Used,
            ReportSort::Files => SortKey::Files,
            ReportSort::Dirs => SortKey::Dirs,
        }
    }
}

pub fn run(cli: Cli) -> Result<()> {
    let cross_file_systems = cli.cross_file_systems;
    let fast = cli.fast;
    let jobs = cli.jobs;

    match cli.command {
        Some(Command::Report(args)) => {
            run_report(merge_report_args(args, cross_file_systems, fast, jobs))
        }
        None => run_tui(cli.path, cross_file_systems, fast, jobs),
    }
}

fn run_tui(path: PathBuf, cross_file_systems: bool, fast: bool, jobs: Option<usize>) -> Result<()> {
    let mut options = scan_options_with_jobs(jobs);
    options.cross_file_systems = cross_file_systems;
    options.include_files_in_output = false;
    options.fast = fast;
    tui::run(path, options)
}

fn run_report(args: ReportArgs) -> Result<()> {
    let progress_state = if args.no_progress || args.json {
        None
    } else {
        Some(ScanProgress::new())
    };
    let mut scan_options = scan_options_with_jobs(args.jobs);
    scan_options.cross_file_systems = args.cross_file_systems;
    scan_options.include_files_in_output = args.files && !args.summarize;
    scan_options.top_files_limit = if args.summarize { 0 } else { args.top };
    scan_options.retained_tree_depth = if args.summarize { 0 } else { args.depth };
    scan_options.retain_root_children = !args.summarize;
    scan_options.fast = args.fast;
    scan_options.progress = progress_state.clone();
    let progress = progress_state.map(|progress| start_progress(&args.path, progress));

    let scan = scan_recursive(&args.path, &scan_options);
    if let Some(progress) = progress {
        progress.finish_and_clear();
    }
    let scan = scan?;

    if args.json {
        println!("{}", render_json(&scan, args.errors)?);
    } else {
        let report_options = ReportOptions {
            depth: args.depth,
            top: args.top,
            include_files: args.files && !args.summarize,
            summarize: args.summarize,
            dirs_only: args.dirs_only,
            sort_key: args.sort.into(),
            show_errors: args.errors,
        };
        println!("{}", render_report(&scan, &report_options));
    }

    Ok(())
}

fn merge_report_args(
    mut args: ReportArgs,
    cross_file_systems: bool,
    fast: bool,
    jobs: Option<usize>,
) -> ReportArgs {
    args.cross_file_systems |= cross_file_systems;
    args.fast |= fast;
    args.jobs = args.jobs.or(jobs);
    args
}

fn scan_options_with_jobs(jobs: Option<usize>) -> ScanOptions {
    let mut options = ScanOptions::default();
    if let Some(jobs) = jobs {
        options.jobs = Some(jobs);
    }
    options
}

struct ActiveProgress {
    bar: ProgressBar,
    handle: thread::JoinHandle<()>,
}

impl ActiveProgress {
    fn finish_and_clear(self) {
        let _ = self.handle.join();
        self.bar.finish_and_clear();
    }
}

fn start_progress(path: &Path, progress: ScanProgress) -> ActiveProgress {
    let bar = ProgressBar::new_spinner();
    bar.enable_steady_tick(Duration::from_millis(120));
    if let Ok(style) = ProgressStyle::with_template("{spinner:.cyan} {msg}") {
        bar.set_style(style);
    }
    let target = display_path(path);
    let worker_bar = bar.clone();
    let handle = thread::spawn(move || loop {
        let snapshot = progress.snapshot();
        worker_bar.set_message(format!(
            "Scanning {} | Entries: {} | Errors: {} | Elapsed: {}",
            target,
            crate::util::units::format_count(snapshot.entries_seen),
            crate::util::units::format_count(snapshot.errors_seen),
            crate::util::timing::format_duration(snapshot.elapsed)
        ));
        if snapshot.done {
            break;
        }
        thread::sleep(Duration::from_millis(120));
    });
    ActiveProgress { bar, handle }
}

#[cfg(test)]
mod tests {
    use super::{scan_options_with_jobs, Cli, Command};
    use crate::scanner::ScanOptions;
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn omitted_jobs_preserve_scanner_default_parallelism() {
        let default_jobs = ScanOptions::default().jobs;

        let options = scan_options_with_jobs(None);

        assert_eq!(options.jobs, default_jobs);
    }

    #[test]
    fn explicit_jobs_override_scanner_default_parallelism() {
        let options = scan_options_with_jobs(Some(1));

        assert_eq!(options.jobs, Some(1));
    }

    #[test]
    fn omitted_subcommand_defaults_to_tui_mode_arguments() {
        let cli = Cli::parse_from(["usedu", "/tmp"]);

        assert!(cli.command.is_none());
        assert_eq!(cli.path, PathBuf::from("/tmp"));
    }

    #[test]
    fn report_subcommand_uses_report_arguments() {
        let cli = Cli::parse_from(["usedu", "report", "/tmp", "--depth", "3"]);

        let Some(Command::Report(args)) = cli.command else {
            panic!("expected report subcommand");
        };
        assert_eq!(args.path, PathBuf::from("/tmp"));
        assert_eq!(args.depth, 3);
    }

    #[test]
    fn tui_subcommand_is_not_supported() {
        let result = Cli::try_parse_from(["usedu", "tui", "/tmp"]);

        assert!(result.is_err());
    }
}
