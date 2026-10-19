# ArtServer Design Review — On Future Process Isolation

- Created: 2026-10-17
- Target project: **ArtServer**
- Intended location: `~/MyTasks/RoundShape/ArtServer/docs/PROCESS_ISOLATION_REVIEW.md`
- Documents reviewed: `design-policy.md` ("ArtServer Design Policy"), `architecture.md` ("ArtServer Architecture Investigation and Approach Decision (Phase 0)")
- **Important: This document is a design review for future development. It does not require process isolation to be implemented in Phase 1 (MVP).**

## 1. Conclusion

**The current Option A-2 (embed `vectorcraft-engine` directly and add ArtServer's own MCP layer) is sound and needs no change.**

On that basis, at the stage where the service is exposed over the internet to **multiple users with different trust boundaries**, **we recommend the policy of isolating the drawing engine's execution environment per OS process**.

The development order is as follows.

1. **Phase 1 (now)**: Use a dedicated thread per session inside a single ArtServer process. As in the existing plan, complete the MVP for local use by a small number of trusted users.
2. **Phase 2 (expand the needed features)**: While verifying engine features and compatibility, keep the same separation of responsibilities. **Do not implement process isolation ahead of time**.
3. **Phase 3 (before exposure to multiple users)**: Implement process isolation and verify safety, performance and operations. Take one process per session as **the first candidate for comparison**, and also evaluate approaches such as a worker pool based on load and usage patterns.

**There is no need to "build a complex isolation mechanism now so that we can isolate later". For now, keep a boundary that can be swapped out, and leave the implementation for the future.**

## 2. Why process isolation is recommended

According to the investigation results under review, VectorCraft's engine has state that is not contained within a session. `architecture.md` §2.5 lists process-wide shared state such as color settings, ICC profiles, the font DB and plugin registration. Also, §4.4 clearly states the limits of in-process execution.

| Aspect | Thread isolation (Phase 1) | Process isolation (future) |
|---|---|---|
| Normal working state such as documents and Undo | Each Session can be isolated | Can be isolated |
| Global settings and shared caches | May be shared across the whole process | As a rule, independent per worker |
| Stack overflows, etc. | May affect the whole server | As a rule, can be confined to that worker |
| Work that cannot be stopped by cooperative cancellation | Hard to force-stop safely | Can be stopped by terminating the worker |
| OS resource control and privilege restriction | Hard to control | Easy to apply per worker |
| Startup, memory and operating cost | Lower | Higher |

**Note:** Running in a separate process does not by itself make things safe. Real multi-tenant operation needs a design for per-worker OS privileges, run directory, file access, CPU and memory limits, network restrictions, etc.

## 3. What to keep in the current design (Phase 1)

Keep the policies in `design-policy.md` §4 and §5 and `architecture.md` §4.1–4.4.

### 3.1 Make the executor's responsibility boundary clear

Do not let the MCP/HTTP layer operate `vectorcraft_engine::Session` directly; gather that in the `session` / `engine_host` layers.

```text
MCP / HTTP
    │
    ▼
Tool / policy layer
    │
    ▼
Session management
    │
    ▼
Engine execution boundary (engine_host)
    │
    ├─ Now: dedicated thread per session
    │
    └─ Future: worker process (Phase 3)
    ▼
vectorcraft-engine
```

The "boundary" here means **separation of roles**. There is no need to build two implementations, `ThreadExecutor` and `ProcessExecutor`, at this point. Turning it into a Rust `trait` is also adopted only if it helps the natural separation of the current code. Do not add abstractions only for a future implementation.

### 3.2 Keep data that crosses the execution boundary simple

Pass command requests, results and errors as **data with clear ownership** (command ID, JSON arguments, results, session identifier, etc.) as much as possible.

- Do not expose the engine's internal `Session`, document objects, locks, etc. to the HTTP layer.
- Do not build engine-specific object references into the MCP API specification.
- Keep in mind that serialization may be needed for inter-process communication in the future. However, **there is no need to define or implement an IPC protocol now**.
- Order matters for edit requests within the same session, so serialize them as in the current design.

### 3.3 Keep the MVP's security constraints as they are

Follow the defaults in `design-policy.md` §6.

- Listen only on `127.0.0.1`.
- Require Bearer token authentication.
- Use an allowlist for engine commands.
- In the MVP, do not expose input/output by file path; pass data as Base64.
- Set upper limits on the number of sessions, queues, request body size, timeouts, etc.
- Do not let other sessions' information or confidential information get into logs.

**Do not offer public internet exposure to untrusted users at this point.** State the failures that cannot be prevented within the process and the limits of shared state clearly in the README as known limitations.

## 4. Implementation options to consider in Phase 3 (not started yet)

### 4.1 Basic form

```text
               Internet
                   │
                   ▼
         ArtServer (front end)
           HTTP / MCP / auth
                   │
          Session management
                   │
           ┌───────┼───────┐
           ▼       ▼       ▼
         Worker  Worker  Worker
        Process Process Process
           │       │       │
         Engine  Engine  Engine
```

Only the workers load `vectorcraft-engine`; the front end handles session routing, authentication, request limits and returning results. Aim for a structure where, if a worker terminates abnormally, that session is treated as failed and the front end keeps running.

### 4.2 Choosing the unit of isolation

- **One process per session**: easy to understand, with a clear failure boundary. The first implementation candidate.
- **One process per user**: there is room to reduce load when one person uses several sessions, but sessions of the same user share failures and shared state.
- **Worker pool**: there is room to reduce startup cost, but resetting state before reuse and preventing leftover data are hard. Do not carelessly mix sessions with different trust boundaries in the same process.

**Do not fix one process per session as a permanent specification.** Choose by measuring the number of users, concurrency, document size, font initialization cost and memory use.

### 4.3 What to design at the start of Phase 3

1. Worker start and stop, idle reclaim, crash detection, restart and session expiry.
2. IPC (comparing candidates such as JSON), request IDs, ordering guarantees, timeouts, cancellation, error reporting.
3. Per-session data storage location and access permissions, process resource limits.
4. Per-process initialization cost of fonts, settings and shared resources.
5. Policy for persisting and recovering document state (whether to promise resumption after a crash).
6. Monitoring, logs, metrics, backpressure, and the upper limit on concurrent processing.
7. OS-level isolation in production environments such as Linux. If needed, also compare containerization.

**Additional note:** Isolation by OS process and isolation by Docker container are not the same thing. Containers are an option for distribution, operations and additional isolation, and are not required from the start.

## 5. Guidelines for deciding to move to Phase 3

Start on process isolation when the following conditions become real requirements.

- External users who do not trust each other use the same ArtServer.
- Abnormal engine termination or long-running work must not affect other users.
- The number of concurrent sessions or the load grows, and resources must be controlled per user/session.
- Features where isolation matters more are exposed, such as input/output with file paths, or user-supplied data and fonts.

When checking the future migration, focus testing on "the front end keeps running even if a worker crashes", "no access to other sessions' state or files", and "a worker can be stopped when it exceeds its limits".

## 6. Review verdict and instructions for Claude Code

### Verdict

**Agree with the design policy, with conditions. There is no need to change the current Phase 1 implementation plan.**

There are only the following three conditions.

1. **Separate the responsibilities of the MCP/HTTP layer and the engine execution layer.**
2. **Document the assumption that the MVP is for local use with trusted clients.**
3. **Design and verify process isolation and OS-level restrictions before exposing the service to users across multiple trust boundaries.**

### Instructions for Claude Code CLI

> This review contains design notes for future exposure to multiple users. **Do not implement process isolation in Phase 1.** Keep the existing Option A-2 and the thread-per-session approach. Only clarify the responsibilities of `engine_host` so that the internal implementation of `Session` does not leak into the MCP layer. Do not implement traits, IPC or a worker manager ahead of time beyond what is needed. First give priority to building the MVP and checking that it works end to end, and record the open issues for Phase 3 in the documentation.

## 7. Assumptions and scope of this review

- This review is based on **the two design documents provided**. This review did not independently verify the VectorCraft source code itself.
- The beginning of `architecture.md` clearly states **static reading only; build/tests not run**. Check assumptions such as `Session: Send` with the compiler when building Phase 1.
- The concrete IPC approach, OS restrictions and whether to adopt a worker pool in the future are decided once the load and safety requirements at implementation time are known.
- Sections referenced in the source documents: `design-policy.md` §§3–6, 10–12 / `architecture.md` §§2.4–2.5, 4.1–4.4, 5–6.
