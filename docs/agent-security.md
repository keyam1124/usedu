# Agent Security Boundary

[English](agent-security.md) | [日本語](agent-security.ja.md)

`usedu` treats AI agents as first-class clients, but it keeps the same read-only product boundary for human and machine interfaces.

## Root Allowlist

The MCP server accepts one or more allowed roots:

```bash
usedu mcp --stdio --allow-root ~/Library
```

If no root is passed, the current directory is the only allowed root.
MCP scan requests are canonicalized before scanning.
Requests outside the allowlist are rejected before traversal.
This also rejects symlink-based escape from an allowed root.

The normal CLI remains explicit-path based.
`usedu` still never defaults to `/`.

## Filesystem Policy

Scanning uses metadata only.
It does not read file contents.

Symbolic links are counted as link entries and are not followed.
By default, scanning stays on the requested root filesystem.
Cross-filesystem scanning requires `--cross-file-systems` or the equivalent MCP argument.

## Output Boundary

Machine-readable output is structured JSON.
File names are returned as fields, not as instructions embedded in prose.

`displayName` and `displayPath` are display-only.
Machine clients should use `entryId` inside one scan and `pathRef` for reversible path identity.

Use `--redact-paths` for CLI machine output or `redactPaths: true` for MCP scans to redact display fields.
`pathRef` remains present because it is the machine identity.
Callers that must hide reversible path bytes should not forward `pathRef` to untrusted recipients.

## Resource Controls

MCP sessions have a bounded in-process session table and a TTL.
Large list operations use cursor pagination and clamp page size.
Scan output can be limited with `maxOutputEntries` and `maxOutputBytes`.
Scan traversal can be bounded with `maxScanEntries` and `maxScanDurationMs`.
Background MCP scans expose progress through `usedu_scan_status` and can be stopped with `usedu_cancel_scan` or by closing the session.

These controls do not authorize cleanup actions.
`usedu` never deletes, moves, quarantines, or recommends files for removal.
