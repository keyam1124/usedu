# Agent Security Boundary

[English](agent-security.md) | [日本語](agent-security.ja.md)

`usedu` treats AI agents as first-class clients while keeping the same read-only boundary as the human CLI and TUI.

The MCP server grants an agent permission to inspect filesystem metadata under configured roots. It does not grant permission to modify the filesystem or read file contents.

## What an MCP client is allowed to do

Within an allowed root, an MCP client can:

- scan metadata and allocated sizes;
- retain a scan result in an in-memory session;
- query retained children, top entries, and scan issues;
- compare two in-memory scan sessions;
- monitor and request cancellation of a background scan.

It cannot use `usedu` to delete, move, mutate, quarantine, or recommend files for removal.

## Root allowlist

Start the server with one or more allowed roots:

```bash
usedu mcp --stdio \
  --allow-root "$HOME/Library" \
  --allow-root "$HOME/Projects"
```

If no root is passed, the current directory is the only allowed root.

Allowed roots are fixed when the process starts. The current implementation does not dynamically import MCP client `roots`, and a tool call cannot widen the allowlist.

The server canonicalizes both configured roots and requested scan paths. Requests that resolve outside the allowlist, including through a symbolic link, are rejected before traversal. A nonexistent path that cannot be canonicalized is also rejected.

The allowlist controls where a scan may start. It does not make every descendant readable: normal macOS permissions and Full Disk Access still apply.

## Filesystem traversal policy

Scanning reads metadata only. It does not open file contents for analysis.

Symbolic links are counted as link entries and are not followed.

### Strict mode

`fast: false` is the default and is the appropriate mode when filesystem-boundary behavior matters.

In strict mode:

- directory-own allocated bytes are included;
- regular-file hard links are deduplicated where practical;
- traversal stays on the requested root filesystem unless `crossFileSystems: true` is explicitly passed.

### Fast mode

`fast: true` selects approximate accounting. It may:

- omit directory-own allocation;
- double-count hard-linked files;
- traverse mounted filesystems that strict mode would skip, even when `crossFileSystems` is false.

Therefore, `crossFileSystems: false` is a strict-mode boundary, not a guarantee for fast mode. Agents should keep `fast` disabled when a filesystem boundary is part of the security or accounting requirement.

The startup path allowlist still applies to the requested root. A mounted filesystem reachable below that root can nevertheless be traversed by fast mode.

## Output and path identity

Machine output is structured JSON. File names are returned as data fields, not as instructions embedded in generated prose.

`displayName` and `displayPath` are display-only and can use lossy Unicode conversion. Machine clients should use:

- `entryId` for references within one scan;
- `pathRef` for reversible path identity.

Set `redactPaths: true` in `usedu_scan` to replace `displayName` and `displayPath` with `[redacted]`.

Redaction does **not** remove `pathRef`. `pathRef` contains reversible Unix path bytes, so clients that must hide path identity must avoid forwarding it to untrusted recipients.

A malicious or instruction-like filename remains untrusted data. Clients should consume `structuredContent` and keep filename fields separate from model instructions.

## Session boundary

MCP sessions are process-local and memory-only.

- The default maximum is 8 sessions.
- `--max-sessions` changes the table limit.
- When full, the least recently updated session is evicted.
- Session inactivity TTL is currently fixed at 30 minutes.
- Expired sessions are removed lazily before a later request.
- Closing or evicting a running session requests cancellation.
- All sessions disappear when the MCP process exits.

A `scanId` is not durable authorization and should not be stored as a permanent external identifier.

## Resource controls

A client can bound traversal with:

- `maxScanEntries`
- `maxScanDurationMs`

These are cooperative traversal budgets. Reaching one normally produces a partial scan envelope with a structured `RESOURCE_LIMIT_REACHED` issue; it is not a hard process-level timeout.

A client can bound stored output with:

- `maxOutputEntries`
- `maxOutputBytes`

Output truncation sets the envelope status to `limitReached`. Data removed from the stored envelope cannot be recovered by later MCP queries.

List operations use cursor pagination and clamp page size to the runtime range `1..500`.

Background scans expose progress through `usedu_scan_status` and accept cooperative cancellation through `usedu_cancel_scan` or `usedu_close_scan`.

## Privacy guidance

Use the smallest practical `--allow-root` values. Avoid allowing an entire home directory when the task only needs one project or one Library subtree.

Use `redactPaths: true` when the agent needs size and count information but does not need human-readable paths. Remember that reversible `pathRef` still remains in the structured result.

Use output and traversal budgets when an untrusted or autonomous client can choose scan parameters.

## Result interpretation

A successful MCP call does not guarantee a complete filesystem result.

Clients must inspect:

- the outer session `state`;
- `envelope.status.state`;
- `envelope.status.partialReasons`;
- `envelope.issueSummary` and, when needed, `usedu_get_issues`.

Allocated `Used` bytes are not certified reclaimable bytes. APFS clones, snapshots, compression, sparse files, and File Provider behavior can make deletion recover a different amount of space.

These controls authorize observation only. `usedu` never performs or recommends cleanup actions.

For user workflows and individual tools, see [Use `usedu` from an AI agent over MCP](mcp-tools.md).
