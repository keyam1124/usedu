# ファイルシステム意味論

[English](semantics.md) | [日本語](semantics.ja.md)

この文書は、人間向け report と将来の machine-readable interface が共有する集計用語を定義します。

## サイズ field

`usedBytes` は、entry とその子孫に帰属するファイルシステム上の割り当て済みサイズです。
directory では、現在の accounting policy に従い、`ownBytes` と子孫の割り当て済みサイズを含みます。
leaf entry では、その entry 自身の割り当て済みサイズです。

`ownBytes` は、entry 自身の割り当て済みサイズです。
directory の場合、配下の合計ではなく、directory record 自身の割り当て済みサイズを表します。

`uniqueBytes` は、選択された hard-link policy のもとで entry が一意に所有する割り当て済みサイズです。
`sharedBytes` は、hard link など、共有された file identity を通じて存在する割り当て済みサイズです。
現在の scanner は、この 2 つをまだ個別 field として公開していません。
JSON v2 と snapshot format では分離します。

すべての size field は allocated bytes を表します。
logical byte length でも reclaimable bytes でもありません。
人間向け出力の表示 label は `Used` のままです。

## count field

将来の protocol field では、leaf count を kind ごとに分けます。

- `regularFileCount`
- `directoryCount`
- `symlinkCount`
- `otherCount`

`directoryCount` は directory 自身を含みます。
root directory の直下に child directory が 1 つある場合、`directoryCount` は `2` です。

現在の scanner の `file_count` は regular file より広い意味を持ちます。
regular file、symlink、other leaf entry をまとめて数えます。
JSON v2 では、この曖昧さを引き継ぎません。

## entry kind

`directory` は container として走査される directory です。
`regularFile` は regular file です。
`symlink` は symbolic link entry です。
`other` はそれ以外の filesystem entry です。

symbolic link は link entry として数えますが、link 先はたどりません。

## ファイルシステム境界

既定では、走査は指定 root と同じ device 内に留まります。
別の mounted filesystem 上の entry は、利用者が cross-filesystem scanning を明示しない限り skip します。

machine-readable output は、有効になっている filesystem-boundary policy を出力します。
この policy による skip は permission error ではありません。

## hard link

strict accounting では、同じ device と inode を持つ regular file を重複計上しないようにします。
現在の scanner は scan 内の first-seen ownership を使うため、将来の実装では deterministic owner を定義するか、unique bytes と shared bytes を分けます。

fast mode では、hard-linked file を重複計上することがあります。
machine-readable output は、この違いを単なる性能 option ではなく accounting semantics として公開します。

## strict mode と fast mode

strict mode は accounting consistency を優先します。
`symlink_metadata` を使い、directory 自身の allocation を含め、symlink をたどらず、`--cross-file-systems` が指定されない限り指定 root の filesystem boundary を維持します。

fast mode は latency を優先します。
一部の経路で directory 自身の allocation を省略したり、hard link を重複計上したり、strict mode なら skip する filesystem boundary をまたいだりすることがあります。
machine-readable output では、fast mode を approximate accounting として表現します。

## partial result

scan は complete、partial、cancelled、limit-reached のいずれかになり得ます。
permission error や entry 単位の read failure は partial scan issue であり、それだけで command 全体の fatal error とは限りません。

`error` は、entry を読めない、または処理できないことを意味します。
`skip` は、filesystem-boundary enforcement などの policy によって意図的に entry を含めないことを意味します。
machine-readable output では、この 2 つを区別します。

## APFS caveat

APFS clone、snapshot、compression、sparse file、File Provider の挙動により、allocated bytes と削除で回復可能な bytes は一致しないことがあります。
`usedu` は cleanup safety を保証せず、file や directory を削除すれば表示上の `Used` がそのまま回復するとも約束しません。
