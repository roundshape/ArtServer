# ArtServer

[English](README.md) | 日本語

AIエージェントがネットワーク経由の [MCP](https://modelcontextprotocol.io/)（Streamable HTTP）で操作できる、GUIなしのベクターグラフィックスサーバーです。描画・文書処理のエンジンには [VectorCraft](https://github.com/storytold/vectorcraft) を使っています。

> **現在の状態: Phase 2（機能の拡充と互換性の確認）に取り組み中。MVP（Phase 1）は完了**
> 信頼できる少数のクライアントがローカルで使うことを前提にしています。インターネットへの公開はしないでください（[既知の制限](#既知の制限)）。

## できること

- 作業セッションを作り、その中で文書を作成する
- 図形（矩形・楕円・多角形・星形・直線）とパスを描き、塗り・線・効果を設定する
- 文字を置く（日本語、縦書き、エリア内文字、パスに沿った文字）。使えるフォントの一覧を返す
- 移動・回転・拡大縮小・反転・傾き、図形の合成（パスファインダー）、グラフ、文字の回り込み
- 許可したエンジンのコマンドを、探して実行する（`command_list` / `command_run` / `command_batch`）
- SVGを読み込む。アップロード用の URL で送ったファイル（PDF、Illustrator の .ai、SVG、PNG、JPEG、VectorCraft の .vectorcraft）を開く・配置する
- SVG・PDF・PNG・Illustrator の .ai（PDF 互換）・VectorCraft の .vectorcraft で書き出す（小さい SVG・PNG は返事に入れ、ほかの形式と大きいものはダウンロード用の URL で渡す）。プレビュー画像を返す
- 元に戻す／やり直す

## 必要なもの

- Rust 1.95 以上
- macOS（Apple Silicon で開発・動作確認をしている）
- Linux・Windows は、ビルドが通ることだけを GitHub Actions で確認している（Linux は push のたびに `cargo check` でコンパイルを確認し、完全なビルドはどちらも手動で実行したときに行う）。動作は確認していない
- 初回ビルド時に crates.io と GitHub（storytold/vectorcraft、storytold/craft-libs）へ接続できること

## ビルド

```sh
cargo build            # 開発用。バイナリは target/debug/artserver
cargo build --release  # 配布・常用向け。バイナリは target/release/artserver
```

開発用のビルドでも依存ライブラリは最適化されるので、手元で試すには十分な速さで動きます。配布用のビルドはさらに最適化するため、ビルドに時間がかかります。以下の例では `target/debug/artserver` を使います。

## 起動

### 1. トークンを作る（最初の1回だけ）

トークンは、ArtServer を使うための合言葉です。自分で作るランダムな文字列で、どこかから発行されるものではありません。ArtServer と Claude Code に同じ値を持たせ、一致しない要求は断ります。32文字以上の、見える ASCII 文字だけで作ってください。

ファイルに保存しておくと、ArtServer を起動し直しても同じトークンを使えます。

```sh
mkdir -p ~/.config/artserver
chmod 700 ~/.config/artserver
openssl rand -hex 32 > ~/.config/artserver/token
chmod 600 ~/.config/artserver/token
```

### 2. ArtServer を起動する

```sh
./target/debug/artserver --token-file ~/.config/artserver/token
# artserver listening; MCP endpoint is /mcp
```

127.0.0.1:8471 で待ち受けます。起動したターミナルはそのまま開いておき、止めるときは Ctrl+C を押します。トークンが設定されていなければ起動しません。

トークンは環境変数 `ARTSERVER_TOKEN` でも渡せます。ただし `ARTSERVER_TOKEN=$(openssl rand -hex 32)` のように起動のたびに作ると、トークンが変わるので、Claude Code への登録もやり直すことになります。

**作り直したら起動し直す**: ビルドしてバイナリを作り直しても、動いている ArtServer は古いバイナリのままです。新しいバイナリを使うには、Ctrl+C で止めて起動し直してください。作業セッションと文書はメモリにしか持っていないので、起動し直すと消えます。

### ヘルスチェック

トークンなしで確認できます。

```sh
curl http://127.0.0.1:8471/healthz
# {"status":"ok"}
```

### MCP の動作確認（curl）

```sh
U=http://127.0.0.1:8471/mcp
T=$(cat ~/.config/artserver/token)
H=(-H "Authorization: Bearer $T" -H "Content-Type: application/json" \
   -H "Accept: application/json, text/event-stream" -H "MCP-Protocol-Version: 2025-06-18")

curl -s "${H[@]}" -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' $U
curl -s "${H[@]}" -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"session_create","arguments":{}}}' $U
```

## Claude Code から接続する

ArtServer を起動したまま、別のターミナルで登録します。

### 1. 登録する

**A. `claude mcp add` で登録する（手軽）**

ArtServer を使いたいフォルダに移動してから登録します。

```sh
cd <ArtServer を使いたいフォルダ>
claude mcp add --transport http artserver http://127.0.0.1:8471/mcp \
  --header "Authorization: Bearer $(cat ~/.config/artserver/token)"
claude mcp list   # artserver が Connected と出れば接続できている
```

- 既定では、**登録したフォルダで起動した Claude Code にだけ**現れます（local スコープ）。別のフォルダで起動すると表示されません。
- どのフォルダからでも使いたい場合は `--scope user` を付けます。その場合、ArtServer が止まっている間は、Claude Code を起動するたびに接続失敗と表示されます。
- トークンは Claude Code の設定ファイル（`~/.claude.json`）にそのまま保存されます。
- 登録を消すには `claude mcp remove artserver` を実行します。

**B. `.mcp.json` で登録する（トークンをファイルに残さない）**

使いたいフォルダに `.mcp.json` を作ります。トークンの代わりに `${ARTSERVER_TOKEN}` と書くと、Claude Code が起動するときに環境変数の値で置き換えます。

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

Claude Code は、トークンを環境変数に入れて起動します。初回の起動時に、このプロジェクトの MCP サーバーを使うかどうか聞かれるので許可してください。

```sh
ARTSERVER_TOKEN=$(cat ~/.config/artserver/token) claude
```

### 2. Claude Code を起動し直す

MCP サーバーの登録は、Claude Code の起動時に読み込まれます。登録する前から開いていた Claude Code では使えないので、一度終了して起動し直してください。`claude -c` で起動すると、直前の会話の続きから始まります。

起動したら `/mcp` を入力し、`artserver` がつながっているか確かめます。

### 3. 使ってみる

例えば「ArtServer で 300×200 の文書に青い角丸の矩形を描いて SVG で見せて」と頼みます。

### つながらないとき

| 症状 | 考えられる原因 |
|---|---|
| `/mcp` に `artserver` が出ない | 登録したのと別のフォルダで Claude Code を起動した。または登録する前に起動した Claude Code を使っている |
| 接続失敗と出る | ArtServer が止まっている |
| 401（unauthorized） | 登録したトークンと ArtServer のトークンが一致しない（トークンを作り直したなど）。登録し直す |
| B の方法でつながらない | `ARTSERVER_TOKEN` を設定せずに `claude` を起動した |

## ツール

ArtServer のツールと、VectorCraft の MCP のツールを公開しています。VectorCraft のツールは、ArtServer の許可リストを通してエンジンを呼びます（[設計方針 4.7節](docs/design-policy.md)）。どのツールも、`session_create` が返したハンドルを `session` 引数で受け取ります。

**ArtServer のツール**

| ツール | 内容 |
|---|---|
| `session_create` / `session_close` | 作業セッションの作成・破棄 |
| `document_new` | 新しい文書（アートボード1つ）を作る |
| `import_svg` | SVG のマークアップを新しい文書として開く（リンクされたファイルは読まない） |
| `list_fonts` | 使えるフォントのファミリーの一覧（`query` で絞り込み）。`family` を指定すると、そのスタイルの一覧。`bundled` は、ArtServer に組み込まれていて（Source Sans 3、Source Serif 4、Inter、JetBrains Mono）サーバーの OS には入っていないフォントを示す。利用者のほかのアプリにも、たいてい入っていない |
| `export_document` | SVG・PNG・PDF・Illustrator の `.ai`・VectorCraft の `.vectorcraft` で書き出す。小さい SVG（32KB まで）はテキスト、小さい PNG（256KB まで）は画像で返す。PDF・`.ai`・`.vectorcraft` とそれより大きいものは、取りに来るための URL を返す（[大きい書き出し結果の受け取り](#大きい書き出し結果の受け取り)）。`delivery` で返し方を指定できる（`inline` ならバイナリの形式も Base64 で返す）。`.ai` は PDF 互換で保存する。Illustrator では PDF のアートワーク（ただのパス。Illustrator のライブシェイプやレイヤーにはならない）として開き、ArtServer と VectorCraft ではレイヤーも含めて元どおり編集できる形で開く |
| `upload_create` | ファイルを送るための URL を作る（PDF、PDF 互換で保存した Illustrator の `.ai`、SVG、PNG、JPEG、VectorCraft の `.vectorcraft`）。HTTP の PUT で送るか、ブラウザで URL を開いて送る（[ArtServer へのファイルの送り方](#artserver-へのファイルの送り方)） |
| `open_upload` | 送ったファイルを新しい文書として開く。ArtServer が書き出した PDF は、元の文書のとおりに開く |
| `place_upload` | 送ったファイルを今の文書に配置する（埋め込み）。位置の指定か、四角形に収める指定ができる |

**VectorCraft のツール**

| ツール | 内容 |
|---|---|
| `draw_shape` | 矩形・楕円・多角形・星形・直線を描く。塗り・線・破線・不透明度・名前・効果も指定できる |
| `draw_path` | 点の並び（ベジェのハンドル付きも可）か SVG のパスデータからパスを描く |
| `set_paint` | 塗り・線・線幅を設定する |
| `transform` | 移動・回転・拡大縮小・反転・傾き（組み合わせ可、コピーも可） |
| `add_text` | 文字を置く。フォント・サイズ・色・揃え・縦書き、枠への流し込み、パスに沿った文字 |
| `pathfinder` | 図形を合成する（合体、前面で型抜き、交差、中マド、分割など） |
| `apply_effect` | 効果をかける。種類を省くと効果の一覧を返す |
| `create_graph` | CSV などのデータからグラフを作る |
| `text_wrap` | 文字の回り込みを設定・解除する |
| `render_preview` | アートボードのプレビュー画像を返す |
| `doc_inspect` | 文書のレイヤー構造、オブジェクトの id・種類・範囲・塗りなどを返す（範囲の指定可） |
| `undo` / `redo` | 元に戻す／やり直す |
| `command_list` / `command_run` / `command_batch` | 許可リストにあるエンジンのコマンドを探す・実行する・まとめて実行する。許可リストにないコマンドは使えない |

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
| `--call-timeout-secs` | `ARTSERVER_CALL_TIMEOUT_SECS` | 60 | 1回の呼び出しの制限時間。実際に動き始めたときから数える。同じ時間待っても始められない呼び出し（同じ作業セッションの前のリクエストや、実行枠の空きを待っているもの）は実行せず、そのことを返す |
| `--max-body-bytes` | `ARTSERVER_MAX_BODY_BYTES` | 16777216 | 要求本文の上限 |
| `--queue-depth` | `ARTSERVER_QUEUE_DEPTH` | 8 | 1つの作業セッションの待ち行列で順番を待てるリクエストの数。超えた分は混雑中として断る |
| `--max-documents` | `ARTSERVER_MAX_DOCUMENTS` | 8 | 1セッションで開ける文書数 |
| `--history-mib` | `ARTSERVER_HISTORY_MIB` | 64 | 文書1つあたりの Undo 履歴の上限 |
| `--render-threads` | `ARTSERVER_RENDER_THREADS` | 2 | 書き出し1回あたりの描画スレッド数 |
| `--max-running` | `ARTSERVER_MAX_RUNNING` | CPU のコア数 ÷（描画スレッド数 + 1）、最低 1 | すべての作業セッションを合わせて、エンジンが同時に実行するリクエストの数。超えた分は空きを待つ。待てるのは、1回の呼び出しの制限時間まで |
| `--file-ttl-secs` | `ARTSERVER_FILE_TTL_SECS` | 600 | 大きい書き出し結果と、送られたファイルを預かっておく秒数 |
| `--max-file-mib` | `ARTSERVER_MAX_FILE_MIB` | 256 | 預かる書き出し結果と送られたファイルの合計の上限（メモリに持つ） |
| `--max-upload-mib` | `ARTSERVER_MAX_UPLOAD_MIB` | 64 | 送られたファイル1つの大きさの上限 |
| `--public-url` | `ARTSERVER_PUBLIC_URL` | なし | 外から見える ArtServer の URL（例: `https://example.com/artserver`）。書き出し結果とアップロード用の URL に使う。リバースプロキシの後ろで指定する。ホスト名は `--allowed-hosts` に自動で加わる |
| `--font-dirs` | `ARTSERVER_FONT_DIRS` | なし | OS に入っているフォントに加えて使うフォントのフォルダ（カンマ区切り）。組織で用意したフォントを置く用途。フォントのライセンスは運用する側で確認する |

ログは標準エラー出力に出ます。詳しさは `RUST_LOG`（例: `RUST_LOG=debug`）で変えられます。

## 大きい書き出し結果の受け取り

PDF や大きい SVG・PNG は、`export_document` が中身の代わりに URL を返します。ArtServer は書き出し結果をメモリに預かり（ディスクには書かない）、期限（既定 10 分）が過ぎるか作業セッションを閉じると捨てます。

```json
{"format": "pdf", "bytes": 5242880, "warnings": [],
 "download": {"url": "http://127.0.0.1:8471/files/7f3a…", "path": "/files/7f3a…", "name": "ポスター.pdf", "expiresInSeconds": 600, "note": "…"}}
```

URL は**トークンなしで取れます**。ブラウザで開くとダウンロードされ、curl でも取れます。期限までは何度でも取れます。

```sh
curl -o poster.pdf "http://127.0.0.1:8471/files/7f3a…"
```

URL の ID（256ビットの乱数）が合鍵の代わりなので、**URL を知っている人は、期限までは誰でも取れます**。URL を共有の場所に貼らないでください。取ったファイルは、ブラウザの中では開かず、必ずダウンロードになります（`Content-Disposition: attachment`、`Content-Security-Policy: sandbox`、`X-Content-Type-Options: nosniff`）。Host と Origin は、`/mcp` と同じ許可リストで検査します。

リバースプロキシの後ろで使う場合は、外から見える URL を `--public-url` で指定してください。例えば `--public-url https://example.com/artserver` なら、書き出し結果の URL は `https://example.com/artserver/files/…` になります（プロキシがパスの前置き `/artserver` を外して ArtServer に渡す構成にも対応します）。プロキシが `/files/` も ArtServer に渡すように設定してください。

`--public-url` を指定しない場合は、届いた `Host` と `X-Forwarded-Proto` の https から URL を作ります。この場合、プロキシが利用者の使ったホスト名をそのまま `Host` で渡し、`X-Forwarded-Proto` を付ける必要があります（Caddy は既定でそうなる。nginx は `proxy_set_header Host $host;` と `proxy_set_header X-Forwarded-Proto $scheme;` が必要）。

## ArtServer へのファイルの送り方

ロゴや既存の PDF など、手元のファイルを使うときは、まず AI が `upload_create` を呼びます。ファイルを1つ受け取るための URL が返ります。

```json
{"upload": "4ea8…", "url": "http://127.0.0.1:8471/uploads/4ea8…", "method": "PUT", "maxBytes": 67108864, "expiresInSeconds": 600, …}
```

ファイルの中身を、HTTP の PUT でこの URL に送ります。Claude Code は自分で送ります。

```sh
curl -T logo.png "http://127.0.0.1:8471/uploads/4ea8…"
```

Claude Desktop など、コマンドを実行できないクライアントでは、AI が URL を利用者に渡します。ブラウザで開いてファイルを選び、「Send」を押してください。その後、AI が `open_upload` で新しい文書として開くか、`place_upload` で今の文書に配置します。

- 受け付ける形式: PDF（PDF 互換で保存した Illustrator の `.ai` を含む）、SVG、PNG、JPEG、VectorCraft の `.vectorcraft`。形式はファイル名ではなく、中身から見分けます。
- PDF 互換をオフにして保存した Illustrator の `.ai` は、PDF の部分が案内のページだけです。`open_upload` でページを指定せずに開けば、エンジンがファイルの中の Illustrator 自身のデータから絵を読み込みます。`place_upload` や、ページを指定した `open_upload` では、絵の代わりに案内のページが入ります。どの使い方でも使えるように、`.ai` は PDF 互換をオンにして（Illustrator の既定）保存してください。
- 1つの URL で送れるのは1ファイルで、大きさは `--max-upload-mib`（既定 64 MiB）までです。期限は書き出し結果と同じ（既定 10 分）です。ファイルを使えるのは URL を作った作業セッションだけで、作業セッションを閉じると捨てます。トークンは不要です。書き出し結果の URL と同じく、期限までは URL を知っている人なら誰でもファイルを送れます。
- ファイルはパスではなく中身としてエンジンに渡します。配置したファイルは埋め込まれます。
- **保存して続きをする**: `.vectorcraft` は文書そのものです。ArtServer が書き出した PDF と `.ai` には、文書の編集用データが入っています。どれかを送って `open_upload` で開くと、元の文書のとおりに戻り、続きを編集できます。

## セキュリティ

- 既定では 127.0.0.1 でだけ待ち受けます。
- すべての MCP 要求に Bearer トークンが必要です。書き出し結果の受け取り（`/files/`）とファイルの送信（`/uploads/`）は、推測できない ID を含む期限付きの URL で行い、トークンは求めません。
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
- **送られたファイルは、同じプロセスの中で読み込みます。** 壊れたファイルや悪意のあるファイルで、サーバー全体が止まるおそれがあります。信頼できる人のファイルだけを送ってください（[設計方針 12.4節](docs/design-policy.md)）。
- 日本語フォントは同梱していません。サーバーの OS に入っているフォント（macOS ならヒラギノなど）か、`--font-dirs` で指定したフォルダのフォントを使います。Linux のサーバーには、日本語フォントを用意する必要があります。
- 出力ファイル（PDF の Producer など）には "VectorCraft" と記録されます。

## テスト

```sh
cargo test
```

単体テストと、実際に待ち受けたサーバーに MCP クライアントで接続するエンドツーエンドのテストが走ります。また、PDF の読み書きに使うライブラリが craft-libs の修正版（`Cargo.toml` の `[patch.crates-io]` で選んでいるもの）になっていることを `Cargo.lock` で確かめます（[設計方針 8 章](docs/design-policy.md)）。

## 依存ライブラリのライセンスと脆弱性

[cargo-deny](https://github.com/EmbarkStudios/cargo-deny) と [cargo-about](https://github.com/EmbarkStudios/cargo-about) を使います（例: `brew install cargo-deny cargo-about`）。

```sh
cargo deny check                                          # ライセンス・脆弱性・取得元の検査（設定は deny.toml）
cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md  # ライセンス表示の生成（設定は about.toml）
```

- 依存を追加・更新したら両方を実行し、`THIRD_PARTY_NOTICES.md` を更新します。
- `cargo deny check` は脆弱性のデータベース（RustSec）を、`cargo about generate` は一部のクレートのライセンス文を GitHub から取得するので、ネットワークが必要です。
- `THIRD_PARTY_NOTICES.md` は生成物です。前半の手書きの部分を変えるときは `about.hbs` を編集します。

## ドキュメント

ドキュメントは英語で書かれています。

- [設計方針](docs/design-policy.md)
- [Phase 0 の調査結果](docs/architecture.md)
- [進捗](docs/progress.md)

## ライセンス

ArtServer 自身のコードは MIT ライセンスです（[LICENSE](LICENSE)）。依存しているコンポーネントのライセンスは [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) を参照してください。

## 謝辞

ArtServer は **VectorCraft by the ArtCraft team** をベースにしています（MIT OR Apache-2.0）。ArtServer は VectorCraft の公式の派生版ではなく、ArtCraft team による承認や保証を受けたものでもありません。
