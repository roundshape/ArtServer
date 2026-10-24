# vendor: ArtServer で修正した VectorCraft の crate

[English](README.md) | 日本語

VectorCraft の不具合を、該当する crate だけをここに置いて直し、`Cargo.toml` の
`[patch."https://github.com/storytold/vectorcraft"]` で差し替えている（docs/design-policy.md 8章）。
ここにない VectorCraft の crate は、`Cargo.toml` の commit の GitHub から取ってくる。

いまは差し替えている crate はない（2026-10-25 から）。`Cargo.toml` に `[patch."https://github.com/storytold/vectorcraft"]` もない。

| crate | 元の commit | 修正 | 差分 |
|---|---|---|---|
| （なし） | | | |

## 外した差し替え

| crate | 修正 | 使った期間 | 外した理由 |
|---|---|---|---|
| `vectorcraft-svg` | 日本語フォントの太さ（W0〜W9）が SVG の `font-weight` に書かれず、Illustrator で W6 が標準の太さで表示された（[#2873](https://github.com/storytold/vectorcraft/issues/2873)） | 2026-10-18〜2026-10-25（VectorCraft v0.13.1） | 本家が根本で直した（[#2896](https://github.com/storytold/vectorcraft/pull/2896)）。`vectorcraft-text` の `style_weight` が W0〜W9 を数字 × 100（最低 100）の太さとして読み、太さはこの差し替えと同じ。v0.14.0 からのリリースに入っており、差し替えを外すときに ArtServer を v0.14.0 に上げた |

コピー、差分、その説明は Git の履歴にある（`git log -- vendor/vectorcraft-svg`）。

## コピーの作り方

- ArtServer が参照している commit の `crates/<名前>/src` をそのままコピーして直し、元との差分を
  `patches/<crate>.diff` に残す。
- `Cargo.toml` は元のものがワークスペースの設定（`workspace = true`）を引き継いでいるため、同じ commit の
  値を直接書く。VectorCraft の他の crate は、ArtServer と同じ commit の GitHub を指す。
- テスト（`tests/` と dev-dependencies）は ArtServer のビルドで使わないので要らない。
- `LICENSE-MIT` と `LICENSE-APACHE` を VectorCraft のリポジトリ直下からコピーし（MIT OR Apache-2.0）、
  `about.toml` のその crate の項目をコピーから読む形（`git` ではなく `files`）にする。
- 上の表に crate を足し、その下に症状、原因、修正、影響の範囲、確認、本家への報告を書く。

## VectorCraft を更新するとき

ArtServer の `Cargo.toml` の commit を上げたら、ここの crate も同じ commit に合わせる。VectorCraft の
crate どうしは内部の関数を呼び合っているので、commit がずれるとビルドが通らない。

1. 本家に修正が取り込まれていれば、その crate をここから消し、`Cargo.toml` の `[patch]` の行を外す
   （上の「外した差し替え」に移し、`about.toml` のその crate の項目を `git` に戻す）
2. 取り込まれていなければ、新しい commit の `crates/<名前>/src` をコピーし直し、`patches/<crate>.diff` を当てる
   （当たらなければ、内容を読んで直し、差分を作り直す）
3. `Cargo.toml` の依存の commit と値を、新しい commit のワークスペースの設定に合わせる
4. `cargo test`、`cargo deny check`、ライセンス表示の生成（README の手順）を行う
5. 上の表の「元の commit」を更新する

直す crate が増えて載せ直しが大変になったら、VectorCraft のフォークに切り替える（docs/design-policy.md 8章）。
