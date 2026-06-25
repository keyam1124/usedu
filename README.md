# usedu

[English](README.md) | [日本語](README.ja.md)

`usedu` is a read-only disk usage analyzer for macOS terminals.
It scans file-system metadata and displays allocated size as `Used`.
It provides both a static report and an interactive TUI browser.

`usedu` does not delete, move, or modify files.
It also does not recommend cleanup actions.

## Installation

```bash
cargo install --path .
```

## Usage

```bash
usedu
usedu ~/Library
usedu --fast ~/Library
usedu report ~/Library --depth 2 --top 30
usedu report ~/Library --files
usedu report ~/Library --fast --summarize
usedu report ~/Library --json
```

If no path is provided, `usedu` opens the current directory in the TUI.
It never defaults to `/`.
Scanning the whole root volume requires explicitly running `usedu /` or `usedu report /`.

## TUI

```bash
usedu [PATH]
```

The default command opens an interactive terminal browser.

Useful options:

```text
    --fast                  Use faster approximate scanning
    --cross-file-systems    Allow scanning across mounted filesystems
    --jobs <N>              Worker count for parallel scans
```

The TUI shows only the direct children of the current directory.
Each child directory is aggregated recursively, so the `Used` column shows the full allocated size below that child.

During loading, the TUI shows entries, errors, and elapsed time.
Press `q` while loading to cancel the scan and leave the TUI.

Key bindings:

```text
Up / k          Move up
Down / j        Move down
Enter           Open selected directory
Backspace / h   Parent directory
r               Rescan current directory
R               Clear cached result and rescan
s               Toggle sort: used, name, files, dirs
e               Toggle error list
?               Toggle help
q               Quit
```

## Static Report

```bash
usedu report [PATH]
```

Useful options:

```text
-d, --depth <N>             Display tree depth. Default: 2
-n, --top <N>               Show top N entries. Default: 30
    --files                 Include top large files section
    --summarize             Show only the total summary
    --fast                  Use faster approximate scanning
    --dirs-only             Only show directories in ranking
    --sort used|files|dirs  Sort key. Default: used
    --json                  Output JSON instead of rich text
    --errors                Show error details
    --no-progress           Disable progress indicator
    --cross-file-systems    Allow scanning across mounted filesystems
    --jobs <N>              Worker count for parallel scans
```

While scanning, rich-text mode shows throttled progress with entries, errors, and elapsed time.
JSON mode and `--no-progress` suppress the progress indicator.

## Size Semantics

`usedu` reports allocated file-system size only.
It uses `symlink_metadata` and the Unix block count (`blocks() * 512`) rather than logical byte length.

The display label is always `Used`.
There is no `--logical` or `--allocated` mode switch.

APFS clones, snapshots, compression, sparse files, and file-provider behavior can make reclaimable space differ from the displayed `Used` value.

`--fast` keeps allocated-size accounting for files, but skips some expensive metadata work and uses more aggressive nested parallel traversal.
It may omit a directory's own allocated bytes, over-count hard-linked files, and cross mounted filesystems that strict mode would skip.
Use it when rough totals are acceptable and scan latency matters more than strict accounting.

`--summarize` prints only the root total.
Combined with `--fast`, it also avoids retaining root child summaries.
That combination is useful when you only need a total-only report and want the lowest scan latency.

## Filesystem Behavior

- Symbolic links are counted as link entries but are not followed.
- Hidden files and directories are included.
- Package directories such as `.app` and `.photoslibrary` are ordinary directories.
- Permission errors are recorded and do not abort the whole scan.
- By default, mounted volumes on a different device are skipped.
- Use `--cross-file-systems` to allow crossing file-system boundaries.
- Regular files with multiple hard links are counted once per device/inode where practical.

For protected macOS locations, grant Full Disk Access to the terminal app if expected paths are unreadable.

## Development

Design constraints that should remain stable are documented in [docs/design.md](docs/design.md).

```bash
cargo build
cargo test
cargo fmt
cargo clippy --all-targets --all-features
```
