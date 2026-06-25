# usedu Scanner Benchmarks

[English](README.md) | [日本語](README.ja.md)

This directory stores the scanner performance baseline used to compare future `usedu` changes.

This repository keeps benchmark artifacts so scanner performance changes can be reviewed over time.
For anything published outside this repository, prefer command-level timing, explicit cache-state notes, exact command versions, and a documented workload.

## Internal Baseline

`benchmarks/baseline.json` and `benchmarks/baseline.md` are internal scanner API regression baselines.
They are not public performance rankings.

The benchmark runner scans a deterministic fixture under `.usedu-bench/scanner-benchmark`.
The fixture is generated before measurement and is ignored by git through the existing `/.usedu-*` rule.
All scenarios perform warmup scans before recording measured runs, then measured runs are collected round-robin across scenarios to reduce drift from short-lived system load.
Measured time is the wall-clock duration of the scanner API call for that scenario.
Do not use broad real paths such as `/` or user directories for this baseline.

## Create or Update the Baseline

```bash
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo run --release --example bench_scanner -- --runs 7 --write-json benchmarks/baseline.json --write-md benchmarks/baseline.md --write-md-ja benchmarks/baseline.ja.md
```

Update the baseline only when an intentional scanner performance change lands, or when the benchmark fixture/schema changes.
Do not update it just to hide an unexplained regression.

## Compare a Future Change

```bash
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo run --release --example bench_scanner -- --runs 7 --compare benchmarks/baseline.json
```

Interpretation:

- Changes under 5% or under 5 ms are treated as measurement noise.
- Slowdowns of 10% or more are marked `needs-review`.
- Comparison fails before timing analysis when benchmark profile, fixture shape, scenario set, entries, used bytes, file count, dir count, or error count differ.
- Compare results on the same machine whenever possible; the baseline is environment-dependent.
- Run comparisons without other builds, tests, or large scans in parallel. Re-run once after a short pause if all scenarios move in the same direction, because these short scenarios are sensitive to CPU state.

## Public-Style Comparison Template

Use command-level `hyperfine` when publishing performance claims outside this repository.
Record the `usedu` commit, worktree state, system, Rust version, target path shape, and every command version.
Prefer warm-cache results on macOS unless you have a documented cache-flush procedure; do not label macOS results as cold-cache just because the command ran first.

```bash
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo build --release

BENCH_TARGET=.usedu-bench/scanner-benchmark
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo run --release --example bench_scanner -- --runs 1 >/dev/null

hyperfine --warmup 5 --ignore-failure \
  "target/release/usedu report ${BENCH_TARGET} --summarize --no-progress" \
  "target/release/usedu report ${BENCH_TARGET} --fast --summarize --no-progress" \
  "<comparison-command-1>" \
  "<comparison-command-2>"
```

If a comparison command is unavailable or has materially different accounting semantics, omit it and say why.
For Linux-only cold-cache comparisons, follow the `hyperfine --prepare 'sync; echo 3 | sudo tee /proc/sys/vm/drop_caches'` style used by public disk-usage benchmarks.

## Scenarios

- `recursive_default`: recursive scan with `ScanOptions::default()`.
- `recursive_fast`: recursive scan with `fast = true`.
- `recursive_fast_summary`: recursive scan with `fast = true` and root child retention disabled, matching `usedu --fast --summarize`.
- `recursive_jobs_1`: recursive scan with `jobs = Some(1)`.
- `recursive_with_files`: recursive scan with top-file tracking enabled.
- `current_level_default`: direct-child scan used by the TUI-style browser.
- `wide_root_default`: recursive scan of `fixture/wide`, where root-level child directories expose parallel scan behavior.
- `wide_root_fast`: recursive scan of `fixture/wide` with `fast = true`.
- `wide_root_jobs_1`: recursive scan of `fixture/wide` with `jobs = Some(1)`.
