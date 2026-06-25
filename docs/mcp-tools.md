# MCP Tool Contract

[English](mcp-tools.md) | [日本語](mcp-tools.ja.md)

`usedu mcp --stdio` runs a foreground MCP adapter over stdin and stdout.
Diagnostics must go to stderr.
The adapter returns the same JSON v2 scan envelope used by the CLI machine interface.

## Session Lifecycle

`usedu_scan` creates a scan session and returns `scanId`.
By default it completes the scan before returning and includes the scan envelope.
When `background: true` is passed, it returns immediately with state `running`; clients use `usedu_scan_status` to observe progress and retrieve the completed envelope.
Sessions are held in memory.
The server prunes expired sessions by TTL and evicts the oldest session when the session table reaches its limit.

`usedu_cancel_scan` requests cancellation for a running background scan.
`usedu_close_scan` explicitly removes a session and also cancels it if it is still running.

## Tools

### usedu_scan

Input:

- `root`: path to scan.
- `depth`: retained tree depth.
- `top`: result limit for ranking-style output.
- `includeFiles`: include file entries where retained by depth.
- `dirsOnly`: return directory entries only.
- `sort`: `used`, `name`, `files`, or `dirs`.
- `fast`: use approximate fast scanning.
- `crossFileSystems`: include mounted filesystems.
- `maxScanEntries`: stop traversal after this many entries.
- `maxScanDurationMs`: stop traversal after this duration.
- `maxOutputEntries`: cap returned entries.
- `maxOutputBytes`: cap returned machine-output bytes by truncating structured sections.
- `redactPaths`: redact display fields.
- `background`: return immediately and continue the scan in the server process.

Output:

- `scanId`
- `schemaVersion`
- `state`
- `progress`
- `envelope`

### usedu_scan_status

Input:

- `scanId`
- `includeEnvelope`: include the completed scan envelope when available.

Output:

- `state`: `running`, `complete`, `cancelled`, or `failed`
- `progress`
- `envelope` when requested and complete
- `message` for failed or cancelled sessions

### usedu_cancel_scan

Input:

- `scanId`

Output:

- `cancelRequested`
- `state`
- `progress`

### usedu_list_children

Input:

- `scanId`
- `entryId`
- `limit`
- `cursor`

Output:

- `items`
- `nextCursor`

### usedu_top_entries

Input:

- `scanId`
- `limit`
- `kind`
- `minUsedBytes`

Output:

- `items`

### usedu_get_issues

Input:

- `scanId`
- `limit`
- `cursor`

Output:

- `items`
- `nextCursor`

### usedu_compare

Input:

- `beforeScanId`
- `afterScanId`

Output:

- versioned diff envelope.

### usedu_close_scan

Input:

- `scanId`

Output:

- `closed`

## Error Model

JSON-RPC method errors are returned as JSON-RPC errors.
Tool-level validation errors also return JSON-RPC errors.
Filesystem issues discovered during scanning are returned inside the scan envelope as structured issues.
