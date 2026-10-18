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

## 2026-10-18 VectorCraft のツールの組み込み（文字とフォント）

### 方針

設計方針 1.1節（想定する利用形態）、4.6節（大きいデータの受け渡し）、4.7節（ツールの構成）をユーザーと決めた。4.7節は、VectorCraft の MCP のツールの部分（`call_tool` と trait `Backend`）を、ArtServer が実装する GuardedBackend 越しに使う方式。

### やったこと

- `vectorcraft-mcp` を同じ commit で依存に追加した（ツールの部分だけを使う）。新しい外部ライブラリは増えていない。
- `src/backend.rs`（GuardedBackend）を追加した。`engine.execute` は許可リスト・パス・引数・文書数を検査してから実行、`engine.commands` は許可リストのコマンドだけ、`ui.render` はエンジンの PNG 書き出しで描画、それ以外（`ui.*`、`app.*` など）は断る。
- `src/engine_host.rs`: 作業セッションのスレッドで、VectorCraft のツールを今のファイル制限・キャンセル・制限時間の中で動かせるようにした。コマンドの検査と実行を `execute_checked` にまとめた。
- `src/policy.rs`: 許可リストを約75個に広げた。`file.new`・`document.open`・`document.export` は ArtServer のツールからだけ使える。パスの検査は値の型も見る（文字列は断り、図形の id は通す）。プラグインの効果は断る。
- `src/tools.rs`: VectorCraft のツール16個を、作業セッションの `session` 引数を足して公開した。ArtServer のツールは6個（作業セッション、`document_new`、`import_svg`、`export_document`、新しい `list_fonts`）。重なる10個（`draw_rectangle` などの図形5つ、`set_paint`、`move_objects`、`inspect_document`、`undo`、`redo`）はやめた。サーバーの説明も更新した。
- 起動時の設定 `--font-dirs`（`ARTSERVER_FONT_DIRS`）を追加した。OS のフォントに加えて、指定したフォルダのフォントを使う。
- `about.toml` に `vectorcraft-mcp` を足し、`THIRD_PARTY_NOTICES.md` を生成し直した。README のツールと設定の表を更新した。

### 実行したコマンドと結果

| コマンド | 結果 |
|---|---|
| `cargo build --all-targets --offline --locked` | 成功 |
| `cargo test --offline --locked` | 43件すべて成功（単体30件、E2E 13件） |
| `cargo clippy --all-targets` | 警告なし |
| `cargo fmt --check` | 差分なし |
| `cargo deny check` | advisories ok、bans ok、licenses ok、sources ok（yanked の警告1件は従来どおり） |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | 成功（`vectorcraft-mcp` が VectorCraft の MIT の欄に加わった） |

主な新しいテスト: VectorCraft のツールが GuardedBackend の上で動くこと（日本語の文字、図形、変形、プレビュー）、許可していないコマンドがどのツールからも断られること、`ui.*`・`app.*` が断られること、`command_list` が許可リストのコマンドだけを返すこと、許可リストの全 id がエンジンに存在すること、開放しない分類が許可リストに入っていないこと、公開するツールの一覧がちょうど22個であること、フォントの一覧と絞り込み。

### 気づいたこと

- `--offline` で `Cargo.lock` を更新させると、関係のない箇所（Windows 用の `windows-sys` の参照）まで古いバージョンに寄せられた。元の `Cargo.lock` に `vectorcraft-mcp` の分だけを差し込み、`--locked` で通ることを確かめた。

### 未確認

- Claude Code から、新しいツールを使った確認（ArtServer の再起動が必要）
- 日本語の文字が、ヒラギノなど実際の日本語フォントで描かれているか（テストは文字の配置と書き出しだけを確かめている）
- エンジンを動かしてのコマンド数の確認（約760は静的に数えた値）

## 2026-10-18 Illustrator での確認と、SVG の文字の太さの修正

### 確認したこと

- 新しいバイナリの ArtServer に curl で要求を送り、日本語の文字（ヒラギノ角ゴ W6、ヒラギノ明朝 W6、ヒラギノ丸ゴ W4、縦書き）と図形の文書を作って、`~/Desktop/artserver-output/` に SVG・アウトライン化した SVG・PDF を書き出した。フォントはすべて「exact」（ぴったり一致）。
  - この会話の Claude Code はツールの一覧を読み込み直していなかったため、新しいツールを Claude Code から直接は使っていない（未確認のまま）。
- ユーザーが Illustrator で開いたところ、アウトライン化した SVG は正しく、文字を残した SVG では角ゴと明朝が細い太さ（W3 など）で表示された。

### 原因と修正

| 項目 | 内容 |
|---|---|
| 原因 | VectorCraft の SVG の書き出しは、スタイル名から太さを `vectorcraft_text::style_weight` で決める。この関数は英語の単語（Light、Bold など）しか読まないため「W6」は 400 になり、400 のときは `font-weight` を書かない |
| 直し方 | ユーザーと決めた方針（設計方針 8章）で、`vectorcraft-svg` だけを `vendor/vectorcraft-svg` にコピーし、`src/css.rs` の `font_descriptor` で W0〜W9 を数字 × 100 の太さとして読むようにした。`Cargo.toml` の `[patch."https://github.com/storytold/vectorcraft"]` で差し替える |
| 影響の範囲 | SVG（と CSS）の書き出しだけ。フォントを選ぶ処理は変えていない |
| 記録 | `vendor/README.md`（修正の内容、載せ直しの手順）、`vendor/patches/vectorcraft-svg.diff`（元のコードとの差分） |
| ライセンス | コピーした crate に VectorCraft の `LICENSE-MIT` と `LICENSE-APACHE` を置いた。`cargo about` がそれを ArtServer 自身のライセンスとも数えたので、`about.toml` で ArtServer のライセンスのファイルを明示した |

### 実行したコマンドと結果

| コマンド | 結果 |
|---|---|
| `cargo build --all-targets --offline` | 成功。`Cargo.lock` の変更は `vectorcraft-svg` の取得元の1行だけ |
| `cargo test --offline --locked` | 44件すべて成功（単体31件、E2E 13件）。新しいテスト: W6・W3 のスタイル名が SVG の `font-weight` 600・300 になり、Regular は付かず、Bold は bold のまま |
| `cargo clippy --all-targets` / `cargo fmt --check` | 警告なし / 差分なし |
| `cargo deny check` | すべて ok（yanked の警告1件は従来どおり） |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | 成功 |

### 修正後の確認

- ArtServer を修正後のバイナリで起動し直し、サンプルを書き出し直した。`sample.svg` では、ヒラギノ角ゴ W6 と明朝 W6 の文字に `font-weight="600"` が付いた（丸ゴ W4 は標準の太さなので付かない）。
- ユーザーが Illustrator で開き、角ゴと明朝が W6 の太さで表示されることを確認した。PostScript 名を書く追加の修正は不要だった。
- 本家 VectorCraft に Issue で報告した: [storytold/vectorcraft#2873](https://github.com/storytold/vectorcraft/issues/2873)。重複する報告はなく、本家の `main`（`73f40e3`）でも未修正だった。文面はユーザーが確認し、投稿をユーザーの許可のもとで行った。

### 未確認

- Claude Code から新しいツールを使った確認（ツールの一覧の読み込み直しが必要）

## 2026-10-18 大きい書き出し結果の URL での受け渡し

### 方針

設計方針 4.6節のうち、書き出しの部分を実装した。預かる場所は**メモリ**、取りに来る窓口の認証は **MCP と同じ Bearer トークン**（ユーザーが決定）。

### やったこと

- `src/files.rs` を追加した。書き出し結果の預かり場所（`FileStore`）と、`GET /files/{id}` の窓口。
  - 預けた主体だけが取れる（ほかの主体には 404）。ID は32バイトの乱数
  - 期限は既定 10 分、合計は既定 256MiB（超えたら断る）、1つの作業セッションにつき 8 個（超えたら古いものから捨てる）
  - 期限が過ぎたものは60秒ごとの掃除で、閉じた作業セッションのものは `session_close` で捨てる
  - Host と Origin を `/mcp` と同じ許可リストで検査する
  - ファイル名は文書の名前から作り、`Content-Disposition` に UTF-8 の名前も書く
- `export_document` に `delivery`（auto・inline・url）を足した。auto では SVG は 32KB まで、PNG は 256KB まで返事に入れ、PDF とそれより大きいものは URL を返す。
- 起動時の設定 `--file-ttl-secs`・`--max-file-mib` を追加した。
- サーバーの説明とツールの説明に、URL はトークンを付けて取ることを書いた。README に「大きい書き出し結果の受け取り」の節を足した。

### 実行したコマンドと結果

| コマンド | 結果 |
|---|---|
| `cargo build --all-targets --offline` | 成功（依存は増えておらず、`Cargo.lock` も変わっていない） |
| `cargo test --offline --locked` | 52件すべて成功（単体38件、E2E 14件） |
| `cargo clippy --all-targets` / `cargo fmt --check` | 警告なし / 差分なし |

主な新しいテスト: 預けた主体にだけ返ること、期限切れ、作業セッションごとの個数の上限、合計の上限、作業セッションを閉じたときの削除、ファイル名、Host・Origin の判定（単体）。PDF が URL で返り、トークン付きの GET で PDF が取れること、トークンなしは 401、違う ID は 404、許可されていない Host・Origin は 403、SVG も `delivery: url` で URL にできること、作業セッションを閉じると 404 になること（E2E）。

### 気づいたこと

- AI（Claude Code）は ArtServer のトークンを知らないので、URL を取りに行かせるには、トークンのファイルの場所を伝えておく必要がある（README に記載）。使い勝手が悪ければ、短い期限の、URL だけで取れる方式との比較を改めて検討する。

### Claude Code での確認（2026-10-18）

ArtServer を起動し直し、Claude Code を `claude -c` で起動し直して（`/mcp` の Reconnect だけではツールの一覧が入れ替わらなかった）、AI が新しいツールを直接使えることを確かめた。

| 確かめたこと | 結果 |
|---|---|
| `list_fonts`（絞り込み、スタイルの一覧） | ヒラギノ丸ゴの2ファミリー、ヒラギノ角ゴの W0〜W9 が返った |
| `add_text`（日本語、フォントの指定） | 角ゴと丸ゴで文字を置けた |
| `command_run`（`text.setStyle`、`text.setText`、`text.fonts`） | W6 への変更、文字の差し替え、使用フォントの確認ができた |
| `command_run` で許可していないコマンド（`prefs.set`） | 「not allowed」で断られた |
| `render_preview` | プレビュー画像が表示された |
| `export_document`（PDF） | 中身の代わりに URL が返った。トークンを付けた curl で PDF（約13KB）を取れ、トークンなしは 401 |

- `claude -c` で再開した会話では、前からあるツール（`export_document` など）の説明が古いまま AI に見えていた（`delivery` の引数が見えない）。既定の返し方で動くので確認には支障なかったが、新しい会話で始めれば最新の説明が見えるはず。
- AI が URL を取りに行けたのは、トークンのファイルの場所を知っていたため。ほかの環境では、README のとおり場所を伝える必要がある。

## 2026-10-18 書き出し結果の受け取りをトークン不要の URL に変更

### 経緯

Claude Code での確認の後、ユーザーが返ってきた URL をブラウザで開いたところ、`unauthorized` になった（トークンを求める設計どおりの動き）。ブラウザや、curl を実行できないクライアント（Claude Desktop など）では取れず、AI にもトークンの場所を教える必要があるため、ユーザーの判断で「URL だけで取れる（期限付き）」方式に変えた。

### やったこと

- `src/files.rs`: 預けた主体の確認をやめ、URL の ID（256ビットの乱数）だけで取れるようにした。期限（既定 10 分）までは何度でも取れる（1回取ったら消す方式は、リンクの先読みで消えるおそれがあるので採らなかった）。ブラウザの中で開かせず必ずダウンロードにするヘッダーを足した（`Content-Security-Policy: sandbox`、`X-Content-Type-Options: nosniff`、`Referrer-Policy: no-referrer`）。
- `src/lib.rs`: `/files/{id}` をトークンの検査の外に移した。Host と Origin の検査は残した。
- `src/tools.rs`: サーバーの説明、`export_document` の説明、応答の文言（`auth` → `note`）を、トークン不要に合わせて直した。
- README・設計方針（4.6節、13.2節）を更新した。

### 実行したコマンドと結果

| コマンド | 結果 |
|---|---|
| `cargo build --all-targets --offline` | 成功 |
| `cargo test --offline --locked` | 52件すべて成功（単体38件、E2E 14件）。E2E で、トークンなしで PDF が取れること（2回取れること）、安全のためのヘッダー、違う ID は 404、許可されていない Host・Origin は 403、作業セッションを閉じると 404 を確かめた |
| `cargo clippy --all-targets` / `cargo fmt --check` | 警告なし / 差分なし |

### ブラウザでの確認

ArtServer を起動し直し、Claude Code から PDF を書き出して返ってきた URL を、ユーザーがブラウザで開いた。トークンなしで `browser-download-test.pdf` としてダウンロードできた。

### 次の作業

1. Windows のビルドの確認（手動で実行する GitHub Actions）
2. VectorCraft の更新への追従と、yoke-derive の更新。#2873 の対応状況も確かめる
3. 読み込みのアップロード（PDF や `.ai` の読み込みを許可するとき。設計方針 4.6節）
