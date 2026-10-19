# ArtServer Architecture Investigation and Approach Decision (Phase 0)

- Created: 2026-10-17
- Status: **Option A-2 adopted (approved by the project owner on 2026-10-17)**. This document is the investigation record as of Phase 0. From here on, `docs/design-policy.md` is authoritative for the design policy (session handling and other points changed to follow the latest MCP specification). The findings are kept as they were; what was verified or decided later is added after "→" with its date (updated 2026-10-19).
- Subject: VectorCraft `~/MyTasks/RoundShape/vectorcraft`
  - origin: https://github.com/storytold/vectorcraft.git
  - commit: `fdac986bb49b3e3a9ddd3cda4e88661a6f0aa50c` (2026-10-17, v0.12.0 + 335 commits). The `path:line` references below are to this commit → ArtServer references v0.13.1 (`904f0fe`) since 2026-10-18 (design policy chapter 8)
- Method: static reading of the source only. No build or test was run (Rust is not installed on the development machine, as noted below).
- Reliability order: code → comments → function names. Each `path:line` below is a location in the VectorCraft repository.

---

## 1. Conclusion (Summary)

**Adopt Option A (use the VectorCraft crates as libraries), and within it, recommend the form "embed `vectorcraft-engine` directly and design the MCP layer on the ArtServer side" (Option A-2).**

- The engine layers (L0–L5) do not depend on the GUI, and rendering is CPU-only (`vello_cpu`). There is no structural obstacle to embedding it in a server.
- Every engine feature can be called with "command ID + JSON arguments" (`Session::execute`). Mapping it to MCP tools is easy.
- The existing `vectorcraft-mcp` is built for **stdio, a single user, and trusted local use**, and its assumptions do not fit network exposure or multiple sessions. Treat it as a reference implementation and do not depend on it. → Revised on 2026-10-18: its server and transport parts are not used, but ArtServer uses its tool part (`call_tool`, `Backend`) through a Backend of its own (design policy section 4.7).
- On the other hand, **the engine has mutable state shared across the whole process** (color settings, font DB, plugin registration, etc.). The MVP handles this by "not allowing commands that change that state", and keeps a structure that can move to process isolation in the future.

---

## 2. Findings

### 2.1 Crate structure and GUI dependencies

VectorCraft itself defines a layer structure and checks it with `cargo xtask layers` (`docs/architecture.md:15-31`, `xtask/src/layers.rs`).

| Layer | crate | Use in ArtServer |
|---|---|---|
| L0 | `geom`, `color`, `affinity` | Indirect dependency |
| L1 | `doc` (document model) | Indirect dependency |
| L2 | `pathops`, `brush`, `trace`, `text`, `effects`, `plugins`, `three-d` | Indirect dependency |
| L3 | `render`, `svg`, `pdf`, `format`, `eps`, `cad`, `metafile` | Indirect dependency |
| L4 | `tools` | Indirect dependency |
| L5 | **`engine`** (session, history, selection, about 700 commands, file I/O) | **Direct dependency** |
| L6 | `ui-egui` (GUI), `mcp` (stdio MCP) | Not used (`mcp` is a reference implementation) → `mcp`'s tool part is used since 2026-10-18 (design policy section 4.7) |
| L7 | `apps/vectorcraft` (GUI), `apps/vectorcraft-cli`, `apps/vectorcraft-web` | Not used |

- **Layers below L6 must not depend on UI crates (egui/eframe/winit/rfd/craft-ui)** (`AGENTS.md:45`).
- **L0–L5 must build for wasm32** (`AGENTS.md:47`). Because of this, there are almost no OS-native dependencies.
- The dependency graphs of `engine` / `mcp` / `vectorcraft-cli` contain no GUI crates (checked in each `Cargo.toml`).
- Size: about 550,000 lines in total. `engine` about 145,000 lines, `ui-egui` about 118,000 lines.

### 2.2 Engine API (headless use)

- The entry point is `Session::execute(&mut self, id: &str, params: &Value) -> Result<Value>` (`crates/engine/src/lib.rs:1521`). It has a panic guard.
- A `Session` can hold multiple documents (`docs: Vec<DocState>`, `lib.rs:1194-1306`). Each document has its own selection state and Undo history.
- The whole flow the MVP needs can be done with commands alone, **without using the file system**:

```text
file.new          {width, height, ...}                → {index, previewMode}   cmd/edit.rs:17
shape.rectangle   {x, y, width, height, radius?}      → {id}                   cmd/create.rs:13
paint.setFill     {color, ids}                                                 cmd/paint.rs:15
document.export   {format: "svg"|"pdf"|"png", ...}    → {dataBase64, ...}      cmd/fileio/mod.rs:121
                  (if path is omitted, it returns Base64 instead of writing a file)
```

- Note: the unknown-key check for arguments (`check_params`) runs inside `Session::execute` only under `#[cfg(test)]` (`lib.rs:1523-1526`). The embedding side must call it explicitly (`vectorcraft-mcp` also calls it in `crates/mcp/src/headless.rs:128-134`).
- The argument spec (`params`) is a description string, not a JSON Schema. The input schemas for MCP tools must be defined on the ArtServer side.

### 2.3 Existing MCP (`crates/mcp`)

| Item | Details | Evidence |
|---|---|---|
| Transport | **stdio only**. HTTP/SSE/Streamable HTTP are not implemented | `crates/mcp/Cargo.toml:6`, `docs/mcp.md:275` |
| JSON-RPC | Hand-written implementation (`serde_json::Value`). Does not use an MCP SDK | `crates/mcp/src/server.rs` |
| Supported protocols | 2026-07-28 / 2025-06-18 (default) / 2025-03-26 / 2024-11-05 | `server.rs:21-23` |
| Separation of transport and processing | Already separated by `Server::handle(&mut self, msg: Value) -> Option<Value>` | `server.rs:362` |
| Send for `Server` | **Not Send** (`Box<dyn Backend>` has no Send bound) | `server.rs:90-100`, `backend.rs:11-18` |
| Number of tools | 32. Of these, `command_run` / `command_batch` etc. **can run every engine command (about 700)** | `crates/mcp/src/tools.rs:113-495` |
| Two modes | Headless (holds a Session in the process) / Remote (TCP connection to a running GUI app, **no authentication**) | `headless.rs`, `backend.rs:20-67`, `docs/control-protocol.md` |
| Logging | The log level and queue are shared across the whole process. With multiple sessions, logs leak to other sessions | `crates/mcp/src/logging.rs:22-28` |
| Gaps | Through `handle()`, rejecting calls before initialization, size limits, cancellation and progress notifications do not work (they exist only inside `serve()`) | `server.rs:269-328, 403-459`, `intake.rs` |
| Name | `serverInfo.name` is `"vectorcraft"`, and the tool descriptions also assume VectorCraft | `server.rs:421` |

- Shell execution, launching external processes and fetching URLs **do not exist in the engine** (confirmed with grep).
- Plugins run in a WebAssembly sandbox using wasmi (with limits such as 512MiB of memory and 60 seconds, `crates/plugins/src/runtime.rs:37-48`). However, registration is shared across the whole process.

### 2.4 File access restrictions (`crates/engine/src/file_access.rs`)

- `AutomationRoots { read, write }`. The given roots are normalized and must be existing directories.
- **If no root is given, access is unrestricted** (`file_access.rs:16-20`).
- What is checked: make the path absolute → resolve symbolic links → compare **per path component** whether it is under a root (`/work2` is not treated as under `/work`). `..` below a folder that does not exist, and links with no target, are rejected.
- **The scope is per thread** (`thread_local!`, `file_access.rs:119-122`), or once for the whole process (`confine_process`, `file_access.rs:125-131`).
  - `docs/mcp.md:101` says "effective on all threads of the server process", but this is only when the CLI calls `confine_process`. When used as a library, it is per thread.
- Known weaknesses:
  - **TOCTOU** (if a link is swapped between the check and the open, the restriction is bypassed). A comment in the code admits this (`file_access.rs:22-24`).
  - Relative paths are resolved **against the current directory of the server process**.
  - Error messages contain absolute host paths.
  - Some paths bypass the restriction: app folders (swatch libraries, the Fonts folder), data recovery (`recovery.rs`), creating the template folder (`save.rs:130-137`), getting modification times of linked images (`link_watch.rs`), and calls from threads outside the engine.
- Input limits: no limit on file import, the PDF reader's `max_input_bytes` is `usize::MAX`, Undo history is **up to 1GiB per document**, and in headless mode there is **no execution time limit per call** (cooperative cancellation only).

### 2.5 Concurrency and session isolation

- `Session` is **Send** (`crates/engine/src/tests_large_edits.rs:66-75` moves it between threads). It is probably not Sync.
- `Document` is Send + Sync (`Arc<Document>` is passed to worker threads).
- All execution is synchronous CPU work. An async server must run it on blocking threads.
- Cancellation (`cancel::scope`) and file restrictions (`file_access::confine`) are thread-local. The `ProgressSink` for progress notifications is an `Rc` (`crates/doc/src/cancel.rs:51`). **They must be created and applied inside the execution thread**.
- **Mutable state shared across the whole process** (leaks between sessions):

| State | Location | Commands that change it |
|---|---|---|
| Color management settings | `crates/color/src/cms/mod.rs:890-924` | `edit.colorSettings`, `edit.assignProfile` |
| ICC profile registration | `cms/mod.rs:167` | `color.loadProfile`, when opening a native-format file |
| Font DB (a single `FontDb::global()`) | `crates/text/src/fontdb.rs:1545-1548` | `text.rescanFonts`, font folders in settings |
| Hyphenation exceptions | `crates/text/src/hyphen.rs:34-60` | `prefs.set` |
| Default render thread count | `crates/render/src/lib.rs:2193-2220` | Reset on every `prefs.set` |
| WebAssembly plugin registration | `crates/plugins/src/registry.rs:14` | `plugin.install` etc. |
| Process-wide file restriction roots | `file_access.rs:124-131` | `confine_process` (once only) |

- Content-keyed shared caches (1GiB for images, 512MiB for placed documents, etc.) do not mix data, but memory is shared between tenants.
- Each export creates a new `Renderer`, and each one uses up to 8 threads. This can be limited with `VECTORCRAFT_RENDER_THREADS`.
- Stack overflow and out-of-memory bring down the whole process (the panic guard cannot prevent this).

### 2.6 Fonts

- The bundled fonts are **Latin only** (Source Sans 3 / Source Serif 4 / Inter / JetBrains Mono, OFL-1.1, about 3.2MiB).
- **CJK fonts (such as Japanese) are not bundled**. Setting `CRAFT_FONTS_DIR` at build time lets you embed BIZ UDPGothic / Shippori Mincho / BIZ UDMincho etc. (`docs/development.md:115-152`). Otherwise it relies on fonts installed on the server.
- On Linux, it scans `/usr/share/fonts`, XDG and fontconfig settings in pure Rust (no libfontconfig needed).
- There is one font DB per process. Fonts cannot be isolated per session.
- Depending on `vectorcraft-mcp` forces the bundled fonts on through Cargo feature unification (`crates/mcp/Cargo.toml:11`, `crates/engine/Cargo.toml:11-15`). This contradicts `docs/development.md:164`.

### 2.7 Build and environment

- Requires: **Rust 1.95 or later, edition 2024** (`Cargo.toml:8-10`). There is no `rust-toolchain` file.
- **Rust is not installed on the development machine (this Mac)** (no `cargo` / `rustc` / `~/.rustup`). → Installed on 2026-10-17: Homebrew's `rust` (1.99.1), not rustup (design policy section 13.1 #2).
- The dependencies of the non-GUI parts are pure Rust. For system libraries, the linker alone (Xcode Command Line Tools on macOS) should be enough.
- Building needs network access to crates.io and `github.com/storytold/craft-libs` (git dependency).
- Disk: about 30GB per target for the whole workspace (`AGENTS.md:58`). ArtServer uses only the engine, so a few GB is expected (not yet verified). → Measured on 2026-10-19: ArtServer's `target/` is 11 GB (9.2 GB of it in `deps`, 1.9 GB `incremental`). It holds only debug builds, made over and over for two days with tests and clippy; a clean build and a release build were not measured.
- The repository has no general build/test CI for Linux/macOS. The quality gate is the local `cargo xtask ci`.

### 2.8 Following upstream

- The VectorCraft crates are **not published on crates.io** (they cannot be published because of the git dependency craft-pdf and `include_bytes!` that refers outside the crate).
- The version has gone up almost every day, from 0.1.1 (2026-10-03) → 0.12.0 (2026-10-16), with 200–560 commits per day. **There is no promise of public API stability or semver**.
- **Not inherited** by dependents:
  - `[patch.crates-io]` (`Cargo.toml:126-130`, replaces krilla / hayro-* with craft-libs)
  - `[profile.*]` (including the setting that builds dependency crates at opt-level 2)
  - The workspace lint settings, `Cargo.lock`
- Without copying `[patch.crates-io]`, the PDF backend becomes the crates.io version, and the panic protections and decode limits that VectorCraft adds are lost. The build itself may also fail (not yet verified). → Not tried: ArtServer copies the patch, and `Cargo.lock` was confirmed to use the craft-libs versions (progress log 2026-10-17).

### 2.9 License and brand

- VectorCraft itself and craft-libs are **MIT OR Apache-2.0**.
- Among the dependencies that go into ArtServer, there is **no GPL / LGPL / MPL / proprietary** code (static analysis of Cargo.lock; not yet confirmed with `cargo deny`). → Confirmed with `cargo deny` on 2026-10-18; `THIRD_PARTY_NOTICES.md` is generated with `cargo about` (design policy section 13.2 #4).
- Notices required for distribution:
  - VectorCraft's MIT notice (`Copyright (c) 2026 ArtCraft Team and the VectorCraft contributors`)
  - MIT notices for craft-libs and the Hayro Authors (`licenses/pdf-backend/*`)
  - BSD-3 notices for Adobe CMap and the Foxit/PDFium fonts (embedded in the PDF reader)
  - The full OFL-1.1 texts for the 4 bundled fonts (`assets/fonts/OFL-*.txt`)
  - The IJG wording for jpeg-encoder, Unicode-3.0 (ICU4X), and BSD/Zlib-style notices
- Brand:
  - **The ArtCraft name and logo are not open source** (`docs/brand/LICENSE-brand.txt`). They may not be used in derivative works. Only stating "based on VectorCraft by the ArtCraft team" in text is allowed.
  - The engine has links to the ArtCraft site and Discord as `help.*` commands (`crates/engine/src/cmd/help.rs:8-27`). ArtServer does not expose them.
  - The VectorCraft name is written into output: the PDF Creator/Producer (`crates/pdf/src/export.rs:274-275`), etc. This is not a license obligation but a brand question.

---

## 3. Comparison of implementation options

| Aspect | A-1: Embed `vectorcraft-mcp` and write only the HTTP layer | **A-2: Embed `vectorcraft-engine` and write the MCP layer (recommended)** | B: Relay the CLI (stdio) through an HTTP gateway | C: Copy the source |
|---|---|---|---|---|
| Initial features | ◎ 32 tools and about 700 commands usable right away | ○ Add tools one by one, starting with those needed | ◎ Same as left | ◎ |
| Safety control | △ Assumes all commands are exposed. A denylist has to be added afterwards | ◎ Can use an allowlist (deny by default) | △ Needs care with the CLI's default behavior (such as auto-connecting to port 7979) | ○ |
| Multiple sessions | △ `Server` is not Send, logging is shared, no lifecycle checks | ○ Managed with an execution thread per session | ◎ Process isolation is possible | ○ |
| Time and memory limits | △ Cooperative cancellation only | △ Same as left (solved later by process isolation) | ◎ The process can be killed | △ |
| Streamable HTTP support | △ Progress and cancellation exist only inside `serve()`, so they must be rebuilt | ○ Can be designed from the start | △ Needs stdio↔HTTP conversion and process management | ○ |
| Brand separation | △ `serverInfo` and tool descriptions are VectorCraft | ◎ ArtServer's own | △ | ◎ |
| Following upstream | ○ Update the rev | ○ Update the rev | ◎ Replace the binary | ✕ Following about 550,000 lines updated daily by hand is unrealistic |
| Amount of implementation | Small | Medium | Medium | Large |

**Why A-2 is recommended**

1. The core value of VectorCraft is the engine (L0–L5), and A-2 also reuses all of it. The MCP layer is very thin compared with the whole engine.
2. `vectorcraft-mcp` is built for stdio use by a single trusted user. The parts needed for network exposure (allowlist, session isolation, lifecycle, size limits, Streamable HTTP progress and cancellation) would be almost entirely rebuilt even with A-1.
3. In light of the development plan's "avoid temporary code becoming permanent", separating responsibilities from the start is easier to maintain than stacking wrappers on parts whose assumptions do not fit.

**Why not Option B**: process isolation is a big advantage, but the CLI is not meant for server use in the first place, and session management, error propagation and performance are hard to manage. Process isolation will be introduced in Phase 3 as "ArtServer's own worker processes" (section 4.4).

**Why not Option C**: given the size and update frequency, following it by hand is not realistic.

---

## 4. Recommended architecture (Option A-2)

### 4.1 Overall structure

```text
MCP client (Claude Code, etc.)
        │  HTTP(S) + MCP Streamable HTTP   * TLS is terminated at a reverse proxy
        ▼
┌────────────────────────────────────── artserver (single binary) ───────────────────────────────────────┐
│ HTTP layer       : bind 127.0.0.1 / Origin/Host checks / body size limit / concurrent connection limit │
│ Auth layer       : Bearer token (constant-time comparison)                                             │
│ MCP layer        : JSON-RPC / initialize and lifecycle / tools and resources                           │
│                    tool definitions and input schemas are ArtServer's own                              │
│ Policy layer     : allowlist of permitted engine commands (deny by default)                            │
│                    also checks nesting such as command.batch / argument check (check_params)           │
│ Session manager  : MCP session ID ↔ session executor / max count / idle disposal                       │
│ Session executor : a dedicated thread per session owns the Session                                     │
│                    applies file_access::confine + cancel::scope on each call                           │
│                    (can later be swapped for a worker process with the same interface)                 │
└───────────────────────────────────────────┬────────────────────────────────────────────────────────────┘
                                            ▼
                          vectorcraft-engine (git dependency, pinned rev)
                          doc / render / svg / pdf / text / format ...
```

### 4.2 Division of responsibilities

| Module | Responsibility |
|---|---|
| `config` | Settings (bind address, token, data directory, various limits). Read from CLI arguments and environment variables |
| `http` | HTTP server, `/mcp`, `/healthz`. Origin/Host checks, size limits |
| `auth` | Token verification |
| `mcp` | MCP protocol handling (initialize, tools/list, tools/call, error conversion) |
| `tools` | ArtServer's tool definitions. Conversion from a tool to a sequence of engine commands |
| `policy` | Checking allowed command IDs and arguments. Handling of file path arguments |
| `session` | Session table, limits, idle disposal, handoff to the executor |
| `engine_host` | Owning the `Session` on a dedicated thread, applying confine/cancel, handling panics and timeouts |

### 4.3 Key design decisions

- **No file I/O by default**: in the MVP, both input and output are passed as Base64 (`document.export` with path omitted, `document.open` with `dataBase64`). When features that take paths are opened up, `confine` to a per-session directory as the read/write root, and always build absolute paths on the ArtServer side. Do not use `confine_process`.
- **Allowlist approach**: general-purpose execution tools such as `run_command` are not exposed in the MVP. If they are exposed, they are still restricted by the allowlist. Commands that change process-wide state (the table in section 2.5), `plugin.*`, `prefs.set`, `text.addFontFiles`, `links.findFiles`, `file.list`, `script.check`, `file.recovery.*` and `help.*` are not allowed.
- **Per-session resource limits**: number of documents, byte limit on Undo history (`History::byte_budget`), execution time per call (cooperative cancellation), number of render threads (`VECTORCRAFT_RENDER_THREADS`).
- **Startup preparation**: scan the font DB at startup (so that the first lookup does not make every call wait). Use ArtServer's own logging, not the shared log queue of `vectorcraft-mcp`.
- **Logging**: do not output tokens, document contents or absolute host paths.

### 4.4 Limits of in-process execution and future isolation

As long as execution stays in-process, the following three cannot be fully prevented.

1. Forcibly stopping a runaway command (work that does not respond to cooperative cancellation)
2. The whole process going down from stack overflow or out-of-memory
3. Memory shared between tenants through shared caches

The MVP assumes "trusted users and local use", and **states these clearly as known limitations**. Before opening to multiple users in Phase 3, replace the session executor with "ArtServer's own worker processes" (without changing the interface).

### 4.5 Dependency management

- Reference `vectorcraft-engine` as a **git dependency pinned to a commit rev** (do not follow a branch).
- Copy VectorCraft's `[patch.crates-io]` (krilla / hayro-syntax / hayro-interpret / hayro-jbig2, rev `ca50d63…`) into ArtServer's root `Cargo.toml`.
- Also copy the profile setting that builds dependency crates at opt-level 2.
- Start `Cargo.lock` from VectorCraft's.
- When bumping the VectorCraft rev, first sync the patch and lockfile and make the tests pass. Check in CI that the PDF backend is the patched version (equivalent to VectorCraft's `xtask/src/pdf_deps.rs`). → A test does it instead, since 2026-10-19 (`tests/pdf_backend.rs`; design policy chapter 8).
- If you need to try a locally modified VectorCraft, switch to a path dependency through local settings that are not committed.

---

## 5. MVP (Phase 1) scope

| Area | Details |
|---|---|
| Startup | `artserver` single binary. Default `127.0.0.1`. Does not start if no token is set |
| Endpoints | `POST /mcp` (Streamable HTTP), `GET /healthz` |
| Tools (minimum) | Create a document, add shapes such as rectangles, set fill and stroke, move and transform, get the document state, export SVG/PDF/PNG (Base64), Undo/Redo |
| Security | Bearer token, Origin/Host checks, body size limit, session count limit, idle disposal, call timeout (cooperative cancellation), allowlist |
| Tests | E2E of rectangle creation → SVG export. Error cases (invalid input, authentication failure, disallowed command, size exceeded, timeout, disallowed path) |
| Documents | README (startup, connecting from Claude Code, health check with curl), LICENSE (MIT), THIRD_PARTY_NOTICES, `docs/progress.md` |

**Not implemented** in the MVP (stated explicitly): TLS (terminated at a reverse proxy), multiple users and authorization, process isolation, rate limiting, input/output by file path, bundling CJK fonts (such as Japanese).

---

## 6. Open issues and decisions needed

| # | Issue | Action |
|---|---|---|
| 1 | Rust is not installed on the development machine (1.95 or later is required) | Ask the project owner whether it can be installed with rustup → **Installed (2026-10-17)**: Homebrew's `rust` (1.99.1), not rustup (design policy section 13.1 #2) |
| 2 | How to implement the MCP protocol layer (use the official Rust SDK `rmcp` etc., or implement it ourselves) | Decide when implementation starts, after checking each candidate's latest version and its Streamable HTTP support → **Decided: rmcp 3.5 series** (design policy section 13.2 #1) |
| 3 | Whether the build passes without `[patch.crates-io]` | No real impact, since the plan is to copy the patch. Check at build time → **Not tried**: the patch is copied, and `Cargo.lock` uses the craft-libs versions (progress log 2026-10-17) |
| 4 | Confirming third-party licenses | Confirm by running `cargo deny` / `cargo about` on the ArtServer side → **Done (2026-10-18)**: no GPL / LGPL / MPL (design policy section 13.2 #4) |
| 5 | Policy for CJK fonts such as Japanese (install on the server / embed with `CRAFT_FONTS_DIR` / add at runtime) | Decide in Phase 2. High priority for print use → **Not done for now (2026-10-19)**: the fonts installed in the OS and `--font-dirs` serve; embedding fonts in the build is not done (design policy section 13.2 #5) |
| 6 | Handling of "VectorCraft" written into output files (PDF Producer, etc.) | Consider whether to accept it as a statement of origin, or whether it can be overwritten from outside the engine → **Decided (2026-10-19): left as is** (design policy section 13.2 #6) |
| 7 | Checking the name "ArtServer" for trademark conflicts | Check before release (development plan §7) → **Done (2026-10-19)**: no conflict found in the US and Japan (design policy section 13.2 #7) |
| 8 | Upstream updates very often, and there is no promise of API stability | Pin the rev and update in a planned way → **Followed**: pinned to a commit, updated to v0.13.1 (`904f0fe`) on 2026-10-18 (design policy chapter 8) |
| 9 | Whether `Renderer` / ArtServer's session type is Send | Check with the compiler at build time → **Verified** (the build passed; design policy section 13.2 #3) |

---

## 7. Mismatches between VectorCraft docs and code (for reference)

Candidates to report upstream.

1. `docs/mcp.md:101` "the restriction is effective on all threads of the server process" → only per thread when used as a library.
2. `docs/mcp.md:88-91` "does not write outside the roots" → there are exceptions, such as creating the template folder and data recovery import.
3. `docs/development.md:164` "library crates do not force the bundled fonts" → they are forced through `vectorcraft-mcp`.
4. `cargo xtask cleanroom` in `AGENTS.md:20` does not exist in CI.
5. `vendor/windows-link/VECTORCRAFT-PATCH.md:17` "the workspace has no [patch]" → it does exist now.
