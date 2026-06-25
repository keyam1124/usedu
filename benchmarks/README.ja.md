# usedu スキャナーベンチマーク

[English](README.md) | [日本語](README.ja.md)

このディレクトリには、将来の `usedu` の変更と比較するためのスキャナー性能ベースラインを置きます。

このリポジトリでは、スキャナー性能の変化をあとから確認できるようにベンチマーク成果物を残します。
リポジトリ外に性能上の主張を出す場合は、コマンド単位の測定、キャッシュ状態の明示、正確なコマンドバージョン、ワークロードの説明を揃えます。

## 内部ベースライン

`benchmarks/baseline.json`、`benchmarks/baseline.md`、`benchmarks/baseline.ja.md` は、内部向けのスキャナー API 回帰ベースラインです。
公開用の性能ランキングではありません。

ベンチマーク runner は `.usedu-bench/scanner-benchmark` に毎回同じ内容の fixture を生成して走査します。
fixture は測定前に生成され、既存の `/.usedu-*` ルールで git から除外されます。
各シナリオは、測定 run を記録する前にウォームアップ走査を実行します。
測定 run は、短時間のシステム負荷の偏りを減らすため、シナリオ間で round-robin に収集します。
測定時間は、そのシナリオでスキャナー API を呼び出した wall-clock duration です。
このベースラインでは、`/` やユーザーディレクトリのような広い実パスを使いません。

## ベースラインの作成と更新

```bash
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo run --release --example bench_scanner -- --runs 7 --write-json benchmarks/baseline.json --write-md benchmarks/baseline.md --write-md-ja benchmarks/baseline.ja.md
```

ベースラインは、意図したスキャナー性能変更が入ったとき、またはベンチマーク fixture や schema が変わったときだけ更新します。
説明できていない性能退行を隠すために更新してはいけません。

## 将来の変更との比較

```bash
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo run --release --example bench_scanner -- --runs 7 --compare benchmarks/baseline.json
```

CI では、同じ fixture と scenario の比較を release gate として使います。
ただし、hosted macOS runner は machine 差があるため、timing threshold は無効化します。

```bash
cargo run --release --example bench_scanner -- --runs 1 --compare benchmarks/baseline.json --compare-structure-only
```

解釈は次のとおりです。

- 5% 未満、または 5 ms 未満の変化は測定ノイズとして扱います。
- 10% 以上の低速化は `needs-review` と表示します。
- benchmark profile、fixture shape、scenario set、entries、used bytes、file count、dir count、error count が一致しない場合、時間比較の前に失敗します。
- できるだけ同じマシンで比較します。
- baseline は実行環境に依存します。
- 他の build、test、広い走査と並列に実行しません。
- すべての scenario が同じ方向に動いた場合は、短い待ち時間を置いて一度だけ再実行します。
- これらの短い scenario は CPU state の影響を受けやすいためです。

## 公開向け比較テンプレート

リポジトリ外で性能上の主張を出す場合は、コマンド単位で `hyperfine` を使います。
`usedu` の commit、worktree state、system、Rust version、target path shape、各 command version を記録します。
macOS では、手順化された cache flush がない限り、warm-cache result として扱います。
最初に実行しただけの結果を cold-cache result と呼んではいけません。

```bash
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo build --release

BENCH_TARGET=.usedu-bench/scanner-benchmark
PATH=/opt/homebrew/opt/rustup/bin:$PATH cargo run --release --example bench_scanner -- --runs 1 >/dev/null

hyperfine --warmup 5 --ignore-failure \
  "target/release/usedu report ${BENCH_TARGET} --summarize --no-progress" \
  "target/release/usedu report ${BENCH_TARGET} --fast --summarize --no-progress" \
  "<comparison-command-1>" \
  "<comparison-command-2>"
```

比較 command が使えない場合や、容量の意味論が大きく異なる場合は、その command を省略して理由を書きます。
Linux 専用の cold-cache 比較では、公開されるディスク使用量ベンチマークでよく使われる `hyperfine --prepare 'sync; echo 3 | sudo tee /proc/sys/vm/drop_caches'` 形式に合わせます。

## シナリオ

- `recursive_default`：`ScanOptions::default()` による再帰走査です。
- `recursive_fast`：`fast = true` による再帰走査です。
- `recursive_fast_summary`：`fast = true` に加えて root child retention を無効化した再帰走査で、`usedu --fast --summarize` に対応します。
- `recursive_jobs_1`：`jobs = Some(1)` による再帰走査です。
- `recursive_with_files`：top-file tracking を有効にした再帰走査です。
- `current_level_default`：TUI 形式のブラウザで使う direct-child scan です。
- `wide_root_default`：`fixture/wide` の再帰走査で、root-level child directory による並列走査の挙動を見ます。
- `wide_root_fast`：`fast = true` による `fixture/wide` の再帰走査です。
- `wide_root_jobs_1`：`jobs = Some(1)` による `fixture/wide` の再帰走査です。
