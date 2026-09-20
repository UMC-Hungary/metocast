use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ConnectorStatus {
    Disconnected,
    Connecting,
    Connected,
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtemInput {
    pub id: u16,
    pub name: String,
    pub short_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StreamStatus {
    Idle,
    Connecting,
    Streaming,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecordStatus {
    Idle,
    Recording,
    Stopping,
}

/// What the ATEM reports. Models without a streaming or recording engine leave those `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtemState {
    pub product: String,
    pub program: Option<u16>,
    pub preview: Option<u16>,
    pub inputs: Vec<AtemInput>,
    pub streaming: Option<StreamStatus>,
    pub recording: Option<RecordStatus>,
    pub stream_service: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiddlecontrolState {
    pub selected_camera: Option<u8>,
    pub recording: Option<bool>,
    pub recording_camera_ids: Option<Vec<u8>>,
    pub connected_camera_ids: Option<Vec<u8>>,
    pub connected_apcr_ids: Option<Vec<u8>>,
    pub preset_move_active: Option<bool>,
}

/// `connector.state`: one connector's live state, tagged by `connector`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "connector")]
pub enum ConnectorState {
    #[serde(rename = "obs", rename_all = "camelCase")]
    Obs {
        is_streaming: bool,
        is_recording: bool,
    },
    #[serde(rename = "atem")]
    Atem { state: Option<AtemState> },
    #[serde(rename = "middlecontrol")]
    Middlecontrol { state: Option<MiddlecontrolState> },
    /// Connectors this crate doesn't model yet, such as `blackmagic-camera`.
    #[serde(other)]
    Other,
}

/// One RØDECaster channel from `rodecaster.profile`. The mixer's `mute` is separate from the
/// Wireless PRO transmitter's `wireless_mute`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RodecasterChannel {
    pub channel: u32,
    pub label: String,
    pub mute: bool,
    pub wireless_mute: bool,
}

/// The fields of `rodecaster.profile` a client shows; levels and settings are left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RodecasterProfile {
    pub model: String,
    pub channels: Vec<RodecasterChannel>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecorderStatus {
    Idle,
    Recording,
    Failed,
}

/// The fields of `rodecaster.audio.record.state` a client acts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RodecasterRecorderState {
    pub status: RecorderStatus,
    pub event_id: Option<Uuid>,
    pub started_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
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
