# MCP stdio インターフェース

[English](mcp-tools.md) | [日本語](mcp-tools.ja.md)

`usedu mcp --stdio` は、標準入力と標準出力を使う、フォアグラウンド動作の読み取り専用 MCP サーバーです。
走査結果をメモリ上のセッションとして保持するため、MCP クライアントは問い合わせのたびにファイルシステムを再走査せず、保存済みの結果を参照できます。

この文書でいう `envelope` は、走査結果本体を格納するJSONオブジェクトです。現在の実装は、走査結果にschema version `usedu.scan.v2`、比較結果に `usedu.diff.v1` を使います。

現在の実装には、次の特徴があります。

- トランスポートはstdioのみ
- MCPプロトコルバージョン `2024-11-05` を通知
- 1行につき1件のJSON-RPCメッセージを受け取り、応答も1行で返す
- stdoutはプロトコルメッセージ専用、診断出力はstderr
- セッションはすべてプロセス内のメモリに保持し、プロセス終了時に失われる
- ファイルの削除、移動、隔離、削除候補の推薦は行わない

## まず理解しておくべき流れ

```text
1. サーバー起動時に、走査を許可するルートを指定する
2. usedu_scanで走査し、scanIdを受け取る
3. scanIdを使って、保存済み結果の子要素・上位要素・問題一覧を参照する
4. 不要になったセッションをusedu_close_scanで削除する
```

`usedu_list_children`、`usedu_top_entries`、`usedu_get_issues`、`usedu_compare` は、保存済みの走査結果を参照するtoolです。これらのtoolはファイルシステムを再走査しません。

## サーバーの起動

```bash
usedu mcp --stdio \
  --allow-root "$HOME/Library" \
  --allow-root "$HOME/Projects" \
  --max-sessions 8
```

| オプション | 既定値 | 動作 |
| --- | --- | --- |
| `--stdio` | 必須 | stdioサーバーを起動します。HTTPトランスポートは実装されていません。 |
| `--allow-root PATH` | 現在のディレクトリ | 複数回指定できます。正規化した許可ルート配下の既存パスだけを走査できます。 |
| `--max-sessions N` | `8` | プロセス内に保持するセッション数の上限です。`1` 未満は `1` として扱います。 |

許可ルートはサーバー起動時に固定されます。toolの引数から許可範囲を広げることはできません。現在の実装は、MCPクライアントが提示する `roots` を動的には取り込みません。

許可ルートと走査要求のパスは、どちらも正規化（canonicalize）されます。シンボリックリンク経由を含め、解決後のパスが許可範囲外になる要求は、走査開始前に拒否されます。対象パスが存在せず正規化できない場合もエラーになります。

パスの識別、表示パスの伏字化、ファイルシステム境界、リソース制御については、[Agent Security Boundary](agent-security.ja.md)も参照してください。

## `tools/call` 応答の構造

`tools/call` が成功すると、toolごとの結果が次の2か所に入ります。

- `result.structuredContent`: クライアント実装が利用すべき構造化データ
- `result.content[0].text`: 同じ値をJSON文字列にした、テキスト中心のクライアント向け表現

省略した応答例:

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"scanId\":\"scan_...\",\"state\":\"complete\",...}"
      }
    ],
    "structuredContent": {
      "scanId": "scan_...",
      "state": "complete",
      "envelope": {
        "schemaVersion": "usedu.scan.v2",
        "status": { "state": "complete", "partialReasons": [] }
      }
    }
  }
}
```

`scanId`、`entryId`、cursorは、中身を解釈しない不透明な値として扱ってください。永続的な外部IDではありません。

同期走査の `scanId` は、ルートパスと集計後のサイズ・件数から生成されます。そのため、グローバルに一意なIDでも、走査内容全体のcontent hashでもありません。比較時の注意点は [`usedu_compare`](#usedu_compare) を参照してください。

## セッション状態と走査結果の状態は別物

状態には、互いに独立した2つの階層があります。

### MCPセッションの `state`

外側の `state` は、サーバー側の処理が保存済み `envelope` を生成したかを表します。

| State | 意味 | `envelope` |
| --- | --- | --- |
| `running` | バックグラウンド走査を実行中 | なし |
| `complete` | 走査処理が終了し、走査結果を保存済み | あり |
| `cancelled` | バックグラウンド走査がキャンセルを検知した | 現在の実装ではなし |
| `failed` | バックグラウンド走査が致命的エラーで終了した | なし |

### `envelope.status.state`

内側の `envelope.status.state` は、ファイルシステム走査結果の完全性を表します。

| State | 意味 |
| --- | --- |
| `complete` | 走査上の問題と出力切り詰めが記録されていない |
| `partial` | `envelope` は生成できたが、ファイルシステム上の問題または走査上限により結果が不完全 |
| `limitReached` | `maxOutputEntries` または `maxOutputBytes` により、出力の一部を切り詰めた |

したがって、次の組み合わせはいずれも正常です。

- 権限エラーや走査上限到達後の、セッション `complete` + `envelope.status.state: "partial"`
- 出力切り詰め後の、セッション `complete` + `envelope.status.state: "limitReached"`
- バックグラウンド走査をキャンセルした後の、セッション `cancelled` + `envelope` なし

結果を完全なものとして扱う前に、必ず両方の状態を確認してください。

## 走査セッションに保存されるデータ

完了したセッションは、1つの `ScanEnvelope` をメモリ上に保持します。

```text
envelope.root       ルートエントリ1件
envelope.entries    depthとフィルター条件に従って保持したエントリの平坦な配列
envelope.topFiles   走査全体から収集した大きな通常ファイル
envelope.issues     保存済みのファイルシステム上の問題と走査上限の詳細
```

各参照toolが参照する範囲は次のとおりです。

| Tool | 参照する保存済みデータ |
| --- | --- |
| `usedu_list_children` | `envelope.entries` |
| `usedu_top_entries` | `envelope.entries` |
| `usedu_get_issues` | `envelope.issues` |
| `usedu_compare` | 両セッションの `envelope.root` と `envelope.entries` |

このため、最初の走査で指定した `depth`、`includeFiles`、出力上限によって、後続の問い合わせで見える範囲が決まります。省略または切り詰められたデータを、後続のtoolで復元することはできません。

## 基本的な利用フロー

通常のMCP接続と同様に、最初に `initialize` を行い、必要に応じて `tools/list` でtool定義を取得した後、`tools/call` を使います。

### 同期走査

同期呼び出しは、走査が完了または失敗するまで応答しません。走査中は、stdioのrequest loopも次の入力行を処理しません。

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "usedu_scan",
    "arguments": {
      "root": "/Users/example/Library",
      "depth": 2,
      "includeFiles": true,
      "top": 30
    }
  }
}
```

成功時は `structuredContent.state` が `complete` となり、`structuredContent.envelope` が含まれます。この外側の `complete` は「`envelope` を生成できた」という意味です。部分走査や出力切り詰めの有無は `envelope.status` で確認します。

返された `scanId` は、`usedu_list_children`、`usedu_top_entries`、`usedu_get_issues`、`usedu_compare`、`usedu_close_scan` で利用できます。

### バックグラウンド走査

`background: true` を指定すると、走査完了を待たずに応答します。

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "usedu_scan",
    "arguments": {
      "root": "/Users/example/Library",
      "background": true
    }
  }
}
```

最初の応答は `state: "running"` で、`envelope` は含まれません。`usedu_scan_status` で状態を確認し、完了した `envelope` が必要な呼び出しでは `includeEnvelope: true` を指定します。

キャンセルは協調的に処理されます。`usedu_cancel_scan` はキャンセルを要求するだけなので、直後の応答がまだ `state: "running"` でも異常ではありません。状態が変わるまで `usedu_scan_status` を呼び出してください。

## Tool一覧

| Tool | 用途 |
| --- | --- |
| `usedu_scan` | 同期またはバックグラウンドで走査し、セッションを作成する |
| `usedu_scan_status` | バックグラウンド走査の進捗を確認し、必要に応じて完了済み `envelope` を取得する |
| `usedu_cancel_scan` | 実行中の走査に協調的なキャンセルを要求する |
| `usedu_list_children` | `envelope` に保持された直下の子をページ単位で取得する |
| `usedu_top_entries` | 保存済みtree全体の保持済みエントリを順位付けする |
| `usedu_get_issues` | 保存済みissueの詳細をページ単位で取得する |
| `usedu_compare` | 2つのセッションのルートと保持済みエントリを比較する |
| `usedu_close_scan` | セッションを削除し、実行中ならキャンセルも要求する |

## Toolリファレンス

### `usedu_scan`

許可されたパスを走査し、セッションを作成します。

#### 入力

| Field | 型・既定値 | 実際の動作 |
| --- | --- | --- |
| `root` | string、必須 | 走査する既存パス。正規化後のパスが、起動時に設定した許可リスト内にある必要があります。 |
| `depth` | `0` 以上の整数、既定値 `1` | 走査を打ち切る深さではなく、結果として保持する深さです。`0` はルートのみ、`1` は直下の子までです。下位階層もディレクトリ合計を集計するために走査されます。 |
| `top` | `0` 以上の整数、既定値 `30` | `envelope.topFiles` の件数を制限します。MCPが作るsnapshot形式の `envelope` では、`envelope.entries` の件数を制限しません。 |
| `includeFiles` | boolean、既定値 `false` | 保持深度内の通常ファイル、シンボリックリンク、その他のleafエントリを `entries` に含め、`topFiles` の収集を有効にします。未指定の場合、`entries` にはディレクトリだけが入ります。 |
| `dirsOnly` | boolean、既定値 `false` | `entries` をディレクトリだけに絞ります。`includeFiles` も `true` の場合、`topFiles` は削除しません。 |
| `sort` | `used`、`name`、`files`、`dirs`、既定値 `used` | `envelope` 構築時の順序を指定します。後続の参照toolは、後述する独自の順序を使います。 |
| `fast` | boolean、既定値 `false` | 概算の高速集計を使います。ディレクトリ自身の割り当て済みbytesを省略する、ハードリンクを重複計上する、厳密モードなら除外するマウント済みファイルシステムを走査する、といった可能性があります。`envelope.semantics.accuracy` は `approximate` になります。 |
| `crossFileSystems` | boolean、既定値 `false` | 厳密モードでは、要求したroot配下のマウント済みファイルシステムへ走査を広げます。高速モードは `false` でも境界を越える場合があります。 |
| `maxScanEntries` | 正の整数、任意 | 走査するエントリ数の上限です。上限を超えた時点で、`RESOURCE_LIMIT_REACHED` issueを含む部分的な `envelope` を保存します。 |
| `maxScanDurationMs` | 正の整数、任意 | ミリ秒単位の協調的な走査時間上限です。強制的に処理を止めるタイムアウトではなく、走査中の確認処理で上限超過を検知した時点で、部分的な `envelope` を保存します。 |
| `maxOutputEntries` | `0` 以上の整数、任意 | 保存する `envelope.entries` を切り詰め、`envelope.status.state` を `limitReached` にします。 |
| `maxOutputBytes` | `0` 以上の整数、任意 | シリアライズ後のサイズに対する、達成を保証しない目標値です。実装は `entries`、`topFiles`、`issues` の順で要素を削り、`limitReached` にします。必須フィールドだけで、非常に小さい上限を超えることがあります。 |
| `redactPaths` | boolean、既定値 `false` | `displayName` と `displayPath` を `[redacted]` に置き換えます。可逆的な `pathRef` は残ります。 |
| `background` | boolean、既定値 `false` | ワーカースレッドで走査し、`envelope` 生成前にセッションを返します。 |

MCP走査は常にsnapshotモードの `envelope`（`effectiveOptions.mode: "snapshot"`）を作ります。issueの詳細は既定で有効ですが、`maxOutputBytes` によって削除される場合があります。

MCP toolには `jobs` 引数がありません。スキャナーの既定値を使い、解決後の値を `effectiveOptions.jobs` に記録します。

#### 出力

```text
scanId
schemaVersion
state
progress
[envelope]
```

`envelope` は成功した同期走査では含まれ、バックグラウンド走査の最初の応答では省略されます。

`progress` は次のフィールドを持ちます。

```text
elapsedMs
entriesSeen
filesSeen
dirsSeen
errorsSeen
done
```

`envelope.topFiles` と `usedu_top_entries` は異なる結果集合です。

- `topFiles` は走査全体で見つかった大きな通常ファイルを返し、`top` で件数を制限します。
- `usedu_top_entries` は `envelope.entries` に保持済みのエントリだけを順位付けします。

### `usedu_scan_status`

1つのセッションの現在状態を返します。

#### 入力

| Field | 型・既定値 | 動作 |
| --- | --- | --- |
| `scanId` | string、必須 | 確認するセッションです。 |
| `includeEnvelope` | boolean、既定値 `false` | セッションの `state` が `complete` の場合だけ `envelope` を追加します。 |

#### 出力

```text
scanId
schemaVersion
state
progress
[envelope]
[message]
```

`message` は `cancelled` または `failed` の場合に含まれます。このtoolを呼ぶと、セッションの最終利用時刻が更新されます。

### `usedu_cancel_scan`

実行中のバックグラウンド走査にキャンセルを要求します。

#### 入力

```text
scanId
```

#### 出力

```text
scanId
cancelRequested
state
progress
```

`cancelRequested` が `true` になるのは、呼び出し時点でセッションがまだ `running` の場合だけです。ワーカーがすでに停止したことを意味しません。

### `usedu_list_children`

保存済み `envelope` から、指定エントリの直下の子を返します。

#### 入力

| Field | 型・既定値 | 動作 |
| --- | --- | --- |
| `scanId` | string、必須 | 外側のセッションの `state` が `complete` である必要があります。 |
| `entryId` | string、必須 | 親エントリです。rootのIDは `envelope.root.entryId` で取得できます。 |
| `limit` | integer `1..500`、既定値 `50` | 1ページの件数です。実行時の値はこの範囲へ丸められます。 |
| `cursor` | string、任意 | 直前の応答で返された不透明な継続cursorです。 |

#### 出力

```text
scanId
entryId
items
nextCursor
```

現在の実装では、次の点に注意してください。

- `envelope.entries` に保存済みの直下の子だけが検索対象です。
- 結果は、`usedu_scan` の `sort` に関係なく `usedBytes` の降順です。
- 未知の `entryId` またはleafエントリのIDを指定すると、エラーではなく空のページを返します。
- `depth`、`includeFiles`、`maxOutputEntries`、`maxOutputBytes` により省略されたエントリは、このtoolでは復元できません。

### `usedu_top_entries`

保持深度全体にある保存済みエントリを順位付けします。

#### 入力

| Field | 型・既定値 | 動作 |
| --- | --- | --- |
| `scanId` | string、必須 | `complete` セッションを指定します。 |
| `limit` | integer `1..500`、既定値 `50` | 返すエントリ数の上限です。実行時の値はこの範囲へ丸められます。 |
| `kind` | optional enum | `directory`、`regularFile`、`symlink`、`other`。 |
| `minUsedBytes` | `0` 以上の整数、既定値 `0` | 最小の割り当て済みサイズです。 |

#### 出力

```text
scanId
items
```

rootは対象外です。`envelope.entries` を `usedBytes` の降順で返し、cursorによるページ分割はありません。このtoolは `envelope.topFiles` を参照せず、元の `envelope` に保持されなかったエントリも探索しません。

### `usedu_get_issues`

`envelope` に保存されたissueの詳細をページ単位で返します。

#### 入力

| Field | 型・既定値 | 動作 |
| --- | --- | --- |
| `scanId` | string、必須 | `complete` セッションを指定します。 |
| `limit` | integer `1..500`、既定値 `50` | 1ページの件数です。実行時の値はこの範囲へ丸められます。 |
| `cursor` | string、任意 | 不透明な継続cursorです。 |

#### 出力

```text
scanId
items
nextCursor
```

issueには、ファイルシステム読取エラー、policyに基づく除外、走査上限到達などが含まれます。`maxOutputBytes` で一部のissue detailが削除されても、`envelope.issueSummary` には集計値が残ります。ただし、削除された詳細を現在のセッションから後で取得することはできません。

### `usedu_compare`

保存済みの2つのscan envelopeを比較します。

#### 入力

```text
beforeScanId
afterScanId
```

#### 出力

次のフィールドを持つ `usedu.diff.v1` envelopeです。

```text
schemaVersion
status
beforeScanId
afterScanId
summary
changes
```

比較対象はrootと保持済みの `entries` で、識別には `pathRef` を使います。`topFiles` とissue recordは比較しません。

どちらかの `envelope.status.state` が `complete` でない場合、または集計semanticsが異なる場合、実装はdiffを自動的に不正確とします。不正確なdiffでは、各changeを `uncertain` として返します。

意味のある比較にするため、呼び出し側でもrootとeffective optionsをそろえてください。特に `depth`、`includeFiles`、`dirsOnly`、出力上限が重要です。現在の実装は、すべてのeffective optionsの不一致を自動検出するわけではありません。

現在のセッションID生成にも比較上の制約があります。同じroot、集計後の `usedBytes`、ファイル数、ディレクトリ数を持つ同期走査は、同じ `scanId` を再利用して以前のセッションを置き換える場合があります。同一サーバープロセス内で変更前・変更後を確実に比較する場合は、両方を `background: true` で走査してください。この場合はプロセス内で別々のセッションIDが割り当てられます。

### `usedu_close_scan`

セッションを1つ削除します。

#### 入力

```text
scanId
```

#### 出力

```text
scanId
closed
```

セッションを削除した場合は `closed: true`、すでに存在しない場合は `false` です。実行中のセッションをcloseすると、キャンセルも要求します。

## セッションの保持

セッションはプロセス内のメモリにだけ保持されます。

- 既定の上限は8件で、`--max-sessions` で変更できます。
- 上限に達すると、新しいセッションを追加する前に、最終更新時刻が最も古いセッションを削除します。
- 実行中のセッションを削除する場合はキャンセルを要求します。
- 無操作TTLは現在30分に固定されています。
- 期限切れセッションは専用タイマーではなく、次のrequestを処理する直前に遅延削除されます。
- statusおよび問い合わせtoolの呼び出しは、セッションの最終利用時刻を更新します。

`scanId` を永続化したり、サーバー再起動、TTL切れ、上限超過による削除、`usedu_close_scan` の後も有効だと仮定したりしないでください。

## エラーモデル

プロトコル/toolのエラーと、走査中に記録したissueは別経路で返します。

| 状況 | 結果 |
| --- | --- |
| 未知のJSON-RPC method | JSON-RPC error `-32601` |
| 実行時validationで拒否された引数、未知のtool、不正cursor、未知の`scanId`、allowlist外の要求、走査完了前の問い合わせ、同期走査の致命的エラー | JSON-RPC error `-32602` |
| request dispatch前に失敗する不正入力、またはstdio handler内部のエラー | 通常は `id: null` を伴うJSON-RPC error `-32603` |
| envelopeを生成できる段階で発生したファイルシステム読取エラー、cross-filesystem skip、走査上限到達 | 成功したtool応答内の `envelope.issues` と、完全ではない `envelope.status` |
| 開始済みバックグラウンド走査の致命的エラー | セッションの `state` `failed` と `message` |
| バックグラウンド走査がキャンセルを検知 | セッションの `state` `cancelled` と `message` |

JSON-RPCとして成功していても、ファイルシステム走査結果が完全とは限りません。必ず `envelope.status` と `envelope.issueSummary` を確認してください。

## 現在の実装上の制約

以下は、将来の拡張を保証するものではなく、現在の実装を正確に説明するための制約です。

- トランスポートはstdioのみです。
- 実装済みrequest methodは `initialize`、`tools/list`、`tools/call` です。`id`のないrequestには応答せず、それ以外のmethodは `-32601` を返します。
- 許可ルートはプロセス起動時に設定します。
- セッションとenvelopeは永続化されません。同期走査の `scanId` は、集計値が同じ複数の走査間で一意になるとは限りません。
- 参照toolは保存済みenvelopeを調べるだけで、ファイルシステムを再走査しません。
- 出力切り詰めで削除されたデータは、そのセッションでは復元できません。
- `envelope.nextCursor`はJSON v2 envelopeのフィールドですが、現在のMCP参照toolは、切り詰めたenvelope dataの取得には使いません。
- `usedu_list_children`と`usedu_top_entries`は、元の走査時のsortではなく割り当て済みサイズ順を使います。
- fast traversalは、`crossFileSystems`がfalseでもファイルシステム境界を越える場合があります。現在の`semantics.filesystemBoundaryPolicy`は要求されたflagを反映するため、このbackend固有の注意点までは表現しません。
- `tools/list`が公開するのはinput schemaです。schema外の入力に対する挙動は契約対象ではありません。結果検証には `structuredContent` とJSON v2 schemaを使ってください。

関連する契約:

- [Filesystem Semantics](semantics.ja.md)
- [JSON Contract](json-contract.ja.md)
- [Agent Security Boundary](agent-security.ja.md)
