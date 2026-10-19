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
| Run | run 39084451667 (commit `4390de1`) |
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

- Committed as `8c0819f`, `66cdf16` and `d40af63` and pushed to main. The Linux build on GitHub Actions could not be checked with `gh` (HTTP 404: `gh` is logged in with a different account from the SSH alias used for push).

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

The project owner cleared `~/Desktop/artserver-output/`, and the checks were run again with the server built from `d05794e`. The English line now used Helvetica Neue, which is installed on the Mac, instead of the bundled Inter.

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
