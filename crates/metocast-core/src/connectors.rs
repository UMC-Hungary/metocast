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

/// Where the camera streams to. Never carries the stream key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraDestination {
    /// `youtube`, `facebook`, `discord`, `middlecontrol` or `other`.
    pub service: String,
    /// Scheme and host only, such as `rtmp://relay.local/…`.
    pub url: String,
    /// Today's event, when it has a broadcast on that service.
    #[serde(default)]
    pub broadcast: Option<String>,
    /// The camera already has the channel's YouTube stream; `None` when that can't be checked.
    #[serde(default)]
    pub matches_youtube: Option<bool>,
}

/// The last preset move Metocast watched run to its end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPreset {
    pub preset: u8,
    pub camera_id: u8,
    /// RFC 3339, when the move finished. A string, so the native bridges can carry the record.
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiddlecontrolState {
    pub selected_camera: Option<u8>,
    pub recording: Option<bool>,
    pub recording_camera_ids: Option<Vec<u8>>,
    pub connected_camera_ids: Option<Vec<u8>>,
    pub connected_apcr_ids: Option<Vec<u8>>,
    /// Whether a preset is moving the selected camera; derived from `running_preset`.
    pub preset_move_active: Option<bool>,
    /// The preset (1–12) moving the selected camera, `None` at rest. Middle Control 4.0.4
    /// reports the preset's number in `PRES_ACTIVE`, not the 1/0 its API page describes.
    #[serde(default)]
    pub running_preset: Option<u8>,
    /// The last move whose start Metocast saw and that ran to its end.
    #[serde(default)]
    pub last_preset: Option<LastPreset>,
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
    #[serde(rename = "blackmagic-camera", rename_all = "camelCase")]
    Camera {
        #[serde(default)]
        is_streaming: bool,
        #[serde(default)]
        is_recording: bool,
        /// The camera's own word, such as `Idle`, `Connecting` or `Streaming`.
        #[serde(default)]
        stream_status: String,
        /// False with no card or SSD in the camera; an older server doesn't say, so true.
        #[serde(default = "crate::automations::default_true")]
        has_media: bool,
        /// Where the camera's livestream points; absent from an older server.
        #[serde(default)]
        destination: Option<CameraDestination>,
    },
    #[serde(rename = "middlecontrol")]
    Middlecontrol { state: Option<MiddlecontrolState> },
    /// Connectors this crate doesn't model yet.
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

/// What the RØDECaster's own recorder (its SD card or USB drive) is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeskRecorderStatus {
    Stopped,
    Paused,
    Recording,
}

/// `rodecaster.record.state`: the desk's own recorder, which is not Metocast's FLAC
/// recording of the desk's USB audio (see [`RodecasterRecorderState`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RodecasterDeskRecorder {
    pub status: DeskRecorderStatus,
    /// An SD card or USB drive is there to write to.
    pub media_present: bool,
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

impl RodecasterConfig {
    /// Fills what a save leaves out of `audioRecording` from the stored settings, so a client
    /// that doesn't show them, like the Mac app's connector form, can't reset them by saving.
    pub fn keep_stored_audio_recording(
        body: &mut serde_json::Value,
        stored: &RodecasterAudioRecordingConfig,
    ) {
        let (Some(body), Ok(serde_json::Value::Object(stored))) =
            (body.as_object_mut(), serde_json::to_value(stored))
        else {
            return;
        };
        let given = body
            .entry("audioRecording")
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let serde_json::Value::Object(given) = given {
            for (key, value) in stored {
                given.entry(key).or_insert(value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_that_omits_the_audio_recording_keeps_the_stored_one() {
        let stored = RodecasterAudioRecordingConfig {
            enabled: true,
            directory: "/Volumes/Audio".to_string(),
            ..RodecasterAudioRecordingConfig::default()
        };
        // The Mac form: no `audioRecording` at all.
        let mut flat = serde_json::json!({"enabled": true, "notifyOnMute": false});
        RodecasterConfig::keep_stored_audio_recording(&mut flat, &stored);
        let config: RodecasterConfig = serde_json::from_value(flat).unwrap();
        assert!(config.audio_recording.enabled);
        assert_eq!(config.audio_recording.directory, "/Volumes/Audio");

        // A partial one: what it names wins, the rest stays.
        let mut partial = serde_json::json!({"audioRecording": {"directory": "/Volumes/New"}});
        RodecasterConfig::keep_stored_audio_recording(&mut partial, &stored);
        let config: RodecasterConfig = serde_json::from_value(partial).unwrap();
        assert!(config.audio_recording.enabled);
        assert_eq!(config.audio_recording.directory, "/Volumes/New");

        // Turning it off is a choice, not an omission.
        let mut off = serde_json::json!({"audioRecording": {"enabled": false}});
        RodecasterConfig::keep_stored_audio_recording(&mut off, &stored);
        let config: RodecasterConfig = serde_json::from_value(off).unwrap();
        assert!(!config.audio_recording.enabled);
    }

    #[test]
    fn middlecontrol_state_without_the_preset_fields_still_decodes() {
        let state: MiddlecontrolState = serde_json::from_str(
            r#"{"selectedCamera":1,"recording":null,"recordingCameraIds":null,
                "connectedCameraIds":[1],"connectedApcrIds":[1],"presetMoveActive":false}"#,
        )
        .unwrap();
        assert_eq!(state.running_preset, None);
        assert_eq!(state.last_preset, None);
    }

    #[test]
    fn preset_progress_wire_format_is_stable() {
        let state = MiddlecontrolState {
            selected_camera: Some(1),
            recording: None,
            recording_camera_ids: None,
            connected_camera_ids: Some(vec![1]),
            connected_apcr_ids: Some(vec![1]),
            preset_move_active: Some(true),
            running_preset: Some(3),
            last_preset: Some(LastPreset {
                preset: 1,
                camera_id: 1,
                at: "2026-10-07T08:02:03Z".to_string(),
            }),
        };
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::json!({
                "selectedCamera": 1, "recording": null, "recordingCameraIds": null,
                "connectedCameraIds": [1], "connectedApcrIds": [1],
                "presetMoveActive": true, "runningPreset": 3,
                "lastPreset": {"preset": 1, "cameraId": 1, "at": "2026-10-07T08:02:03Z"}
            })
        );
    }

    #[test]
    fn desk_recorder_wire_format_is_stable() {
        assert_eq!(
            serde_json::to_value(RodecasterDeskRecorder {
                status: DeskRecorderStatus::Recording,
                media_present: true,
            })
            .unwrap(),
            serde_json::json!({"status": "recording", "mediaPresent": true})
        );
    }

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
