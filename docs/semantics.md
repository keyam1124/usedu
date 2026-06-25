# Filesystem Semantics

[English](semantics.md) | [日本語](semantics.ja.md)

This document defines the accounting terms that human reports and future machine-readable interfaces must share.

## Size Fields

`usedBytes` is the allocated file-system size attributed to an entry and its descendants.
For a directory, it includes `ownBytes` plus retained descendant allocation according to the active accounting policy.
For a leaf entry, it is the allocated size of that entry.

`ownBytes` is the allocated size of the entry itself.
For directories, this is the directory record's own allocation, not the total below it.

`uniqueBytes` is allocation owned uniquely by an entry under the selected hard-link policy.
`sharedBytes` is allocation that is present through shared file identity, such as hard links.
The current scanner does not yet expose these two fields separately; JSON v2 and snapshot formats should.

All size fields describe allocated bytes, not logical byte length and not reclaimable bytes.
The display label for human output remains `Used`.

## Count Fields

Future protocol fields should split leaf counts by kind:

- `regularFileCount`
- `directoryCount`
- `symlinkCount`
- `otherCount`

`directoryCount` includes the directory itself.
For a root directory with one child directory, `directoryCount` is `2`.

The current scanner's `file_count` is broader than regular files.
It counts regular files, symlinks, and other leaf entries together.
JSON v2 should avoid carrying that ambiguity forward.

## Entry Kinds

`directory` is a directory scanned as a container.
`regularFile` is a regular file.
`symlink` is a symbolic link entry.
`other` is a filesystem entry that is not one of the above.

Symbolic links are counted as link entries and are not followed.

## Filesystem Boundary

By default, scanning stays on the device of the requested root.
Entries on other mounted filesystems are skipped unless the user explicitly enables cross-filesystem scanning.

Machine-readable output should expose the effective filesystem-boundary policy.
Skips caused by this policy are not permission errors.

## Hard Links

Strict accounting avoids double-counting a regular file with the same device and inode.
Strict traversal processes directory entries in bytewise path order and assigns hard-link ownership to the first path seen by that deterministic traversal.
Future snapshot formats can still split unique and shared bytes to make hard-link groups more explicit.

Fast mode may over-count hard-linked files.
Machine-readable output must expose this difference as accounting semantics, not only as a performance option.

## Strict And Fast Modes

Strict mode favors accounting consistency.
It uses `symlink_metadata`, includes directory own allocation, avoids following symlinks, and keeps the requested filesystem boundary unless `--cross-file-systems` is set.

Fast mode favors lower latency.
It may omit directory own allocation in some paths, over-count hard links, and cross filesystem boundaries that strict mode would skip.
Machine-readable output should report fast mode as approximate accounting.

## Partial Results

A scan can be complete, partial, cancelled, or limit-reached.
Permission errors and per-entry read failures are partial scan issues, not necessarily fatal command errors.

`error` means the scan could not read or process an entry.
`skip` means the scan intentionally did not include an entry because of policy, such as filesystem-boundary enforcement.
Machine-readable output should distinguish them.

## APFS Caveats

APFS clones, snapshots, compression, sparse files, and file-provider behavior can make allocated bytes differ from bytes reclaimable by deletion.
`usedu` does not certify cleanup safety and does not promise that deleting a file or directory will recover its displayed `Used` value.
