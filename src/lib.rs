//! ArtServer: VectorCraft のエンジンを MCP（Streamable HTTP）で操作できるようにする、
//! GUI なしのベクターグラフィックスサーバー。構成は docs/design-policy.md 4章を参照。

pub mod auth;
pub mod config;
pub mod engine_host;
pub mod policy;
pub mod session;
pub mod tools;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::routing::get;
use axum::{Json, Router, middleware};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use vectorcraft_engine::file_access::AutomationRoots;

use crate::auth::Auth;
use crate::config::Config;
use crate::engine_host::EngineLimits;
use crate::session::Sessions;
use crate::tools::ArtServer;

/// 期限切れの作業セッションを探す間隔。
const PURGE_INTERVAL: Duration = Duration::from_secs(60);

/// 組み立て済みのサーバー。`router` を待ち受けに渡す。
pub struct App {
    pub router: Router,
    pub sessions: Arc<Sessions>,
    /// エンジンのファイルアクセスを閉じ込める空のディレクトリ。サーバーが動いている間は保持する。
    _sandbox: tempfile::TempDir,
}

impl App {
    /// `config` からサーバーを組み立てる。`shutdown` が立つと MCP の処理を止める。
    pub fn build(config: &Config, shutdown: CancellationToken) -> std::io::Result<Self> {
        let limits = &config.limits;
        // 読み取りは空のディレクトリの中だけ、書き込みは一切なし。MVP ではエンジンにファイルを触らせない
        let sandbox = tempfile::Builder::new().prefix("artserver-").tempdir()?;
        let roots = AutomationRoots::new(Some(sandbox.path()), None)
            .map_err(std::io::Error::other)?
            .ok_or_else(|| std::io::Error::other("automation roots were not created"))?;
        let engine_limits = EngineLimits {
            queue_depth: limits.queue_depth,
            max_documents: limits.max_documents,
            history_bytes: limits.history_bytes,
        };
        let sessions = Arc::new(Sessions::new(limits.max_sessions, limits.session_idle, engine_limits, roots));

        let mcp_config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_allowed_hosts(config.allowed_hosts.clone())
            .with_allowed_origins(config.allowed_origins.clone())
            .enforce_origin_validation()
            .with_max_request_body_bytes(limits.max_body_bytes)
            .with_cancellation_token(shutdown.child_token());
        let factory_sessions = sessions.clone();
        let call_timeout = limits.call_timeout;
        let mcp = StreamableHttpService::new(
            move || Ok(ArtServer::new(factory_sessions.clone(), call_timeout)),
            Arc::new(LocalSessionManager::default()),
            mcp_config,
        );

        let protected = Router::new()
            .nest_service("/mcp", mcp)
            .layer(middleware::from_fn_with_state(Auth::new(&config.token), auth::require_bearer));
        let router = Router::new()
            .route("/healthz", get(|| async { Json(json!({"status": "ok"})) }))
            .merge(protected);
        Ok(Self {
            router,
            sessions,
            _sandbox: sandbox,
        })
    }
}

/// プロセス全体の準備。エンジンの描画スレッド数を抑え、フォントの走査を先に始める。
pub fn prepare_engine(config: &Config) {
    vectorcraft_render::set_default_threads(Some(config.limits.render_threads));
    vectorcraft_text::FontDb::global().scan_in_background();
}

/// `listener` で待ち受け、`signal` が完了するまでサーバーを動かす。
pub async fn serve(config: Config, listener: TcpListener, signal: impl Future<Output = ()> + Send + 'static) -> std::io::Result<()> {
    let shutdown = CancellationToken::new();
    let app = App::build(&config, shutdown.clone())?;

    let sessions = app.sessions.clone();
    let purge_stop = shutdown.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(PURGE_INTERVAL);
        loop {
            tokio::select! {
                _ = tick.tick() => {
                    let purged = sessions.purge_idle();
                    if purged > 0 {
                        tracing::info!(purged, "closed idle sessions");
                    }
                }
                _ = purge_stop.cancelled() => break,
            }
        }
    });

    let stop = shutdown.clone();
    let result = axum::serve(listener, app.router.clone())
        .with_graceful_shutdown(async move {
            signal.await;
            stop.cancel();
        })
        .await;
    shutdown.cancel();
    result
}
