use super::ConnectorStatus;

pub struct VmixConnector;

impl VmixConnector {
    pub fn new() -> Self {
        Self
    }

    pub fn get_status(&self) -> ConnectorStatus {
        ConnectorStatus::Disconnected
    }
}

impl Default for VmixConnector {
    fn default() -> Self {
        Self::new()
    }
}
