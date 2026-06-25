# Design Notes

[English](design.md) | [日本語](design.ja.md)

This document records the product and implementation constraints that should remain stable as `usedu` evolves.
It is not a historical implementation spec.

Related documents:

- [ADR 0001: Product Contract](adr/0001-product-contract.md)
- [Filesystem Semantics](semantics.md)
- [JSON Contract Repair Plan](json-contract.md)

## Product Boundary

`usedu` is a read-only macOS disk usage analyzer for the terminal.
It helps identify where allocated file-system space is used.

`usedu` must not delete, move, mutate, quarantine, or recommend cleanup actions.
It is also not a GUI, background daemon, duplicate finder, real-time monitor, treemap, or logical-size analyzer.

## Command Model

The default command opens the interactive TUI:

```bash
usedu [PATH]
```

If `PATH` is omitted, the TUI opens the current directory.
`usedu` never defaults to `/`; scanning the root volume requires an explicit path.

Static reports are behind the `report` subcommand:

```bash
usedu report [PATH]
```

## Size Semantics

`usedu` reports allocated file-system size only.
The display label is always `Used`.
There is no logical-size mode and no size-mode switch.

Strict scanning uses `symlink_metadata` and Unix allocated block counts:

```rust
metadata.blocks().saturating_mul(512)
```

Directory totals include the directory's own allocated bytes plus descendant file and directory allocated bytes.

APFS clones, snapshots, compression, sparse files, and file-provider behavior can make reclaimable space differ from the displayed `Used` value.

## Filesystem Rules

Symbolic links are counted as link entries but are not followed.
Hidden files and directories are included.
Package directories such as `.app` and `.photoslibrary` are treated as ordinary directories.

Permission errors are recorded and do not abort the whole scan.
Detailed error output is opt-in.

By default, scanning stays on the root filesystem of the requested path.
Use `--cross-file-systems` to include mounted filesystems on other devices.

Regular files with multiple hard links should be counted once per device and inode where practical.

## TUI Scan Model

The TUI is a one-level browser.
For the current directory, it displays only direct children.
Each direct child directory still shows its recursive `Used` total.

For example, when viewing `~/Library`, the TUI may show `Application Support`, `Containers`, and `Caches`.
It should not show grandchildren such as `Application Support/A` until the user opens `Application Support`.

This model keeps the UI scannable while preserving useful disk-usage totals.

## Scanner Architecture

Scanner code should be independent from terminal output and TUI rendering.
Both report mode and TUI mode should share scanner logic.

The scanner should work with `PathBuf` and `OsString`.
It should not assume paths are valid UTF-8.
Lossy string conversion belongs at the final display or serialization layer.

The scanner should avoid storing every file as a retained tree node.
It should retain directory summaries, report-relevant files, and top-file candidates as needed.

## Performance Principles

Scanning should read metadata only.
It should not read file contents.

Progress output must be throttled.
The scanner should not print one line per file or build display strings during traversal.
Sorting should happen before output or screen rendering, not repeatedly during scanning.

Parallelism should use a bounded worker pool.
Directory subtrees are the useful unit of parallel work; individual file metadata reads are too fine-grained.

## Fast Mode

Fast mode favors lower scan latency over strict accounting.
On macOS, it may use bulk metadata APIs to reduce per-entry filesystem calls.
For unretained subtrees, it can aggregate totals without building display nodes for every entry.

Fast mode may omit a directory's own allocated bytes, over-count hard-linked files, and cross mounted filesystems that strict mode would skip.
It is appropriate when approximate totals are acceptable.

## Error Behavior

Permission errors and other per-entry read failures are partial scan errors.
They should be reported in counts and optional detail views, but they should not make the whole command fail.

Fatal CLI, configuration, or runtime errors should still produce a non-zero exit status.
