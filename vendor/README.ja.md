# vendor: ArtServer で修正した VectorCraft の crate

[English](README.md) | 日本語

VectorCraft の不具合を、該当する crate だけをここに置いて直し、`Cargo.toml` の
`[patch."https://github.com/storytold/vectorcraft"]` で差し替えている（docs/design-policy.md 8章）。
ここにない VectorCraft の crate は、`Cargo.toml` の commit の GitHub から取ってくる。

| crate | 元の commit | 修正 | 差分 |
|---|---|---|---|
| `vectorcraft-svg` | `904f0fe5227bd987d55319e89a38b4b8f046998c`（v0.13.1） | SVG の文字の太さ（下記） | `patches/vectorcraft-svg.diff` |

## vectorcraft-svg: 日本語フォントの太さ（W0〜W9）が SVG に書かれない

- **症状**: ヒラギノ角ゴ W6 などで書いた文字を SVG に書き出すと、`font-weight` が付かず、
  Illustrator などで開くと標準の太さ（W3 など）で表示される。PDF とアウトライン化した SVG は正しい。
- **原因**: SVG の書き出しは、スタイル名から太さを `vectorcraft_text::style_weight` で決める。
  この関数は英語の単語（Light、Medium、Bold など）しか読まないので、「W6」は 400（標準）になり、
  400 のときは `font-weight` を書かない（`src/css.rs` の `font_props`）。
- **修正**: `src/css.rs` の `font_descriptor` で、W0〜W9 を数字 × 100 の太さとして読む
  （W3 は 300、W6 は 600。W0 と W1 は 100）。それ以外は今までどおり `style_weight` を使う。
  可変フォントの `wght` 軸の設定があるときは、それを先に使う（v0.13.1 の本家の動きのまま）。
  埋め込みフォントの `@font-face` の太さも同じ関数を使うので、そろって直る。
- **影響の範囲**: SVG の書き出し（と CSS の書き出し）だけ。フォントを選ぶ処理（`vectorcraft-text`）は変えていない。
- **確認**: 修正後の SVG を Illustrator で開き、ヒラギノ角ゴ W6・明朝 W6 が W6 の太さで表示されることを確かめた（2026-10-18）。
- **本家への報告**: [storytold/vectorcraft#2873](https://github.com/storytold/vectorcraft/issues/2873)（2026-10-18）。
  根本の直し方として、`vectorcraft-text` の `style_weight` で W0〜W9 を読む案を添えた（フォントを選ぶ処理にも効く）。
  本家で直ったら、この差し替えを外す。
- **載せ直しの記録**: 2026-10-18 に v0.13.1 へ上げたとき、本家の `font_descriptor` が可変フォントの `wght` 軸を
  先に見る形に変わっていたので、差分は当てずに内容を読んで載せ直した。v0.13.1 でも #2873 は直っていない。

## コピーの作り方

- 元の commit の `crates/svg/src` をそのままコピーし、`patches/` の差分を当てた。
- `Cargo.toml` は元のものがワークスペースの設定（`workspace = true`）を引き継いでいるため、同じ commit の
  値を直接書いた。VectorCraft の他の crate は、ArtServer と同じ commit の GitHub を指す。
- テスト（`tests/` と dev-dependencies）は ArtServer のビルドで使わないので含めていない。
- v0.13.1 から `craft-docpath`（craft-libs、rev `82a24b0`）に依存するので、`Cargo.toml` に足した。
- `LICENSE-MIT` と `LICENSE-APACHE` は VectorCraft のリポジトリ直下のもの（MIT OR Apache-2.0）。v0.13.1 でも変わっていない。

## VectorCraft を更新するとき

ArtServer の `Cargo.toml` の commit を上げたら、ここの crate も同じ commit に合わせる。VectorCraft の
crate どうしは内部の関数を呼び合っているので、commit がずれるとビルドが通らない。

1. 本家に修正が取り込まれていれば、その crate をここから消し、`Cargo.toml` の `[patch]` の行を外す
2. 取り込まれていなければ、新しい commit の `crates/<名前>/src` をコピーし直し、`patches/<crate>.diff` を当てる
   （当たらなければ、内容を読んで直し、差分を作り直す）
3. `Cargo.toml` の依存の commit と値を、新しい commit のワークスペースの設定に合わせる
4. `cargo test`、`cargo deny check`、ライセンス表示の生成（README の手順）を行う
5. 上の表の「元の commit」を更新する

直す crate が増えて載せ直しが大変になったら、VectorCraft のフォークに切り替える（docs/design-policy.md 8章）。
