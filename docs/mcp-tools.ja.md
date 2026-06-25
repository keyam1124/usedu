# AI エージェントから MCP で `usedu` を使う

[English](mcp-tools.md) | [日本語](mcp-tools.ja.md)

`usedu mcp --stdio` は、AI エージェントなどの MCP クライアントから、許可したディレクトリのディスク使用状況を読み取り専用で調べるためのインターフェースです。

MCP を使う主な価値は、単に `usedu` を起動することではありません。エージェントが走査結果をセッションとして保持し、その結果に対して「大きい場所を探す」「子ディレクトリを掘り下げる」「問題を確認する」「2 回の走査を比較する」といった追加の問い合わせを行える点にあります。

## MCP で利用者が実現できること

| 利用者の目的 | エージェントが行う処理 | 主に使う tool |
| --- | --- | --- |
| どのディレクトリが容量を使っているか把握する | 許可済みルートを走査し、容量上位の保持済みエントリを抽出する | `usedu_scan`、`usedu_top_entries` |
| 大きなファイルを探す | 通常ファイルを含めて走査し、走査全体から収集した `topFiles` を読む | `usedu_scan` |
| 特定ディレクトリを掘り下げる | 保存済み結果から直下の子をページ単位で取得する | `usedu_list_children` |
| 結果が不完全な理由を確認する | 権限エラー、別ファイルシステムのスキップ、走査上限到達などを取得する | `usedu_get_issues` |
| 長い走査の進捗を確認・停止する | バックグラウンド走査を開始し、進捗をポーリングして必要ならキャンセルする | `usedu_scan_status`、`usedu_cancel_scan` |
| 2 時点で何が増減したか確認する | 同じサーバープロセス内の 2 つの走査セッションを比較する | `usedu_compare` |
| エージェントに見せるパス情報を減らす | 表示名と表示パスを伏せて走査する | `redactPaths: true` |

利用者は、たとえばエージェントに次のように依頼できます。

- 「`~/Library` の中で容量を多く使っているディレクトリを上位 10 件調べて」
- 「このプロジェクト配下で大きい通常ファイルを探して」
- 「`Application Support` の直下を容量順に見せて」
- 「走査結果が partial になった理由を説明して」
- 「この処理は長そうなのでバックグラウンドで走査し、必要なら止めて」
- 「同じディレクトリを変更前後で走査し、増えた場所を調べて」

## MCP で実現しないこと

MCP を使っても、`usedu` のプロダクト境界は変わりません。

- ファイルやディレクトリを削除、移動、変更、隔離しない
- 「安全に削除できる」と判定しない
- cleanup 候補を推薦しない
- ファイル内容を読まない
- allowlist 外を走査しない
- 表示された `Used` が削除後に解放される容量だとは保証しない
- リアルタイム監視やバックグラウンド daemon として常駐しない

MCP セッションはメモリ上だけに存在し、サーバー終了後には残りません。再起動をまたぐ比較には、CLI の snapshot を使います。

```bash
usedu snapshot PATH > before.usedu.json
usedu snapshot PATH > after.usedu.json
usedu compare before.usedu.json after.usedu.json
```

## 基本的な仕組み

```text
1. サーバー起動時に、走査を許可するルートを指定する
2. usedu_scan で走査し、scanId を受け取る
3. scanId を使って、保存済み結果へ追加の問い合わせを行う
4. 不要になったセッションを usedu_close_scan で破棄する
```

後続の query tool は、保存済みの走査結果を参照します。ファイルシステムを再走査しません。

```text
envelope.root       走査ルート
envelope.entries    depth と filter に従って保持したエントリ
envelope.topFiles   走査全体から収集した大きな通常ファイル
envelope.issues     ファイルシステム上の問題や走査上限の詳細
```

このため、最初の `usedu_scan` で指定した `depth`、`includeFiles`、出力上限が、後続の問い合わせで見える範囲を決めます。省略または切り詰められたデータは、後から復元できません。

## サーバーを起動する

```bash
usedu mcp --stdio \
  --allow-root "$HOME/Library" \
  --allow-root "$HOME/Projects" \
  --max-sessions 8
```

| オプション | 既定値 | 動作 |
| --- | --- | --- |
| `--stdio` | 必須 | stdio MCP サーバーを起動します。HTTP transport は未実装です。 |
| `--allow-root PATH` | 現在のディレクトリ | 複数回指定できます。正規化後にこの配下となる既存パスだけを走査できます。 |
| `--max-sessions N` | `8` | プロセス内に保持するセッション数の上限です。`1` 未満は `1` として扱います。 |

許可ルートはサーバー起動時に固定されます。現在の実装は、MCP client の `roots` を動的には取り込みません。

許可ルートと走査対象は `canonicalize` されます。シンボリックリンク経由を含め、解決後のパスが allowlist 外になる要求は走査前に拒否されます。存在しないパスも拒否されます。

## 目的別の使い方

### 容量を使っているディレクトリを把握する

最初に、ディレクトリを十分な深さまで保持して走査します。

```json
{
  "root": "/Users/example/Library",
  "depth": 2,
  "includeFiles": false,
  "sort": "used"
}
```

続いて `usedu_top_entries` を `kind: "directory"` で呼び出すと、保持済み階層全体から容量上位のディレクトリを取得できます。

注意点として、`usedu_scan` の `top` は MCP では `envelope.topFiles` の件数を制限します。`envelope.entries` やディレクトリ上位件数を制限する引数ではありません。ディレクトリの上位件数は `usedu_top_entries.limit` で指定します。

### 大きな通常ファイルを探す

```json
{
  "root": "/Users/example/Projects",
  "depth": 1,
  "includeFiles": true,
  "top": 50
}
```

完了した `envelope.topFiles` には、保持深度にかかわらず走査全体から収集した大きな通常ファイルが最大 50 件入ります。

`usedu_top_entries` で `kind: "regularFile"` を指定した場合は意味が異なります。こちらは `envelope.entries` に保持された通常ファイルだけを順位付けします。

### ディレクトリを掘り下げる

`usedu_list_children` に親の `entryId` を渡します。ルートの ID は `envelope.root.entryId` です。

```json
{
  "scanId": "mcp_scan_...",
  "entryId": "entry_...",
  "limit": 50
}
```

最初の走査で必要な深さを保持していなかった場合、後から子を復元することはできません。その場合は、対象ディレクトリを新しい `root` として `usedu_scan` し直します。対象は起動時の allowlist 内である必要があります。

### 長い走査をバックグラウンドで実行する

```json
{
  "root": "/Users/example/Library",
  "background": true,
  "maxScanDurationMs": 60000
}
```

初回応答は `state: "running"` で、まだ `envelope` を含みません。`usedu_scan_status` を呼び、完了結果も必要なら `includeEnvelope: true` を指定します。

`usedu_cancel_scan` は協調的なキャンセル要求です。直後の応答がまだ `running` でも異常ではありません。状態が変わるまで status を確認します。

同期走査中は stdio request loop が次の要求を処理できないため、進捗確認やキャンセルが必要な走査では `background: true` を使います。

### 2 回の走査を比較する

`usedu_compare` は、同じサーバープロセス内に保持された 2 つの走査結果を比較します。

現在の同期走査 ID はルートパスと集計値から生成されるため、同じルートでサイズと件数が変わらない 2 回の同期走査は同じ `scanId` となり、先のセッションを置き換える場合があります。before/after 比較では、異なるプロセス内 ID が割り当てられる `background: true` の走査を 2 回使うのが安全です。

比較する 2 回の走査では、少なくとも次を揃えます。

- `root`
- `depth`
- `includeFiles`
- `dirsOnly`
- `fast`
- `crossFileSystems`
- 出力上限

片方が partial または `limitReached`、あるいは accounting semantics が異なる場合、diff は `exact: false` となり、変更は `uncertain` として扱われます。すべての option mismatch が自動検出されるわけではありません。

### 不完全な結果の理由を確認する

`envelope.status.state` が `partial` または `limitReached` の場合は、次を確認します。

- `envelope.status.partialReasons`
- `envelope.issueSummary`
- `usedu_get_issues`

issue には、権限エラー、走査中の消失、別ファイルシステムのスキップ、走査上限到達などが含まれます。

## 2 種類の状態

MCP の外側の `state` と、走査結果内の `envelope.status.state` は別物です。

### セッションの `state`

| 値 | 意味 | `envelope` |
| --- | --- | --- |
| `running` | バックグラウンド走査中 | なし |
| `complete` | 走査処理が終了し、結果を保存済み | あり |
| `cancelled` | バックグラウンド走査がキャンセルを検知 | 現在の実装ではなし |
| `failed` | バックグラウンド走査が致命的エラーで終了 | なし |

### `envelope.status.state`

| 値 | 意味 |
| --- | --- |
| `complete` | 走査 issue と出力切り詰めが記録されていない |
| `partial` | 結果は生成できたが、権限エラーや走査上限などにより不完全 |
| `limitReached` | `maxOutputEntries` または `maxOutputBytes` で出力を切り詰めた |

したがって、セッションが `complete` でも、`envelope` は `partial` または `limitReached` の場合があります。

## Tool 一覧

| Tool | できること |
| --- | --- |
| `usedu_scan` | 同期またはバックグラウンドで走査し、セッションを作成する |
| `usedu_scan_status` | バックグラウンド走査の進捗と完了結果を取得する |
| `usedu_cancel_scan` | 実行中の走査にキャンセルを要求する |
| `usedu_list_children` | 保持済み結果から直下の子をページ単位で取得する |
| `usedu_top_entries` | 保持済みエントリ全体を容量順に取得する |
| `usedu_get_issues` | 保存済み issue details をページ単位で取得する |
| `usedu_compare` | 2 つの保存済み走査結果を比較する |
| `usedu_close_scan` | セッションを削除し、実行中ならキャンセルも要求する |

## Tool リファレンス

### `usedu_scan`

主な input:

| Field | 型・既定値 | 実際の動作 |
| --- | --- | --- |
| `root` | string、必須 | 既存パス。正規化後に起動時 allowlist 内である必要があります。 |
| `depth` | `0` 以上、既定 `1` | 走査深度ではなく保持深度です。`0` は root のみ、`1` は直下まで保持します。集計のための traversal 自体は子孫まで行います。 |
| `top` | `0` 以上、既定 `30` | `envelope.topFiles` の上限です。MCP の `entries` 件数は制限しません。 |
| `includeFiles` | boolean、既定 `false` | 保持深度内の通常ファイル、symlink、other を `entries` に含め、`topFiles` の収集を有効にします。 |
| `dirsOnly` | boolean、既定 `false` | `entries` を directory のみにします。`includeFiles: true` のときの `topFiles` は消しません。 |
| `sort` | `used` / `name` / `files` / `dirs`、既定 `used` | envelope 構築時の順序です。後続 query tool は別の並び順を使います。 |
| `fast` | boolean、既定 `false` | approximate accounting。directory own bytes の省略、hard link の重複計上、strict なら skip する mount の traversal が起こり得ます。 |
| `crossFileSystems` | boolean、既定 `false` | strict mode で mount 配下を含めます。fast mode は false でも filesystem boundary を越える場合があります。 |
| `maxScanEntries` | positive integer、任意 | traversal budget。到達時は `RESOURCE_LIMIT_REACHED` を含む partial envelope を返します。 |
| `maxScanDurationMs` | positive integer、任意 | traversal 中に確認する協調的な時間上限です。hard timeout ではありません。 |
| `maxOutputEntries` | non-negative integer、任意 | 保存する `envelope.entries` を切り詰め、status を `limitReached` にします。 |
| `maxOutputBytes` | non-negative integer、任意 | serialized size の best-effort 上限です。entries、topFiles、issues の順で末尾から削ります。必須 field だけで上限を超える場合があります。 |
| `redactPaths` | boolean、既定 `false` | `displayName` と `displayPath` を `[redacted]` にします。可逆的な `pathRef` は残ります。 |
| `background` | boolean、既定 `false` | worker thread で走査し、完了前に session を返します。 |

MCP scan は常に snapshot mode の envelope を作ります。`effectiveOptions.mode` は `snapshot` です。MCP tool は `jobs` 引数を公開しておらず、scanner default を使います。

### `usedu_scan_status`

Input:

```text
scanId
includeEnvelope?   default: false
```

`includeEnvelope: true` でも、session が `complete` の場合だけ `envelope` を返します。呼び出すと session の最終利用時刻が更新されます。

### `usedu_cancel_scan`

Input:

```text
scanId
```

`cancelRequested` は、呼び出し時点で session が `running` だった場合だけ `true` です。worker の停止完了を意味しません。

### `usedu_list_children`

Input:

```text
scanId
entryId
limit?    default: 50, runtime range: 1..500
cursor?
```

現在の動作:

- `envelope.entries` に保持済みの direct children だけを返す
- `usedu_scan.sort` にかかわらず `usedBytes` 降順
- 不明な `entryId` や leaf の `entryId` は error ではなく空配列
- `depth` や出力上限で省略した entry は取得不能

### `usedu_top_entries`

Input:

```text
scanId
limit?          default: 50, runtime range: 1..500
kind?           directory | regularFile | symlink | other
minUsedBytes?   default: 0
```

root を除く `envelope.entries` 全体を `usedBytes` 降順で返します。cursor pagination はありません。`envelope.topFiles` は参照しません。

### `usedu_get_issues`

Input:

```text
scanId
limit?    default: 50, runtime range: 1..500
cursor?
```

`maxOutputBytes` によって issue details が削られていても、`issueSummary` の集計値は残ります。ただし削除済み details は後から取得できません。

### `usedu_compare`

Input:

```text
beforeScanId
afterScanId
```

`usedu.diff.v1` envelope を返します。比較対象は各 envelope の root と retained `entries` です。`topFiles` と issues は比較しません。

### `usedu_close_scan`

Input:

```text
scanId
```

`closed` は session を削除できた場合に `true` です。実行中 session を閉じるとキャンセルも要求します。

## セッションの保持期間

- 既定の最大数は 8。`--max-sessions` で変更可能
- 上限到達時は、最終利用時刻が最も古い session を削除
- inactivity TTL は現在 30 分固定
- TTL cleanup は専用 timer ではなく、次の request の前に遅延実行
- status と query tool の呼び出しで最終利用時刻を更新
- process 終了、TTL、eviction、close の後は `scanId` を利用できない

## パスとプライバシー

- `displayName` と `displayPath` は表示専用
- 1 回の scan 内の参照には `entryId` を使う
- 可逆的な path identity には `pathRef` を使う
- `redactPaths: true` でも `pathRef` は残る

`pathRef` は Unix path bytes を可逆的に表現するため、信頼できない相手へ転送するとパス情報が漏れる可能性があります。

## 応答形式

成功した `tools/call` は tool 固有の payload を 2 か所へ返します。

- `result.structuredContent`: machine client が使う構造化値
- `result.content[0].text`: 同じ値を JSON text にした互換表現

`scanId`、`entryId`、cursor は opaque value として扱います。永続 ID ではありません。

現在のサーバーは stdio のみを実装し、MCP protocol version `2024-11-05` を通知します。stdin から 1 行 1 JSON-RPC message を受け取り、stdout に 1 行 1 response を返します。diagnostics は stderr に出します。

## Error model

| 状況 | 結果 |
| --- | --- |
| 未知の JSON-RPC method | JSON-RPC error `-32601` |
| 不正 argument、未知 tool、invalid cursor、unknown `scanId`、allowlist 外、完了前 query、同期走査の fatal error | JSON-RPC error `-32602` |
| dispatch 前の malformed input または内部 stdio handler failure | JSON-RPC error `-32603`、通常 `id: null` |
| permission error、filesystem policy skip、traversal budget | successful tool response 内の `envelope.issues` と non-complete status |
| background scan の fatal error | session `failed` と `message` |
| background cancellation を検知 | session `cancelled` と `message` |

JSON-RPC が成功しても、ファイルシステム走査が完全とは限りません。`envelope.status` と `envelope.issueSummary` を確認してください。

## 現在の実装上の制約

- transport は stdio のみ
- allowed roots は process 起動時に固定
- session は非永続
- query tool は再走査せず、stored envelope だけを見る
- output truncation で失われた data は session 内でも復元不能
- `envelope.nextCursor` は JSON v2 field だが、MCP query tool は truncated envelope の続きを取得する用途には使わない
- `usedu_list_children` と `usedu_top_entries` は元の scan sort ではなく `usedBytes` 順
- 永続 snapshot の import や、process 再起動をまたぐ MCP compare は未実装

関連文書:

- [ファイルシステム意味論](semantics.ja.md)
- [JSON 契約](json-contract.ja.md)
- [Agent Security Boundary](agent-security.ja.md)
