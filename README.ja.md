# usedu

[English](README.md) | [日本語](README.ja.md)

`usedu` は、macOS のターミナルで使う読み取り専用のディスク使用量アナライザーです。
ファイルシステムのメタデータを走査し、割り当て済みサイズを `Used` として表示します。
静的レポート、対話型 TUI、versioned machine output、AI agent 向けのローカル MCP interface を提供します。

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

主なオプション:

```text
    --fast                  Use faster approximate scanning
    --cross-file-systems    Allow scanning across mounted filesystems
    --jobs <N>              Worker count for parallel scans
```

TUI は現在のディレクトリの直下だけを表示します。
子ディレクトリは再帰的に集計されるため、`Used` 列にはその配下の割り当て済みサイズ合計が表示されます。

読み込み中は、エントリ数、エラー数、経過時間を表示します。
読み込み中に `q` を押すと、走査をキャンセルして TUI を終了します。

キー操作:

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

主なオプション:

```text
-d, --depth <N>             Display tree depth. Default: 2
-n, --top <N>               Show top N entries. Default: 30
    --files                 Include top large files section
    --summarize             Show only the total summary
    --fast                  Use faster approximate scanning
    --dirs-only             Only show directories in ranking
    --sort used|name|files|dirs
                            Sort key. Default: used
    --json                  Output legacy JSON instead of rich text
    --format text|json-v1|json-v2|ndjson
                            Output format. Default: text
    --errors                Show error details
    --redact-paths          Redact display paths in machine-readable output
    --max-output-bytes <N>  Cap JSON v2/NDJSON output bytes
    --no-progress           Disable progress indicator
    --cross-file-systems    Allow scanning across mounted filesystems
    --jobs <N>              Worker count for parallel scans
```

リッチテキスト出力では、走査中にエントリ数、エラー数、経過時間を間引いて表示します。
machine-readable format と `--no-progress` では進捗表示を抑止します。

`--json` は legacy JSON report format です。
versioned machine-readable scan envelope には `--format json-v2` を使います。
1 行単位の scan event には `--format ndjson` を使います。

JSON v2 schema の出力:

```bash
usedu schema json-v2
```

stdout 経由で永続 snapshot を作成:

```bash
usedu snapshot [PATH] > scan.usedu.json
```

2 つの snapshot file を比較:

```bash
usedu compare before.usedu.json after.usedu.json
```

## AI agent と MCP

走査を許可する root を指定して、ローカルの foreground MCP server を起動します。

```bash
usedu mcp --stdio \
  --allow-root "$HOME/Library" \
  --allow-root "$HOME/Projects"
```

MCP に接続した agent は、次の処理を実行できます。

- どのディレクトリが割り当て済み容量を多く使っているか調べる
- 走査ツリー全体から大きな通常ファイルを探す
- 保存済みの走査結果を使って、ディレクトリを再走査せずに掘り下げる
- 権限エラー、filesystem boundary skip、partial result の理由を確認する
- 長い走査をバックグラウンドで実行し、進捗確認やキャンセルを行う
- 2 つの in-memory scan session を比較し、増減した場所を調べる

利用者は、たとえば agent に次のように依頼できます。

```text
~/Library の中で容量上位のディレクトリを 10 件調べて。
このプロジェクト配下の大きな通常ファイルを探して。
走査結果が partial になった理由を説明して。
build 前後でこのディレクトリを比較して。
```

MCP server も読み取り専用です。ファイルを削除せず、cleanup action も推薦しません。読み取るのは metadata であり、ファイル内容ではありません。

許可 root は server 起動時に固定されます。session は memory 上だけに保持され、process 終了時に失われます。後続 query は最初の scan で保持した entry だけを見るため、`depth`、`includeFiles`、出力上限が後の drill-down 範囲に影響します。

利用フロー、各 tool の実際の動作、現在の制約は [AI エージェントから MCP で `usedu` を使う](docs/mcp-tools.ja.md) を参照してください。権限と privacy boundary は [Agent Security Boundary](docs/agent-security.ja.md) にまとめています。

## サイズの扱い

`usedu` は、ファイルシステム上の割り当て済みサイズだけを報告します。
論理バイト長ではなく、`symlink_metadata` と Unix のブロック数（`blocks() * 512`）を使います。

表示ラベルは常に `Used` です。
`--logical` や `--allocated` のようなサイズモード切り替えはありません。

APFS の clone、snapshot、compression、sparse file、File Provider の挙動により、実際に解放できる容量と表示上の `Used` が一致しないことがあります。

`--fast` は、ファイルの割り当て済みサイズの計算を保ちながら、一部の高コストな metadata 処理を省略し、より積極的な並列走査を行います。
directory 自身の割り当て済みサイズを省略したり、hard link を重複計上したり、strict mode なら skip する mounted filesystem を走査したりする場合があります。
厳密な集計より走査時間を優先し、概算で足りる場合に使います。

`--summarize` は root の合計だけを表示します。
`--fast` と組み合わせると root 直下の要約も保持しません。
合計だけを低い待ち時間で得たい場合に向いています。

## ファイルシステム上の挙動

- symbolic link は link entry として数えますが、link 先はたどりません。
- hidden file と hidden directory も含めます。
- `.app` や `.photoslibrary` は通常の directory として扱います。
- permission error は記録しますが、走査全体を中断しません。
- strict mode は既定で指定 root と同じ filesystem 内にとどまります。
- strict mode で filesystem boundary を越えるには `--cross-file-systems` を使います。
- fast mode は approximate であり、この option がなくても mounted filesystem を走査する場合があります。
- strict mode では、複数の hard link を持つ通常ファイルを可能な範囲で device/inode ごとに一度だけ数えます。

machine-readable JSON v2 は regular file、directory、symlink、other の count を分離します。
表示専用 path に加え、可逆的な `pathRef` も含みます。

保護された macOS の場所を走査できない場合は、ターミナルアプリにフルディスクアクセスを付与します。

## 開発

維持すべき設計上の制約は [docs/design.ja.md](docs/design.ja.md) にまとめています。
プロダクト契約は [docs/adr/0001-product-contract.ja.md](docs/adr/0001-product-contract.ja.md)、ファイルシステム用語は [docs/semantics.ja.md](docs/semantics.ja.md)、JSON interface は [docs/json-contract.ja.md](docs/json-contract.ja.md)、agent boundary は [docs/agent-security.ja.md](docs/agent-security.ja.md)、MCP の利用フローと tool 仕様は [docs/mcp-tools.ja.md](docs/mcp-tools.ja.md) に記録しています。

```bash
cargo build
cargo test --workspace
cargo fmt
cargo clippy --workspace --all-targets --all-features
```
