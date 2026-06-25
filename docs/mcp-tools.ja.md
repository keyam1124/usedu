# MCP tool contract

[English](mcp-tools.md) | [日本語](mcp-tools.ja.md)

`usedu mcp --stdio` は、stdin と stdout で動く foreground MCP adapter を起動します。
diagnostics は stderr に出します。
adapter は、CLI の machine interface と同じ JSON v2 scan envelope を返します。

## session lifecycle

`usedu_scan` は scan session を作成し、`scanId` を返します。
既定では scan を完了してから返し、scan envelope を含めます。
`background: true` を渡すと、`running` state ですぐに返ります。
client は `usedu_scan_status` で progress を確認し、完了後の envelope を取得します。
session は memory 上に保持します。
server は TTL で期限切れ session を prune し、session table が上限に達した場合は最も古い session を evict します。

`usedu_cancel_scan` は running background scan に cancellation を要求します。
`usedu_close_scan` は session を明示的に削除し、実行中なら cancellation も要求します。

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
- `maxScanEntries`: この entry 数を超えたら traversal を止める
- `maxScanDurationMs`: この duration を超えたら traversal を止める
- `maxOutputEntries`: returned entries の上限
- `maxOutputBytes`: structured section を削って machine output bytes を制限する
- `redactPaths`: display field を伏せる
- `background`: すぐに返し、server process 内で scan を継続する

output:

- `scanId`
- `schemaVersion`
- `state`
- `progress`
- `envelope`

### usedu_scan_status

input:

- `scanId`
- `includeEnvelope`: 完了済みの場合に scan envelope を含める

output:

- `state`: `running`、`complete`、`cancelled`、`failed`
- `progress`
- `envelope`: requested かつ complete の場合
- `message`: failed または cancelled session の場合

### usedu_cancel_scan

input:

- `scanId`

output:

- `cancelRequested`
- `state`
- `progress`

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
