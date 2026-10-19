# ArtServer Development Plan and Claude Code CLI Handover Document

- Created: 2026-10-17
- Project name: **ArtServer**
- Working directory (developer's Mac): `~/MyTasks/RoundShape/ArtServer`
- Implementation language: **Rust**
- Planned release: **GitHub / open source (ArtServer's own code under the MIT license)**
- Reference project: **VectorCraft** — https://github.com/storytold/vectorcraft.git
- Purpose of this document: a handover document so that Claude Code CLI understands the policy and carries out investigation, design, implementation and verification

## 1. Project goals

**ArtServer aims to be a full-featured headless vector graphics server that AI agents and external systems can operate over the network.**

The final product is not just an MCP connection proxy. It reuses VectorCraft's drawing and document features as much as possible, and in the future aims at a feature range equivalent to a server version of Illustrator. This does not mean reusing Adobe's code, trademarks or assets.

Basic principles:
1. **Build on existing assets**: do not rebuild the vector drawing engine from scratch.
2. **No GUI needed**: run headless on a server.
3. **AI-native**: MCP comes first. A REST API for general applications will also be provided later.
4. **Rich functionality**: expand drawing, editing and file compatibility step by step.
5. **Independent project**: use a product name and GitHub repository separate from VectorCraft.
6. **Security first**: assume exposure on the network, and isolate file operations and sessions.
7. **Avoid temporary code becoming permanent**: consider an extensible structure first, and do not pile up ad hoc wrappers.

## 2. Scope

### First milestone (MVP)

- A standalone ArtServer that starts on macOS and Linux **without a GUI**.
- AI can operate it via **MCP over the network (Streamable HTTP as the first candidate)**.
- Create documents, add and change shapes, and export through existing VectorCraft features.
- Actually verify SVG input/output and PDF output (state the available features and limits clearly).
- Include authentication and access restrictions, working directory restrictions, error handling and tests.
- Can be reproduced in a local environment by following only the README steps.

### Future extensions

- Editing of paths, fills, strokes, groups, layers, artboards, text, images, masks, transforms, and so on.
- Stronger compatibility with PDF / PDF-compatible AI / SVG and others. **Full compatibility with Adobe native `.ai` is not guaranteed**.
- REST API and SDK, integration with external applications.
- Multiple users, document isolation per user/session, authentication and authorization.
- Job queue, asynchronous batches, bulk rendering, monitoring, container distribution.
- Expansion into figures, print data and automated typesetting linked to databases and CSV.

## 3. Investigate the existing VectorCraft first

The official repository (README as of 2026-10-17) uses a Rust workspace and describes roughly the following structure.

- `crates/`: `engine`, `render`, `doc`, `svg`, `pdf`, `mcp`, etc.
- `apps/vectorcraft-cli`: entry point for the CLI, headless processing and MCP
- Official example: `cargo run --release -p vectorcraft-cli -- mcp` (**stdio MCP**)
- Official example: `cargo run --release -p vectorcraft-cli -- run --in examples/ribbons.vectorcraft --export out.pdf`
- License: `LICENSE-MIT`, `LICENSE-APACHE` and `NOTICE` are at the root.

However, **this information is only a starting point for the investigation.** Actually check the source code, README, `docs/mcp.md`, `docs/control-protocol.md`, `AGENTS.md` and `Cargo.toml` as of the start of implementation, and give priority to the current state.

Items to investigate:
1. Whether core drawing, documents and file I/O can be used as **UI-independent Rust crates**.
2. Whether the existing MCP is stdio only, or whether an HTTP transport is already implemented.
3. The list of MCP tools, document state management, export features, file access control.
4. The implementation and effectiveness of boundary settings such as `--automation-read-root` / `--automation-write-root`.
5. Third-party dependencies such as licenses, assets, fonts and the PDF backend.
6. Conditions for building, testing and running headless on macOS Apple Silicon and Linux.
7. A suitable way to manage dependencies so that upstream updates can be taken in later.

References:
- https://github.com/storytold/vectorcraft
- https://github.com/storytold/vectorcraft/blob/main/docs/mcp.md
- https://github.com/storytold/vectorcraft/blob/main/docs/control-protocol.md
- https://github.com/storytold/vectorcraft/blob/main/AGENTS.md
- https://github.com/storytold/vectorcraft/blob/main/LICENSE-MIT
- https://github.com/storytold/vectorcraft/blob/main/NOTICE

## 4. Technical policy (finalized after the investigation)

Ideal structure:

```text
Claude Code / Codex / other MCP clients
                 │
        HTTPS + MCP (Streamable HTTP)
                 ▼
         ArtServer (Rust)
         ├─ Authentication / authorization
         ├─ MCP transport / API
         ├─ Session / document management
         ├─ File access control
         └─ Engine derived from VectorCraft
                    ├─ Documents / commands
                    ├─ Drawing / path processing
                    └─ SVG / PDF / AI compatibility
```

Compare the implementation options and record the reasons.

- **Option A: Reference the needed VectorCraft crates as Rust libraries**. As a rule, consider this one first. It makes it easy to separate the engine and the network layer.
- **Option B: Expose the existing CLI's stdio MCP through a secure HTTP gateway**. May suit an early MVP. However, managing processes, sessions, errors and performance is a challenge.
- **Option C: Copy only the needed sources into ArtServer and adapt them**. High independence, but the burden of following upstream and maintaining duplicates grows. Choose only if the need is shown.

**Do not start a large-scale port before the architecture is decided.** Do not prioritize HTTP connectivity alone and end up with a dangerous remote command execution server.

## 5. Phased development plan

### Phase 0 — Source investigation and approach decision

- Examine the reference project's current source and existing CLI, and verify the MCP commands and licenses.
- If possible, reproduce command execution, SVG/PDF output and tests.
- Record a comparison table of Options A/B/C, the chosen option, the rationale and open issues in `docs/architecture.md`.
- Record the VectorCraft **commit SHA** used at the start of implementation.
- Do not move, delete or overwrite the user's existing working directories without permission.

### Phase 1 — Minimal ArtServer (first implementation)

- Create a standalone Rust server binary `artserver`.
- Provide MCP connections that follow the standard specification at an endpoint such as `/mcp`.
- Verify minimal document creation, shape operations and SVG/PDF output end to end.
- The initial bind address is **127.0.0.1**. Public listening is disabled by default.
- Design and implement authentication such as access tokens. For TLS in transit, consider approaches such as a trusted reverse proxy.
- Implement path restrictions, session isolation, input size limits, timeouts and error responses.
- Document in the README how to start it, how to connect from Claude Code and others, health checks with curl and similar tools, and working examples.

### Phase 2 — Document features and compatibility

- Make existing engine features usable from ArtServer one by one.
- Verify document state, save formats and file compatibility with real files.
- Verify Unicode and CJK fonts (such as Japanese), images, layers, transparency, path processing, etc.
- Continue automated tests and performance measurement.

### Phase 3 — Turning the server into a product

- REST API, multiple users, authorization, persistence of sessions and jobs.
- Asynchronous batch processing, monitoring, logging, Docker, production deployment on Linux.
- Threat analysis, load testing and license/trademark checks before external release.

## 6. Security requirements

- **Do not design it so that arbitrary shell commands can be run** from MCP tools.
- The user running the server has least privilege. Do not trust input files.
- Restrict the readable and writable roots, and check for escapes through `..`, symbolic links, absolute paths, etc.
- Isolate documents, temporary files and processes between sessions. Prevent data from mixing between users.
- Set limits on request size, memory, CPU time, page/object counts, etc.
- Do not unconditionally write secrets, access tokens or the contents of user documents to logs.
- For remote exposure, consider TLS, authentication/authorization, Origin/Host protections and rate limiting, and forbid exposing it to the whole world without authentication.
- Check the main code and dependency libraries for known vulnerabilities.

## 7. Handling licenses and brands

- VectorCraft's root is **MIT OR Apache-2.0**. For the code and dependencies being reused, check the applicable terms file by file.
- If MIT is chosen, keep the original copyright notice and MIT license text in the required distributions. Also follow the terms for parts where Apache-2.0 is chosen and for third-party components.
- The policy is to release ArtServer's own code under MIT. However, **do not uniformly rewrite the licenses of mixed-in components to MIT**.
- Check and include `NOTICE`, license information, and the terms for assets, fonts and the PDF backend.
- The code license does not grant the right to use the VectorCraft/ArtCraft names, logos, images, etc. **ArtServer is released under its own name and its own logo**.
- Check for conflicts of "ArtServer" with existing product names and trademarks before release.
- Give proper credit for the original project's name and attribution in the README. Do not let it be mistaken for an official derivative.

## 8. Concrete first tasks for Claude Code CLI

**The following are the first work instructions.**

1. Check the state of `~/MyTasks/RoundShape/ArtServer` and protect existing files.
2. Read this document and understand the goals and constraints.
3. Find where the current VectorCraft source is. If it is already cloned, use that. If not, consider cloning it under `upstream/` or similar so it is not confused with other code (do not clone it twice needlessly).
4. Investigate the official `README.md`, `AGENTS.md`, `docs/mcp.md`, `Cargo.toml`, the core crates and the licenses.
5. Verify how MCP/CLI are actually started and whether HTTP is already supported.
6. If building and testing are possible, run them and record the environment, commands and results.
7. **Decide the architecture to adopt and the minimal implementation scope**, and write them in `docs/architecture.md`.
8. Implement a minimal MVP and, if possible, go as far as confirming that "create a rectangle from AI → SVG output" runs.
9. Prepare `README.md`, `LICENSE`, `THIRD_PARTY_NOTICES.md` (as needed), tests and `docs/progress.md`.
10. At the end, report what is implemented, what is not implemented, known limitations, how to verify, and next tasks.

**Decision rule along the way:** If serious unknowns remain about whether reusing the existing engine is sound, or about licenses, do not go ahead with reusing code or releasing based on guesses; report the findings and the options.

## 9. MVP completion criteria

- ArtServer starts without a GUI and accepts MCP connections over local HTTP.
- Shape creation and SVG export actually succeed from a supported MCP client.
- Differences from the existing VectorCraft, license information and adopted dependencies are documented.
- Basic error cases (invalid input, authentication failure, disallowed paths, timeouts) are tested.
- There are reproducible build and run steps.
- If any safety measures for production are not implemented, clearly mark them as not implemented.

---

**Example start instruction for Claude Code CLI**

> Read `DEVELOPMENT_PLAN.md` in this directory and start developing ArtServer. First carry out the Phase 0 investigation, and decide the architecture to adopt based on its results. Then start on the Phase 1 MVP. Do not destroy existing files, pay close attention to the licenses of reused code and to safety in MCP connections, and record your work and test results.
