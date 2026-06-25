# Filesystem Semantics

[English](semantics.md) | [日本語](semantics.ja.md)

This document defines the accounting terms shared by human reports, JSON v2, snapshots, diffs, and MCP tools.

## What `Used` means

`usedu` reports allocated filesystem bytes.

It does not report:

- logical file length;
- free-space change after deletion;
- bytes that are certified safe to reclaim.

Human output labels allocated size as `Used`. Machine output records `sizeMetric: "allocated"` and `reclaimableBytesKnown: false`.

## Size fields

### `usedBytes`

`usedBytes` is the allocated size attributed to an entry.

- For a directory, it is the directory's own allocation plus attributed descendant allocation under the active accounting policy.
- For a leaf entry, it is the allocation attributed to that entry.

A directory's `usedBytes` can be incomplete when traversal records issues or stops at a resource budget.

### `ownBytes`

`ownBytes` is the allocation of the entry itself.

For a directory, this means the directory record's own allocation, not the total below it. Strict mode includes directory-own bytes. Fast mode may report zero because it can omit this metadata.

### `uniqueBytes` and `sharedBytes`

JSON v2 includes nullable `uniqueBytes` and `sharedBytes` fields so the protocol can later represent shared allocation explicitly.

The current scanner does not split allocation into those fields, so both are currently `null`. Hard-link behavior is described by `semantics.hardLinkPolicy` instead.

## Count fields

JSON v2 separates entry counts into:

- `regularFiles`
- `directories`
- `symlinks`
- `other`

`directories` includes the directory itself. A root with one child directory therefore has `directories: 2`.

Legacy internal and JSON v1 `fileCount` values are broader than regular files: they count regular files, symbolic links, and other leaf entries together. New integrations should use JSON v2 counts.

## Entry kinds

- `directory`: a directory scanned as a container;
- `regularFile`: a regular file;
- `symlink`: a symbolic link entry;
- `other`: an entry that is none of the above.

Symbolic links are counted as link entries and are not followed.

## Filesystem boundary

Strict mode stays on the device of the requested root by default. Entries on another mounted filesystem are recorded as policy skips unless cross-filesystem traversal is explicitly enabled.

Machine output exposes the effective policy as:

- `stayOnRootFilesystem`; or
- `includeMountedFilesystems`.

A filesystem-boundary skip is a warning/skip, not a permission error.

Fast mode is approximate and may traverse mounted filesystems that strict mode would skip, even when the cross-filesystem option is false. The effective semantics therefore must be read together with `accuracy: "approximate"`.

## Hard links

Strict mode avoids double-counting regular files with the same device and inode.

Strict directory entries are processed in bytewise path order, and strict traversal is not parallelized. Allocation is assigned to the first path encountered for a device/inode identity. JSON v2 reports this policy as `firstSeenDeviceInode`.

This keeps strict results repeatable, but it does not expose a separate shared-allocation total. `uniqueBytes` and `sharedBytes` remain null.

Fast mode may count hard-linked files more than once and reports `hardLinkPolicy: "mayDoubleCount"`.

## Strict mode

Strict mode favors accounting consistency.

- metadata source: Unix `blocks() * 512`;
- symbolic links are not followed;
- directory-own allocation is included;
- hard links are deduplicated where practical;
- filesystem boundaries are enforced unless explicitly disabled;
- directory traversal is deterministic.

JSON v2 reports:

```text
accuracy: strict
accountingSource: unixBlocks512
directoryOwnBytesIncluded: true
```

## Fast mode

Fast mode favors lower scan latency and can use macOS bulk metadata APIs.

It may:

- omit directory-own allocation;
- double-count hard links;
- traverse mounted filesystems that strict mode would skip.

JSON v2 reports:

```text
accuracy: approximate
accountingSource: getattrlistbulkAllocSize
directoryOwnBytesIncluded: false
```

Use strict mode when exact boundary behavior or repeatable accounting is more important than latency.

## Complete, partial, and limited results

A scan envelope has its own result status.

### `complete`

No filesystem issue or output truncation was recorded.

### `partial`

An envelope was produced, but it is incomplete because of issues such as:

- permission denial;
- an entry disappearing during traversal;
- filesystem-boundary skipping;
- `maxScanEntries` or `maxScanDurationMs` being reached.

Traversal budgets produce a structured `RESOURCE_LIMIT_REACHED` issue.

### `limitReached`

The scan result was produced, but serialized sections were truncated by `maxOutputEntries` or `maxOutputBytes`.

This is different from traversal stopping early. Output truncation removes retained entries or details after scanning; traversal budgets stop collection during scanning.

MCP session state is separate from envelope status. A session can be `complete` because an envelope exists while that envelope is `partial` or `limitReached`.

## Issues and skips

An `error` means an entry could not be read or processed.

A `skip` means policy intentionally excluded an entry, such as strict filesystem-boundary enforcement.

JSON v2 exposes aggregate counts in `issueSummary` and optional details in `issues`. MCP clients can page through stored details with `usedu_get_issues`.

## APFS and reclaimable-space caveats

APFS clones, snapshots, compression, sparse files, and File Provider behavior can make allocated bytes differ from bytes recovered by deletion.

`usedu` does not certify cleanup safety and does not promise that deleting an entry will recover its displayed `Used` value.

For JSON field behavior, see [JSON Machine Interface](json-contract.md). For agent workflows, see [Use `usedu` from an AI agent over MCP](mcp-tools.md).
