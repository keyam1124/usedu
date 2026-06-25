# MCP stdio interface

[English](mcp-tools.md) | [日本語](mcp-tools.ja.md)

`usedu mcp --stdio` runs a foreground, read-only MCP server over standard input and standard output.
It exposes completed scan results as in-memory sessions so an MCP client can inspect the result without rescanning the filesystem for every query.

The current implementation:

- supports stdio only;
- advertises MCP protocol version `2024-11-05`;
- accepts one JSON-RPC message per line and writes one response per line;
- reserves stdout for protocol messages and stderr for diagnostics;
- keeps all sessions in memory, so they disappear when the process exits;
- returns JSON v2 scan envelopes with schema version `usedu.scan.v2` and diff envelopes with schema version `usedu.diff.v1`.

`usedu` remains an inspection tool. The MCP server does not delete, move, quarantine, or recommend files for removal.

## Start the server

```bash
usedu mcp --stdio \
  --allow-root "$HOME/Library" \
  --allow-root "$HOME/Projects" \
  --max-sessions 8
```

| Option | Default | Behavior |
| --- | --- | --- |
| `--stdio` | required | Starts the stdio server. No HTTP transport is implemented. |
| `--allow-root PATH` | current directory | May be repeated. Only existing paths under a canonicalized allowed root can be scanned. |
| `--max-sessions N` | `8` | Maximum number of sessions retained by the process. Values below `1` are treated as `1`. |

Allowed roots are fixed when the server starts. Tool arguments cannot widen the allowlist, and the current implementation does not import roots dynamically from the MCP client.

Every allowed root and requested scan path is canonicalized. A requested path that resolves outside the allowlist, including through a symbolic link, is rejected before traversal.

See [Agent Security Boundary](agent-security.md) for path identity, redaction, filesystem boundaries, and resource controls.

## MCP result shape

Every successful `tools/call` response contains the tool-specific payload twice:

- `result.structuredContent`: the value machine clients should consume;
- `result.content[0].text`: the same value serialized as JSON text for compatibility with text-oriented clients.

Abbreviated example:

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"scanId\":\"scan_...\",\"state\":\"complete\",...}"
      }
    ],
    "structuredContent": {
      "scanId": "scan_...",
      "state": "complete",
      "envelope": {
        "schemaVersion": "usedu.scan.v2",
        "status": { "state": "complete", "partialReasons": [] }
      }
    }
  }
}
```

Treat `scanId`, `entryId`, and cursors as opaque values. They are implementation identifiers, not durable external IDs.

## Session state and scan status are different

There are two independent status layers.

### MCP session state

The outer `state` describes whether the server-side operation has produced a stored envelope.

| State | Meaning | Envelope available |
| --- | --- | --- |
| `running` | A background scan is still executing. | No |
| `complete` | The scan operation finished and a scan envelope is stored. | Yes |
| `cancelled` | A background scan observed cancellation. | No in the current implementation |
| `failed` | A background scan ended with a fatal error. | No |

### Scan envelope status

`envelope.status.state` describes the completeness of the filesystem result itself.

| State | Meaning |
| --- | --- |
| `complete` | No scan issue or output truncation was recorded. |
| `partial` | An envelope was produced, but filesystem issues or traversal budgets made the result incomplete. |
| `limitReached` | Output sections were truncated by `maxOutputEntries` or `maxOutputBytes`. |

Consequently, these are valid combinations:

- session `complete` + envelope `partial` after a permission error or traversal budget;
- session `complete` + envelope `limitReached` after output truncation;
- session `cancelled` with no envelope after cancelling a background scan.

Clients should check both layers before treating a result as complete.

## What a scan session stores

A completed session stores one `ScanEnvelope` in memory:

```text
envelope.root       one root entry
envelope.entries    flat retained tree entries selected by depth and filters
envelope.topFiles   largest regular files collected across traversal
envelope.issues     stored filesystem and budget issue details
```

The query tools do not rescan the filesystem. They read these stored sections:

| Tool | Stored data used |
| --- | --- |
| `usedu_list_children` | `envelope.entries` |
| `usedu_top_entries` | `envelope.entries` |
| `usedu_get_issues` | `envelope.issues` |
| `usedu_compare` | `envelope.root` and `envelope.entries` from both sessions |

This means the original `depth`, `includeFiles`, and output limits determine what later queries can see.

## Typical workflows

As with a normal MCP connection, call `initialize` first, optionally inspect `tools/list`, and then invoke tools through `tools/call`.

### Synchronous scan

A synchronous call blocks until the scan completes or fails.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "usedu_scan",
    "arguments": {
      "root": "/Users/example/Library",
      "depth": 2,
      "includeFiles": true,
      "top": 30
    }
  }
}
```

On success, `structuredContent.state` is `complete` and `structuredContent.envelope` is present. This outer state only means that an envelope exists; inspect `envelope.status` for partial or truncated results.

The returned `scanId` can then be used with `usedu_list_children`, `usedu_top_entries`, `usedu_get_issues`, `usedu_compare`, and `usedu_close_scan`.

### Background scan

Set `background: true` to return immediately:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "usedu_scan",
    "arguments": {
      "root": "/Users/example/Library",
      "background": true
    }
  }
}
```

The initial result contains `state: "running"` and no envelope. Poll `usedu_scan_status`; set `includeEnvelope: true` when the completed envelope is needed.

Cancellation is cooperative. `usedu_cancel_scan` only requests cancellation, so its immediate response may still report `state: "running"`. Continue polling until the state changes.

## Tool overview

| Tool | Purpose |
| --- | --- |
| `usedu_scan` | Run a synchronous or background scan and create a session. |
| `usedu_scan_status` | Read background progress and optionally retrieve the completed envelope. |
| `usedu_cancel_scan` | Request cooperative cancellation of a running scan. |
| `usedu_list_children` | Page through direct children retained in the envelope. |
| `usedu_top_entries` | Rank retained entries across the stored tree. |
| `usedu_get_issues` | Page through stored issue details. |
| `usedu_compare` | Compare the root and retained entries of two sessions. |
| `usedu_close_scan` | Remove a session and cancel it if still running. |

## Tool reference

### `usedu_scan`

Scans one allowed path and creates a session.

#### Input

| Field | Type / default | Actual behavior |
| --- | --- | --- |
| `root` | string, required | Existing path to scan. It is canonicalized and must resolve inside the startup allowlist. |
| `depth` | integer `>= 0`, default `1` | Retained result depth, not traversal depth. `0` returns only the root; `1` includes direct children. Descendants are still traversed to aggregate directory totals. |
| `top` | integer `>= 0`, default `30` | Limits `envelope.topFiles`. In the MCP snapshot-style envelope it does **not** limit `envelope.entries`. |
| `includeFiles` | boolean, default `false` | Includes file, symlink, and other leaf entries within the retained depth and enables collection of `topFiles`. Without it, `entries` contains directories only. |
| `dirsOnly` | boolean, default `false` | Filters `entries` to directories. It does not remove `topFiles` when `includeFiles` is also true. |
| `sort` | `used`, `name`, `files`, or `dirs`; default `used` | Controls ordering while the envelope is built. Query tools have their own ordering rules described below. |
| `fast` | boolean, default `false` | Uses approximate fast accounting. It can omit directory-own bytes, double-count hard links, and cross mounted filesystems that strict mode would skip. |
| `crossFileSystems` | boolean, default `false` | Allows strict traversal into mounted filesystems below the requested root. Fast mode can cross filesystem boundaries even when this is false. |
| `maxScanEntries` | positive integer, optional | Traversal budget. Exceeding it produces a stored partial envelope with a `RESOURCE_LIMIT_REACHED` issue. |
| `maxScanDurationMs` | positive integer, optional | Traversal time budget in milliseconds. Exceeding it produces a stored partial envelope. |
| `maxOutputEntries` | non-negative integer, optional | Truncates the stored `envelope.entries` array and sets envelope status to `limitReached`. |
| `maxOutputBytes` | non-negative integer, optional | Best-effort serialized-size target. The implementation removes entries first, then top files and issue details, and sets `limitReached`. Mandatory envelope fields can still exceed a very small target. |
| `redactPaths` | boolean, default `false` | Replaces `displayName` and `displayPath` with `[redacted]`. Reversible `pathRef` values remain present. |
| `background` | boolean, default `false` | Runs the scan in a worker thread and returns the session before an envelope exists. |

#### Output

```text
scanId
schemaVersion
state
progress
[envelope]
```

`envelope` is present for a successful synchronous scan and omitted from the initial background response.

`progress` contains:

```text
elapsedMs
entriesSeen
filesSeen
dirsSeen
errorsSeen
done
```

`envelope.topFiles` and `usedu_top_entries` are different views:

- `topFiles` contains the largest regular files found across the traversal and is limited by `top`;
- `usedu_top_entries` ranks only entries already retained in `envelope.entries`.

### `usedu_scan_status`

Returns the current state of one session.

#### Input

| Field | Type / default | Behavior |
| --- | --- | --- |
| `scanId` | string, required | Session to inspect. |
| `includeEnvelope` | boolean, default `false` | Adds `envelope` only when the session state is `complete`. |

#### Output

```text
scanId
schemaVersion
state
progress
[envelope]
[message]
```

`message` is present for `cancelled` and `failed` sessions. Calling this tool refreshes the session's inactivity timestamp.

### `usedu_cancel_scan`

Requests cancellation of a running background scan.

#### Input

```text
scanId
```

#### Output

```text
scanId
cancelRequested
state
progress
```

`cancelRequested` is `true` only when the session was still `running`. It does not mean the worker has already stopped.

### `usedu_list_children`

Lists direct children of an entry from the stored envelope.

#### Input

| Field | Type / default | Behavior |
| --- | --- | --- |
| `scanId` | string, required | Must refer to a session whose outer state is `complete`. |
| `entryId` | string, required | Parent entry. The root entry ID is returned as `envelope.root.entryId`. |
| `limit` | integer `1..500`, default `50` | Page size. Values outside this range are clamped. |
| `cursor` | string, optional | Opaque continuation cursor returned by the previous call. |

#### Output

```text
scanId
entryId
items
nextCursor
```

Current behavior to account for:

- only direct children already present in `envelope.entries` are searchable;
- results are ordered by `usedBytes` descending, regardless of the `sort` passed to `usedu_scan`;
- an unknown or leaf `entryId` returns an empty page rather than an error;
- entries omitted by `depth`, `includeFiles`, `maxOutputEntries`, or `maxOutputBytes` cannot be recovered through this tool.

### `usedu_top_entries`

Ranks retained entries across all retained depths.

#### Input

| Field | Type / default | Behavior |
| --- | --- | --- |
| `scanId` | string, required | Must refer to a complete session. |
| `limit` | integer `1..500`, default `50` | Maximum number of returned entries. Values outside this range are clamped. |
| `kind` | optional enum | `directory`, `regularFile`, `symlink`, or `other`. |
| `minUsedBytes` | non-negative integer, default `0` | Minimum allocated size. |

#### Output

```text
scanId
items
```

The root is excluded. Results come from `envelope.entries`, are ordered by `usedBytes` descending, and are not cursor-paginated. This tool does not read `envelope.topFiles` and does not discover entries that were not retained in the original envelope.

### `usedu_get_issues`

Pages through issue details stored in the envelope.

#### Input

| Field | Type / default | Behavior |
| --- | --- | --- |
| `scanId` | string, required | Must refer to a complete session. |
| `limit` | integer `1..500`, default `50` | Page size. Values outside this range are clamped. |
| `cursor` | string, optional | Opaque continuation cursor. |

#### Output

```text
scanId
items
nextCursor
```

Issues can represent filesystem errors, policy skips, or traversal-budget limits. `envelope.issueSummary` retains aggregate counts even if `maxOutputBytes` removed some issue details; removed details cannot be fetched later from the current session.

### `usedu_compare`

Compares two stored scan envelopes.

#### Input

```text
beforeScanId
afterScanId
```

#### Output

A `usedu.diff.v1` envelope containing:

```text
schemaVersion
status
beforeScanId
afterScanId
summary
changes
```

The comparison uses `pathRef` identity for the root and retained `entries`. It does not compare `topFiles` or issue records.

The implementation automatically marks the diff inexact when either input envelope is not `complete` or when their accounting semantics differ. When a diff is inexact, changes are reported as `uncertain`.

For a meaningful comparison, callers must also use compatible roots and effective options, especially `depth`, `includeFiles`, `dirsOnly`, and output limits. The current implementation does not automatically reject every effective-option mismatch.

### `usedu_close_scan`

Removes one session.

#### Input

```text
scanId
```

#### Output

```text
scanId
closed
```

`closed` is `true` when a session was removed and `false` when it was already absent. Closing a running session also requests cancellation.

## Session retention

Sessions are process-local and in memory.

- The default maximum is 8 sessions, configurable with `--max-sessions`.
- When the table is full, the least recently updated session is removed before inserting the new one.
- Removing a running session requests cancellation.
- The inactivity TTL is currently fixed at 30 minutes.
- Expired sessions are pruned lazily before the next incoming request, not by a dedicated timer.
- Status and query calls refresh the session timestamp.

Do not persist a `scanId` or assume it remains valid after server restart, TTL expiry, eviction, or `usedu_close_scan`.

## Error model

The server separates protocol/tool errors from scan issues.

| Situation | Result |
| --- | --- |
| Unknown JSON-RPC method | JSON-RPC error `-32601` |
| Missing/invalid tool argument, unknown tool, invalid cursor, unknown `scanId`, request outside allowlist, query before scan completion, or fatal synchronous scan failure | JSON-RPC error `-32602` |
| Malformed input line or an internal request-handler failure | JSON-RPC error `-32603` with a null request ID |
| Filesystem read failure, cross-filesystem skip, or traversal budget reached after a scan envelope can be built | Successful tool response with structured `envelope.issues` and non-complete `envelope.status` |
| Fatal error in an already-started background scan | Session state `failed` with `message` |
| Observed background cancellation | Session state `cancelled` with `message` |

A successful JSON-RPC response does not imply a complete filesystem result. Always inspect `envelope.status` and `envelope.issueSummary`.

## Current implementation boundaries

The following points are intentional documentation of the current implementation, not promises of broader behavior:

- stdio is the only transport;
- allowed roots are configured at process startup;
- requests without an `id` are treated as notifications and produce no response;
- sessions and envelopes are not durable;
- query tools inspect the stored envelope and do not rescan the filesystem;
- output truncation permanently removes data from that session;
- `envelope.nextCursor` is part of the JSON v2 envelope, but current MCP query tools do not use it to retrieve truncated envelope data;
- `usedu_list_children` and `usedu_top_entries` use allocated-size ordering rather than the original scan sort;
- `tools/list` advertises input schemas; clients should use `structuredContent` and the JSON v2 schema for result validation.

Related contracts:

- [Filesystem Semantics](semantics.md)
- [JSON Contract](json-contract.md)
- [Agent Security Boundary](agent-security.md)
