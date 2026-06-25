# JSON 契約修正計画

[English](json-contract.md) | [日本語](json-contract.ja.md)

この文書は B01 の互換性契約と migration plan を記録します。

## 現在の契約上の問題

`usedu report --json` は、人間向け report に近い構造を serialize しています。
CLI はすでに複数の report option を受け取りますが、それらが JSON output に一貫して反映されていません。
agent-facing interface では、option が黙って無視されることは危険です。

現在の JSON output は、display path を identity のような field として使っています。
これは表示用 data としてのみ許容できます。
非 UTF-8 path では lossy UTF-8 conversion によって path が変化し得るためです。

## 互換性方針

現在の `--json` output は current JSON format として維持します。
既存 format をその場で変更せず、明示的な machine format を使います。

```bash
usedu report PATH --format json-v2
usedu report PATH --format ndjson
usedu schema json-v2
usedu snapshot PATH > scan.usedu.json
usedu compare before.usedu.json after.usedu.json
```

現在の `--json` flag は current JSON format の documented alias です。
stdout には progress や diagnostics を混ぜません。

## JSON v2 envelope

JSON v2 は versioned envelope を使います。

```text
schemaVersion
scanId
status
semantics
effectiveOptions
root
entries
issueSummary
issues
nextCursor
```

`semantics` には次を含めます。

- `sizeMetric: allocated`
- accounting source
- accounting accuracy
- hard-link policy
- filesystem-boundary policy
- symlink policy
- directory own bytes を含むかどうか
- `reclaimableBytesKnown: false`

`effectiveOptions` には、depth、top limit、file inclusion、directory-only filtering、sort、fast mode、cross-filesystem policy、output limit の解決済み値を含めます。

## option の扱い

JSON v2 では、report option を黙って無視しません。

- `--top` は ranking-style result set と top-file result set の件数を制限する。
- `--sort` は sort が要求される場所の deterministic ordering を制御する。
- `--dirs-only` は ranked entries を directory に絞る。
- `--files` は structured result に top-file entries を含める。
- `--depth` は tree view が要求される場合だけ retained tree depth を制御する。flat entry list は pagination または明示的な limit を使う。
- `--errors` は issue details を含める。issue count は常に利用できる。
- unsupported option combination は、走査前に structured CLI error として返す。

## identity と path

`displayPath` と `displayName` は表示専用です。
JSON v2 では、scan-local reference として `entryId` を使い、snapshot の可逆的な path identity には `pathRef` を使います。
lossy string を唯一の identity にしてはいけません。

## test

B01 では次の golden test を追加します。

- current JSON compatibility
- JSON v2 envelope fields
- `effectiveOptions` への option 反映
- `--top`、`--sort`、`--dirs-only`、`--files`、`--depth`
- structured issues
- 非 UTF-8 path と control-character path の display safety
