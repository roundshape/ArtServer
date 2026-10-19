use std::process::ExitCode;

use artserver::config::Cli;
use clap::Parser;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();

    let config = match Cli::parse().resolve() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("artserver: {e}");
            return ExitCode::FAILURE;
        }
    };
    artserver::prepare_engine(&config);

    let listener = match tokio::net::TcpListener::bind(config.bind).await {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("artserver: cannot listen on {}: {e}", config.bind);
            return ExitCode::FAILURE;
        }
    };
    tracing::info!(
        bind = %config.bind,
        max_sessions = config.limits.max_sessions,
        max_running = config.limits.max_running,
        "artserver listening; MCP endpoint is /mcp"
    );

    match artserver::serve(config, listener, shutdown_signal()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("artserver: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Shuts down on Ctrl+C (and on SIGTERM on Unix).
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = tokio::signal::ctrl_c().await {
            tracing::error!("cannot listen for Ctrl+C: {e}");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(e) => {
                tracing::error!("cannot listen for SIGTERM: {e}");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    tracing::info!("shutting down");
}
