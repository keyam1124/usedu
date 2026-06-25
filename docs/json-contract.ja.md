# JSON Machine Interface

[English](json-contract.md) | [日本語](json-contract.ja.md)

この文書は、CLI、snapshot、MCP adapter が現在実装している machine-readable format を説明します。

## 利用できる format

| Command | 用途 |
| --- | --- |
| `usedu report PATH --json` | 互換性のために維持する legacy JSON report |
| `usedu report PATH --format json-v1` | legacy JSON report の明示形式 |
| `usedu report PATH --format json-v2` | machine client 向けの versioned scan envelope |
| `usedu report PATH --format ndjson` | JSON v2 envelope から生成する line-delimited event |
| `usedu snapshot PATH` | full-depth JSON v2 snapshot を stdout に出力 |
| `usedu compare BEFORE AFTER` | 2 つの snapshot file の versioned diff |
| `usedu schema json-v2` | JSON v2 schema を出力 |

machine-readable output の stdout には progress を混ぜません。

## 互換性方針

legacy `--json` format は、すでに外部から観測できる構造であるため、その場で JSON v2 に置き換えません。

新しい integration では、JSON v2、NDJSON、snapshot、または MCP の `structuredContent` を使います。

現在の schema identifier:

```text
usedu.scan.v2
usedu.diff.v1
```

consumer は field semantics を利用する前に `schemaVersion` を確認してください。

## Scan envelope

JSON v2 の top-level structure:

```text
schemaVersion
scanId
status
semantics
effectiveOptions
root
entries
topFiles
issueSummary
issues
nextCursor
```

### `status`

`status.state` は次のいずれかです。

- `complete`
- `partial`
- `cancelled`
- `limitReached`

現在の scanner は、permission error や traversal budget 到達時も、通常は `partial` envelope を正常に生成します。output truncation は `limitReached` です。

`partialReasons` には、`issuesRecorded`、`resourceLimitReached`、`maxOutputEntries`、`maxOutputBytes` などの machine-readable reason が入ります。

### `semantics`

envelope は size calculation rule を記録します。

- `sizeMetric: allocated`
- accounting source
- strict / approximate accuracy
- hard-link policy
- filesystem-boundary policy
- symlink policy
- directory own bytes を含むか
- `reclaimableBytesKnown: false`

client は `usedBytes` を reclaimable space の保証として扱ってはいけません。

### `effectiveOptions`

envelope は、次の解決済み値を記録します。

- mode: `report` または `snapshot`
- depth
- top limit
- file inclusion
- summary mode
- directory-only filter
- sort
- issue detail inclusion
- fast mode
- cross-filesystem policy
- worker count
- output entry / byte limit
- display-path redaction

これにより consumer は、どの範囲を保持した結果か、2 つの result が比較可能かを判断できます。

## Entry と path

`root` は 1 件の `EntryDto` です。`entries` は flat array で、`parentEntryId` から retained tree を復元できます。

各 entry の field:

```text
entryId
parentEntryId
kind
displayName
displayPath
pathRef
usedBytes
ownBytes
uniqueBytes
sharedBytes
counts
complete
issueCountBelow
skippedCountBelow
```

`kind` は `directory`、`regularFile`、`symlink`、`other` のいずれかです。

`counts` は次を分離します。

- regular files
- directories
- symbolic links
- other entries

directory count は、その directory 自身を含みます。

`displayName` と `displayPath` は表示専用で、lossy Unicode conversion を含む場合があります。`pathRef` は Unix path bytes を hexadecimal で保持し、snapshot と diff の可逆的な identity として使います。

`entryId` は 1 回の scan 内で参照するための値であり、durable globally unique ID ではありません。

`uniqueBytes` と `sharedBytes` は schema に存在しますが、現在は `null` です。scanner は hard-link allocation をまだこの 2 field に分離していません。

## Report mode と snapshot mode

同じ envelope type を 2 つの mode で使います。

### Report mode

`usedu report --format json-v2` は `effectiveOptions.mode: "report"` です。

- `depth` は retained tree depth を制御
- `top` は各 retained directory の selected children と `topFiles` を制限
- `includeFiles` は CLI の `--files` で有効化
- `dirsOnly` は ranked tree entries を directory に限定
- issue details は `--errors` の場合だけ含める。aggregate count は常に利用可能

### Snapshot mode

`usedu snapshot` と MCP `usedu_scan` は `effectiveOptions.mode: "snapshot"` です。

- snapshot CLI は、output byte cap で切り詰められない限り full tree と全 file entry を保持
- MCP は tool argument の depth と filter に従って保持
- MCP snapshot mode の `top` は `topFiles` を制限するが、`entries` は制限しない

したがって、MCP の `top` は directory ranking limit ではありません。

## Sort と determinism

protocol layer の entry ordering は、requested sort key と path-byte tie-breaker を使います。

MCP query tool の現在の順序は別です。

- `usedu_list_children`: `usedBytes` 降順
- `usedu_top_entries`: `usedBytes` 降順

これらは original envelope sort をそのまま保持しません。

## Output limit

`maxOutputEntries` は `entries` を切り詰め、`status.state` を `limitReached` にします。

`maxOutputBytes` は serialized size の best-effort target です。target に達するまで entries、top files、issue details の順に削除します。必須 envelope field だけで非常に小さい target を超える場合があります。

truncation は stored result 自体を変更します。MCP query tool は、これらの limit で削除された data を復元できません。

`nextCursor` は envelope truncation または report-mode ranking の続きがあることを表しますが、現在の MCP tool は omitted envelope data を取得する continuation として使いません。

## NDJSON

NDJSON は scan 完了後に JSON v2 envelope から生成します。line-delimited ですが、現在は live traversal stream ではありません。

event sequence:

```text
scanStarted
entry ...
issue ...
scanCompleted
```

各 line に `schemaVersion` と `scanId` が入ります。

## Diff envelope

`usedu compare` と MCP `usedu_compare` は `usedu.diff.v1` を返します。

```text
schemaVersion
status
beforeScanId
afterScanId
summary
changes
```

diff identity は `pathRef` です。各 root と retained `entries` を比較し、`topFiles` と issue records は比較しません。

どちらかの input が complete でない場合、または `semantics` が異なる場合、diff は inexact となります。inexact change は `uncertain` に分類します。

caller は root と `effectiveOptions` も互換にする必要があります。現在の diff implementation は、すべての option mismatch を自動拒否するわけではありません。

## Redaction

CLI machine output は `--redact-paths`、MCP は `redactPaths: true` を使います。

redaction は `displayName` と `displayPath` を `[redacted]` に置き換えます。machine identity を可逆に保つため、`pathRef` は残します。可逆的な path disclosure を許容できない場合は `pathRef` を転送しないでください。

## Schema と test

authoritative JSON v2 schema:

```bash
usedu schema json-v2
```

protocol test は option reflection、entry kind 別 count、非 UTF-8 path identity、output limit、structured scan-budget issue、snapshot diff を検証します。

filesystem accounting term は [ファイルシステム意味論](semantics.ja.md)、MCP 固有の workflow と制約は [AI エージェントから MCP で `usedu` を使う](mcp-tools.ja.md) を参照してください。
