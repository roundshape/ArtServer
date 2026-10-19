//! The engine execution boundary.
//!
//! Each working session has one dedicated thread, and only that thread touches the engine's `Session`. Only
//! command IDs, tool names and JSON arguments and results cross the boundary (docs/design-policy.md
//! section 4.3). Requests to the same working session run one at a time in the order they arrive; when
//! the queue is full, they are refused as busy. Across working sessions, at most `--max-running` jobs run at
//! once ([`RunSlots`]); the others wait for a free slot.

use std::fmt;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};
use vectorcraft_engine::cancel::{self, CancelToken};
use vectorcraft_engine::file_access::{self, AutomationRoots};
use vectorcraft_engine::{EngineError, Session};
use vectorcraft_mcp::ToolResult;

use crate::backend::GuardedBackend;
use crate::policy::{self, Caller};

/// Stack size of the engine thread. Imports run on threads of their own with large stacks, so this only
/// needs to be big enough for ordinary commands.
const STACK_SIZE: usize = 8 * 1024 * 1024;

/// How often a job waiting for a slot checks whether its caller gave up.
const SLOT_POLL: Duration = Duration::from_millis(50);

/// The jobs allowed to run at once across all working sessions (docs/design-policy.md section 5.3). Each
/// export already uses its session's thread and `--render-threads` rendering threads, so this keeps many
/// sessions working at once from taking every core.
pub struct RunSlots {
    free: Mutex<usize>,
    freed: Condvar,
}

impl RunSlots {
    pub fn new(slots: usize) -> Self {
        Self {
            free: Mutex::new(slots.max(1)),
            freed: Condvar::new(),
        }
    }

    /// Waits for a free slot. `None` when `token` is cancelled first: the caller gave up or ran out of time
    /// (the time limit counts the wait), so the job doesn't run.
    fn take(&self, token: &CancelToken) -> Option<RunSlot<'_>> {
        let mut free = self.free.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if token.is_cancelled() {
                return None;
            }
            if *free > 0 {
                *free -= 1;
                return Some(RunSlot { slots: self });
            }
            free = self.freed.wait_timeout(free, SLOT_POLL).unwrap_or_else(PoisonError::into_inner).0;
        }
    }
}

/// A slot taken from [`RunSlots`], given back when dropped.
struct RunSlot<'a> {
    slots: &'a RunSlots,
}

impl Drop for RunSlot<'_> {
    fn drop(&mut self) {
        *self.slots.free.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.slots.freed.notify_one();
    }
}

/// One command for the engine to run.
#[derive(Clone, Debug)]
pub struct Step {
    pub command: &'static str,
    pub params: Value,
    /// Replace `$N.path` in the arguments with the results of the earlier steps? Used only for arguments a
    /// tool built, so that a user's string is never mistaken for a reference.
    pub resolve_refs: bool,
}

impl Step {
    pub fn new(command: &'static str, params: Value) -> Self {
        Self {
            command,
            params,
            resolve_refs: false,
        }
    }

    /// A step that refers to the result of an earlier step (e.g. `"ids": ["$0.id"]`).
    pub fn referring(command: &'static str, params: Value) -> Self {
        Self {
            command,
            params,
            resolve_refs: true,
        }
    }
}

/// A failed run.
#[derive(Debug)]
pub enum HostError {
    /// The queue is full
    Busy,
    /// The execution thread has stopped
    Stopped,
    /// Refused by the allowlist, the argument checks or a resource limit
    Rejected(String),
    /// The engine returned an error
    Engine(String),
    /// Cancelled (nothing was changed)
    Cancelled,
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::Busy => f.write_str("the session is busy with earlier requests; retry later"),
            HostError::Stopped => f.write_str("the session's engine has stopped"),
            HostError::Rejected(m) => write!(f, "rejected: {m}"),
            HostError::Engine(m) => f.write_str(m),
            HostError::Cancelled => f.write_str("cancelled; nothing was changed"),
        }
    }
}

/// Limits of the executor.
#[derive(Clone, Debug)]
pub struct EngineLimits {
    pub queue_depth: usize,
    pub max_documents: usize,
    pub history_bytes: usize,
}

/// Work for the executor.
enum Work {
    /// A sequence of commands built by an ArtServer tool
    Steps(Vec<Step>),
    /// One VectorCraft tool (run on GuardedBackend)
    Tool { name: String, args: Value },
}

/// The result of the work.
enum Done {
    Steps(Vec<Value>),
    Tool(ToolResult),
}

struct Job {
    work: Work,
    token: CancelToken,
    reply: oneshot::Sender<Result<Done, HostError>>,
}

/// The engine executor of one working session. Dropping it ends the execution thread too.
pub struct EngineHost {
    jobs: mpsc::Sender<Job>,
}

impl EngineHost {
    /// Starts a dedicated thread that holds a new engine `Session`.
    /// The engine's file access is always confined to `roots`, and each job runs in one of the shared `slots`.
    pub fn spawn(limits: EngineLimits, roots: Arc<AutomationRoots>, slots: Arc<RunSlots>) -> std::io::Result<Self> {
        let (jobs, rx) = mpsc::channel(limits.queue_depth.max(1));
        std::thread::Builder::new()
            .name("artserver-session".into())
            .stack_size(STACK_SIZE)
            .spawn(move || serve(rx, &limits, &roots, &slots))?;
        Ok(Self { jobs })
    }

    /// Runs `steps` in order and returns each step's result. Setting `token` asks the engine to stop.
    pub async fn run(&self, steps: Vec<Step>, token: CancelToken) -> Result<Vec<Value>, HostError> {
        match self.submit(Work::Steps(steps), token).await? {
            Done::Steps(results) => Ok(results),
            Done::Tool(_) => Err(HostError::Stopped),
        }
    }

    /// Runs VectorCraft's tool `name` with `args`. Every call into the engine goes through
    /// GuardedBackend's checks.
    pub async fn run_tool(&self, name: String, args: Value, token: CancelToken) -> Result<ToolResult, HostError> {
        match self.submit(Work::Tool { name, args }, token).await? {
            Done::Tool(result) => Ok(result),
            Done::Steps(_) => Err(HostError::Stopped),
        }
    }

    async fn submit(&self, work: Work, token: CancelToken) -> Result<Done, HostError> {
        let (reply, answer) = oneshot::channel();
        self.jobs.try_send(Job { work, token, reply }).map_err(|e| match e {
            TrySendError::Full(_) => HostError::Busy,
            TrySendError::Closed(_) => HostError::Stopped,
        })?;
        answer.await.map_err(|_| HostError::Stopped)?
    }
}

fn serve(mut jobs: mpsc::Receiver<Job>, limits: &EngineLimits, roots: &Arc<AutomationRoots>, slots: &RunSlots) {
    let mut session = Session::new();
    while let Some(job) = jobs.blocking_recv() {
        let result = match slots.take(&job.token) {
            // Don't run a request the caller gave up on while it was waiting, in the queue or for a slot
            None => Err(HostError::Cancelled),
            Some(slot) => {
                let result = file_access::confine(Some(roots), || {
                    cancel::scope(&job.token, None, || match &job.work {
                        Work::Steps(steps) => run_steps(&mut session, steps, limits).map(Done::Steps),
                        Work::Tool { name, args } => {
                            let result = vectorcraft_mcp::call_tool(&mut GuardedBackend::new(&mut session, limits), name, args);
                            // A cancelled tool ran only part of the way, so report the cancellation rather than a result
                            if job.token.is_cancelled() {
                                Err(HostError::Cancelled)
                            } else {
                                Ok(Done::Tool(result))
                            }
                        }
                    })
                });
                drop(slot);
                result
            }
        };
        let _ = job.reply.send(result);
    }
}

fn run_steps(session: &mut Session, steps: &[Step], limits: &EngineLimits) -> Result<Vec<Value>, HostError> {
    let mut results = Vec::with_capacity(steps.len());
    for step in steps {
        let params = if step.resolve_refs {
            vectorcraft_engine::steps::resolve(&step.params, &results).map_err(HostError::Rejected)?
        } else {
            step.params.clone()
        };
        results.push(execute_checked(session, step.command, &params, limits, Caller::ArtServer)?);
    }
    Ok(results)
}

/// Checks the allowlist, the arguments and the resource limits, then runs one command. Every command
/// for the engine goes through here.
pub(crate) fn execute_checked(
    session: &mut Session,
    command: &str,
    params: &Value,
    limits: &EngineLimits,
    caller: Caller,
) -> Result<Value, HostError> {
    policy::check(command, params, caller).map_err(HostError::Rejected)?;
    // The engine doesn't check for unknown arguments in release builds, so check them here
    session.check_params(command, params).map_err(|e| HostError::Rejected(e.to_string()))?;
    let opens_document = matches!(command, "file.new" | "document.open");
    if opens_document && session.documents().len() >= limits.max_documents {
        return Err(HostError::Rejected(format!(
            "at most {} documents may be open in one session",
            limits.max_documents
        )));
    }
    let value = session.execute(command, params).map_err(engine_error)?;
    if opens_document && let Ok(doc) = session.doc_mut() {
        doc.history.byte_budget = limits.history_bytes;
    }
    Ok(value)
}

fn engine_error(e: EngineError) -> HostError {
    match e {
        EngineError::Cancelled => HostError::Cancelled,
        other => HostError::Engine(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use serde_json::json;

    fn host(dir: &tempfile::TempDir, max_documents: usize) -> EngineHost {
        host_with_slots(dir, max_documents, Arc::new(RunSlots::new(4)))
    }

    fn host_with_slots(dir: &tempfile::TempDir, max_documents: usize, slots: Arc<RunSlots>) -> EngineHost {
        let roots = AutomationRoots::new(Some(dir.path()), None).unwrap().unwrap();
        EngineHost::spawn(
            EngineLimits {
                queue_depth: 4,
                max_documents,
                history_bytes: 8 << 20,
            },
            roots,
            slots,
        )
        .unwrap()
    }

    #[test]
    fn a_slot_is_waited_for_and_handed_over() {
        let slots = Arc::new(RunSlots::new(1));
        let held = slots.take(&CancelToken::new()).unwrap();
        let (taken, got) = std::sync::mpsc::channel();
        let waiter = {
            let slots = slots.clone();
            std::thread::spawn(move || {
                let slot = slots.take(&CancelToken::new());
                taken.send(slot.is_some()).unwrap();
            })
        };
        assert!(got.recv_timeout(Duration::from_millis(200)).is_err(), "no slot is free yet");
        drop(held);
        assert!(got.recv_timeout(Duration::from_secs(5)).unwrap(), "the freed slot is handed over");
        waiter.join().unwrap();
    }

    #[test]
    fn a_cancelled_job_stops_waiting_for_a_slot() {
        let slots = Arc::new(RunSlots::new(1));
        let _held = slots.take(&CancelToken::new()).unwrap();
        let token = CancelToken::new();
        let waiter = {
            let (slots, token) = (slots.clone(), token.clone());
            std::thread::spawn(move || slots.take(&token).is_some())
        };
        token.cancel();
        assert!(!waiter.join().unwrap(), "a cancelled job gets no slot");
    }

    #[tokio::test]
    async fn sessions_share_the_slots() {
        // With one slot, a job of another session waits until the slot is free; a cancelled one never runs
        let dir = tempfile::tempdir().unwrap();
        let slots = Arc::new(RunSlots::new(1));
        let host = host_with_slots(&dir, 2, slots.clone());
        let held = slots.take(&CancelToken::new()).unwrap();
        let waiting = tokio::spawn(async move { host.run(vec![Step::new("file.new", json!({}))], CancelToken::new()).await.is_ok() });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!waiting.is_finished(), "the job waits for the slot");
        drop(held);
        assert!(waiting.await.unwrap(), "the job runs once the slot is free");

        let other = host_with_slots(&dir, 2, slots.clone());
        let _held = slots.take(&CancelToken::new()).unwrap();
        let token = CancelToken::new();
        token.cancel();
        let err = other.run(vec![Step::new("file.new", json!({}))], token).await.unwrap_err();
        assert!(matches!(err, HostError::Cancelled), "{err}");
    }

    #[tokio::test]
    async fn rectangle_exports_as_svg() {
        let dir = tempfile::tempdir().unwrap();
        let host = host(&dir, 2);
        let steps = vec![
            Step::new("file.new", json!({"width": 200, "height": 100})),
            Step::new("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 30})),
            Step::referring("paint.setFill", json!({"color": "#ff0000", "ids": ["$1.id"]})),
            Step::new("document.export", json!({"format": "svg"})),
        ];
        let results = host.run(steps, CancelToken::new()).await.unwrap();
        let data = results[3]["dataBase64"].as_str().unwrap();
        let svg = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(data).unwrap()).unwrap();
        assert!(svg.contains("<svg"), "{svg}");
        assert!(
            svg.to_ascii_lowercase().contains("#ff0000") || svg.to_ascii_lowercase().contains("#f00"),
            "{svg}"
        );
    }

    #[tokio::test]
    async fn refused_commands_never_reach_the_engine() {
        let dir = tempfile::tempdir().unwrap();
        let host = host(&dir, 2);
        let err = host.run(vec![Step::new("prefs.set", json!({}))], CancelToken::new()).await.unwrap_err();
        assert!(matches!(err, HostError::Rejected(_)), "{err}");
        let err = host
            .run(vec![Step::new("file.new", json!({"bogus": 1}))], CancelToken::new())
            .await
            .unwrap_err();
        assert!(matches!(err, HostError::Rejected(_)), "{err}");
    }

    #[tokio::test]
    async fn document_count_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let host = host(&dir, 1);
        host.run(vec![Step::new("file.new", json!({}))], CancelToken::new()).await.unwrap();
        let err = host.run(vec![Step::new("file.new", json!({}))], CancelToken::new()).await.unwrap_err();
        assert!(matches!(err, HostError::Rejected(_)), "{err}");
    }

    #[tokio::test]
    async fn a_cancelled_request_does_not_run() {
        let dir = tempfile::tempdir().unwrap();
        let host = host(&dir, 2);
        let token = CancelToken::new();
        token.cancel();
        let err = host.run(vec![Step::new("file.new", json!({}))], token).await.unwrap_err();
        assert!(matches!(err, HostError::Cancelled), "{err}");
        // Nothing was created, so no shape can be drawn
        let err = host
            .run(
                vec![Step::new("shape.rectangle", json!({"x": 0, "y": 0, "width": 1, "height": 1}))],
                CancelToken::new(),
            )
            .await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn linked_files_outside_the_roots_are_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let host = host(&dir, 2);
        let secret = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(secret.path(), b"\x89PNG not really").unwrap();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><image href="{}" width="10" height="10"/></svg>"#,
            secret.path().display()
        );
        let data = base64::engine::general_purpose::STANDARD.encode(svg);
        let results = host
            .run(
                vec![Step::new("document.open", json!({"name": "a.svg", "dataBase64": data}))],
                CancelToken::new(),
            )
            .await
            .unwrap();
        let missing = results[0]["missingLinks"].as_array().map_or(0, Vec::len);
        let warnings = results[0]["warnings"].to_string();
        assert!(
            missing > 0 || warnings.contains("not granted") || warnings.contains("rejected"),
            "{}",
            results[0]
        );
    }

    async fn tool(host: &EngineHost, name: &str, args: Value) -> ToolResult {
        host.run_tool(name.to_string(), args, CancelToken::new()).await.unwrap()
    }

    fn text_of(r: &ToolResult) -> String {
        r.content.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n")
    }

    async fn host_with_document(dir: &tempfile::TempDir) -> EngineHost {
        let host = host(dir, 2);
        host.run(vec![Step::new("file.new", json!({"width": 300, "height": 200}))], CancelToken::new())
            .await
            .unwrap();
        host
    }

    #[tokio::test]
    async fn vectorcraft_tools_run_through_the_guarded_backend() {
        let dir = tempfile::tempdir().unwrap();
        let host = host_with_document(&dir).await;
        let r = tool(&host, "add_text", json!({"text": "こんにちは ArtServer", "x": 20, "y": 50, "size": 24})).await;
        assert!(!r.is_error, "{}", text_of(&r));
        let r = tool(
            &host,
            "draw_shape",
            json!({"shape": "rectangle", "x": 10, "y": 10, "width": 50, "height": 30, "fill": "#3366ff", "stroke": "none"}),
        )
        .await;
        assert!(!r.is_error, "{}", text_of(&r));
        let id = serde_json::from_str::<Value>(&text_of(&r)).unwrap()["id"].as_u64().unwrap();
        let r = tool(&host, "transform", json!({"ids": [id], "dx": 5, "rotate": 15})).await;
        assert!(!r.is_error, "{}", text_of(&r));
        let r = tool(&host, "render_preview", json!({"max_side": 200})).await;
        assert!(!r.is_error, "{}", text_of(&r));
        assert_eq!(r.content[0]["type"], "image");
    }

    #[tokio::test]
    async fn the_guarded_backend_refuses_what_the_policy_does_not_allow() {
        let dir = tempfile::tempdir().unwrap();
        let host = host_with_document(&dir).await;
        for (name, args) in [
            ("command_run", json!({"id": "prefs.set", "params": {}})),
            ("command_run", json!({"id": "document.export", "params": {"format": "svg"}})),
            ("command_run", json!({"id": "file.new", "params": {}})),
            ("command_batch", json!({"steps": [{"id": "plugin.install", "params": {}}]})),
        ] {
            let r = tool(&host, name, args.clone()).await;
            assert!(text_of(&r).contains("not allowed"), "{name} {args}: {}", text_of(&r));
        }
        // The gateways for operating the UI and for reading and writing files can't be used even by naming the tool
        for (name, args) in [("select_tool", json!({"tool": "pen"})), ("open_file", json!({"path": "/etc/hosts"}))] {
            let r = tool(&host, name, args).await;
            assert!(r.is_error, "{name}: {}", text_of(&r));
            assert!(text_of(&r).contains("not available"), "{name}: {}", text_of(&r));
        }
    }

    #[tokio::test]
    async fn command_list_shows_only_allowed_commands() {
        let dir = tempfile::tempdir().unwrap();
        let host = host_with_document(&dir).await;
        let r = tool(&host, "command_list", json!({"brief": true})).await;
        assert!(!r.is_error, "{}", text_of(&r));
        let list: Value = serde_json::from_str(&text_of(&r)).unwrap();
        let ids: Vec<&str> = list.as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
        assert!(ids.contains(&"text.setText"), "{ids:?}");
        assert!(ids.iter().all(|id| policy::is_allowed(id, Caller::VectorCraftTool)), "{ids:?}");
        assert!(!ids.contains(&"document.export") && !ids.contains(&"prefs.set"), "{ids:?}");
    }

    #[tokio::test]
    async fn japanese_weight_names_reach_the_svg() {
        // The vendor/vectorcraft-svg fix: W0–W9 style names become the SVG's font-weight.
        // The SVG gets a value decided only by the given style name, so the result is the same where the fonts aren't installed
        let dir = tempfile::tempdir().unwrap();
        let host = host_with_document(&dir).await;
        let mut steps = Vec::new();
        for (i, (font, style)) in [
            ("Hiragino Sans", "W6"),
            ("Hiragino Mincho ProN", "W3"),
            ("Inter", "Regular"),
            ("Source Sans 3", "Bold"),
        ]
        .into_iter()
        .enumerate()
        {
            let y = 30 + 30 * i;
            steps.push(Step::new(
                "text.create",
                json!({"x": 10, "y": y, "text": format!("t{i}"), "font": font, "style": style}),
            ));
        }
        steps.push(Step::new("document.export", json!({"format": "svg"})));
        let results = host.run(steps, CancelToken::new()).await.unwrap();
        let data = results[4]["dataBase64"].as_str().unwrap();
        let svg = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(data).unwrap()).unwrap();
        let text_tag = |name: &str| {
            svg.split("<text")
                .find(|t| t.contains(&format!(">{name}<")) || t.contains(&format!(">{name}</")))
                .map(|t| t.split('>').next().unwrap_or_default().to_string())
                .unwrap_or_else(|| panic!("no <text> for {name}: {svg}"))
        };
        assert!(text_tag("t0").contains(r#"font-weight="600""#), "{}", text_tag("t0"));
        assert!(text_tag("t1").contains(r#"font-weight="300""#), "{}", text_tag("t1"));
        assert!(!text_tag("t2").contains("font-weight"), "{}", text_tag("t2"));
        assert!(text_tag("t3").contains(r#"font-weight="bold""#), "{}", text_tag("t3"));
    }
}
