#![forbid(unsafe_code)]
#![recursion_limit = "256"]

use std::{net::SocketAddr, sync::Arc};

pub mod bible;
mod broadlink;
pub mod connectors;
pub mod database;
mod models;
mod obs_devices;
pub mod queue;
pub mod runtime;
pub mod scheduler;
pub mod server;
mod uploader;

#[derive(Debug, Clone)]
pub struct HostAsset {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Clone)]
pub struct ApplicationLog {
    pub path: String,
    pub content: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("host capability is unavailable")]
    Unavailable,
    #[error("host operation failed: {0}")]
    Failed(String),
}

pub trait Host: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value) -> Result<(), HostError>;

    fn asset(&self, _path: &str) -> Result<Option<HostAsset>, HostError> {
        Err(HostError::Unavailable)
    }

    fn caption_logo(&self) -> Result<Option<String>, HostError> {
        Err(HostError::Unavailable)
    }

    fn application_log(&self) -> Result<ApplicationLog, HostError> {
        Err(HostError::Unavailable)
    }

    fn clear_application_log(&self) -> Result<(), HostError> {
        Err(HostError::Unavailable)
    }
}

pub type HostHandle = Arc<dyn Host>;

pub fn emit<T: serde::Serialize>(
    host: &HostHandle,
    event: &str,
    payload: &T,
) -> Result<(), HostError> {
    let payload =
        serde_json::to_value(payload).map_err(|error| HostError::Failed(error.to_string()))?;
    host.emit(event, payload)
}

pub struct ServerHandle {
    pub address: SocketAddr,
    shutdown: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<Result<(), String>>,
}

impl ServerHandle {
    fn new(
        address: SocketAddr,
        shutdown: tokio::sync::watch::Sender<bool>,
        task: tokio::task::JoinHandle<Result<(), String>>,
    ) -> Self {
        Self {
            address,
            shutdown,
            task,
        }
    }

    pub async fn shutdown(self) -> Result<(), HostError> {
        self.shutdown
            .send(true)
            .map_err(|_| HostError::Unavailable)?;
        self.task
            .await
            .map_err(|error| HostError::Failed(error.to_string()))?
            .map_err(HostError::Failed)
    }
}
