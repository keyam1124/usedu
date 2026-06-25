#![cfg(unix)]

use anyhow::{bail, Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use usedu::scanner::{scan_current_level, scan_recursive, ScanOptions};

const BENCHMARK_PROFILE: &str = "scanner-baseline";
const MARKER_FILE: &str = ".usedu-benchmark-fixture";

const WIDE_ROOTS: usize = 4;
const WIDE_DIRS_PER_ROOT: usize = 64;
const WIDE_FILES_PER_DIR: usize = 96;
const WIDE_SUBDIRS_PER_DIR: usize = 6;
const WIDE_FILES_PER_SUBDIR: usize = 24;
const DEEP_DEPTH: usize = 48;
const DEEP_FILES_PER_LEVEL: usize = 12;
const HIDDEN_FILES: usize = 32;
const HARD_LINK_ALIASES: usize = 4;
const SYMLINK_ENTRIES: usize = 3;
const WARMUP_RUNS: usize = 10;
const NOISE_PERCENT_THRESHOLD: f64 = 5.0;
const REVIEW_PERCENT_THRESHOLD: f64 = 10.0;
const NOISE_ABSOLUTE_MS: f64 = 5.0;

#[derive(Debug, Parser)]
#[command(about = "Run deterministic usedu scanner benchmarks")]
struct Args {
    #[arg(long, default_value_t = 11, help = "Measured runs per scenario")]
    runs: usize,

    #[arg(
        long,
        value_name = "PATH",
        default_value = ".usedu-bench/scanner-benchmark",
        help = "Fixture directory to create and scan"
    )]
    fixture: PathBuf,

    #[arg(long = "write-json", value_name = "PATH", help = "Write JSON report")]
    write_json: Option<PathBuf>,

    #[arg(long = "write-md", value_name = "PATH", help = "Write Markdown report")]
    write_md: Option<PathBuf>,

    #[arg(
        long = "write-md-ja",
        value_name = "PATH",
        help = "Write Japanese Markdown report"
    )]
    write_md_ja: Option<PathBuf>,

    #[arg(
        long,
        value_name = "PATH",
        help = "Compare current run against a baseline JSON report"
    )]
    compare: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkReport {
    benchmark_profile: String,
    fixture: FixtureMetadata,
    environment: EnvironmentMetadata,
    scenarios: Vec<ScenarioResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixtureMetadata {
    profile: String,
    path: String,
    wide_roots: usize,
    wide_dirs_per_root: usize,
    wide_files_per_dir: usize,
    wide_subdirs_per_dir: usize,
    wide_files_per_subdir: usize,
    deep_depth: usize,
    deep_files_per_level: usize,
    hidden_files: usize,
    hard_link_entries: usize,
    symlink_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnvironmentMetadata {
    os: String,
    arch: String,
    cpu_brand: String,
    logical_cpus: usize,
    rustc_version: String,
    cargo_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScenarioResult {
    name: String,
    runs_ms: Vec<f64>,
    median_ms: f64,
    min_ms: f64,
    max_ms: f64,
    entries_per_second: u64,
    entries_seen: u64,
    used_bytes: u64,
    file_count: u64,
    dir_count: u64,
    errors_count: u64,
}

#[derive(Debug, Clone, Copy)]
struct Scenario {
    name: &'static str,
    kind: ScenarioKind,
}

#[derive(Debug, Clone, Copy)]
enum ScenarioKind {
    RecursiveDefault,
    RecursiveFast,
    RecursiveFastSummary,
    RecursiveJobsOne,
    RecursiveWithFiles,
    CurrentLevelDefault,
    WideRootDefault,
    WideRootFast,
    WideRootJobsOne,
}

#[derive(Debug, Clone)]
struct RunSample {
    elapsed_ms: f64,
    entries_seen: u64,
    used_bytes: u64,
    file_count: u64,
    dir_count: u64,
    errors_count: u64,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "recursive_default",
        kind: ScenarioKind::RecursiveDefault,
    },
    Scenario {
        name: "recursive_fast",
        kind: ScenarioKind::RecursiveFast,
    },
    Scenario {
        name: "recursive_fast_summary",
        kind: ScenarioKind::RecursiveFastSummary,
    },
    Scenario {
        name: "recursive_jobs_1",
        kind: ScenarioKind::RecursiveJobsOne,
    },
    Scenario {
        name: "recursive_with_files",
        kind: ScenarioKind::RecursiveWithFiles,
    },
    Scenario {
        name: "current_level_default",
        kind: ScenarioKind::CurrentLevelDefault,
    },
    Scenario {
        name: "wide_root_default",
        kind: ScenarioKind::WideRootDefault,
    },
    Scenario {
        name: "wide_root_fast",
        kind: ScenarioKind::WideRootFast,
    },
    Scenario {
        name: "wide_root_jobs_1",
        kind: ScenarioKind::WideRootJobsOne,
    },
];

fn main() -> Result<()> {
    let args = Args::parse();
    if args.runs == 0 {
        bail!("--runs must be greater than 0");
    }

    ensure_fixture(&args.fixture)?;
    let report = run_benchmarks(&args)?;

    print_report_summary(&report);

    if let Some(path) = &args.compare {
        let baseline = read_baseline(path)?;
        print_comparison(path, &baseline, &report)?;
    }

    if let Some(path) = &args.write_json {
        write_text(path, &serde_json::to_string_pretty(&report)?)
            .with_context(|| format!("failed to write JSON report to {}", path.display()))?;
        println!("Wrote JSON report: {}", path.display());
    }

    if let Some(path) = &args.write_md {
        write_text(path, &render_markdown(&report))
            .with_context(|| format!("failed to write Markdown report to {}", path.display()))?;
        println!("Wrote Markdown report: {}", path.display());
    }

    if let Some(path) = &args.write_md_ja {
        write_text(path, &render_markdown_ja(&report)).with_context(|| {
            format!(
                "failed to write Japanese Markdown report to {}",
                path.display()
            )
        })?;
        println!("Wrote Japanese Markdown report: {}", path.display());
    }

    Ok(())
}

fn run_benchmarks(args: &Args) -> Result<BenchmarkReport> {
    for _ in 0..WARMUP_RUNS {
        for scenario in SCENARIOS {
            let _ = run_sample(scenario.kind, &args.fixture)
                .with_context(|| format!("failed to warm up scenario {}", scenario.name))?;
        }
    }

    let mut samples_by_scenario = vec![Vec::with_capacity(args.runs); SCENARIOS.len()];
    for _ in 0..args.runs {
        for (index, scenario) in SCENARIOS.iter().enumerate() {
            let sample = run_sample(scenario.kind, &args.fixture)
                .with_context(|| format!("failed to run scenario {}", scenario.name))?;
            samples_by_scenario[index].push(sample);
        }
    }

    let scenarios = SCENARIOS
        .iter()
        .copied()
        .zip(samples_by_scenario)
        .map(|(scenario, samples)| summarize_scenario(scenario, samples))
        .collect::<Result<Vec<_>>>()?;

    Ok(BenchmarkReport {
        benchmark_profile: BENCHMARK_PROFILE.to_string(),
        fixture: fixture_metadata(&args.fixture),
        environment: environment_metadata(),
        scenarios,
    })
}

fn summarize_scenario(scenario: Scenario, samples: Vec<RunSample>) -> Result<ScenarioResult> {
    let first = samples
        .first()
        .context("scenario produced no samples despite positive run count")?;
    for sample in &samples[1..] {
        if sample.used_bytes != first.used_bytes
            || sample.file_count != first.file_count
            || sample.dir_count != first.dir_count
            || sample.errors_count != first.errors_count
            || sample.entries_seen != first.entries_seen
        {
            bail!(
                "scenario {} produced inconsistent scan totals",
                scenario.name
            );
        }
    }

    let raw_runs_ms: Vec<f64> = samples.iter().map(|sample| sample.elapsed_ms).collect();
    let median_ms = median(&raw_runs_ms);
    let min_ms = raw_runs_ms.iter().copied().fold(f64::INFINITY, f64::min);
    let max_ms = raw_runs_ms.iter().copied().fold(0.0_f64, f64::max);
    let entries_per_second = entries_per_second(first.entries_seen, median_ms);

    Ok(ScenarioResult {
        name: scenario.name.to_string(),
        runs_ms: raw_runs_ms.into_iter().map(round_ms).collect(),
        median_ms: round_ms(median_ms),
        min_ms: round_ms(min_ms),
        max_ms: round_ms(max_ms),
        entries_per_second,
        entries_seen: first.entries_seen,
        used_bytes: first.used_bytes,
        file_count: first.file_count,
        dir_count: first.dir_count,
        errors_count: first.errors_count,
    })
}

fn run_sample(kind: ScenarioKind, fixture: &Path) -> Result<RunSample> {
    match kind {
        ScenarioKind::RecursiveDefault => {
            let started = Instant::now();
            let scan = scan_recursive(fixture, &ScanOptions::default())?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
        ScenarioKind::RecursiveJobsOne => {
            let options = ScanOptions {
                jobs: Some(1),
                ..Default::default()
            };
            let started = Instant::now();
            let scan = scan_recursive(fixture, &options)?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
        ScenarioKind::RecursiveFast => {
            let options = ScanOptions {
                fast: true,
                ..Default::default()
            };
            let started = Instant::now();
            let scan = scan_recursive(fixture, &options)?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
        ScenarioKind::RecursiveFastSummary => {
            let options = ScanOptions {
                fast: true,
                top_files_limit: 0,
                retained_tree_depth: 0,
                retain_root_children: false,
                ..Default::default()
            };
            let started = Instant::now();
            let scan = scan_recursive(fixture, &options)?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
        ScenarioKind::RecursiveWithFiles => {
            let options = ScanOptions {
                include_files_in_output: true,
                ..Default::default()
            };
            let started = Instant::now();
            let scan = scan_recursive(fixture, &options)?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
        ScenarioKind::CurrentLevelDefault => {
            let options = ScanOptions {
                retained_tree_depth: 1,
                ..Default::default()
            };
            let started = Instant::now();
            let scan = scan_current_level(fixture, &options)?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
        ScenarioKind::WideRootDefault => {
            let target = fixture.join("wide");
            let started = Instant::now();
            let scan = scan_recursive(&target, &ScanOptions::default())?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
        ScenarioKind::WideRootFast => {
            let target = fixture.join("wide");
            let options = ScanOptions {
                fast: true,
                ..Default::default()
            };
            let started = Instant::now();
            let scan = scan_recursive(&target, &options)?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
        ScenarioKind::WideRootJobsOne => {
            let target = fixture.join("wide");
            let options = ScanOptions {
                jobs: Some(1),
                ..Default::default()
            };
            let started = Instant::now();
            let scan = scan_recursive(&target, &options)?;
            Ok(RunSample {
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                entries_seen: scan.metrics.entries_seen,
                used_bytes: scan.root.used_bytes,
                file_count: scan.root.file_count,
                dir_count: scan.root.dir_count,
                errors_count: scan.metrics.errors_seen,
            })
        }
    }
}

fn ensure_fixture(path: &Path) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!(
                "fixture path exists but is not a directory: {}",
                path.display()
            );
        }

        let marker = path.join(MARKER_FILE);
        if marker.exists() {
            let marker_version = fs::read_to_string(&marker)
                .with_context(|| format!("failed to read {}", marker.display()))?;
            if marker_version.trim() == fixture_signature() {
                return Ok(());
            }
            fs::remove_dir_all(path)
                .with_context(|| format!("failed to remove stale fixture {}", path.display()))?;
        } else if path
            .read_dir()
            .with_context(|| format!("failed to read {}", path.display()))?
            .next()
            .is_some()
        {
            bail!(
                "fixture directory exists without {} marker: {}",
                MARKER_FILE,
                path.display()
            );
        }
    }

    create_fixture(path)
}

fn create_fixture(root: &Path) -> Result<()> {
    fs::create_dir_all(root).with_context(|| format!("failed to create {}", root.display()))?;
    create_wide_tree(root)?;
    create_deep_tree(root)?;
    create_hidden_files(root)?;
    create_hard_links(root)?;
    create_symlinks(root)?;
    fs::write(root.join(MARKER_FILE), format!("{}\n", fixture_signature()))
        .with_context(|| format!("failed to write fixture marker in {}", root.display()))?;
    Ok(())
}

fn create_wide_tree(root: &Path) -> Result<()> {
    for root_index in 0..WIDE_ROOTS {
        let wide_root = if root_index == 0 {
            root.join("wide")
        } else {
            root.join(format!("wide-{root_index:02}"))
        };
        create_wide_root(&wide_root, root_index)?;
    }
    Ok(())
}

fn create_wide_root(wide_root: &Path, root_index: usize) -> Result<()> {
    fs::create_dir_all(wide_root)?;
    for dir_index in 0..WIDE_DIRS_PER_ROOT {
        let dir = wide_root.join(format!("dir-{dir_index:02}"));
        fs::create_dir_all(&dir)?;
        for file_index in 0..WIDE_FILES_PER_DIR {
            let bytes = 1024 + ((root_index * 11 + dir_index * 13 + file_index * 17) % 4096);
            write_pattern(
                &dir.join(format!("file-{file_index:03}.bin")),
                bytes,
                (root_index + dir_index + file_index) as u8,
            )?;
        }
        for subdir_index in 0..WIDE_SUBDIRS_PER_DIR {
            let subdir = dir.join(format!("subdir-{subdir_index:02}"));
            fs::create_dir_all(&subdir)?;
            for file_index in 0..WIDE_FILES_PER_SUBDIR {
                let bytes = 512
                    + ((root_index * 17 + dir_index * 19 + subdir_index * 23 + file_index * 29)
                        % 2048);
                write_pattern(
                    &subdir.join(format!("nested-{file_index:03}.bin")),
                    bytes,
                    (root_index + dir_index + subdir_index + file_index) as u8,
                )?;
            }
        }
    }
    Ok(())
}

fn create_deep_tree(root: &Path) -> Result<()> {
    let mut current = root.join("deep");
    fs::create_dir_all(&current)?;
    for depth in 0..DEEP_DEPTH {
        for file_index in 0..DEEP_FILES_PER_LEVEL {
            let bytes = 256 + ((depth * 31 + file_index * 7) % 1536);
            write_pattern(
                &current.join(format!("depth-{depth:02}-file-{file_index:02}.bin")),
                bytes,
                (depth + file_index) as u8,
            )?;
        }
        current = current.join(format!("level-{depth:02}"));
        fs::create_dir_all(&current)?;
    }
    Ok(())
}

fn create_hidden_files(root: &Path) -> Result<()> {
    let hidden_root = root.join("hidden");
    fs::create_dir_all(&hidden_root)?;
    for file_index in 0..HIDDEN_FILES {
        let bytes = 128 + file_index * 11;
        write_pattern(
            &hidden_root.join(format!(".hidden-{file_index:02}.bin")),
            bytes,
            file_index as u8,
        )?;
    }
    Ok(())
}

fn create_hard_links(root: &Path) -> Result<()> {
    let hard_link_root = root.join("hard-links");
    fs::create_dir_all(&hard_link_root)?;
    let original = hard_link_root.join("original.bin");
    write_pattern(&original, 8192, 42)?;
    for alias_index in 0..HARD_LINK_ALIASES {
        fs::hard_link(
            &original,
            hard_link_root.join(format!("alias-{alias_index:02}.bin")),
        )
        .with_context(|| format!("failed to create hard link for {}", original.display()))?;
    }
    Ok(())
}

fn create_symlinks(root: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    let symlink_root = root.join("symlinks");
    fs::create_dir_all(&symlink_root)?;
    symlink("../wide/dir-00", symlink_root.join("wide-dir-00-link"))
        .context("failed to create directory symlink")?;
    symlink(
        "../wide/dir-00/file-000.bin",
        symlink_root.join("file-link"),
    )
    .context("failed to create file symlink")?;
    symlink("../missing-target", symlink_root.join("dangling-link"))
        .context("failed to create dangling symlink")?;
    Ok(())
}

fn write_pattern(path: &Path, bytes: usize, seed: u8) -> Result<()> {
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let buffer: Vec<u8> = (0..bytes)
        .map(|index| seed.wrapping_add((index % 251) as u8))
        .collect();
    file.write_all(&buffer)
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn fixture_metadata(path: &Path) -> FixtureMetadata {
    FixtureMetadata {
        profile: BENCHMARK_PROFILE.to_string(),
        path: path.display().to_string(),
        wide_roots: WIDE_ROOTS,
        wide_dirs_per_root: WIDE_DIRS_PER_ROOT,
        wide_files_per_dir: WIDE_FILES_PER_DIR,
        wide_subdirs_per_dir: WIDE_SUBDIRS_PER_DIR,
        wide_files_per_subdir: WIDE_FILES_PER_SUBDIR,
        deep_depth: DEEP_DEPTH,
        deep_files_per_level: DEEP_FILES_PER_LEVEL,
        hidden_files: HIDDEN_FILES,
        hard_link_entries: HARD_LINK_ALIASES + 1,
        symlink_entries: SYMLINK_ENTRIES,
    }
}

fn fixture_signature() -> String {
    format!(
        "{BENCHMARK_PROFILE}:wide={WIDE_ROOTS}x{WIDE_DIRS_PER_ROOT}x{WIDE_FILES_PER_DIR}+{WIDE_SUBDIRS_PER_DIR}x{WIDE_FILES_PER_SUBDIR}:deep={DEEP_DEPTH}x{DEEP_FILES_PER_LEVEL}:hidden={HIDDEN_FILES}:hard={HARD_LINK_ALIASES}:symlink={SYMLINK_ENTRIES}"
    )
}

fn environment_metadata() -> EnvironmentMetadata {
    EnvironmentMetadata {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_brand: command_output("sysctl", &["-n", "machdep.cpu.brand_string"]),
        logical_cpus: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        rustc_version: command_output("rustc", &["--version"]),
        cargo_version: command_output("cargo", &["--version"]),
    }
}

fn command_output(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unavailable".to_string())
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let middle = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

fn round_ms(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn entries_per_second(entries_seen: u64, median_ms: f64) -> u64 {
    if median_ms <= f64::EPSILON {
        entries_seen
    } else {
        ((entries_seen as f64) / (median_ms / 1000.0)).round() as u64
    }
}

fn read_baseline(path: &Path) -> Result<BenchmarkReport> {
    let json = fs::read_to_string(path)
        .with_context(|| format!("failed to read baseline {}", path.display()))?;
    serde_json::from_str(&json)
        .with_context(|| format!("failed to parse baseline {}", path.display()))
}

fn write_text(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, content)?;
    Ok(())
}

fn print_report_summary(report: &BenchmarkReport) {
    println!("usedu scanner benchmark ({})", report.benchmark_profile);
    println!(
        "{:<24} {:>10} {:>10} {:>10} {:>12} {:>9} {:>8} {:>8} {:>8}",
        "scenario", "median", "min", "max", "entries/s", "entries", "files", "dirs", "errors"
    );
    for scenario in &report.scenarios {
        println!(
            "{:<24} {:>9.3}ms {:>9.3}ms {:>9.3}ms {:>12} {:>9} {:>8} {:>8} {:>8}",
            scenario.name,
            scenario.median_ms,
            scenario.min_ms,
            scenario.max_ms,
            scenario.entries_per_second,
            scenario.entries_seen,
            scenario.file_count,
            scenario.dir_count,
            scenario.errors_count
        );
    }
}

fn print_comparison(
    path: &Path,
    baseline: &BenchmarkReport,
    current: &BenchmarkReport,
) -> Result<()> {
    validate_comparable_baseline(path, baseline, current)?;

    println!();
    println!("Comparison against {}:", path.display());
    println!(
        "{:<24} {:>10} {:>10} {:>9} {:<12}",
        "scenario", "baseline", "current", "change", "status"
    );

    let baseline_by_name: BTreeMap<&str, &ScenarioResult> = baseline
        .scenarios
        .iter()
        .map(|scenario| (scenario.name.as_str(), scenario))
        .collect();

    for scenario in &current.scenarios {
        let previous = baseline_by_name
            .get(scenario.name.as_str())
            .with_context(|| format!("baseline is missing scenario {}", scenario.name))?;
        let delta = percent_change(previous.median_ms, scenario.median_ms);
        let delta_ms = scenario.median_ms - previous.median_ms;
        println!(
            "{:<24} {:>9.3}ms {:>9.3}ms {:>8.2}% {:<12}",
            scenario.name,
            previous.median_ms,
            scenario.median_ms,
            delta,
            comparison_status(delta, delta_ms)
        );
    }
    Ok(())
}

fn validate_comparable_baseline(
    path: &Path,
    baseline: &BenchmarkReport,
    current: &BenchmarkReport,
) -> Result<()> {
    if baseline.benchmark_profile != current.benchmark_profile {
        bail!(
            "baseline {} uses benchmark_profile {}, but current run uses {}",
            path.display(),
            baseline.benchmark_profile,
            current.benchmark_profile
        );
    }
    if !fixture_shapes_match(&baseline.fixture, &current.fixture) {
        bail!(
            "baseline {} fixture shape does not match current fixture shape",
            path.display()
        );
    }

    let current_by_name: BTreeMap<&str, &ScenarioResult> = current
        .scenarios
        .iter()
        .map(|scenario| (scenario.name.as_str(), scenario))
        .collect();
    for previous in &baseline.scenarios {
        if !current_by_name.contains_key(previous.name.as_str()) {
            bail!("current run is missing baseline scenario {}", previous.name);
        }
    }

    let baseline_by_name: BTreeMap<&str, &ScenarioResult> = baseline
        .scenarios
        .iter()
        .map(|scenario| (scenario.name.as_str(), scenario))
        .collect();
    for scenario in &current.scenarios {
        let previous = baseline_by_name
            .get(scenario.name.as_str())
            .with_context(|| format!("baseline is missing scenario {}", scenario.name))?;
        validate_same_work(previous, scenario)?;
    }

    Ok(())
}

fn fixture_shapes_match(left: &FixtureMetadata, right: &FixtureMetadata) -> bool {
    left.profile == right.profile
        && left.wide_roots == right.wide_roots
        && left.wide_dirs_per_root == right.wide_dirs_per_root
        && left.wide_files_per_dir == right.wide_files_per_dir
        && left.wide_subdirs_per_dir == right.wide_subdirs_per_dir
        && left.wide_files_per_subdir == right.wide_files_per_subdir
        && left.deep_depth == right.deep_depth
        && left.deep_files_per_level == right.deep_files_per_level
        && left.hidden_files == right.hidden_files
        && left.hard_link_entries == right.hard_link_entries
        && left.symlink_entries == right.symlink_entries
}

fn validate_same_work(baseline: &ScenarioResult, current: &ScenarioResult) -> Result<()> {
    let mismatches = [
        ("entries_seen", baseline.entries_seen, current.entries_seen),
        ("used_bytes", baseline.used_bytes, current.used_bytes),
        ("file_count", baseline.file_count, current.file_count),
        ("dir_count", baseline.dir_count, current.dir_count),
        ("errors_count", baseline.errors_count, current.errors_count),
    ]
    .into_iter()
    .filter(|&(_name, expected, actual)| expected != actual)
    .map(|(name, expected, actual)| format!("{name}: baseline={expected}, current={actual}"))
    .collect::<Vec<_>>();

    if !mismatches.is_empty() {
        bail!(
            "scenario {} did different work than the baseline ({})",
            current.name,
            mismatches.join("; ")
        );
    }
    Ok(())
}

fn percent_change(baseline_ms: f64, current_ms: f64) -> f64 {
    if baseline_ms <= f64::EPSILON {
        0.0
    } else {
        ((current_ms - baseline_ms) / baseline_ms) * 100.0
    }
}

fn comparison_status(delta_percent: f64, delta_ms: f64) -> &'static str {
    if delta_ms.abs() < NOISE_ABSOLUTE_MS || delta_percent.abs() < NOISE_PERCENT_THRESHOLD {
        "noise"
    } else if delta_percent >= REVIEW_PERCENT_THRESHOLD {
        "needs-review"
    } else if delta_percent > 0.0 {
        "slower"
    } else {
        "faster"
    }
}

fn render_markdown(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str("# usedu Scanner Benchmark Baseline\n\n");
    out.push_str("[English](baseline.md) | [日本語](baseline.ja.md)\n\n");
    out.push_str("This baseline tracks the scanner API against a deterministic local fixture. It is an internal regression guard for `usedu`, not a public performance ranking.\n\n");
    out.push_str("Fixture generation and warmup scans are excluded from measured runs. See [benchmarks/README.md](README.md) for the public-style benchmark policy and optional command-level comparison template.\n\n");
    out.push_str("## Scope\n\n");
    out.push_str("- Benchmark style: internal scanner API regression baseline\n");
    out.push_str("- Measured operation: scanner API call only\n");
    out.push_str("- Public claims: use command-level `hyperfine` runs with command versions and warm/cache notes\n\n");
    out.push_str("## Environment\n\n");
    out.push_str(&format!(
        "- Benchmark profile: `{}`\n",
        report.benchmark_profile
    ));
    out.push_str(&format!("- Fixture: `{}`\n", report.fixture.path));
    out.push_str(&format!(
        "- System: `{}` `{}` on `{}` with `{}` logical CPUs\n",
        report.environment.os,
        report.environment.arch,
        report.environment.cpu_brand,
        report.environment.logical_cpus
    ));
    out.push_str(&format!("- Rust: `{}`\n", report.environment.rustc_version));
    out.push_str(&format!(
        "- Cargo: `{}`\n\n",
        report.environment.cargo_version
    ));

    out.push_str("## Workload\n\n");
    out.push_str(&format!(
        "- Wide trees: `{}` roots x `{}` dirs/root x (`{}` files + `{}` subdirs x `{}` files)\n",
        report.fixture.wide_roots,
        report.fixture.wide_dirs_per_root,
        report.fixture.wide_files_per_dir,
        report.fixture.wide_subdirs_per_dir,
        report.fixture.wide_files_per_subdir
    ));
    out.push_str(&format!(
        "- Deep tree: `{}` levels x `{}` files/level\n",
        report.fixture.deep_depth, report.fixture.deep_files_per_level
    ));
    out.push_str(&format!(
        "- Edge entries: `{}` hidden files, `{}` hard-link entries, `{}` symlink entries\n\n",
        report.fixture.hidden_files,
        report.fixture.hard_link_entries,
        report.fixture.symlink_entries
    ));

    out.push_str("## Results\n\n");
    out.push_str("| Scenario | Runs | Median ms | Min ms | Max ms | Entries/s | Entries | Used bytes | Files | Dirs | Errors |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for scenario in &report.scenarios {
        out.push_str(&format!(
            "| `{}` | {} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {} | {} | {} |\n",
            scenario.name,
            scenario.runs_ms.len(),
            scenario.median_ms,
            scenario.min_ms,
            scenario.max_ms,
            scenario.entries_per_second,
            scenario.entries_seen,
            scenario.used_bytes,
            scenario.file_count,
            scenario.dir_count,
            scenario.errors_count
        ));
    }
    out
}

fn render_markdown_ja(report: &BenchmarkReport) -> String {
    let mut out = String::new();
    out.push_str("# usedu スキャナーベンチマークベースライン\n\n");
    out.push_str("[English](baseline.md) | [日本語](baseline.ja.md)\n\n");
    out.push_str(
        "このベースラインは、毎回同じ内容で生成するローカル fixture に対してスキャナー API を測定します。\n",
    );
    out.push_str(
        "`usedu` の内部回帰検知に使うものであり、公開向けに性能を順位付けする資料ではありません。\n\n",
    );
    out.push_str("fixture 生成とウォームアップ走査は、測定対象から除外します。\n");
    out.push_str("公開向けベンチマークの方針と任意のコマンド単位比較テンプレートは [benchmarks/README.ja.md](README.ja.md) にまとめています。\n\n");
    out.push_str("## 位置づけ\n\n");
    out.push_str("- ベンチマーク種別：内部向けスキャナー API 回帰ベースライン\n");
    out.push_str("- 測定対象：スキャナー API 呼び出しのみ\n");
    out.push_str("- 公開向けの性能説明：コマンド単位の `hyperfine` 実行、コマンドバージョン、キャッシュ状態の注記を使う\n\n");
    out.push_str("## 実行環境\n\n");
    out.push_str(&format!(
        "- Benchmark profile: `{}`\n",
        report.benchmark_profile
    ));
    out.push_str(&format!("- Fixture: `{}`\n", report.fixture.path));
    out.push_str(&format!(
        "- System: `{}` `{}` on `{}` with `{}` logical CPUs\n",
        report.environment.os,
        report.environment.arch,
        report.environment.cpu_brand,
        report.environment.logical_cpus
    ));
    out.push_str(&format!("- Rust: `{}`\n", report.environment.rustc_version));
    out.push_str(&format!(
        "- Cargo: `{}`\n\n",
        report.environment.cargo_version
    ));

    out.push_str("## ワークロード\n\n");
    out.push_str(&format!(
        "- Wide trees: `{}` roots x `{}` dirs/root x (`{}` files + `{}` subdirs x `{}` files)\n",
        report.fixture.wide_roots,
        report.fixture.wide_dirs_per_root,
        report.fixture.wide_files_per_dir,
        report.fixture.wide_subdirs_per_dir,
        report.fixture.wide_files_per_subdir
    ));
    out.push_str(&format!(
        "- Deep tree: `{}` levels x `{}` files/level\n",
        report.fixture.deep_depth, report.fixture.deep_files_per_level
    ));
    out.push_str(&format!(
        "- Edge entries: hidden files `{}`, hard-link entries `{}`, symlink entries `{}`\n\n",
        report.fixture.hidden_files,
        report.fixture.hard_link_entries,
        report.fixture.symlink_entries
    ));

    out.push_str("## 結果\n\n");
    out.push_str("| シナリオ | 実行回数 | 中央値 ms | 最小 ms | 最大 ms | Entries/s | Entries | Used bytes | Files | Dirs | Errors |\n");
    out.push_str("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |\n");
    for scenario in &report.scenarios {
        out.push_str(&format!(
            "| `{}` | {} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {} | {} | {} |\n",
            scenario.name,
            scenario.runs_ms.len(),
            scenario.median_ms,
            scenario.min_ms,
            scenario.max_ms,
            scenario.entries_per_second,
            scenario.entries_seen,
            scenario.used_bytes,
            scenario.file_count,
            scenario.dir_count,
            scenario.errors_count
        ));
    }
    out
}
