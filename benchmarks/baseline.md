# usedu Scanner Benchmark Baseline

[English](baseline.md) | [日本語](baseline.ja.md)

This baseline tracks the scanner API against a deterministic local fixture. It is an internal regression guard for `usedu`, not a public performance ranking.

Fixture generation and warmup scans are excluded from measured runs. See [benchmarks/README.md](README.md) for the public-style benchmark policy and optional command-level comparison template.

## Scope

- Benchmark style: internal scanner API regression baseline
- Measured operation: scanner API call only
- Public claims: use command-level `hyperfine` runs with command versions and warm/cache notes

## Environment

- Benchmark profile: `scanner-baseline`
- Fixture: `.usedu-bench/scanner-benchmark`
- System: `macos` `aarch64` on `Apple M4` with `10` logical CPUs
- Rust: `rustc 1.96.0 (ac68faa20 2026-05-25)`
- Cargo: `cargo 1.96.0 (30a34c682 2026-05-25)`

## Workload

- Wide trees: `4` roots x `64` dirs/root x (`96` files + `6` subdirs x `24` files)
- Deep tree: `48` levels x `12` files/level
- Edge entries: `32` hidden files, `5` hard-link entries, `3` symlink entries

## Results

| Scenario | Runs | Median ms | Min ms | Max ms | Entries/s | Entries | Used bytes | Files | Dirs | Errors |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `recursive_default` | 7 | 38.494 | 36.509 | 47.504 | 1660148 | 63906 | 254160896 | 62057 | 1849 | 0 |
| `recursive_fast` | 7 | 32.814 | 30.239 | 33.602 | 1947525 | 63906 | 254193664 | 62057 | 1849 | 0 |
| `recursive_fast_summary` | 7 | 33.646 | 28.319 | 34.686 | 1899380 | 63906 | 254193664 | 62057 | 1849 | 0 |
| `recursive_jobs_1` | 7 | 114.970 | 110.287 | 124.451 | 555852 | 63906 | 254160896 | 62057 | 1849 | 0 |
| `recursive_with_files` | 7 | 37.653 | 37.250 | 47.337 | 1697215 | 63906 | 254160896 | 62057 | 1849 | 0 |
| `current_level_default` | 7 | 38.179 | 36.165 | 58.610 | 1673841 | 63906 | 254160896 | 62057 | 1849 | 0 |
| `wide_root_default` | 7 | 11.662 | 11.465 | 13.877 | 1355595 | 15809 | 62914560 | 15360 | 449 | 0 |
| `wide_root_fast` | 7 | 8.774 | 7.811 | 9.060 | 1801809 | 15809 | 62914560 | 15360 | 449 | 0 |
| `wide_root_jobs_1` | 7 | 27.263 | 26.527 | 33.822 | 579878 | 15809 | 62914560 | 15360 | 449 | 0 |
