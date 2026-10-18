# ArtServer 進捗記録

## 2026-10-17 Phase 0（調査・方式決定）

- VectorCraft（`~/MyTasks/RoundShape/vectorcraft`、commit `fdac986bb49b3e3a9ddd3cda4e88661a6f0aa50c`）をソースの静的読解で調査した。結果は `docs/architecture.md`。
- 案A-2（`vectorcraft-engine` をライブラリとして組み込み、MCP層は ArtServer で作る）の採用をユーザーが承認した。方針は `docs/design-policy.md`。
- 外部レビュー（`docs/ARTSERVER_PROCESS_ISOLATION_REVIEW.md`）を受けて、将来のプロセス分離の方針を設計書の12章に追加した。
- MCP の最新仕様（2026-07-28 版）でプロトコル上のセッションが廃止されたことを確認し、作業セッションをハンドルで受け渡す設計に改めた（設計書 4.4、4.5節）。

## 2026-10-17 Phase 1（MVP）の最初の実装

### 環境

| 項目 | 内容 |
|---|---|
| OS | macOS（Darwin 27.0.0、Apple Silicon） |
| Rust | 1.99.1（Homebrew の `rust` で導入） |
| 主な依存 | vectorcraft-engine（git、commit 固定）、rmcp 3.5.1、axum 0.8.9、tokio 1.53.1 |
| PDF バックエンド | VectorCraft と同じ `[patch.crates-io]` を写し、krilla と hayro-* が craft-libs 版になっていることを `Cargo.lock` で確認 |

### 実行したコマンドと結果

| コマンド | 結果 |
|---|---|
| `cargo fetch` | 成功。`Cargo.lock` は VectorCraft のものを起点にした |
| `cargo build --all-targets` | 成功（初回 約2分） |
| `cargo test` | 27件すべて成功（単体17件、E2E 10件） |
| `cargo clippy --all-targets` | 警告なし |
| `cargo fmt --check` | 差分なし |
| バイナリを起動して curl で確認 | `/healthz` 応答、トークンなしは 401、`tools/list` で15ツール、セッション作成 → 文書作成 → 矩形 → SVG 書き出しまで成功 |

### 実装したもの

- `artserver` バイナリ（127.0.0.1 で待ち受け。ループバック以外は `--allow-public-bind` が必要）
- MCP Streamable HTTP（`POST /mcp`）。rmcp を使い、2026-07-28 版とそれ以前の版の両方に対応。セッションIDは発行しない
- `GET /healthz`
- Bearer トークン認証（定数時間比較、32文字以上を要求）
- Host / Origin 検査、本文サイズ上限（rmcp の設定で有効化）
- 作業セッション（ハンドルを主体に結びつけて発行、上限数、アイドル破棄）
- セッションごとのエンジン実行スレッド（届いた順に実行、待ち行列が溢れたら混雑エラー、制限時間を超えたらキャンセル、クライアントの切断でキャンセル）
- コマンドの許可リスト、ファイルパス引数の拒否、引数の未知キー検査、文書数と Undo 履歴の上限
- エンジンのファイルアクセスを空の一時ディレクトリの中に閉じ込める（読み取りはその中だけ、書き込みは一切不可）
- ツール15個（セッション、文書作成、SVG 読み込み、図形5種、塗り、移動、文書の状態、書き出し、Undo/Redo）

### テストで確かめている異常系

| 異常系 | テスト |
|---|---|
| 認証失敗（トークンなし・誤り） | `requests_without_the_right_token_get_401` |
| 不正な Origin / Host | `foreign_origins_and_hosts_are_refused` |
| 本文サイズ超過 | `oversized_bodies_are_refused` |
| タイムアウト | `a_call_over_the_time_limit_is_cancelled` |
| 不正な入力（未知のセッション、未知の引数 `path`） | `unknown_sessions_and_unknown_arguments_are_refused` |
| 文書がない状態での描画 | `drawing_without_a_document_reports_a_tool_error` |
| セッション数の上限 | `sessions_are_capped`、`session::tests` |
| 許可外のコマンド・パス引数 | `policy::tests`、`engine_host::tests::refused_commands_never_reach_the_engine` |
| 許可外のパス（SVG にリンクされたファイル） | `engine_host::tests::linked_files_outside_the_roots_are_not_read` |
| 他の主体のハンドル | `session::tests::handles_are_bound_to_their_owner` |

### 未実装・既知の制限

- TLS（リバースプロキシで終端する前提）、複数ユーザーと権限管理、レート制限、プロセス分離
- ファイルパス指定での入出力、日本語フォントの同梱
- 制限時間を超えた処理はキャンセルを求めるが、エンジンが止まるまでそのセッションは混雑中になる
- `THIRD_PARTY_NOTICES.md` は Phase 0 の監査をもとにした要約。バイナリ配布の前に `cargo about` などで全文を生成する必要がある
- `cargo deny` によるライセンス・脆弱性の検査は未実施

### 未確認

- Claude Code から実際に接続しての動作確認（README に手順を記載済み。ユーザーの Claude Code の設定に追加する必要があるため未実施）
- Linux でのビルドと動作
- 古いクライアント（`initialize` と `Mcp-Session-Id` を使うもの）での動作。E2E テストは rmcp 3.5.1 のクライアントで行っている

### 次の作業

1. Claude Code からの接続確認
2. `LICENSE`（MIT）の追加（著作権者の表記を決める必要がある）
3. `cargo deny` / `cargo about` の導入とライセンス表示の生成
4. Linux でのビルド・テスト
5. Phase 2: 日本語フォントの方針決定、ツールの拡充（テキスト、レイヤー、グループ、画像など）

## 2026-10-18 Claude Code からの接続確認と修正

### やったこと

- `LICENSE`（MIT）を追加した。著作権者の表記は `Copyright (c) 2026 RoundShape`（ユーザーが決定）。
- Claude Code から接続して動作を確かめた。
  - トークンは `~/.config/artserver/token` に置き、`./target/debug/artserver --token-file ~/.config/artserver/token` で起動した
  - 登録は `claude mcp add --transport http artserver ...`。既定の local スコープなので、登録したフォルダで起動した Claude Code にだけ現れる（ホームフォルダで起動して見えなかったことがある）
  - `session_create` → `document_new` → `draw_rectangle` → `export_document`（PNG と SVG）まで成功

### 見つかった不具合と修正

| 項目 | 内容 |
|---|---|
| 症状 | SVG 書き出しで、Claude Code には `{"bytes":445,"format":"svg","warnings":[]}` だけが表示され、SVG 本文が届かない |
| 原因 | `content` に SVG 本文、`structuredContent` に要約という別々の中身を返していた。Claude Code は `structuredContent` があるとそちらを表示する。MCP の仕様では、両者は同じ結果を表すことになっている |
| 修正 | SVG と PNG は `structuredContent` を付けず、本文だけを返す。警告があるときだけ2つ目のテキストとして `{"warnings":[...]}` を添える（`src/tools.rs` の `export_result`）。PDF は変更なし |
| テスト | 単体テストを3件追加。E2E で、SVG と PNG の結果に `structuredContent` がないことを確かめるようにした |
| 見逃した理由 | E2E は rmcp のクライアントで `content` を直接読んでいたため、Claude Code での見え方の違いを検出できなかった |

### 実行したコマンドと結果

| コマンド | 結果 |
|---|---|
| `cargo build --all-targets` | 成功 |
| `cargo test` | 30件すべて成功（単体20件、E2E 10件） |
| `cargo clippy --all-targets` | 警告なし |
| `cargo fmt --check` | 差分なし |
| サーバーを再起動して Claude Code から SVG 書き出し | SVG 本文が表示された |

### 未確認

- Linux でのビルドと動作
- 古いクライアント（`initialize` と `Mcp-Session-Id` を使うもの）での動作
- Claude Code から PDF 書き出しを呼んだときの扱い。今は base64 を `structuredContent` に入れて返すので、大きい文書では会話が重くなるおそれがある

## 2026-10-18 依存ライブラリのライセンス・脆弱性の検査

### やったこと

- cargo-deny 0.20.2 と cargo-about 0.9.2 を Homebrew で導入した。
- `deny.toml` を作った。
  - 許可するライセンス: MIT、Apache-2.0、BSD-2-Clause、BSD-3-Clause、Zlib、Unicode-3.0、BSL-1.0。`jpeg-encoder` だけ IJG も許可
  - 対象: macOS と Linux（aarch64・x86_64）。テスト専用の依存は除く
  - 取得元: crates.io と、GitHub の storytold/vectorcraft・storytold/craft-libs だけ
- `about.toml` と `about.hbs` を作り、`THIRD_PARTY_NOTICES.md` を要約から全文入りに置き換えた（286 クレート、ライセンス文 141 種類、約 226KB）。
  - 前半は `about.hbs` に手書き: VectorCraft の由来、同梱フォント4種の著作権表示と OFL-1.1 の全文、Adobe CMap と PDFium フォントの BSD-3-Clause の全文、IJG の文言、krilla の MIT 表示。全文は、ビルドに使うソースのファイルからそのまま差し込んだ
  - 後半は cargo about が `Cargo.lock` から生成

### 実行したコマンドと結果

| コマンド | 結果 |
|---|---|
| `cargo deny check` | advisories ok、bans ok、licenses ok、sources ok。警告1件（yanked、下記） |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | 成功 |

### 分かったことと対処

| 項目 | 内容 |
|---|---|
| LICENSE ファイルを同梱していないクレート | VectorCraft の21クレート、craft-pdf、krilla など32個で、著作権者が空欄のひな形（`Copyright (c) <year> <copyright holders>`）になっていた。`about.toml` の `clarify` で正しいファイルを指定した。chrono は cargo about に組み込みの対処（`workarounds`）で解決した |
| 生成にネットワークが必要 | alloc-stdlib、wasmi（4クレート）、wasmparser はクレートに LICENSE がないため、元のリポジトリの該当 commit から取得する。checksum が合わなければ生成が失敗するので、文面が黙って変わることはない |
| krilla | craft-libs に取り込まれた版に LICENSE がなく、自動では取れない。元のリポジトリの MIT 表示を `about.hbs` に手書きで載せた。生成部分では汎用の MIT の文面の下に並ぶ |
| テスト専用の依存 | cargo deny は、`[dev-dependencies]` で有効にした rmcp の機能（reqwest、aws-lc-sys など）も数える。`cargo tree -e normal --target all` でバイナリには入らないことを確認した。数えすぎる分には安全側なのでそのままにした。cargo about は正しく除外する |
| VectorCraft の参照 | commit 固定の git 依存はバージョン指定がないため、`wildcards` の検査に引っかかる。`deny.toml` で許可した |
| yanked | `yoke-derive 0.8.3` が取り下げ済み（0.8.4 あり、取り下げ理由の記載なし）。`cargo update -p yoke-derive` で上げられるが未実施 |

## 2026-10-18 Linux でのビルド確認（GitHub Actions）

### 方針

Linux で ArtServer を使う人は少ない見込み。VectorCraft が Linux に対応していることもあり、ビルドが通る状態は保つが、テストと動作確認はしない（ユーザーの判断）。

### やったこと

- `.github/workflows/build.yml` を追加した。push（main）と pull request のたびに、ubuntu-latest で `cargo build --locked` を実行する。
  - 依存のビルド結果を Swatinem/rust-cache でキャッシュする
  - 同じブランチで新しい push があれば、古い実行は止める
  - 権限はリポジトリの読み取りだけ

### 結果

| 項目 | 内容 |
|---|---|
| 実行 | run 39084451667（commit `4390de1`） |
| 結果 | 成功。警告・エラーなし |
| 所要時間 | 全体 約16分半、うちビルド 15分49秒。ArtServer 自身のコンパイルとリンクは約3分 |

- 時間がかかるのは、private リポジトリの GitHub のマシンが2コアであることと、開発用ビルドでも依存に `opt-level = 2` をかけているため。2回目からはキャッシュが効く。`Cargo.lock` が変わると再び長くなる
- 長すぎる場合は `cargo check` に替える案がある（リンクまでは確かめられない）
- GitHub の案内: `ubuntu-latest` は 2026-10-19 から順に Ubuntu 26 に切り替わる

### 次の作業

1. yoke-derive を上げるかどうかの判断（上げる場合は再ビルドとテスト）
2. Phase 2: 日本語フォントの方針決定、ツールの拡充（テキスト、レイヤー、グループ、画像など）、PDF など大きい出力の受け渡し方（ファイル出力など）
