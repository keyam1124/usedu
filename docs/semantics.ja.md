# ファイルシステム意味論

[English](semantics.md) | [日本語](semantics.ja.md)

この文書は、人間向け report、JSON v2、snapshot、diff、MCP tool が共有する accounting term を定義します。

## `Used` の意味

`usedu` が報告するのは、filesystem 上の allocated bytes です。

次の値ではありません。

- logical file length
- 削除後の free-space 増加量
- 安全に reclaim できると保証された byte 数

人間向け output は allocated size を `Used` と表示します。machine output は `sizeMetric: "allocated"` と `reclaimableBytesKnown: false` を記録します。

## Size field

### `usedBytes`

`usedBytes` は entry に帰属する allocated size です。

- directory では、active accounting policy に従った directory own allocation と descendant allocation の合計
- leaf entry では、その entry に帰属する allocation

traversal issue または resource budget により、directory の `usedBytes` が不完全な場合があります。

### `ownBytes`

`ownBytes` は entry 自身の allocation です。

directory では、配下合計ではなく directory record 自身の allocation を表します。strict mode は directory own bytes を含みます。fast mode はこの metadata を省略できるため、0 になる場合があります。

### `uniqueBytes` と `sharedBytes`

JSON v2 は、将来 shared allocation を明示できるように nullable な `uniqueBytes` と `sharedBytes` を持ちます。

現在の scanner は allocation をこの 2 field に分離していないため、どちらも現在は `null` です。hard-link behavior は `semantics.hardLinkPolicy` で表します。

## Count field

JSON v2 は entry count を次に分離します。

- `regularFiles`
- `directories`
- `symlinks`
- `other`

`directories` は対象 directory 自身を含みます。root に child directory が 1 つある場合、`directories` は 2 です。

legacy internal value と JSON v1 の `fileCount` は regular file より広い意味を持ち、regular file、symbolic link、other leaf entry をまとめて数えます。新しい integration は JSON v2 count を使ってください。

## Entry kind

- `directory`: container として走査する directory
- `regularFile`: regular file
- `symlink`: symbolic link entry
- `other`: 上記以外の filesystem entry

symbolic link は link entry として数え、link 先はたどりません。

## Filesystem boundary

strict mode は既定で requested root と同じ device 内にとどまります。別 mounted filesystem の entry は、cross-filesystem traversal を明示的に有効化しない限り policy skip として記録します。

machine output は effective policy を次の値で表します。

- `stayOnRootFilesystem`
- `includeMountedFilesystems`

filesystem-boundary skip は warning/skip であり、permission error ではありません。

fast mode は approximate で、cross-filesystem option が false でも、strict mode なら skip する mounted filesystem を走査する場合があります。したがって effective semantics は `accuracy: "approximate"` と合わせて解釈します。

## Hard link

strict mode は、同じ device と inode を持つ regular file の重複計上を避けます。

strict directory entry は path byte 順で処理し、strict traversal は並列化しません。device/inode identity について最初に出会った path へ allocation を帰属させます。JSON v2 はこの policy を `firstSeenDeviceInode` として報告します。

この方式により strict result は再現しやすくなりますが、shared allocation の合計は別 field として公開しません。`uniqueBytes` と `sharedBytes` は null のままです。

fast mode は hard-linked file を複数回数える場合があり、`hardLinkPolicy: "mayDoubleCount"` を報告します。

## Strict mode

strict mode は accounting consistency を優先します。

- metadata source: Unix `blocks() * 512`
- symbolic link をたどらない
- directory own allocation を含める
- hard link を可能な範囲で重複排除する
- 明示的に解除しない限り filesystem boundary を適用する
- directory traversal は deterministic

JSON v2:

```text
accuracy: strict
accountingSource: unixBlocks512
directoryOwnBytesIncluded: true
```

## Fast mode

fast mode は scan latency を優先し、macOS の bulk metadata API を使う場合があります。

次の挙動が起こり得ます。

- directory own allocation を省略
- hard link を重複計上
- strict mode なら skip する mounted filesystem を traversal

JSON v2:

```text
accuracy: approximate
accountingSource: getattrlistbulkAllocSize
directoryOwnBytesIncluded: false
```

正確な boundary behavior または repeatable accounting を latency より優先する場合は strict mode を使います。

## Complete、partial、limited result

scan envelope は独自の result status を持ちます。

### `complete`

filesystem issue と output truncation が記録されていません。

### `partial`

envelope は生成できたものの、次のような issue で不完全です。

- permission denial
- traversal 中に entry が消失
- filesystem-boundary skip
- `maxScanEntries` または `maxScanDurationMs` 到達

traversal budget 到達時は structured `RESOURCE_LIMIT_REACHED` issue を生成します。

### `limitReached`

scan result は生成できましたが、`maxOutputEntries` または `maxOutputBytes` により serialized section を切り詰めています。

これは traversal の早期停止とは異なります。output truncation は scan 後に retained entry または detail を削り、traversal budget は scan 中の collection を止めます。

MCP session state と envelope status は別です。envelope が存在するため session が `complete` でも、その envelope は `partial` または `limitReached` の場合があります。

## Issue と skip

`error` は entry を読み取りまたは処理できなかったことを意味します。

`skip` は、strict filesystem-boundary enforcement などの policy により意図的に entry を含めなかったことを意味します。

JSON v2 は aggregate count を `issueSummary`、任意の detail を `issues` に出します。MCP client は `usedu_get_issues` で stored detail をページ取得できます。

## APFS と reclaimable space の注意

APFS clone、snapshot、compression、sparse file、File Provider の挙動により、allocated bytes と削除後に回復する bytes は異なる場合があります。

`usedu` は cleanup safety を保証せず、entry を削除すると表示された `Used` が回復するとは約束しません。

JSON field の動作は [JSON Machine Interface](json-contract.ja.md)、agent workflow は [AI エージェントから MCP で `usedu` を使う](mcp-tools.ja.md) を参照してください。
