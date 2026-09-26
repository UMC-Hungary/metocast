//! Headless Metocast server — the full core stack (embedded PostgreSQL, Axum
//! HTTP/WS, connectors, scheduler, uploads) with no Tauri window or display.
//!
//! Env vars:
//!   METOCAST_AUTH_TOKEN  (required) bearer token clients must present
//!   METOCAST_PORT        listen port (default: 3737)
//!   METOCAST_DATA_DIR    PostgreSQL data directory (default: ./data)
//!   METOCAST_STATIC_DIR  directory of a built UI to serve (dev builds fall back
//!                        to the repo's `build/` directory when it exists)
//!   METOCAST_ADMIN_TOKEN admin token for reading stored secrets over loopback
//!                        (default: a fresh random token every run)
//!   METOCAST_EXIT_ON_STDIN_EOF  set to `1` to shut down when stdin closes, so a
//!                        parent app that launched this process takes it (and
//!                        its PostgreSQL) down with it even if the app crashes
//!
//! SIGINT and SIGTERM stop the server gracefully, including PostgreSQL.
//! Desktop-only integrations (OAuth login flows, dialogs) are unavailable
//! here — they need a Tauri AppHandle.

use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::sync::RwLock;

use metocast_server::runtime::{self, CoreOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let token = std::env::var("METOCAST_AUTH_TOKEN")
        .map_err(|_| anyhow::anyhow!("METOCAST_AUTH_TOKEN env var must be set"))?;

    let port: u16 = match std::env::var("METOCAST_PORT") {
        Ok(p) => p.parse()?,
        Err(_) => 3737,
    };

    let data_dir = std::path::PathBuf::from(
        std::env::var("METOCAST_DATA_DIR").unwrap_or_else(|_| "./data".into()),
    );
    std::fs::create_dir_all(&data_dir)?;

    let exit_on_stdin_eof = std::env::var("METOCAST_EXIT_ON_STDIN_EOF").is_ok_and(|v| v == "1");

    let mut server = runtime::spawn(CoreOptions::new(
        data_dir,
        port,
        Arc::new(RwLock::new(token)),
    ))
    .await?;
    tracing::info!(address = %server.address, "Metocast server ready");

    tokio::select! {
        result = server.finished() => return Ok(result?),
        () = shutdown_requested(exit_on_stdin_eof) => {}
    }
    tracing::info!("Shutting down");
    server.shutdown().await?;
    // A stdin watcher stays blocked in read() until the parent closes the pipe, and the
    // runtime would wait for it. Everything worth stopping has stopped, so exit now.
    std::process::exit(0)
}

/// Resolves on SIGINT/SIGTERM, or when stdin closes if the parent asked for that.
async fn shutdown_requested(exit_on_stdin_eof: bool) {
    let stdin_closed = async {
        if !exit_on_stdin_eof {
            return std::future::pending().await;
        }
        let mut stdin = tokio::io::stdin();
        let mut buffer = [0u8; 64];
        while matches!(stdin.read(&mut buffer).await, Ok(read) if read > 0) {}
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        () = terminate() => {}
        () = stdin_closed => {}
    }
}

#[cfg(unix)]
async fn terminate() {
    match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
        Ok(mut signal) => {
            signal.recv().await;
        }
        Err(_) => std::future::pending().await,
    }
}

#[cfg(not(unix))]
async fn terminate() {
    std::future::pending().await
}
