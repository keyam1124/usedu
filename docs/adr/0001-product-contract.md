# ADR 0001: Product Contract

Status: Accepted

## Context

`usedu` is a macOS disk usage tool for people working in terminals.
It is also intended to become a safe observation engine for AI agents, MCP adapters, and future Rust APIs.
Those interfaces must share one core model instead of growing separate scanning behavior.

The product boundary matters because disk usage tools are often confused with cleanup tools.
`usedu` reports allocated file-system usage.
It does not know which bytes are safely reclaimable.

## Decision

`usedu` is a read-only disk allocation inspection engine.
The CLI report, TUI, future MCP interface, and future Rust API must share the same scanner semantics and domain model.

AI agents are first-class clients, but `usedu` is not agent-only.
Human-readable interfaces and machine-readable interfaces are adapters over the same core behavior.
The core must not depend on terminal rendering, MCP transport, or CLI formatting.

`usedu` must not:

- delete, move, mutate, or quarantine files;
- recommend cleanup actions or assert that an entry is safe to remove;
- become a background daemon by default;
- become a GUI application;
- read file contents for normal scanning;
- treat logical size as an alternate reporting mode.

Snapshot output should use stdout by default:

```bash
usedu snapshot PATH > scan.usedu.json
```

If a later command writes directly to an output file, that write must be documented as an explicit exception to the read-only scanning boundary.

MCP is an adapter.
The protocol-neutral domain model, semantics, and DTO contract must exist before an MCP server is implemented.
A default network daemon is outside the product boundary; an MCP server should use foreground stdio unless a separate ADR changes that boundary.

`Used` means allocated file-system size.
It is not reclaimable bytes.
APFS clones, snapshots, compression, sparse files, and file-provider behavior can make the displayed `Used` value differ from the space recovered after deletion.

## Consequences

Machine-readable output must expose its semantics instead of relying on human labels.
Fields that look like identities must not be lossy display strings.
Errors, partial scans, filesystem-boundary skips, and fast-mode trade-offs must be represented structurally.

Scanner code should remain shared by report and TUI modes and should be shaped so future protocol, snapshot, diff, and MCP adapters can reuse it.
