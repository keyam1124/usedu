# ADR 0001: プロダクト契約

ステータス: Accepted

## 背景

`usedu` は、ターミナルで使う macOS 向けディスク使用量ツールです。
今後は、人間だけでなく AI agent、MCP adapter、将来の Rust API からも安全に使える観測エンジンとして扱えるようにします。
これらの interface は、個別の走査挙動を持たず、同じ core model を共有します。

ディスク使用量ツールは cleanup tool と混同されやすいため、プロダクト境界を明確にします。
`usedu` が報告するのは、ファイルシステム上の割り当て済み使用量です。
どの byte が安全に解放できるかは判断しません。

## 決定

`usedu` は、read-only の disk allocation inspection engine です。
CLI report、TUI、将来の MCP interface、将来の Rust API は、同じ scanner semantics と domain model を共有します。

AI agent は第一級の利用者ですが、`usedu` は agent 専用ツールではありません。
人間向け interface と機械向け interface は、同じ core behavior の adapter として実装します。
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

将来、command が output file へ直接書き込む場合、その write は read-only scanning boundary に対する明示的な例外として文書化します。

MCP は adapter です。
MCP server を実装する前に、protocol-neutral な domain model、semantics、DTO contract を用意します。
default の network daemon はプロダクト境界外です。
別 ADR で境界を変更しない限り、MCP server は foreground stdio を使います。

`Used` は、ファイルシステム上の割り当て済みサイズを意味します。
reclaimable bytes ではありません。
APFS clone、snapshot、compression、sparse file、File Provider の挙動により、表示される `Used` と削除後に回復する容量は一致しないことがあります。

## 影響

machine-readable output は、人間向け label へ依存せず、自身の semantics を出力します。
identity に見える field には、lossy な display string を使いません。
error、partial scan、filesystem boundary skip、fast mode の trade-off は構造化して表現します。

scanner code は report mode と TUI mode で共有し、将来の protocol、snapshot、diff、MCP adapter が再利用できる構造へ寄せます。
