# 設計ノート

[English](design.md) | [日本語](design.ja.md)

この文書は、`usedu` の変更後も維持する product / implementation constraint を記録します。過去の実装履歴ではありません。

関連文書:

- [ADR 0001: プロダクト契約](adr/0001-product-contract.ja.md)
- [ファイルシステム意味論](semantics.ja.md)
- [JSON Machine Interface](json-contract.ja.md)
- [Agent Security Boundary](agent-security.ja.md)
- [MCP の利用フローと tool リファレンス](mcp-tools.ja.md)

## プロダクト境界

`usedu` は、macOS 向けの読み取り専用 disk allocation inspection tool です。
filesystem 上の allocated space がどこに帰属しているかを調べます。

`usedu` は、ファイルの削除、移動、変更、隔離、cleanup action の推薦を行いません。
GUI、background daemon、duplicate finder、real-time monitor、treemap、logical-size analyzer も対象外です。

CLI、TUI、JSON、snapshot、diff、MCP のすべてで同じ product boundary を維持します。

## コマンドモデル

既定の command は interactive TUI を開きます。

```bash
usedu [PATH]
```

`PATH` を省略した場合は現在のディレクトリを開きます。
`/` を暗黙の既定値にはせず、root volume を走査するには path を明示します。

static report と machine-readable report format は `report` から利用します。

```bash
usedu report [PATH]
usedu report [PATH] --format json-v2
usedu report [PATH] --format ndjson
```

persistent snapshot は stdout に出力し、file への保存は caller の責任とします。

```bash
usedu snapshot [PATH] > scan.usedu.json
usedu compare before.usedu.json after.usedu.json
```

agent interface は foreground stdio adapter です。

```bash
usedu mcp --stdio --allow-root [PATH]
```

network transport と default daemon behavior は product boundary 外です。

## サイズの扱い

`usedu` は filesystem 上の allocated size だけを報告します。
human output の label は常に `Used` です。
logical-size mode や size-mode switch はありません。

strict scan は `symlink_metadata` と Unix allocated block count を使います。

```rust
metadata.blocks().saturating_mul(512)
```

directory total には directory 自身の allocated bytes と、帰属する descendant allocation を含めます。

APFS clone、snapshot、compression、sparse file、File Provider の挙動により、reclaimable space と表示上の `Used` は一致しないことがあります。

machine interface は human label に依存せず、effective accounting semantics を出力します。

## ファイルシステム上の規則

symbolic link は link entry として数えますが、link 先はたどりません。
hidden file と hidden directory も含めます。
`.app` や `.photoslibrary` は通常の directory として扱います。

permission error は記録しますが、scan 全体を中断しません。
詳細 issue は report mode では任意、MCP では output limit の範囲で session に保持します。

strict mode は、cross-filesystem traversal を明示しない限り requested root filesystem 内にとどまります。
fast mode は approximate であり、strict mode なら skip する mounted filesystem を走査する場合があります。

strict mode は同じ device/inode の regular file を可能な範囲で重複排除します。strict entry traversal は deterministic で、first-seen hard-link attribution を再現可能にします。

## TUI の interaction model

TUI は一階層の browser です。
現在の directory では direct children だけを表示します。
ただし direct child directory には、その配下を再帰的に集計した `Used` total を表示します。

たとえば `~/Library` では `Application Support`、`Containers`、`Caches` を表示できます。利用者が `Application Support` を開くまで、`Application Support/A` のような grandchild は表示しません。

この model により、画面の見通しを保ちながら recursive total を提示します。

## MCP の interaction model

MCP server は foreground、process-local の stdio adapter です。

- allowed roots は process 起動時に設定
- `usedu_scan` は in-memory session を作成し、`scanId` を返す
- follow-up tool は stored scan envelope を参照し、自動的には再走査しない
- retained depth、file inclusion、output limit が follow-up query で見える範囲を決める
- session 数には上限があり、inactivity TTL で期限切れとなり、process 終了時に消失する
- background scan は progress と cooperative cancellation を公開する

MCP は agent が scan result を inspection、navigation、explanation、comparison するための interface です。cleanup capability は追加しません。

利用者向けの workflow と現在の制約は [MCP の利用フローと tool リファレンス](mcp-tools.ja.md) に記録します。

## Scanner architecture

scanner code は `usedu-core` crate に分離し、terminal rendering と MCP transport に依存しません。

report mode、TUI mode、snapshot、MCP tool は同じ scanner logic を共有します。
`ScanEngine` は `ScanRequest` を受け取り `ScanOutcome` を返します。collector は scanner に presentation concern を持ち込まず、summary と retained view を作ります。

versioned machine-readable DTO は `usedu-protocol` に置きます。
root `usedu` crate は CLI、TUI、output、snapshot、diff、MCP adapter を提供します。

scanner は `PathBuf` と `OsString` を使い、path が valid UTF-8 であると仮定しません。lossy conversion は display / serialization boundary だけで行います。可逆的な machine identity は `pathRef` の raw Unix path bytes を使います。

scanner は全 file を tree node として保持しません。request に応じて directory summary、retained tree entry、issue record、top-file candidate を保持します。

## 性能方針

scan は metadata だけを読み、ファイル内容は読みません。

progress output は throttle します。
scanner は file ごとに出力せず、traversal 中に display string を組み立てません。
sort は deterministic collection または presentation boundary で行い、scan 中に場当たり的に繰り返しません。

parallelism は bounded worker resource を使います。fast mode は directory subtree を parallelize できますが、strict mode は accounting consistency のため deterministic traversal を維持します。

long-lived adapter は session と worker resource を bounded に保ちます。

## Fast mode

fast mode は strict accounting より scan latency を優先します。
macOS では bulk metadata API を使い、entry ごとの filesystem call を減らせます。
unretained subtree では、各 display node を作らず合計だけを集計できます。

fast mode は directory-own bytes を省略し、hard-linked file を重複計上し、strict mode なら skip する mounted filesystem を traversal する場合があります。
machine output は fast mode を単なる performance flag ではなく approximate semantics として報告します。

## Error behavior

permission error と entry 単位の failure は partial scan issue です。
count と optional structured detail には表しますが、それだけで command または MCP call 全体を必ず失敗にはしません。

traversal budget も structured resource-limit issue を持つ partial envelope を生成します。
output limit は `limitReached` envelope を生成します。

fatal CLI、configuration、transport、runtime error では、non-zero command status または JSON-RPC error を返します。
