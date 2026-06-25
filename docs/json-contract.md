# JSON Machine Interface

[English](json-contract.md) | [日本語](json-contract.ja.md)

This document describes the currently implemented machine-readable formats used by the CLI, snapshots, and MCP adapter.

## Available formats

| Command | Purpose |
| --- | --- |
| `usedu report PATH --json` | Legacy JSON report format. Kept for compatibility. |
| `usedu report PATH --format json-v1` | Explicit form of the legacy JSON report. |
| `usedu report PATH --format json-v2` | Versioned scan envelope for machine clients. |
| `usedu report PATH --format ndjson` | Line-delimited events derived from the JSON v2 envelope. |
| `usedu snapshot PATH` | Full-depth JSON v2 snapshot written to stdout. |
| `usedu compare BEFORE AFTER` | Versioned diff of two snapshot files. |
| `usedu schema json-v2` | Prints the JSON v2 schema. |

Machine-readable output never mixes progress output into stdout.

## Compatibility policy

The legacy `--json` format is not silently replaced by JSON v2 because its shape is already externally observable.

New integrations should use JSON v2, NDJSON, snapshots, or MCP `structuredContent`.

The current schema identifiers are:

```text
usedu.scan.v2
usedu.diff.v1
```

A consumer should validate `schemaVersion` before relying on field semantics.

## Scan envelope

JSON v2 uses this top-level structure:

```text
schemaVersion
scanId
status
semantics
effectiveOptions
root
entries
topFiles
issueSummary
issues
nextCursor
```

### `status`

`status.state` is one of:

- `complete`
- `partial`
- `cancelled`
- `limitReached`

The current scanner normally represents permission errors and traversal-budget limits as a successfully produced `partial` envelope. Output truncation produces `limitReached`.

`partialReasons` contains machine-readable reason strings such as `issuesRecorded`, `resourceLimitReached`, `maxOutputEntries`, and `maxOutputBytes`.

### `semantics`

The envelope records how its sizes were calculated:

- `sizeMetric: allocated`
- accounting source
- strict or approximate accuracy
- hard-link policy
- filesystem-boundary policy
- symlink policy
- whether directory-own bytes are included
- `reclaimableBytesKnown: false`

Clients must not interpret `usedBytes` as guaranteed reclaimable space.

### `effectiveOptions`

The envelope records resolved values for:

- mode: `report` or `snapshot`
- depth
- top limit
- file inclusion
- summary mode
- directory-only filtering
- sort
- issue-detail inclusion
- fast mode
- cross-filesystem policy
- worker count
- output entry and byte limits
- display-path redaction

This allows consumers to determine how much of the tree was retained and whether two results are comparable.

## Entries and paths

`root` is one `EntryDto`. `entries` is a flat array whose `parentEntryId` fields reconstruct the retained tree.

Each entry contains:

```text
entryId
parentEntryId
kind
displayName
displayPath
pathRef
usedBytes
ownBytes
uniqueBytes
sharedBytes
counts
complete
issueCountBelow
skippedCountBelow
```

`kind` is one of `directory`, `regularFile`, `symlink`, or `other`.

`counts` separates:

- regular files
- directories
- symbolic links
- other entries

Directory counts include the directory itself.

`displayName` and `displayPath` are display-only and may contain lossy Unicode conversion. `pathRef` preserves Unix path bytes as hexadecimal and is the reversible identity used by snapshots and diffs.

`entryId` is a convenient reference within a scan, not a durable globally unique identifier.

`uniqueBytes` and `sharedBytes` are present in the schema but are currently `null`; the scanner does not yet split hard-link allocation into these fields.

## Report mode versus snapshot mode

The same envelope type is used in two modes.

### Report mode

`usedu report --format json-v2` uses `effectiveOptions.mode: "report"`.

- `depth` controls retained tree depth.
- `top` truncates selected children per retained directory and limits `topFiles`.
- `includeFiles` is enabled through the CLI `--files` option.
- `dirsOnly` filters ranked tree entries to directories.
- issue details are included only with `--errors`; aggregate issue counts remain available.

### Snapshot mode

`usedu snapshot` and MCP `usedu_scan` use `effectiveOptions.mode: "snapshot"`.

- Snapshot CLI retains the full tree and all file entries unless an output-byte cap truncates the serialized envelope.
- MCP chooses retained depth and filters from tool arguments.
- In MCP snapshot mode, `top` limits `topFiles` but does not limit `entries`.

This distinction is important when reusing MCP scan arguments: `top` is not a directory-ranking limit there.

## Sorting and determinism

Entry ordering uses the requested sort key with path-byte tie-breaking in the protocol layer.

The MCP query tools have their own current behavior:

- `usedu_list_children` orders by `usedBytes` descending;
- `usedu_top_entries` orders by `usedBytes` descending.

They do not preserve the original envelope sort for those query results.

## Output limits

`maxOutputEntries` truncates `entries` and sets `status.state` to `limitReached`.

`maxOutputBytes` is a best-effort serialized-size target. The implementation removes entries first, then top files and issue details until the target is met or only mandatory fields remain. A very small target can still be exceeded by the mandatory envelope structure.

Truncation changes the stored result. MCP query tools cannot recover data removed by these limits.

`nextCursor` records that the envelope was truncated or that report-mode ranking has more entries, but the current MCP tools do not use it to retrieve omitted envelope data.

## NDJSON

NDJSON output is produced after the scan completes from the JSON v2 envelope. It is line-delimited but is not currently a live traversal stream.

The event sequence is:

```text
scanStarted
entry ...
issue ...
scanCompleted
```

Each line includes `schemaVersion` and `scanId`.

## Diff envelope

`usedu compare` and MCP `usedu_compare` return `usedu.diff.v1`:

```text
schemaVersion
status
beforeScanId
afterScanId
summary
changes
```

Diff identity is `pathRef`. The comparison includes each root and retained `entries`; it does not compare `topFiles` or issue records.

A diff is marked inexact when either input is not complete or their `semantics` differ. Inexact changes are classified as `uncertain`.

Callers should also use compatible roots and `effectiveOptions`. The current diff implementation does not reject every option mismatch automatically.

## Redaction

CLI machine output uses `--redact-paths`; MCP uses `redactPaths: true`.

Redaction replaces `displayName` and `displayPath` with `[redacted]`. It intentionally leaves `pathRef` intact so machine identity remains reversible. Do not forward `pathRef` when reversible path disclosure is not acceptable.

## Schema and tests

Print the authoritative JSON v2 schema with:

```bash
usedu schema json-v2
```

Protocol tests cover option reflection, separate entry counts, non-UTF-8 path identity, output limits, structured scan-budget issues, and snapshot diffs.

For filesystem accounting terms, see [Filesystem Semantics](semantics.md). For MCP-specific workflows and limitations, see [Use `usedu` from an AI agent over MCP](mcp-tools.md).
