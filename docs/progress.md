# ArtServer Progress Log

## 2026-10-17 Phase 0 (investigation and choice of approach)

- Investigated VectorCraft (`~/MyTasks/RoundShape/vectorcraft`, commit `fdac986bb49b3e3a9ddd3cda4e88661a6f0aa50c`) by static reading of the source. The results are in `docs/architecture.md`.
- The project owner approved adopting Option A-2 (embed `vectorcraft-engine` as a library and build the MCP layer in ArtServer). The policy is in `docs/design-policy.md`.
- Following an external review (`docs/ARTSERVER_PROCESS_ISOLATION_REVIEW.md`), added the policy for future process isolation as chapter 12 of the design document.
- Confirmed that the latest MCP specification (2026-07-28 version) removed protocol-level sessions, and changed the design to pass working sessions around as handles (design document sections 4.4 and 4.5).

## 2026-10-17 First implementation of Phase 1 (MVP)

### Environment

| Item | Details |
|---|---|
| OS | macOS (Darwin 27.0.0, Apple Silicon) |
| Rust | 1.99.1 (installed with Homebrew's `rust`) |
| Main dependencies | vectorcraft-engine (git, pinned commit), rmcp 3.5.1, axum 0.8.9, tokio 1.53.1 |
| PDF backend | Copied the same `[patch.crates-io]` as VectorCraft, and confirmed in `Cargo.lock` that krilla and hayro-* are the craft-libs versions |

### Commands run and results

| Command | Result |
|---|---|
| `cargo fetch` | Succeeded. `Cargo.lock` was started from VectorCraft's |
| `cargo build --all-targets` | Succeeded (about 2 minutes the first time) |
| `cargo test` | All 27 passed (17 unit, 10 E2E) |
| `cargo clippy --all-targets` | No warnings |
| `cargo fmt --check` | No changes |
| Started the binary and checked with curl | `/healthz` responded, 401 without a token, 15 tools in `tools/list`, session creation → document creation → rectangle → SVG export all succeeded |

### What was implemented

- `artserver` binary (listens on 127.0.0.1. Anything other than loopback needs `--allow-public-bind`)
- MCP Streamable HTTP (`POST /mcp`). Uses rmcp and supports both the 2026-07-28 version and earlier versions. Does not issue session IDs
- `GET /healthz`
- Bearer token authentication (constant-time comparison, requires 32 characters or more)
- Host / Origin checks, body size limit (enabled through rmcp's settings)
- Working sessions (handles are issued bound to a principal, maximum count, disposal when idle)
- An engine execution thread per session (runs in order of arrival, busy error when the queue overflows, cancels when the time limit is exceeded, cancels when the client disconnects)
- Command allowlist, refusal of file path arguments, check for unknown argument keys, limits on the number of documents and on Undo history
- Confines the engine's file access to an empty temporary directory (reads only inside it, no writes at all)
- 15 tools (session, document creation, SVG import, 5 kinds of shapes, fill, move, document state, export, Undo/Redo)

### Failure cases covered by tests

| Failure case | Test |
|---|---|
| Authentication failure (no token, wrong token) | `requests_without_the_right_token_get_401` |
| Invalid Origin / Host | `foreign_origins_and_hosts_are_refused` |
| Body size exceeded | `oversized_bodies_are_refused` |
| Timeout | `a_call_over_the_time_limit_is_cancelled` |
| Invalid input (unknown session, unknown argument `path`) | `unknown_sessions_and_unknown_arguments_are_refused` |
| Drawing with no document | `drawing_without_a_document_reports_a_tool_error` |
| Limit on the number of sessions | `sessions_are_capped`, `session::tests` |
| Commands and path arguments that are not allowed | `policy::tests`, `engine_host::tests::refused_commands_never_reach_the_engine` |
| Paths that are not allowed (files linked from an SVG) | `engine_host::tests::linked_files_outside_the_roots_are_not_read` |
| Another principal's handle | `session::tests::handles_are_bound_to_their_owner` |

### Not implemented / known limitations

- TLS (assumed to be terminated at a reverse proxy), multiple users and permission management, rate limiting, process isolation
- Input and output by file path, bundling CJK fonts such as Japanese
- Work that goes over the time limit is asked to cancel, but the session stays busy until the engine stops
- `THIRD_PARTY_NOTICES.md` is a summary based on the Phase 0 audit. Before distributing binaries, the full text needs to be generated with `cargo about` or similar
- License and vulnerability checks with `cargo deny` have not been done yet

### Not yet verified

- Checking operation by actually connecting from Claude Code (the steps are already in the README. Not done because it needs to be added to the project owner's Claude Code settings)
- Building and running on Linux
- Operation with older clients (those that use `initialize` and `Mcp-Session-Id`). The E2E tests use the rmcp 3.5.1 client

### Next steps

1. Check the connection from Claude Code
2. Add `LICENSE` (MIT) (the copyright holder notation needs to be decided)
3. Introduce `cargo deny` / `cargo about` and generate the license notices
4. Build and test on Linux
5. Phase 2: decide the policy for CJK fonts such as Japanese, expand the tools (text, layers, groups, images, etc.)

## 2026-10-18 Connection check from Claude Code and fixes

### What was done

- Added `LICENSE` (MIT). The copyright holder notation is `Copyright (c) 2026 RoundShape` (decided by the project owner).
- Connected from Claude Code and checked that it works.
  - Put the token in `~/.config/artserver/token` and started with `./target/debug/artserver --token-file ~/.config/artserver/token`
  - Registered with `claude mcp add --transport http artserver ...`. It uses the default local scope, so it only appears in Claude Code started in the folder where it was registered (once it did not appear when Claude Code was started in the home folder)
  - `session_create` → `document_new` → `draw_rectangle` → `export_document` (PNG and SVG) all succeeded

### Bug found and fix

| Item | Details |
|---|---|
| Symptom | On SVG export, Claude Code showed only `{"bytes":445,"format":"svg","warnings":[]}`, and the SVG body did not arrive |
| Cause | It returned different content in the two places: the SVG body in `content` and a summary in `structuredContent`. When `structuredContent` is present, Claude Code shows that. In the MCP specification, both are meant to represent the same result |
| Fix | For SVG and PNG, return only the body, without `structuredContent`. Only when there are warnings, add `{"warnings":[...]}` as a second text (`export_result` in `src/tools.rs`). PDF unchanged |
| Tests | Added 3 unit tests. The E2E tests now check that SVG and PNG results have no `structuredContent` |
| Why it was missed | The E2E tests read `content` directly with the rmcp client, so they could not detect the difference in how Claude Code shows the result |

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --all-targets` | Succeeded |
| `cargo test` | All 30 passed (20 unit, 10 E2E) |
| `cargo clippy --all-targets` | No warnings |
| `cargo fmt --check` | No changes |
| Restarted the server and ran SVG export from Claude Code | The SVG body was shown |

### Not yet verified

- Building and running on Linux
- Operation with older clients (those that use `initialize` and `Mcp-Session-Id`)
- How a PDF export called from Claude Code is handled. It currently returns base64 in `structuredContent`, so large documents may weigh down the conversation

## 2026-10-18 License and vulnerability checks of dependencies

### What was done

- Installed cargo-deny 0.20.2 and cargo-about 0.9.2 with Homebrew.
- Created `deny.toml`.
  - Allowed licenses: MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, Zlib, Unicode-3.0, BSL-1.0. IJG is also allowed, for `jpeg-encoder` only
  - Targets: macOS and Linux (aarch64, x86_64). Test-only dependencies are excluded
  - Sources: only crates.io and GitHub's storytold/vectorcraft and storytold/craft-libs
- Created `about.toml` and `about.hbs`, and replaced the summary in `THIRD_PARTY_NOTICES.md` with a version that contains the full texts (286 crates, 141 kinds of license text, about 226KB).
  - The first half is handwritten in `about.hbs`: the origin of VectorCraft, the copyright notices of the 4 bundled fonts and the full text of OFL-1.1, the full BSD-3-Clause text for Adobe CMap and the PDFium fonts, the IJG wording, and krilla's MIT notice. The full texts were inserted as is from the source files used in the build
  - The second half is generated by cargo about from `Cargo.lock`

### Commands run and results

| Command | Result |
|---|---|
| `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok. 1 warning (yanked, see below) |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | Succeeded |

### Findings and how they were handled

| Item | Details |
|---|---|
| Crates that do not ship a LICENSE file | 32 crates, including VectorCraft's 21 crates, craft-pdf and krilla, ended up with a template whose copyright holder was blank (`Copyright (c) <year> <copyright holders>`). Specified the correct files with `clarify` in `about.toml`. chrono was resolved by cargo about's built-in handling (`workarounds`) |
| Generation needs the network | alloc-stdlib, wasmi (4 crates) and wasmparser have no LICENSE in the crate, so it is fetched from the matching commit in the original repository. If the checksum does not match, generation fails, so the text cannot change silently |
| krilla | The version taken into craft-libs has no LICENSE, so it cannot be fetched automatically. Put the MIT notice from the original repository into `about.hbs` by hand. In the generated part, krilla is listed under the generic MIT text |
| Test-only dependencies | cargo deny also counts the rmcp features enabled in `[dev-dependencies]` (reqwest, aws-lc-sys, etc.). Confirmed with `cargo tree -e normal --target all` that they are not in the binary. Counting too many errs on the safe side, so this was left as is. cargo about excludes them correctly |
| Reference to VectorCraft | git dependencies pinned to a commit have no version requirement, so they trip the `wildcards` check. Allowed in `deny.toml` |
| yanked | `yoke-derive 0.8.3` has been yanked (0.8.4 exists, no reason given for the yank). It can be upgraded with `cargo update -p yoke-derive`, but this has not been done |

## 2026-10-18 Build check on Linux (GitHub Actions)

### Policy

Few people are expected to use ArtServer on Linux. Since VectorCraft also supports Linux, the build is kept passing, but tests and operation checks are not done (the project owner's decision).

### What was done

- Added `.github/workflows/build.yml`. On every push (main) and pull request, it runs `cargo build --locked` on ubuntu-latest.
  - Caches the dependency build results with Swatinem/rust-cache
  - If there is a new push on the same branch, the old run is stopped
  - The only permission is reading the repository

### Results

| Item | Details |
|---|---|
| Run | run 39084451667 (commit `bea476b`) |
| Result | Succeeded. No warnings or errors |
| Duration | About 16.5 minutes in total, of which the build took 15m 49s. Compiling and linking ArtServer itself took about 3 minutes |

- It takes long because GitHub's machines for private repositories have 2 cores, and because `opt-level = 2` is applied to dependencies even in development builds. From the second run on, the cache takes effect. When `Cargo.lock` changes, it becomes long again
- If it is too long, one option is to switch to `cargo check` (which cannot check linking)
- GitHub's notice: `ubuntu-latest` switches to Ubuntu 26 gradually from 2026-10-19

## 2026-10-18 Integrating VectorCraft's tools (text and fonts)

### Policy

Decided design policy section 1.1 (intended usage), section 4.6 (passing large data) and section 4.7 (tool structure) with the project owner. Section 4.7 is the approach of using the tool part of VectorCraft's MCP (`call_tool` and the trait `Backend`) through the GuardedBackend that ArtServer implements.

### What was done

- Added `vectorcraft-mcp` at the same commit as a dependency (only the tool part is used). No new external libraries were added.
- Added `src/backend.rs` (GuardedBackend). `engine.execute` runs after checking the allowlist, paths, arguments and number of documents; `engine.commands` returns only the allowlisted commands; `ui.render` renders with the engine's PNG export; anything else (`ui.*`, `app.*`, etc.) is refused.
- `src/engine_host.rs`: VectorCraft's tools can now run on the working session's thread under the existing file restrictions, cancellation and time limit. Combined command checking and execution into `execute_checked`.
- `src/policy.rs`: expanded the allowlist to about 75 commands. `file.new`, `document.open` and `document.export` can be used only from ArtServer's own tools. The path check also looks at the value's type (strings are refused, shape ids pass). Plugin effects are refused.
- `src/tools.rs`: exposed 16 VectorCraft tools, with the working session's `session` argument added. ArtServer's own tools are 6 (working session, `document_new`, `import_svg`, `export_document`, the new `list_fonts`). Dropped the 10 overlapping ones (the 5 shapes such as `draw_rectangle`, `set_paint`, `move_objects`, `inspect_document`, `undo`, `redo`). Also updated the server description.
- Added the startup setting `--font-dirs` (`ARTSERVER_FONT_DIRS`). In addition to the OS fonts, fonts in the specified folders are used.
- Added `vectorcraft-mcp` to `about.toml` and regenerated `THIRD_PARTY_NOTICES.md`. Updated the tool and settings tables in the README.

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --all-targets --offline --locked` | Succeeded |
| `cargo test --offline --locked` | All 43 passed (30 unit, 13 E2E) |
| `cargo clippy --all-targets` | No warnings |
| `cargo fmt --check` | No changes |
| `cargo deny check` | advisories ok, bans ok, licenses ok, sources ok (the 1 yanked warning, as before) |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | Succeeded (`vectorcraft-mcp` was added to VectorCraft's MIT section) |

Main new tests: VectorCraft's tools work on top of GuardedBackend (Japanese text, shapes, transforms, preview); commands that are not allowed are refused from every tool; `ui.*` and `app.*` are refused; `command_list` returns only the allowlisted commands; every id in the allowlist exists in the engine; categories that are not opened up are not in the allowlist; the list of exposed tools is exactly 22; font listing and filtering.

### Observations

- Letting `Cargo.lock` update with `--offline` also moved unrelated parts (the `windows-sys` references for Windows) to older versions. Inserted only the `vectorcraft-mcp` part into the original `Cargo.lock` and confirmed that it passes with `--locked`.

### Not yet verified

- Checking from Claude Code with the new tools (ArtServer needs a restart)
- Whether Japanese text is drawn with actual Japanese fonts such as Hiragino (the tests only check text layout and export)
- Checking the number of commands by running the engine (about 760 is a static count)

## 2026-10-18 Check in Illustrator, and fix for the text weight in SVG

### What was checked

- Sent requests with curl to ArtServer running the new binary, created a document with Japanese text (Hiragino Kaku Gothic W6, Hiragino Mincho W6, Hiragino Maru Gothic W4, vertical type) and shapes, and exported SVG, SVG converted to outlines, and PDF to `~/Desktop/artserver-output/`. All fonts were "exact" (an exact match).
  - Claude Code in this conversation had not reloaded the tool list, so the new tools were not used directly from Claude Code (still not yet verified).
- When the project owner opened the files in Illustrator, the SVG converted to outlines was correct, but in the SVG that kept the text, Kaku Gothic and Mincho were shown in a thin weight (such as W3).

### Cause and fix

| Item | Details |
|---|---|
| Cause | VectorCraft's SVG export decides the weight from the style name with `vectorcraft_text::style_weight`. This function only reads English words (Light, Bold, etc.), so "W6" becomes 400, and at 400 it does not write `font-weight` |
| How it was fixed | Following the policy decided with the project owner (design policy chapter 8), copied only `vectorcraft-svg` to `vendor/vectorcraft-svg`, and made `font_descriptor` in `src/css.rs` read W0–W9 as a weight of the number × 100. It is swapped in with `[patch."https://github.com/storytold/vectorcraft"]` in `Cargo.toml` |
| Scope of impact | Only SVG (and CSS) export. The font selection logic was not changed |
| Records | `vendor/README.md` (what the fix is, steps for reapplying the patch), `vendor/patches/vectorcraft-svg.diff` (diff against the original code) |
| License | Placed VectorCraft's `LICENSE-MIT` and `LICENSE-APACHE` in the copied crate. `cargo about` also counted them as ArtServer's own license, so ArtServer's license file was specified explicitly in `about.toml` |

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --all-targets --offline` | Succeeded. The only change to `Cargo.lock` was one line: the source of `vectorcraft-svg` |
| `cargo test --offline --locked` | All 44 passed (31 unit, 13 E2E). New test: the W6 and W3 style names become SVG `font-weight` 600 and 300, Regular gets none, and Bold stays bold |
| `cargo clippy --all-targets` / `cargo fmt --check` | No warnings / no changes |
| `cargo deny check` | All ok (the 1 yanked warning, as before) |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | Succeeded |

### Check after the fix

- Restarted ArtServer with the fixed binary and exported the samples again. In `sample.svg`, the Hiragino Kaku Gothic W6 and Mincho W6 text got `font-weight="600"` (Maru Gothic W4 is the normal weight, so it gets none).
- The project owner opened it in Illustrator and confirmed that Kaku Gothic and Mincho are shown in the W6 weight. An additional fix to write PostScript names was not needed.
- Reported it to upstream VectorCraft in an Issue: [storytold/vectorcraft#2873](https://github.com/storytold/vectorcraft/issues/2873). There was no duplicate report, and it was not fixed in upstream `main` (`73f40e3`) either. The project owner checked the wording, and it was posted with the project owner's permission.

### Not yet verified

- Checking from Claude Code with the new tools (the tool list needs to be reloaded)

## 2026-10-18 Passing large exports by URL

### Policy

Implemented the export part of design policy section 4.6. The file store is kept in **memory**, and the endpoint that files are fetched from is authenticated with **the same Bearer token as MCP** (decided by the project owner).

### What was done

- Added `src/files.rs`: the file store for exports (`FileStore`) and the `GET /files/{id}` endpoint.
  - Only the principal that stored a file can fetch it (other principals get 404). IDs are 32 random bytes
  - Expiry defaults to 10 minutes, the total defaults to 256MiB (refused when exceeded), 8 files per working session (when exceeded, the oldest are discarded first)
  - Expired files are discarded by a cleanup every 60 seconds, and files of closed working sessions by `session_close`
  - Host and Origin are checked against the same allowlist as `/mcp`
  - The file name is made from the document name, and a UTF-8 name is also written in `Content-Disposition`
- Added `delivery` (auto, inline, url) to `export_document`. With auto, SVG up to 32KB and PNG up to 256KB are put in the response, and PDF and anything larger is returned as a URL.
- Added the startup settings `--file-ttl-secs` and `--max-file-mib`.
- Wrote in the server description and the tool description that the URL is fetched with the token. Added a "Receiving large exports" section to the README.

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --all-targets --offline` | Succeeded (no new dependencies, and `Cargo.lock` did not change) |
| `cargo test --offline --locked` | All 52 passed (38 unit, 14 E2E) |
| `cargo clippy --all-targets` / `cargo fmt --check` | No warnings / no changes |

Main new tests: a file is returned only to the principal that stored it, expiry, the per-working-session count limit, the total limit, deletion when a working session is closed, file names, Host and Origin checks (unit). A PDF is returned as a URL and can be fetched with a GET with the token, 401 without a token, 404 for a wrong ID, 403 for a Host or Origin that is not allowed, SVG can also be returned as a URL with `delivery: url`, 404 after the working session is closed (E2E).

### Observations

- The AI (Claude Code) does not know ArtServer's token, so to have it fetch a URL, it must be told where the token file is (described in the README). If this proves inconvenient, compare again with an approach where a short-lived URL alone is enough.

### Check in Claude Code (2026-10-18)

Restarted ArtServer and restarted Claude Code with `claude -c` (Reconnect in `/mcp` alone did not replace the tool list), and confirmed that the AI can use the new tools directly.

| What was checked | Result |
|---|---|
| `list_fonts` (filtering, list of styles) | Returned the 2 Hiragino Maru Gothic families and Hiragino Kaku Gothic W0–W9 |
| `add_text` (Japanese, with a font specified) | Placed text in Kaku Gothic and Maru Gothic |
| `command_run` (`text.setStyle`, `text.setText`, `text.fonts`) | Changing to W6, replacing the text and checking the fonts in use all worked |
| `command_run` with a command that is not allowed (`prefs.set`) | Refused with "not allowed" |
| `render_preview` | The preview image was shown |
| `export_document` (PDF) | A URL was returned instead of the content. Fetched the PDF (about 13KB) with curl with the token; 401 without the token |

- In the conversation resumed with `claude -c`, the AI still saw the old descriptions of existing tools (such as `export_document`) (the `delivery` argument was not visible). It works with the default delivery, so this did not affect the check, but a new conversation should show the latest descriptions.
- The AI could fetch the URL because it knew where the token file was. In other environments, it needs to be told the location, as described in the README.

## 2026-10-18 Changed receiving exports to URLs that need no token

### Background

After the check in Claude Code, the project owner opened the returned URL in a browser and got `unauthorized` (the designed behavior, since a token was required). Browsers and clients that cannot run curl (such as Claude Desktop) could not fetch it, and the AI also had to be told where the token is, so by the project owner's decision the approach was changed to "fetchable with the URL alone (with an expiry)".

### What was done

- `src/files.rs`: stopped checking the principal that stored the file, so it can be fetched with the URL's ID (256 random bits) alone. It can be fetched any number of times until it expires (default 10 minutes) (deleting the file after one fetch was not adopted, because link prefetching could delete it). Added headers so that it is never opened inside the browser and is always downloaded (`Content-Security-Policy: sandbox`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`).
- `src/lib.rs`: moved `/files/{id}` outside the token check. Kept the Host and Origin checks.
- `src/tools.rs`: updated the server description, the `export_document` description and the response wording (`auth` → `note`) to match not needing a token.
- Updated the README and the design policy (sections 4.6 and 13.2).

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --all-targets --offline` | Succeeded |
| `cargo test --offline --locked` | All 52 passed (38 unit, 14 E2E). The E2E tests checked that the PDF can be fetched without a token (and fetched twice), the security headers, 404 for a wrong ID, 403 for a Host or Origin that is not allowed, and 404 after the working session is closed |
| `cargo clippy --all-targets` / `cargo fmt --check` | No warnings / no changes |

### Check in a browser

Restarted ArtServer, exported a PDF from Claude Code, and the project owner opened the returned URL in a browser. It downloaded as `browser-download-test.pdf` without a token.

## 2026-10-18 Added `--public-url`

### Background

Behind a reverse proxy, the export URL depended on the proxy settings (whether it passes `Host` and `X-Forwarded-Proto`), and setups that add a path prefix (such as `/artserver/`) were not supported. At the project owner's instruction, added `--public-url` (`ARTSERVER_PUBLIC_URL`), which sets the externally visible URL at startup.

### What was done

- `src/config.rs`: added `--public-url`. Only a full http or https URL is accepted, and user info and queries are not accepted (if it is wrong, the server does not start). A trailing `/` is removed. The host name is added to `--allowed-hosts` automatically.
- `src/tools.rs`, `src/lib.rs`: when `--public-url` is set, the export URL is built by appending `/files/{id}` to it. Otherwise, it is built from `Host` and `X-Forwarded-Proto` as before.
- Updated the README (settings table, reverse proxy explanation) and the design policy (section 4.6).

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 54 passed (39 unit, 15 E2E). New tests: the `--public-url` check and automatic addition of the host name (unit); with `--public-url` set, the URL becomes `https://example.com/artserver/files/…`, and ArtServer itself serves it at `/files/…` (E2E) |
| `cargo clippy --all-targets` / `cargo fmt --check` | No warnings / no changes |

## 2026-10-18 Build check on Windows (GitHub Actions, manual run)

- Added `.github/workflows/build-windows.yml`. It runs `cargo build --locked` on `windows-latest` (no tests). For private repositories, Windows machines use up the free quota about twice as fast as Linux, so it does not run on push and runs only when started manually (`gh workflow run build-windows.yml` or the Actions screen).
- The first run (run 39120244900) succeeded. No warnings or errors. About 34 minutes in total, of which the build took 32m 20s. `vendor/vectorcraft-svg` was also swapped in and built on Windows.
- This confirmed that the build passes on all three: macOS, Linux and Windows. Operation on Windows (running on a real machine) is not yet verified.

## 2026-10-18 Updated VectorCraft to v0.13.1

### Policy

The project owner chose the official release v0.13.1 as the target (commit `904f0fe5227bd987d55319e89a38b4b8f046998c`, published 2026-10-18, 142 commits ahead of the previous pin `fdac986`). A release point is used rather than the latest main (`797e899`).

### What changed and how it was handled

| Item | Details |
|---|---|
| commit | Raised the 4 VectorCraft entries in `Cargo.toml` and the dependency in `vendor/vectorcraft-svg/Cargo.toml` to `904f0fe` |
| PDF patches and build settings | Same in VectorCraft v0.13.1 (craft-libs `ca50d63`). Unchanged |
| New dependency | `craft-docpath` (craft-libs, rev `82a24b0`. Decides whether a path saved in a document may be opened). `vectorcraft-svg` uses it, so it was added to the `Cargo.toml` in `vendor/`. Added its license setting to `about.toml` |
| `vendor/vectorcraft-svg` | Copied the v0.13.1 code again. Upstream's `font_descriptor` had changed to look at the variable font `wght` axis first, so instead of applying the diff, read the code and reapplied the patch by hand, then recreated the diff file. #2873 is still not fixed in v0.13.1 |
| yoke-derive | Upgraded from 0.8.3 (yanked) to 0.8.4. The `cargo deny` yanked warning went away |
| ArtServer's code | No changes needed (the public API changed little, and the names of VectorCraft's MCP tools are the same) |
| License notices | Updated the VectorCraft and craft-libs entries in `about.hbs` and regenerated |

### Commands run and results

| Command | Result |
|---|---|
| `cargo fetch` | Succeeded |
| `cargo build --all-targets --locked` | Succeeded (no warnings or errors, about 1m 14s) |
| `cargo test --locked` | All 54 passed (39 unit, 15 E2E). Includes checks that every allowlisted command exists in the v0.13.1 engine, that the 16 VectorCraft tools are exposed, and the SVG weight fix |
| `cargo clippy --all-targets` / `cargo fmt --check` | No warnings / no changes |
| `cargo deny check` | All ok (no warnings) |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | Succeeded |

### Builds on GitHub Actions

| Environment | Result |
|---|---|
| Linux (automatic on push, run 39123583009) | Succeeded. No warnings or errors. About 12.5 minutes in total, of which the build took 10m 45s |
| Windows (manual, run 39123684572) | Succeeded. No warnings or errors. About 33 minutes in total, of which the build took 28m 33s (long even with the cache, because all VectorCraft crates were rebuilt) |

### Checking operation from Claude Code

Restarted ArtServer with the v0.13.1 binary and checked from Claude Code. Everything worked as before.

| What was checked | Result |
|---|---|
| `list_fonts`, `draw_shape` (star), `transform` (move and rotate), `add_text` (Kaku Gothic, Mincho) | Worked |
| `command_run` (W6 with `text.setStyle`, `text.setText`, `text.fonts`) | Worked. Both fonts exact |
| `command_run` with a command that is not allowed (`prefs.set`) | Refused |
| `render_preview` | The preview image was shown |
| `export_document` (SVG) | The body was returned, and the Kaku Gothic and Mincho text had `font-weight="600"` (the `vendor/` fix still works in v0.13.1). Since v0.13.1, text is written with `<tspan>` |
| `export_document` (PDF) | A URL was returned and could be fetched without a token. Hiragino Kaku Gothic W6 and Mincho W6 were embedded |

### Next steps

1. Upload for import (when import of PDF or `.ai` is allowed. Design policy section 4.6)

## 2026-10-19 What exported files record about VectorCraft

### Background

The project owner asked whether the "VectorCraft" that PDFs record shows on the page, and whether `.ai` and `.vectorcraft` record it too. Checked by exporting from a temporary ArtServer (`target/debug/artserver` on port 18471, stopped afterwards) and by reading the VectorCraft v0.13.1 source.

### Results

| Format | What is recorded | How it was checked |
|---|---|---|
| PDF | Creator and Producer `VectorCraft`, in the document information and in XMP (`xmp:CreatorTool`, `pdf:Producer`). Nothing on the page: it shows only in the document properties | Exported and read with `pdfinfo` |
| PDF | The embedded file `vectorcraft-editing.vectorcraft` (the native document, with `"generator":"VectorCraft 0.13.1"`), described as "VectorCraft editing data". Written because of VectorCraft's PDF setting `preserve_editing` | Exported and extracted |
| VectorCraft's `.ai` | A PDF that always carries the editing data (`encode_ai` forces `preserveEditing`), with the same Creator and Producer | Source |
| `.vectorcraft` | `"generator": "VectorCraft <version>"` at the top of the JSON (`crates/format/src/lib.rs:352`) | Source |
| Legacy `.ai` (PostScript) | `%%Creator: VectorCraft <version>` (`crates/eps/src/legacy_ai.rs:168`) | Source |
| SVG / PNG | None. PNG has only `Title` and `Creation Time` | Exported |

- VectorCraft writes these names as fixed strings; ArtServer has no setting for them. The licenses (MIT OR Apache-2.0) neither require nor forbid them, and VectorCraft's brand terms cover only the ArtCraft marks. How to handle them is still open (design policy section 13.2, #6).
- ArtServer can't export `.ai` or `.vectorcraft`, and it opens only SVG. If PDF import were allowed, the editing data in ArtServer's PDFs could serve for saving work and resuming it later (not tried).

## 2026-10-19 English for the project

### Policy

The project owner decided that ArtServer is not aimed at Japan only. The docs, code comments, config comments and commit messages are in English from now on. The READMEs keep a Japanese copy.

### What was done

- `README.md` and `vendor/README.md`: full English translations. The Japanese versions moved to `README.ja.md` and `vendor/README.ja.md`, with language links at the top. Fixed three outdated points in both languages: the status (Phase 2 in progress, MVP done), PDF export (a download URL by default, not Base64) and the platforms (Linux and Windows are build-checked only).
- Translated the five documents in `docs/`. Compared each with the original by script: the line counts, headings, table rows, code spans and URLs are the same, apart from the intended changes. Aligned three wordings between the documents.
- Translated the comments in `src/`, `tests/`, `Cargo.toml`, `about.toml`, `deny.toml`, the GitHub Actions workflows (including the step names) and `vendor/vectorcraft-svg/Cargo.toml`. `artserver --help` is now in English.
- Generalized Japan-specific wording: `example.co.jp` → `example.com` (code, tests, READMEs, docs), and "Japanese fonts" as a general requirement → "CJK fonts such as Japanese". Facts about specific Japanese fonts (Hiragino, the W0–W9 weights) stay. Japanese strings that are test data stay.
- Design policy chapter 11: Phase 2 is in progress since 2026-10-18, with what is done and what remains. Added both changes to its revision history.

### Commands run and results

| Command | Result |
|---|---|
| `cargo fmt --check` | No changes, after `cargo fmt` put one assertion in `src/config.rs` on a single line (it fits with the shorter domain) |
| `cargo clippy --all-targets --offline --locked` | No warnings |
| `cargo test --offline --locked` | All 54 passed (39 unit, 15 E2E) |
| Parsing the edited TOML and YAML files | ok |
| `cargo deny check` | Not run: `cargo-deny` was not found in this shell. Only comments changed in `deny.toml` |

- Committed as `8327012`, `8ae30fa` and `ab006a9` and pushed to main. The Linux build on GitHub Actions could not be checked with `gh` (HTTP 404: `gh` is logged in with a different account from the SSH alias used for push).

## 2026-10-19 Notes from reviewing concurrency and sessions (no changes)

Found while explaining how concurrent requests and working sessions are handled. Nothing was changed.

- There is no process-wide limit on concurrent heavy work, which design policy section 5.3 plans. Up to 8 working sessions can export at once, each with up to 2 rendering threads.
- The time limit (`--call-timeout-secs`) also counts the time a request waits in the queue, so a request can time out before it starts.
- The queue depth (8) has no command-line setting.
- No test covers working sessions running in parallel or the busy error.
- The server description tells the AI to call `session_create` first but doesn't mention `session_close`, so working sessions most likely end by idle disposal (30 minutes) and count toward the limit of 8 until then.
- `LocalSessionManager` is passed to rmcp but not used, because legacy session mode is off. `NeverSessionManager` would state that intent.

### Next steps

1. Upload for import (when import of PDF or `.ai` is allowed. Design policy section 4.6)
2. Decide how exported files record "VectorCraft" (design policy section 13.2, #6)
3. Check the name "ArtServer" for trademark conflicts before publishing (design policy section 13.2, #7)

## 2026-10-19 Decided to leave "VectorCraft" in exported files as is

- The project owner decided to keep what VectorCraft writes into exported files unchanged (design policy section 13.2, #6): the PDF Creator and Producer stay "VectorCraft", and PDFs keep the embedded editing data. The other options were "ArtServer" for both, and Creator "ArtServer" with Producer "VectorCraft". Changing them would have needed an option upstream, a patched copy of the PDF crate in `vendor/`, or rewriting the PDF after export.

### Next steps

1. Upload for import (when import of PDF or `.ai` is allowed. Design policy section 4.6)
2. Check the name "ArtServer" for trademark conflicts before publishing (design policy section 13.2, #7)

## 2026-10-19 Trademark check for the name "ArtServer"

A preliminary check before publishing (design policy section 13.2, #7). It is not a legal clearance by a patent attorney.

| Where | Result |
|---|---|
| USPTO ([official search](https://tmsearch.uspto.gov/)) | No live marks. ARTSERVER (serial 75042242, class 42, filed 1996) was abandoned in 1997, and the two ARTSERVE marks (classes 35, 41, 45) were cancelled. No VECTORCRAFT mark. The live ARTCRAFT marks belong to unrelated companies (toys, lighting, film) |
| J-PlatPat ([trademark search](https://www.j-platpat.inpit.go.jp/t0100), searched by the project owner) | Mark text ARTSERVER: 0 hits. Similar pronunciation アートサーバー: 7 hits, none of them アートサーバー. The closest is AUTO SERVER (owner: 株式会社オートサーバー, a used-car trading company; registrations 5913670 and 6530958). Its class 9 includes similarity group 11C01, which covers computer programs, but the project owner judged the names not similar: the first sound (アー / オー) and the meaning (art / auto) differ |
| Products with similar names | Blackdove "Art Server" (hardware that shows digital art on a display) and Trimble TruckMate "ART Server" (an API server for logistics). No software named ArtServer in vector graphics or MCP |
| Package names | `artserver` is free on crates.io, npm, PyPI and Homebrew. GitHub has about 20 small personal repositories named artserver. artserver.com is a parked domain |
| Not checked | EU (EUIPO / TMview), WIPO, other countries |

- Conclusion: no conflict found, and the name ArtServer stays. Registering "ArtServer" as a trademark in Japan would need a patent attorney's review, because AUTO SERVER covers software.
- ArtCraft names its apps "…Craft" (such as VectorCraft), so ArtServer is unlikely to be taken for an ArtCraft product. The README states that ArtServer is not an official VectorCraft derivative.

### Next steps

1. Upload for import (when import of PDF or `.ai` is allowed. Design policy section 4.6)

## 2026-10-19 Upload for import

### Policy

The project owner chose option A of four: a URL made by a tool. The others were an upload endpoint that takes the Bearer token, a fixed inbox folder, and fetching from a web URL. Import is opened without process isolation, for the users design policy section 1.1 assumes (trusted, in one organization), with limits on size, time and formats; section 12.4 was revised to say so. The project owner's long-term idea, a server-side image library to place images from, is recorded in design policy section 4.6 (not scheduled).

### What was done

- `src/uploads.rs` (new): `PUT /uploads/{id}` receives one file as the raw body, with no token (the ID in the URL is the key). `GET /uploads/{id}` shows a page with a file chooser whose script (`/uploads/upload.js`) sends the file to the same URL; the page may run only that script and talk only to this server. The format is told from the first bytes: PDF (including `.ai` saved with PDF compatibility), SVG, PNG and JPEG; anything else gets 415. Host and Origin are checked as for `/files/`, and this server's own origin also passes, for the page.
- `src/files.rs`: the file store also holds uploads, sharing the total limit, the expiry and the deletion on `session_close` with exports, and keeping at most 8 per working session (counted apart from exports). An upload takes one file, and only its own working session can use it. `base_url` moved here from `src/tools.rs`, and the Host and Origin checks are shared with the upload endpoint.
- `src/tools.rs`: new tools `upload_create`, `open_upload` (`document.open` with the contents as data) and `place_upload` (`file.place`, always embedded). The server description explains the steps. `import_svg` now refuses an argument that isn't SVG, since the engine picks its reader from the contents.
- `src/policy.rs`: `file.place` is ArtServer-only. `document.open` and `file.place` take only data (`name` and `dataBase64`) whose name ends in svg, pdf, ai, png or jpg, and never a link.
- `src/config.rs`: `--max-upload-mib` (`ARTSERVER_MAX_UPLOAD_MIB`, default 64). `src/lib.rs`: the upload routes, with that body limit.
- README (English and Japanese): the new tools and setting, and a section on sending files. Design policy: sections 4.6, 4.7, 6.3, 12.4 and 13.2, and chapter 11.

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --all-targets --offline --locked` | Succeeded. No new dependencies; `Cargo.lock` is unchanged |
| `cargo test --offline --locked` | All 62 passed (45 unit, 17 E2E). New tests: the upload store (one file, own session only, never served as an export, shared limits, deletion), telling formats from contents, the names the engine gets, the own-origin rule (unit); exporting a PDF and a PNG, sending them back, opening the PDF and placing the PNG, refusal for another session, deletion on close; 415, 413, 404, 403 for Host and Origin, the page with its policy and script, a PUT from the server's own origin, `import_svg` refusing a PDF (E2E) |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |

### Check against the running server

Started `target/debug/artserver`, drew a star, exported a PDF, sent it back with `upload_create` and a PUT (4,015 bytes, format pdf), and opened it with `open_upload`. The result had `"restored": true` and no warnings: the engine reopened the document from the editing data in the PDF, not as plain artwork. So a PDF from ArtServer can serve for saving work and continuing it later. The E2E test now checks `restored` too.

### Not yet verified

- Claude Code using the new tools (ArtServer needs a restart, and Claude Code a new conversation to see the new tools)
- The browser page in a real browser (the tests check that the page, its policy and the script are served, and that a PUT with the server's own Origin passes)
- Illustrator `.ai` files and PDFs from other apps (the tests use ArtServer's own PDF and PNG)
- `cargo deny check` (`cargo-deny` was not found in this shell; no dependencies were added)

### Next steps

1. Check uploads from Claude Code and from a browser, with real `.ai` and PDF files

## 2026-10-19 .ai and .vectorcraft export

### Policy

At the project owner's suggestion, added these before trying uploads from Claude Code, since both can be tested by sending the export back. The `.ai` is PDF-compatible only (the project owner's choice): VectorCraft can also write the legacy PostScript `.ai` (the Illustrator 7.0 format, the only one Adobe published), which Illustrator edits as objects, but it turns Japanese text into outlines and drops transparency, so it is not written. Design policy section 4.6.

### What was done

- `src/tools.rs`: `export_document` takes `ai` and `vectorcraft`. Both always come back as a URL (`delivery: inline` gives Base64) and download as `application/octet-stream`, so that browsers keep the file name. `.vectorcraft` files are written compressed. The tool descriptions say how Illustrator opens the `.ai`.
- `src/policy.rs`: `ai` and `vectorcraft` are export formats; `aiVersion` other than `pdf` is refused; `.vectorcraft` data may be opened or placed.
- `src/uploads.rs`: accepts `.vectorcraft`, told by VectorCraft's own test (`vectorcraft_format::sniff`). `vectorcraft-format` became a direct dependency at the same commit; it was already in `Cargo.lock` and in the license notices, so nothing new is built or listed.
- `src/files.rs`: export file names drop a known file extension from the document title. The new E2E test found this: a document opened from `poster.ai` is titled `poster.ai`, so its export was named `poster.ai.vectorcraft`.
- README (English and Japanese) and design policy (section 4.6, chapter 11).

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --all-targets --offline` | Succeeded. `Cargo.lock` changed by one line (ArtServer lists `vectorcraft-format`) |
| `cargo test --offline --locked` | All 63 passed (45 unit, 18 E2E). New E2E test: export `.ai` and `.vectorcraft` (a URL, the file name, the content type, a PDF and a gzip stream inside), send each back and open it: the same format, no warnings, the colour kept, and the `.ai` restored from its editing data. Unit tests extended: the export formats and `aiVersion` in the allowlist, `.vectorcraft` told from its contents, file names from titles that end in an extension |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |

### Not yet verified

- Opening the exported `.ai` in Illustrator (VectorCraft's source says one clipped group per page)
- Opening the exported `.vectorcraft` in the VectorCraft desktop app
- Using uploads and the new formats from Claude Code and from a browser

### Next steps

1. Check uploads and the new export formats from Claude Code and from a browser, with real `.ai` and PDF files, and open the exported `.ai` in Illustrator

## 2026-10-19 Checking uploads and the new formats (server, browser, Illustrator)

### Against a running server

Started `target/debug/artserver` and drove it with a script. The test document had a blue rectangle, an orange ellipse at 50% opacity, a green star with a stroke, "ArtServer テスト" in Hiragino Sans W6, "Hello, Illustrator" in Inter, and a second layer. Both fonts were exact. The exported files went to `~/Desktop/artserver-output/` for the project owner.

| What was checked | Result |
|---|---|
| Export `.ai` (15,804 bytes) | No warnings |
| Send the `.ai` back and open it | `restored: true`. The two layers and six objects, with their names, matched the original |
| Export `.vectorcraft` (2,568 bytes, compressed), send it back and open it | The layers and objects matched the original |
| Place the `.ai` into another document | One group, with the warning "text was converted to outlines" (placed text becomes outlines) |
| Place a PNG fitted into a rectangle | Placed at the given size |

- `draw_shape` takes opacity in percent (0–100). The first run passed 0.5 by mistake (an error in the script, not in the server), which made the ellipse nearly invisible; rerun with 50.

### In a browser (Chrome)

Opened an upload URL, chose the `.ai` and sent it: the PUT returned 200, the page said "Sent. You can close this page…", and `open_upload` gave `restored: true`. Opening the URL again shows that the file has been sent.

- The automation tool's clicks did not trigger the Send button, while a click dispatched in the page did, so the page works. A click by a person is not yet verified.
- The browser tool can upload only files in folders this session can read, so the `.ai` was copied from the desktop to the session's scratchpad first.

### In Illustrator (the project owner)

The project owner opened `artserver-check.ai` in Illustrator and judged it good.

Clicking with the Selection tool showed no selection, unlike art drawn in Illustrator. With simpler files (one rectangle, and three shapes) the Layers panel showed why: the art sat in a `<Clip Group>` whose clipping path was VectorCraft's PDF layer form box (±32,767 pt, `crates/pdf/src/forms.rs`), next to an empty `<Clip Group>` made from the form's marker (an empty clip at x = −32,000). A click selected the outer group, whose selection box was far off-screen. Illustrator showed its own "レイヤー 1", not the file's "Layer 1", so it doesn't make PDF layers its own. The same drawings exported as PDF (which has no PDF layers) opened as plain `<Path>` items directly in the layer and selected normally. A rectangle still differs from one drawn in Illustrator: it is a `<Path>`, not a live `<Rectangle>`, because PDF can't say that a path is a rectangle and Illustrator's own editing data is undocumented. Hence the next entry.

The English line was set in Inter (`Inter-Regular` embedded), which is not installed on this Mac. Inter is one of the Latin families VectorCraft compiles into the program (feature `bundled-fonts`, `crates/text/src/fontdb.rs`: Source Sans 3, Source Serif 4, Inter, JetBrains Mono; nine files). ArtServer registers them before the OS fonts, so it has them on any machine, and so does the VectorCraft app; Illustrator sees only the installed fonts. The test used Inter on purpose because it is always there, but a file meant for Illustrator should use fonts installed on the client.

### Found

- The bundled fonts appear in `list_fonts` like installed ones, and Source Sans 3 (bundled) is the default when no font is given. A file using them looks right in Illustrator (the PDF part embeds the glyphs), but editing the text there reports a missing font. Options: (A) mark the bundled fonts in `list_fonts`, (B) tell the AI in the server description to use installed fonts for files people will edit, (C) change the default font. Not decided.

### Not yet verified

- Claude Code using the new tools
- A person clicking Send on the upload page
- Opening the exported `.vectorcraft` in the VectorCraft desktop app
- Real `.ai` files from Illustrator and PDFs from other apps

### Next steps

1. Decide how to handle the bundled fonts (A, B, C above)
2. Check from Claude Code in a new conversation, with real `.ai` and PDF files

## 2026-10-19 .ai without PDF layers

### What was done

- `src/tools.rs`: `export_document` passes `createLayers: false` for `.ai`. VectorCraft's `.ai` writer turns PDF layers on only when the caller doesn't say otherwise (`crates/engine/src/cmd/fileio/save.rs:435`), so VectorCraft's code is unchanged. The document's layers stay in the editing data.
- The E2E test of the `.ai` and `.vectorcraft` round trip now gives the document a second layer, checks that the `.ai` has no PDF layers (`/OCProperties`), and checks that both formats reopen with both layers.
- README (English and Japanese) and design policy section 4.6: how Illustrator opens the `.ai`, and why it has no PDF layers.
- Regenerated `ai-1-shape.ai`, `ai-3-shapes.ai` and `artserver-check.ai` in `~/Desktop/artserver-output/`. They have no PDF layers; the only form left in `artserver-check.ai` is the transparency group of the 50% ellipse.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 63 passed (45 unit, 18 E2E) |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |
| The check script against the rebuilt server | The `.ai` reopened with `restored: true` and the same layers and objects |

### Not yet verified

- The regenerated `.ai` files in Illustrator

## 2026-10-19 Checking again from the start (after the .ai change)

The project owner cleared `~/Desktop/artserver-output/`, and the checks were run again with the server built from `233ca3f`. The English line now used Helvetica Neue, which is installed on the Mac, instead of the bundled Inter.

| What was checked | Result |
|---|---|
| Fonts | Helvetica Neue and Hiragino Sans W6, both exact |
| Export `.ai` (13,073 bytes) | No warnings, no PDF layers; embedded fonts HelveticaNeue and HiraginoSans-W6 |
| Send the `.ai` back and open it | `restored: true`; the two layers and six objects, with their names, matched the original |
| Send the `.vectorcraft` back and open it | The layers and objects matched the original |
| Place the `.ai` and a PNG into another document | One group (placed text becomes outlines), and the PNG at the given size |
| Send the `.ai` from Chrome's upload page | "Sent…", then `open_upload` gave `restored: true` (the Send button was again pressed from inside the page) |
| Illustrator (the project owner) | `ai-1-shape.ai`, `ai-3-shapes.ai` and `artserver-check.ai` were fine: the shapes select one by one, and the fonts are not missing |

### Next steps

1. Decide how to handle the bundled fonts (options A, B, C in the check entry above)
2. Check from Claude Code in a new conversation, with real `.ai` and PDF files

## 2026-10-19 Marking the bundled fonts (options A and B)

### Policy

The project owner chose options A and B from the check entry above: mark the bundled fonts in `list_fonts`, and tell the AI in the server description. Option C (changing the default font) was left for later. Design policy section 7.4.

### What was done

- `src/fonts.rs` (new): the families built into the program and not installed on the OS. The built-in families come from a font database without font folders, which holds only those, so nothing is hard-coded; a built-in family counts as installed when some style of it is read from a font file (the OS or `--font-dirs`). Worked out once, on a blocking thread.
- `src/tools.rs`: `list_fonts` adds `bundled`, the listed families that are built in only (after `query`), or, with `family`, whether that family is. The tool description and the server description tell the AI to use installed families for files people will edit, and to name a font for every text (text without one is set in the built-in Source Sans 3).
- README (English and Japanese) and design policy section 7.4.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 64 passed (46 unit, 18 E2E). New: the built-in families are VectorCraft's four, nothing counts as installed without font files (unit); `bundled` names only listed families and agrees with the answer for one family (E2E) |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |
| `list_fonts` on this Mac | 379 families; `bundled`: Inter, Source Sans 3, Source Serif 4. JetBrains Mono is not marked because it is installed in `~/Library/Fonts`; Helvetica Neue and Hiragino Sans are not marked |

### Next steps

1. Check from Claude Code in a new conversation, with real `.ai` and PDF files (including whether the AI avoids the `bundled` fonts)

## 2026-10-19 Telling the AI to close working sessions

- `src/tools.rs`: the server description and the `session_close` description now tell the AI to call `session_close` when the work is done, and only once the user has the files they need, since closing discards the documents, exports and uploads and makes the download URLs stop working. Without this, working sessions mostly ended by idle disposal (30 minutes) and counted toward the limit of 8 until then (found in the review of concurrency and sessions above).
- `cargo test --offline --locked`: all 64 passed (46 unit, 18 E2E). `cargo clippy` / `cargo fmt --check`: no warnings / no changes.
- Not yet verified: whether the AI follows it (part of the check from Claude Code).

## 2026-10-19 A server-wide limit on the jobs running at once

### Policy

From the review of concurrency and sessions above: there was no server-wide limit on heavy work, so many working sessions exporting at once could take every core. The project owner chose to make the limit a setting. Every job counts, light ones included (they finish quickly, and telling heavy work apart inside VectorCraft's generic tools would be unreliable). The default follows the CPU cores. The wait for a slot counts toward the time limit of the call, as the queue wait already does. Design policy section 5.3.

### What was done

- `src/engine_host.rs`: `RunSlots`, shared by every working session's executor. A session's thread takes a slot before running a job and gives it back when the job is done; while all slots are taken, it waits, checking every 50 ms whether the caller gave up or ran out of time, and then doesn't run the job.
- `src/config.rs`: `--max-running` (`ARTSERVER_MAX_RUNNING`). The default is the CPU cores ÷ (rendering threads + 1), at least 1, because an export uses its session's thread and `--render-threads` rendering threads. `src/session.rs` and `src/lib.rs` pass the slots to the executors; `src/main.rs` logs the value at startup.
- README (English and Japanese) settings table, and design policy section 5.3.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 68 passed (50 unit, 18 E2E). New unit tests: a slot is waited for and handed over, a cancelled job stops waiting, two sessions share one slot (the second waits, a cancelled one never runs), and the setting and its default |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |
| Startup on this Mac (8 cores) | `max_running=2` in the log (8 ÷ 3) |

## 2026-10-19 The queue length as a setting

- `src/config.rs`: `--queue-depth` (`ARTSERVER_QUEUE_DEPTH`, default 8): the requests one working session can have waiting in its queue; more are refused as busy. It was fixed at 8 before (found in the review of concurrency and sessions above).
- New unit test: with a queue of one, while the session's thread waits for a slot, the second request waits in the queue and the third is refused as busy (the slot is held by the test, so the timing doesn't depend on how long the engine takes; passed 10 runs in a row). The config test covers the default and the value.
- README (English and Japanese) settings table, and design policy section 5.1.
- `cargo test --offline --locked`: all 69 passed (51 unit, 18 E2E). `cargo clippy` / `cargo fmt --check`: no warnings / no changes.

## 2026-10-19 Opening the first batch of document commands

### Inventory

Read every `cmd!` declaration in VectorCraft's engine (`crates/engine/src`) with a script: 763 commands, all with distinct ids, 159 of them read only (`query`). All 75 allowlisted commands were among them. By the categories of design policy section 4.7: "opened step by step" 558 (69 open, 43 of the rest read only), "not opened" 106, "decide later" 48, ArtServer's tools 4, and 47 in namespaces the section doesn't classify yet (`document`, `color`, `eyedropper`, `image`, `importMatches`, `magicWand`, `node`, `shaper`, `slice`). The full list went to `~/Desktop/artserver-output/vectorcraft-commands.csv` for the project owner. (Earlier notes said about 160 read-only commands could be opened early; that was all categories together.)

### Policy

The project owner chose a first batch: the read-only commands of "opened step by step", plus artboards, transparency, gradients, appearance, swatches, symbols, and clipping masks, compound paths and blends. Each of the 131 candidates' parameter descriptions was read. 11 were left out: `artboard.copy` and `artboard.cut` (useful only with paste, part of the clipboard commands left for later), `graphicStyle.libraries` and `graphicStyle.library` (the user library folder), `perspective.presets.*` (file paths) and `perspective.widget.options` (screen only; perspective is for a later batch), and `transparency.viewOpacityMask` (screen only). The preferences some commands set (`appearance.setNewArtBasic`, `graphicStyle.setOptions`, the new-mask toggles) live in the engine's `Session` (`s.prefs`, `s.menu`), so they stay within one working session. `prefs.set` itself stays closed: applying preferences also sets process-wide state (the user font folders, hyphenation exceptions and rendering threads; `crates/engine/src/cmd/prefscmds.rs`).

### What was done

- `src/policy.rs`: 120 commands added to the allowlist, grouped with comments; 195 in all. `dataset.export` is open only as data (a string `path` is refused like every path).
- Tests: the left-out commands are refused, and `dataset.export` with a path is refused (unit); one command of each group runs through `command_run` as the AI runs them: a new artboard and its properties, opacity 50% (read back with `transparency.info`), an added fill, a new swatch listed by `swatch.list`, a symbol made and placed, a blend, a compound path, `guide.list` and `graphicStyle.list`; the left-out ones answer "not allowed" (E2E). The existing test checks that every allowlisted command exists in the engine.
- Design policy section 4.7 and chapter 11.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 70 passed (51 unit, 19 E2E) |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |

## 2026-10-19 Opening the second batch of document commands

### Policy

The project owner wants as many commands open as possible, in batches: second, the rest of "opened step by step"; third, the 47 unclassified; fourth, the 48 "decided individually later". For the second batch, the parameter descriptions of all 369 remaining commands were read, and four points were checked in VectorCraft's code:

| Point | Finding |
|---|---|
| Copy and paste (`edit.copy`, `edit.paste*`, `artboard.copy`/`cut`) | The clipboard is a field of the engine's `Session`, and the engine depends on no system clipboard library, so it stays within one working session |
| `perspective.draw` runs another command | The engine accepts only `shape.*` commands (`distortcmds.rs:1241`), which only draw |
| `object.rasterize` could make a huge image | The engine refuses results over 64 million pixels or 65,535 pixels on a side (`menucmds.rs:438`) |
| `colorTheme.save` and `recolor.apply` mention libraries | Themes are kept in the session's preferences; `recolor.apply` only snaps colours to a swatch library, and the engine can't read files outside its empty folder |

357 were opened and 12 left closed: `text.rescanFonts` (rescans the system fonts for the whole process), four graphic style library commands (the user library folder), and seven that only change the screen or the tool (`transparency.viewOpacityMask`, `perspective.widget.options`, `perspective.grid.show`/`rulers`/`lock`/`lockStation`, `object.intertwine.edit`). `dataset.import` and `perspective.presets.import`/`export` work only with data, since the path check refuses a string path.

### What was done

- `src/policy.rs`: the 357 commands, generated from the inventory and grouped by namespace with the reasons for what stays closed; the allowlist has 552.
- Tests: the 12 closed ones are refused, and the four data-only commands are refused with a path and allowed as data (unit). A new E2E test runs, through `command_run`: find and replace, copy and paste, selecting the same fill colour, reversing a path, offsetting a path, an arc envelope, a radial repeat, a spiral, duplicating a layer, saving and exporting perspective presets as data (a file path is refused), and checks that four of the closed ones answer "not allowed". The first batch's E2E test no longer expects `artboard.copy` and `perspective.presets.export` to be refused.
- Design policy section 4.7 and chapter 11.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 71 passed (51 unit, 20 E2E). The first run failed in the new E2E test because there were no saved presets to export (an error in the test); saving one first fixed it |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |

## 2026-10-19 Opening the third batch: the unclassified namespaces

### Policy

The 47 commands of the namespaces design policy section 4.7 didn't classify (`color`, `document`, `eyedropper`, `image`, `importMatches`, `magicWand`, `node`, `shaper`, `slice`) were read in full; 2 of them (`document.inspect`, `document.node`) were already open. The project owner agreed to open 23 and leave 22 closed:

| Left closed | Why |
|---|---|
| The exports and saves (15): `document.exportPdf`, `exportEps`, `exportDxf`, `exportForOffice`, `exportForScreens`, `exportForWeb` (and its preview), `exportSelection`, `document.save`, `document.serialize`, the export presets and settings, `document.pdfSettings` | Exports belong to `export_document` (section 4.7); without a path these return the file as Base64 in the reply, which floods the AI's conversation |
| `document.pdfInfo`, `document.dxfInfo` | They read a file given as data in the arguments; files come in through uploads |
| `document.formats` | It lists formats this server doesn't accept |
| `document.json` | The whole document model, which embedded images can make very large; `document.inspect` and `document.node` cover it |
| `importMatches.*` (3) | For importing other apps' shortcut sets, which stay closed |

Opened: `color.convert`, `gamutCheck`, `harmony`, `plates`; `document.activate` (a working session can now switch between its documents), `document.find`, `document.info` (its fontDetails list each font's file on the server, accepted for users within one organization), `document.setup`, `setUnits`, the ruler origin commands, `rasterEffectsSettings`; `eyedropper.setOptions`, `magicWand.options`/`set`; `image.info`, `node.move`, `shaper.*` (4), `slice.list`.

### What was done

- `src/policy.rs`: the 23 commands, with the reasons for what stays closed; the allowlist has 575.
- Tests: the 22 closed ones are refused (unit). A new E2E test opens two documents and switches back to the first with `document.activate` (`doc_inspect` then shows its 300 pt artboard), converts a colour to CMYK, sets the bleed, finds a rectangle, sets the Magic Wand, moves a row with `node.move`, lists slices, and checks that five of the closed ones answer "not allowed".
- Design policy section 4.7 and chapter 11.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 72 passed (51 unit, 21 E2E). The first run failed at `node.move`, which the test called without `parent` (that means the top level, where only layers can go); passing the layer fixed it |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |

### Next steps

1. The fourth batch: the 48 "decided individually later" commands
2. Add to `export_document` what the closed exports offer (PDF/X and other PDF options, printer's marks, bleed, exporting the selection, EPS, DXF), and `document.pdfInfo` as a tool that takes an upload
3. Check from Claude Code in a new conversation

## 2026-10-19 Opening the fourth batch: the commands decided individually

### Policy

The 48 "decided individually later" commands of design policy section 4.7 (`assets`, `clipboard`, `css`, `flattener`, `imageTrace`, `pdf.preset`, `svgInteractivity`, `webExport`) were read in full, and three points were checked in the engine's code: Image Trace scales down images over 64 megapixels and caps the anchors it makes; its `library` parameter looks up only the built-in swatch libraries and those loaded into the working session (none, while `swatch.library.*` is closed), so nothing is read from disk; `css.selection` only names the pictures of rasterized art. The project owner agreed to open 19 and leave 29 closed:

| Left closed | Why |
|---|---|
| `clipboard.importPdf`, `importImage`, `importSvg`, `importEps`, `importEmf` | They read a file given as data in the arguments; files come in through uploads, which don't accept EPS, EMF, TIFF or SVGZ |
| `clipboard.exportPdf`, `exportPng`, `exportSvg`, `css.export` | They put an export in the reply (PNG up to 64 times); exports belong to `export_document` |
| `clipboard.flavours`, `imageTrace.setView` | The formats handed to the system clipboard, and the screen only |
| `svgInteractivity.setEvent`, `addFile`, `removeFile` | They write JavaScript into exported SVG, which runs when a user opens the file in a browser; an AI led on by a malicious instruction could plant a script (`addFile`/`removeFile` take a file string the path check refuses anyway) |
| `assets.*` (6), `pdf.preset.*` (5), `webExport.*` (4) | The setup of exports that stay closed (Asset Export, the detailed PDF options, Save for Web); of no use until `export_document` takes those features, and opened with them |

Opened: `imageTrace.make`, `makeAndExpand`, `expand`, `release`, `presets` (an image placed through an upload can be traced into paths); `flattener.presets.list`/`save`/`delete`/`import`/`export` (the working session's preferences, as data only) and `flattener.preview`; `css.selection`/`generate`; `clipboard.conflicts`/`importText`/`exportText`; `svgInteractivity.info`/`removeEvent`/`clearEvents` (the event scripts an imported SVG brings can be read and removed).

### What was done

- `src/policy.rs`: the 19 commands, with the reasons for what stays closed; the allowlist has 594.
- Tests: the 29 closed ones are refused, and the flattener presets refuse a path but work as data (unit). A new E2E test imports SVG with an `onclick` script, reads it with `svgInteractivity.info` and removes it with `clearEvents` (the exported SVG no longer has it), gets a rectangle's CSS, pastes text through `clipboard.importText`, saves and exports a flattener preset (a path is refused), previews flattening, uploads and places a PNG and traces and expands it with `imageTrace.make`/`expand`, and checks that eight of the closed ones answer "not allowed".
- Design policy section 4.7 and chapter 11.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 73 passed (51 unit, 22 E2E). The first run failed in the test itself, which read `document.find`'s result under the wrong key; it now finds the rectangle by its SVG id |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |

### Next steps

1. Add to `export_document` what the closed exports offer (PDF/X and other PDF options, printer's marks, bleed, exporting the selection, EPS, DXF), opening `pdf.preset.*` with them, and `document.pdfInfo` as a tool that takes an upload
2. Check from Claude Code in a new conversation

## 2026-10-19 Not extending export_document for now

### Policy

A plan to extend `export_document` was drafted: jpg, eps and dxf; choosing artboards, the selection and the crop; the main PDF options (presets, PDF/X, compatibility, bleed, printer's marks, outlined text) with `pdf.preset.*` opened; a download URL per file when an export writes several; and `inspect_upload` built on `document.pdfInfo`. The engine's `document.export` already takes all of these, so no closed command would have to be opened for them.

The project owner judged these not much needed by the server, and the extension is not made for now (design policy section 4.7):

- ArtServer is for having an AI make art within an organization; finishing files for print is a designer's work in Illustrator, which the `.ai` and PDF exports already serve. EPS and DXF have few uses.
- Open commands cover the common needs: `artboard.reorder` to export another artboard as PNG, `artboard.fitToSelection` to export only some art, `document.setup` for the bleed.
- Every option added to a tool is sent to the AI with its description on every call, costing context and giving it more to choose wrongly from.

The closed exports and their setup stay closed; what real use shows to be missing is added then, and only that.

### What was done

- Design policy section 4.7 and chapter 11 (the command opening is done; the extension is not made for now).

### Next steps

1. Check from Claude Code in a new conversation (`RUST_LOG=debug`): opening and placing, through uploads, real files from other apps (`.ai` files saved by Illustrator and PDFs from other apps; ArtServer's own `.ai` and PDF exports were already checked in Illustrator), whether the AI finds and uses the opened commands, prefers installed fonts to the `bundled` ones and calls `session_close`, and the MCP protocol version it uses. Note what turns out to be missing

## 2026-10-19 Not working on CJK fonts on Linux for now

The project owner decided not to work on CJK fonts (such as Japanese) on Linux for now (design policy section 7.3): Linux is expected to have few users. An operator can already provide them, by installing them in the server's OS or with `--font-dirs`; embedding fonts in the build is not done. Revisited when users on Linux appear.

- Design policy section 7.3, chapter 11 (Phase 2's remaining work is now the verification with real files) and section 13.2 #5.

## 2026-10-19 The time limit counts from when a request starts

### Policy

The time limit of a call (`--call-timeout-secs`, default 60) counted from the moment the tool was called. It so included the wait behind the working session's earlier requests and for a free slot of `--max-running`: a request could run out of time without starting, or start with seconds left and be stopped partway, and the AI got the same "timed out" message either way, so it couldn't tell whether the document had changed.

Of three options (A: separate the wait from the run, with the same length for both; B: as A, with a setting of its own for the wait; C: only separate the messages), the project owner chose A. The settings stay as they are; a call can take up to twice the time limit before it answers.

### What was done

- `src/engine_host.rs`: the time limits moved from the tools into the executor. `submit` waits for the job to start, up to the time limit; once it starts, it waits for the result, up to the time limit again. A job that doesn't start in time is not run, and the AI is told "the server stayed busy for 60 s, so the request was not started; nothing was changed. Retry later"; one that runs past the limit gets "timed out after 60 s; the operation was cancelled" as before. Whether the job started or its caller stopped waiting is settled once (`Start`, a `OnceLock`): the executor's thread claims it after taking a slot, the caller when it stops waiting, and whichever comes first wins, so a job is never run after its caller was told it wasn't.
- `src/tools.rs`: the tools keep only the client's cancellation. `src/lib.rs`, `src/config.rs`: the time limit goes to the executor with the other engine limits.
- Tests (unit): a job that can't start in time is refused and never runs (with one document allowed, a document can still be made afterwards); with an executor the test plays itself, a job that waits 250 ms and runs 250 ms under a 400 ms limit succeeds, one that runs past the limit is asked to stop, and an executor that comes to a job after its caller stopped waiting can't start it.
- README (both languages) and design policy sections 5.1 and 5.3.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 77 passed (55 unit, 22 E2E). A first test, a real export after a long wait for a slot, would have passed under the old limit too (the export takes milliseconds), so it was replaced with the played executor |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |

### Next steps

1. Check from Claude Code in a new conversation (`RUST_LOG=debug`): opening and placing, through uploads, real files from other apps (`.ai` files saved by Illustrator and PDFs from other apps; ArtServer's own `.ai` and PDF exports were already checked in Illustrator), whether the AI finds and uses the opened commands, prefers installed fonts to the `bundled` ones and calls `session_close`, and the MCP protocol version it uses. Note what turns out to be missing

## 2026-10-19 Bringing architecture.md up to date

`docs/architecture.md` is the Phase 0 investigation record. Its findings are kept as they were, and what was verified or decided later is added after "→" with its date: the nine open issues of chapter 6 (all settled), Rust installed, the license check, the build without `[patch.crates-io]` (not tried; the patch is copied), the use of `vectorcraft-mcp`'s tool part, and the VectorCraft commit now referenced.

- The build size was measured for the first time: `target/` is 11 GB (`du -sh`), against "a few GB" expected in Phase 0. It holds only debug builds, made over and over with tests and clippy; a clean build and a release build were not measured.

## 2026-10-19 A test checks that the PDF backend is the fixed version

### Policy

Design policy chapter 8 said that CI checks that the PDF backend is the fixed version, but CI only runs `cargo build --locked`, and nothing checked it. The fixed versions come from craft-libs through `[patch.crates-io]` in `Cargo.toml`, since Cargo doesn't carry VectorCraft's over. If that stops applying, say after VectorCraft moves to another craft-libs revision, Cargo only warns and takes the crates.io versions, which lack the limits on broken PDFs (nesting depth, decode limits, the page tree's work budget, no partial documents). VectorCraft's code was read on this: craft-pdf refuses to build with an unfixed krilla (the writer; `crates/pdf/src/write.rs:12` in craft-libs `ca50d63` reads a constant only the fixed krilla has), but nothing stops an unfixed reader (hayro-syntax, hayro-interpret, hayro-jbig2). VectorCraft checks this with `cargo xtask pdf-deps`.

Of three options (A: correct the design policy and check `Cargo.lock` by hand when updating VectorCraft; B: a test; C: a step in CI), the project owner chose B. Updating VectorCraft always goes through the tests, and the other checks of a VectorCraft update are tests too. The test compares with the revision craft-pdf comes from, not with a revision written in the test, so it needs no change when VectorCraft is updated.

### What was done

- `tests/pdf_backend.rs` (new): reads `Cargo.lock` and checks that craft-pdf comes from craft-libs and that krilla, hayro-syntax, hayro-interpret and hayro-jbig2 each resolve once, from the same source. A second test feeds it made-up lockfiles: each crate from crates.io, from another revision, twice, or missing, and craft-pdf from crates.io, are all refused. No dependency was added.
- Design policy chapter 8, README (both languages), and the Phase 0 recommendation in `docs/architecture.md` section 4.5.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 79 passed (55 unit, 22 E2E, 2 for the PDF backend) |
| The test against changed copies of `Cargo.lock` (hayro-syntax from crates.io; krilla from another craft-libs revision) | Both failed, naming the crate and the source found. The test was then put back to read `Cargo.lock` |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes |

### Next steps

1. Check from Claude Code in a new conversation (`RUST_LOG=debug`): opening and placing, through uploads, real files from other apps (`.ai` files saved by Illustrator and PDFs from other apps; ArtServer's own `.ai` and PDF exports were already checked in Illustrator), whether the AI finds and uses the opened commands, prefers installed fonts to the `bundled` ones and calls `session_close`, and the MCP protocol version it uses. Note what turns out to be missing

## 2026-10-19 An E2E test of older MCP clients

### Policy

Design policy section 4.4 says that clients of the versions before 2026-07-28 (2025-06-18 and 2025-11-25, which start with `initialize` and keep `Mcp-Session-Id` if the server issues one) are supported, and section 13.2 #1 listed this as not yet verified. rmcp 3.5.1's code was read on how it serves them with `with_legacy_session_mode(false)` (`src/transport/streamable_http_server/tower.rs`): every request takes the stateless path; `initialize` answers with the version asked for when the server supports it (`negotiate_protocol_version`); no session ID is issued; notifications get 202; an `MCP-Protocol-Version` header must name a known version (none means 2025-03-26); GET gets 405. The test sends the exchange by hand, so it doesn't rest on rmcp's client, the same SDK as the server.

Reading this showed that rmcp's client, which the other E2E tests use, also starts with `initialize` (`ServiceExt::serve` keeps the legacy lifecycle): it asks for 2026-07-28, which has no `initialize`, and the server settles on 2025-11-25 (checked with a throwaway test). So 2025-11-25 was already exercised through rmcp's client, and the version no test covers is 2026-07-28 (`server/discover` and per-request metadata).

### What was done

- `tests/e2e.rs`: `older_clients_work_without_protocol_sessions`, for 2025-06-18 and 2025-11-25: `initialize` (the version comes back as asked, the server name, the tools capability), `notifications/initialized` (202), `tools/list` (the 25 tools, in one page), and tool calls each in a request of their own: `session_create`, `document_new`, `draw_shape`, `export_document` (the SVG has the rectangle's colour) and `session_close`. Every answer is plain JSON with no `Mcp-Session-Id`, and GET `/mcp` gets 405. The helper `rpc` sends one request and checks the form of the answer.
- Design policy sections 4.4 and 13.2 #1.

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 80 passed (55 unit, 23 E2E, 2 for the PDF backend) |
| The new test against a server with `with_legacy_session_mode(true)` (changed for the run, then put back) | Failed at the check that no session ID is issued |
| A throwaway test printing the version rmcp's client settles on (deleted) | Asked for 2026-07-28, settled on 2025-11-25 |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes, after `cargo fmt` reflowed the new test |

### Next steps

1. Check from Claude Code in a new conversation (`RUST_LOG=debug`): opening and placing, through uploads, real files from other apps (`.ai` files saved by Illustrator and PDFs from other apps; ArtServer's own `.ai` and PDF exports were already checked in Illustrator), whether the AI finds and uses the opened commands, prefers installed fonts to the `bundled` ones and calls `session_close`, and the MCP protocol version it uses. Note what turns out to be missing
2. Not decided: a test of the 2026-07-28 version (`server/discover` and per-request metadata), with rmcp's client in its discover mode (`ClientLifecycleMode::Discover`) or by hand

## 2026-10-19 Not supporting Docker

The project owner decided that ArtServer doesn't support Docker: no Dockerfile or container image is provided, and Docker is no longer part of Phase 3. Process isolation (design policy chapter 12) doesn't depend on it, since isolation by OS process and Docker containers are different things (section 12.1).

- Design policy chapter 11 (Phase 3) and sections 12.1 and 12.6; `docs/DEVELOPMENT_PLAN.md` sections 2 and 5 (the decision added after "→"). The external review `docs/ARTSERVER_PROCESS_ISOLATION_REVIEW.md` is left as written.

## 2026-10-19 Not switching color settings per job for now

The project owner decided that switching color settings per job (design policy section 13.2 #8; for example a Japan Color profile for one print job and another for the next) waits; it was listed for Phase 2. The engine keeps one set of color settings for the whole process (`crates/color/src/cms/mod.rs:892`), and even `edit.assignProfile` on one document replaces it (`crates/engine/src/cmd/colormgmt.rs:264`), so the commands stay closed and every working session uses the defaults: RGB sRGB and CMYK "VectorCraft Generic CMYK (SWOP-like)", the engine's own model. VectorCraft has no Japan Color built in, and ICC profiles can't be loaded (`color.loadProfile` is closed). Revisited when print work needs CMYK conversion per printing condition on the server, which would bring process isolation forward (section 12.4).

- Design policy section 13.2 #8.

## 2026-10-19 Not returning exports as resource links for now

The project owner decided that returning an export's download URL as an MCP resource link (design policy section 13.2 #11; a `resource_link` block in the tool result, which tells the client app that it is a file) waits; it was listed for Phase 2. Today the URL is in the tool's JSON result (`src/tools.rs`, `deposit`), and with Claude Code the AI saves the file with curl (verified 2026-10-18). Claude Code's MCP documentation and changelog were searched, and nothing was found on how it handles a resource link: whether it shows the model the URI, fetches the file itself, or shows the user a link. The MCP specification lets a client fetch one with `resources/read`, which ArtServer doesn't implement. A resource link might help clients that can't run curl; it would be tried when such a client is needed.

- Design policy sections 4.6 and 13.2 #11.

## 2026-10-19 Counting the commands at runtime, and opening the text styles

### Policy

Design policy section 13.2 #12 listed as not yet verified the count of the engine's commands, about 760, counted from the source. The engine lists its commands in `vectorcraft_engine::command_specs()`, and a throwaway test (deleted) counted 779 there, 161 of them read only, in 62 namespaces, against the 763, 159 and 60 of the inventory behind the four batches. The 16 missing are the character and paragraph styles (`charStyle.*` and `paraStyle.*`: `list`, `new`, `apply`, `redefine`, `setAttrs`, `duplicate`, `rename`, `delete`): `crates/engine/src/cmd/textstyles.rs` builds their ids with `concat!` in a macro of its own, and its comment says why ("the `cmd!` macro wants literals"), so reading the `cmd!` declarations didn't find them. They were closed, as everything off the allowlist is, but had never been classified. All 594 allowlisted ids exist at runtime.

Their code was read: the styles are kept in the document (`char_styles` / `para_styles` of `Document`), they take the attributes `text.setStyle` / `text.setFormat` take (both open) and refuse unknown ones, and they touch no files and no process-wide state; the two `FontDb::global().add_font` calls in the file are in its tests. The project owner agreed to open all 16. Illustrator reads only the PDF part of an `.ai`, so the styles most likely don't show in its Styles panels (not tried).

### What was done

- `src/policy.rs`: the 16 commands, with why the inventory missed them; the allowlist has 610.
- `tests/e2e.rs`: `text_styles_work_through_command_run`, for both kinds: a style is made, applied to a text, changed with `setAttrs` and taken from the text with `redefine`, duplicated, renamed and deleted; `list` shows it in use with the new attribute and without the deleted copy, and an unknown attribute is refused. The exported SVG has the character style's size (30, against the text's own 12).
- Design policy section 4.7, chapter 11 and section 13.2 #12.

### Commands run and results

| Command | Result |
|---|---|
| A throwaway test counting `command_specs()` and comparing the ids with the `cmd!` declarations (deleted) | 779 at runtime, 763 declared with `cmd!`; the 16 others are `charStyle.*` and `paraStyle.*`; no declared id is missing at runtime |
| `cargo test --offline --locked` | All 81 passed (55 unit, 24 E2E, 2 for the PDF backend) |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes, after `cargo fmt` reflowed the new test |

### Next steps

1. Check from Claude Code in a new conversation (`RUST_LOG=debug`): opening and placing, through uploads, real files from other apps (`.ai` files saved by Illustrator and PDFs from other apps; ArtServer's own `.ai` and PDF exports were already checked in Illustrator), whether the AI finds and uses the opened commands, prefers installed fonts to the `bundled` ones and calls `session_close`, and the MCP protocol version it uses. Note what turns out to be missing
2. Not decided: a test of the 2026-07-28 version (`server/discover` and per-request metadata)

## 2026-10-19 Upstream fixed the Japanese font weights in `main`

The issue ArtServer reported, [storytold/vectorcraft#2873](https://github.com/storytold/vectorcraft/issues/2873) (the weights W0–W9 of Japanese fonts don't become SVG `font-weight`), was closed when [PR #2896](https://github.com/storytold/vectorcraft/pull/2896) was merged into upstream `main` on 2026-10-18 at 12:20 UTC, after the v0.13.1 release ArtServer pins (04:36 UTC). It is not in a release yet: the latest is still v0.13.1, and `main` (`a466625`) is 207 commits ahead of it with the same version number. ArtServer follows releases (design policy chapter 8), so `vendor/vectorcraft-svg` stays until the first release that includes the fix.

The fix (`eec304d`) is the root fix the report suggested: `style_weight` in `vectorcraft-text` reads W0–W9 as the digit × 100, at least 100, the same weights as ArtServer's patch, so SVG output should not change when the patch is removed. Unlike the patch, it also changes font selection: when no face has the exact style name, the engine takes the nearest weight by `style_weight` (`crates/text/src/fontdb.rs:2250` at v0.13.1), which now knows the W weights. That is checked when moving to that release.

- `vendor/README.md` and `vendor/README.ja.md`; design policy chapter 8.

## 2026-10-21 Measuring what one ArtServer process costs

### Policy

The project owner asked whether process isolation (design policy chapter 12) is worth having for a server that keeps running when one job fails, and agreed to decide after measuring what one process costs and after the check with real files from other apps. One process per session is the first candidate (section 12.3), and section 12.5 named the font initialization cost as a concern. The measurement was made from outside: scripts kept outside the repository started the release build and called it over HTTP, with the MCP requests of the README's curl example (`MCP-Protocol-Version: 2025-06-18`). Nothing in the repository was changed.

### Environment

| Item | Details |
|---|---|
| Machine | Apple M3, 8 cores, 24 GB of memory, macOS 26.7.1 |
| Build | `target/release/artserver` at `8966b51`, default settings |
| Fonts | 432 font files in the folders VectorCraft scans (`/System/Library/Fonts` 370, `/System/Library/AssetsV2/com_apple_MobileAsset_Font8` 59, `~/Library/Fonts` 3, `/Library/Fonts` none); `list_fonts` lists 379 families |
| Memory figures | RSS from `ps`, and the maximum RSS and peak memory footprint from `/usr/bin/time -l` |

### Results

Time, in three runs (the first with the disk cache cold):

| Item | Result |
|---|---|
| From starting the process to `/healthz` answering | 23 ms, then 6 ms and 6 ms |
| From starting the process to the first `list_fonts` answering. The font scan starts in the background at startup (`src/lib.rs:139`), and the first lookup waits for it | 161 ms, then 39 ms and 40 ms |
| A second `list_fonts` | 1 ms |
| A document with 20 ellipses and one Japanese text (Hiragino Sans) | 96 ms, then 13 ms and 13 ms |
| Exporting it as PNG / PDF | About 20 ms / 2–4 ms |

Memory of one session, step by step (RSS, MiB; putting the three fonts in the other order changed them little):

| Step | Increase | Total |
|---|---|---|
| Started, with one session | | 18.1 |
| `list_fonts` | +2.1 | 20.2 |
| `document_new` and 20 ellipses | +1.8 | 22.0 |
| Text in Inter (built in) | +0.9 | 22.9 |
| Text in Helvetica Neue | +4.4 | 27.3 |
| Text in Hiragino Sans | +57.0 | 84.3 |
| The first rendering (`render_preview`) | +74.3 | 158.6 |
| Two PNG exports | +1.6 | 160.2 |
| PDF export | +14.7 | 174.9 |
| SVG export, `session_close` | +0.4 | 175.3 |

- The first rendering adds about 74 MiB without any text too (a run with the ellipses only: +73.8 MiB, 101 MiB in all after the exports). What it holds was not investigated.
- Hiragino Sans adds the size of its ten files, W0 to W9 (56.5 MiB): the engine reads every installed file of a family into memory the first time the family is used (`load_family_files`, `crates/text/src/fontdb.rs:2118`; files are read with `std::fs::read`, `:1021`). Each further family used adds its files likewise.
- In one process, sessions share the fonts and the rendering caches: seven more sessions doing the same work (a document with ellipses and Japanese text) added 0.15 MiB each. Eight such sessions: 161 MiB at most (RSS), peak memory footprint 147 MiB. The footprint is close to the RSS, so this memory is the process's own, not file pages other processes could share. The RSS didn't go down after `session_close`.

What this means for process isolation:

- Starting a process and scanning the fonts cost tens of milliseconds on this Mac, so the font initialization cost section 12.5 worried about is small here. A machine with many more fonts, Linux and Windows were not measured.
- Memory is the cost. With one process per session, each process pays the fonts and the rendering caches itself: about 100 MiB once it renders, about 160–175 MiB with Japanese text and a PDF export. Eight sessions would take about 1.3 GB, against about 160 MiB in one process today.
- The document was small. The memory of large documents read from real files grows the same way in both forms.

### What was done

- Design policy section 12.5: the measured cost of starting a process, the font scan and memory per process. Section 12.3: the table's costs of each isolation unit now rest on memory, not on the startup cost.

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --release --offline --locked` | Built in 3 min 47 s |
| A script measuring startup, the font scan, the document, the exports and eight sessions (three runs) | The figures above |
| A script printing the RSS after each step (the three fonts in two orders, and once without text) | The figures above |

### Next steps

1. Check from Claude Code in a new conversation (`RUST_LOG=debug`): opening and placing, through uploads, real files from other apps (`.ai` files saved by Illustrator and PDFs from other apps; ArtServer's own `.ai` and PDF exports were already checked in Illustrator), whether the AI finds and uses the opened commands, prefers installed fonts to the `bundled` ones and calls `session_close`, and the MCP protocol version it uses. Also note whether reading a file makes the server crash or hang, how long it takes and the most memory it uses. Note what turns out to be missing
2. Not decided: process isolation, after step 1. Discussed: a startup option choosing between today's threads and worker processes, with today's form as the default
3. Not decided: a test of the 2026-07-28 version (`server/discover` and per-request metadata)

## 2026-10-21 Endurance test with hostile files

### Policy

To see how close the current form (one process, a thread per working session) comes to the crashes and hangs process isolation guards against (design policy sections 12.1 and 12.5), hostile files were sent through uploads, as an AI or a person could send them. Each file went through `upload_create`, a PUT, `open_upload`, and when it opened, `render_preview` and a PNG export; the steps stop at the first that fails. After each file, another working session made a document, drew a rectangle and exported SVG, to see whether the server still served others. A watchdog in the script killed the server when its RSS went over 4 GiB (8 GiB in the follow-up), so the Mac wouldn't start swapping; that counts as running out of memory. The scripts and files were kept outside the repository, and nothing in it was changed.

### Environment

| Item | Details |
|---|---|
| Machine | As in the entry before (Apple M3, 8 cores, 24 GB, macOS 26.7.1) |
| Build | `target/release/artserver` of `8966b51` (the later commits changed documents only), default settings: a call's time limit 60 s, `--max-running` 2, upload limit 64 MiB |
| Memory figures | RSS sampled every 50 ms, so short peaks may be higher. The server keeps the memory a file took (see below), so an increase is counted from the RSS before the file |

### Results

No file crashed or hung the server, and the server log has no panic. After every file the other session's job finished at once, so no run slot was left held either.

Refused by VectorCraft's limits:

| File | Size | Answer of `open_upload` |
|---|---|---|
| SVG with 100,000 nested groups | 700 KB | "the SVG's elements are nested more than 256 deep" |
| SVG with entities expanding to 10^9 words (billion laughs) | 886 B | "a possible entity reference loop is detected" |
| SVG whose `<use>` elements fan out to 10^9 rectangles / refer to each other in a cycle | 3 KB / 207 B | "SVG data parsing failed cause nodes limit reached" |
| SVG with 1,000,000 rectangles | 44 MB | The same nodes limit, after 0.9 s and +1.2 GB. A large SVG made in good faith would be refused too |
| PDF whose content stream inflates to about 1 GB (80 million rectangles) | 2 MB | "a stream in it would decode to more than 256 MB", after 0.5 s and +0.6 GB |
| PDF with an image claiming 40,000 × 40,000 RGB (4.8 GB decoded) | 4.7 MB | The same 256 MB limit, after 0.7 s; the peak was 1.9–3.2 GB above the RSS before |
| PDF whose Form XObjects draw each other 16 times over 8 levels (16^8 draws) | 5 KB | "PDF content is incomplete or exceeds the reader's limits", after 9.4 s |
| PDF with 200,000 nested arrays / 2 MB of random bytes after the header | 400 KB / 2 MB | "PDF parser rejected the document: Invalid" |
| PNG claiming 2^31 × 2^31 pixels | 69 B | "Memory limit exceeded" |
| `.vectorcraft` unpacking to 1 GB | 1 MB | "the compressed file unpacks to more than VectorCraft reads (1024 MB of document, 1024 MB per image)", after 0.5 s and +0.9 GB |
| `.vectorcraft` with 100,000 nested JSON arrays | 200 KB | "missing field `format`": the file lacked a required field, so how far the nesting goes in a complete file was not tried; reading the nesting itself did no harm |

Opened, with the limits applying later:

| File | Result |
|---|---|
| PNG of 60,000 × 60,000 pixels (zeros; 437 KB) / JPEG claiming 65,000 × 65,000 | Opened with the warning "an image of … pixels is too large to draw"; the preview worked; the PNG export was refused: "raster exports are limited to 32768 pixels per side and 268 megapixels" |
| SVG of 1,000,000 × 1,000,000 points with a blur of 100,000 | Opened; the preview was refused: "artboard is too large for a bounded preview" (the export was not tried after that) |
| PDF of 20,000 pages | Opened in 0.6 s, one artboard a page (+130 MiB); preview and export worked |
| PDF with a Form XObject that draws itself / a page tree that contains itself and claims 10^9 pages | Opened, previewed and exported normally |
| SVG filling 2,000 × 2,000 points with a pattern of 0.001-point tiles | Previewed in 0.07 s, exported in 0.13 s |

Memory is the weak point:

- **A refused file can still take gigabytes before it is refused**, as the table shows: the 4.7 MB PDF with the image up to 3.2 GB, the 1 MB `.vectorcraft` 0.9 GB, the 44 MB SVG 1.2 GB. These are small files, so the upload limit (64 MiB) doesn't stop them.
- **A file nothing refuses can take as much**: an SVG with one stroked path of 2,000,000 segments (17.5 MB) opened in 0.3 s and took 2.8 GB to preview and export (2.1 s and 2.2 s), on a fresh server. Memory grows with the segments: 500,000 took 830 MB.
- **The memory a file took stays with the process.** After the PDF with the image, on a fresh server, the RSS stayed at 2.2 GB and was the same 20 seconds later; `vmmap` put 1.8 GB of it in `MALLOC_REALLOC (empty)` and 0.3 GB in `MALLOC_LARGE (empty)`: freed memory that macOS's allocator keeps for reuse. It is reused, not leaked, but the server stays at the highest peak it has reached.
- In the run over all the files, the memory kept from earlier files (3.0 GB) and the 2,000,000-segment path's preview together went over the 4 GiB watchdog, which killed the server. This Mac has the memory to finish it, and on its own the file finishes; on a server with 2–4 GB, it would end the whole process and every working session.

What this means for process isolation:

- VectorCraft's own limits already stop the crashes and hangs tried here.
- Memory is what one process can't bound: a single upload can take gigabytes, and the process keeps them. A worker process could be given a memory limit and stopped alone, and its memory goes back to the system when it ends.

Not tried: EPS (uploads don't take it), real `.ai` files (step 1 of the next steps), Linux and Windows, and two such files at once (with two run slots their peaks can add up).

### Commands run and results

| Command | Result |
|---|---|
| A script writing the 21 hostile files | 69 B to 44 MB each |
| A script sending each file through uploads to one server, with the other session's job after each | No crash or hang; the server was killed once by the 4 GiB watchdog, at the 2,000,000-segment path |
| Follow-ups: the paths of 100,000 to 2,000,000 segments (8 GiB watchdog, also each on a fresh server), the image files' warnings, and `vmmap -summary` after the bombs | The figures above |

### Next steps

1. Check from Claude Code in a new conversation (`RUST_LOG=debug`), as in the entry before, including the time and the most memory real files take
2. Not decided: process isolation, after step 1. The test points to memory: a limit per worker, and memory returned when a worker ends
3. Not decided: a test of the 2026-07-28 version (`server/discover` and per-request metadata)

## 2026-10-21 A shorter Linux CI

### Policy

The project owner asked for the Linux build on GitHub Actions to take less time, after asking ChatGPT, which suggested three changes: `cargo check --locked` on ordinary pushes, `cargo build --locked` when needed, and a lower optimization level for CI only. The runs and their logs were read first:

- A run took 6 to 8 minutes: 45 s restoring the cache and about 6 minutes building. Of the 42 commits since 2026-10-18, 20 changed only documents, and each still built for 6 minutes.
- Even with the cache, every run compiled `vectorcraft-svg`, `vectorcraft-engine`, `vectorcraft-mcp` and `artserver` (run 39547554230). Swatinem/rust-cache doesn't keep crates that live in the repository, and `vendor/vectorcraft-svg` is one, so the engine that depends on it is compiled again each time. `vendor/vectorcraft-svg` goes away with the first VectorCraft release that has the #2873 fix (design policy chapter 8), so this is left alone.

The project owner agreed to the first two suggestions, and to skipping pushes that change only documents. The third was not taken: `cargo check` makes no machine code, so the optimization level hardly matters to it, and a profile for CI only would be one more setting in `Cargo.toml`.

### What was done

- `.github/workflows/build.yml`:
  - On push and pull requests, `cargo check --locked` instead of `cargo build --locked`. It finds what the build found except errors at the last stages, such as linking.
  - Run by hand (`workflow_dispatch`: the Actions page, or `gh workflow run build.yml`), the full `cargo build --locked`, for example after raising VectorCraft's rev. The Windows build stays as it was, run by hand.
  - `paths-ignore` (`docs/**`, `**.md`, `LICENSE`): a push or pull request that changes only those doesn't run it. No `.md` file is read by the build (no `include_str!` of one).
  - rust-cache keeps a cache for the check and one for the build (`key`), since they leave different results: with one key, a check restoring the build's cache would find nothing to reuse and, the key already saved, never save its own.
  - Concurrency is grouped by the event too, so that a push doesn't cancel a build run by hand.
- Design policy section 1.1 (Linux); `README.md` and `README.ja.md` (Requirements).

### Commands run and results

| Command | Result |
|---|---|
| `gh run list` / `gh run view --json jobs` / `gh run view --log` (run 39547554230) | 438 s in all, the build step 359 s; it compiled the four crates above |
| `git show --name-only` over the commits since 2026-10-18 | 42 commits, 20 of them only documents |
| Locally, after touching `vendor/vectorcraft-svg/src/lib.rs`: `cargo check --locked --offline`, then `cargo build --locked --offline` | The same four crates: check 5.6 s, build 61 s (8 cores; GitHub's machines have 2) |
| The workflow read with Ruby's YAML parser | The triggers, `paths-ignore`, the two steps' conditions and the concurrency group as written |

### Next steps

1. After the push, see how long the check takes on GitHub Actions. The first run has no check cache yet and checks every dependency; the runs after it reuse that cache
2. Check from Claude Code in a new conversation (`RUST_LOG=debug`), as in the entries before
3. Not decided: process isolation, after step 2
4. Not decided: a test of the 2026-07-28 version (`server/discover` and per-request metadata)

## 2026-10-21 Check from Claude Code with real files

### Policy

The check the entries before left as the next step: real files from other apps sent through uploads, with requests a person would make, to see what the AI does with ArtServer's tools. The requests were made by the project owner in a Claude Code started in a folder holding the test files, which knew nothing of ArtServer's documents, so that the AI found the tools and commands by itself. What the AI did was read afterwards from the server's log, and the files it wrote were compared with renderings by another PDF reader (poppler's `pdftoppm`).

### Setup

- The token file (`~/.config/artserver/token`) and Claude Code's registration of ArtServer, both made on 2026-10-18, were gone: `claude mcp list` didn't show it, and `~/.claude.json` had no MCP server in any folder. Why is not known. A new token was made, and ArtServer was registered again with `claude mcp add` (local scope) in the folder of the test files; `claude mcp list` there showed it connected.
- The release build of the entries before, started with `RUST_LOG=debug` and its log kept in a file. Default settings.
- The test files: `PDF互換.ai` (Illustrator 30.8, saved with PDF compatibility; 400 × 300 pt; a rectangle, an ellipse, a star, another rectangle, "ArtServer テスト" in Hiragino Sans W6 and "Hello, Illustrator" in Helvetica Neue), `nonPDF互換.ai` (the same art saved without PDF compatibility: its PDF part is Illustrator's placeholder page), `all_photos.pdf` (made with ReportLab; A4, one page, eight JPEG photos of 1280–1920 pixels, 3.4 MB) and the photos themselves as JPEG files.

### Results

| Request | What the AI did | Result |
|---|---|---|
| Open `PDF互換.ai` and show a preview | `session_create`, `upload_create`, the PUT, `open_upload` (149 ms), then `render_preview` and `doc_inspect` at once | ArtServer's preview matches poppler's rendering: the places, the colours, the ellipse's transparency over the rectangle, the star's stroke and the W6 text |
| Arrange the photos of `all_photos.pdf` in a new document | Its first call used the session of the first request, which the server had closed after 30 minutes unused; the answer "unknown or expired session; create one with session_create" was enough for it to make a new one. `open_upload` (305 ms), then nine `command_list` searches ("group", the Edit menu, "select", "document", `edit.paste`, `select.set`, Window › Transform, "bounds", `document.activate`). It copied the eight photos with `select.set` and `edit.copy`, made a landscape A4 document, pasted them with `edit.pasteInPlace`, and set each photo's place and size in one atomic `command_batch` of `select.set` and `object.setBounds`, keeping their proportions | Two rows of photos (five and three) |
| Open `nonPDF互換.ai` | `upload_create`, the PUT, `open_upload` (46 ms) | Opened whole, without a warning: the same art as `PDF互換.ai` (see below) |
| Place a photo into `PDF互換.ai`, add a title and export PDF | `open_upload`, `document.node`, `list_fonts`; one atomic `command_batch` widening the artboard to 600 × 360 pt and moving the art down; a JPEG uploaded and placed into a rectangle with `place_upload` (20 ms); `add_text` "PDF互換テスト" in Hiragino Sans; searches for "font" and "character", then `text.setStyle` to W6; `export_document` PDF (30 ms) | 1.1 MB. Hiragino Sans W6 and Helvetica Neue embedded, the photo kept as JPEG; it renders as the preview showed |
| Trace the photo | `command_list` "trace", `imageTrace.presets`, `imageTrace.make` with "Low Fidelity Photo" (953 ms), previews (one at 2,400 pixels), `export_document` `.ai` (1,229 ms) | 30 MB; the photo became paths (object ids reached about 27,700); it renders correctly |
| Open a file sent from the upload page | `upload_create` without a name; the project owner opened the URL in a browser, chose a JPEG and clicked Send; `open_upload` 72 s later (8 ms), a preview, `export_document` `.ai` | The photo opened as a document of 1280 × 1920 pt |

The check items of the entries before:

| Item | Result |
|---|---|
| Opening and placing real files through uploads | Done: both `.ai` files, a PDF and JPEG photos opened, and a photo was placed |
| Whether the AI finds and uses the opened commands | Yes: it searched with `command_list` and used `select.set`, `edit.copy`, `edit.pasteInPlace`, `object.setBounds`, `artboard.setProps`, `object.move`, `text.setStyle`, `document.node`, `imageTrace.presets` and `imageTrace.make`, through `command_run` and `command_batch` (atomic where it changed several objects). Text styles were not tried |
| Installed fonts rather than the `bundled` ones | Yes: it called `list_fonts` before adding text and took Hiragino Sans, the font of the file's own text |
| `session_close` | Never called, in any of the requests. The server closed the unused session after 30 minutes, as it should |
| The MCP protocol version | **2026-07-28.** Claude Code first sends `server/discover`, then makes every request in the 2026-07-28 version. No test covers that version (section 13.2 #1 of the design policy), but the real client works through it |
| A person clicking Send on the upload page | Works |
| Crashes, hangs, time and memory | None; the slowest call took 1.2 s (the 30 MB `.ai` export); the server's RSS was about 160 MiB after an hour and about 100 MiB after 75 minutes; the only error was the expired session |
| Opening an exported `.vectorcraft` in the VectorCraft desktop app | Not tried |

Findings:

- **An `.ai` saved without PDF compatibility opens too.** When a file's PDF part is only Illustrator's placeholder page, the engine reads Illustrator's own copy of the art kept in the file (`AIPrivateData`) instead (VectorCraft `crates/engine/src/cmd/fileio/load.rs:242-245`). Its error for such files, "save it again with PDF compatibility on", is for when that copy can't be read. The README, the formats upload_create lists (`uploads::ACCEPTED`) and the tools' descriptions say "saved with PDF compatibility", narrower than what works. `.ai` files in the old PostScript form (Illustrator 8 and earlier) are still not taken by uploads.
- **Previews have a transparent background.** Both previews were RGBA PNGs whose background was fully transparent (no white pixels); one was shown with white behind it and one with black. The document has no black, but the AI may take the background for black.
- `LocalSessionManager` (`src/lib.rs:91`) is not needed by Claude Code, which keeps no protocol session in the 2026-07-28 version. Nothing was decided about it.

### Commands run and results

| Command | Result |
|---|---|
| `claude mcp add --transport http artserver …` in the folder of the test files, then `claude mcp list` there with the server running | Connected |
| The server's log read for each tool call (name, arguments, time, errors) | The figures above |
| `pdfinfo`, `pdffonts`, `pdfimages -list` and `pdftoppm` on the test files and the files the AI exported | The fonts, images and renderings above |

### Next steps

1. Not tried yet: text styles (`charStyle.*`, `paraStyle.*`) from Claude Code, and opening an exported `.vectorcraft` in the VectorCraft desktop app
2. Not decided: whether to describe uploaded `.ai` files without "saved with PDF compatibility" (README, `uploads::ACCEPTED`, the tools' descriptions)
3. Not decided: whether to do anything about `session_close` never being called (the server closes unused sessions after 30 minutes), and about previews' transparent background
4. Not decided: process isolation. The measurement, the endurance test and this check are all in
5. Not decided: a test of the 2026-07-28 version (Claude Code uses it), and what to do with `LocalSessionManager`
6. After the next push that changes code, see how long the cached check takes on GitHub Actions, and that a push of documents alone doesn't run it

## 2026-10-21 A test of the 2026-07-28 version, and `NeverSessionManager`

### Policy

The check from Claude Code showed that it uses the 2026-07-28 version, which no test covered: the E2E tests used the versions before it, through rmcp's client (2025-11-25) and by hand (2025-06-18 and 2025-11-25). ArtServer leaves that version to rmcp, so a change in rmcp could break it while every test passed.

rmcp 3.5.1's code was read on what it asks of a 2026-07-28 request (`src/transport/streamable_http_server/tower.rs`, `src/transport/common/mcp_headers.rs`, `src/model/meta.rs`): the `MCP-Protocol-Version` header; `Mcp-Method`, and `Mcp-Name` for a tool call, matching the body; and in the params' `_meta`, `io.modelcontextprotocol/protocolVersion` and `io.modelcontextprotocol/clientCapabilities`, without which the request gets 400. The project owner agreed to two changes:

- A test that sends the exchange by hand, as the test of older versions does (2026-10-19), rather than switching rmcp's client to its discover mode (`ClientLifecycleMode::Discover`) for every E2E test: a client of another SDK, as Claude Code is, can catch what rmcp's own client and server would agree on wrongly, and the tools behave the same in every version, which the other E2E tests already cover. The server's log shows only what rmcp read, not Claude Code's raw requests, so the test follows the rules as rmcp enforces them.
- `NeverSessionManager` in place of `LocalSessionManager`. With `with_legacy_session_mode(false)`, rmcp serves every request without protocol sessions, and `LocalSessionManager` went unused (noted on 2026-10-19); rmcp documents `NeverSessionManager` as the manager that disables sessions. The behavior doesn't change.

### What was done

- `tests/e2e.rs`: `latest_clients_work_without_initialize`, with the helper `latest_rpc` (headers and `_meta` of a 2026-07-28 request) and the constant `LATEST`:
  - `server/discover`: 2026-07-28 among the supported versions, the server's name in `_meta`, the tools capability
  - `tools/list`: the 25 tools, in one page
  - Tool calls, each in a request of its own: `session_create`, `document_new`, `draw_shape`, `export_document` (the SVG has the rectangle's colour) and `session_close`. Every answer is plain JSON with no `Mcp-Session-Id` (checked by `rpc`)
  - A tool call without the `_meta` gets 400, its error naming `_meta`: the server holds the request to that version's rules and doesn't take it for an older version's
  - GET `/mcp` gets 405
- `src/lib.rs`: rmcp is given `NeverSessionManager`, with a comment on why no protocol sessions are kept.
- Design policy sections 4.4 and 13.2 (#1, #10).

### Commands run and results

| Command | Result |
|---|---|
| `cargo test --offline --locked` | All 82 passed (55 unit, 25 E2E, 2 for the PDF backend) |
| The new test against a server whose `ServerHandler` advertises only 2025-11-25 and 2025-06-18 (`supported_protocol_versions`, added for the run, then removed; `git diff` showed `src/tools.rs` unchanged) | The new test failed at `server/discover` (400); the test of older versions passed |
| `cargo clippy --all-targets --offline --locked` / `cargo fmt --check` | No warnings / no changes, after `cargo fmt` reflowed the new test |

### Next steps

1. Push, and see how long the cached check takes on GitHub Actions (this push changes code)
2. Not tried yet: text styles (`charStyle.*`, `paraStyle.*`) from Claude Code, and opening an exported `.vectorcraft` in the VectorCraft desktop app
3. Not decided: whether to describe uploaded `.ai` files without "saved with PDF compatibility" (README, `uploads::ACCEPTED`, the tools' descriptions)
4. Not decided: whether to do anything about `session_close` never being called, and about previews' transparent background
5. Not decided: process isolation

## 2026-10-21 `.ai` files saved without PDF compatibility, `session_close`, and the cached CI check

### Policy

Two open points of the check from Claude Code, looked at before changing anything:

- **`.ai` saved without PDF compatibility.** The check found that one opens whole, and the entry said the wording "saved with PDF compatibility" was narrower than what works. Trying the same file in each way showed otherwise: only `open_upload` without `pages` reads the art, from Illustrator's own data in the file (VectorCraft `crates/engine/src/cmd/fileio/load.rs:186-195`, which skips that data when pages are picked, and `:236-245`). `open_upload` with `pages` and `place_upload` bring in the notice page Illustrator writes as the PDF part ("save it again with PDF compatibility…"), reported as a success with the warning "only the PDF-compatible part of this file was read: art outside its artboards…", which doesn't say what happened. "Saved with PDF compatibility" is the condition under which a file works every way, so the project owner chose to keep it in what the AI reads (`uploads::ACCEPTED`, the tools' descriptions) and to note the rest for people. Whether to report the success with the notice page to VectorCraft is not decided.
- **`session_close` never called.** The check's entry and the explanation before it relied on the 2026-10-19 note that the server's instructions don't mention `session_close`; that was fixed the same day (`e690bc2`): the instructions and `session_close`'s own description both say to close the session when the work is done. In the check, the AI did requests 2 to 6 in one session (06:43–07:06 UTC) while the project owner kept asking, so the work was never done; closing after each request would have discarded the documents. The server closes unused sessions after 30 minutes. The project owner chose to change nothing.

### What was done

- `README.md` and `README.ja.md` (Sending files to ArtServer): how an `.ai` saved without PDF compatibility opens and places, and the advice to save with PDF compatibility, Illustrator's default.
- Design policy section 4.6 (Formats).

### Commands run and results

| Command | Result |
|---|---|
| A script sending `nonPDF互換.ai` to the release build: `open_upload` named `.ai`, without a name, and with `pages` "1"; `place_upload` with and without `page` | Opened whole in the first two (the artboard's name from Illustrator, "アートボード 1"); the notice page in the other three, each a success with the warning above. Previews of the last two show the notice text |
| `gh run view` of run 39562121287 (commit `c1087d6`, the first check with its cache) | Succeeded in 51 s in all: 10 s restoring the cache, 28 s checking (`vectorcraft-svg`, the engine, `vectorcraft-mcp` and `artserver`, as before). The cached build took 438 s |

### Next steps

1. Not tried yet: text styles (`charStyle.*`, `paraStyle.*`) from Claude Code, and opening an exported `.vectorcraft` in the VectorCraft desktop app
2. Not decided: whether to report to VectorCraft that an `.ai` without PDF compatibility, placed or opened with pages, comes in as its notice page reported as a success
3. Not decided: whether to do anything about previews' transparent background
4. Not decided: process isolation

## 2026-10-22 The last two items of the check from Claude Code: text styles, and the desktop app

### Policy

The two items the entries before left untried, done on 2026-10-21 after them in the same way as the check from Claude Code: the project owner made the requests in a Claude Code started in the folder of the test files, with the same release build, and the server's log was read afterwards. The project owner opened the files the AI wrote in Illustrator and in the VectorCraft desktop app. Nothing in ArtServer was changed.

### Results

| Request | What the AI did | Result |
|---|---|---|
| Make character styles from the two texts of `PDF互換.ai`, apply them, and export `.ai` | Its first call used the session of the request before, closed by then for being unused; it made a new one and opened the file. `charStyle.list`, then one atomic `command_batch`: `charStyle.new` 見出し from "ArtServer テスト" and 本文 from "Hello, Illustrator", and `charStyle.apply` of each to its text. It looked at the text with `document.node`, put the tracking of "テ" back to −50 with `text.setRangeStyle` (`start` 10, `end` 13; see below) and exported `.ai`. It then sent that file back through an upload, opened it and listed the styles | In ArtServer the styles survive the export: 見出し Hiragino Sans W6 28 pt, 本文 Helvetica Neue 18 pt. In Illustrator "ArtServer テスト" is three text objects (see below) |
| Draw a picture and export `.vectorcraft` | `document_new` "秋の夜" 600 × 400 pt; one atomic `command_batch` of shapes (a sky in a gradient, 22 stars, a moon, hills and trees); `pathfinder` `minusFront` for the moon's shape; `add_text` "秋の夜" in Hiragino Mincho ProN 36 pt and a caption in Helvetica Neue 9 pt; tracking 200 on the caption with `text.setStyle`; `export_document` `.vectorcraft`; then the file sent back through an upload, opened and previewed | Opens in ArtServer as drawn. The VectorCraft desktop app (`/Applications/VectorCraft.app`, 0.13.1) opens it with the notice 「見つからないフォント: Hiragino Mincho ProN Regular — W3 で代用」 (see below) |

Findings:

- **In Illustrator, "ArtServer テスト" came in as three text objects, one for each run.** "ArtServer テスト" in `PDF互換.ai` is one text in Illustrator, with kerning before "ス". Opened in ArtServer without the AI touching it, it is one text of three runs, "ArtServer ", "テ" and "スト", "テ" with tracking −50: the engine reads the file's PDF part, where that kerning is a move between pieces of text. `charStyle.apply` to the whole text made it one run with tracking 0. The AI saw the change of look and put −50 back on "テ" (`start` and `end` are UTF-8 byte offsets, VectorCraft `crates/engine/src/cmd/typecmd.rs:444`), which made three runs again. VectorCraft writes each run as a text block of its own (`BT` … `ET`) in the PDF part: `PDF互換_文字スタイル.ai` has three blocks for that text and one for "Hello, Illustrator", and `PDF互換.pdf` of the check before, where the AI didn't touch that text, has the same three. The export holds none of Illustrator's own data (`AIPrivateData`), so the PDF part is what Illustrator reads. Runs, blocks and objects agree in this one file, but that Illustrator makes one object of each block is not established: Illustrator's own `PDF互換.ai` has both texts in a single block of its PDF part and still shows two texts, yet Illustrator opens that file from its own data, so it says nothing about how a PDF part is read. A text of one run was not tried in Illustrator: the title "PDF互換テスト" of `PDF互換.pdf`, styled only as a whole, is one block with its spacing inside it (`TJ`), but that file was not opened in Illustrator. What this check shows is that putting the look back made the text come apart in Illustrator; whether some way keeps both is not known. The AI knew how to style part of a text; what it can't see is how Illustrator shows the file.
- **The character styles are only in ArtServer's own data in the `.ai`.** Illustrator reads the PDF part, so they are not expected to show there; not looked at in Illustrator.
- **A font given without a style keeps the style "Regular".** `add_text` passes only the family (`font`) to `text.create` and has no style argument (VectorCraft `crates/mcp/src/tools.rs:710`). `text.create` and `text.setStyle` change the style only when one is given (`crates/engine/src/cmd/create.rs:583-588`, `typecmd.rs:574-579`), so the text keeps the default "Regular" (`crates/doc/src/text.rs:257`). `text.setRangeStyle`, by contrast, takes the family's closest style when the family has no face of the old one (`crates/engine/src/cmd/textedit.rs:459-470`). The same code is in v0.14.0. `秋の夜.vectorcraft` stores Hiragino Mincho ProN "Regular", but that family has only W3 and W6. ArtServer draws it with the nearest weight without a word (`crates/text/src/fontdb.rs:2250`), so its preview looked right; the desktop app reports the font as missing and draws W3, which looks the same. The caption's Helvetica Neue has a Regular face, so no notice came for it. Giving the style (`text.setStyle` with `style` "W3", or `text.create` with `style`) would avoid the notice.

### Commands run and results

| Command | Result |
|---|---|
| The server's log read for each tool call | The calls above |
| A script inflating the content streams of `PDF互換_文字スタイル.ai` and `PDF互換.pdf` and listing their text blocks | In both, three blocks for "ArtServer テスト" (10, 1 and 2 glyphs) and one for "Hello, Illustrator"; `PDF互換.pdf` also has one for its title "PDF互換テスト" |
| On 2026-10-24, the script written again and run on these two and on `PDF互換.ai`, and the log read for the title of `PDF互換.pdf` | The two exports as above, neither with `AIPrivateData`. The title's block has spacing inside it (`TJ`); the AI made the title with `add_text` and gave it W6 with `text.setStyle` without `start` or `end`. `PDF互換.ai`: "ArtServer テスト" and "Hello, Illustrator" in one block, and `AIPrivateData` |
| `gzip -dc` of `秋の夜.vectorcraft` | The title's style: `"font_family":"Hiragino Mincho ProN","font_style":"Regular"` |
| `fc-list` | Hiragino Mincho ProN has W3 and W6 |
| `gh api` for `crates/mcp/src/tools.rs` and `crates/engine/src/cmd/typecmd.rs` at v0.14.0 | `add_text` and `text.setStyle` as at v0.13.1 |

### Next steps

1. VectorCraft v0.14.0 was released on 2026-10-21 at 07:46 UTC, with the fix of the Japanese weights (PR #2896): raise the pin, remove `vendor/vectorcraft-svg` and its `[patch]`, classify the new commands, check the weight selection, and see whether the points below still happen with it
2. Not decided: what to do about Illustrator splitting a text (a README note, a line in the AI's instructions, a report to VectorCraft, or nothing). The project owner tests more cases in Illustrator first
3. Not decided: what to do about the "Regular" style (a report to VectorCraft, telling the AI to give a style, or nothing)
4. Not decided: whether to report to VectorCraft that an `.ai` without PDF compatibility, placed or opened with pages, comes in as its notice page reported as a success. It could go with 2 and 3
5. Not decided: whether to do anything about previews' transparent background
6. Not decided: process isolation
7. Not checked yet: whether `artserver-check.vectorcraft` (2026-10-19) opens in the desktop app without a notice
8. Design policy 13.2 #12 still says that whether Claude Code's AI uses the tools correctly is not verified

## 2026-10-25 Updated VectorCraft to v0.14.0, and removed the `vectorcraft-svg` replacement

### Policy

The project owner's local VectorCraft clone was pulled to `8bf090b` (main, 492 commits after v0.16.0); the latest release is v0.16.0 (`1019075`, 2026-10-22). Reading v0.13.1 against v0.16.0 showed two changes that need ArtServer's code changed, both first released in v0.15.0: `AutomationRoots::new` takes lists of folders instead of one, and `draw_shape` and `draw_path` run the creation and styling as one `command.batch` (upstream #3545), a command ArtServer's allowlist refuses, so both tools would fail. The project owner wants upstream's fix of the Japanese font weights to replace ArtServer's patch, so the update is split in two: first v0.14.0 (`ad0566943ab9741e6356bbc9b53c3cc09895068d`, 2026-10-19, 460 commits after v0.13.1), the first release with the fix (PR #2896, `eec304d`) and without those two changes, with the patch removed; then v0.16.0. This way the first step changes only who fixes the weights.

Before the change, the code was searched for other fixes of VectorCraft bugs made in ArtServer: there are none. What ArtServer adds is not a bug fix and is still missing upstream in v0.16.0, so it stays: `list_fonts` (VectorCraft's tools still have no font list) and the check of unknown arguments (`src/engine_host.rs:316`; the engine still checks them only in its own tests). `[patch.crates-io]` (the PDF backend) is VectorCraft's own setting copied over, the same craft-libs `ca50d63` in v0.14.0 and v0.16.0.

Upstream's fix compared with the patch, by reading the code of v0.14.0 and v0.16.0: the SVG export writes a text's weight through `font_props` → `font_descriptor` (`crates/svg/src/css.rs`) → `style_weight` (`crates/text/src/fontdb.rs`), and `style_weight` now reads "W" and one digit as the digit × 100, at least 100, as the patch did in `font_descriptor`. The weights are the same for W0–W9. Upstream splits the style name at any character that is not an ASCII letter or digit (the patch: spaces, `-` and `_`), and its fix also reaches font selection (the nearest weight, and `is_bold` of craft-fonts counts W6 and above as bold). Upstream's test `crates/svg/tests/font_weights.rs` exports SVG and checks W0 100, W3 300, W4 none, W6 600 and W9 900.

### What changed and how it was handled

| Item | Details |
|---|---|
| commit | Raised the 5 VectorCraft entries in `Cargo.toml` to `ad05669` (v0.14.0) |
| `vendor/vectorcraft-svg` | Removed, with `vendor/patches/vectorcraft-svg.diff` and `[patch."https://github.com/storytold/vectorcraft"]` in `Cargo.toml`. `vendor/README.md` and `vendor/README.ja.md` now say that no crate is patched, list the removed patch, and keep how to make a copy and the steps for updating |
| `Cargo.lock` | Only the 22 VectorCraft crates changed (0.13.1 → 0.14.0, and the rev); `vectorcraft-svg` now comes from GitHub. No other crate moved |
| PDF patches and build settings | Same in VectorCraft v0.14.0 (craft-libs `ca50d63`; the workspace adds only the `tga` feature of `image`). Unchanged |
| ArtServer's code | No changes needed. The comment of the weight test (`src/engine_host.rs:664`) now points to upstream's fix |
| New commands in v0.14.0 | 7: `edit.clearHistory`, `library.moveGroup`, `object.objectsOnPath.attach`, `object.objectsOnPath.expand`, `object.objectsOnPath.options`, `path.moveAnchor`, `select.color`. Not on the allowlist, so closed; classified with v0.16.0's |
| License notices | `about.toml`: `vectorcraft-svg` reads its license from GitHub like the other VectorCraft crates. `about.hbs`: the VectorCraft entry. Regenerated |
| Documents | Design policy chapter 8 (no replacement at present; the referenced commit) and its revisions; `docs/architecture.md` (the referenced commit) |

### Commands run and results

| Command | Result |
|---|---|
| `git diff v0.13.1 v0.16.0` and others in the local VectorCraft clone (no fetch) | As in the policy above. No allowlisted command was removed or renamed up to v0.16.0 |
| `cargo build` (online, updating `Cargo.lock`) | Succeeded in 1m 11s, no warnings |
| `git diff Cargo.lock` | As in the table above |
| `cargo test --locked` | All 82 passed (55 unit, 25 E2E, 2 for the PDF backend). `japanese_weight_names_reach_the_svg` passed without the patch: Hiragino Sans W6 `font-weight="600"`, Hiragino Mincho ProN W3 `300`, Inter Regular none, Source Sans 3 Bold `bold` |
| `cargo clippy --all-targets --locked` / `cargo fmt --check` | No warnings / no changes |
| `cargo deny check` | bans, licenses and sources ok; **advisories failed**: RUSTSEC-2026-0386 (tokio, `AsyncFd::new` and related constructors violate I/O safety; fixed in 1.51.6 and 1.53.3). `Cargo.lock` has tokio 1.53.1, unchanged by this update; the advisory is new since the last check. ArtServer doesn't use `AsyncFd` itself |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | Succeeded. Only the VectorCraft entry and the crate versions changed |

### Checking the weights with the new build

The project owner restarted ArtServer with the new build (`target/debug/artserver`, built 08:17, started 08:26), and the tools were used from Claude Code. One document of 600 × 460 pt, made with one atomic `command_batch` of `text.create` with a `style`: Hiragino Sans W1, W3, W6 and W9, Hiragino Mincho ProN W3 and W6, and two Hiragino Mincho ProN texts in styles the family lacks, "Regular" and "Bold". It was exported as SVG, PDF, PNG and `.ai` to `~/Desktop/artserver-output/v0140-weight-check.*`; the project owner opened the SVG and the `.ai` in Illustrator.

| Check | Result |
|---|---|
| `render_preview` | Each text in its weight. "Regular" drawn in W3 and "Bold" in W6 (the nearest weights) |
| `font-weight` in the SVG | Hiragino Sans W1 `100`, W3 `300`, W6 `600`, W9 `900`; Hiragino Mincho ProN W3 `300`, W6 `600`; "Regular" none, "Bold" `bold` |
| Fonts in the PDF and the `.ai` | HiraginoSans-W1, W3, W6, W9 and HiraMinProN-W3, W6, and no others: "Regular" was set in W3 and "Bold" in W6 |
| SVG in Illustrator | The weights look right. **Illustrator warns on opening**: 「ドキュメントで使用されているフォントは、システムにないか、形式が異なります。」 listing 「serif:」, with 「一部の字体切り換え文字は、このバージョンの Illustrator では使用できません。」 |
| `.ai` in Illustrator | No warning |

The warning comes from a change in v0.14.0 that is not about the weights: the SVG export adds a generic family to `font-family` (upstream #3135, `0f855be7`), so `'Hiragino Mincho ProN'` of v0.13.1 (`sample.svg` of 2026-10-18) is now `'Hiragino Mincho ProN', serif`, and Hiragino Sans gets `, sans-serif`. Upstream added it so that viewers without the font draw the right kind of face. Illustrator seems to look for `serif` as a font; why `sans-serif` isn't listed is not known. Only the SVG and CSS export write it (`crates/svg/src/css.rs:117`); `.ai` and PDF are written by the PDF backend, which names the fonts themselves, and the `.ai` has no `serif` anywhere. Upstream `main` (`8bf090b`) still writes it.

The project owner decided to note in the README that a file to edit in Illustrator should be exported as `.ai`, and SVG is for browsers and other viewers, without reporting to VectorCraft or patching ArtServer. The note is in "Known limitations" of `README.md` and `README.ja.md`.

### Next steps

1. Not decided: the tokio advisory (RUSTSEC-2026-0386): raise tokio to 1.53.3 or later
2. The second step, v0.16.0: `AutomationRoots::new`, how GuardedBackend treats the `command.batch` that `draw_shape` and `draw_path` now run, and classifying the new commands (17 since v0.13.1)
3. Not decided: what to do about Illustrator splitting a text, and about the "Regular" style (`add_text` still has no style argument in v0.16.0); whether to report the `.ai` without PDF compatibility; previews' transparent background; process isolation
4. Not checked yet: whether `artserver-check.vectorcraft` (2026-10-19) opens in the desktop app without a notice
5. Design policy 13.2 #12 still says that whether Claude Code's AI uses the tools correctly is not verified

## 2026-10-25 tokio 1.54.1 for RUSTSEC-2026-0386

### Policy

`cargo deny check` of the entry before failed on RUSTSEC-2026-0386 (tokio 1.53.1: `AsyncFd::new` and related constructors violate I/O safety; fixed in 1.51.6, and in 1.53.3 and later). ArtServer doesn't call `AsyncFd` itself. The VectorCraft crates ArtServer uses don't depend on tokio (in ArtServer's `Cargo.lock` it comes through ArtServer, axum, hyper, rmcp and their helpers); VectorCraft's own `Cargo.lock` has tokio 1.53.1 at v0.13.1 and v0.14.0, and 1.54.0 at v0.16.0 and `main` (bumped along with craft-window in `cb0a7d9`, not for the advisory; 1.54.0 is yanked). The project owner chose to follow VectorCraft's 1.54 line: 1.54.1 (2026-10-21, the latest; not 1.53.3, the smallest change). `Cargo.toml` keeps `tokio = "1"`.

### What changed and how it was handled

| Item | Details |
|---|---|
| `Cargo.lock` | Only tokio's entry: 1.53.1 → 1.54.1, and its checksum (`b52fd32…`, the same as crates.io's). `cargo update -p tokio --precise 1.54.1` also moved six unrelated crates (errno, quinn-udp, rustix, rustls-platform-verifier, tempfile and winapi-util) from windows-sys 0.61.2 to 0.52.0, both already in the lock, which only matters on Windows. So `Cargo.lock` was restored and only tokio's entry was changed; tokio's dependency list is the same in 1.53.1 and 1.54.1, and the builds with `--locked` below accept the file |
| License notices | Regenerated: only the tokio line changed |

### Commands run and results

| Command | Result |
|---|---|
| `cargo build --locked` | Succeeded in 15 s (tokio, tokio-util, tokio-stream and artserver rebuilt), no warnings |
| `cargo test --locked` | All 82 passed |
| `cargo clippy --all-targets --locked` / `cargo fmt --check` | No warnings / no changes |
| `cargo deny check` | advisories, bans, licenses and sources ok, no warnings |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | Succeeded |

### Next steps

1. The second step, v0.16.0: `AutomationRoots::new`, how GuardedBackend treats the `command.batch` that `draw_shape` and `draw_path` now run, and classifying the new commands (17 since v0.13.1)
2. Not decided: what to do about Illustrator splitting a text, and about the "Regular" style (`add_text` still has no style argument in v0.16.0); whether to report the `.ai` without PDF compatibility; previews' transparent background; process isolation
3. Not checked yet: whether `artserver-check.vectorcraft` (2026-10-19) opens in the desktop app without a notice
4. Design policy 13.2 #12 still says that whether Claude Code's AI uses the tools correctly is not verified

## 2026-10-25 One author for every commit, before going public

### Policy

Before making the repository public, the project owner had every commit's author and committer set to `roundshape <99551120+roundshape@users.noreply.github.com>`, GitHub's private address of the repository's owner. Before this, 38 commits (2026-10-19 to 2026-10-21) were by `kasuya` with no address, and 25 by `roundshape <roundshape2000@gmail.com>`, an address tied to another GitHub account.

### What changed and how it was handled

| Item | Details |
|---|---|
| History | `git filter-branch --env-filter` over `main`: all 63 commits. Their files, messages and dates are unchanged; only the IDs changed |
| Commit IDs in this file | The 8 IDs of ArtServer's own commits in the entries before were replaced with the new ones. The GitHub Actions run numbers in those entries are unchanged, and the runs show the old IDs |
| Git setting | `user.email` of this repository (`.git/config`) is the address above, so new commits use it |

### Commands run and results

| Check | Result |
|---|---|
| Commits before and after | 63 and 63; every commit's tree, author date and committer date the same; messages identical; the last tree identical |
| Authors and committers after | All `roundshape <99551120+roundshape@users.noreply.github.com>` |
| Own commit IDs in commit messages | None, so no message needed changing |

## 2026-10-25 A security policy, before going public

### Policy

Before making the repository public, the project owner decided to publish `docs/ARTSERVER_PROCESS_ISOLATION_REVIEW.md` and `docs/DEVELOPMENT_PLAN.md` as they are, and asked for a security policy. Reports go through GitHub's private vulnerability reporting, so no address is published. VectorCraft has no security policy of its own (none in `8bf090b`), so problems in its engine that affect ArtServer are taken here and passed on as needed.

### What was done

- `SECURITY.md` and `SECURITY.ja.md`: the supported version (the latest commit of `main`; no releases yet), how to report (the repository's Security tab › Report a vulnerability) and what to include, best-effort handling without a promised response time, published advisories of dependencies as ordinary issues, and the scope: the intended usage (design policy section 1.1), the kinds of report especially welcome, the known limitations of the README, and VectorCraft's engine.
- `README.md` and `README.ja.md` (Security): a line pointing to the policy.

### Next steps

1. Not done yet: turning on private vulnerability reporting in the repository's settings; until then the link the policy names doesn't appear
2. Not done yet: the Windows build of the current `main`, the repository's description, and whether to turn off its unused wiki
3. Then making the repository public

## 2026-10-25 The repository is public

### What was done

At the project owner's instruction, the repository was made public with `gh repo edit roundshape/ArtServer --visibility public --accept-visibility-change-consequences`, and private vulnerability reporting was turned on right after with `gh api -X PUT repos/roundshape/ArtServer/private-vulnerability-reporting`. GitHub offers private vulnerability reporting only for public repositories: tried while the repository was private, the same request returned 404, so the two steps were run one after the other.

### Commands run and results

| Check | Result |
|---|---|
| `gh api repos/roundshape/ArtServer` | `visibility` is `public` |
| `gh api repos/roundshape/ArtServer/private-vulnerability-reporting` | `enabled` is `true` |
| The repository page and `SECURITY.md` without logging in | Both 200 |
| GraphQL `isSecurityPolicyEnabled` / `securityPolicyUrl` | `true` / `https://github.com/roundshape/ArtServer/security/policy`: GitHub shows `SECURITY.md` as the policy |

### The repository's description and wiki

At the project owner's instruction, with `gh repo edit roundshape/ArtServer --description … --enable-wiki=false`:

- Description (the About box, search results and link previews): "Headless vector graphics server driven over MCP (Streamable HTTP), built on the VectorCraft engine", the same as `description` in `Cargo.toml`. No topics
- Wiki: turned off. The documents are the README and `docs/`, and the wiki was never used
- Issues stay on: `SECURITY.md` sends published advisories of dependencies to ordinary issues

Read back with `gh api repos/roundshape/ArtServer`: the description as above, `has_wiki` false, `has_issues` true, `visibility` public.

### Next steps

1. Running: the Windows build of the current `main` (run 40146146077, started 2026-10-25 00:17 UTC on `0400bff`; no cache in the new repository, so all VectorCraft crates are built)
2. Not decided: GitHub's secret scanning and Dependabot alerts, free for public repositories and off now (`cargo deny` checks published advisories locally)
3. The second step, v0.16.0: `AutomationRoots::new`, how GuardedBackend treats the `command.batch` that `draw_shape` and `draw_path` now run, and classifying the new commands (17 since v0.13.1)
4. Not decided: what to do about Illustrator splitting a text, and about the "Regular" style; whether to report the `.ai` without PDF compatibility; previews' transparent background; process isolation

## 2026-10-25 Updated VectorCraft to v0.16.0, with `command.batch` checked step by step

### Policy

The second step of the update (the entry on v0.14.0 above): the latest release, v0.16.0 (`10190753ef356ce8de7592626bcb99fd69db467c`, published 2026-10-23). Since v0.15.0, VectorCraft's `draw_shape` and `draw_path` make the object and style it with one `command.batch`, so that a step the engine refuses leaves nothing behind (upstream #3545), and ArtServer's allowlist refused `command.batch`. The project owner chose option A of the four considered: VectorCraft's tools may run `command.batch` when every step passes the checks a command of its own would (design policy section 4.7, with the options not taken). The project owner also agreed to open 16 of the 17 commands new since v0.13.1 and keep `library.moveGroup` closed.

Read in the engine before the change: its `command.batch` (`crates/engine/src/cmd/fileio/batch.rs`) runs each step with `execute_step`, refuses a batch inside a batch, resolves no references between steps, and on an error brings back the documents and the session's paint defaults, drawing mode, clipboard and Untitled count. Its `check_params` looks into a batch's steps (`crates/engine/src/cmd/params.rs`), but as the free function, without the check of `effect.setParams` that `Session::check_params` adds. `AutomationRoots::new` takes lists of folders since v0.15.0; an empty list of write roots refuses every write as `None` did (`crates/engine/src/file_access.rs`).

### What changed and how it was handled

| Item | Details |
|---|---|
| commit | Raised the 5 VectorCraft entries in `Cargo.toml` to `1019075` (v0.16.0) |
| `Cargo.lock` | Only the 22 VectorCraft crates changed (0.14.0 → 0.16.0, and the rev), and their dependency lists gained `chrono` (two crates) and `vello_cpu` (`vectorcraft-text`), both already in the lock. No other crate moved |
| PDF patches and build settings | Same in v0.16.0 (craft-libs `ca50d63`). Unchanged |
| `AutomationRoots::new` | `(Some(dir), None)` became `(&[dir], &[])` in `src/lib.rs` and two tests |
| `command.batch` (`src/policy.rs`) | `policy::check` lets `Caller::VectorCraftTool` run it when every step passes `check` as a command of its own; `batch_steps` refuses more than 256 steps, a step that isn't an object of `command` and `params`, and a batch inside a batch. A refusal reads as the engine's failed step (`batch step N (`id`) failed: …`), which `draw_shape` and `draw_path` read back to the step's own error. `Caller::ArtServer` still may not run it, and it isn't on the list, so `command_list` doesn't show it |
| Unknown arguments (`src/engine_host.rs`) | `execute_checked` runs `Session::check_params` on each step of a batch, in the same form, before the engine's check of the batch as a whole |
| Allowlist | The 16 commands, grouped with a comment. 626 commands in all. `library.moveGroup` is in the test of refused commands; `command.batch` left that list for a test of its own |
| Tests | `batches_are_open_to_vectorcraft_tools_with_every_step_checked` (policy) and `the_drawing_tools_style_their_objects_in_one_checked_batch` (engine host: `draw_shape` with a fill, opacity and name and `draw_path` work; a step with an unknown argument refuses the batch before it runs; a colour the engine refuses rolls `draw_shape` back, and the reply is the step's own error). `command_run` of `command.batch` with `prefs.set` joined the test of refusals |
| License notices | The VectorCraft entry in `about.hbs`, regenerated |
| Documents | Design policy sections 4.7 (`command.batch`, the new commands) and 6.1, chapter 8 and its revisions; `docs/architecture.md` (the referenced commit) |

### Commands run and results

| Command | Result |
|---|---|
| `cargo build` (online, updating `Cargo.lock`) | Succeeded in 1m 14s, no warnings |
| `cargo test --locked` | The first run: 1 failed. The engine's check of the batch as a whole looked into the steps first and gave the step's error without `batch step 0`, so the per-step check moved before it. Then all 84 passed (57 unit, 25 E2E, 2 for the PDF backend) |
| `cargo clippy --all-targets --locked` | No warnings |
| `cargo fmt --check` | Lines added in this change were not formatted; `cargo fmt` changed only those, and the check then passed |
| `cargo deny check` | advisories, bans, licenses and sources ok |
| `cargo about generate about.hbs -o THIRD_PARTY_NOTICES.md` | Succeeded. Only the VectorCraft entry and the crate versions changed |

The Windows build of `0400bff` (before this change; run 40146146077, by hand) succeeded in 27 minutes, 25m 56s of it the build, with no cache in the new repository.

### Checking from Claude Code

The project owner restarted ArtServer with the new build (`target/debug/artserver`, built 09:49, started 10:25), and the tools were used from Claude Code in a document of 400 × 300 pt.

| Call | Result |
|---|---|
| `draw_shape` rounded rectangle: fill, `stroke` "none", `name`, `opacity` 80, `effects` `stylize.dropShadow` | Succeeded; the reply lists the name, opacity, paint and effect, and `doc_inspect` shows them on the object |
| `draw_shape` star: fill, stroke 3 pt, `dash` [6, 3], `join` round | Succeeded |
| `draw_path` from SVG path data: `fill` "none", stroke 6 pt, `cap` round | Succeeded |
| `draw_shape` with `fill` "not a colour" | Error "invalid parameters for `paint`: bad color "not a colour""; no ellipse left |
| `command_batch`, `atomic`, label 点を3つ: three `draw_shape` steps | Succeeded (`created` 5, 6, 7); one `undo` removed all three (`undone` 点を3つ) and `redo` brought them back |
| `command_run` of `command.batch`: `shape.rectangle`, then `prefs.set` | Refused before it ran: "batch step 1 (`prefs.set`) failed: command `prefs.set` is not allowed"; no rectangle made |
| `doc_inspect` | Six objects. The history has one step for each call: Rounded Rectangle, Star, Pen, 点を3つ (with v0.13.1 the styling took steps of its own) |
| `render_preview` | As drawn, the drop shadow included |

### Next steps

1. Not checked yet: the Linux CI and the Windows build of this change
2. Not decided: GitHub's secret scanning and Dependabot alerts
3. Not decided: what to do about Illustrator splitting a text, and about the "Regular" style (`add_text` still has no style argument in v0.16.0); whether to report the `.ai` without PDF compatibility; previews' transparent background; process isolation
4. Not checked yet: whether `artserver-check.vectorcraft` (2026-10-19) opens in the desktop app without a notice
5. Design policy 13.2 #12 still says that whether Claude Code's AI uses the tools correctly is not verified

## 2026-10-25 A font style the family doesn't have: reported upstream as #4128

### What was found

Text made with `add_text` in Hiragino Mincho ProN or Hiragino Sans, with no font style named, opened in the desktop app (0.16.0) with Missing Fonts: "Hiragino Mincho ProN Regular — W3 で代用", "Hiragino Sans Regular — W4 で代用". Drawing, PNG and PDF were right (W3 and W4), but the `.vectorcraft` kept the font style "Regular", which neither family has. "Font style" here is the face that the Character panel's font style menu shows (W3, W6, Regular, Bold…; `CharStyle::font_style`), not a character style of the Character Styles panel (`charStyle.*`).

Read in the engine (v0.16.0): `text.create` and `text.createInPath` start from the default "Regular" and keep it when only `font` is given (`new_type_style`, `crates/engine/src/cmd/create.rs`); `text.setStyle` keeps the text's previous font style (`set_style`, `crates/engine/src/cmd/typecmd.rs`). Only `text.setRangeStyle` takes the family's closest font style when the family lacks the one kept (`CharChange::apply`, `crates/engine/src/cmd/textedit.rs:499-511`). `add_text` has no `style` argument, so text in any family without "Regular" (Hiragino W0–W9, Morisawa and the like) gets this when no font style is named.

The project owner's view: when a request doesn't name a full face, text in an equivalent face is fine, even if the AI's choice varies a little; a font style that isn't installed is not.

### Checked in a scratch copy

In a document of 500 × 320 pt: two texts with `add_text` (Hiragino Mincho ProN, Hiragino Sans), a text made in Helvetica Neue Medium and moved to Hiragino Mincho ProN with `text.setStyle`, and the same with `text.setRangeStyle`.

| | Stored font styles | PDF export | Live-text SVG | Desktop app |
|---|---|---|---|---|
| v0.16.0 | Regular, Regular, Medium, W6 | Warned that three font styles were drawn in the closest one (W3, W4, W6) | `font-weight` 500 for Medium, drawn in W6 | Missing Fonts (the project owner, for a file of the two `add_text` texts) |
| Patched | W3, W3, W6, W6 | No warning | Weights match the PDF (300, 300, 600, 600) | Opened without a notice (the project owner) |

The patch applies `CharChange::apply`'s rule in `new_type_style` and `set_style` when `font` is given without `style`. It was made in a copy of VectorCraft v0.16.0, with a copy of ArtServer `46b7fe7` pointing its 22 VectorCraft crates at it through `[patch]`, both outside the repository; `cargo build` there succeeded in 1m 44s, and the project owner ran ArtServer from that build. A text in Helvetica Neue (which has Regular) kept Regular, and a text made with W6 named kept W6. The project owner's own request from another Claude Code (ヒラギノ明朝で「秋の夜長」、ヒラギノ角ゴで「読書の秋」) gave W3 and W3, and the file opened without a notice.

Seen but not verified: Hiragino Sans was drawn in W4 before the patch and stored as W3 after it. VectorCraft's `FontDb` picks the closest face among the faces it has already loaded (`face` and `find`), and another Claude Code had used Hiragino Kaku Gothic ProN W3 (in the same file as Hiragino Sans W3) in between, so the choice may depend on what the server used before. The project owner accepts this variation, and it was left out of the report.

### Reported upstream

storytold/vectorcraft#4128, posted by the project owner (2026-10-25 20:29), with the steps, the results, the Missing Fonts dialog and the diff. Related: #3229 (a font style the caller names that the family doesn't have). ArtServer changes nothing: the fix belongs in the engine, and ArtServer follows upstream.

The Linux CI (run 40150759011) and the Windows build (run 40151171676, by hand; 15m 33s, 14m 19s of it the build) of `46b7fe7` succeeded.

### Next steps

1. Follow #4128; when a release has the fix, update and check in the desktop app that text with no font style named opens without a notice
2. The project owner restarts ArtServer from the repository's build; the scratch copy (about 7.3 GB) is deleted on the project owner's word
3. Not decided: GitHub's secret scanning and Dependabot alerts
4. Not decided: what to do about Illustrator splitting a text; whether to report the `.ai` without PDF compatibility; previews' transparent background; process isolation
5. Not checked yet: whether `artserver-check.vectorcraft` (2026-10-19) opens in the desktop app without a notice
6. Design policy 13.2 #12 still says that whether Claude Code's AI uses the tools correctly is not verified

## 2026-10-25 `artserver-check.vectorcraft` in the desktop app 0.16.0, and the cleanup

### What was done

- The project owner opened `~/Desktop/artserver-output/artserver-check.vectorcraft` (2026-10-19, written by ArtServer with the engine of VectorCraft 0.13.1, format version 4) in the VectorCraft desktop app 0.16.0. It opened without any notice. Its texts are in Helvetica Neue Regular and Hiragino Sans W6, both font styles the families have, so the case of #4128 doesn't arise in it. Whether the look matches `artserver-check-reference.png` was not reported.
- The project owner restarted ArtServer from the repository's build (`./target/debug/artserver`, 20:35). The scratch copy of the font style patch (`style-patch`, 7.4 GB) was deleted on the project owner's word; no process used it.

### Next steps

1. Follow #4128; when a release has the fix, update and check in the desktop app that text with no font style named opens without a notice
2. Not decided: GitHub's secret scanning and Dependabot alerts
3. Not decided: what to do about Illustrator splitting a text; whether to report the `.ai` without PDF compatibility; previews' transparent background; process isolation
4. Design policy 13.2 #12 still says that whether Claude Code's AI uses the tools correctly is not verified

## 2026-10-25 Secret scanning, push protection and Dependabot alerts

### Policy

The project owner turned on three of GitHub's free features for public repositories. Dependabot security updates (pull requests made by GitHub) stay off: such a pull request rewrites `Cargo.lock`, which can move unrelated crates as `cargo update` did with tokio (windows-sys 0.61 → 0.52, the entry on tokio 1.54.1 above), and every change to `Cargo.lock` here is read before it goes in. An alert is fixed by hand instead. The CI doesn't run `cargo deny`, so until now a new advisory showed only when it was run by hand, as RUSTSEC-2026-0386 did.

### What was done

| Setting | How | State |
|---|---|---|
| Secret scanning | `gh api -X PATCH repos/roundshape/ArtServer` (`security_and_analysis`) | Enabled |
| Push protection | The same call | Enabled; a push with a secret is refused |
| Dependabot alerts | `gh api -X PUT repos/roundshape/ArtServer/vulnerability-alerts` (204) | Enabled |
| Dependabot security updates | Unchanged | Disabled |

Read back afterwards: the settings as above, 0 secret scanning alerts and 0 Dependabot alerts (the scans may still have been running), and 407 packages in the dependency graph, read from `Cargo.lock`. Dependabot matches crates from crates.io; the git dependencies (VectorCraft, craft-libs) aren't covered, so their advisories still come from following upstream.

### Next steps

1. Follow #4128; when a release has the fix, update and check in the desktop app that text with no font style named opens without a notice
2. When an alert comes: read it, fix by hand, and check the change to `Cargo.lock`
3. Not decided: what to do about Illustrator splitting a text; whether to report the `.ai` without PDF compatibility; previews' transparent background; process isolation
4. Design policy 13.2 #12 still says that whether Claude Code's AI uses the tools correctly is not verified
