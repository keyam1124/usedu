# MCP tool contract

[English](mcp-tools.md) | [日本語](mcp-tools.ja.md)

`usedu mcp --stdio` は、stdin と stdout で動く foreground MCP adapter を起動します。
diagnostics は stderr に出します。
adapter は、CLI の machine interface と同じ JSON v2 scan envelope を返します。

## session lifecycle

`usedu_scan` は scan session を作成し、`scanId` を返します。
session は memory 上に保持します。
server は TTL で期限切れ session を prune し、session table が上限に達した場合は最も古い session を evict します。

`usedu_close_scan` は session を明示的に削除します。

## tools

### usedu_scan

input:

- `root`: scan 対象 path
- `depth`: retained tree depth
- `top`: ranking-style output の result limit
- `includeFiles`: depth の範囲内で file entry を含める
- `dirsOnly`: directory entry だけを返す
- `sort`: `used`、`name`、`files`、`dirs`
- `fast`: approximate fast scanning を使う
- `crossFileSystems`: mounted filesystem を含める
- `maxOutputEntries`: returned entries の上限
- `redactPaths`: display field を伏せる

output:

- `scanId`
- `schemaVersion`
- `envelope`

### usedu_list_children

input:

- `scanId`
- `entryId`
- `limit`
- `cursor`

output:

- `items`
- `nextCursor`

### usedu_top_entries

input:

- `scanId`
- `limit`
- `kind`
- `minUsedBytes`

output:

- `items`

### usedu_get_issues

input:

- `scanId`
- `limit`
- `cursor`

output:

- `items`
- `nextCursor`

### usedu_compare

input:

- `beforeScanId`
- `afterScanId`

output:

- versioned diff envelope

### usedu_close_scan

input:

- `scanId`

output:

- `closed`

## error model

JSON-RPC method error は JSON-RPC error として返します。
tool-level validation error も JSON-RPC error として返します。
scan 中に見つかった filesystem issue は、scan envelope 内の structured issue として返します。
