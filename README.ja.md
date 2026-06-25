# usedu

[English](README.md) | [日本語](README.ja.md)

`usedu` は、macOS のターミナルで使う読み取り専用のディスク使用量アナライザーです。
ファイルシステムのメタデータを走査し、割り当て済みサイズを `Used` として表示します。
静的レポートと対話型 TUI ブラウザを提供します。

`usedu` はファイルの削除、移動、変更を行いません。
クリーンアップ候補も提案しません。

## インストール

```bash
cargo install --path .
```

## 使い方

```bash
usedu
usedu ~/Library
usedu --fast ~/Library
usedu report ~/Library --depth 2 --top 30
usedu report ~/Library --files
usedu report ~/Library --fast --summarize
usedu report ~/Library --json
usedu report ~/Library --format json-v2
usedu report ~/Library --format ndjson
usedu schema json-v2
usedu snapshot ~/Library > scan.usedu.json
usedu compare before.usedu.json after.usedu.json
usedu mcp --stdio --allow-root ~/Library
```

パスを渡さない場合、`usedu` は現在のディレクトリを TUI で開きます。
`/` を暗黙の既定値にはしません。
ルートボリューム全体を走査するには、`usedu /` または `usedu report /` を明示的に実行します。

## TUI

```bash
usedu [PATH]
```

既定のコマンドは、対話型のターミナルブラウザを開きます。

主なオプションは次のとおりです。

```text
    --fast                  Use faster approximate scanning
    --cross-file-systems    Allow scanning across mounted filesystems
    --jobs <N>              Worker count for parallel scans
```

TUI は、現在のディレクトリの直下の子だけを表示します。
子ディレクトリは再帰的に集計されるため、`Used` 列にはその子ディレクトリ以下の割り当て済みサイズの合計が表示されます。

読み込み中は、エントリ数、エラー数、経過時間を表示します。
読み込み中に `q` を押すと、走査をキャンセルして TUI を終了します。

キー操作は次のとおりです。

```text
Up / k          Move up
Down / j        Move down
Enter           Open selected directory
Backspace / h   Parent directory
r               Rescan current directory
R               Clear cached result and rescan
s               Toggle sort: used, name, files, dirs
e               Toggle error list
?               Toggle help
q               Quit
```

## 静的レポート

```bash
usedu report [PATH]
```

主なオプションは次のとおりです。

```text
-d, --depth <N>             Display tree depth. Default: 2
-n, --top <N>               Show top N entries. Default: 30
    --files                 Include top large files section
    --summarize             Show only the total summary
    --fast                  Use faster approximate scanning
    --dirs-only             Only show directories in ranking
    --sort used|files|dirs  Sort key. Default: used
    --json                  Output JSON instead of rich text
    --format text|json-v1|json-v2|ndjson
                            Output format. Default: text
    --errors                Show error details
    --redact-paths          Redact display paths in machine-readable output
    --no-progress           Disable progress indicator
    --cross-file-systems    Allow scanning across mounted filesystems
    --jobs <N>              Worker count for parallel scans
```

リッチテキスト出力では、走査中にエントリ数、エラー数、経過時間を含む進捗を間引いて表示します。
JSON 出力と `--no-progress` は進捗表示を抑止します。

`--json` は現在の JSON report format を維持します。
versioned machine-readable scan envelope を使う場合は `--format json-v2` を指定します。
line-delimited scan event を使う場合は `--format ndjson` を指定します。

JSON v2 schema は次の command で出力します。

```bash
usedu schema json-v2
```

snapshot は stdout に作成します。

```bash
usedu snapshot [PATH] > scan.usedu.json
```

2 つの snapshot は次の command で比較します。

```bash
usedu compare before.usedu.json after.usedu.json
```

MCP adapter は次の command で起動します。

```bash
usedu mcp --stdio --allow-root [PATH]
```

## サイズの扱い

`usedu` は、ファイルシステム上の割り当て済みサイズだけを報告します。
論理バイト長ではなく、`symlink_metadata` と Unix のブロック数（`blocks() * 512`）を使います。

表示ラベルは常に `Used` です。
`--logical` や `--allocated` のようなサイズモード切り替えはありません。

APFS のクローン、スナップショット、圧縮、スパースファイル、File Provider の挙動によって、実際に解放できる容量と表示上の `Used` は一致しないことがあります。

`--fast` は、ファイルの割り当て済みサイズの計算を保ったまま、一部の高コストなメタデータ処理を省略し、より積極的に入れ子の並列走査を使います。
そのため、ディレクトリ自身の割り当て済みサイズを省略したり、ハードリンクされたファイルを重複して数えたり、厳密モードならスキップするマウント済みファイルシステムをまたいだりすることがあります。
厳密な集計より走査時間を優先し、概算で足りるときに使います。

`--summarize` はルートの合計だけを表示します。
`--fast` と組み合わせると、ルート直下の要約も保持しません。
この組み合わせは、合計だけのレポートを低い待ち時間で得たい場合に向いています。

## ファイルシステム上の挙動

- シンボリックリンクはリンク自身のエントリとして数えますが、リンク先はたどりません。
- 隠しファイルと隠しディレクトリも含めます。
- `.app` や `.photoslibrary` のようなパッケージディレクトリは、通常のディレクトリとして扱います。
- 権限エラーは記録しますが、走査全体を中断しません。
- 既定では、別デバイス上のマウント済みボリュームをスキップします。
- ファイルシステム境界をまたぐには `--cross-file-systems` を使います。
- 複数のハードリンクを持つ通常ファイルは、実用上可能な範囲でデバイスと inode ごとに一度だけ数えます。

machine-readable JSON v2 では、regular file、directory、symlink、other の count を分離します。
表示専用 path に加えて、可逆的な `pathRef` も含めます。

保護された macOS の場所を走査できない場合は、ターミナルアプリにフルディスクアクセスを付与します。

## 開発

維持すべき設計上の制約は [docs/design.ja.md](docs/design.ja.md) にまとめています。
プロダクト契約は [docs/adr/0001-product-contract.ja.md](docs/adr/0001-product-contract.ja.md)、ファイルシステム用語は [docs/semantics.ja.md](docs/semantics.ja.md)、JSON 契約は [docs/json-contract.ja.md](docs/json-contract.ja.md)、agent boundary は [docs/agent-security.ja.md](docs/agent-security.ja.md)、MCP tool contract は [docs/mcp-tools.ja.md](docs/mcp-tools.ja.md) に記録しています。

```bash
cargo build
cargo test
cargo fmt
cargo clippy --all-targets --all-features
```
