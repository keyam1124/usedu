# JSON Contract Repair Plan

[English](json-contract.md) | [日本語](json-contract.ja.md)

This document records the B01 implementation plan.
It does not change current runtime behavior.

## Current Contract Problem

`usedu report --json` serializes a human report-shaped structure.
Several report options are already accepted by the CLI but are not represented consistently in JSON output.
For an agent-facing interface, silently ignored options are unsafe.

The current JSON output also uses display paths as identity-like fields.
That is acceptable only as display data because lossy UTF-8 conversion can change non-UTF-8 paths.

## Compatibility Strategy

Keep the current `--json` output as the current JSON format until a migration is implemented.
Add an explicit machine format instead of changing the existing format in place:

```bash
usedu report PATH --format json-v2
usedu report PATH --format ndjson
usedu schema json-v2
```

The current `--json` flag can later become a documented alias for the current format or emit a migration warning on stderr.
It must not mix progress or diagnostics into stdout.

## JSON v2 Envelope

JSON v2 should use a versioned envelope:

```text
schemaVersion
scanId
status
semantics
effectiveOptions
root
entries
issueSummary
issues
nextCursor
```

`semantics` must include:

- `sizeMetric: allocated`
- accounting source
- accounting accuracy
- hard-link policy
- filesystem-boundary policy
- symlink policy
- whether directory own bytes are included
- `reclaimableBytesKnown: false`

`effectiveOptions` must include the resolved values for depth, top limit, file inclusion, directory-only filtering, sort, fast mode, cross-filesystem policy, and output limits.

## Option Handling

JSON v2 must not silently ignore report options.

- `--top` limits ranking-style result sets and top-file result sets.
- `--sort` controls deterministic ordering where sorting is requested.
- `--dirs-only` filters ranked entries to directories.
- `--files` includes top-file entries in the structured result.
- `--depth` controls retained tree depth only where a tree view is requested; flat entry lists should use pagination or explicit limits.
- `--errors` includes issue details; issue counts are always available.
- unsupported option combinations return a structured CLI error before scanning.

## Identity And Paths

`displayPath` and `displayName` are display-only.
JSON v2 should use `entryId` as the scan-local reference and `pathRef` for reversible path identity in snapshots.
Lossy strings must never be the only identity.

## Tests

B01 should add golden tests for:

- current JSON compatibility;
- JSON v2 envelope fields;
- option reflection in `effectiveOptions`;
- `--top`, `--sort`, `--dirs-only`, `--files`, and `--depth`;
- structured issues;
- non-UTF-8 and control-character path display safety.
