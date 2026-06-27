# Agent Security Boundary

[English](agent-security.md) | [日本語](agent-security.ja.md)

`usedu` は AI agent を第一級の client として扱いますが、人間向け CLI/TUI と同じ読み取り専用の境界を維持します。

MCP server が agent に許可するのは、設定済み root 配下の filesystem metadata を調べることです。ファイルシステムの変更やファイル内容の読み取りは許可しません。

## MCP client に許可されること

許可 root 内で、MCP client は次を実行できます。

- metadata と allocated size の走査
- 走査結果を in-memory session として保持
- 保持済みの子エントリ、上位エントリ、issue の参照
- 2 つの in-memory scan session の比較
- background scan の進捗確認とキャンセル要求

`usedu` を使った削除、移動、変更、隔離、削除推薦はできません。

## Root allowlist

1 つ以上の許可 root を指定して起動します。

```bash
usedu mcp --stdio \
  --allow-root "$HOME/Library" \
  --allow-root "$HOME/Projects"
```

root を指定しない場合は、現在のディレクトリだけを許可します。

許可 root は process 起動時に固定されます。現在の実装は MCP client の `roots` を動的には取り込まず、tool call から allowlist を広げることもできません。

server は、設定済み root と scan request の path をどちらも canonicalize します。symbolic link 経由を含め、解決後の path が allowlist 外になる request は traversal 前に拒否します。存在せず canonicalize できない path も拒否します。

allowlist が制御するのは、scan を開始できる場所です。配下の全 entry を読み取れることを保証するものではありません。通常の macOS permission と Full Disk Access の制約はそのまま適用されます。

## Filesystem traversal policy

走査で読むのは metadata だけです。分析のためにファイル内容を開きません。

symbolic link は link entry として数えますが、link 先はたどりません。

### Strict mode

`fast: false` が既定値です。filesystem boundary を重視する場合はこちらを使います。

strict mode では、次の規則になります。

- directory 自身の allocated bytes を含める
- regular file の hard link を可能な範囲で重複排除する
- `crossFileSystems: true` を明示しない限り、requested root と同じ filesystem 内にとどまる

### Fast mode

`fast: true` は approximate accounting です。次の挙動が起こり得ます。

- directory own allocation を省略する
- hard-linked file を重複計上する
- `crossFileSystems: false` でも、strict mode なら skip する mounted filesystem を走査する

したがって、`crossFileSystems: false` は strict mode の境界であり、fast mode の保証ではありません。filesystem boundary が security または accounting requirement の一部なら、agent は `fast` を無効のまま使うべきです。

startup allowlist は requested root に対して引き続き有効です。ただし、その root 配下から到達できる mounted filesystem を fast mode が走査する可能性はあります。

## Output と path identity

machine output は structured JSON です。ファイル名は data field として返し、生成する説明文の命令として扱いません。

`displayName` と `displayPath` は表示専用で、lossy Unicode conversion を含む場合があります。machine client は次を使います。

- 1 回の scan 内の参照: `entryId`
- 可逆的な path identity: `pathRef`

`usedu_scan` で `redactPaths: true` を指定すると、`displayName` と `displayPath` を `[redacted]` に置き換えます。

redaction は `pathRef` を削除しません。`pathRef` には可逆的な Unix path bytes が含まれるため、path identity を隠す必要がある場合は、信頼できない recipient へ転送しないでください。

命令のように見える filename も未信頼 data です。client は `structuredContent` を利用し、filename field と model instruction を分離してください。

## Session boundary

MCP session は process-local かつ memory-only です。

- 既定の最大数は 8
- `--max-sessions` で table limit を変更可能
- 上限到達時は、最終利用時刻が最も古い session を eviction
- inactivity TTL は現在 30 分固定
- expired session は専用 timer ではなく、後続 request の前に遅延削除
- running session の close または eviction は cancellation を要求
- MCP process 終了時に全 session を消失

`scanId` は永続的な authorization ではなく、恒久的な external ID として保存すべきではありません。

## Resource control

traversal は次で制限できます。

- `maxScanEntries`
- `maxScanDurationMs`

これらは協調的な traversal budget です。到達時は通常、structured `RESOURCE_LIMIT_REACHED` issue を持つ partial envelope を返します。process-level の hard timeout ではありません。

stored output は次で制限できます。

- `maxOutputEntries`
- `maxOutputBytes`

output truncation では envelope status が `limitReached` になります。stored envelope から削除された data は、後続 MCP query では復元できません。

list operation は cursor pagination を使い、page size を runtime range `1..500` に丸めます。

background scan は `usedu_scan_status` で progress を公開し、`usedu_cancel_scan` または `usedu_close_scan` で協調的 cancellation を要求できます。

## Privacy guidance

`--allow-root` は必要な最小範囲にしてください。1 つの project または Library subtree だけが必要な task で、home directory 全体を許可するのは避けます。

agent に size/count は必要でも human-readable path が不要なら、`redactPaths: true` を使います。ただし、structured result には可逆的な `pathRef` が残る点に注意してください。

untrusted または autonomous client が scan parameter を選べる場合は、traversal budget と output budget を設定してください。

## Result の解釈

MCP call が成功しても、filesystem result が完全とは限りません。

client は次を確認します。

- outer session `state`
- `envelope.status.state`
- `envelope.status.partialReasons`
- `envelope.issueSummary` と、必要に応じて `usedu_get_issues`

allocated `Used` bytes は reclaimable bytes の保証ではありません。APFS clone、snapshot、compression、sparse file、File Provider の挙動により、削除後に回復する容量は異なる場合があります。

これらの control が許可するのは観測だけです。`usedu` は cleanup action を実行も推薦もしません。

利用フローと各 tool の説明は [AI エージェントから MCP で `usedu` を使う](mcp-tools.ja.md) を参照してください。
