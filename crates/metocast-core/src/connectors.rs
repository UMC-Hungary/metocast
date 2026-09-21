use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ConnectorStatus {
    Disconnected,
    Connecting,
    Connected,
    Error { message: String },
}

pub trait ConnectorConfig {
    fn is_configured(&self) -> bool;
}

macro_rules! host_config {
    ($name:ident, $default_port:literal) => {
        #[derive(Debug, Clone, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct $name {
            pub enabled: bool,
            pub host: String,
            pub port: u16,
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    enabled: false,
                    host: String::new(),
                    port: $default_port,
                }
            }
        }

        impl ConnectorConfig for $name {
            fn is_configured(&self) -> bool {
                self.enabled && !self.host.is_empty()
            }
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObsConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub password: Option<String>,
}

impl Default for ObsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "localhost".to_string(),
            port: 4455,
            password: None,
        }
    }
}

impl ConnectorConfig for ObsConfig {
    fn is_configured(&self) -> bool {
        self.enabled && !self.host.is_empty() && self.port > 0
    }
}

host_config!(VmixConfig, 8088);
host_config!(AtemConfig, 9910);
host_config!(MiddlecontrolConfig, 11584);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct YouTubeConfig {
    pub enabled: bool,
    pub client_id: String,
    pub client_secret: String,
}

impl ConnectorConfig for YouTubeConfig {
    fn is_configured(&self) -> bool {
        self.enabled && !self.client_id.is_empty() && !self.client_secret.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FacebookConfig {
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    pub page_id: String,
}

impl ConnectorConfig for FacebookConfig {
    fn is_configured(&self) -> bool {
        self.enabled && !self.app_id.is_empty() && !self.app_secret.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiscordConfig {
    pub enabled: bool,
    pub webhook_url: String,
}

impl ConnectorConfig for DiscordConfig {
    fn is_configured(&self) -> bool {
        self.enabled && !self.webhook_url.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SzentirasConfig {
    pub enabled: bool,
    pub api_key: String,
}

impl ConnectorConfig for SzentirasConfig {
    fn is_configured(&self) -> bool {
        self.enabled && !self.api_key.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BroadlinkConfig {
    pub enabled: bool,
}

impl ConnectorConfig for BroadlinkConfig {
    fn is_configured(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BlackmagicCameraConfig {
    pub enabled: bool,
    pub host: String,
    pub fingerprint: String,
    pub username: String,
    pub password: String,
}

impl ConnectorConfig for BlackmagicCameraConfig {
    fn is_configured(&self) -> bool {
        self.enabled && !self.host.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum RodecasterAudioSource {
    MainMix,
    FaderSlot { number: usize },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum RodecasterProcessingMode {
    #[default]
    PreFader,
    PreFaderBypass,
    PostFader,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum RodecasterAudioOutputMode {
    #[default]
    MainMix,
    Separate,
    CombinedStereo,
    MultichannelFlac,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RodecasterAudioRecordingConfig {
    pub schema_version: u8,
    pub enabled: bool,
    pub directory: String,
    pub processing_mode: RodecasterProcessingMode,
    pub output_mode: RodecasterAudioOutputMode,
    pub sources: Vec<RodecasterAudioSource>,
}

impl Default for RodecasterAudioRecordingConfig {
    fn default() -> Self {
        Self {
            schema_version: 1,
            enabled: false,
            directory: String::new(),
            processing_mode: RodecasterProcessingMode::PreFader,
            output_mode: RodecasterAudioOutputMode::MainMix,
            sources: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RodecasterConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub notify_on_mute: bool,
    #[serde(default)]
    pub audio_recording: RodecasterAudioRecordingConfig,
}

impl ConnectorConfig for RodecasterConfig {
    fn is_configured(&self) -> bool {
        self.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_wire_format_is_stable() {
        assert_eq!(
            serde_json::to_value(ConnectorStatus::Error {
                message: "offline".to_string(),
            })
            .unwrap(),
            serde_json::json!({"type": "error", "message": "offline"})
        );
    }
}
