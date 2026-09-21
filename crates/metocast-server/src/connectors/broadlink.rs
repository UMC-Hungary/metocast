use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

use super::ConnectorStatus;

#[derive(Debug, Clone)]
pub struct BroadlinkLearnEvent {
    pub code: Option<String>,
    pub error: Option<String>,
}

pub struct BroadlinkConnector {
    pub status: Arc<RwLock<ConnectorStatus>>,
    pub status_tx: broadcast::Sender<ConnectorStatus>,
    pub learn_tx: broadcast::Sender<BroadlinkLearnEvent>,
}

impl BroadlinkConnector {
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        let (learn_tx, _) = broadcast::channel(16);
        Self {
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            status_tx,
            learn_tx,
        }
    }

    pub async fn get_status(&self) -> ConnectorStatus {
        self.status.read().await.clone()
    }

    pub async fn set_status(&self, status: ConnectorStatus) {
        *self.status.write().await = status.clone();
        let _ = self.status_tx.send(status);
    }
}

impl Default for BroadlinkConnector {
    fn default() -> Self {
        Self::new()
    }
}
