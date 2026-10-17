# ArtServer 開発方針・Claude Code CLI 引き継ぎ書

- 作成日: 2026-10-17
- プロジェクト名: **ArtServer**
- 作業ディレクトリ（開発者のMac）: `~/MyTasks/RoundShape/ArtServer`
- 実装言語: **Rust**
- 公開予定: **GitHub / オープンソース（ArtServer自作部分はMITライセンス）**
- 参照元: **VectorCraft** — https://github.com/storytold/vectorcraft.git
- 本書の役割: Claude Code CLIが方針を理解し、調査・設計・実装・検証を進めるための引き継ぎ書

## 1. プロジェクトの目的

**ArtServerは、AIエージェントや外部システムからネットワーク経由で操作できる、本格的なヘッドレス・ベクターグラフィックスサーバーを目指す。**

単なるMCP接続プロキシを最終製品としない。VectorCraftの描画・ドキュメント機能を可能な限り再利用し、将来的にはIllustratorのサーバー版に相当する機能領域を目指す。ただし、Adobeのコード・商標・素材を流用するという意味ではない。

基本原則:
1. **既存資産を活かす**：ベクター描画エンジンを最初から作り直さない。
2. **GUI不要**：サーバー上でヘッドレス動作させる。
3. **AIネイティブ**：MCPを最優先。一般アプリ向けREST APIも後に提供。
4. **高い機能性**：描画・編集・ファイル互換性を段階的に充実させる。
5. **自立したプロジェクト**：VectorCraftとは別の製品名・GitHubリポジトリとする。
6. **セキュリティ優先**：ネットワーク公開を想定し、ファイル操作とセッションを隔離する。
7. **暫定コードの常態化を避ける**：最初に拡張可能な構造を検討し、場当たり的なラッパーを蓄積しない。

## 2. スコープ

### 最初の到達点（MVP）

- macOSとLinuxで**GUIなし**に起動する単独のArtServer。
- ネットワーク経由の **MCP（Streamable HTTPを第一候補）** でAIから操作可能。
- 既存VectorCraft機能を通じて、ドキュメント作成・図形追加・変更・書き出しを実行。
- SVG入出力、およびPDF出力を実際に検証（利用可能な機能・制限を明記）。
- 認証とアクセス制限、作業ディレクトリ制限、エラー処理、テストを備える。
- READMEの手順だけでローカル環境に再現できる。

### 将来拡張

- パス、塗り、線、グループ、レイヤー、アートボード、テキスト、画像、マスク、変形などの編集。
- PDF / PDF互換AI / SVG等の互換性強化。**Adobeネイティブ`.ai`の完全互換は保証しない**。
- REST APIとSDK、外部アプリ連携。
- 複数ユーザー、ユーザー/セッションごとの文書分離、認証・認可。
- ジョブキュー、非同期バッチ、大量レンダリング、監視、コンテナ配布。
- データベースやCSVと連携する図版・印刷データ・自動組版への展開。

## 3. 既存VectorCraftを先に調査する

公式リポジトリ（2026-10-17時点のREADME）では、Rust Workspaceを採用し、おおむね以下の構成が説明されている。

- `crates/`：`engine`, `render`, `doc`, `svg`, `pdf`, `mcp` など
- `apps/vectorcraft-cli`：CLI、ヘッドレス処理、MCPの入口
- 公式例：`cargo run --release -p vectorcraft-cli -- mcp` （**stdio MCP**）
- 公式例：`cargo run --release -p vectorcraft-cli -- run --in examples/ribbons.vectorcraft --export out.pdf`
- ライセンス：ルートに `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE` がある。

ただし、**この情報は調査の出発点に過ぎない。** 実装開始時点のソースコード・README・`docs/mcp.md`・`docs/control-protocol.md`・`AGENTS.md`・`Cargo.toml`を実際に確認し、現状を優先する。

調査項目:
1. コア描画・ドキュメント・ファイルI/Oを**UI非依存のRust crate**として利用できるか。
2. 既存MCPはstdioのみか、HTTPトランスポートがすでに実装されていないか。
3. MCPツールの一覧・ドキュメント状態管理・エクスポート機能・ファイルアクセス制御。
4. `--automation-read-root` / `--automation-write-root` 等の境界設定の実装と有効性。
5. ライセンス・素材・フォント・PDFバックエンド等の第三者依存関係。
6. macOS Apple SiliconとLinux上でのビルド・テスト・ヘッドレス稼働条件。
7. upstreamの更新を今後取り込むための適切な依存関係管理方法。

参考:
- https://github.com/storytold/vectorcraft
- https://github.com/storytold/vectorcraft/blob/main/docs/mcp.md
- https://github.com/storytold/vectorcraft/blob/main/docs/control-protocol.md
- https://github.com/storytold/vectorcraft/blob/main/AGENTS.md
- https://github.com/storytold/vectorcraft/blob/main/LICENSE-MIT
- https://github.com/storytold/vectorcraft/blob/main/NOTICE

## 4. 技術方針（調査後に確定）

理想的な構成:

```text
Claude Code / Codex / その他MCPクライアント
                 │
        HTTPS + MCP (Streamable HTTP)
                 ▼
         ArtServer (Rust)
         ├─ 認証・認可
         ├─ MCPトランスポート / API
         ├─ セッション・ドキュメント管理
         ├─ ファイルアクセス制御
         └─ VectorCraft由来のエンジン
                    ├─ ドキュメント / コマンド
                    ├─ 描画 / パス処理
                    └─ SVG / PDF / AI互換
```

実装案を比較し、理由を記録する。

- **案A：必要なVectorCraft crateをRustライブラリとして参照**。原則こちらを優先的に検討。エンジンとネットワーク層を分離しやすい。
- **案B：既存CLIのstdio MCPを安全なHTTPゲートウェイで公開**。早期MVPに向く可能性。ただしプロセス・セッション・エラー・性能の管理が課題。
- **案C：必要なソースのみArtServer側へコピー・適合**。独立性は高い一方、upstream追従・重複保守の負担が増える。必要性が示された場合だけ選択。

**アーキテクチャの決定前に大規模な移植を始めないこと。** HTTP接続できることだけを優先して危険な遠隔コマンド実行サーバーにしないこと。

## 5. 段階的な開発計画

### Phase 0 — ソース調査・方式決定

- 参照元の現行ソースと既存CLIを調べ、MCPコマンド・ライセンスを検証。
- 可能ならコマンド実行、SVG/PDF出力、テストを再現。
- 案A/B/Cの比較表と採用案、根拠、未解決点を `docs/architecture.md` に記録。
- 実装開始時に使用したVectorCraftの**commit SHA**を記録。
- 既存のユーザー作業ディレクトリを勝手に移動・削除・上書きしない。

### Phase 1 — 最小ArtServer（初回実装）

- 独立したRustサーバーバイナリ `artserver` を作成。
- `/mcp` 等のエンドポイントで、標準仕様に沿ったMCP接続を実現。
- 最小限のドキュメント作成・図形操作・SVG/PDF出力をエンドツーエンド検証。
- 初期バインド先は **127.0.0.1**。パブリックな待受は既定で無効。
- アクセストークン等の認証を設計・実装。通信時のTLSは信頼できるリバースプロキシ方式等を検討。
- パス制限・セッション分離・入力サイズ制限・タイムアウト・エラー応答を実装。
- 起動方法、Claude Code等からの接続方法、curl等によるヘルスチェック、実例をREADME化。

### Phase 2 — ドキュメント機能・互換性

- 既存エンジン機能を順にArtServerから使えるようにする。
- ドキュメント状態・保存形式・ファイル互換性を実ファイルで検証。
- Unicode・日本語フォント、画像・レイヤー・透明・パス処理等の検証。
- 自動テストと性能測定を継続。

### Phase 3 — サーバー製品化

- REST API、複数ユーザー、認可、セッションとジョブの永続化。
- 非同期バッチ処理、監視、ログ、Docker、Linux本番配備。
- 外部公開前の脅威分析・負荷試験・ライセンス/商標確認。

## 6. セキュリティ上の必須事項

- MCPツールから**任意のシェルコマンドを実行できる設計にしない**。
- サーバー実行ユーザーは最小権限。入力ファイルは信用しない。
- 読み書き可能なルートを制限し、`..`、シンボリックリンク、絶対パス等による逸脱を検査。
- 文書・一時ファイル・プロセスのセッション間分離。ユーザー間のデータ混在を防ぐ。
- リクエスト容量、メモリ、CPU時間、ページ/オブジェクト数等に制限を設ける。
- ログに秘密情報、アクセストークン、ユーザー文書の中身を無条件に出さない。
- リモート公開時はTLS、認証・認可、Origin/Host対策、レート制限を検討し、無認証の全世界公開を禁止。
- 本体・依存ライブラリに既知の脆弱性がないか検査する。

## 7. ライセンス・ブランドの扱い

- VectorCraftのルートは**MIT OR Apache-2.0**。流用する対象コードと依存物について、適用条件をファイル単位で確認する。
- MITを選ぶ場合、元の著作権表示とMITライセンス文を必要な配布物に残す。Apache-2.0を選択する部分や第三者コンポーネントの条件も守る。
- ArtServerの独自コードはMITで公開する方針。ただし**混在するコンポーネントのライセンスまで一律MITに書き換えない**。
- `NOTICE`、ライセンス情報、素材・フォント・PDFバックエンドの条件を確認して取り込む。
- VectorCraft/ArtCraftの名称・ロゴ・画像等は、コードライセンスで利用権が保証されるものではない。**ArtServerは独自名称・独自ロゴ**で公開。
- 「ArtServer」の既存製品名・商標の衝突は公開前に確認。
- 元プロジェクトの名称・帰属についてREADMEで適切に謝辞を示す。公式派生版と誤認させない。

## 8. Claude Code CLIに任せる具体的な初回タスク

**以下を最初の作業指示とする。**

1. `~/MyTasks/RoundShape/ArtServer` の状態を調べ、既存ファイルを保護する。
2. 本書を読み、目標・制約を把握する。
3. VectorCraftの現行ソースの存在場所を探す。既にcloneされていればそれを参照。存在しない場合は、混同しないよう `upstream/` 等の下にcloneする案を検討する（むやみに二重cloneしない）。
4. 公式`README.md`、`AGENTS.md`、`docs/mcp.md`、`Cargo.toml`、コアcrate、ライセンスを調査する。
5. 実際のMCP/CLI起動方法と、既存HTTP対応の有無を検証する。
6. ビルド・テストが可能なら実行し、環境・コマンド・結果を記録する。
7. **採用するアーキテクチャと最小実装範囲を決定**し `docs/architecture.md` に記載する。
8. 最小MVPを実装し、可能なら「AIから矩形を作成 → SVG出力」の実行確認まで行う。
9. `README.md`、`LICENSE`、`THIRD_PARTY_NOTICES.md`（必要に応じて）、テスト、`docs/progress.md` を整備する。
10. 実装済み・未実装・既知の制限・確認方法・次の作業を最後に報告する。

**途中の判断基準:** 既存エンジン再利用の妥当性やライセンスに重大な不明点が残る場合は、推測でコード流用・公開まで進めず、調査結果と選択肢を報告する。

## 9. MVPの完了条件

- ArtServerがGUIなしで起動し、ローカルHTTPでMCP接続できる。
- 対応するMCPクライアントから実際に図形生成とSVG書き出しが成功する。
- 既存VectorCraftとの差分・ライセンス情報・採用依存関係が文書化される。
- 基本的な異常系（不正入力、認証失敗、許可外パス、タイムアウト）がテストされる。
- 再現可能なビルド・実行手順がある。
- 本番向けに未実装の安全対策があれば、明確に未実装と表示する。

---

**Claude Code CLIへの開始指示例**

> このディレクトリの `DEVELOPMENT_PLAN.md` を読み、ArtServerの開発を始めてください。まずPhase 0の調査を実施し、その結果に基づいて採用アーキテクチャを決定してください。その後、Phase 1のMVPに着手してください。既存ファイルを破壊せず、流用ライセンスとMCP接続時の安全性を重視し、作業内容とテスト結果を記録してください。
