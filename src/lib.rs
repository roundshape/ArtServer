//! ArtServer: a headless vector graphics server that makes VectorCraft's engine operable over MCP
//! (Streamable HTTP). See chapter 4 of docs/design-policy.md for the structure.

pub mod auth;
mod backend;
pub mod config;
pub mod engine_host;
pub mod files;
mod fonts;
pub mod policy;
pub mod session;
pub mod tools;
pub mod uploads;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
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
use crate::files::{Downloads, FileStore};
use crate::session::Sessions;
use crate::tools::ArtServer;
use crate::uploads::Uploads;

/// How often to look for expired working sessions.
const PURGE_INTERVAL: Duration = Duration::from_secs(60);

/// The assembled server. Pass `router` to the listener.
pub struct App {
    pub router: Router,
    pub sessions: Arc<Sessions>,
    /// The file store for large exports and uploaded files (docs/design-policy.md section 4.6)
    pub files: Arc<FileStore>,
    /// The empty directory the engine's file access is confined to. Kept while the server runs.
    _sandbox: tempfile::TempDir,
}

impl App {
    /// Assembles the server from `config`. When `shutdown` is set, MCP processing stops.
    pub fn build(config: &Config, shutdown: CancellationToken) -> std::io::Result<Self> {
        let limits = &config.limits;
        // Reads only inside the empty directory and no writes at all: in the MVP the engine touches no files
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
        let files = Arc::new(FileStore::new(limits.file_ttl, limits.max_file_bytes));

        let mcp_config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_allowed_hosts(config.allowed_hosts.clone())
            .with_allowed_origins(config.allowed_origins.clone())
            .enforce_origin_validation()
            .with_max_request_body_bytes(limits.max_body_bytes)
            .with_cancellation_token(shutdown.child_token());
        let factory_sessions = sessions.clone();
        let factory_files = files.clone();
        let public_url: Option<Arc<str>> = config.public_url.as_deref().map(Arc::from);
        let factory_public_url = public_url.clone();
        let call_timeout = limits.call_timeout;
        let max_upload_bytes = limits.max_upload_bytes;
        let mcp = StreamableHttpService::new(
            move || {
                Ok(ArtServer::new(
                    factory_sessions.clone(),
                    factory_files.clone(),
                    factory_public_url.clone(),
                    call_timeout,
                    max_upload_bytes,
                ))
            },
            Arc::new(LocalSessionManager::default()),
            mcp_config,
        );

        let allowed_hosts: Arc<[String]> = config.allowed_hosts.clone().into();
        let allowed_origins: Arc<[String]> = config.allowed_origins.clone().into();
        let downloads = Downloads {
            store: files.clone(),
            allowed_hosts: allowed_hosts.clone(),
            allowed_origins: allowed_origins.clone(),
        };
        let uploads = Uploads {
            store: files.clone(),
            allowed_hosts,
            allowed_origins,
            public_url,
            max_bytes: max_upload_bytes,
        };
        let protected = Router::new()
            .nest_service("/mcp", mcp)
            .layer(middleware::from_fn_with_state(Auth::new(&config.token), auth::require_bearer));
        // The ID in the URL is the key to an export or an upload, so no token is required
        // (docs/design-policy.md section 4.6)
        let upload_routes = Router::new()
            .route("/uploads/upload.js", get(uploads::script))
            .route("/uploads/{id}", get(uploads::page).put(uploads::receive))
            .layer(DefaultBodyLimit::max(max_upload_bytes))
            .with_state(uploads);
        let router = Router::new()
            .route("/healthz", get(|| async { Json(json!({"status": "ok"})) }))
            .route("/files/{id}", get(files::download))
            .with_state(downloads)
            .merge(upload_routes)
            .merge(protected);
        Ok(Self {
            router,
            sessions,
            files,
            _sandbox: sandbox,
        })
    }
}

/// Process-wide preparation: limits the engine's rendering threads, adds the given font folders and starts
/// scanning the fonts early (the folders must be added before the first scan).
pub fn prepare_engine(config: &Config) {
    vectorcraft_render::set_default_threads(Some(config.limits.render_threads));
    vectorcraft_text::set_user_font_dirs(config.font_dirs.clone());
    vectorcraft_text::FontDb::global().scan_in_background();
}

/// Listens on `listener` and runs the server until `signal` completes.
pub async fn serve(config: Config, listener: TcpListener, signal: impl Future<Output = ()> + Send + 'static) -> std::io::Result<()> {
    let shutdown = CancellationToken::new();
    let app = App::build(&config, shutdown.clone())?;

    let sessions = app.sessions.clone();
    let files = app.files.clone();
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
                    // Exports and uploads of working sessions no longer in use also go when they expire (10 minutes by default)
                    let expired = files.purge_expired();
                    if expired > 0 {
                        tracing::info!(expired, "dropped expired exports and uploads");
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
