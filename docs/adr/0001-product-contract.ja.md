# ADR 0001: プロダクト契約

ステータス: Accepted / implemented

## 背景

`usedu` は、ターミナルで使う macOS 向け disk usage tool です。
現在は、人間だけでなく AI agent、MCP adapter、reusable Rust component からも安全に使える observation engine として構成しています。
これらの interface は、個別の走査挙動を持たず、同じ core model を共有します。

disk usage tool は cleanup tool と混同されやすいため、product boundary を明確にします。
`usedu` が報告するのは filesystem 上の allocated usage です。どの byte が安全に reclaim できるかは判断しません。

## 決定

`usedu` は、read-only の disk allocation inspection engine です。

CLI report、TUI、machine-readable output、snapshot、diff、MCP interface、reusable Rust crate は、同じ scanner semantics と domain model を共有します。

AI agent は第一級の利用者ですが、`usedu` は agent 専用 tool ではありません。
human-readable interface と machine-readable interface は、同じ core behavior の adapter として実装します。
core は terminal rendering、MCP transport、CLI formatting に依存しません。

`usedu` は次を行いません。

- ファイルの削除、移動、変更、隔離
- cleanup action の推薦、または entry を安全に削除できるという断定
- default の background daemon 化
- GUI application 化
- 通常走査でのファイル内容読み取り
- logical size を別 report mode として扱うこと

snapshot output は stdout を基本にします。

```bash
usedu snapshot PATH > scan.usedu.json
```

将来 command が output file へ直接書き込む場合、その write は read-only scanning boundary に対する明示的な例外として文書化します。

MCP は protocol-neutral domain model と versioned DTO contract の adapter です。
実装済み server は、startup-configured allowed roots と memory-only session を持つ foreground stdio process です。
別 ADR で変更しない限り、default network daemon は引き続き product boundary 外です。

`Used` は filesystem 上の allocated size であり、reclaimable bytes ではありません。
APFS clone、snapshot、compression、sparse file、File Provider の挙動により、表示される `Used` と削除後に回復する容量は一致しないことがあります。

## 現在の実装

この決定は、現在次の構成で実現しています。

- scanner behavior: `usedu-core`
- JSON v2 / diff DTO: `usedu-protocol`
- CLI text、JSON v1、JSON v2、NDJSON adapter
- stdout snapshot と file-based diff command
- process-local MCP adapter: `usedu mcp --stdio`

MCP adapter により、agent は stored scan result の inspection、navigation、explanation、comparison を実行できます。cleanup capability は追加しません。

[MCP の利用フローと tool リファレンス](../mcp-tools.ja.md) と [Agent Security Boundary](../agent-security.ja.md) も参照してください。

## 影響

machine-readable output は、人間向け label に依存せず、自身の semantics を出力します。
identity に見える field を lossy display string だけでは表現しません。
error、partial scan、filesystem-boundary skip、output limit、fast-mode trade-off を構造化して表現します。

scanner code は report、TUI、snapshot、diff、MCP adapter で共有します。
