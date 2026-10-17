//! エンジン実行境界。
//!
//! 作業セッション1つにつき専用スレッドを1本持ち、そのスレッドだけがエンジンの `Session` を
//! 触る。境界を越えるのはコマンド ID と JSON の引数・結果だけ（docs/design-policy.md 4.3節）。
//! 同じ作業セッションへの要求は届いた順に1件ずつ実行し、待ち行列が溢れたら混雑として断る。

use std::fmt;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};
use vectorcraft_engine::cancel::{self, CancelToken};
use vectorcraft_engine::file_access::{self, AutomationRoots};
use vectorcraft_engine::{EngineError, Session};

use crate::policy;

/// エンジンのスレッドのスタック。読み込み処理は自前の大きなスタックのスレッドを使うので、
/// ここは通常のコマンドが足りる大きさでよい。
const STACK_SIZE: usize = 8 * 1024 * 1024;

/// エンジンに実行させる1コマンド。
#[derive(Clone, Debug)]
pub struct Step {
    pub command: &'static str,
    pub params: Value,
    /// 引数の中の `$N.path` を、それまでのステップの結果で置き換えるか。
    /// 利用者の文字列を参照と取り違えないよう、ツールが組み立てた引数にだけ使う。
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

    /// 前のステップの結果を参照するステップ（例: `"ids": ["$0.id"]`）。
    pub fn referring(command: &'static str, params: Value) -> Self {
        Self {
            command,
            params,
            resolve_refs: true,
        }
    }
}

/// 実行の失敗。
#[derive(Debug)]
pub enum HostError {
    /// 待ち行列が一杯
    Busy,
    /// 実行スレッドが止まっている
    Stopped,
    /// 許可リストや引数の検査、資源の上限で断った
    Rejected(String),
    /// エンジンがエラーを返した
    Engine(String),
    /// キャンセルされた（何も変更していない）
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

/// 実行器の上限。
#[derive(Clone, Debug)]
pub struct EngineLimits {
    pub queue_depth: usize,
    pub max_documents: usize,
    pub history_bytes: usize,
}

struct Job {
    steps: Vec<Step>,
    token: CancelToken,
    reply: oneshot::Sender<Result<Vec<Value>, HostError>>,
}

/// 作業セッション1つ分のエンジン実行器。落とすと実行スレッドも終わる。
pub struct EngineHost {
    jobs: mpsc::Sender<Job>,
}

impl EngineHost {
    /// 専用スレッドを起こし、そこで新しいエンジンの `Session` を持つ。
    /// エンジンのファイルアクセスは常に `roots` の中に閉じ込める。
    pub fn spawn(limits: EngineLimits, roots: Arc<AutomationRoots>) -> std::io::Result<Self> {
        let (jobs, rx) = mpsc::channel(limits.queue_depth.max(1));
        std::thread::Builder::new()
            .name("artserver-session".into())
            .stack_size(STACK_SIZE)
            .spawn(move || serve(rx, &limits, &roots))?;
        Ok(Self { jobs })
    }

    /// `steps` を順に実行し、各ステップの結果を返す。`token` を立てるとエンジンに中断を求める。
    pub async fn run(&self, steps: Vec<Step>, token: CancelToken) -> Result<Vec<Value>, HostError> {
        let (reply, answer) = oneshot::channel();
        self.jobs.try_send(Job { steps, token, reply }).map_err(|e| match e {
            TrySendError::Full(_) => HostError::Busy,
            TrySendError::Closed(_) => HostError::Stopped,
        })?;
        answer.await.map_err(|_| HostError::Stopped)?
    }
}

fn serve(mut jobs: mpsc::Receiver<Job>, limits: &EngineLimits, roots: &Arc<AutomationRoots>) {
    let mut session = Session::new();
    while let Some(job) = jobs.blocking_recv() {
        // 待っている間に呼び出し元が諦めた要求は実行しない
        let result = if job.token.is_cancelled() {
            Err(HostError::Cancelled)
        } else {
            file_access::confine(Some(roots), || {
                cancel::scope(&job.token, None, || run_steps(&mut session, &job.steps, limits))
            })
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
        policy::check(step.command, &params).map_err(HostError::Rejected)?;
        // エンジンは本番ビルドでは未知の引数を検査しないので、ここで検査する
        session
            .check_params(step.command, &params)
            .map_err(|e| HostError::Rejected(e.to_string()))?;
        let opens_document = matches!(step.command, "file.new" | "document.open");
        if opens_document && session.documents().len() >= limits.max_documents {
            return Err(HostError::Rejected(format!(
                "at most {} documents may be open in one session",
                limits.max_documents
            )));
        }
        let value = session.execute(step.command, &params).map_err(engine_error)?;
        if opens_document && let Ok(doc) = session.doc_mut() {
            doc.history.byte_budget = limits.history_bytes;
        }
        results.push(value);
    }
    Ok(results)
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
        let roots = AutomationRoots::new(Some(dir.path()), None).unwrap().unwrap();
        EngineHost::spawn(
            EngineLimits {
                queue_depth: 4,
                max_documents,
                history_bytes: 8 << 20,
            },
            roots,
        )
        .unwrap()
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
        // 何も作られていないので、図形は描けない
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
}
