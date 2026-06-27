# Design Notes

[English](design.md) | [日本語](design.ja.md)

This document records product and implementation constraints that should remain stable as `usedu` evolves. It is not a historical implementation log.

Related documents:

- [ADR 0001: Product Contract](adr/0001-product-contract.md)
- [Filesystem Semantics](semantics.md)
- [JSON Machine Interface](json-contract.md)
- [Agent Security Boundary](agent-security.md)
- [MCP workflows and tool reference](mcp-tools.md)

## Product boundary

`usedu` is a read-only macOS disk allocation inspection tool.
It helps identify where allocated filesystem space is attributed.

`usedu` must not delete, move, mutate, quarantine, or recommend cleanup actions.
It is also not a GUI, background daemon, duplicate finder, real-time monitor, treemap, or logical-size analyzer.

CLI, TUI, JSON, snapshot, diff, and MCP interfaces must preserve this same product boundary.

## Command model

The default command opens the interactive TUI:

```bash
usedu [PATH]
```

If `PATH` is omitted, the TUI opens the current directory.
`usedu` never defaults to `/`; scanning the root volume requires an explicit path.

Static reports and machine-readable report formats are behind `report`:

```bash
usedu report [PATH]
usedu report [PATH] --format json-v2
usedu report [PATH] --format ndjson
```

Persistent snapshots are written to stdout, and file persistence remains the caller's responsibility:

```bash
usedu snapshot [PATH] > scan.usedu.json
usedu compare before.usedu.json after.usedu.json
```

The agent interface is a foreground stdio adapter:

```bash
usedu mcp --stdio --allow-root [PATH]
```

Network transport and default daemon behavior remain outside the product boundary.

## Size semantics

`usedu` reports allocated filesystem size only.
The human display label is always `Used`.
There is no logical-size mode and no size-mode switch.

Strict scanning uses `symlink_metadata` and Unix allocated block counts:

```rust
metadata.blocks().saturating_mul(512)
```

Directory totals include the directory's own allocated bytes plus attributed descendant allocation.

APFS clones, snapshots, compression, sparse files, and File Provider behavior can make reclaimable space differ from displayed `Used`.

Machine interfaces must carry their effective accounting semantics instead of relying on a human label.

## Filesystem rules

Symbolic links are counted as link entries but are not followed.
Hidden files and directories are included.
Package directories such as `.app` and `.photoslibrary` are ordinary directories.

Permission errors are recorded and do not abort the whole scan.
Detailed issue output is optional in report mode and stored by MCP subject to output limits.

Strict mode stays on the requested root filesystem unless cross-filesystem traversal is explicitly enabled.
Fast mode is approximate and can traverse mounted filesystems that strict mode would skip.

Strict mode deduplicates regular files with the same device and inode where practical. Strict entry traversal is deterministic so first-seen hard-link attribution is repeatable.

## TUI interaction model

The TUI is a one-level browser.
For the current directory, it displays only direct children.
Each direct child directory still shows its recursive `Used` total.

For example, when viewing `~/Library`, the TUI may show `Application Support`, `Containers`, and `Caches`. It should not show grandchildren such as `Application Support/A` until the user opens `Application Support`.

This model keeps the screen scannable while preserving useful recursive totals.

## MCP interaction model

The MCP server is a foreground, process-local adapter over stdio.

- Allowed roots are configured when the process starts.
- `usedu_scan` creates an in-memory session and returns a `scanId`.
- Follow-up tools query the stored scan envelope; they do not rescan automatically.
- Retained depth, file inclusion, and output limits determine what follow-up queries can observe.
- Sessions are bounded, expire by inactivity TTL, and disappear when the process exits.
- Background scans expose progress and cooperative cancellation.

MCP is intended to let an agent inspect, navigate, explain, and compare scan results. It must not add cleanup capabilities.

The user-facing behavior and current limitations are documented in [MCP workflows and tool reference](mcp-tools.md).

## Scanner architecture

Scanner code is isolated in the `usedu-core` crate and is independent from terminal rendering and MCP transport.

Report mode, TUI mode, snapshots, and MCP tools share scanner logic.
`ScanEngine` accepts `ScanRequest` values and returns `ScanOutcome` values. Collectors derive summaries and retained views without adding presentation concerns to the scanner.

Versioned machine-readable DTOs live in `usedu-protocol`.
The root `usedu` crate provides CLI, TUI, output, snapshot, diff, and MCP adapters.

The scanner uses `PathBuf` and `OsString` and must not assume paths are valid UTF-8. Lossy conversion belongs only at display or serialization boundaries. Reversible machine identity uses raw Unix path bytes through `pathRef`.

The scanner should avoid retaining every file as a tree node. It retains directory summaries, requested tree entries, issue records, and top-file candidates according to the request.

## Performance principles

Scanning reads metadata only and does not read file contents.

Progress output must be throttled.
The scanner must not print one line per file or build display strings during traversal.
Sorting belongs at deterministic collection or presentation boundaries, not repeated ad hoc during scanning.

Parallelism must use bounded worker resources. Fast mode can parallelize directory subtrees; strict mode preserves deterministic traversal for accounting consistency.

Long-lived adapters must keep sessions and worker resources bounded.

## Fast mode

Fast mode favors lower scan latency over strict accounting.
On macOS, it can use bulk metadata APIs to reduce per-entry filesystem calls.
For unretained subtrees, it can aggregate totals without building display nodes for every entry.

Fast mode may omit directory-own bytes, over-count hard-linked files, and traverse mounted filesystems that strict mode would skip.
Machine output must report fast mode as approximate semantics, not merely as a performance flag.

## Error behavior

Permission errors and other per-entry failures are partial scan issues.
They are represented in counts and optional structured details but do not necessarily make the command or MCP call fail.

Traversal budgets also produce partial envelopes with structured resource-limit issues.
Output limits produce `limitReached` envelopes.

Fatal CLI, configuration, transport, or runtime errors still produce a non-zero command status or JSON-RPC error.
