# MCP Tool Contract

[English](mcp-tools.md) | [日本語](mcp-tools.ja.md)

`usedu mcp --stdio` runs a foreground MCP adapter over stdin and stdout.
Diagnostics must go to stderr.
The adapter returns the same JSON v2 scan envelope used by the CLI machine interface.

## Session Lifecycle

`usedu_scan` creates a scan session and returns `scanId`.
Sessions are held in memory.
The server prunes expired sessions by TTL and evicts the oldest session when the session table reaches its limit.

`usedu_close_scan` explicitly removes a session.

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
- `redactPaths`: redact display fields.

Output:

- `scanId`
- `schemaVersion`
- `envelope`

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
