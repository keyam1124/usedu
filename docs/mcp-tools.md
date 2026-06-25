# Use `usedu` from an AI agent over MCP

[English](mcp-tools.md) | [日本語](mcp-tools.ja.md)

`usedu mcp --stdio` lets an MCP client, such as an AI agent, inspect disk usage under explicitly allowed directories through a read-only interface.

The main value is not merely that an agent can launch a scan. A scan result is kept as an in-memory session, and the agent can ask follow-up questions such as “show the largest areas,” “list this directory's children,” “explain why the result is incomplete,” or “compare two scans” without rescanning for every query.

## What users can accomplish

| User goal | What the agent does | Main tools |
| --- | --- | --- |
| Understand which directories use the most space | Scan an allowed root and rank retained entries by allocated size | `usedu_scan`, `usedu_top_entries` |
| Find large files | Include files in the scan and read `envelope.topFiles`, collected across the traversal | `usedu_scan` |
| Drill into a directory | Page through direct children stored in the scan result | `usedu_list_children` |
| Explain an incomplete result | Retrieve permission errors, filesystem-boundary skips, and traversal-budget issues | `usedu_get_issues` |
| Observe or stop a long scan | Start a background scan, poll progress, and request cancellation | `usedu_scan_status`, `usedu_cancel_scan` |
| Find what grew or shrank between two points | Compare two sessions held by the same server process | `usedu_compare` |
| Reduce display-path exposure | Redact display names and paths in the result | `redactPaths: true` |

Example requests a user could give an agent include:

- “Find the ten directories using the most allocated space under `~/Library`.”
- “Find the largest regular files under this project.”
- “Show the direct children of `Application Support`, ordered by size.”
- “Explain why this scan is partial.”
- “Run this scan in the background and stop it if it takes too long.”
- “Scan this directory before and after the operation and show what grew.”

## What MCP does not enable

Using MCP does not change the `usedu` product boundary.

- It does not delete, move, mutate, or quarantine files.
- It does not certify that an entry is safe to delete.
- It does not recommend cleanup actions.
- It does not read file contents.
- It cannot scan outside the startup allowlist.
- Reported `Used` bytes are not guaranteed reclaimable bytes.
- It is not a real-time monitor or a background daemon.

MCP sessions are memory-only and disappear when the server exits. For comparisons that must survive process restarts, use CLI snapshots instead:

```bash
usedu snapshot PATH > before.usedu.json
usedu snapshot PATH > after.usedu.json
usedu compare before.usedu.json after.usedu.json
```

## How the interface works

```text
1. Configure allowed roots when the server starts
2. Call usedu_scan and receive a scanId
3. Use that scanId to query the stored result
4. Close the session when it is no longer needed
```

Follow-up query tools read the stored envelope. They do not rescan the filesystem.

```text
envelope.root       the scan root
envelope.entries    entries retained by depth and filters
envelope.topFiles   largest regular files collected across traversal
envelope.issues     filesystem and traversal-budget issue details
```

The original `depth`, `includeFiles`, and output limits determine what later queries can see. Data omitted or truncated from the envelope cannot be recovered by a later tool call.

## Start the server

```bash
usedu mcp --stdio \
  --allow-root "$HOME/Library" \
  --allow-root "$HOME/Projects" \
  --max-sessions 8
```

| Option | Default | Behavior |
| --- | --- | --- |
| `--stdio` | required | Starts the stdio MCP server. HTTP transport is not implemented. |
| `--allow-root PATH` | current directory | May be repeated. Only existing paths that canonicalize under an allowed root can be scanned. |
| `--max-sessions N` | `8` | Maximum sessions retained by the process. Values below `1` are treated as `1`. |

Allowed roots are fixed at startup. The current implementation does not dynamically import MCP client `roots`, and tool arguments cannot widen the allowlist.

Allowed roots and requested scan paths are canonicalized. Requests that resolve outside the allowlist, including through symbolic links, are rejected before traversal. Nonexistent paths are also rejected.

## Goal-oriented workflows

### Understand the largest directories

Start with enough retained depth to make the directories of interest queryable:

```json
{
  "root": "/Users/example/Library",
  "depth": 2,
  "includeFiles": false,
  "sort": "used"
}
```

Then call `usedu_top_entries` with `kind: "directory"` to rank directories across all retained levels.

In the MCP interface, `usedu_scan.top` limits `envelope.topFiles`; it does **not** limit `envelope.entries` or the number of directories. Use `usedu_top_entries.limit` for the directory ranking size.

### Find the largest regular files

```json
{
  "root": "/Users/example/Projects",
  "depth": 1,
  "includeFiles": true,
  "top": 50
}
```

After completion, `envelope.topFiles` contains up to 50 of the largest regular files found across the traversal, independent of the retained tree depth.

`usedu_top_entries` with `kind: "regularFile"` is a different view: it ranks only regular files already retained in `envelope.entries`.

### Drill into a directory

Pass a parent `entryId` to `usedu_list_children`. The root ID is returned as `envelope.root.entryId`.

```json
{
  "scanId": "mcp_scan_...",
  "entryId": "entry_...",
  "limit": 50
}
```

If the initial scan did not retain enough depth, later calls cannot reconstruct the omitted children. Start a new `usedu_scan` with that directory as `root`, provided it remains inside the startup allowlist.

### Run a long scan in the background

```json
{
  "root": "/Users/example/Library",
  "background": true,
  "maxScanDurationMs": 60000
}
```

The initial response has `state: "running"` and no envelope. Poll `usedu_scan_status`; set `includeEnvelope: true` when the completed result is needed.

Cancellation is cooperative. `usedu_cancel_scan` requests cancellation, so its immediate response may still be `running`. Continue polling until the state changes.

A synchronous scan blocks the stdio request loop, so use `background: true` whenever progress polling or cancellation is required.

### Compare two scans

`usedu_compare` compares two sessions held by the same server process.

Synchronous scan IDs are currently derived from the root path and aggregate size/count values. Two synchronous scans of the same root can therefore reuse the same `scanId` when those aggregate values are unchanged, replacing the earlier session. For a reliable before/after comparison, run both scans with `background: true`, which assigns distinct process-local session IDs.

Use compatible options for both scans, especially:

- `root`
- `depth`
- `includeFiles`
- `dirsOnly`
- `fast`
- `crossFileSystems`
- output limits

If either envelope is partial or `limitReached`, or their accounting semantics differ, the diff is marked `exact: false` and changes become `uncertain`. The current implementation does not automatically reject every effective-option mismatch.

### Explain a partial result

When `envelope.status.state` is `partial` or `limitReached`, inspect:

- `envelope.status.partialReasons`
- `envelope.issueSummary`
- `usedu_get_issues`

Issues can include permission errors, entries disappearing during traversal, filesystem-boundary skips, and traversal-budget limits.

## Two different status layers

The outer MCP session `state` and `envelope.status.state` describe different things.

### Session `state`

| Value | Meaning | Envelope |
| --- | --- | --- |
| `running` | A background scan is still executing | absent |
| `complete` | The scan operation ended and an envelope is stored | present |
| `cancelled` | A background scan observed cancellation | absent in the current implementation |
| `failed` | A background scan ended with a fatal error | absent |

### `envelope.status.state`

| Value | Meaning |
| --- | --- |
| `complete` | No scan issue or output truncation was recorded |
| `partial` | An envelope exists, but filesystem issues or traversal budgets made it incomplete |
| `limitReached` | `maxOutputEntries` or `maxOutputBytes` truncated output sections |

A session can therefore be `complete` while its envelope is `partial` or `limitReached`. Clients should inspect both layers.

## Tool overview

| Tool | What it provides |
| --- | --- |
| `usedu_scan` | Run a synchronous or background scan and create a session |
| `usedu_scan_status` | Read background progress and optionally retrieve the completed envelope |
| `usedu_cancel_scan` | Request cooperative cancellation of a running scan |
| `usedu_list_children` | Page through direct children retained in the envelope |
| `usedu_top_entries` | Rank entries retained across the stored tree |
| `usedu_get_issues` | Page through stored issue details |
| `usedu_compare` | Compare two stored scan envelopes |
| `usedu_close_scan` | Remove a session and cancel it if still running |

## Tool reference

### `usedu_scan`

| Field | Type / default | Actual behavior |
| --- | --- | --- |
| `root` | string, required | Existing path that canonicalizes inside the startup allowlist. |
| `depth` | integer `>= 0`, default `1` | Retained result depth, not traversal depth. `0` keeps only the root; `1` keeps direct children. Descendants are still traversed for directory totals. |
| `top` | integer `>= 0`, default `30` | Limits `envelope.topFiles`. It does not limit MCP `entries`. |
| `includeFiles` | boolean, default `false` | Retains regular files, symlinks, and other leaf entries within `depth`, and enables `topFiles` collection. |
| `dirsOnly` | boolean, default `false` | Filters `entries` to directories. It does not remove `topFiles` when `includeFiles` is true. |
| `sort` | `used`, `name`, `files`, or `dirs`; default `used` | Controls envelope construction order. Query tools use their own ordering described below. |
| `fast` | boolean, default `false` | Approximate accounting. It can omit directory-own bytes, double-count hard links, and traverse mounted filesystems strict mode would skip. |
| `crossFileSystems` | boolean, default `false` | In strict mode, includes mounted filesystems below the root. Fast mode may cross boundaries even when false. |
| `maxScanEntries` | positive integer, optional | Traversal budget. Reaching it produces a partial envelope with `RESOURCE_LIMIT_REACHED`. |
| `maxScanDurationMs` | positive integer, optional | Cooperative duration budget checked during traversal, not a hard preemptive timeout. |
| `maxOutputEntries` | non-negative integer, optional | Truncates stored `envelope.entries` and sets `limitReached`. |
| `maxOutputBytes` | non-negative integer, optional | Best-effort serialized-size target. Entries are removed first, then top files and issues. Mandatory fields can still exceed a very small target. |
| `redactPaths` | boolean, default `false` | Replaces `displayName` and `displayPath` with `[redacted]`; reversible `pathRef` remains. |
| `background` | boolean, default `false` | Runs in a worker thread and returns before the envelope exists. |

MCP scans always build a snapshot-mode envelope with `effectiveOptions.mode: "snapshot"`. The tool does not expose a `jobs` argument; it uses the scanner default.

A successful synchronous result contains:

```text
scanId
schemaVersion
state
progress
envelope
```

An initial background result omits `envelope`.

`progress` contains `elapsedMs`, `entriesSeen`, `filesSeen`, `dirsSeen`, `errorsSeen`, and `done`.

### `usedu_scan_status`

Input:

```text
scanId
includeEnvelope?   default: false
```

`envelope` is added only when `includeEnvelope` is true and the session is `complete`. Calling this tool refreshes the session's inactivity timestamp.

### `usedu_cancel_scan`

Input:

```text
scanId
```

`cancelRequested` is true only if the session was still `running`. It does not mean the worker has already stopped.

### `usedu_list_children`

Input:

```text
scanId
entryId
limit?    default: 50, runtime range: 1..500
cursor?
```

Current behavior:

- only direct children already present in `envelope.entries` are returned;
- results are ordered by `usedBytes` descending, regardless of the original scan `sort`;
- an unknown or leaf `entryId` returns an empty list rather than an error;
- entries omitted by depth or output limits cannot be recovered.

### `usedu_top_entries`

Input:

```text
scanId
limit?          default: 50, runtime range: 1..500
kind?           directory | regularFile | symlink | other
minUsedBytes?   default: 0
```

The root is excluded. Results come from all retained `envelope.entries`, are sorted by `usedBytes` descending, and are not cursor-paginated. This tool does not read `envelope.topFiles`.

### `usedu_get_issues`

Input:

```text
scanId
limit?    default: 50, runtime range: 1..500
cursor?
```

`issueSummary` retains aggregate counts even when `maxOutputBytes` removes issue details. Removed details cannot be fetched later.

### `usedu_compare`

Input:

```text
beforeScanId
afterScanId
```

Returns a `usedu.diff.v1` envelope. The comparison uses each envelope's root and retained `entries`, identified by `pathRef`. It does not compare `topFiles` or issue records.

### `usedu_close_scan`

Input:

```text
scanId
```

`closed` is true when a session was removed. Closing a running session also requests cancellation.

## Session retention

- The default maximum is 8 sessions, configurable with `--max-sessions`.
- When full, the least recently updated session is removed before insertion.
- The inactivity TTL is currently fixed at 30 minutes.
- Expiry is checked lazily before the next incoming request, not by a timer.
- Status and query calls refresh the session timestamp.
- A `scanId` becomes invalid after process exit, TTL expiry, eviction, or `usedu_close_scan`.

## Paths and privacy

- `displayName` and `displayPath` are display-only.
- Use `entryId` for references within one scan.
- Use `pathRef` for reversible path identity.
- `redactPaths: true` does not remove `pathRef`.

Because `pathRef` contains reversible Unix path bytes, forwarding it to an untrusted recipient can reveal path information.

## MCP response shape

Every successful `tools/call` returns the tool payload twice:

- `result.structuredContent`: structured data for machine clients;
- `result.content[0].text`: the same value serialized as JSON text.

Treat `scanId`, `entryId`, and cursors as opaque values. They are not durable IDs.

The current server supports stdio only, advertises MCP protocol version `2024-11-05`, reads one JSON-RPC message per input line, and writes one response per output line. Diagnostics go to stderr.

## Error model

| Situation | Result |
| --- | --- |
| Unknown JSON-RPC method | JSON-RPC error `-32601` |
| Invalid argument, unknown tool, invalid cursor, unknown `scanId`, path outside allowlist, query before completion, or fatal synchronous scan error | JSON-RPC error `-32602` |
| Malformed input before dispatch or internal stdio handler failure | JSON-RPC error `-32603`, usually with `id: null` |
| Permission error, filesystem-policy skip, or traversal budget | Successful response with `envelope.issues` and non-complete envelope status |
| Fatal background error | Session `failed` with `message` |
| Observed background cancellation | Session `cancelled` with `message` |

A successful JSON-RPC response does not guarantee a complete filesystem result. Inspect `envelope.status` and `envelope.issueSummary`.

## Current implementation boundaries

- stdio is the only transport;
- allowed roots are fixed at process startup;
- sessions are not durable;
- query tools inspect only the stored envelope and do not rescan;
- output truncation permanently removes data from that session;
- `envelope.nextCursor` exists in JSON v2, but MCP tools do not use it to retrieve truncated envelope data;
- `usedu_list_children` and `usedu_top_entries` use allocated-size ordering rather than the original scan sort;
- importing persistent snapshots or comparing across server restarts through MCP is not implemented.

Related documents:

- [Filesystem Semantics](semantics.md)
- [JSON Contract](json-contract.md)
- [Agent Security Boundary](agent-security.md)
