use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    connectors::{ConnectorState, ConnectorStatus, RodecasterProfile, RodecasterRecorderState},
    events::{Event, EventSummary},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum PresenterCommand {
    #[serde(rename = "presenter.status")]
    Status,
    #[serde(rename = "presenter.next")]
    Next,
    #[serde(rename = "presenter.prev")]
    Previous,
    #[serde(rename = "presenter.first")]
    First,
    #[serde(rename = "presenter.last")]
    Last,
    #[serde(rename = "presenter.goto")]
    GoTo { slide: u32 },
    #[serde(rename = "presenter.mute")]
    Mute,
    #[serde(rename = "presenter.unmute")]
    Unmute,
}

/// Live production commands. Each is answered by `ok` or `error`; the resulting state arrives
/// separately (`connector.state`, `rodecaster.mute`, `rodecaster.audio.record.state`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum ProductionCommand {
    #[serde(rename = "obs.stream.start")]
    ObsStreamStart,
    #[serde(rename = "obs.stream.stop")]
    ObsStreamStop,
    #[serde(rename = "obs.record.start")]
    ObsRecordStart,
    #[serde(rename = "obs.record.stop")]
    ObsRecordStop,
    #[serde(rename = "atem.program.set")]
    AtemProgram { input: u16 },
    #[serde(rename = "atem.preview.set")]
    AtemPreview { input: u16 },
    #[serde(rename = "atem.cut")]
    AtemCut,
    #[serde(rename = "atem.auto")]
    AtemAuto,
    #[serde(rename = "atem.stream.start")]
    AtemStreamStart,
    #[serde(rename = "atem.stream.stop")]
    AtemStreamStop,
    #[serde(rename = "atem.record.start")]
    AtemRecordStart,
    #[serde(rename = "atem.record.stop")]
    AtemRecordStop,
    #[serde(rename = "middlecontrol.camera.select")]
    MiddlecontrolCamera { camera_id: u8 },
    #[serde(rename = "middlecontrol.record.start_all")]
    MiddlecontrolRecordStartAll,
    #[serde(rename = "middlecontrol.record.stop_all")]
    MiddlecontrolRecordStopAll,
    /// Answered by `rodecaster.profile`.
    #[serde(rename = "rodecaster.profile")]
    RodecasterProfile,
    #[serde(rename = "rodecaster.mute.set")]
    RodecasterMute { channel: u32, mute: bool },
    #[serde(rename = "rodecaster.audio.record.start")]
    RodecasterRecordStart { event_id: Uuid },
    #[serde(rename = "rodecaster.audio.record.stop")]
    RodecasterRecordStop,
}

/// The visual treatment of text-mode song and Bible slides.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PresenterTheme {
    #[default]
    Classic,
    Editorial,
}

impl PresenterTheme {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Editorial => "editorial",
        }
    }
}

impl std::str::FromStr for PresenterTheme {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "classic" => Ok(Self::Classic),
            "editorial" => Ok(Self::Editorial),
            _ => Err(()),
        }
    }
}

/// Slides and the presentation backend. `presentation.*` routes to the web presenter or to
/// Keynote depending on the server's setting, so one command drives either.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum PresentationCommand {
    #[serde(rename = "presentation.open")]
    Open { file_path: String },
    #[serde(rename = "presentation.close")]
    Close,
    #[serde(rename = "presentation.start")]
    Start,
    #[serde(rename = "presentation.stop")]
    Stop,
    /// Answered by `presentation.settings`.
    #[serde(rename = "presentation.get_settings")]
    GetSettings,
    /// Answered by `presentation.status`.
    #[serde(rename = "presentation.status")]
    Status,
    #[serde(rename = "presentation.set_use_web_presenter")]
    SetUseWebPresenter { enabled: bool },
    #[serde(rename = "presentation.set_presenter_theme")]
    SetPresenterTheme { theme: PresenterTheme },
    #[serde(rename = "presentation.next")]
    Next,
    #[serde(rename = "presentation.prev")]
    Previous,
    #[serde(rename = "presentation.first")]
    First,
    #[serde(rename = "presentation.last")]
    Last,
    #[serde(rename = "presentation.goto")]
    GoTo { slide: u32 },
    #[serde(rename = "presentation.mute")]
    Mute,
    #[serde(rename = "presentation.unmute")]
    Unmute,
    /// Names this client in everyone's connected-devices list.
    #[serde(rename = "presenter.register")]
    Register {
        label: String,
        hostname: Option<String>,
    },
    /// Answered by `clients.list`.
    #[serde(rename = "clients.list")]
    ClientsList,
}

/// `presentation.status`: the active backend, whichever it is.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PresentationStatus {
    pub app_running: bool,
    pub slideshow_active: bool,
    pub current_slide: Option<u32>,
    pub total_slides: Option<u32>,
    pub document_name: Option<String>,
    #[serde(default)]
    pub blanked: bool,
}

/// One WebSocket client in `clients.list` / `clients.updated`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedClient {
    pub id: Uuid,
    pub label: String,
    pub user_agent: Option<String>,
    pub hostname: Option<String>,
    pub connected_at: DateTime<Utc>,
    pub latency_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PresenterRenderMode {
    #[default]
    Text,
    Svg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphContent {
    #[serde(default)]
    pub lines: Vec<String>,
    pub align: String,
    #[serde(default)]
    pub font_size_pt: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideContent {
    pub index: u32,
    pub paragraphs: Vec<ParagraphContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvgSlideContent {
    pub index: u32,
    pub svg: String,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenterState {
    pub loaded: bool,
    pub file_path: Option<String>,
    pub current_slide: u32,
    pub total_slides: u32,
    #[serde(default)]
    pub render_mode: PresenterRenderMode,
    #[serde(default)]
    pub slides: Vec<SlideContent>,
    #[serde(default)]
    pub svg_slides: Vec<SvgSlideContent>,
    pub muted: bool,
    #[serde(default = "default_slide_width")]
    pub slide_width_emu: u64,
    #[serde(default = "default_slide_height")]
    pub slide_height_emu: u64,
}

const fn default_slide_width() -> u64 {
    12_192_000
}

const fn default_slide_height() -> u64 {
    6_858_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "connected")]
    Connected {
        #[serde(rename = "serverId")]
        server_id: String,
    },
    #[serde(rename = "connector.status")]
    ConnectorStatus {
        connector: String,
        status: ConnectorStatus,
    },
    /// The server sends one top-level key per connector next to `type`.
    #[serde(rename = "connectors.status")]
    ConnectorStatuses {
        #[serde(flatten)]
        statuses: HashMap<String, ConnectorStatus>,
    },
    #[serde(rename = "events.list")]
    EventsList { events: Vec<EventSummary> },
    #[serde(rename = "events.get")]
    Event { event: Event },
    #[serde(rename = "presenter.state")]
    PresenterState { state: PresenterState },
    /// Sent instead of `presenter.state` when only the slide moved.
    #[serde(rename = "presenter.slide_changed")]
    PresenterSlideChanged {
        #[serde(rename = "currentSlide")]
        current_slide: u32,
        #[serde(rename = "totalSlides")]
        total_slides: u32,
    },
    /// An event row was inserted, updated or deleted. Its `data` holds the raw row when it
    /// comes from a database trigger, so clients reload the list instead of reading it.
    #[serde(rename = "event.changed")]
    EventChanged {},
    #[serde(rename = "notification")]
    Notification { level: String, message: String },
    #[serde(rename = "connector.state")]
    ConnectorState(ConnectorState),
    #[serde(rename = "rodecaster.profile")]
    RodecasterProfile { profile: RodecasterProfile },
    /// A channel was muted on the desk, or on its Wireless PRO transmitter when `remote`.
    #[serde(rename = "rodecaster.mute")]
    RodecasterMute {
        channel: u32,
        label: String,
        muted: bool,
        remote: bool,
        /// The desk's "notify on mute" setting is on.
        notify: bool,
    },
    #[serde(rename = "rodecaster.audio.record.state")]
    RodecasterRecorder { state: RodecasterRecorderState },
    #[serde(rename = "obs.devices.available")]
    ObsDevices {
        devices: crate::discovery::ObsDevices,
    },
    #[serde(rename = "rodecaster.audio.discovery")]
    AudioDiscovery {
        discovery: crate::discovery::AudioDiscovery,
    },
    /// A command this client sent failed. `message` is a stable code (`atem_not_connected`)
    /// or the device's own words.
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "presentation.settings")]
    PresentationSettings {
        #[serde(rename = "useWebPresenter")]
        use_web_presenter: bool,
        #[serde(rename = "presenterTheme")]
        presenter_theme: PresenterTheme,
    },
    #[serde(rename = "presentation.status")]
    PresentationStatus { status: PresentationStatus },
    /// Reply to `clients.list`.
    #[serde(rename = "clients.list")]
    ClientsList { clients: Vec<ConnectedClient> },
    /// Broadcast whenever a client connects, leaves or is renamed.
    #[serde(rename = "clients.updated")]
    ClientsUpdated { clients: Vec<ConnectedClient> },
    #[serde(rename = "ping")]
    Ping {
        #[serde(rename = "pingId")]
        ping_id: i64,
    },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presenter_command_matches_existing_protocol() {
        assert_eq!(
            serde_json::to_string(&PresenterCommand::GoTo { slide: 3 }).unwrap(),
            r#"{"type":"presenter.goto","slide":3}"#
        );
    }

    #[test]
    fn production_commands_match_the_server_wire_names() {
        let json = |command: ProductionCommand| serde_json::to_value(command).unwrap();
        assert_eq!(
            json(ProductionCommand::ObsStreamStart),
            serde_json::json!({"type": "obs.stream.start"})
        );
        assert_eq!(
            json(ProductionCommand::AtemProgram { input: 3 }),
            serde_json::json!({"type": "atem.program.set", "input": 3})
        );
        assert_eq!(
            json(ProductionCommand::MiddlecontrolCamera { camera_id: 2 }),
            serde_json::json!({"type": "middlecontrol.camera.select", "camera_id": 2})
        );
        assert_eq!(
            json(ProductionCommand::RodecasterMute {
                channel: 1,
                mute: true
            }),
            serde_json::json!({"type": "rodecaster.mute.set", "channel": 1, "mute": true})
        );
        let event_id = Uuid::nil();
        assert_eq!(
            json(ProductionCommand::RodecasterRecordStart { event_id }),
            serde_json::json!({"type": "rodecaster.audio.record.start", "event_id": event_id})
        );
    }

    #[test]
    fn connector_state_decodes_each_connector_shape() {
        // Shapes sent by server/mod.rs and on connect by websocket.rs.
        let decode = |json: &str| match serde_json::from_str(json).unwrap() {
            ServerEvent::ConnectorState(state) => state,
            other => panic!("expected connector.state, got {other:?}"),
        };
        assert_eq!(
            decode(
                r#"{"type":"connector.state","connector":"obs","isStreaming":true,"isRecording":false}"#
            ),
            ConnectorState::Obs {
                is_streaming: true,
                is_recording: false
            }
        );
        let ConnectorState::Atem { state: Some(atem) } = decode(
            r#"{"type":"connector.state","connector":"atem","state":{
                "product":"ATEM Mini Pro","program":1,"preview":2,
                "inputs":[{"id":1,"name":"Camera 1","shortName":"CAM1"}],
                "streaming":"streaming","recording":null,"streamService":"YouTube"}}"#,
        ) else {
            panic!("expected an ATEM state");
        };
        assert_eq!(atem.inputs[0].short_name, "CAM1");
        assert_eq!(
            atem.streaming,
            Some(crate::connectors::StreamStatus::Streaming)
        );
        assert_eq!(atem.recording, None);
        let ConnectorState::Middlecontrol {
            state: Some(middle),
        } = decode(
            r#"{"type":"connector.state","connector":"middlecontrol","state":{
                "selectedCamera":2,"recording":false,"recordingCameraIds":[],
                "connectedCameraIds":[1,2],"connectedApcrIds":null,"presetMoveActive":null}}"#,
        )
        else {
            panic!("expected a Middle Control state");
        };
        assert_eq!(middle.connected_camera_ids, Some(vec![1, 2]));
        assert_eq!(
            decode(
                r#"{"type":"connector.state","connector":"blackmagic-camera","isStreaming":false,"streamStatus":"Idle","isRecording":false}"#
            ),
            ConnectorState::Other
        );
    }

    #[test]
    fn rodecaster_messages_decode_the_server_shapes() {
        let profile = r#"{"type":"rodecaster.profile","profile":{"model":"RØDECaster Pro II",
            "firmware":"1.6.0","channels":[{"channel":0,"label":"Host","source":3,"level":0.5,
            "mute":false,"wirelessMute":true,"cue":false,"processing":["compressor"],"settings":[]}]}}"#;
        let ServerEvent::RodecasterProfile { profile } = serde_json::from_str(profile).unwrap()
        else {
            panic!("expected rodecaster.profile");
        };
        assert_eq!(profile.channels[0].label, "Host");
        assert!(profile.channels[0].wireless_mute);

        let mute = r#"{"type":"rodecaster.mute","channel":2,"label":"Pastor","muted":true,"remote":true,"notify":false}"#;
        assert!(matches!(
            serde_json::from_str(mute).unwrap(),
            ServerEvent::RodecasterMute {
                channel: 2,
                muted: true,
                remote: true,
                ..
            }
        ));

        let recorder = r#"{"type":"rodecaster.audio.record.state","state":{"status":"recording",
            "sessionId":"6f2c1f8e-9a3e-4a8b-9f6e-2d8a1b7c3e10","eventId":"0d9c4b7a-1e2f-4a3b-8c5d-6e7f8a9b0c1d",
            "outputMode":"mainMix","startedAt":"2026-09-20T08:00:00Z","endpoint":{"name":"Main"},
            "error":null,"partialFiles":["a.flac.partial"],"finalizedFiles":[]}}"#;
        let ServerEvent::RodecasterRecorder { state } = serde_json::from_str(recorder).unwrap()
        else {
            panic!("expected rodecaster.audio.record.state");
        };
        assert_eq!(state.status, crate::connectors::RecorderStatus::Recording);
        assert!(state.event_id.is_some() && state.started_at.is_some());

        let error = r#"{"type":"error","message":"atem_not_connected"}"#;
        assert!(matches!(
            serde_json::from_str(error).unwrap(),
            ServerEvent::Error { message } if message == "atem_not_connected"
        ));
    }

    #[test]
    fn presentation_messages_decode_the_server_shapes() {
        // Shapes from websocket.rs `make_presentation_settings` / `make_presentation_status`.
        let settings = r#"{"type":"presentation.settings","useWebPresenter":false,"presenterTheme":"editorial"}"#;
        assert!(matches!(
            serde_json::from_str(settings).unwrap(),
            ServerEvent::PresentationSettings {
                use_web_presenter: false,
                presenter_theme: PresenterTheme::Editorial
            }
        ));

        let status = r#"{"type":"presentation.status","status":{"appRunning":true,
            "slideshowActive":true,"currentSlide":3,"totalSlides":12,
            "documentName":"Song.key","blanked":false}}"#;
        let ServerEvent::PresentationStatus { status } = serde_json::from_str(status).unwrap()
        else {
            panic!("expected presentation.status");
        };
        assert_eq!(status.document_name.as_deref(), Some("Song.key"));
        assert_eq!(
            (status.current_slide, status.total_slides),
            (Some(3), Some(12))
        );

        let clients = r#"{"type":"clients.updated","clients":[{"id":"6f2c1f8e-9a3e-4a8b-9f6e-2d8a1b7c3e10",
            "label":"Sanctum","userAgent":"Mozilla/5.0","hostname":"mac.local",
            "connectedAt":"2026-09-20T08:00:00Z","lastPongAt":null,"latencyMs":12}]}"#;
        let ServerEvent::ClientsUpdated { clients } = serde_json::from_str(clients).unwrap() else {
            panic!("expected clients.updated");
        };
        assert_eq!(
            (clients[0].label.as_str(), clients[0].latency_ms),
            ("Sanctum", Some(12))
        );
    }

    #[test]
    fn presentation_commands_match_the_server_wire_names() {
        assert_eq!(
            serde_json::to_value(PresentationCommand::Open {
                file_path: "/Decks/Song.pptx".to_string()
            })
            .unwrap(),
            serde_json::json!({"type": "presentation.open", "file_path": "/Decks/Song.pptx"})
        );
        assert_eq!(
            serde_json::to_value(PresentationCommand::SetPresenterTheme {
                theme: PresenterTheme::Editorial
            })
            .unwrap(),
            serde_json::json!({"type": "presentation.set_presenter_theme", "theme": "editorial"})
        );
        assert_eq!(
            serde_json::to_value(PresentationCommand::Register {
                label: "iPhone".to_string(),
                hostname: None
            })
            .unwrap(),
            serde_json::json!({"type": "presenter.register", "label": "iPhone", "hostname": null})
        );
    }

    #[test]
    fn connector_statuses_decode_the_flat_server_message() {
        // Shape sent by the server's `connectors.status` reply (websocket.rs).
        let json = r#"{
            "type": "connectors.status",
            "obs": {"type": "connected"},
            "atem": {"type": "error", "message": "unreachable"},
            "blackmagic-camera": {"type": "disconnected"}
        }"#;
        let ServerEvent::ConnectorStatuses { statuses } = serde_json::from_str(json).unwrap()
        else {
            panic!("expected connectors.status");
        };
        assert_eq!(statuses.len(), 3);
        assert_eq!(statuses["obs"], ConnectorStatus::Connected);
        assert_eq!(
            statuses["atem"],
            ConnectorStatus::Error {
                message: "unreachable".to_string()
            }
        );
        assert_eq!(statuses["blackmagic-camera"], ConnectorStatus::Disconnected);
    }

    #[test]
    fn broadcasts_decode_the_server_messages() {
        // Shapes sent by websocket.rs and scheduler/mod.rs.
        let changed =
            r#"{"type":"event.changed","data":{"operation":"UPDATE","record":{"id":"x"}}}"#;
        assert!(matches!(
            serde_json::from_str(changed).unwrap(),
            ServerEvent::EventChanged {}
        ));
        let slide = r#"{"type":"presenter.slide_changed","currentSlide":3,"totalSlides":9}"#;
        assert!(matches!(
            serde_json::from_str(slide).unwrap(),
            ServerEvent::PresenterSlideChanged {
                current_slide: 3,
                total_slides: 9
            }
        ));
        let notification = r#"{"type":"notification","level":"error","message":"Upload failed"}"#;
        let ServerEvent::Notification { level, message } =
            serde_json::from_str(notification).unwrap()
        else {
            panic!("expected notification");
        };
        assert_eq!(
            (level.as_str(), message.as_str()),
            ("error", "Upload failed")
        );
    }
}
