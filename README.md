# ArtServer

AIエージェントがネットワーク経由の [MCP](https://modelcontextprotocol.io/)（Streamable HTTP）で操作できる、GUIなしのベクターグラフィックスサーバーです。描画・文書処理のエンジンには [VectorCraft](https://github.com/storytold/vectorcraft) を使っています。

> **現在の状態: MVP（Phase 1）**
> 信頼できる少数のクライアントがローカルで使うことを前提にしています。インターネットへの公開はしないでください（[既知の制限](#既知の制限)）。

## できること

- 作業セッションを作り、その中で文書を作成する
- 矩形・楕円・多角形・星形・直線を描き、塗りと線を設定し、移動する
- SVGを読み込む
- SVG（テキスト）、PDF（Base64）、PNG（画像）で書き出す
- 元に戻す／やり直す

## 必要なもの

- Rust 1.95 以上
- macOS（Apple Silicon で確認）または Linux
- 初回ビルド時に crates.io と GitHub（storytold/vectorcraft、storytold/craft-libs）へ接続できること

## ビルド

```sh
cargo build --release
```

バイナリは `target/release/artserver` にできます。

## 起動

アクセストークン（32文字以上）を決めて、環境変数かファイルで渡します。

```sh
export ARTSERVER_TOKEN=$(openssl rand -hex 32)
./target/release/artserver
# → 127.0.0.1:8471 で待ち受け、MCP エンドポイントは /mcp
```

ファイルで渡す場合は `--token-file <path>` を使います。トークンが設定されていなければ起動しません。

### ヘルスチェック

```sh
curl http://127.0.0.1:8471/healthz
# {"status":"ok"}
```

### MCP の動作確認（curl）

```sh
U=http://127.0.0.1:8471/mcp
H=(-H "Authorization: Bearer $ARTSERVER_TOKEN" -H "Content-Type: application/json" \
   -H "Accept: application/json, text/event-stream" -H "MCP-Protocol-Version: 2025-06-18")

curl -s "${H[@]}" -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' $U
curl -s "${H[@]}" -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"session_create","arguments":{}}}' $U
```

## Claude Code から接続する

```sh
claude mcp add --transport http artserver http://127.0.0.1:8471/mcp \
  --header "Authorization: Bearer $ARTSERVER_TOKEN"
```

この方法では、トークンが Claude Code の設定ファイルにそのまま保存されます。避けたい場合は、プロジェクトの `.mcp.json` に書き、トークンは環境変数から読ませます。

```json
{
  "mcpServers": {
    "artserver": {
      "type": "http",
      "url": "http://127.0.0.1:8471/mcp",
      "headers": { "Authorization": "Bearer ${ARTSERVER_TOKEN}" }
    }
  }
}
```

接続したら、例えば「ArtServer で 300×200 の文書に青い角丸の矩形を描いて SVG で見せて」と頼みます。

## ツール

| ツール | 内容 |
|---|---|
| `session_create` / `session_close` | 作業セッションの作成・破棄。他のツールは返されたハンドルを `session` 引数で受け取る |
| `document_new` | 新しい文書（アートボード1つ）を作る |
| `import_svg` | SVG のマークアップを新しい文書として開く（リンクされたファイルは読まない） |
| `draw_rectangle` / `draw_ellipse` / `draw_polygon` / `draw_star` / `draw_line` | 図形を描く。塗り・線も同時に指定できる。新しいオブジェクトの id を返す |
| `set_paint` | オブジェクトの塗り・線の色・線幅を設定する |
| `move_objects` | オブジェクトを移動する |
| `inspect_document` | 文書のレイヤー構造、オブジェクトの id・種類・範囲・塗りなどを返す |
| `export_document` | SVG / PDF / PNG で書き出す |
| `undo` / `redo` | 元に戻す／やり直す |

座標の単位はポイント（1/72インチ）で、原点はアートボードの左上、y は下向きです。

## 設定

| 引数 | 環境変数 | 既定値 | 内容 |
|---|---|---|---|
| `--bind` | `ARTSERVER_BIND` | `127.0.0.1:8471` | 待ち受けアドレス |
| `--allow-public-bind` | | 無効 | ループバック以外での待ち受けを許可する（TLSを終端するリバースプロキシの背後でだけ使う） |
| `--token-file` | `ARTSERVER_TOKEN_FILE` | | トークンのファイル。未指定なら `ARTSERVER_TOKEN` |
| `--allowed-hosts` | `ARTSERVER_ALLOWED_HOSTS` | `localhost,127.0.0.1,::1` | 受け付ける Host ヘッダ |
| `--allowed-origins` | `ARTSERVER_ALLOWED_ORIGINS` | なし | 受け付ける Origin。空なら Origin 付きの要求（ブラウザから）をすべて拒否 |
| `--max-sessions` | `ARTSERVER_MAX_SESSIONS` | 8 | 同時に持てる作業セッション数 |
| `--session-idle-secs` | `ARTSERVER_SESSION_IDLE_SECS` | 1800 | 使われないセッションを破棄するまでの秒数 |
| `--call-timeout-secs` | `ARTSERVER_CALL_TIMEOUT_SECS` | 60 | 1回の呼び出しの制限時間 |
| `--max-body-bytes` | `ARTSERVER_MAX_BODY_BYTES` | 16777216 | 要求本文の上限 |
| `--max-documents` | `ARTSERVER_MAX_DOCUMENTS` | 8 | 1セッションで開ける文書数 |
| `--history-mib` | `ARTSERVER_HISTORY_MIB` | 64 | 文書1つあたりの Undo 履歴の上限 |
| `--render-threads` | `ARTSERVER_RENDER_THREADS` | 2 | 書き出し1回あたりの描画スレッド数 |

ログは標準エラー出力に出ます。詳しさは `RUST_LOG`（例: `RUST_LOG=debug`）で変えられます。

## セキュリティ

- 既定では 127.0.0.1 でだけ待ち受けます。
- すべての MCP 要求に Bearer トークンが必要です。
- Host / Origin ヘッダを検査します（DNSリバインディング対策）。
- エンジンのコマンドは許可リスト方式で、プロセス全体の設定を変えるコマンドやファイルを扱うコマンドは実行できません。
- ファイルパスは受け付けません。入力も出力もツールの引数と結果で受け渡します。エンジンのファイルアクセスは、空の一時ディレクトリの中に閉じ込めています。
- セッション数・待ち行列・本文サイズ・文書数・履歴・実行時間に上限があります。

## 既知の制限

- **TLS は実装していません。** 外部に出す場合は、TLS を終端するリバースプロキシの背後に置いてください。
- **利用者は1人（トークン1つ）だけです。** 複数ユーザーと権限管理は未実装です。
- **1つのプロセスの中で全セッションを動かしています。** そのため次のことは防げません（[設計方針 6.5節](docs/design-policy.md)）。
  - キャンセルに応じない処理の強制停止。制限時間を超えるとキャンセルを求めますが、処理が止まるまでそのセッションは混雑中になります。
  - スタックオーバーフロー・メモリ不足によるサーバー全体の停止
  - 共有キャッシュによるメモリのセッション間共有
- レート制限は未実装です。
- 日本語フォントは同梱していません。日本語を扱うには、サーバーに日本語フォントが必要です（Linux）。
- 出力ファイル（PDF の Producer など）には "VectorCraft" と記録されます。

## テスト

```sh
cargo test
```

単体テストと、実際に待ち受けたサーバーに MCP クライアントで接続するエンドツーエンドのテストが走ります。

## ドキュメント

- [設計方針](docs/design-policy.md)
- [Phase 0 の調査結果](docs/architecture.md)
- [進捗](docs/progress.md)

## ライセンス

ArtServer 自身のコードは MIT ライセンスです（[LICENSE](LICENSE)）。依存しているコンポーネントのライセンスは [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) を参照してください。

## 謝辞

ArtServer は **VectorCraft by the ArtCraft team** をベースにしています（MIT OR Apache-2.0）。ArtServer は VectorCraft の公式の派生版ではなく、ArtCraft team による承認や保証を受けたものでもありません。
