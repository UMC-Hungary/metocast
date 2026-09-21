use super::ConnectorStatus;

pub struct DiscordConnector;

impl DiscordConnector {
    pub fn new() -> Self {
        Self
    }

    pub fn get_status(&self) -> ConnectorStatus {
        ConnectorStatus::Disconnected
    }
}

impl Default for DiscordConnector {
    fn default() -> Self {
        Self::new()
    }
}
