# agent security boundary

[English](agent-security.md) | [日本語](agent-security.ja.md)

`usedu` は AI agent を第一級の client として扱います。
ただし、人間向け interface と machine interface のどちらでも、同じ read-only のプロダクト境界を維持します。

## root allowlist

MCP server は、1 つ以上の allowed root を受け取ります。

```bash
usedu mcp --stdio --allow-root ~/Library
```

root を渡さない場合、現在のディレクトリだけを allowed root とします。
MCP scan request は、走査前に canonicalize します。
allowlist の外にある request は traversal 前に拒否します。
これにより、symlink を使った allowed root からの脱出も拒否します。

通常の CLI は、明示 path を対象にします。
`usedu` は `/` を暗黙の既定値にしません。

## filesystem policy

走査では metadata だけを読みます。
ファイル内容は読みません。

symbolic link は link entry として数えますが、link 先はたどりません。
既定では、指定 root の filesystem 内だけを走査します。
filesystem boundary をまたぐには、`--cross-file-systems` または同等の MCP argument を明示します。

## output boundary

machine-readable output は structured JSON です。
ファイル名は field として返し、命令文の一部として自然文に埋め込みません。

`displayName` と `displayPath` は表示専用です。
machine client は、1 回の scan 内では `entryId` を使い、可逆的な path identity には `pathRef` を使います。

CLI の machine output では `--redact-paths`、MCP scan では `redactPaths: true` を指定すると display field を伏せられます。
`pathRef` は machine identity であるため残します。
可逆的な path byte を隠す必要がある caller は、信頼できない recipient に `pathRef` を転送しないでください。

## resource control

MCP session は、上限付きの in-process session table と TTL を持ちます。
大量の list operation では cursor pagination を使い、page size を上限内に丸めます。
scan output は `maxOutputEntries` で制限できます。
scan traversal は `maxScanEntries` と `maxScanDurationMs` で制限できます。

これらの制御は cleanup action を許可するものではありません。
`usedu` は、ファイルの削除、移動、隔離、削除推薦を行いません。
