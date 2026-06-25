# usedu

[English](README.md) | [日本語](README.ja.md)

`usedu` is a read-only disk usage analyzer for macOS terminals.
It scans file-system metadata and displays allocated size as `Used`.
It provides a static report, an interactive TUI browser, versioned machine output, and a local MCP interface for AI agents.

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
usedu report ~/Library --format json-v2
usedu report ~/Library --format ndjson
usedu schema json-v2
usedu snapshot ~/Library > scan.usedu.json
usedu compare before.usedu.json after.usedu.json
usedu mcp --stdio --allow-root ~/Library
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
    --sort used|name|files|dirs
                            Sort key. Default: used
    --json                  Output legacy JSON instead of rich text
    --format text|json-v1|json-v2|ndjson
                            Output format. Default: text
    --errors                Show error details
    --redact-paths          Redact display paths in machine-readable output
    --max-output-bytes <N>  Cap JSON v2/NDJSON output bytes
    --no-progress           Disable progress indicator
    --cross-file-systems    Allow scanning across mounted filesystems
    --jobs <N>              Worker count for parallel scans
```

While scanning, rich-text mode shows throttled progress with entries, errors, and elapsed time.
Machine-readable formats and `--no-progress` suppress the progress indicator.

`--json` is the legacy JSON report format.
Use `--format json-v2` for the versioned machine-readable scan envelope.
Use `--format ndjson` for line-delimited scan events.

Print the JSON v2 schema with:

```bash
usedu schema json-v2
```

Create a persistent snapshot through stdout with:

```bash
usedu snapshot [PATH] > scan.usedu.json
```

Compare two snapshot files with:

```bash
usedu compare before.usedu.json after.usedu.json
```

## AI Agents and MCP

Run the local foreground MCP server with one or more allowed roots:

```bash
usedu mcp --stdio \
  --allow-root "$HOME/Library" \
  --allow-root "$HOME/Projects"
```

An MCP-connected agent can:

- identify the directories using the most allocated space;
- find the largest regular files across a scanned tree;
- drill into retained directory results without rescanning for each question;
- explain permission errors, filesystem-boundary skips, and partial results;
- run a long scan in the background, report progress, and request cancellation;
- compare two in-memory scan sessions to show what grew or shrank.

Typical requests include:

```text
Find the ten largest directories under ~/Library.
Find the largest regular files in this project.
Explain why the scan result is partial.
Compare this directory before and after the build.
```

The server is read-only. It does not remove files or recommend cleanup actions. It reads metadata, not file contents.

Allowed roots are fixed when the server starts. Sessions are held only in memory and disappear when the process exits. Queries operate on entries retained by the original scan, so `depth`, `includeFiles`, and output limits affect later drill-down.

For workflows, tool behavior, and current limitations, see [Use `usedu` from an AI agent over MCP](docs/mcp-tools.md). For the permission and privacy boundary, see [Agent Security Boundary](docs/agent-security.md).

## Size Semantics

`usedu` reports allocated file-system size only.
It uses `symlink_metadata` and the Unix block count (`blocks() * 512`) rather than logical byte length.

The display label is always `Used`.
There is no `--logical` or `--allocated` mode switch.

APFS clones, snapshots, compression, sparse files, and file-provider behavior can make reclaimable space differ from the displayed `Used` value.

`--fast` keeps allocated-size accounting for files, but skips some expensive metadata work and uses more aggressive nested parallel traversal.
It may omit a directory's own allocated bytes, over-count hard-linked files, and traverse mounted filesystems that strict mode would skip.
Use it when rough totals are acceptable and scan latency matters more than strict accounting.

`--summarize` prints only the root total.
Combined with `--fast`, it also avoids retaining root child summaries.
That combination is useful when you only need a total-only report and want the lowest scan latency.

## Filesystem Behavior

- Symbolic links are counted as link entries but are not followed.
- Hidden files and directories are included.
- Package directories such as `.app` and `.photoslibrary` are ordinary directories.
- Permission errors are recorded and do not abort the whole scan.
- Strict mode stays on the requested root filesystem by default.
- Use `--cross-file-systems` to allow strict traversal across file-system boundaries.
- Fast mode is approximate and may traverse mounted filesystems even without that option.
- Regular files with multiple hard links are counted once per device/inode where practical in strict mode.

Machine-readable JSON v2 separates regular file, directory, symlink, and other entry counts.
It also includes display-only paths plus reversible `pathRef` values.

For protected macOS locations, grant Full Disk Access to the terminal app if expected paths are unreadable.

## Development

Design constraints that should remain stable are documented in [docs/design.md](docs/design.md).
The product contract is recorded in [docs/adr/0001-product-contract.md](docs/adr/0001-product-contract.md), filesystem terms are defined in [docs/semantics.md](docs/semantics.md), the JSON interface is documented in [docs/json-contract.md](docs/json-contract.md), the agent boundary is in [docs/agent-security.md](docs/agent-security.md), and MCP workflows and tools are in [docs/mcp-tools.md](docs/mcp-tools.md).

```bash
cargo build
cargo test --workspace
cargo fmt
cargo clippy --workspace --all-targets --all-features
```
