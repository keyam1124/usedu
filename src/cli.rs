use crate::output::json::render_json;
use crate::output::{render_report, ReportOptions};
use crate::protocol::{
    diff_snapshots, json_v2_schema, render_json_v2, render_ndjson, EnvelopeMode, EnvelopeOptions,
    ScanEnvelope,
};
use crate::scanner::{scan_recursive, ScanOptions, ScanProgress, SortKey};
use crate::util::path::display_path;
use crate::{mcp, tui};
use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
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
    #[command(about = "Print machine-readable schemas")]
    Schema(SchemaArgs),
    #[command(about = "Write a versioned scan snapshot to stdout")]
    Snapshot(SnapshotArgs),
    #[command(about = "Compare two usedu snapshot JSON files")]
    Compare(CompareArgs),
    #[command(about = "Run a foreground MCP stdio server")]
    Mcp(McpArgs),
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

    #[arg(
        long = "format",
        value_enum,
        default_value = "text",
        help = "Output format"
    )]
    pub format: ReportFormat,

    #[arg(long = "errors", help = "Show error details")]
    pub errors: bool,

    #[arg(
        long = "redact-paths",
        help = "Redact displayName and displayPath in machine-readable output"
    )]
    pub redact_paths: bool,

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

#[derive(Debug, Args)]
pub struct SchemaArgs {
    #[arg(value_enum, help = "Schema to print")]
    pub schema: SchemaKind,
}

#[derive(Debug, Args)]
pub struct SnapshotArgs {
    #[arg(value_name = "PATH", default_value = ".", help = "Path to scan")]
    pub path: PathBuf,

    #[arg(
        long = "fast",
        help = "Use faster approximate scanning; may over-count hard links and mounted filesystems"
    )]
    pub fast: bool,

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

    #[arg(
        long = "redact-paths",
        help = "Redact displayName and displayPath in the snapshot"
    )]
    pub redact_paths: bool,
}

#[derive(Debug, Args)]
pub struct CompareArgs {
    #[arg(value_name = "BEFORE")]
    pub before: PathBuf,

    #[arg(value_name = "AFTER")]
    pub after: PathBuf,
}

#[derive(Debug, Args)]
pub struct McpArgs {
    #[arg(long = "stdio", help = "Run MCP over stdin/stdout")]
    pub stdio: bool,

    #[arg(
        long = "allow-root",
        value_name = "PATH",
        help = "Allow MCP scans under this root; defaults to the current directory"
    )]
    pub allow_roots: Vec<PathBuf>,

    #[arg(
        long = "max-sessions",
        default_value_t = 8,
        help = "Maximum stored MCP scan sessions"
    )]
    pub max_sessions: usize,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ReportFormat {
    Text,
    JsonV1,
    JsonV2,
    Ndjson,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SchemaKind {
    JsonV2,
}

pub fn run(cli: Cli) -> Result<()> {
    let cross_file_systems = cli.cross_file_systems;
    let fast = cli.fast;
    let jobs = cli.jobs;

    match cli.command {
        Some(Command::Report(args)) => {
            run_report(merge_report_args(args, cross_file_systems, fast, jobs))
        }
        Some(Command::Schema(args)) => run_schema(args),
        Some(Command::Snapshot(args)) => {
            run_snapshot(merge_snapshot_args(args, cross_file_systems, fast, jobs))
        }
        Some(Command::Compare(args)) => run_compare(args),
        Some(Command::Mcp(args)) => run_mcp(args),
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
    let format = report_format(&args);
    if args.json && !matches!(args.format, ReportFormat::Text | ReportFormat::JsonV1) {
        bail!("--json cannot be combined with --format other than json-v1");
    }
    let progress_state = if args.no_progress || format != ReportFormat::Text {
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

    let report_options = ReportOptions {
        depth: args.depth,
        top: args.top,
        include_files: args.files && !args.summarize,
        summarize: args.summarize,
        dirs_only: args.dirs_only,
        sort_key: args.sort.into(),
        show_errors: args.errors,
    };
    match format {
        ReportFormat::Text => println!("{}", render_report(&scan, &report_options)),
        ReportFormat::JsonV1 => println!("{}", render_json(&scan, args.errors)?),
        ReportFormat::JsonV2 => println!(
            "{}",
            render_json_v2(
                &scan,
                &envelope_options_for_report(&args, EnvelopeMode::Report)
            )?
        ),
        ReportFormat::Ndjson => println!(
            "{}",
            render_ndjson(
                &scan,
                &envelope_options_for_report(&args, EnvelopeMode::Report)
            )?
        ),
    }

    Ok(())
}

fn run_schema(args: SchemaArgs) -> Result<()> {
    match args.schema {
        SchemaKind::JsonV2 => println!("{}", json_v2_schema()),
    }
    Ok(())
}

fn run_snapshot(args: SnapshotArgs) -> Result<()> {
    let mut scan_options = scan_options_with_jobs(args.jobs);
    scan_options.cross_file_systems = args.cross_file_systems;
    scan_options.include_files_in_output = true;
    scan_options.top_files_limit = usize::MAX;
    scan_options.retained_tree_depth = usize::MAX;
    scan_options.retain_root_children = true;
    scan_options.fast = args.fast;

    let scan = scan_recursive(&args.path, &scan_options)?;
    let options = EnvelopeOptions {
        mode: EnvelopeMode::Snapshot,
        depth: usize::MAX,
        top: 0,
        include_files: true,
        summarize: false,
        dirs_only: false,
        sort_key: SortKey::Used,
        show_errors: true,
        fast: args.fast,
        cross_file_systems: args.cross_file_systems,
        jobs: args.jobs,
        max_output_entries: None,
        redact_paths: args.redact_paths,
    };
    println!("{}", render_json_v2(&scan, &options)?);
    Ok(())
}

fn run_compare(args: CompareArgs) -> Result<()> {
    let before: ScanEnvelope = serde_json::from_str(&fs::read_to_string(args.before)?)?;
    let after: ScanEnvelope = serde_json::from_str(&fs::read_to_string(args.after)?)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&diff_snapshots(&before, &after))?
    );
    Ok(())
}

fn run_mcp(args: McpArgs) -> Result<()> {
    if !args.stdio {
        bail!("MCP currently supports only --stdio");
    }
    mcp::run_stdio(mcp::McpServerConfig {
        allowed_roots: args.allow_roots,
        max_sessions: args.max_sessions,
        ..Default::default()
    })
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

fn merge_snapshot_args(
    mut args: SnapshotArgs,
    cross_file_systems: bool,
    fast: bool,
    jobs: Option<usize>,
) -> SnapshotArgs {
    args.cross_file_systems |= cross_file_systems;
    args.fast |= fast;
    args.jobs = args.jobs.or(jobs);
    args
}

fn report_format(args: &ReportArgs) -> ReportFormat {
    if args.json {
        ReportFormat::JsonV1
    } else {
        args.format
    }
}

fn envelope_options_for_report(args: &ReportArgs, mode: EnvelopeMode) -> EnvelopeOptions {
    EnvelopeOptions {
        mode,
        depth: if args.summarize { 0 } else { args.depth },
        top: if args.summarize { 0 } else { args.top },
        include_files: args.files && !args.summarize,
        summarize: args.summarize,
        dirs_only: args.dirs_only,
        sort_key: args.sort.into(),
        show_errors: args.errors,
        fast: args.fast,
        cross_file_systems: args.cross_file_systems,
        jobs: args.jobs,
        max_output_entries: None,
        redact_paths: args.redact_paths,
    }
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
    use super::{scan_options_with_jobs, Cli, Command, ReportFormat, SchemaKind};
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
    fn report_subcommand_accepts_machine_format() {
        let cli = Cli::parse_from(["usedu", "report", "/tmp", "--format", "json-v2"]);

        let Some(Command::Report(args)) = cli.command else {
            panic!("expected report subcommand");
        };
        assert_eq!(args.format, ReportFormat::JsonV2);
    }

    #[test]
    fn schema_subcommand_accepts_json_v2() {
        let cli = Cli::parse_from(["usedu", "schema", "json-v2"]);

        let Some(Command::Schema(args)) = cli.command else {
            panic!("expected schema subcommand");
        };
        assert!(matches!(args.schema, SchemaKind::JsonV2));
    }

    #[test]
    fn snapshot_subcommand_uses_snapshot_arguments() {
        let cli = Cli::parse_from(["usedu", "snapshot", "/tmp", "--cross-file-systems"]);

        let Some(Command::Snapshot(args)) = cli.command else {
            panic!("expected snapshot subcommand");
        };
        assert_eq!(args.path, PathBuf::from("/tmp"));
        assert!(args.cross_file_systems);
    }

    #[test]
    fn mcp_subcommand_requires_stdio_arguments() {
        let cli = Cli::parse_from(["usedu", "mcp", "--stdio", "--allow-root", "/tmp"]);

        let Some(Command::Mcp(args)) = cli.command else {
            panic!("expected mcp subcommand");
        };
        assert!(args.stdio);
        assert_eq!(args.allow_roots, vec![PathBuf::from("/tmp")]);
    }

    #[test]
    fn tui_subcommand_is_not_supported() {
        let result = Cli::try_parse_from(["usedu", "tui", "/tmp"]);

        assert!(result.is_err());
    }
}
