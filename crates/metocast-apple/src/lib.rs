mod discovery;
use discovery::{DiscoveredDeviceRecord, DiscoveryTool};

use std::sync::{Arc, Mutex};

use metocast_client::{
    ClientConfig, ClientError, ConnectionState, MetocastClient, WebSocketConnection,
};
use metocast_core::{
    config::ConfigFieldKind,
    connectors::{
        AtemInput, AtemState, ConnectorState, ConnectorStatus, MiddlecontrolState, RecordStatus,
        RecorderStatus, RodecasterChannel, RodecasterProfile, RodecasterRecorderState,
        StreamStatus,
    },
    events::{
        BibleReference, BibleVerse, CreateBibleReference, CreateConnection, CreateEvent, Event,
        EventConnection, EventSummary,
    },
    operations::{
        BroadlinkCommand, CronJob, CronJobDraft, DeviceListener, QueueJob, QueueSummary,
        UntrackedRecording,
    },
    protocol::{
        ConnectedClient, DeviceAlertCommand, ParagraphContent, PresentationCommand,
        PresentationStatus, PresenterCommand, PresenterState, PresenterTheme, ProductionCommand,
        ServerEvent, SlideContent, SvgSlideContent,
    },
    recordings::Recording,
    slides::{CreateSongSlides, PptFile, SongSlides},
};
use serde::Serialize;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AppleError {
    #[error("invalid client configuration")]
    InvalidConfiguration,
    #[error("authentication was rejected")]
    Authentication,
    #[error("transport failed")]
    Transport,
    /// The server refused and said why, in its own words.
    #[error("{message}")]
    Server { message: String },
    #[error("identifier is invalid")]
    InvalidIdentifier,
    #[error("event connection is not running")]
    NotConnected,
    #[error("client runtime failed")]
    Runtime,
    #[error("input is invalid")]
    InvalidInput,
}

#[derive(Clone, uniffi::Record)]
pub struct EventSummaryRecord {
    pub id: String,
    pub title: String,
    pub computed_title: String,
    pub date_time: String,
    pub speaker: String,
    pub recording_count: i64,
    pub is_completed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, uniffi::Record)]
pub struct EventConnectionRecord {
    pub platform: String,
    pub external_id: Option<String>,
    pub stream_url: Option<String>,
    pub event_url: Option<String>,
    pub schedule_status: String,
    pub privacy_status: Option<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct BibleVerseRecord {
    pub chapter: i32,
    pub verse: i32,
    pub text: String,
}

#[derive(Clone, uniffi::Record)]
pub struct BibleReferenceRecord {
    pub reference_type: String,
    pub reference: String,
    pub translation: String,
    pub verses: Vec<BibleVerseRecord>,
}

#[derive(Clone, uniffi::Record)]
pub struct EventRecord {
    pub id: String,
    pub title: String,
    pub computed_title: String,
    pub date_time: String,
    pub speaker: String,
    pub description: String,
    pub auto_upload_enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub connections: Vec<EventConnectionRecord>,
    pub bible_references: Vec<BibleReferenceRecord>,
}

/// What the event editor saves. An empty `reference` removes that Bible reference.
#[derive(Clone, uniffi::Record)]
pub struct EventDraftRecord {
    pub title: String,
    pub computed_title: String,
    /// RFC 3339.
    pub date_time: String,
    pub speaker: String,
    pub description: String,
    pub auto_upload_enabled: bool,
    pub youtube_privacy: String,
    pub bible_references: Vec<BibleReferenceRecord>,
}

#[derive(Clone, uniffi::Enum)]
pub enum ConnectorStatusCode {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Clone, uniffi::Record)]
pub struct ConnectorStatusRecord {
    pub connector: String,
    pub status: ConnectorStatusCode,
    pub message: Option<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct ParagraphRecord {
    pub lines: Vec<String>,
    pub align: String,
    pub font_size_pt: f32,
}

#[derive(Clone, uniffi::Record)]
pub struct SlideRecord {
    pub index: u32,
    pub paragraphs: Vec<ParagraphRecord>,
}

#[derive(Clone, uniffi::Record)]
pub struct SvgSlideRecord {
    pub index: u32,
    pub svg: String,
    pub width_px: u32,
    pub height_px: u32,
}

#[derive(Clone, uniffi::Record)]
pub struct PresenterStateRecord {
    pub loaded: bool,
    pub file_path: Option<String>,
    pub current_slide: u32,
    pub total_slides: u32,
    pub render_mode: String,
    pub slides: Vec<SlideRecord>,
    pub svg_slides: Vec<SvgSlideRecord>,
    pub muted: bool,
    pub slide_width_emu: u64,
    pub slide_height_emu: u64,
}

#[derive(Clone, uniffi::Enum)]
pub enum PresenterControl {
    Status,
    Next,
    Previous,
    First,
    Last,
    GoTo { slide: u32 },
    Mute,
    Unmute,
}

// Live production state is plain data, so Swift gets the core types as they are.

#[uniffi::remote(Enum)]
pub enum StreamStatus {
    Idle,
    Connecting,
    Streaming,
    Stopping,
}

#[uniffi::remote(Enum)]
pub enum RecordStatus {
    Idle,
    Recording,
    Stopping,
}

#[uniffi::remote(Record)]
pub struct AtemInput {
    pub id: u16,
    pub name: String,
    pub short_name: String,
}

#[uniffi::remote(Record)]
pub struct AtemState {
    pub product: String,
    pub program: Option<u16>,
    pub preview: Option<u16>,
    pub inputs: Vec<AtemInput>,
    pub streaming: Option<StreamStatus>,
    pub recording: Option<RecordStatus>,
    pub stream_service: Option<String>,
}

#[uniffi::remote(Record)]
pub struct MiddlecontrolState {
    pub selected_camera: Option<u8>,
    pub recording: Option<bool>,
    pub recording_camera_ids: Option<Vec<u8>>,
    pub connected_camera_ids: Option<Vec<u8>>,
    pub connected_apcr_ids: Option<Vec<u8>>,
    pub preset_move_active: Option<bool>,
}

#[uniffi::remote(Record)]
pub struct RodecasterChannel {
    pub channel: u32,
    pub label: String,
    pub mute: bool,
    pub wireless_mute: bool,
}

#[uniffi::remote(Record)]
pub struct RodecasterProfile {
    pub model: String,
    pub channels: Vec<RodecasterChannel>,
}

#[uniffi::remote(Enum)]
pub enum RecorderStatus {
    Idle,
    Recording,
    Failed,
}

#[derive(Clone, uniffi::Record)]
pub struct RecorderStateRecord {
    pub status: RecorderStatus,
    pub event_id: Option<String>,
    /// RFC 3339.
    pub started_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, uniffi::Enum)]
pub enum ProductionControl {
    ObsStreaming {
        on: bool,
    },
    ObsRecording {
        on: bool,
    },
    AtemProgram {
        input: u16,
    },
    AtemPreview {
        input: u16,
    },
    AtemCut,
    AtemAuto,
    AtemStreaming {
        on: bool,
    },
    AtemRecording {
        on: bool,
    },
    MiddlecontrolCamera {
        camera_id: u8,
    },
    MiddlecontrolRecordingAll {
        on: bool,
    },
    /// Answered by `AppleEvent::RodecasterProfile`.
    RodecasterProfile,
    RodecasterMute {
        channel: u32,
        mute: bool,
    },
    RodecasterRecordStart {
        event_id: String,
    },
    RodecasterRecordStop,
    CameraRecording {
        on: bool,
    },
    CameraStreaming {
        on: bool,
    },
    /// Copies the YouTube ingestion address and key into the camera, without going live.
    CameraPushYoutube,
    /// The same for the ATEM.
    AtemPushYoutube,
}

/// The OBS sources the server watches. Ids cross the bridge as strings.
#[derive(Clone, uniffi::Enum)]
pub enum DeviceAlertControl {
    List,
    Scan,
    Available,
    Create {
        category: String,
        device_item_value: String,
        device_item_name: String,
        friendly_name: String,
    },
    Delete {
        id: String,
    },
}

#[derive(Clone, uniffi::Record)]
pub struct DeviceListenerRecord {
    pub id: String,
    pub category: String,
    pub device_item_name: String,
    pub friendly_name: String,
}

// Slides and the presentation backend: the core types as they are.

#[uniffi::remote(Record)]
pub struct PptFile {
    pub id: String,
    pub name: String,
    pub path: String,
    pub folder_id: String,
}

#[derive(Clone, uniffi::Record)]
pub struct PptFolderRecord {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[uniffi::remote(Record)]
pub struct SongSlides {
    pub file_path: String,
    pub slide_count: u32,
}

#[uniffi::remote(Enum)]
pub enum PresenterTheme {
    Classic,
    Editorial,
}

#[uniffi::remote(Record)]
pub struct PresentationStatus {
    pub app_running: bool,
    pub slideshow_active: bool,
    pub current_slide: Option<u32>,
    pub total_slides: Option<u32>,
    pub document_name: Option<String>,
    pub blanked: bool,
}

#[uniffi::remote(Enum)]
pub enum PresentationCommand {
    Open {
        file_path: String,
    },
    Close,
    Start,
    Stop,
    GetSettings,
    Status,
    SetUseWebPresenter {
        enabled: bool,
    },
    SetPresenterTheme {
        theme: PresenterTheme,
    },
    Next,
    Previous,
    First,
    Last,
    GoTo {
        slide: u32,
    },
    Mute,
    Unmute,
    Register {
        label: String,
        hostname: Option<String>,
    },
    ClientsList,
}

#[derive(Clone, uniffi::Record)]
pub struct CronJobRecord {
    pub id: String,
    pub name: String,
    pub cron_expression: String,
    pub enabled: bool,
    pub pull_youtube: bool,
    pub auto_upload: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct QueueSummaryRecord {
    pub queue: String,
    pub pending: i64,
    pub processing: i64,
    pub succeeded: i64,
    pub dead: i64,
}

#[derive(Clone, uniffi::Record)]
pub struct QueueJobRecord {
    pub id: String,
    pub queue: String,
    pub job_type: String,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    /// RFC 3339.
    pub updated_at: String,
}

#[derive(Clone, uniffi::Record)]
pub struct BroadlinkCommandRecord {
    pub id: String,
    pub name: String,
    pub category: String,
}

#[derive(Clone, uniffi::Record)]
pub struct UntrackedRecordingRecord {
    pub id: String,
    pub file_name: String,
    pub file_size: i64,
    pub duration_seconds: f64,
    /// RFC 3339.
    pub detected_at: String,
}

#[derive(Clone, uniffi::Record)]
pub struct BibleSuggestionRecord {
    pub label: String,
    pub category: String,
}

/// One recorded file for an event, as the event screen lists it.
#[derive(Clone, uniffi::Record)]
pub struct RecordingRecord {
    pub id: String,
    pub file_name: String,
    pub file_size: i64,
    pub duration_seconds: f64,
    pub source: String,
    /// RFC 3339.
    pub detected_at: String,
    pub uploadable: bool,
    pub uploaded: bool,
    pub video_url: Option<String>,
    /// "youtube: uploading 12%", one per platform that has started.
    pub upload_states: Vec<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct ServerSettingsRecord {
    pub title_template: String,
    /// Where `Generate Slides` writes an event's Bible decks.
    pub slide_folder: String,
    pub song_slide_folder: String,
}

#[derive(Clone, uniffi::Record)]
pub struct ConnectorNameRecord {
    pub id: String,
    pub name: String,
}

#[uniffi::remote(Enum)]
pub enum ConfigFieldKind {
    Toggle,
    Text,
    Number,
    Secret,
}

#[derive(Clone, uniffi::Record)]
pub struct ConfigFieldRecord {
    pub key: String,
    pub label: String,
    pub kind: ConfigFieldKind,
    pub value: String,
    /// A secret is stored, though its value never leaves the server.
    pub is_set: bool,
}

#[derive(Clone, uniffi::Record)]
pub struct ConfigValueRecord {
    pub key: String,
    pub value: String,
    /// Forget the stored secret instead of keeping it.
    pub clear: bool,
}

/// One device on the server's WebSocket, as the connected-devices list shows it.
#[derive(Clone, uniffi::Record)]
pub struct ConnectedClientRecord {
    pub id: String,
    pub label: String,
    pub hostname: Option<String>,
    pub user_agent: Option<String>,
    /// RFC 3339.
    pub connected_at: String,
    pub latency_ms: Option<i64>,
}

#[derive(Clone, uniffi::Enum)]
pub enum AppleConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    AuthenticationFailed,
    Closed,
}

#[derive(Clone, uniffi::Enum)]
pub enum AppleEvent {
    Discovery {
        tool: DiscoveryTool,
        devices: Vec<DiscoveredDeviceRecord>,
        message: Option<String>,
    },
    Connected {
        server_id: String,
    },
    ConnectorStatus {
        status: ConnectorStatusRecord,
    },
    EventsList {
        events: Vec<EventSummaryRecord>,
    },
    Event {
        event: EventRecord,
    },
    PresenterState {
        state: PresenterStateRecord,
    },
    ConnectorStatuses {
        statuses: Vec<ConnectorStatusRecord>,
    },
    PresenterSlideChanged {
        current_slide: u32,
        total_slides: u32,
    },
    EventChanged,
    Notification {
        level: String,
        message: String,
    },
    ObsState {
        streaming: bool,
        recording: bool,
    },
    AtemState {
        state: Option<AtemState>,
    },
    MiddlecontrolState {
        state: Option<MiddlecontrolState>,
    },
    RodecasterProfile {
        profile: RodecasterProfile,
    },
    /// A desk mute, or a Wireless PRO transmitter mute when `remote`.
    RodecasterMute {
        channel: u32,
        label: String,
        muted: bool,
        remote: bool,
        notify: bool,
    },
    RodecasterRecorder {
        state: RecorderStateRecord,
    },
    /// A command this client sent failed: a stable code such as `atem_not_connected`, or the
    /// device's own words.
    CommandFailed {
        message: String,
    },
    PresentationSettings {
        use_web_presenter: bool,
        theme: PresenterTheme,
    },
    PresentationStatus {
        status: PresentationStatus,
    },
    Clients {
        clients: Vec<ConnectedClientRecord>,
    },
    CameraState {
        streaming: bool,
        recording: bool,
        /// The camera's own word for what streaming is doing.
        stream_status: String,
    },
    DeviceAlerts {
        listeners: Vec<DeviceListenerRecord>,
    },
    Unknown,
}

#[uniffi::export(foreign)]
pub trait EventListener: Send + Sync {
    fn on_state_changed(&self, state: AppleConnectionState);
    fn on_event(&self, event: AppleEvent);
}

#[derive(uniffi::Object)]
pub struct AppleClient {
    runtime: Arc<tokio::runtime::Runtime>,
    client: MetocastClient,
    connection: Arc<Mutex<Option<WebSocketConnection>>>,
    event_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[uniffi::export]
impl AppleClient {
    #[uniffi::constructor]
    pub fn new(base_url: String, auth_token: String) -> Result<Arc<Self>, AppleError> {
        let config = ClientConfig::new(&base_url, auth_token)
            .map_err(|_| AppleError::InvalidConfiguration)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|_| AppleError::Runtime)?;
        Ok(Arc::new(Self {
            runtime: Arc::new(runtime),
            client: MetocastClient::new(config),
            connection: Arc::new(Mutex::new(None)),
            event_task: Mutex::new(None),
        }))
    }

    pub async fn health(&self) -> Result<(), AppleError> {
        let client = self.client.clone();
        self.run(async move { client.health().await }).await
    }

    pub async fn list_events(&self) -> Result<Vec<EventSummaryRecord>, AppleError> {
        let client = self.client.clone();
        let events = self.run(async move { client.list_events().await }).await?;
        Ok(events.into_iter().map(Into::into).collect())
    }

    pub async fn event(&self, id: String) -> Result<EventRecord, AppleError> {
        let id = parse_id(&id)?;
        let client = self.client.clone();
        Ok(self
            .run(async move { client.event(id).await })
            .await?
            .into())
    }

    pub async fn connector_statuses(&self) -> Result<Vec<ConnectorStatusRecord>, AppleError> {
        let client = self.client.clone();
        let statuses = self
            .run(async move { client.connector_statuses().await })
            .await?;
        Ok(statuses
            .into_iter()
            .map(|(connector, status)| connector_status(connector, status))
            .collect())
    }

    pub async fn create_event(&self, draft: EventDraftRecord) -> Result<EventRecord, AppleError> {
        let event = CreateEvent::try_from(draft)?;
        let client = self.client.clone();
        Ok(self
            .run(async move { client.create_event(&event).await })
            .await?
            .into())
    }

    pub async fn update_event(
        &self,
        id: String,
        draft: EventDraftRecord,
    ) -> Result<EventRecord, AppleError> {
        let id = parse_id(&id)?;
        let event = CreateEvent::try_from(draft)?;
        let client = self.client.clone();
        Ok(self
            .run(async move { client.update_event(id, &event).await })
            .await?
            .into())
    }

    pub async fn delete_event(&self, id: String) -> Result<(), AppleError> {
        let id = parse_id(&id)?;
        let client = self.client.clone();
        self.run(async move { client.delete_event(id).await }).await
    }

    /// The template the published event title is rendered from.
    pub async fn title_template(&self) -> Result<String, AppleError> {
        let client = self.client.clone();
        Ok(self
            .run(async move { client.title_template().await })
            .await?
            .template)
    }

    pub async fn bible_verses(
        &self,
        reference: String,
        translation: String,
    ) -> Result<Vec<BibleVerseRecord>, AppleError> {
        let client = self.client.clone();
        let passage = self
            .run(async move { client.bible_passage(&reference, &translation).await })
            .await?;
        Ok(passage.verses.into_iter().map(Into::into).collect())
    }

    pub async fn start_events(&self, listener: Arc<dyn EventListener>) -> Result<(), AppleError> {
        self.shutdown_events();
        let client = self.client.clone();
        let stored_connection = Arc::clone(&self.connection);
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let task = self.runtime.spawn(async move {
            let connection = client.connect_websocket();
            let mut events = connection.subscribe();
            let mut state = connection.state();
            if let Ok(mut slot) = stored_connection.lock() {
                *slot = Some(connection.clone());
            }
            let _ = ready_tx.send(());
            listener.on_state_changed(connection_state(*state.borrow()));
            loop {
                tokio::select! {
                    result = state.changed() => {
                        if result.is_err() { break; }
                        listener.on_state_changed(connection_state(*state.borrow()));
                        if *state.borrow() == ConnectionState::Closed { break; }
                    }
                    event = events.recv() => match event {
                        Ok(event) => listener.on_event(event.into()),
                        Err(broadcast_error) => {
                            if matches!(broadcast_error, tokio::sync::broadcast::error::RecvError::Closed) {
                                break;
                            }
                        }
                    }
                }
            }
        });
        *self.event_task.lock().map_err(|_| AppleError::Runtime)? = Some(task);
        ready_rx.await.map_err(|_| AppleError::Runtime)
    }

    pub async fn presenter_control(&self, control: PresenterControl) -> Result<(), AppleError> {
        self.send(PresenterCommand::from(control)).await
    }

    pub async fn production_control(&self, control: ProductionControl) -> Result<(), AppleError> {
        self.send(ProductionCommand::try_from(control)?).await
    }

    /// Manages the OBS sources the server watches and warns about.
    pub async fn device_alert_control(
        &self,
        command: DeviceAlertControl,
    ) -> Result<(), AppleError> {
        self.send(DeviceAlertCommand::try_from(command)?).await
    }

    pub async fn presentation_control(
        &self,
        command: PresentationCommand,
    ) -> Result<(), AppleError> {
        self.send(command).await
    }

    // ── Housekeeping ─────────────────────────────────────────────────────────

    pub async fn cron_jobs(&self) -> Result<Vec<CronJobRecord>, AppleError> {
        let client = self.client.clone();
        let jobs = self.run(async move { client.cron_jobs().await }).await?;
        Ok(jobs.into_iter().map(Into::into).collect())
    }

    /// Creates the job when `id` is empty, otherwise updates it.
    pub async fn save_cron_job(
        &self,
        id: String,
        job: CronJobRecord,
    ) -> Result<CronJobRecord, AppleError> {
        let draft = CronJobDraft {
            name: job.name,
            cron_expression: job.cron_expression,
            enabled: job.enabled,
            pull_youtube: job.pull_youtube,
            auto_upload: job.auto_upload,
        };
        if draft.name.trim().is_empty() || draft.cron_expression.trim().is_empty() {
            return Err(AppleError::InvalidInput);
        }
        let client = self.client.clone();
        let saved = if id.is_empty() {
            self.run(async move { client.create_cron_job(&draft).await })
                .await?
        } else {
            let id = parse_id(&id)?;
            self.run(async move { client.update_cron_job(id, &draft).await })
                .await?
        };
        Ok(saved.into())
    }

    pub async fn delete_cron_job(&self, id: String) -> Result<(), AppleError> {
        let id = parse_id(&id)?;
        let client = self.client.clone();
        self.run(async move { client.delete_cron_job(id).await })
            .await
    }

    pub async fn queues(&self) -> Result<Vec<QueueSummaryRecord>, AppleError> {
        let client = self.client.clone();
        let queues = self.run(async move { client.queues().await }).await?;
        Ok(queues.into_iter().map(Into::into).collect())
    }

    pub async fn queue_jobs(&self, queue: String) -> Result<Vec<QueueJobRecord>, AppleError> {
        let client = self.client.clone();
        let jobs = self
            .run(async move { client.queue_jobs(&queue).await })
            .await?;
        Ok(jobs.into_iter().map(Into::into).collect())
    }

    pub async fn retry_job(&self, id: String) -> Result<(), AppleError> {
        let id = parse_id(&id)?;
        let client = self.client.clone();
        self.run(async move { client.retry_job(id).await }).await
    }

    pub async fn purge_job(&self, id: String) -> Result<(), AppleError> {
        let id = parse_id(&id)?;
        let client = self.client.clone();
        self.run(async move { client.purge_job(id).await }).await
    }

    /// Runs the upload cycle now.
    pub async fn trigger_uploads(&self) -> Result<(), AppleError> {
        let client = self.client.clone();
        self.run(async move { client.trigger_uploads().await })
            .await
    }

    pub async fn broadlink_commands(&self) -> Result<Vec<BroadlinkCommandRecord>, AppleError> {
        let client = self.client.clone();
        let commands = self
            .run(async move { client.broadlink_commands().await })
            .await?;
        Ok(commands.into_iter().map(Into::into).collect())
    }

    pub async fn send_broadlink_command(&self, id: String) -> Result<(), AppleError> {
        let id = parse_id(&id)?;
        let client = self.client.clone();
        self.run(async move { client.send_broadlink_command(id).await })
            .await
    }

    pub async fn untracked_recordings(&self) -> Result<Vec<UntrackedRecordingRecord>, AppleError> {
        let client = self.client.clone();
        let recordings = self
            .run(async move { client.untracked_recordings().await })
            .await?;
        Ok(recordings.into_iter().map(Into::into).collect())
    }

    /// Moves a stray recording onto an event.
    pub async fn assign_untracked(&self, id: String, event_id: String) -> Result<(), AppleError> {
        let id = parse_id(&id)?;
        let event = parse_id(&event_id)?;
        let client = self.client.clone();
        self.run(async move { client.assign_untracked(id, event).await })
            .await
    }

    pub async fn delete_untracked(&self, id: String) -> Result<(), AppleError> {
        let id = parse_id(&id)?;
        let client = self.client.clone();
        self.run(async move { client.delete_untracked(id).await })
            .await
    }

    /// Book and chapter autocomplete for a Bible reference.
    pub async fn bible_suggestions(
        &self,
        term: String,
    ) -> Result<Vec<BibleSuggestionRecord>, AppleError> {
        let client = self.client.clone();
        let suggestions = self
            .run(async move { client.bible_suggestions(&term).await })
            .await?;
        Ok(suggestions
            .into_iter()
            .map(|suggestion| BibleSuggestionRecord {
                label: suggestion.label,
                category: suggestion.cat,
            })
            .collect())
    }

    /// The core's own log. Only a core inside the desktop app has one.
    pub async fn application_log(&self) -> Result<String, AppleError> {
        let client = self.client.clone();
        Ok(self
            .run(async move { client.application_log().await })
            .await?
            .content)
    }

    /// The files recorded for one event, newest first.
    pub async fn recordings(&self, event_id: String) -> Result<Vec<RecordingRecord>, AppleError> {
        let id = parse_id(&event_id)?;
        let client = self.client.clone();
        let recordings = self.run(async move { client.recordings(id).await }).await?;
        Ok(recordings.into_iter().map(Into::into).collect())
    }

    /// Marks a recording to be uploaded to YouTube.
    pub async fn flag_upload(
        &self,
        event_id: String,
        recording_id: String,
        platforms: Vec<String>,
    ) -> Result<(), AppleError> {
        let event = parse_id(&event_id)?;
        let recording = parse_id(&recording_id)?;
        let client = self.client.clone();
        self.run(async move { client.flag_upload(event, recording, platforms).await })
            .await
    }

    /// The server's slide folders and title template.
    pub async fn server_settings(&self) -> Result<ServerSettingsRecord, AppleError> {
        let client = self.client.clone();
        let settings = self
            .run(async move { client.server_settings().await })
            .await?;
        Ok(ServerSettingsRecord {
            title_template: settings.title_template,
            slide_folder: settings.slide_folder,
            song_slide_folder: settings.song_slide_folder,
        })
    }

    /// Saves them. The server refuses a folder that does not exist.
    pub async fn save_server_settings(
        &self,
        settings: ServerSettingsRecord,
    ) -> Result<(), AppleError> {
        let settings = metocast_client::ServerSettings {
            title_template: settings.title_template,
            slide_folder: settings.slide_folder,
            song_slide_folder: settings.song_slide_folder,
        };
        let client = self.client.clone();
        self.run(async move { client.save_server_settings(&settings).await })
            .await
    }

    /// Every connector this app can configure, in display order.
    pub fn connectors(&self) -> Vec<ConnectorNameRecord> {
        metocast_core::config::CONNECTORS
            .iter()
            .map(|(id, name)| ConnectorNameRecord {
                id: (*id).to_string(),
                name: (*name).to_string(),
            })
            .collect()
    }

    /// One connector's settings, as fields to show. Stored secrets come back blank but flagged.
    pub async fn connector_form(
        &self,
        connector: String,
    ) -> Result<Vec<ConfigFieldRecord>, AppleError> {
        let client = self.client.clone();
        let name = connector.clone();
        let config = self
            .run(async move { client.connector_config(&name).await })
            .await?;
        Ok(metocast_core::config::form(&connector, &config)
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// The stored secrets for one connector, which only the Mac hosting the server can read.
    /// Returns the same fields as `connector_form`, with the secrets filled in.
    pub async fn connector_secrets(
        &self,
        connector: String,
        admin_token: String,
    ) -> Result<Vec<ConfigFieldRecord>, AppleError> {
        let client = self.client.clone();
        let name = connector.clone();
        let secrets = self
            .run(async move { client.connector_secrets(&name, &admin_token).await })
            .await?;
        Ok(metocast_core::config::schema(&connector)
            .iter()
            .filter(|(_, _, kind)| matches!(kind, metocast_core::config::ConfigFieldKind::Secret))
            .map(|(key, label, kind)| ConfigFieldRecord {
                key: (*key).to_string(),
                label: (*label).to_string(),
                kind: *kind,
                value: secrets
                    .get(*key)
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                is_set: true,
            })
            .collect())
    }

    /// Saves a connector's settings and applies them on the server.
    pub async fn save_connector_form(
        &self,
        connector: String,
        values: Vec<ConfigValueRecord>,
    ) -> Result<(), AppleError> {
        let body = metocast_core::config::body(
            &connector,
            &values.into_iter().map(Into::into).collect::<Vec<_>>(),
        );
        let client = self.client.clone();
        self.run(async move { client.save_connector_config(&connector, &body).await })
            .await
    }

    /// Where to send the operator to sign in to YouTube (`youtube`) or Facebook (`facebook`).
    pub async fn auth_url(&self, platform: String) -> Result<String, AppleError> {
        let client = self.client.clone();
        self.run(async move { client.auth_url(&platform).await })
            .await
    }

    pub async fn sign_out(&self, platform: String) -> Result<(), AppleError> {
        let client = self.client.clone();
        self.run(async move { client.sign_out(&platform).await })
            .await
    }

    /// PowerPoint files in the server's watched folders whose name contains `filter`.
    pub async fn ppt_files(&self, filter: String) -> Result<Vec<PptFile>, AppleError> {
        let client = self.client.clone();
        self.run(async move { client.ppt_files(&filter).await })
            .await
    }

    pub async fn ppt_folders(&self) -> Result<Vec<PptFolderRecord>, AppleError> {
        let client = self.client.clone();
        let folders = self.run(async move { client.ppt_folders().await }).await?;
        Ok(folders
            .into_iter()
            .map(|folder| PptFolderRecord {
                id: folder.id.to_string(),
                name: folder.name,
                path: folder.path,
            })
            .collect())
    }

    /// Writes a deck per Bible reference on the event; returns the files written.
    pub async fn create_event_slides(&self, id: String) -> Result<Vec<String>, AppleError> {
        let id = parse_id(&id)?;
        let client = self.client.clone();
        Ok(self
            .run(async move { client.create_event_slides(id).await })
            .await?
            .files)
    }

    pub async fn create_song_slides(
        &self,
        title: String,
        lyrics: String,
    ) -> Result<SongSlides, AppleError> {
        let song = CreateSongSlides { title, lyrics };
        let client = self.client.clone();
        self.run(async move { client.create_song_slides(&song).await })
            .await
    }

    pub fn shutdown_events(&self) {
        if let Ok(mut connection) = self.connection.lock()
            && let Some(connection) = connection.take()
        {
            connection.close();
        }
        if let Ok(mut task) = self.event_task.lock()
            && let Some(task) = task.take()
        {
            task.abort();
        }
    }
}

impl AppleClient {
    /// Queues one command on the event connection.
    async fn send(
        &self,
        command: impl Serialize + Send + Sync + 'static,
    ) -> Result<(), AppleError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppleError::Runtime)?
            .clone()
            .ok_or(AppleError::NotConnected)?;
        self.runtime
            .spawn(async move { connection.send(&command).await })
            .await
            .map_err(|_| AppleError::Runtime)?
            .map_err(|_| AppleError::NotConnected)
    }

    /// Runs one request on this client's runtime.
    async fn run<T: Send + 'static>(
        &self,
        request: impl Future<Output = Result<T, ClientError>> + Send + 'static,
    ) -> Result<T, AppleError> {
        self.runtime
            .spawn(request)
            .await
            .map_err(|_| AppleError::Runtime)?
            .map_err(map_client_error)
    }
}

fn parse_id(id: &str) -> Result<uuid::Uuid, AppleError> {
    uuid::Uuid::parse_str(id).map_err(|_| AppleError::InvalidIdentifier)
}

fn map_client_error(error: ClientError) -> AppleError {
    match error {
        ClientError::Authentication => AppleError::Authentication,
        ClientError::Config(_) => AppleError::InvalidConfiguration,
        ClientError::Http {
            message: Some(message),
            ..
        } => AppleError::Server { message },
        ClientError::Http { .. } | ClientError::Transport(_) | ClientError::Decode(_) => {
            AppleError::Transport
        }
    }
}

fn connector_status(connector: String, status: ConnectorStatus) -> ConnectorStatusRecord {
    let (status, message) = match status {
        ConnectorStatus::Disconnected => (ConnectorStatusCode::Disconnected, None),
        ConnectorStatus::Connecting => (ConnectorStatusCode::Connecting, None),
        ConnectorStatus::Connected => (ConnectorStatusCode::Connected, None),
        ConnectorStatus::Error { message } => (ConnectorStatusCode::Error, Some(message)),
    };
    ConnectorStatusRecord {
        connector,
        status,
        message,
    }
}

fn connection_state(state: ConnectionState) -> AppleConnectionState {
    match state {
        ConnectionState::Connecting => AppleConnectionState::Connecting,
        ConnectionState::Connected => AppleConnectionState::Connected,
        ConnectionState::Reconnecting => AppleConnectionState::Reconnecting,
        ConnectionState::AuthenticationFailed => AppleConnectionState::AuthenticationFailed,
        ConnectionState::Closed => AppleConnectionState::Closed,
    }
}

impl From<EventSummary> for EventSummaryRecord {
    fn from(event: EventSummary) -> Self {
        Self {
            id: event.id.to_string(),
            title: event.title,
            computed_title: event.computed_title,
            date_time: event.date_time.to_rfc3339(),
            speaker: event.speaker,
            recording_count: event.recording_count,
            is_completed: event.is_completed,
            created_at: event.created_at.to_rfc3339(),
            updated_at: event.updated_at.to_rfc3339(),
        }
    }
}

impl From<EventConnection> for EventConnectionRecord {
    fn from(connection: EventConnection) -> Self {
        Self {
            platform: connection.platform,
            external_id: connection.external_id,
            stream_url: connection.stream_url,
            event_url: connection.event_url,
            schedule_status: connection.schedule_status,
            privacy_status: connection.privacy_status,
        }
    }
}

impl From<BibleVerse> for BibleVerseRecord {
    fn from(verse: BibleVerse) -> Self {
        Self {
            chapter: verse.chapter,
            verse: verse.verse,
            text: verse.text,
        }
    }
}

impl From<BibleReference> for BibleReferenceRecord {
    fn from(reference: BibleReference) -> Self {
        let verses = serde_json::from_value::<Vec<BibleVerse>>(reference.verses)
            .unwrap_or_default()
            .into_iter()
            .map(Into::into)
            .collect();
        Self {
            reference_type: reference.r#type,
            reference: reference.reference,
            translation: reference.translation,
            verses,
        }
    }
}

impl From<Event> for EventRecord {
    fn from(event: Event) -> Self {
        Self {
            id: event.id.to_string(),
            title: event.title,
            computed_title: event.computed_title,
            date_time: event.date_time.to_rfc3339(),
            speaker: event.speaker,
            description: event.description,
            auto_upload_enabled: event.auto_upload_enabled,
            created_at: event.created_at.to_rfc3339(),
            updated_at: event.updated_at.to_rfc3339(),
            connections: event.connections.into_iter().map(Into::into).collect(),
            bible_references: event.bible_references.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<EventDraftRecord> for CreateEvent {
    type Error = AppleError;

    fn try_from(draft: EventDraftRecord) -> Result<Self, AppleError> {
        let date_time = chrono::DateTime::parse_from_rfc3339(&draft.date_time)
            .map_err(|_| AppleError::InvalidInput)?
            .to_utc();
        Ok(Self {
            title: draft.title,
            computed_title: Some(draft.computed_title),
            date_time,
            speaker: Some(draft.speaker),
            description: Some(draft.description),
            auto_upload_enabled: Some(draft.auto_upload_enabled),
            // Other platforms keep their stored privacy; the server creates them on insert.
            connections: Some(vec![CreateConnection {
                platform: "youtube".to_string(),
                privacy_status: Some(draft.youtube_privacy),
            }]),
            bible_references: Some(
                draft
                    .bible_references
                    .into_iter()
                    .map(|reference| CreateBibleReference {
                        r#type: reference.reference_type,
                        reference: Some(reference.reference),
                        translation: Some(reference.translation),
                        verses: Some(serde_json::json!(
                            reference
                                .verses
                                .into_iter()
                                .map(|verse| BibleVerse {
                                    chapter: verse.chapter,
                                    verse: verse.verse,
                                    text: verse.text,
                                })
                                .collect::<Vec<_>>()
                        )),
                    })
                    .collect(),
            ),
        })
    }
}

impl From<PresenterControl> for PresenterCommand {
    fn from(control: PresenterControl) -> Self {
        match control {
            PresenterControl::Status => Self::Status,
            PresenterControl::Next => Self::Next,
            PresenterControl::Previous => Self::Previous,
            PresenterControl::First => Self::First,
            PresenterControl::Last => Self::Last,
            PresenterControl::GoTo { slide } => Self::GoTo { slide },
            PresenterControl::Mute => Self::Mute,
            PresenterControl::Unmute => Self::Unmute,
        }
    }
}

impl TryFrom<ProductionControl> for ProductionCommand {
    type Error = AppleError;

    fn try_from(control: ProductionControl) -> Result<Self, AppleError> {
        Ok(match control {
            ProductionControl::ObsStreaming { on: true } => Self::ObsStreamStart,
            ProductionControl::ObsStreaming { on: false } => Self::ObsStreamStop,
            ProductionControl::ObsRecording { on: true } => Self::ObsRecordStart,
            ProductionControl::ObsRecording { on: false } => Self::ObsRecordStop,
            ProductionControl::AtemProgram { input } => Self::AtemProgram { input },
            ProductionControl::AtemPreview { input } => Self::AtemPreview { input },
            ProductionControl::AtemCut => Self::AtemCut,
            ProductionControl::AtemAuto => Self::AtemAuto,
            ProductionControl::AtemStreaming { on: true } => Self::AtemStreamStart,
            ProductionControl::AtemStreaming { on: false } => Self::AtemStreamStop,
            ProductionControl::AtemRecording { on: true } => Self::AtemRecordStart,
            ProductionControl::AtemRecording { on: false } => Self::AtemRecordStop,
            ProductionControl::MiddlecontrolCamera { camera_id } => {
                Self::MiddlecontrolCamera { camera_id }
            }
            ProductionControl::MiddlecontrolRecordingAll { on: true } => {
                Self::MiddlecontrolRecordStartAll
            }
            ProductionControl::MiddlecontrolRecordingAll { on: false } => {
                Self::MiddlecontrolRecordStopAll
            }
            ProductionControl::RodecasterProfile => Self::RodecasterProfile,
            ProductionControl::RodecasterMute { channel, mute } => {
                Self::RodecasterMute { channel, mute }
            }
            ProductionControl::RodecasterRecordStart { event_id } => Self::RodecasterRecordStart {
                event_id: parse_id(&event_id)?,
            },
            ProductionControl::RodecasterRecordStop => Self::RodecasterRecordStop,
            ProductionControl::CameraRecording { on: true } => Self::CameraRecordStart,
            ProductionControl::CameraRecording { on: false } => Self::CameraRecordStop,
            ProductionControl::CameraStreaming { on: true } => Self::CameraStreamStart,
            ProductionControl::CameraStreaming { on: false } => Self::CameraStreamStop,
            ProductionControl::CameraPushYoutube => Self::CameraPushYoutube,
            ProductionControl::AtemPushYoutube => Self::AtemPushYoutube,
        })
    }
}

impl TryFrom<DeviceAlertControl> for DeviceAlertCommand {
    type Error = AppleError;

    fn try_from(command: DeviceAlertControl) -> Result<Self, AppleError> {
        Ok(match command {
            DeviceAlertControl::List => Self::List,
            DeviceAlertControl::Scan => Self::Scan,
            DeviceAlertControl::Available => Self::Available,
            DeviceAlertControl::Create {
                category,
                device_item_value,
                device_item_name,
                friendly_name,
            } => Self::Create {
                // Only OBS reports devices to watch today.
                connector_type: "obs".to_string(),
                category,
                device_item_value,
                device_item_name,
                friendly_name,
            },
            DeviceAlertControl::Delete { id } => Self::Delete { id: parse_id(&id)? },
        })
    }
}

impl From<DeviceListener> for DeviceListenerRecord {
    fn from(listener: DeviceListener) -> Self {
        Self {
            id: listener.id.to_string(),
            category: listener.category,
            device_item_name: listener.device_item_name,
            friendly_name: listener.friendly_name,
        }
    }
}

impl From<CronJob> for CronJobRecord {
    fn from(job: CronJob) -> Self {
        Self {
            id: job.id.to_string(),
            name: job.name,
            cron_expression: job.cron_expression,
            enabled: job.enabled,
            pull_youtube: job.pull_youtube,
            auto_upload: job.auto_upload,
        }
    }
}

impl From<QueueSummary> for QueueSummaryRecord {
    fn from(queue: QueueSummary) -> Self {
        Self {
            queue: queue.queue,
            pending: queue.pending,
            processing: queue.processing,
            succeeded: queue.succeeded,
            dead: queue.dead,
        }
    }
}

impl From<QueueJob> for QueueJobRecord {
    fn from(job: QueueJob) -> Self {
        Self {
            id: job.id.to_string(),
            queue: job.queue,
            job_type: job.job_type,
            status: job.status,
            attempts: job.attempts,
            max_attempts: job.max_attempts,
            last_error: job.last_error,
            updated_at: job.updated_at.to_rfc3339(),
        }
    }
}

impl From<BroadlinkCommand> for BroadlinkCommandRecord {
    fn from(command: BroadlinkCommand) -> Self {
        Self {
            id: command.id.to_string(),
            name: command.name,
            category: command.category,
        }
    }
}

impl From<UntrackedRecording> for UntrackedRecordingRecord {
    fn from(recording: UntrackedRecording) -> Self {
        Self {
            id: recording.id.to_string(),
            file_name: recording.file_name,
            file_size: recording.file_size,
            duration_seconds: recording.duration_seconds,
            detected_at: recording.detected_at.to_rfc3339(),
        }
    }
}

impl From<Recording> for RecordingRecord {
    fn from(recording: Recording) -> Self {
        Self {
            id: recording.id.to_string(),
            file_name: recording.file_name,
            file_size: recording.file_size,
            duration_seconds: recording.duration_seconds,
            source: recording.source,
            detected_at: recording.detected_at.to_rfc3339(),
            uploadable: recording.uploadable,
            uploaded: recording.uploaded,
            video_url: recording.video_url,
            upload_states: recording
                .uploads
                .into_iter()
                .map(|upload| {
                    let percent = if upload.total_bytes > 0 {
                        #[allow(clippy::cast_precision_loss)]
                        let share = upload.progress_bytes as f64 / upload.total_bytes as f64;
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let percent = (share * 100.0).round() as u8;
                        format!(" {percent}%")
                    } else {
                        String::new()
                    };
                    format!("{}: {}{percent}", upload.platform, upload.state)
                })
                .collect(),
        }
    }
}

impl From<metocast_core::config::ConfigField> for ConfigFieldRecord {
    fn from(field: metocast_core::config::ConfigField) -> Self {
        Self {
            key: field.key,
            label: field.label,
            kind: field.kind,
            value: field.value,
            is_set: field.is_set,
        }
    }
}

impl From<ConfigValueRecord> for metocast_core::config::ConfigValue {
    fn from(value: ConfigValueRecord) -> Self {
        Self {
            key: value.key,
            value: value.value,
            clear: value.clear,
        }
    }
}

impl From<ConnectedClient> for ConnectedClientRecord {
    fn from(client: ConnectedClient) -> Self {
        Self {
            id: client.id.to_string(),
            label: client.label,
            hostname: client.hostname,
            user_agent: client.user_agent,
            connected_at: client.connected_at.to_rfc3339(),
            latency_ms: client.latency_ms,
        }
    }
}

impl From<RodecasterRecorderState> for RecorderStateRecord {
    fn from(state: RodecasterRecorderState) -> Self {
        Self {
            status: state.status,
            event_id: state.event_id.map(|id| id.to_string()),
            started_at: state.started_at.map(|date| date.to_rfc3339()),
            error: state.error,
        }
    }
}

impl From<ServerEvent> for AppleEvent {
    fn from(event: ServerEvent) -> Self {
        match event {
            ServerEvent::Connected { server_id } => Self::Connected { server_id },
            ServerEvent::ConnectorStatus { connector, status } => Self::ConnectorStatus {
                status: connector_status(connector, status),
            },
            ServerEvent::EventsList { events } => Self::EventsList {
                events: events.into_iter().map(Into::into).collect(),
            },
            ServerEvent::Event { event } => Self::Event {
                event: event.into(),
            },
            ServerEvent::PresenterState { state } => Self::PresenterState {
                state: state.into(),
            },
            ServerEvent::ConnectorStatuses { statuses } => Self::ConnectorStatuses {
                statuses: statuses
                    .into_iter()
                    .map(|(connector, status)| connector_status(connector, status))
                    .collect(),
            },
            ServerEvent::PresenterSlideChanged {
                current_slide,
                total_slides,
            } => Self::PresenterSlideChanged {
                current_slide,
                total_slides,
            },
            ServerEvent::EventChanged {} => Self::EventChanged,
            ServerEvent::Notification { level, message } => Self::Notification { level, message },
            ServerEvent::ConnectorState(ConnectorState::Obs {
                is_streaming,
                is_recording,
            }) => Self::ObsState {
                streaming: is_streaming,
                recording: is_recording,
            },
            ServerEvent::ConnectorState(ConnectorState::Atem { state }) => {
                Self::AtemState { state }
            }
            ServerEvent::ConnectorState(ConnectorState::Middlecontrol { state }) => {
                Self::MiddlecontrolState { state }
            }
            ServerEvent::ConnectorState(ConnectorState::Camera {
                is_streaming,
                is_recording,
                stream_status,
            }) => Self::CameraState {
                streaming: is_streaming,
                recording: is_recording,
                stream_status,
            },
            ServerEvent::DeviceAlerts { listeners } => Self::DeviceAlerts {
                listeners: listeners.into_iter().map(Into::into).collect(),
            },
            ServerEvent::RodecasterProfile { profile } => Self::RodecasterProfile { profile },
            ServerEvent::RodecasterMute {
                channel,
                label,
                muted,
                remote,
                notify,
            } => Self::RodecasterMute {
                channel,
                label,
                muted,
                remote,
                notify,
            },
            ServerEvent::RodecasterRecorder { state } => Self::RodecasterRecorder {
                state: state.into(),
            },
            ServerEvent::ObsDevices { devices } => Self::Discovery {
                tool: DiscoveryTool::Obs,
                devices: discovery::obs_devices(devices),
                message: None,
            },
            ServerEvent::AudioDiscovery { discovery } => {
                let (devices, message) = discovery::audio_devices(discovery);
                Self::Discovery {
                    tool: DiscoveryTool::Rodecaster,
                    devices,
                    message,
                }
            }
            ServerEvent::Error { message } => Self::CommandFailed { message },
            ServerEvent::PresentationSettings {
                use_web_presenter,
                presenter_theme,
            } => Self::PresentationSettings {
                use_web_presenter,
                theme: presenter_theme,
            },
            ServerEvent::PresentationStatus { status } => Self::PresentationStatus { status },
            ServerEvent::ClientsList { clients } | ServerEvent::ClientsUpdated { clients } => {
                Self::Clients {
                    clients: clients.into_iter().map(Into::into).collect(),
                }
            }
            ServerEvent::ConnectorState(ConnectorState::Other)
            | ServerEvent::Ping { .. }
            | ServerEvent::Unknown => Self::Unknown,
        }
    }
}

impl From<ParagraphContent> for ParagraphRecord {
    fn from(paragraph: ParagraphContent) -> Self {
        Self {
            lines: paragraph.lines,
            align: paragraph.align,
            font_size_pt: paragraph.font_size_pt,
        }
    }
}

impl From<SlideContent> for SlideRecord {
    fn from(slide: SlideContent) -> Self {
        Self {
            index: slide.index,
            paragraphs: slide.paragraphs.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<SvgSlideContent> for SvgSlideRecord {
    fn from(slide: SvgSlideContent) -> Self {
        Self {
            index: slide.index,
            svg: slide.svg,
            width_px: slide.width_px,
            height_px: slide.height_px,
        }
    }
}

impl From<PresenterState> for PresenterStateRecord {
    fn from(state: PresenterState) -> Self {
        Self {
            loaded: state.loaded,
            file_path: state.file_path,
            current_slide: state.current_slide,
            total_slides: state.total_slides,
            render_mode: match state.render_mode {
                metocast_core::protocol::PresenterRenderMode::Text => "text",
                metocast_core::protocol::PresenterRenderMode::Svg => "svg",
            }
            .to_string(),
            slides: state.slides.into_iter().map(Into::into).collect(),
            svg_slides: state.svg_slides.into_iter().map(Into::into).collect(),
            muted: state.muted,
            slide_width_emu: state.slide_width_emu,
            slide_height_emu: state.slide_height_emu,
        }
    }
}

#[uniffi::export]
pub fn bridge_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts_become_the_create_event_body_the_server_reads() {
        let draft = EventDraftRecord {
            title: "Sunday".to_string(),
            computed_title: "2026.09.20. Sunday".to_string(),
            date_time: "2026-09-20T08:00:00+00:00".to_string(),
            speaker: "Pastor".to_string(),
            description: String::new(),
            auto_upload_enabled: true,
            youtube_privacy: "unlisted".to_string(),
            bible_references: vec![BibleReferenceRecord {
                reference_type: "leckio".to_string(),
                reference: String::new(),
                translation: "RUF_v2".to_string(),
                verses: Vec::new(),
            }],
        };
        let body = serde_json::to_value(CreateEvent::try_from(draft.clone()).unwrap()).unwrap();
        assert_eq!(body["date_time"], "2026-09-20T08:00:00Z");
        assert_eq!(
            body["connections"],
            serde_json::json!([{"platform": "youtube", "privacy_status": "unlisted"}])
        );
        // An empty reference is how the server is told to delete one.
        assert_eq!(body["bible_references"][0]["reference"], "");

        let undated = EventDraftRecord {
            date_time: "tomorrow".to_string(),
            ..draft
        };
        assert!(matches!(
            CreateEvent::try_from(undated),
            Err(AppleError::InvalidInput)
        ));
    }

    #[test]
    fn production_controls_become_the_server_commands() {
        assert_eq!(
            ProductionCommand::try_from(ProductionControl::ObsStreaming { on: false }).unwrap(),
            ProductionCommand::ObsStreamStop
        );
        assert_eq!(
            ProductionCommand::try_from(ProductionControl::AtemRecording { on: true }).unwrap(),
            ProductionCommand::AtemRecordStart
        );
        assert!(matches!(
            ProductionCommand::try_from(ProductionControl::RodecasterRecordStart {
                event_id: "not an id".to_string()
            }),
            Err(AppleError::InvalidIdentifier)
        ));
    }

    #[test]
    fn invalid_configuration_has_a_typed_error() {
        assert!(matches!(
            AppleClient::new("not a URL".to_string(), "secret".to_string()),
            Err(AppleError::InvalidConfiguration)
        ));
    }
}
