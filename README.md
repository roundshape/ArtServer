# ArtServer

English | [日本語](README.ja.md)

A headless vector graphics server that AI agents operate over the network through [MCP](https://modelcontextprotocol.io/) (Streamable HTTP). Drawing and document processing are done by the [VectorCraft](https://github.com/storytold/vectorcraft) engine.

> **Status: working on Phase 2 (adding features and checking compatibility). The MVP (Phase 1) is complete**
> It assumes a few trusted clients using it locally. Do not expose it to the internet ([Known limitations](#known-limitations)).

## Features

- Create a working session and create documents in it
- Draw shapes (rectangle, ellipse, polygon, star, line) and paths; set fill, stroke and effects
- Place text (Japanese, vertical type, area type, type on a path). List the available fonts
- Move, rotate, scale, reflect and shear; combine shapes (Pathfinder); graphs; text wrap
- Find and run the allowed engine commands (`command_list` / `command_run` / `command_batch`)
- Import SVG, and open or place files sent through an upload URL (PDF, Illustrator .ai, SVG, PNG, JPEG, VectorCraft .vectorcraft)
- Export as SVG, PDF, PNG, Illustrator .ai (PDF compatible) and VectorCraft .vectorcraft (small SVG and PNG come back in the reply; the other formats and larger files come back as a download URL). Return preview images
- Undo / redo

## Requirements

- Rust 1.95 or later
- macOS (developed and tested on Apple Silicon)
- On Linux and Windows, only the build is checked (on GitHub Actions: Linux is compiled with `cargo check` on every push, and both are fully built when their workflows are run by hand). Running on them has not been tested
- Access to crates.io and GitHub (storytold/vectorcraft, storytold/craft-libs) for the first build

## Build

```sh
cargo build            # for development; the binary is target/debug/artserver
cargo build --release  # for distribution and everyday use; the binary is target/release/artserver
```

The development build optimizes the dependencies too, so it runs fast enough for trying things locally. The release build optimizes further and takes longer to build. The examples below use `target/debug/artserver`.

## Running

### 1. Create a token (first time only)

The token is the password for using ArtServer. It is a random string you make yourself; nobody issues it. ArtServer and Claude Code hold the same value, and requests that don't match are refused. Use at least 32 characters, visible ASCII characters only.

If you save it in a file, ArtServer keeps using the same token after a restart.

```sh
mkdir -p ~/.config/artserver
chmod 700 ~/.config/artserver
openssl rand -hex 32 > ~/.config/artserver/token
chmod 600 ~/.config/artserver/token
```

### 2. Start ArtServer

```sh
./target/debug/artserver --token-file ~/.config/artserver/token
# artserver listening; MCP endpoint is /mcp
```

It listens on 127.0.0.1:8471. Keep the terminal open; press Ctrl+C to stop it. It doesn't start without a token.

You can also pass the token in the environment variable `ARTSERVER_TOKEN`. But if you generate it on every start, as in `ARTSERVER_TOKEN=$(openssl rand -hex 32)`, the token changes each time and you have to register it with Claude Code again.

**Restart after rebuilding**: rebuilding the binary doesn't change an ArtServer that is already running; it keeps running the old binary. To use the new one, stop it with Ctrl+C and start it again. Working sessions and documents are kept only in memory, so a restart discards them.

### Health check

Works without a token.

```sh
curl http://127.0.0.1:8471/healthz
# {"status":"ok"}
```

### Checking MCP with curl

```sh
U=http://127.0.0.1:8471/mcp
T=$(cat ~/.config/artserver/token)
H=(-H "Authorization: Bearer $T" -H "Content-Type: application/json" \
   -H "Accept: application/json, text/event-stream" -H "MCP-Protocol-Version: 2025-06-18")

curl -s "${H[@]}" -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' $U
curl -s "${H[@]}" -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"session_create","arguments":{}}}' $U
```

## Connecting from Claude Code

Leave ArtServer running and register it from another terminal.

### 1. Register

**A. With `claude mcp add` (easy)**

Go to the folder where you want to use ArtServer, then register it.

```sh
cd <folder where you want to use ArtServer>
claude mcp add --transport http artserver http://127.0.0.1:8471/mcp \
  --header "Authorization: Bearer $(cat ~/.config/artserver/token)"
claude mcp list   # artserver shows Connected when it can connect
```

- By default it appears **only in Claude Code started in the folder where you registered it** (local scope). Claude Code started in another folder doesn't show it.
- To use it from any folder, add `--scope user`. Then, while ArtServer is stopped, Claude Code reports a failed connection every time it starts.
- The token is stored as is in Claude Code's settings file (`~/.claude.json`).
- To remove the registration, run `claude mcp remove artserver`.

**B. With `.mcp.json` (keeps the token out of files)**

Create `.mcp.json` in the folder where you want to use ArtServer. Write `${ARTSERVER_TOKEN}` in place of the token, and Claude Code replaces it with the environment variable's value when it starts.

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

Start Claude Code with the token in the environment variable. On the first start, it asks whether to use this project's MCP servers; allow it.

```sh
ARTSERVER_TOKEN=$(cat ~/.config/artserver/token) claude
```

### 2. Restart Claude Code

Claude Code reads the MCP server registrations when it starts. A Claude Code that was already open before you registered can't use ArtServer, so quit it and start it again. `claude -c` continues from the previous conversation.

Once it starts, type `/mcp` and check that `artserver` is connected.

### 3. Try it

For example, ask: "Using ArtServer, draw a blue rounded rectangle in a 300×200 document and show it as SVG."

### If it doesn't connect

| Symptom | Likely cause |
|---|---|
| `artserver` isn't listed in `/mcp` | Claude Code was started in a different folder from the one you registered in, or you are using a Claude Code started before you registered |
| A failed connection is reported | ArtServer isn't running |
| 401 (unauthorized) | The registered token doesn't match ArtServer's token (for example, the token was regenerated). Register again |
| Method B doesn't connect | `claude` was started without `ARTSERVER_TOKEN` set |

## Tools

ArtServer publishes its own tools and VectorCraft's MCP tools. VectorCraft's tools call the engine through ArtServer's allowlist ([design policy, section 4.7](docs/design-policy.md)). Every tool takes the handle returned by `session_create` as its `session` argument.

**ArtServer tools**

| Tool | Description |
|---|---|
| `session_create` / `session_close` | Create / discard a working session |
| `document_new` | Create a new document (with one artboard) |
| `import_svg` | Open SVG markup as a new document (linked files are not read) |
| `list_fonts` | List the available font families (filter with `query`). With `family`, list that family's styles. `bundled` marks the families built into ArtServer (Source Sans 3, Source Serif 4, Inter, JetBrains Mono) that the server's OS doesn't have installed: other apps on the clients usually lack them too |
| `export_document` | Export as SVG, PNG, PDF, Illustrator `.ai` or VectorCraft `.vectorcraft`. Small SVG (up to 32 KB) comes back as text and small PNG (up to 256 KB) as an image. PDF, `.ai`, `.vectorcraft` and larger files come back as a URL to fetch them from ([Receiving large exports](#receiving-large-exports)). `delivery` chooses how the result is returned (`inline` returns the binary formats as Base64 too). The `.ai` is saved with PDF compatibility: Illustrator opens it as PDF artwork (plain paths, not its own live shapes or layers), while ArtServer and VectorCraft open it fully editable, layers included |
| `upload_create` | Make a URL to send a file to (PDF, Illustrator `.ai` saved with PDF compatibility, SVG, PNG, JPEG or VectorCraft `.vectorcraft`). Send the file with an HTTP PUT, or open the URL in a browser ([Sending files to ArtServer](#sending-files-to-artserver)) |
| `open_upload` | Open a sent file as a new document. A PDF exported by ArtServer opens as the document it was |
| `place_upload` | Place a sent file into the active document (embedded), at a point or fitted into a rectangle |

**VectorCraft tools**

| Tool | Description |
|---|---|
| `draw_shape` | Draw a rectangle, ellipse, polygon, star or line. Fill, stroke, dashes, opacity, name and effects can be set too |
| `draw_path` | Draw a path from a list of points (Bézier handles allowed) or from SVG path data |
| `set_paint` | Set fill, stroke and stroke width |
| `transform` | Move, rotate, scale, reflect, shear (can be combined, and can copy) |
| `add_text` | Place text. Font, size, color, alignment, vertical type, flowing into a frame, type on a path |
| `pathfinder` | Combine shapes (unite, minus front, intersect, exclude, divide, and more) |
| `apply_effect` | Apply an effect. Without a kind, returns the list of effects |
| `create_graph` | Make a graph from CSV or other data |
| `text_wrap` | Set or release text wrap |
| `render_preview` | Return a preview image of the artboard |
| `doc_inspect` | Return the document's layer structure and each object's id, kind, bounds, paint and so on (a range can be given) |
| `undo` / `redo` | Undo / redo |
| `command_list` / `command_run` / `command_batch` | Find, run, or run several of the engine commands on the allowlist. Commands not on the allowlist can't be used |

Coordinates are in points (1/72 inch), with the origin at the artboard's top-left and y pointing down.

## Configuration

| Argument | Environment variable | Default | Description |
|---|---|---|---|
| `--bind` | `ARTSERVER_BIND` | `127.0.0.1:8471` | Listen address |
| `--allow-public-bind` | | off | Allow listening on addresses other than loopback (use only behind a reverse proxy that terminates TLS) |
| `--token-file` | `ARTSERVER_TOKEN_FILE` | | Token file. If not given, `ARTSERVER_TOKEN` is used |
| `--allowed-hosts` | `ARTSERVER_ALLOWED_HOSTS` | `localhost,127.0.0.1,::1` | Host headers to accept |
| `--allowed-origins` | `ARTSERVER_ALLOWED_ORIGINS` | none | Origins to accept. If empty, every request with an Origin header (from a browser) is refused |
| `--max-sessions` | `ARTSERVER_MAX_SESSIONS` | 8 | Number of working sessions held at once |
| `--session-idle-secs` | `ARTSERVER_SESSION_IDLE_SECS` | 1800 | Seconds before an unused session is discarded |
| `--call-timeout-secs` | `ARTSERVER_CALL_TIMEOUT_SECS` | 60 | Time limit for one call, counted from when it starts running. A call that can't start within as long (behind the working session's earlier requests, or waiting for a free slot) is not run, and says so |
| `--max-body-bytes` | `ARTSERVER_MAX_BODY_BYTES` | 16777216 | Maximum request body size |
| `--queue-depth` | `ARTSERVER_QUEUE_DEPTH` | 8 | Requests one working session can have waiting in its queue; more are refused as busy |
| `--max-documents` | `ARTSERVER_MAX_DOCUMENTS` | 8 | Number of documents open in one session |
| `--history-mib` | `ARTSERVER_HISTORY_MIB` | 64 | Undo history limit per document |
| `--render-threads` | `ARTSERVER_RENDER_THREADS` | 2 | Rendering threads per export |
| `--max-running` | `ARTSERVER_MAX_RUNNING` | CPU cores ÷ (rendering threads + 1), at least 1 | Requests the engine runs at once across all working sessions. Others wait for a free slot, up to the time limit of one call |
| `--file-ttl-secs` | `ARTSERVER_FILE_TTL_SECS` | 600 | Seconds large exports and uploaded files are kept |
| `--max-file-mib` | `ARTSERVER_MAX_FILE_MIB` | 256 | Total size limit of the kept exports and uploaded files (kept in memory) |
| `--max-upload-mib` | `ARTSERVER_MAX_UPLOAD_MIB` | 64 | Size limit of one uploaded file |
| `--public-url` | `ARTSERVER_PUBLIC_URL` | none | ArtServer's URL as seen from outside (e.g. `https://example.com/artserver`). Used for export and upload URLs. Set it behind a reverse proxy. Its host name is added to `--allowed-hosts` automatically |
| `--font-dirs` | `ARTSERVER_FONT_DIRS` | none | Font folders to use in addition to the fonts installed in the OS (comma-separated), for fonts your organization provides. Checking the font licenses is up to whoever runs the server |

Logs go to standard error. Set the level of detail with `RUST_LOG` (e.g. `RUST_LOG=debug`).

## Receiving large exports

For PDF and large SVG or PNG, `export_document` returns a URL in place of the content. ArtServer keeps the export in memory (it never writes it to disk) and discards it when it expires (10 minutes by default) or when the working session is closed.

```json
{"format": "pdf", "bytes": 5242880, "warnings": [],
 "download": {"url": "http://127.0.0.1:8471/files/7f3a…", "path": "/files/7f3a…", "name": "poster.pdf", "expiresInSeconds": 600, "note": "…"}}
```

The URL **can be fetched without a token**. Opening it in a browser downloads the file, and curl works too. It can be fetched any number of times until it expires.

```sh
curl -o poster.pdf "http://127.0.0.1:8471/files/7f3a…"
```

The ID in the URL (a 256-bit random number) serves as the key, so **anyone who knows the URL can fetch the file until it expires**. Don't paste the URL in shared places. The file is always downloaded and never opened inside the browser (`Content-Disposition: attachment`, `Content-Security-Policy: sandbox`, `X-Content-Type-Options: nosniff`). The Host and Origin headers are checked against the same allowlists as `/mcp`.

Behind a reverse proxy, give the URL seen from outside with `--public-url`. For example, with `--public-url https://example.com/artserver`, export URLs become `https://example.com/artserver/files/…` (this also works when the proxy strips the `/artserver` prefix before passing requests to ArtServer). Configure the proxy to pass `/files/` to ArtServer as well.

Without `--public-url`, the URL is built from the request's `Host` header and the https of `X-Forwarded-Proto`. In that case the proxy must pass the host name the user used as `Host` unchanged and add `X-Forwarded-Proto` (Caddy does this by default; nginx needs `proxy_set_header Host $host;` and `proxy_set_header X-Forwarded-Proto $scheme;`).

## Sending files to ArtServer

To use a file you have, such as a logo or an existing PDF, the AI first calls `upload_create`. It returns a URL that takes one file:

```json
{"upload": "4ea8…", "url": "http://127.0.0.1:8471/uploads/4ea8…", "method": "PUT", "maxBytes": 67108864, "expiresInSeconds": 600, …}
```

The file's raw bytes go to the URL with an HTTP PUT. Claude Code does this itself:

```sh
curl -T logo.png "http://127.0.0.1:8471/uploads/4ea8…"
```

A client that can't run commands, such as Claude Desktop, can give you the URL instead: open it in a browser, choose the file and press Send. Then the AI calls `open_upload` to open the file as a new document, or `place_upload` to put it into the active document.

- Accepted formats: PDF (including Illustrator `.ai` files saved with PDF compatibility), SVG, PNG, JPEG and VectorCraft `.vectorcraft`. The format is told from the file's contents, not from its name.
- A URL takes one file, up to `--max-upload-mib` (64 MiB by default), and expires like exports (10 minutes by default). Only the working session that made it can use the file, and closing the session discards it. No token is needed: as with download URLs, anyone who knows the URL can send a file to it until then.
- The file is passed to the engine as data, never as a path, and placed files are embedded.
- **Saving and resuming:** a `.vectorcraft` file is the document itself, and a PDF or `.ai` exported by ArtServer carries the document's editing data. Send one back and open it with `open_upload`, and the document comes back as it was, ready to edit.

## Security

- By default it listens only on 127.0.0.1.
- Every MCP request needs the Bearer token. Exports are fetched from `/files/`, and files are sent to `/uploads/`, through expiring URLs that contain an unguessable ID; no token is needed there.
- The Host and Origin headers are checked (against DNS rebinding).
- Engine commands go through an allowlist. Commands that change process-wide settings or handle files can't be run.
- File paths are not accepted. Input and output travel in tool arguments and results. The engine's file access is confined to an empty temporary directory.
- Sessions, queues, body size, documents, history and run time are limited.

## Known limitations

- **TLS is not implemented.** To expose ArtServer outside, put it behind a reverse proxy that terminates TLS.
- **Only one user (one token).** Multiple users and permission management are not implemented.
- **All sessions run in one process.** So the following can't be prevented ([design policy, section 6.5](docs/design-policy.md)):
  - Forcibly stopping work that doesn't respond to cancellation. Past the time limit, ArtServer asks for cancellation, but the session stays busy until the work stops.
  - The whole server stopping because of a stack overflow or running out of memory
  - Memory shared between sessions through shared caches
- Rate limiting is not implemented.
- **Files sent for import are read in the same process.** A broken or malicious file could stop the whole server, so send only files from people you trust ([design policy, section 12.4](docs/design-policy.md)).
- No Japanese fonts are bundled. ArtServer uses the fonts installed in the server's OS (such as Hiragino on macOS) or the fonts in the folders given with `--font-dirs`. A Linux server needs Japanese fonts installed.
- Output files record "VectorCraft" (in the PDF Producer, for example).

## Tests

```sh
cargo test
```

This runs the unit tests and the end-to-end tests, which connect an MCP client to a server that is actually listening. It also checks in `Cargo.lock` that PDF reading and writing use the fixed versions of their libraries from craft-libs, which `[patch.crates-io]` in `Cargo.toml` selects ([design policy, chapter 8](docs/design-policy.md)).

## Dependency licenses and vulnerabilities

ArtServer uses [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) and [cargo-about](https://github.com/EmbarkStudios/cargo-about) (e.g. `brew install cargo-deny cargo-about`).

```sh
cargo deny check                                          # check licenses, vulnerabilities and sources (settings in deny.toml)
cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md  # generate the license notices (settings in about.toml)
```

- After adding or updating dependencies, run both and update `THIRD_PARTY_NOTICES.md`.
- Both need the network: `cargo deny check` fetches the vulnerability database (RustSec), and `cargo about generate` fetches the license texts of some crates from GitHub.
- `THIRD_PARTY_NOTICES.md` is generated. To change its hand-written first part, edit `about.hbs`.

## Documentation

- [Design policy](docs/design-policy.md)
- [Phase 0 investigation](docs/architecture.md)
- [Progress](docs/progress.md)

## License

ArtServer's own code is licensed under the MIT License ([LICENSE](LICENSE)). For the licenses of the components it depends on, see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## Acknowledgments

ArtServer is based on **VectorCraft by the ArtCraft team** (MIT OR Apache-2.0). ArtServer is not an official VectorCraft derivative, and it is not approved or warranted by the ArtCraft team.
