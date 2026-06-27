# ADR 0001: Product Contract

Status: Accepted and implemented

## Context

`usedu` is a macOS disk usage tool for people working in terminals.
It also serves as a safe observation engine for AI agents, the MCP adapter, and reusable Rust components.
Those interfaces share one core model instead of growing separate scanning behavior.

The product boundary matters because disk usage tools are often confused with cleanup tools.
`usedu` reports allocated filesystem usage. It does not know which bytes are safely reclaimable.

## Decision

`usedu` is a read-only disk allocation inspection engine.

The CLI report, TUI, machine-readable output, snapshots, diffs, MCP interface, and reusable Rust crates share the same scanner semantics and domain model.

AI agents are first-class clients, but `usedu` is not agent-only.
Human-readable and machine-readable interfaces are adapters over the same core behavior.
The core must not depend on terminal rendering, MCP transport, or CLI formatting.

`usedu` must not:

- delete, move, mutate, or quarantine files;
- recommend cleanup actions or assert that an entry is safe to remove;
- become a background daemon by default;
- become a GUI application;
- read file contents for normal scanning;
- treat logical size as an alternate reporting mode.

Snapshot output uses stdout by default:

```bash
usedu snapshot PATH > scan.usedu.json
```

If a later command writes directly to an output file, that write must be documented as an explicit exception to the read-only scanning boundary.

MCP is an adapter over the protocol-neutral domain model and versioned DTO contract.
The implemented server is a foreground stdio process with startup-configured allowed roots and memory-only sessions.
A default network daemon remains outside the product boundary unless a separate ADR changes it.

`Used` means allocated filesystem size, not reclaimable bytes.
APFS clones, snapshots, compression, sparse files, and File Provider behavior can make displayed `Used` differ from the space recovered after deletion.

## Current implementation

The decision is currently realized through:

- `usedu-core` for scanner behavior;
- `usedu-protocol` for JSON v2 and diff DTOs;
- CLI text, JSON v1, JSON v2, and NDJSON adapters;
- stdout snapshot and file-based diff commands;
- `usedu mcp --stdio` for the process-local MCP adapter.

The MCP adapter allows agents to inspect, navigate, explain, and compare stored scan results. It does not add cleanup capabilities.

See [MCP workflows and tool reference](../mcp-tools.md) and [Agent Security Boundary](../agent-security.md).

## Consequences

Machine-readable output exposes its semantics instead of relying on human labels.
Fields that look like identities are not represented only by lossy display strings.
Errors, partial scans, filesystem-boundary skips, output limits, and fast-mode trade-offs are represented structurally.

Scanner code remains shared by report, TUI, snapshot, diff, and MCP adapters.
