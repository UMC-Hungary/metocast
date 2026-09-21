//! Headless Metocast server — the full core stack (embedded PostgreSQL, Axum
//! HTTP/WS, connectors, scheduler, uploads) with no Tauri window or display.
//!
//! Env vars:
//!   METOCAST_AUTH_TOKEN  (required) bearer token clients must present
//!   METOCAST_PORT        listen port (default: 3737)
//!   METOCAST_DATA_DIR    PostgreSQL data directory (default: ./data)
//!   METOCAST_STATIC_DIR  directory of a built UI to serve (dev builds fall back
//!                        to the repo's `build/` directory when it exists)
//!
//! Desktop-only integrations (OAuth login flows, dialogs) are unavailable
//! here — they need a Tauri AppHandle.

use std::sync::Arc;
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

    runtime::start(CoreOptions::new(
        data_dir,
        port,
        Arc::new(RwLock::new(token)),
    ))
    .await
}
