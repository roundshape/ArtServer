# ArtServer アーキテクチャ調査・方式決定（Phase 0）

- 作成日: 2026-10-17
- ステータス: **案A-2の採用が決定（2026-10-17 ユーザー承認）**。本書はPhase 0時点の調査記録。以降の設計方針は `docs/design-policy.md` を正とする（MCP最新仕様への対応でセッションの扱いなどが変わっている）。
- 調査対象: VectorCraft `~/MyTasks/RoundShape/vectorcraft`
  - origin: https://github.com/storytold/vectorcraft.git
  - commit: `fdac986bb49b3e3a9ddd3cda4e88661a6f0aa50c`（2026-10-17、v0.12.0 + 335 commits）
- 調査方法: ソースの静的読解のみ。ビルド・テストは未実施（後述の通り開発機にRust未導入）。
- 信頼性の優先順位: コード → コメント → 関数名。以下の `path:line` は VectorCraft リポジトリ内の位置。

---

## 1. 結論（要約）

**案A（VectorCraftのcrateをライブラリとして参照）を採用し、そのうち「`vectorcraft-engine` を直接組み込み、MCP層はArtServer側で設計する」形（案A-2）を推奨する。**

- エンジン層（L0〜L5）はGUIに依存せず、描画もCPUのみ（`vello_cpu`）。サーバーへの組み込みに構造上の障害はない。
- エンジンは「コマンドID + JSON引数」で全機能を呼べる（`Session::execute`）。MCPツールへの対応付けが容易。
- 既存の `vectorcraft-mcp` は **stdio・単一ユーザー・信頼された手元利用** を前提とした作りで、ネットワーク公開・複数セッションには前提が合わない。参考実装として扱い、依存はしない。
- 一方、**エンジンにはプロセス全体で共有される可変状態がある**（カラー設定、フォントDB、プラグイン登録など）。MVPでは「その状態を変えるコマンドを許可しない」ことで対処し、将来はプロセス分離に移行できる構造にしておく。

---

## 2. 調査結果

### 2.1 crate構成とGUI依存

VectorCraft自身が層構造を定義し、`cargo xtask layers` で検査している（`docs/architecture.md:15-31`、`xtask/src/layers.rs`）。

| 層 | crate | ArtServerでの扱い |
|---|---|---|
| L0 | `geom`, `color`, `affinity` | 間接依存 |
| L1 | `doc`（文書モデル） | 間接依存 |
| L2 | `pathops`, `brush`, `trace`, `text`, `effects`, `plugins`, `three-d` | 間接依存 |
| L3 | `render`, `svg`, `pdf`, `format`, `eps`, `cad`, `metafile` | 間接依存 |
| L4 | `tools` | 間接依存 |
| L5 | **`engine`**（セッション・履歴・選択・約700コマンド・ファイルI/O） | **直接依存** |
| L6 | `ui-egui`（GUI）, `mcp`（stdio MCP） | 使わない（`mcp`は参考実装） |
| L7 | `apps/vectorcraft`（GUI）, `apps/vectorcraft-cli`, `apps/vectorcraft-web` | 使わない |

- **L6未満はUI系crate（egui/eframe/winit/rfd/craft-ui）への依存禁止**（`AGENTS.md:45`）。
- **L0〜L5はwasm32でビルドできることが必須**（`AGENTS.md:47`）。このためOSネイティブ依存がほぼない。
- `engine` / `mcp` / `vectorcraft-cli` の依存グラフにGUI系crateは含まれない（各 `Cargo.toml` で確認）。
- 規模: 全体約55万行。`engine` 約14.5万行、`ui-egui` 約11.8万行。

### 2.2 エンジンAPI（ヘッドレス利用）

- 入口は `Session::execute(&mut self, id: &str, params: &Value) -> Result<Value>`（`crates/engine/src/lib.rs:1521`）。パニックガード付き。
- `Session` は複数文書を保持できる（`docs: Vec<DocState>`、`lib.rs:1194-1306`）。文書ごとに選択状態・Undo履歴を持つ。
- MVPで必要な流れはすべてコマンドで完結し、**ファイルシステムを使わずに済む**:

```text
file.new          {width, height, ...}                → {index, previewMode}   cmd/edit.rs:17
shape.rectangle   {x, y, width, height, radius?}      → {id}                   cmd/create.rs:13
paint.setFill     {color, ids}                                                 cmd/paint.rs:15
document.export   {format: "svg"|"pdf"|"png", ...}    → {dataBase64, ...}      cmd/fileio/mod.rs:121
                  （path 省略時はファイルに書かず Base64 で返す）
```

- 注意: 引数の未知キー検査（`check_params`）は `Session::execute` 内では `#[cfg(test)]` のときしか動かない（`lib.rs:1523-1526`）。組み込む側で明示的に呼ぶ必要がある（`vectorcraft-mcp` も `crates/mcp/src/headless.rs:128-134` で呼んでいる）。
- 引数仕様（`params`）はJSON Schemaではなく説明文字列。MCPツールの入力スキーマはArtServer側で定義する必要がある。

### 2.3 既存MCP（`crates/mcp`）

| 項目 | 内容 | 根拠 |
|---|---|---|
| 通信方式 | **stdioのみ**。HTTP/SSE/Streamable HTTPは未実装 | `crates/mcp/Cargo.toml:6`、`docs/mcp.md:275` |
| JSON-RPC | 手書き実装（`serde_json::Value`）。MCP SDK不使用 | `crates/mcp/src/server.rs` |
| 対応プロトコル | 2026-07-28 / 2025-06-18（既定）/ 2025-03-26 / 2024-11-05 | `server.rs:21-23` |
| 通信層と処理の分離 | `Server::handle(&mut self, msg: Value) -> Option<Value>` で分離済み | `server.rs:362` |
| `Server` の Send | **Sendではない**（`Box<dyn Backend>` にSend境界なし） | `server.rs:90-100`、`backend.rs:11-18` |
| ツール数 | 32個。うち `command_run` / `command_batch` 等で**エンジンの全コマンド（約700）を実行可能** | `crates/mcp/src/tools.rs:113-495` |
| 2つのモード | Headless（プロセス内でSessionを持つ）／Remote（起動中GUIアプリへTCP接続、**認証なし**） | `headless.rs`、`backend.rs:20-67`、`docs/control-protocol.md` |
| ログ | ログレベルとキューがプロセス全体で共有。複数セッションだと他セッションに漏れる | `crates/mcp/src/logging.rs:22-28` |
| 不足点 | `handle()` 経由では、初期化前の呼び出し拒否・サイズ上限・キャンセル・進捗通知が効かない（`serve()` 内にしかない） | `server.rs:269-328, 403-459`、`intake.rs` |
| 名称 | `serverInfo.name` が `"vectorcraft"`、ツール説明もVectorCraft前提 | `server.rs:421` |

- シェル実行・外部プロセス起動・URL取得は**エンジン側に存在しない**（grepで確認済み）。
- プラグインはwasmiによるWebAssemblyサンドボックス（メモリ512MiB、60秒など上限あり、`crates/plugins/src/runtime.rs:37-48`）。ただし登録はプロセス全体で共有。

### 2.4 ファイルアクセス制限（`crates/engine/src/file_access.rs`）

- `AutomationRoots { read, write }`。指定したルートは正規化され、実在するディレクトリである必要がある。
- **ルート未指定なら無制限**（`file_access.rs:16-20`）。
- 検査内容: 絶対パス化 → シンボリックリンク解決 → ルート配下かを**パス要素単位**で比較（`/work2` は `/work` の配下と見なさない）。存在しないフォルダより下の `..` や、行き先のないリンクは拒否。
- **適用範囲はスレッド単位**（`thread_local!`、`file_access.rs:119-122`）か、プロセス全体に一度だけ（`confine_process`、`file_access.rs:125-131`）。
  - `docs/mcp.md:101` は「サーバープロセスの全スレッドで有効」と書いているが、これはCLIが `confine_process` を呼ぶ場合だけ。ライブラリとして使う場合はスレッド単位になる。
- 既知の弱点:
  - **TOCTOU**（検査してから開くまでの間にリンクを差し替えられると突破される）。コード内コメントで認めている（`file_access.rs:22-24`）。
  - 相対パスは**サーバープロセスのカレントディレクトリ基準**で解決される。
  - エラーメッセージにホストの絶対パスが含まれる。
  - 制限を通らない経路がある: アプリ用フォルダ（スウォッチライブラリ、Fontsフォルダ）、データ復旧（`recovery.rs`）、テンプレートフォルダの作成（`save.rs:130-137`）、リンク画像の更新日時取得（`link_watch.rs`）、エンジン外のスレッドでの呼び出し。
- 入力の上限: ファイル読み込みに上限なし、PDFリーダーの `max_input_bytes` が `usize::MAX`、Undo履歴は**文書ごとに最大1GiB**、ヘッドレス時は**呼び出しごとの実行時間制限なし**（協調的キャンセルのみ）。

### 2.5 同時実行・セッション分離

- `Session` は **Send**（`crates/engine/src/tests_large_edits.rs:66-75` でスレッド間移動している）。Syncではない見込み。
- `Document` は Send + Sync（`Arc<Document>` をワーカースレッドに渡している）。
- 実行はすべて同期的なCPU処理。非同期サーバーからはブロッキングスレッドで実行する必要がある。
- キャンセル（`cancel::scope`）とファイル制限（`file_access::confine`）はスレッドローカル。進捗通知の `ProgressSink` は `Rc`（`crates/doc/src/cancel.rs:51`）。**実行スレッド内で生成・適用する必要がある**。
- **プロセス全体で共有される可変状態**（セッション間で漏れる）:

| 状態 | 場所 | 変更するコマンド |
|---|---|---|
| カラーマネジメント設定 | `crates/color/src/cms/mod.rs:890-924` | `edit.colorSettings`, `edit.assignProfile` |
| ICCプロファイル登録 | `cms/mod.rs:167` | `color.loadProfile`, ネイティブ形式を開いたとき |
| フォントDB（単一の `FontDb::global()`） | `crates/text/src/fontdb.rs:1545-1548` | `text.rescanFonts`, 設定のフォントフォルダ |
| ハイフネーション例外 | `crates/text/src/hyphen.rs:34-60` | `prefs.set` |
| 描画スレッド数の既定値 | `crates/render/src/lib.rs:2193-2220` | `prefs.set` のたびにリセット |
| WebAssemblyプラグイン登録 | `crates/plugins/src/registry.rs:14` | `plugin.install` 等 |
| プロセス全体のファイル制限ルート | `file_access.rs:124-131` | `confine_process`（一度だけ） |

- 内容をキーにした共有キャッシュ（画像1GiB、配置文書512MiB等）はデータ混在はしないが、メモリはテナント間で共有される。
- エクスポートのたびに新しい `Renderer` が作られ、各々最大8スレッドを使う。`VECTORCRAFT_RENDER_THREADS` で抑制できる。
- スタックオーバーフローとメモリ不足はプロセスごと落ちる（パニックガードでは防げない）。

### 2.6 フォント

- 同梱フォントは**ラテン文字のみ**（Source Sans 3 / Source Serif 4 / Inter / JetBrains Mono、OFL-1.1、約3.2MiB）。
- **日本語フォントは同梱されない**。ビルド時に `CRAFT_FONTS_DIR` を指定すると BIZ UDPGothic / Shippori Mincho / BIZ UDMincho 等を埋め込める（`docs/development.md:115-152`）。それ以外はサーバーにインストールされたフォント頼み。
- Linuxでは `/usr/share/fonts`、XDG、fontconfig設定を純Rustで走査する（libfontconfig不要）。
- フォントDBはプロセスで1つ。セッションごとのフォント分離はできない。
- `vectorcraft-mcp` に依存すると、Cargoのfeature統合により同梱フォントが強制的に有効になる（`crates/mcp/Cargo.toml:11`、`crates/engine/Cargo.toml:11-15`）。`docs/development.md:164` の記述と矛盾。

### 2.7 ビルド・環境

- 必要: **Rust 1.95以上、edition 2024**（`Cargo.toml:8-10`）。`rust-toolchain` ファイルはない。
- **開発機（このMac）にRustが未導入**（`cargo` / `rustc` / `~/.rustup` なし）。
- 非GUI部分の依存は純Rust。システムライブラリはリンカ（macOSはXcode Command Line Tools）だけで足りる見込み。
- ビルドには crates.io と `github.com/storytold/craft-libs`（git依存）へのネットワーク接続が必要。
- ディスク: ワークスペース全体で約30GB/ターゲット（`AGENTS.md:58`）。ArtServerはエンジンのみなので数GBの見込み（未検証）。
- リポジトリ内にLinux/macOSの一般的なビルド・テストCIはない。品質ゲートはローカルの `cargo xtask ci`。

### 2.8 upstream追従

- VectorCraftのcrateは**crates.ioに公開されていない**（git依存のcraft-pdf、crate外を参照する `include_bytes!` があるため公開不可）。
- バージョンは 0.1.1（2026-10-03）→ 0.12.0（2026-10-16）とほぼ毎日上がっており、1日200〜560コミット。**公開APIの安定性・semverの約束はない**。
- 依存側に**引き継がれないもの**:
  - `[patch.crates-io]`（`Cargo.toml:126-130`、krilla / hayro-* を craft-libs に差し替え）
  - `[profile.*]`（依存crateを opt-level 2 にする設定を含む）
  - workspaceのlint設定、`Cargo.lock`
- `[patch.crates-io]` を写さないと、PDFバックエンドがcrates.io版になり、VectorCraftが入れているパニック対策・デコード制限が失われる。ビルド自体が失敗する可能性もある（未検証）。

### 2.9 ライセンス・ブランド

- VectorCraft本体と craft-libs は **MIT OR Apache-2.0**。
- ArtServerに入る依存の中に **GPL / LGPL / MPL / プロプライエタリはない**（Cargo.lockからの静的解析。`cargo deny` による確定は未実施）。
- 配布時に必要な表示:
  - VectorCraftのMIT表示（`Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors`）
  - craft-libs、Hayro AuthorsのMIT表示（`licenses/pdf-backend/*`）
  - Adobe CMap、Foxit/PDFiumフォント（PDFリーダーに埋め込まれる）のBSD-3表示
  - 同梱フォントのOFL-1.1全文4種（`assets/fonts/OFL-*.txt`）
  - jpeg-encoderのIJG文言、Unicode-3.0（ICU4X）、BSD/Zlib系の表示
- ブランド:
  - **ArtCraftの名称・ロゴはオープンソースではない**（`docs/brand/LICENSE-brand.txt`）。派生物では使用不可。「based on VectorCraft by the ArtCraft team」とテキストで記すことだけは許可。
  - エンジン内に `help.*` コマンドとしてArtCraftのサイト・Discordへのリンクがある（`crates/engine/src/cmd/help.rs:8-27`）。ArtServerでは公開しない。
  - VectorCraftの名前が出力に書き込まれる: PDFの Creator/Producer（`crates/pdf/src/export.rs:274-275`）など。ライセンス上の義務ではなく、ブランド上の検討事項。

---

## 3. 実装案の比較

| 観点 | A-1: `vectorcraft-mcp` を組み込みHTTP層だけ自作 | **A-2: `vectorcraft-engine` を組み込みMCP層を自作（推奨）** | B: CLI(stdio)をHTTPゲートウェイで中継 | C: ソースをコピー |
|---|---|---|---|---|
| 初期の機能量 | ◎ 32ツール・約700コマンドがすぐ使える | ○ 必要なツールから順に追加 | ◎ 同左 | ◎ |
| 安全性の制御 | △ 全コマンド公開が前提。拒否リストを後付けする形になる | ◎ 許可リスト方式（既定で拒否）にできる | △ CLIの既定動作（7979番への自動接続など）に注意が必要 | ○ |
| 複数セッション | △ `Server` 非Send・ログ共有・ライフサイクル検査なし | ○ セッションごとの実行スレッドで管理 | ◎ プロセス分離できる | ○ |
| 時間・メモリ制限 | △ 協調キャンセルのみ | △ 同左（将来プロセス分離で解決） | ◎ プロセスを強制終了できる | △ |
| Streamable HTTP対応 | △ 進捗・キャンセルは `serve()` 内にしかなく、作り直しが要る | ○ 最初から設計できる | △ stdio↔HTTPの変換とプロセス管理が必要 | ○ |
| ブランド分離 | △ `serverInfo`・ツール説明がVectorCraft | ◎ ArtServer独自 | △ | ◎ |
| upstream追従 | ○ rev更新 | ○ rev更新 | ◎ バイナリ差し替え | ✕ 約55万行・毎日更新の手動追従は非現実的 |
| 実装量 | 小 | 中 | 中 | 大 |

**A-2を推奨する理由**

1. VectorCraftの価値の中心はエンジン（L0〜L5）であり、A-2でもそれを丸ごと再利用できる。MCP層はエンジン全体に比べればごく薄い。
2. `vectorcraft-mcp` は信頼された単一ユーザーのstdio利用を前提に作られている。ネットワーク公開に必要な部分（許可リスト、セッション分離、ライフサイクル、サイズ制限、Streamable HTTPの進捗・キャンセル）は、A-1でもほぼ作り直しになる。
3. 計画書の「暫定コードの常態化を避ける」に照らすと、前提の合わない部品にラッパーを重ねるより、最初から責務を分けた方が保守しやすい。

**案Bを採らない理由**: プロセス分離は大きな利点だが、CLIはそもそもサーバー用途向けではなく、セッション管理・エラー伝搬・性能の管理が難しい。プロセス分離はPhase 3で「ArtServer自身のワーカープロセス」として導入する（4.4節）。

**案Cを採らない理由**: 規模と更新頻度から、手動での追従が現実的でない。

---

## 4. 推奨アーキテクチャ（案A-2）

### 4.1 全体構成

```text
MCPクライアント（Claude Code 等）
        │  HTTP(S) + MCP Streamable HTTP   ※TLSはリバースプロキシで終端
        ▼
┌─────────────────────────── artserver（単一バイナリ） ───────────────────────────┐
│ HTTP層           : 127.0.0.1バインド / Origin・Host検査 / 本文サイズ上限 / 同時接続上限   │
│ 認証層           : Bearerトークン（定数時間比較）                                    │
│ MCP層            : JSON-RPC / initialize・ライフサイクル / tools・resources            │
│                    ツール定義と入力スキーマはArtServer独自                             │
│ ポリシー層       : 許可するエンジンコマンドの許可リスト（既定で拒否）                    │
│                    command.batch 等の入れ子も検査 / 引数検査（check_params）           │
│ セッション管理   : MCPセッションID ↔ セッション実行器 / 上限数 / アイドル破棄            │
│ セッション実行器 : セッションごとの専用スレッドが Session を所有                         │
│                    呼び出しごとに file_access::confine + cancel::scope を適用          │
│                    （将来、同じインターフェースのままワーカープロセスに差し替え可能）      │
└───────────────────────────────────────────┬─────────────────────────────────┘
                                            ▼
                          vectorcraft-engine（git依存・rev固定）
                          doc / render / svg / pdf / text / format ...
```

### 4.2 責務の分け方

| モジュール | 責務 |
|---|---|
| `config` | 設定（バインド先、トークン、データディレクトリ、各種上限）。CLI引数・環境変数から読む |
| `http` | HTTPサーバー、`/mcp`、`/healthz`。Origin/Host検査、サイズ上限 |
| `auth` | トークン検証 |
| `mcp` | MCPプロトコル処理（initialize、tools/list、tools/call、エラー変換） |
| `tools` | ArtServerのツール定義。ツール → エンジンコマンド列への変換 |
| `policy` | 許可するコマンドIDと引数の検査。ファイルパス引数の扱い |
| `session` | セッション表、上限、アイドル破棄、実行器への受け渡し |
| `engine_host` | 専用スレッドでの `Session` 所有、confine/cancel の適用、パニック・タイムアウトの扱い |

### 4.3 重要な設計判断

- **ファイルI/Oを既定で使わない**: MVPでは入力も出力もBase64で受け渡す（`document.export` の path 省略、`document.open` の `dataBase64`）。パス指定の機能を開くときは、セッション専用ディレクトリを read/write ルートとして `confine` し、パスは必ずArtServer側で絶対パスに組み立てる。`confine_process` は使わない。
- **許可リスト方式**: `run_command` のような汎用実行ツールはMVPでは公開しない。公開する場合も許可リストで制限する。プロセス全体の状態を変えるコマンド（2.5節の表）、`plugin.*`、`prefs.set`、`text.addFontFiles`、`links.findFiles`、`file.list`、`script.check`、`file.recovery.*`、`help.*` は許可しない。
- **セッションごとの資源上限**: 文書数、Undo履歴のバイト上限（`History::byte_budget`）、1呼び出しの実行時間（協調キャンセル）、描画スレッド数（`VECTORCRAFT_RENDER_THREADS`）。
- **起動時の準備**: フォントDBを起動時に走査しておく（初回検索で全呼び出しが待たされるのを防ぐ）。ログはArtServer独自のものを使い、`vectorcraft-mcp` の共有ログキューは使わない。
- **ログ**: トークン・文書内容・ホストの絶対パスを出さない。

### 4.4 プロセス内実行の限界と将来の分離

プロセス内で実行する限り、次の3つは完全には防げない。

1. 暴走したコマンドの強制停止（協調キャンセルに応じない処理）
2. スタックオーバーフロー・メモリ不足によるプロセス全体の停止
3. 共有キャッシュによるメモリのテナント間共有

MVPでは「信頼できる利用者・ローカル利用」を前提とし、これらを**既知の制限として明記**する。Phase 3で複数ユーザーに公開する前に、セッション実行器を「ArtServer自身のワーカープロセス」に置き換える（インターフェースは変えない）。

### 4.5 依存関係の管理

- `vectorcraft-engine` を **git依存・commit rev固定**で参照する（ブランチ追従はしない）。
- VectorCraftの `[patch.crates-io]`（krilla / hayro-syntax / hayro-interpret / hayro-jbig2、rev `ca50d63…`）をArtServerのルート `Cargo.toml` に写す。
- 依存crateを opt-level 2 にするprofile設定も写す。
- `Cargo.lock` はVectorCraftのものを起点にする。
- VectorCraftのrevを上げるときは、patch・lockfileを同期し、テストを通してから行う。PDFバックエンドがpatch版になっているかをCIで検査する（VectorCraftの `xtask/src/pdf_deps.rs` と同等）。
- 手元でVectorCraftを改変して試す必要が出た場合は、コミットしないローカル設定でパスに差し替える。

---

## 5. MVP（Phase 1）の範囲

| 区分 | 内容 |
|---|---|
| 起動 | `artserver` 単一バイナリ。既定 `127.0.0.1`。トークン未設定なら起動しない |
| エンドポイント | `POST /mcp`（Streamable HTTP）、`GET /healthz` |
| ツール（最小） | 文書作成、矩形などの図形追加、塗り・線の設定、移動・変形、文書の状態取得、SVG/PDF/PNGの書き出し（Base64）、Undo/Redo |
| セキュリティ | Bearerトークン、Origin/Host検査、本文サイズ上限、セッション数上限、アイドル破棄、呼び出しタイムアウト（協調キャンセル）、許可リスト |
| テスト | 矩形作成 → SVG書き出しのE2E。異常系（不正入力、認証失敗、許可外コマンド、サイズ超過、タイムアウト、許可外パス） |
| 文書 | README（起動、Claude Codeからの接続、curlでのヘルスチェック）、LICENSE（MIT）、THIRD_PARTY_NOTICES、`docs/progress.md` |

MVPで**実装しないもの**（明記する）: TLS（リバースプロキシで終端）、複数ユーザーと認可、プロセス分離、レート制限、ファイルパス指定での入出力、日本語フォントの同梱。

---

## 6. 未解決事項・要判断事項

| # | 内容 | 対応 |
|---|---|---|
| 1 | 開発機にRustが未導入（1.95以上が必要） | rustupでの導入可否をユーザーに確認 |
| 2 | MCPプロトコル層の実装手段（公式Rust SDK `rmcp` 等を使うか、自前で実装するか） | 実装着手時に、各候補の最新版とStreamable HTTP対応状況を確認して決める |
| 3 | `[patch.crates-io]` なしでビルドが通るか | patchを写す前提なので実害なし。ビルド時に確認 |
| 4 | 第三者ライセンスの確定 | ArtServer側で `cargo deny` / `cargo about` を実行して確定させる |
| 5 | 日本語フォントの方針（サーバーへのインストール／`CRAFT_FONTS_DIR` での埋め込み／実行時追加） | Phase 2で決定。印刷用途なら優先度高 |
| 6 | 出力ファイルに書かれる "VectorCraft"（PDF Producer等）の扱い | 由来の表示として許容するか、エンジン外から上書きできるか検討 |
| 7 | 「ArtServer」という名称の商標衝突確認 | 公開前に確認（計画書§7） |
| 8 | upstreamの更新頻度が非常に高く、APIの安定性の約束がない | rev固定とし、更新は計画的に行う |
| 9 | `Renderer` / ArtServer側のセッション型がSendか | ビルド時にコンパイラで確認 |

---

## 7. VectorCraft側ドキュメントとコードの不一致（参考）

upstreamへの報告候補。

1. `docs/mcp.md:101`「サーバープロセスの全スレッドで制限が有効」→ ライブラリ利用時はスレッド単位のみ。
2. `docs/mcp.md:88-91`「ルート外には書き込まない」→ テンプレートフォルダの作成、データ復旧の読み込み等に例外あり。
3. `docs/development.md:164`「ライブラリcrateが同梱フォントを強制しない」→ `vectorcraft-mcp` 経由で強制される。
4. `AGENTS.md:20` の `cargo xtask cleanroom` がCIに存在しない。
5. `vendor/windows-link/VECTORCRAFT-PATCH.md:17`「workspaceに[patch]はない」→ 現在は存在する。
