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
