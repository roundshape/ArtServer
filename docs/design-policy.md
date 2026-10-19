# ArtServer Design Policy

- Created: 2026-10-17
- Revisions:
  - 2026-10-17 Reflected the external review on process isolation (sections 4.3, 6.2, 6.5; chapters 11, 12; section 13.2)
  - 2026-10-17 Decided to adopt Option A-2. Reflected support for the latest MCP specification (2026-07-28 version) (section 3.3; chapter 4; sections 5.1, 6.1, 6.4, 10.2; chapters 11, 13)
  - 2026-10-18 Agreed the intended usage with the project owner and added it (sections 1.1, 7.3, 7.4, 13.1)
  - 2026-10-18 Updated the status to match implementation progress (chapter 11; sections 13.1, 13.2)
  - 2026-10-18 Agreed the policy for passing large data with the project owner and added it (sections 4.6, 6.3, 13.2)
  - 2026-10-18 Added the tool structure (hand-written tools, and a generic tool with an allowlist) (sections 4.7, 6.2, 13.2)
  - 2026-10-18 Revised the tool structure, changing to using VectorCraft's tools through ArtServer's Backend (sections 3.1, 3.2, 3.3, 4.1, 4.2, 4.7, 6.2; chapter 8; section 13.2)
  - 2026-10-18 Implemented the approach in section 4.7 (heading of section 4.7, section 13.2)
  - 2026-10-18 Added the policy of fixing VectorCraft bugs by replacing individual crates (chapter 8)
  - 2026-10-18 Implemented URL delivery for export in section 4.6 (sections 4.6, 13.2)
  - 2026-10-18 Changed retrieval of export results to expiring URLs that need no token (sections 4.6, 13.2)
  - 2026-10-18 Added `--public-url` to set the externally visible URL (section 4.6)
  - 2026-10-18 Verified the build on Windows (sections 1.1, 13.1)
  - 2026-10-18 Updated VectorCraft to v0.13.1 (`904f0fe`) (chapter 8)
  - 2026-10-19 Translated into English
  - 2026-10-19 Updated the status of Phase 2 to in progress (chapter 11)
  - 2026-10-19 Decided to leave "VectorCraft" in exported files as is (section 13.2)
  - 2026-10-19 Recorded the trademark check for "ArtServer" (chapter 9, section 13.2)
  - 2026-10-19 Implemented upload for import, and recorded the idea of a server-side image library (sections 4.6, 4.7, 6.3, 12.4, 13.2; chapter 11)
  - 2026-10-19 Added `.ai` (PDF-compatible only) and `.vectorcraft` export, and `.vectorcraft` upload (section 4.6; chapter 11)
  - 2026-10-19 Write `.ai` without PDF layers, after checking in Illustrator (section 4.6)
  - 2026-10-19 Mark the bundled fonts in `list_fonts`, and tell the AI to prefer installed fonts (section 7.4)
  - 2026-10-19 Added a server-wide limit on the jobs running at once (section 5.3)
  - 2026-10-19 Made the queue length of a working session a setting (section 5.1)
  - 2026-10-19 Opened the first batch of document commands (section 4.7; chapter 11)
  - 2026-10-19 Opened the second batch of document commands (section 4.7; chapter 11)
  - 2026-10-19 Opened the third batch: the namespaces section 4.7 didn't classify (section 4.7; chapter 11)
  - 2026-10-19 Opened the fourth batch: the commands decided individually (section 4.7; chapter 11)
  - 2026-10-19 Decided not to extend `export_document` with the closed exports' features for now (section 4.7; chapter 11)
  - 2026-10-19 Decided not to work on CJK fonts on Linux for now (section 7.3; chapter 11; section 13.2)
  - 2026-10-19 The time limit counts from when a request starts, and waiting to start has a limit of its own (sections 5.1, 5.3)
  - 2026-10-19 A test, not CI, checks that the PDF backend is the fixed version (chapter 8)
  - 2026-10-19 Verified older clients (2025-06-18, 2025-11-25) with an E2E test; recorded that no test covers the 2026-07-28 version (sections 4.4, 13.2)
  - 2026-10-19 Decided not to support Docker (chapter 11; sections 12.1, 12.6)
  - 2026-10-19 Decided not to switch color settings per job for now (section 13.2)
- Status: **Adopted (approved by the project owner on 2026-10-17)**
- Related documents:
  - `docs/DEVELOPMENT_PLAN.md`: development policy and handover document (the original source of goals and constraints)
  - `docs/architecture.md`: Phase 0 investigation results and their basis (with locations in the code)
  - `docs/ARTSERVER_PROCESS_ISOLATION_REVIEW.md`: external review of future process isolation
- Purpose of this document: collect in one place the **design policy** settled in the Phase 0 investigation and the discussions after it. For the details of the basis, see `docs/architecture.md`.

---

## 1. What ArtServer is

A **vector graphics server with no GUI** that AI agents and external systems can drive through **MCP over the network**.

- It does not build a rendering and document-processing engine from scratch. It reuses the engine of **VectorCraft** (an Illustrator-style vector editor written in Rust).
- It has a product name and repository separate from VectorCraft. ArtServer's own code is published under the MIT license.
- The long-term goal is a feature scope equivalent to a "server version of Illustrator". Automatic generation of figures and print data from DB/CSV data is also in view.

### 1.1 Intended usage

Agreed with the project owner on 2026-10-18. **ArtServer is used by users within a single organization**, and it is not designed as a service for the general public.

| Mode | Server location | Users |
|---|---|---|
| Standalone | The user's own PC | That person only |
| Internal LAN | A server inside the organization | The organization's PCs |
| The organization's own cloud | A cloud server provided by the organization | The organization's PCs |

Even though it is published as open source, each organization runs its own ArtServer for itself. Sharing one ArtServer among several organizations is not expected.

From this premise, we take the following policies.

- **No isolation between organizations (multi-tenancy) is built.** Authentication and permissions that separate users within the same organization (multiple users in Phase 3, chapter 11) are needed for use on an internal LAN or in the cloud.
- **Network-level protections stay.** On an internal LAN or in the cloud, encrypted communication (TLS; currently terminated at a reverse proxy) and per-user authentication are needed. Because of the limitations in section 6.5, it is still not exposed to untrusted users.
- **The server environment matches the organization's clients.** Glyph shapes are determined by the server's fonts (chapter 7). In standalone mode they naturally match the client. On an internal LAN or in the cloud, the organization aligns the server's OS and fonts with its clients.
- **Supported OSes**:
  - macOS: the environment used for development and verification
  - Linux: expected to have few users. Since VectorCraft supports it, we only keep the build passing (GitHub Actions, `.github/workflows/build.yml`). No tests or runtime verification
  - Windows: expected to be needed as standalone or as an internal server by organizations whose PCs run Windows. The build is confirmed to pass (GitHub Actions, the manually run `.github/workflows/build-windows.yml`). Behavior on a real machine is not yet verified, and the priority of support is undecided (section 13.1)
- **Font licensing is the operator's responsibility.** ArtServer only provides the ability to use the fonts on the server. It does not check licenses or restrict use. ArtServer is responsible only for the fonts that ArtServer itself distributes (the bundled fonts, and also any fonts embedded at build time) (chapter 9).

---

## 2. Basic policies

1. **Reuse the engine and build only the server part.** Leave rendering, path processing and file I/O to VectorCraft.
2. **Do not postpone security.** Do not put getting HTTP connections working first and end up with a dangerous remote-control server.
3. **Design for concurrent execution from the start.** Adding it later would mean rebuilding the structure.
4. **Default to the safe side.** Reject commands that are not allowed, do not use file access by default, and listen only on 127.0.0.1.
5. **Do not pile up temporary wrappers.** Rather than retrofitting parts whose assumptions do not fit, separate responsibilities and design from the start.
6. **State openly the limits that cannot be enforced within the process.** Keep a structure that future process isolation can solve.

---

## 3. Chosen approach

### 3.1 Policy: Option A-2

**Embed `vectorcraft-engine` as a library, and design the MCP part on the ArtServer side.**

| Option | Description | Decision |
|---|---|---|
| A-1 | Embed VectorCraft's existing MCP (`vectorcraft-mcp`) and build only the HTTP part | Rejected (only the tool part is used; see the revision below) |
| **A-2** | **Embed the engine and build the MCP part in ArtServer** | **Adopted** |
| B | Start the existing CLI (stdio MCP) as a separate process and relay to it from HTTP | Rejected |
| C | Copy and modify VectorCraft's source | Rejected |

**2026-10-18 revision**: The server and transport parts of `vectorcraft-mcp` are not used. However, the tool part (`call_tool` and `Backend`) is published as a component usable from outside, and ArtServer uses it through a Backend that ArtServer implements (section 4.7).

### 3.2 Reasons

- **The engine can be used as is**: The rendering and document-processing layers do not depend on the GUI, and rendering runs on the CPU only. Every feature can be called as "command ID + JSON arguments", so mapping to MCP tools is easy.
- **The existing MCP is not meant for network exposure**: It is stdio-only and assumes one local user, so all of the roughly 700 engine commands can be called. Used with multiple sessions, logs leak, and the names and tool descriptions stay VectorCraft's. Even with A-1, the needed parts would mostly be rebuilt.
- **Drawbacks of B and C**: With B, process management, error propagation and performance are hard to manage. C would require manually tracking about 550,000 lines of code updated almost daily, which is not realistic.
- We do not depend on the server and transport parts of the existing MCP, and **treat them as a reference implementation**. In particular, its queue, cancellation and busy response within one session are a good model. The tool part is used through ArtServer's Backend (section 4.7; revised 2026-10-18).

### 3.3 Terms

| Term | Meaning |
|---|---|
| crate | A unit of Rust library/program. Close to a "package" in other languages. Not a type |
| MCP | Model Context Protocol. The standard protocol for AI agents to call external tools |
| Streamable HTTP | The transport for using MCP over HTTP (instead of stdio) |
| Session | The unit of work of the VectorCraft engine. Holds multiple documents, Undo history and selection state |
| working session | The unit of work managed by ArtServer. Corresponds to one engine Session and is identified by a handle |
| handle | The identifier of a working session, issued by ArtServer. Passed as a tool argument |
| Backend | The plug-in point that VectorCraft's MCP tools use to call the engine (a Rust trait; close to a Swift protocol). ArtServer implements it and adds checks. It is not a separate server or process |
| MVP | Minimum Viable Product. The smallest complete form in which the core flow works end to end |

---

## 4. Overall architecture

### 4.1 Architecture diagram

```text
MCP client (Claude Code, etc.)
        │  HTTP(S) + MCP Streamable HTTP   * TLS is terminated at a reverse proxy
        ▼
┌───────────────────────────────── artserver (single binary) ──────────────────────────────────┐
│ HTTP layer          bind 127.0.0.1 / Origin/Host checks / body size limit / connection limit │
│ Auth layer          Bearer token                                                             │
│ MCP layer           rmcp (official Rust SDK) / protocol version detection / tools, resources │
│ Tool layer          ArtServer's tools + VectorCraft's tools (through Backend, section 4.7)   │
│ Policy layer        allowlist (deny by default) / nested command checks / argument checks    │
│ Session management  handle ↔ session executor / session limit / idle disposal                │
│ Session executor    a dedicated thread per session owns the engine Session                   │
└───────────────────────────────────────────────┬──────────────────────────────────────────────┘
                                                ▼
                       vectorcraft-engine (git dependency, pinned commit)
```

### 4.2 Module responsibilities

| Module | Responsibility |
|---|---|
| `config` | Settings (bind address, token, data directory, various limits). Read from CLI arguments and environment variables |
| `http` | HTTP server, `/mcp`, `/healthz`. Host/Origin checks and the body size limit are enabled through rmcp settings |
| `auth` | Token verification |
| `mcp` | MCP protocol handling. Plugs into rmcp and leaves protocol version detection and header validation to it. Handles tool calls and error conversion |
| `tools` | Definitions of ArtServer's tools and their conversion into sequences of engine commands. Exposing VectorCraft's tools (with an input schema that adds the working session handle) and passing calls on to `call_tool` (section 4.7) |
| `backend` | GuardedBackend. Runs engine calls from VectorCraft's tools only after they pass the allowlist and argument checks. Runs on the session executor's thread (section 4.7) |
| `policy` | Checks of allowed commands and arguments |
| `session` | The table of working sessions (handle ↔ executor), issuing and verifying handles, limits, idle disposal |
| `engine_host` | Owning the Session on a dedicated thread, applying file restrictions and cancellation, handling panics and timeouts |

### 4.3 Engine execution boundary

`engine_host` is "the layer that actually runs the engine", and all engine operations are gathered here.

```text
MCP / HTTP
    ▼
Tool / policy layers
    ▼
Session management
    ▼
Engine execution boundary (engine_host)
    ├─ Now: a dedicated thread per session
    └─ Future: worker processes (chapter 12)
    ▼
vectorcraft-engine
```

- Do not let the MCP and HTTP layers operate `vectorcraft_engine::Session` directly.
- Data that crosses the boundary is limited to **simple data with clear ownership** (session identifier, command ID, JSON arguments, results, errors).
  - The engine's public API has the form `Session::execute(command ID, JSON arguments)`, so this is easy to keep.
  - If processes are separated in the future, this data can go over inter-process communication as is. However, we do not define or implement an inter-process communication spec now.
- Do not expose `Session`, document objects, locks, etc. to the HTTP layer. Do not build references to engine-internal objects into the MCP API spec.
- Requests within the same session are run one at a time, in arrival order (section 5.1).
- The "boundary" here means **separation of roles**. At this point we do not build two implementations, a thread version and a process version. Rust trait abstractions are used only when they help separate the current code, and are not added just for a future implementation.

### 4.4 MCP protocol support

The latest MCP specification (2026-07-28 version) changed the shape of Streamable HTTP significantly.

| Change | Handling in ArtServer |
|---|---|
| The protocol-level session (`Mcp-Session-Id`) was removed | For working sessions, **a handle issued by ArtServer is passed as a tool argument** (the method the spec suggests). HTTP connections and MCP sessions hold no state (section 4.5) |
| The `initialize` handshake was removed, and each request carries the protocol version. `server/discover` became required | Left to rmcp |
| The GET stream was removed; only POST remains | Left to rmcp (returns 405 for GET/DELETE) |
| The `MCP-Protocol-Version` / `Mcp-Method` / `Mcp-Name` headers became required, and they must be validated to match the body | Left to rmcp (behavior to be checked during implementation) |
| Closing the response stream became the signal for cancellation | Connected to the engine's cooperative cancellation |
| The Logging feature was deprecated | MCP's logging feature is not implemented |
| Origin header validation (MUST), binding to localhost (SHOULD), authentication (SHOULD) | Origin checks are enabled through rmcp settings (disabled by default, so they are enabled explicitly). Binding and authentication are implemented in ArtServer (section 6.1) |

- **rmcp (the official Rust SDK, 3.5 series) is used for the MCP protocol layer.** The spec changes fast, and keeping up with header validation and protocol version detection on our own is a heavy burden. The transport spec can change between minor versions, so the version is pinned down to the minor version.
- Checking the rmcp 3.5.1 source showed that the Host check (loopback only by default), the Origin check (disabled by default; enabled with `enforce_origin_validation`) and the body size limit (413 when exceeded) can be done with rmcp settings. ArtServer enables these through settings, and implements only authentication (Bearer token), which rmcp lacks, as axum middleware.
- If rmcp's Host allowlist is empty, it lets every Host through. ArtServer rejects an empty allowlist at startup.
- rmcp returns tool argument validation errors (unknown arguments, etc.) not as protocol errors but as tool error results (`isError: true`). This lets the AI fix its own arguments, and follows the recommendation of the new MCP spec.
- axum is used as the HTTP framework and tokio as the async runtime, because rmcp's Streamable HTTP server is built to plug into an axum router.
- **Older clients** (the 2025-06-18 and 2025-11-25 versions, which use `initialize` and `Mcp-Session-Id`) are also supported. rmcp implements the 2026-07-28 version and states that it is compatible with earlier versions. Since state is held in handles, it is enough not to issue session IDs to older versions either. **Verified by an E2E test (2026-10-19)**: for 2025-06-18 and 2025-11-25, `older_clients_work_without_protocol_sessions` in `tests/e2e.rs` sends the exchange as raw HTTP (`initialize`, `notifications/initialized`, `tools/list`, and tool calls from `session_create` through a drawing and an SVG export to `session_close`). The version comes back as asked, the answers are plain JSON with no `Mcp-Session-Id`, the notification gets 202, the handle carries the state from one request to the next, and GET gets 405. rmcp's client in the other E2E tests also starts with `initialize` and settles on 2025-11-25, so **the 2026-07-28 version (`server/discover` and per-request metadata) is not covered by a test**.

### 4.5 Working sessions and handles

- The `session_create` tool issues a handle, and the other tools take the handle as an argument. `session_close` discards it. Sessions that exceed the idle time are also discarded.
- A handle is a random value long enough that it cannot be guessed. It is bound to the principal (token) it was issued to, and cannot be used by other principals (groundwork for multi-user support).
- One handle corresponds to one engine Session. One working session can handle multiple documents (an engine feature).
- When processes are separated in the future, mapping the handle to an HTTP header with `x-mcp-header` would let a load balancer route requests to workers without reading the body. Not implemented now.

### 4.6 Passing large data (export implemented 2026-10-18; upload for import implemented 2026-10-19)

Agreed with the project owner on 2026-10-18. Tool results go straight into the AI's conversation, so the current approach of putting file contents into the response as Base64 (section 6.3) does not suit large data. Claude Code also has a limit on MCP tool output size. For import, the AI has to write out the file's Base64 as an argument, which is not realistic for large files.

**Policy: combine putting data in the response (Option A) with passing it by URL (Option B).**

| Category | How it is passed |
|---|---|
| What the AI looks at (result summaries, document state, small PNG previews, small SVGs) | In the response (the current approach) |
| What is delivered as a file (PDF, `.ai`, `.vectorcraft`, large PNG/SVG) | Stored temporarily on the server, and an ID and URL are returned. The file itself is downloaded through an HTTP endpoint separate from MCP |
| Import | Small SVGs are passed as an argument through `import_svg`. Anything else is sent to an upload URL that the `upload_create` tool makes, and the upload's ID is passed to `open_upload` or `place_upload` (implemented 2026-10-19; details below) |

- **Which way to return**: By default it is decided automatically by size, and the caller can also specify it with a tool argument. The threshold is based on the capacity of the AI's conversation, roughly tens of KB of text. For PNG, think in pixel count, and only preview-sized images go in the response.
- **Keep the response shape the same**: In both cases, always return a summary such as format, size, page count and warnings. The only difference is contents versus URL.
- **File management**: IDs are unguessable random values, usable only by the working session (principal) that created them. They get a short expiry and are deleted when the working session closes. Limits are set on the number and total size of stored files.
- **Do not let the engine touch files**: For export, ArtServer receives Base64 from the engine as before and turns it into a file. For import, ArtServer reads the file and passes it to the engine as Base64. The engine's file restrictions (section 6.3) and the policy of not letting clients specify file paths do not change.
- **Guidance for the AI**: The tool descriptions explain what to do when a URL is returned (save it with `curl` etc.).
- Returning the URL as an MCP "resource link" is also considered. How Claude Code handles it will be checked during implementation (section 13.2).
- Options not taken: export to a fixed folder (handy for standalone, but unusable in the cloud; if needed, adding it for standalone will be considered). Relying on compression alone (does not solve the problem of the data entering the conversation).

**Decided during implementation (2026-10-18, agreed with the project owner)**

| Item | Details |
|---|---|
| File store | **Memory** (`FileStore` in `src/files.rs`). Nothing is written to disk. Lost when the server stops |
| Download endpoint | `GET /files/{id}`. **No token is required; the ID in the URL is the key** ("Authentication revisited" below). Host and Origin are checked against the same allowlist as `/mcp` (rmcp's checks apply only to `/mcp`, so the same idea was implemented here). It is always a download and never opened inside the browser (`Content-Disposition: attachment`, `Content-Security-Policy: sandbox`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`) |
| ID | 32-byte random value (hex) |
| Default delivery (auto) | SVG up to 32KB and PNG up to 256KB go in the response. PDF is always a URL, as are `.ai` and `.vectorcraft` (added 2026-10-19). It can also be set with `delivery` (auto, inline, url) of `export_document` |
| Expiry and limits | Expiry defaults to 10 minutes (`--file-ttl-secs`). The total defaults to 256MiB (`--max-file-mib`); beyond that, requests are refused. Up to 8 files per working session; beyond that, the oldest are discarded |
| Deletion | When the expiry passes (also removed by a cleanup every 60 seconds), and when the working session closes. Files remain until expiry even after being downloaded (so they can be downloaded again, and so that clients that prefetch the contents of displayed links do not make them disappear before the user opens them) |
| URL | If `--public-url` is given (e.g. `https://example.com/artserver`), `/files/{id}` is appended to it. This also supports a path prefix on a reverse proxy. The given host name is added to `--allowed-hosts` automatically. If not given, the URL is built from the incoming `Host` and, for https, `X-Forwarded-Proto` (this depends on the reverse proxy settings) |
| File name | Built from the document name (`title` of `document.inspect`). `Content-Disposition` carries an ASCII fallback name and a UTF-8 name |
| Response | `{format, bytes, warnings, download: {url, path, name, expiresInSeconds, note}}`. The same thing goes into content and structuredContent |

- **Authentication revisited (2026-10-18, the project owner's decision)**: At first the same Bearer token as MCP was required, but opening the URL in a browser gave `unauthorized`. Browsers and clients that cannot run curl (such as Claude Desktop) could not retrieve files, and the AI also had to be told where the token was, so we switched to expiring URLs that need no token. The risk is reduced by the unguessable ID (256 bits), the short expiry (10 minutes by default), deletion when the working session closes, and the premise that users are confined to a single organization (section 1.1). URLs are also shown in the AI conversation, so the README states clearly that anyone who knows the URL can retrieve the file until it expires. Deleting a file after one download was not adopted, because link prefetching could delete it.


**Decided during the implementation of uploads (2026-10-19, agreed with the project owner)**

| Item | Details |
|---|---|
| Approach | A URL made by a tool (option A of the four compared: a URL from a tool, an upload endpoint that takes the Bearer token, a fixed inbox folder, fetching from a web URL). `upload_create` returns a URL that takes one file; the AI sends it with `curl -T`, or the user opens the URL in a browser and chooses the file. The AI needs no token, and clients without a shell can still send files |
| Endpoint | `PUT /uploads/{id}` takes the raw bytes as the body. **No token is required; the ID in the URL is the key**, as for downloads. One file per URL (a second PUT gets 409). Host and Origin are checked as for `/files/`; an Origin also passes when it is this server's own (that of the request, or of `--public-url`), because the browser page sends from there |
| Browser page | `GET /uploads/{id}` shows a page with a file chooser. Its script (`/uploads/upload.js`, served by ArtServer) sends the file with a PUT to the same URL. The page may run only that script and talk only to this server (`Content-Security-Policy`) |
| Formats | PDF (including Illustrator `.ai` files saved with PDF compatibility), SVG, PNG, JPEG and VectorCraft's native `.vectorcraft`, **told from the contents** (the first bytes), not from the name, because the engine also picks its reader from the contents. Anything else gets 415. EPS and other PostScript (including the legacy `.ai`), compressed SVG and PSD are not accepted yet. A `.vectorcraft` file is told by VectorCraft's own test (`vectorcraft_format::sniff`), compressed or not. `import_svg` now checks its argument the same way |
| Limits | One file up to `--max-upload-mib` (default 64 MiB; a bigger body gets 413). Uploads share the file store with exports: the same total limit (`--max-file-mib`), the same expiry (`--file-ttl-secs`, counted again from when the file arrives), at most 8 per working session (counted apart from exports), and deletion when the working session closes |
| Using the file | `open_upload` opens it as a new document (`document.open`), and `place_upload` places it into the active document (`file.place`). ArtServer passes the contents as data (`name` and `dataBase64`), so the engine still never touches a file. Placed files are always embedded (`link: false`). Only the working session that made the upload can use it, any number of times until it expires. Both commands are ArtServer-only in the allowlist and accept only data whose name ends in svg, pdf, ai, png or jpg |
| Saving and resuming | ArtServer's PDF exports carry the native document as editing data, so sending such a PDF back and opening it restores the document as it was (`"restored": true`; checked by an E2E test). This serves for saving work and continuing it later, without a separate save format. ArtServer's `.ai` does the same, and a `.vectorcraft` file is the document itself |

- **.ai and .vectorcraft export (2026-10-19, agreed with the project owner)**: `export_document` also writes Illustrator `.ai` and VectorCraft's native `.vectorcraft`, both always delivered by URL, as `application/octet-stream` so that browsers keep the file name. The `.ai` is **PDF-compatible only**: a PDF with VectorCraft's editing data, **without PDF layers** (`createLayers: false`, decided later on 2026-10-19). With PDF layers, Illustrator turned each layer's form into a clip group whose clipping path (±32,767 pt) is far larger than the page, so a click selected that group and its selection box was off-screen; it never made them its own layers. Without them, the shapes come in as plain paths directly in a layer. Illustrator's own layers and live shapes live in its undocumented editing data, so they can't be written either way; the document's layers stay in VectorCraft's editing data, and ArtServer and VectorCraft open the file fully editable. The legacy PostScript `.ai` (the Illustrator 7.0 format, the only one Adobe published), which Illustrator could edit as objects but which turns Japanese text into outlines and drops transparency, is not written: the allowlist refuses `aiVersion` other than `pdf`. `.vectorcraft` files are written compressed.
- **Future: a server-side image library.** The project owner's long-term idea is to keep images in a library (a database) on the server and place them from there. `place_upload` takes the ID of a temporary upload now; it can later take the ID of a library item as well. Either way ArtServer reads the bytes and passes them to the engine as data, so the policy of not letting the engine touch files stays. Not scheduled.
### 4.7 Tool structure: using VectorCraft's tools through ArtServer's Backend (implemented 2026-10-18)

The approach agreed with the project owner on 2026-10-18. On the same day, it replaced the plan of "hand-written tools combined with a generic tool with an allowlist".

**Background**

- Engine commands, counted statically from the source, number about 760 (in 60 categories). Each has an argument description meant to be read by AI (`CommandSpec.params`; e.g. `shape.rectangle` is `{x, y, width, height, radius?: pt, rotation?: deg (about centre)} → {id}`). It is not JSON Schema.
- The 15 MVP tools were hand-written one by one. Expanding to an Illustrator-equivalent scope by this method alone is not realistic.
- VectorCraft's MCP (`crates/mcp`) has tools crafted for AI (`add_text`, `transform`, `draw_path`, etc.) and generic tools that find and run commands (`command_list`, `command_run`, `command_batch`). The tool part is published as a component usable from outside (`crates/mcp/src/lib.rs:26-31`): `call_tool(backend, tool name, arguments)`, and the trait `Backend`, the entry point the tools use to call the engine (its only function is `call(method, params)`).
- Findings (2026-10-18, VectorCraft `fdac986`):
  - The editing tools and the generic tools make every engine call through `Backend`. The entry points used are `engine.execute`, `engine.commands`, `document.inspect`, `document.node` and `ui.render`.
  - Only the screen capture tools (`screenshot`, `ui_screenshot`) touch files directly. `call_tool` does not use the shared logging mechanism.
  - The dependencies of `vectorcraft-mcp` are only ones ArtServer already uses, so no new external libraries are added.
  - The tool part is under active improvement (`crates/mcp/src/tools.rs` was updated 43 times in the last 60 days).
  - The engine also exposes what the allowlist mechanism needs: `command_specs()` (all commands), `Session::commands()` (whether each is usable in the current state, with the reason), `check_params` (argument checking), `steps::resolve` (references to the results of earlier steps).

**Approach**

ArtServer implements `Backend` (hereafter GuardedBackend), and VectorCraft's tools run on top of it. No separate server or process is started.

```text
AI → ArtServer MCP layer (checks the working session handle)
       → session executor thread (section 4.3)
           → vectorcraft_mcp::call_tool (add_text, etc.)
               → GuardedBackend (ArtServer): checks allowlist, paths, arguments, document count
                   → vectorcraft-engine
```

| GuardedBackend entry point | Handling |
|---|---|
| `engine.execute` | Runs after passing the same checks as the current tools (allowlist, file paths, `check_params`, document count limit) |
| `engine.commands` | Returns only the commands in the allowlist (this becomes the result of `command_list`) |
| `document.inspect`, `document.node` | Run through the allowlist |
| `ui.render` | Rendering for previews (used by `render_preview`). Implemented with the engine's PNG export |
| Everything else (`ui.*`, `app.*`, etc.) | Refused |

- VectorCraft's `Headless` (VectorCraft's own Backend implementation) is not used, because it has entry points for opening and saving files.
- `call_tool` runs on the session executor's thread, within the current file restrictions, cancellation and time limit. What crosses the engine execution boundary is simple data (tool name, JSON arguments and results), so it also fits the policy in section 4.3 (future process isolation).
- ArtServer converts the results of `call_tool` (text and images) into MCP results.

**Tools used**

| Category | Tools |
|---|---|
| VectorCraft's tools (exposed with the working session handle added as an argument) | `draw_shape`, `draw_path`, `set_paint`, `transform`, `add_text`, `pathfinder`, `apply_effect`, `create_graph`, `text_wrap`, `render_preview`, `doc_inspect`, `undo`, `redo`, `command_list`, `command_run`, `command_batch` |
| ArtServer's tools | `session_create`, `session_close`, `document_new`, `import_svg`, `export_document` (for format restrictions and the delivery in section 4.6), `list_fonts` (because VectorCraft's tools lack it) |
| VectorCraft tools not used | Screen operations (`ui_inspect`, `ui_screenshot`, `inspect_ui`, `select_tool`, `pointer_gesture`, `press_key`, `type_text`, `open_panel`, `invoke_menu`), file reading and writing (`open_file`, `save_file`, `screenshot`), `export` (ArtServer's `export_document` is used instead), aliases of the same features (`list_commands`, `run_command`, `inspect_document`) |
| ArtServer tools dropped (because they overlap with VectorCraft's tools) | The five shape tools such as `draw_rectangle` (→`draw_shape`), `set_paint`, `move_objects` (→`transform`), `inspect_document` (→`doc_inspect`), `undo`, `redo` |

- If a tool description contains wording meant for VectorCraft (such as pointers to the desktop app), ArtServer replaces it (the brand policy in chapter 9).

**Allowlist**

- The commands used by VectorCraft's tools (`text.create`, `text.createInPath`, `type.pathOptions`, `shape.*`, `path.create`, `paint.setFill`, `paint.setStroke`, `stroke.set`, `object.move`/`rotate`/`scale`/`reflect`/`shear`, `object.pathfinder.*`, `effect.list`, `effect.apply`, `object.setProps`, `select.set`, `graph.create`, `object.textWrap.make`/`release`, `document.inspect`, `document.node`, `edit.undo`/`redo`, etc.) are added to the allowlist after review.
- The generic tools (`command_list`, `command_run`, `command_batch`) can also use only allowlisted commands. Commands that run other commands inside them (`command.batch`, `action.batch`, `plugin.run`, `script.*`) are not allowed.

Categories for opening commands (initial plan; each command is reviewed when it is opened):

| Category | Targets | Reason |
|---|---|---|
| Not opened | `prefs`, `plugin`, `script`, `help`, `shortcutSet`, `action`, `command`, `print`, `links`, `library`, `view`, `file` (except `file.new`), `color.loadProfile`/`color.removeProfile`, `text.addFontFiles`/`text.findFontFiles`, `edit.colorSettings`/`edit.assignProfile`, saving and loading of presets and libraries (`swatch.library.*`, `graphicStyle.saveLibrary`, etc.) | Changes process-wide state, touches the file system, runs other commands, is meaningful only on screen, or is not exposed for brand reasons (section 6.2) |
| Consolidated into ArtServer's tools | Creating and importing documents, placing files (`file.place`, through `place_upload`), export (`document.export*` etc.; uses the delivery in section 4.6) | ArtServer handles the format restrictions and the passing of large data |
| Opened step by step | The commands used by VectorCraft's tools (above), and those that edit only within the document, such as `object`, `path`, `select`, `edit`, `layer`, `paint`, `stroke`, `transparency`, `appearance`, `gradient`, `pattern`, `swatch`, `graphicStyle`, `brush`, `symbol`, `artboard`, `guide`, `type`, `text`, `shape`, `shapeBuilder`, `livePaint`, `perspective`, `effect`, `graph`, `variable`, `dataset`, `recolor`, `colorTheme`, `dimension`, `attributes`. Anything that falls under "Not opened" above is excluded | Self-contained within the document |
| Decided individually later | `imageTrace` (heavy processing, takes images as input), `clipboard`, `webExport`, `assets`, `svgInteractivity`, `css`, `pdf`, `flattener` | Handles external data, produces output, or is heavy processing |

- Read-only commands (about 160; those marked `query` in the engine) are candidates to open early.
- **First batch (2026-10-19, agreed with the project owner)**: an inventory of the engine's commands, read from their `cmd!` declarations, found 763 (159 read only); of the 558 in "opened step by step", 69 were open and 43 of the rest were read only. The first batch opened 120 more: the read-only ones, and artboards, transparency and opacity masks, gradients (`gradient.selectStop`), appearance, the document's swatches, symbols, clipping masks, compound paths and blends. Left out: `artboard.copy`/`cut` (they need the clipboard commands, decided later), `graphicStyle.libraries`/`library` (the user library folder), `perspective.presets.*` (file paths) and `perspective.widget.options` and `transparency.viewOpacityMask` (screen only). Reviewing them showed that the preferences commands set (`s.prefs`, `s.menu`) belong to the engine's `Session`, so they stay within one working session; `prefs.set` stays closed: applying preferences also sets process-wide state (the user font folders, hyphenation exceptions and rendering threads; `crates/engine/src/cmd/prefscmds.rs`). The allowlist now has 195 commands.
- **Second batch (2026-10-19, agreed with the project owner)**: the rest of "opened step by step", 357 commands, after reading each one's parameters and checking four points in the engine's code: the clipboard of copy and paste (`edit.copy`, `edit.paste*`, `artboard.copy`/`cut`) is a field of the engine's `Session`, so it stays within one working session and never reaches the system clipboard; `perspective.draw` runs only `shape.*` commands (`distortcmds.rs`); `object.rasterize` refuses results over 64 million pixels or 65,535 pixels on a side (`menucmds.rs`); colour themes are kept in the session's preferences. `dataset.import` and `perspective.presets.import`/`export` take an optional file path, which the path check refuses, so they work only with data. Left closed: `text.rescanFonts` (rescans the system fonts for the whole process), `graphicStyle.libraries`/`library`/`loadLibrary`/`addFromLibrary` (the user library folder), and what only changes the screen or the tool (`transparency.viewOpacityMask`, `perspective.widget.options`, `perspective.grid.show`/`rulers`/`lock`/`lockStation`, `object.intertwine.edit`). The allowlist now has 552 commands; left are the 47 commands of namespaces this section doesn't classify and the 48 "decided individually later".
- **Third batch (2026-10-19, agreed with the project owner)**: the namespaces this section didn't classify. Opened 23: `color` computations and checks (convert, gamut check, harmony, plates), `document.activate` (switching between a working session's documents), `document.find`, `document.info`, `document.setup`, units, ruler origins and raster effects settings, the Magic Wand and Eyedropper settings, `image.info`, `node.move`, `shaper.*` and `slice.list`. `document.info` lists each font's file on the server (fontDetails), which is accepted for users within one organization (section 1.1). Left closed: the exports and saves (`document.exportPdf`/`exportEps`/`exportDxf`/`exportForOffice`/`exportForScreens`/`exportForWeb`/`exportSelection`, `document.save`/`serialize`, the export presets and settings, `document.pdfSettings`): exports belong to `export_document`, and without a path these return the file as Base64 in the reply; `document.pdfInfo` and `document.dxfInfo` (a file given as data in the arguments; files come in through uploads); `document.formats` (it lists formats this server doesn't accept); `document.json` (the whole model, which embedded images can make very large; `document.inspect` and `document.node` cover it); `importMatches.*` (for importing other apps' shortcut sets, which stay closed). The allowlist now has 575 commands. What the closed exports offer (PDF/X and other PDF options, printer's marks, bleed, exporting the selection, EPS and DXF) is better added to `export_document`, and `document.pdfInfo` as a tool that takes an upload.
- **Fourth batch (2026-10-19, agreed with the project owner)**: the 48 "decided individually later", after reading each one's parameters and checking three points in the engine's code: Image Trace scales down images over 64 megapixels and caps the anchors it makes (`buildcmds.rs`, the `vectorcraft-trace` crate); its `library` parameter looks up only the built-in swatch libraries and those loaded into the working session, which stays empty while `swatch.library.*` is closed, so nothing is read from disk (`swatchlib.rs`); `css.selection` names the pictures of rasterized art but leaves their encoding to `css.export`. Opened 19: `imageTrace.make`/`makeAndExpand`/`expand`/`release`/`presets` (tracing images placed through uploads; heavy, but within the engine's caps, the time limit and the run limit of section 5.3), the flattener presets (kept in the working session's preferences; `flattener.presets.import`/`export` work only with data, as the path check refuses a path) and `flattener.preview`, `css.selection`/`generate` (the CSS of objects as text), `clipboard.conflicts`/`importText`/`exportText` (text through the working session's own clipboard), and `svgInteractivity.info`/`removeEvent`/`clearEvents` (reading and removing event scripts, such as those an imported SVG brings). Left closed: `clipboard.importPdf`/`importImage`/`importSvg`/`importEps`/`importEmf` (a file given as data in the arguments; files come in through uploads, which don't accept EPS, EMF, TIFF or SVGZ); `clipboard.exportPdf`/`exportPng`/`exportSvg` and `css.export` (an export in the reply; exports belong to `export_document`); `clipboard.flavours` (the formats handed to the system clipboard) and `imageTrace.setView` (the screen only); `svgInteractivity.setEvent`/`addFile`/`removeFile` (they write JavaScript into exported SVG, which runs when a user opens the file in a browser, so an AI led on by a malicious instruction could plant a script; `addFile`/`removeFile` take a file string the path check refuses anyway); `assets.*`, `pdf.preset.*` and `webExport.*` (the setup of exports that stay closed: Asset Export, the detailed PDF options and Save for Web; they are of no use until `export_document` takes those features, and are opened with them). The allowlist now has 594 commands.
- **Export features (2026-10-19, decided with the project owner)**: `export_document` is not extended with what the closed exports offer (PDF/X and other PDF options, printer's marks, bleed, exporting the selection, EPS, DXF, multi-file exports), and `document.pdfInfo` doesn't become a tool, for now. ArtServer is for having an AI make art within an organization (section 1.1); finishing files for print is a designer's work in Illustrator, which the `.ai` and PDF exports already serve. Open commands cover the common needs: `artboard.reorder` to export another artboard as PNG, `artboard.fitToSelection` to export only some art, `document.setup` for the bleed. Every option added to a tool is sent to the AI with its description on every call, which costs context and gives the AI more to choose wrongly from. The closed exports and their setup (`pdf.preset.*`, `assets.*`, `webExport.*`, the export presets) stay closed; what real use shows to be missing is added then, and only that.

**Opening procedure**

1. Read the argument description of the target command, and check that it does not involve file paths, process-wide state, or running other commands
2. Add it to the allowlist (grouped by category, with the reason in a comment)
3. Add tests: every id in the allowlist exists in the engine (so a rename in a VectorCraft update is noticed), no "Not opened" category is in the allowlist, and E2E tests of representative operations for each category
4. Record it in the progress log

**Guidance for the AI**

- The server description (instructions) says: "Use the dedicated tools first; for features they lack, find commands with `command_list` and run them with `command_run`."
- Many engine commands act on "the current selection" when the target is omitted (about 160 assume a selection, and about 200 accept `ids?`). The descriptions tell the AI to specify `ids` explicitly.

**Benefits and caveats**

- Benefits: The names, arguments and descriptions of the tools that VectorCraft crafted for AI can be used as is. ArtServer has less code. VectorCraft's improvements are picked up when the commit is bumped. Most feature additions are just additions to the allowlist. The number of tools stays small, so the AI can choose more easily.
- Caveats:
  - GuardedBackend is the cornerstone of safety. Tests focus on "a command that is not allowed is refused no matter which tool calls it" and "`ui.*` and `app.*` are refused".
  - Bumping the VectorCraft commit can change tool arguments and behavior. Each bump is checked with the tools' E2E tests (chapter 8).
  - The current path check (refusing based only on argument names) wrongly refuses `add_text`'s type on a path (`path` in `text.createInPath` is a shape id). Fix it to also look at the value's type (section 13.2).
  - Tool descriptions are VectorCraft's text, and argument descriptions are not JSON Schema. Wrong arguments are refused by the engine's checks and returned as tool errors.

---

## 5. Concurrency policy

### 5.1 Two kinds of concurrent requests

| Kind | Handling | Reason |
|---|---|---|
| **Arriving at the same time from different clients** | **Processed in parallel.** Each session has a dedicated thread | Each session handles only its own documents, so they do not need to wait for each other. Confirmed that the engine Session can move between threads (Send) |
| **Arriving at the same time for the same working session (same handle)** | **Processed one at a time, in arrival order.** The queue has a limit (`--queue-depth`, default 8; a setting since 2026-10-19), and on overflow a "busy" error is returned | Rewriting one document concurrently corrupts it. Order matters in editing ("create → fill" fails if reversed) |

- HTTP requests are accepted with async processing, and heavy work runs on the session's thread. Health checks etc. can be answered even during rendering.
- Cancellation is accepted even during execution (using the engine's cooperative cancellation). With Streamable HTTP, cancellation happens when the client closes the response stream.
- **Time limit (2026-10-19, agreed with the project owner)**: the time limit (`--call-timeout-secs`, default 60) counts from when a request starts running. Waiting to start, in the queue or for a free slot (section 5.3), has a limit of the same length: a request that can't start within it is not run, and the AI is told that nothing was changed and to retry later. Before, one limit counted from the call, so a request could run out of time before it started, or start with seconds left and be stopped partway, and the same message hid whether it had run. Whether a request started or its caller stopped waiting is settled once, by whichever comes first, so the two can't be confused. A call can so take up to twice the time limit before it answers; a separate setting for the wait was not added, to keep the settings few.

### 5.2 Differences from VectorCraft

- VectorCraft's existing MCP is designed as **"1 client = 1 process"**. Since it uses stdio, a separate process starts for each client, and clients are naturally independent through process isolation.
- Within one process, it handles only overlapping requests from one client (queue, execution in arrival order, busy response, cancellation).
- Since ArtServer handles **multiple clients in one process**, it newly needs measures against "process-wide shared state", which was not a problem in VectorCraft.

### 5.3 Measures for shared state and resources

| Problem | Measure |
|---|---|
| Settings shared across the whole process (color settings, ICC profiles, font DB, plugin registration, hyphenation exceptions, etc.) affect everyone if one session changes them | Do not allow commands that change them |
| Each export creates up to 8 rendering threads, so concurrent exports make the thread count explode | Limit the number of rendering threads (`--render-threads`, default 2), and set a global limit on concurrent heavy operations: **implemented 2026-10-19** as `--max-running` (`ARTSERVER_MAX_RUNNING`), the jobs the engine runs at once across all working sessions, light ones included; others wait for a free slot, up to the time limit (section 5.1). The default is the CPU cores ÷ (rendering threads + 1), at least 1 |
| Undo history alone uses up to 1GiB per document | Set limits on the number of sessions, the number of documents, and history bytes |
| The first font lookup scans all fonts, and everyone waits during it | Scan in advance at startup |
| The glyph cache shares a single lock and may become contended | Check in the Phase 3 load tests |

---

## 6. Security policy

### 6.1 Implemented in the MVP

- Listen **on 127.0.0.1 only**. External exposure is disabled by default.
- **Bearer token authentication.** The server does not start if no token is set. Comparison is constant-time.
- **Origin / Host header checks** (against DNS rebinding). Origin validation is a MUST in the MCP spec, and an invalid Origin gets 403. Enabled through rmcp settings (section 4.4). The Origin allowlist is empty by default, and every request with an Origin header (requests from browsers) is rejected.
- **Handle verification.** Use by any principal other than the one it was issued to is rejected.
- Body size limit, session count limit, disposal of idle sessions, call timeout (cooperative cancellation).
- **Command allowlist** (deny by default). The contents of nested execution such as `command.batch` are also checked.
- Unknown-key checks on arguments (the engine does not do this in release builds, so ArtServer always calls it).

### 6.2 Commands not allowed (examples)

- Those that change process-wide state: `edit.colorSettings`, `edit.assignProfile`, `color.loadProfile`, `plugin.*`, `prefs.set`, `text.rescanFonts`, `text.addFontFiles`
- Those that broadly search or reference the file system: `links.findFiles`, `text.findFontFiles`, `file.list`, `script.check`, `file.recovery.*`
- Those not exposed for brand reasons: `help.*` (links to the ArtCraft site and Discord)
- No generic tool that can run arbitrary commands is exposed. In Phase 2, VectorCraft's `command_list`, `command_run` and `command_batch` are placed behind GuardedBackend, so that only allowlisted commands can be found and run (section 4.7).
- Banning "those that change process-wide state" is a constraint that comes from running multiple sessions in one process. After process isolation, some things such as color settings can be allowed per session (section 12.5).

### 6.3 File access

- **The MVP does not use the file system.** Both input and output are passed as Base64 (the engine's export returns Base64 when the path is omitted). In Phase 2, large data moves to the approach in section 4.6 (exports since 2026-10-18, uploads since 2026-10-19).
- If path specification is opened in the future:
  - Apply the engine's file restrictions with a per-session directory as the read/write root.
  - Always build absolute paths on the ArtServer side, because the engine resolves relative paths against the server's current directory.
  - Do not use the process-wide restriction (`confine_process`), because it can be set only once and cannot be split per session.
- For known weaknesses of the engine's file restrictions (TOCTOU, the existence of code paths that bypass the restrictions), see section 2.4 of `docs/architecture.md`.

### 6.4 Logging

- Do not log access tokens, document contents, or absolute host paths.
- Use ArtServer's own logging. VectorCraft's existing MCP logging mechanism is shared across the whole process and leaks to other sessions, so it is not used.
- MCP's Logging feature was deprecated in the spec, so it is not implemented. Logs go to standard error.

### 6.5 What cannot be prevented within the process (stated as known limitations)

1. Forcibly stopping work that does not respond to cooperative cancellation
2. Whole-process crashes caused by stack overflow or out-of-memory
3. Memory shared between tenants through shared caches

The MVP assumes "a small number of trusted clients using it locally", and the README states this premise and the limitations above. It is not exposed on the internet for the general public.

Before exposing it to untrusted users, engine execution will be isolated per OS process (chapter 12). Phases 1 and 2 do not build the isolation mechanism first; they prepare for future replacement by keeping the boundary in section 4.3.

---

## 7. Font policy

### 7.1 Why the server needs fonts

The server is not the side that **opens** output files but the side that **creates** them. Text layout and export happen on the server, so font data is needed.

| Output | Why fonts are needed |
|---|---|
| PNG | Text is drawn as pixels |
| PDF | The shapes of the glyphs used are embedded in the PDF |
| SVG (converted to outlines) | Text is converted to shape paths |
| SVG (kept as `<text>`) | The server computes line break positions and per-character coordinates and writes them. Vertical type and type on a path are always converted to outlines |

Fonts are also needed outside export, in editing that uses text dimensions, such as centering text and wrapping within an area. Without fonts, text comes out with missing characters (with a warning), and its placement is off.

### 7.2 Status by environment

| Content | macOS server | Linux server |
|---|---|---|
| Shapes only | Not needed | Not needed |
| Latin letters and digits | Not needed (bundled fonts) | Not needed (bundled fonts) |
| Japanese and other CJK text | Basically not needed (OS-standard fonts such as Hiragino are used automatically) | **Must be provided** |

- The bundled fonts cover Latin script only (Source Sans 3 / Source Serif 4 / Inter / JetBrains Mono, OFL-1.1).

### 7.3 How to handle CJK text (such as Japanese) on Linux (not done for now)

1. Install fonts on the server (e.g. Noto Sans CJK JP)
2. Embed them at build time (`CRAFT_FONTS_DIR`; BIZ UDPGothic / Shippori Mincho / BIZ UDMincho)
3. ArtServer loads the fonts in a specified folder at startup

If method 2 is chosen, the embedded fonts become fonts that ArtServer distributes, and ArtServer bears the responsibility for their license notices (section 1.1). With methods 1 and 3, providing the fonts is the operator's job.

**Not done for now (2026-10-19, decided with the project owner)**: Linux is expected to have few users (section 1.1). Methods 1 and 3 already work (the fonts installed in the server's OS, and `--font-dirs`), so an operator can provide CJK fonts without more work; method 2 is not done. Revisited when users on Linux appear.

### 7.4 Notes

- The operator checks the licenses of the fonts installed on the server (section 1.1). As a reference for operators: commercial fonts (such as Morisawa) often need a separate license for server use or PDF embedding.
- There is one font DB per process. The available fonts cannot be split per session.
- If a font the AI specifies is not on the server, a fallback font is used. Make it possible to know and expose which fonts are installed on the server.
- **The bundled fonts (2026-10-19, agreed with the project owner)**: ArtServer has VectorCraft's bundled Latin fonts (Source Sans 3, Source Serif 4, Inter, JetBrains Mono) built into the program, and the default for text without a font is Source Sans 3. Files using them look right anywhere (the glyphs are embedded), but people editing the text in another app such as Illustrator usually lack the font. So `list_fonts` marks as `bundled` the built-in families that the server's OS doesn't have installed (worked out from the font files: a built-in family that is also installed is not marked), and the server description tells the AI to use installed families for files people will edit and to name a font for every text. Changing the default font (it would differ per OS) and leaving the bundled fonts out of the build (text could not be drawn on servers without fonts) were not chosen.

---

## 8. Dependencies and tracking upstream

- `vectorcraft-engine` is referenced as a **git dependency pinned to a commit**. It does not track a branch.
  - Reason: VectorCraft bumps its version almost daily and makes no stability promise for its public API.
  - VectorCraft's crates are not published on crates.io.
- `vectorcraft-mcp` is also referenced at the same commit, and only its tool part (`call_tool`, `Backend`) is used (section 4.7).
- The following VectorCraft settings are not inherited by dependents, so they are copied into ArtServer's root `Cargo.toml`:
  - `[patch.crates-io]` (replaces the PDF backend krilla / hayro-* with the fixed versions from craft-libs)
  - Profile settings that build dependency crates with optimization
- `Cargo.lock` starts from VectorCraft's.
- When updating VectorCraft, sync the patches and the lockfile, and get the tests passing before updating. A test checks that the PDF backend is the fixed version (`tests/pdf_backend.rs`): in `Cargo.lock`, krilla and hayro-syntax / hayro-interpret / hayro-jbig2 each resolve once, from the craft-libs revision craft-pdf comes from. If `[patch.crates-io]` stops applying, Cargo only warns and takes the crates.io versions, which lack the limits on broken PDFs; craft-pdf refuses to build with an unfixed krilla, but nothing else stops an unfixed reader. CI only builds (with `--locked`), so the test runs when the tests are run locally. Tests also check changes in the arguments and behavior of VectorCraft's tools, and that every id in the allowlist exists.
- To try local modifications to VectorCraft, switch to a path dependency with local settings that are not committed.
- **When fixing a VectorCraft bug in ArtServer** (agreed with the project owner on 2026-10-18): copy only the affected crate into `vendor/`, fix it there, and replace the original with `[patch."https://github.com/storytold/vectorcraft"]` in `Cargo.toml`.
  - Copy from the same commit that ArtServer references. VectorCraft's crates call each other's internal functions, so the build fails if the commits differ.
  - Keep the content of the fix, the diff from the original code, and the steps for reapplying the patch in `vendor/README.md` and `vendor/patches/`. When updating VectorCraft, replace the copy and reapply the diff.
  - This is separate from Option C in section 3.1 (copying all of VectorCraft). Only the crates with bugs are covered, and the diffs are kept small.
  - Also propose the fix upstream, and remove the replacement once it is merged.
  - If the number of fixed crates grows, or fixes keep coming and reapplying the patch becomes hard, switch to a fork of VectorCraft.
  - Current target: `vectorcraft-svg` (the weights W0–W9 of Japanese fonts do not become SVG `font-weight`; upstream #2873).
- Referenced commit: v0.13.1 (`904f0fe5227bd987d55319e89a38b4b8f046998c`, updated 2026-10-18). As a rule, it follows release commits.

---

## 9. License and brand policy

- ArtServer's own code is **MIT**. The licenses of parts taken from VectorCraft and elsewhere are not changed.
- There is no GPL / LGPL / MPL / proprietary code among the dependencies (result of static analysis; to be confirmed with `cargo deny` / `cargo about`).
- Notices bundled with binary distributions:
  - MIT notices for VectorCraft, craft-libs and the Hayro Authors
  - BSD-3 notices for Adobe CMap and the Foxit/PDFium fonts
  - The full OFL-1.1 text for the bundled fonts
  - The IJG wording for jpeg-encoder, and Unicode-3.0 and BSD/Zlib-style notices
  - These are collected in `THIRD_PARTY_NOTICES.md`, generated by a tool
- Brand:
  - **Do not use the ArtCraft name or logo** (ArtCraft is not open source).
  - The README states the origin in text as "based on VectorCraft by the ArtCraft team", so that it is not mistaken for an official derivative.
  - When mentioning compatibility with Illustrator, include a note on Adobe's trademarks.
  - Check for trademark conflicts with the name "ArtServer" before publication (done on 2026-10-19; section 13.2, #7).

---

## 10. MVP (Phase 1)

### 10.1 Goal

An AI (Claude Code etc.) connects to ArtServer over local HTTP, and **"create a rectangle → export to SVG" actually succeeds**. PDF and PNG export are included.

### 10.2 Scope

| Area | Details |
|---|---|
| Startup | Single `artserver` binary. 127.0.0.1 by default. Does not start if no token is set |
| Endpoints | `POST /mcp` (Streamable HTTP; both the 2026-07-28 version and earlier versions), `GET /healthz` |
| Tools | Create and discard working sessions, create documents, add shapes, set fill and stroke, move and transform, get document state, SVG/PDF/PNG export (Base64), Undo/Redo |
| Security | Everything in section 6.1 |
| Tests | E2E of create rectangle → SVG export. Error cases (invalid input, authentication failure, invalid Origin, disallowed command, another principal's handle, size exceeded, timeout, disallowed path) |
| Documentation | README (how to start, connecting from Claude Code, health check with curl), LICENSE (MIT), THIRD_PARTY_NOTICES, `docs/progress.md` |

### 10.3 Not included in the MVP (stated explicitly)

TLS (terminated at a reverse proxy), multiple users and permission management, process isolation, rate limiting, I/O by file path, bundling CJK fonts (such as Japanese).

---

## 11. Phase plan

| Phase | Contents | Status |
|---|---|---|
| 0 | Investigation of VectorCraft, choice of approach | Done (Option A-2 adopted) |
| 1 | MVP (chapter 10) | Done (implemented 2026-10-17; connection from Claude Code verified 2026-10-18; `cargo deny` / `cargo about` introduced). Items not yet verified are in section 13.2 |
| 2 | Feature expansion and compatibility verification (CJK fonts such as Japanese, images, layers, transparency, path operations, verification with real files). Process isolation is not built first (revisit if a condition in section 12.4 applies) | In progress (since 2026-10-18). Done: VectorCraft's tools through GuardedBackend (section 4.7), text and fonts (`list_fonts`, `--font-dirs`), the fix for the weights of Japanese fonts in SVG (chapter 8), URL delivery of exports (section 4.6), upload for import (opening PDF, .ai, SVG, PNG, JPEG and .vectorcraft, and placing images; section 4.6), `.ai` and `.vectorcraft` export (section 4.6), opening the commands (section 4.7; four batches, 594 commands; `export_document` is not extended for now). Remaining: verification with real files. Not done for now: CJK fonts on Linux (section 7.3) |
| 3 | Productization (REST API, multiple users, process isolation (chapter 12), job persistence, monitoring, threat analysis and load tests). Not supported: Docker (decided by the project owner on 2026-10-19); no Dockerfile or container image is provided | Not started |

---

## 12. Future process isolation (Phase 3)

Not implemented in Phases 1 and 2. This chapter summarizes the policy and what to consider when starting the work.

### 12.1 Why isolate

| Aspect | Thread isolation (Phase 1) | Process isolation (future) |
|---|---|---|
| Working state such as documents and Undo | Can be isolated | Can be isolated |
| Settings and caches shared across the whole process | Shared | Independent per worker |
| Stack overflow and out-of-memory | The whole server stops | Only that worker is affected |
| Work that does not respond to cooperative cancellation | Cannot be forcibly stopped | Can be stopped by terminating the worker |
| Resource control and privilege restriction by the OS | Hard to apply | Easy to apply per worker |
| Startup, memory and operating cost | Low | High |

- **Separating processes alone does not make it safe.** Design per-worker OS privileges, working directories, file access, CPU and memory limits, and network restrictions together.
- Isolation by OS process and Docker containers are different things. ArtServer does not support Docker (2026-10-19, chapter 11), and process isolation is designed without it.

### 12.2 Basic structure

```text
             MCP client
                  │
                  ▼
        ArtServer (front end)
HTTP / MCP / auth / session management
                  │
          ┌───────┼───────┐
          ▼       ▼       ▼
        Worker  Worker  Worker
          │       │       │
        Engine  Engine  Engine
```

- Only the workers load the engine. The front end (the ArtServer main process) handles authentication, session routing, request limits and responses.
- If a worker terminates abnormally, that session is treated as failed, and the front end keeps running.

### 12.3 Isolation unit

| Unit | Pros | Cons |
|---|---|---|
| One process per session | Easy to understand; the scope of a failure is clear | The startup cost (especially font initialization) is paid for every session |
| One process per user | Lower startup cost | Sessions of the same user share failures and shared state |
| Worker pool | Lower startup cost | Hard to reset state before reuse and to prevent leftover data. Sessions with different trust boundaries must not be mixed in the same process |

One process per session is the first candidate, but it is not fixed as a permanent spec. Choose after measuring the number of users, concurrency, document size, font initialization cost and memory usage.

### 12.4 Conditions for starting

Start as soon as any of the following becomes a real requirement.

- External users who do not trust each other use the same ArtServer.
- Abnormal engine termination or long-running work must not affect other users.
- The number of concurrent sessions or the load grows, and resources need to be controlled per user or per session.
- Features where isolation matters a lot are exposed, such as I/O by file path or fonts brought in by users.
- **Import of files from users who are not trusted is exposed.** Import is the part most likely to crash the process. Import has been open since 2026-10-19 for the users section 1.1 assumes (one organization, trusted), with limits on size, time and formats (section 4.6). It must not be offered to users who are not trusted before process isolation.
- **A feature that switches color settings per job becomes necessary** (section 12.5). In this case, revisit without waiting for Phase 3.

When migrating, test with emphasis on "the front end keeps running when a worker crashes", "a session cannot touch the state or files of other sessions", and "a worker can be stopped when it exceeds its limits".

### 12.5 Known caveats from VectorCraft's code

- **Isolation may be needed for features**: Color settings (working color space, ICC profiles) are shared across the whole process, so the commands that change them are currently banned (section 6.2). If print use cases need to switch CMYK profiles per job, separating processes would allow this per session. Isolation can be needed not only for safety but also for features.
- **Memory limits are decided per process, based on the whole machine's memory**: The limits for the image cache and Undo history are computed on Linux as 1/8 of the installed memory (MemTotal in `/proc/meminfo`), and do not take memory limits of containers etc. into account (VectorCraft `crates/render/src/images.rs:33-45`, `crates/engine/src/lib.rs:196-214`). With multiple processes, each process decides that it "may use up to 1/8 of all memory", so the limits must be set explicitly.
- **Font initialization cost**: The first font lookup scans every font folder. With one process per session, this wait happens every time a session starts.
- **Risks of import**: Stack overflow and out-of-memory cannot be prevented by panic guards. SVG / PDF / EPS import has measures such as nesting depth limits, but the chance of the whole process crashing is not zero.

### 12.6 What to design when starting

1. Starting and stopping workers, idle reclamation, detecting abnormal termination, restart and session invalidation
2. Inter-process communication (compare candidates such as JSON), request IDs, ordering guarantees, timeouts, cancellation, error reporting
3. Per-session data storage location and access permissions, per-process resource limits
4. Per-process initialization cost of fonts, settings and shared resources, and explicit memory limit settings
5. Policy for persisting and recovering document state (whether to promise resumption after abnormal termination)
6. Monitoring, logs, metrics, backpressure, limits on concurrent processing
7. OS-level isolation in production environments such as Linux (Docker is not supported; chapter 11)

---

## 13. Pending decisions and open issues

### 13.1 Items needing the project owner's decision

| # | Details |
|---|---|
| 1 | Is the policy in this document (especially adopting Option A-2) acceptable? → **Approved (2026-10-17)** |
| 2 | Whether to install Rust on the development machine (this Mac) → **Installed (2026-10-17)**. By the project owner's choice, Homebrew's `rust` (1.99.1) is used instead of rustup |
| 3 | Priority of Windows support (section 1.1). Expected to be needed by organizations whose PCs run Windows. The build is verified on GitHub Actions (2026-10-18). Verifying behavior on a real machine needs a Windows PC (a virtual machine on an Apple Silicon Mac would be the ARM version) |

### 13.2 Items to verify or decide during implementation

| # | Details | When |
|---|---|---|
| 1 | How to implement the MCP protocol layer → **Decided: rmcp 3.5 series** (section 4.4). Header validation and cancellation verified by E2E tests. Compatibility with older versions (clients that use `initialize` and `Mcp-Session-Id`) → **Verified by an E2E test (2026-10-19)** for 2025-06-18 and 2025-11-25 (section 4.4). The 2026-07-28 version is **not covered by a test**: rmcp's client in the E2E tests settles on 2025-11-25 | Phase 1 |
| 2 | HTTP framework and async runtime → **Decided: axum + tokio** (section 4.4) | Phase 1 |
| 10 | Which protocol version real clients such as Claude Code connect with → Connection and operation from Claude Code verified (2026-10-18). The protocol version used is **not yet verified** | During Phase 1 E2E verification |
| 3 | Whether ArtServer's session type and renderer can move between threads (checked by the compiler) → **Verified** (the build passed) | At the Phase 1 build |
| 4 | Finalizing third-party licenses (`cargo deny` / `cargo about`) → **Done (2026-10-18)**. `THIRD_PARTY_NOTICES.md` is generated from `deny.toml`, `about.toml` and `about.hbs`. No GPL / LGPL / MPL | Phase 1 |
| 5 | How to provide CJK fonts (such as Japanese) (section 7.3) → **Not done for now (2026-10-19)**: the fonts installed in the OS and `--font-dirs` already serve; embedding fonts in the build is not done | When users on Linux appear |
| 6 | Handling of "VectorCraft" written into output files (PDF Producer etc.) → **Decided (2026-10-19): left as is**. The PDF Creator and Producer stay "VectorCraft", and PDFs keep the embedded editing data (`vectorcraft-editing.vectorcraft`) | Phase 2 |
| 7 | Checking trademark conflicts for "ArtServer" → **Done (2026-10-19): no conflict found**. US (USPTO): only dead marks (ARTSERVER, abandoned 1997; ARTSERVE, cancelled). Japan (J-PlatPat): no ARTSERVER or アートサーバー; the closest, AUTO SERVER (class 9, including software), is not similar (different first sound and meaning). EU, WIPO and other countries were not checked. Registering "ArtServer" as a trademark would need a patent attorney's review | Before publication |
| 8 | Whether color settings need to be switched per job (if so, consider moving process isolation earlier; section 12.4) → **Not for now (2026-10-19, the project owner's decision)**: every working session keeps the engine's default settings (RGB: sRGB; CMYK: VectorCraft Generic CMYK (SWOP-like), a model of the engine's own, not an Adobe or ECI profile). Japan Color and other ICC profiles can't be loaded, since `color.loadProfile` stays closed (section 6.2) | When print work needs CMYK conversion per printing condition on the server |
| 9 | Isolation unit and method (section 12.3) | When Phase 3 starts |
| 11 | Passing large data (section 4.6) → **Export implemented (2026-10-18)**. Thresholds are SVG 32KB and PNG 256KB, and PDF is always a URL. Retrieval uses expiring URLs that need no token (that Claude Code's AI can fetch files with curl was verified with the token-required version). Download in a browser is also verified. Upload for import **implemented (2026-10-19)**: `upload_create` → PUT → `open_upload` / `place_upload`. **Not implemented**: returning a resource link | Phase 2 |
| 12 | Tool structure (section 4.7) → **Implemented (2026-10-18)**. The path check now also looks at the value's type. `call_tool` results are converted to MCP results without structuredContent. The descriptions of the 16 exposed tools contain no "VectorCraft" wording, so no replacement was needed (verified by tests). **Not yet verified**: confirming the command count by running the engine (about 760 is a static count), and whether Claude Code's AI uses VectorCraft's tools and the generic tools correctly | Phase 2 |
