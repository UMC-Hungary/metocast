use std::sync::{Arc, Mutex};

use metocast_client::{
    ClientConfig, ClientError, ConnectionState, MetocastClient, WebSocketConnection,
};
use metocast_core::{
    connectors::ConnectorStatus,
    events::{BibleReference, Event, EventConnection, EventSummary},
    protocol::{
        ParagraphContent, PresenterCommand, PresenterState, ServerEvent, SlideContent,
        SvgSlideContent,
    },
};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AppleError {
    #[error("invalid client configuration")]
    InvalidConfiguration,
    #[error("authentication was rejected")]
    Authentication,
    #[error("transport failed")]
    Transport,
    #[error("identifier is invalid")]
    InvalidIdentifier,
    #[error("event connection is not running")]
    NotConnected,
    #[error("client runtime failed")]
    Runtime,
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
    pub chapter: u32,
    pub verse: u32,
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
        self.runtime
            .spawn(async move { client.health().await })
            .await
            .map_err(|_| AppleError::Runtime)?
            .map_err(map_client_error)
    }

    pub async fn list_events(&self) -> Result<Vec<EventSummaryRecord>, AppleError> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { client.list_events().await })
            .await
            .map_err(|_| AppleError::Runtime)?
            .map(|events| events.into_iter().map(Into::into).collect())
            .map_err(map_client_error)
    }

    pub async fn event(&self, id: String) -> Result<EventRecord, AppleError> {
        let id = uuid::Uuid::parse_str(&id).map_err(|_| AppleError::InvalidIdentifier)?;
        let client = self.client.clone();
        self.runtime
            .spawn(async move { client.event(id).await })
            .await
            .map_err(|_| AppleError::Runtime)?
            .map(Into::into)
            .map_err(map_client_error)
    }

    pub async fn connector_statuses(&self) -> Result<Vec<ConnectorStatusRecord>, AppleError> {
        let client = self.client.clone();
        self.runtime
            .spawn(async move { client.connector_statuses().await })
            .await
            .map_err(|_| AppleError::Runtime)?
            .map(|statuses| {
                statuses
                    .into_iter()
                    .map(|(connector, status)| connector_status(connector, status))
                    .collect()
            })
            .map_err(map_client_error)
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
        let connection = self
            .connection
            .lock()
            .map_err(|_| AppleError::Runtime)?
            .clone()
            .ok_or(AppleError::NotConnected)?;
        self.runtime
            .spawn(async move { connection.send(control.into()).await })
            .await
            .map_err(|_| AppleError::Runtime)?
            .map_err(|_| AppleError::NotConnected)
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

fn map_client_error(error: ClientError) -> AppleError {
    match error {
        ClientError::Authentication => AppleError::Authentication,
        ClientError::Config(_) => AppleError::InvalidConfiguration,
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

#[derive(serde::Deserialize)]
struct BibleVerseWire {
    chapter: u32,
    verse: u32,
    text: String,
}

impl From<BibleReference> for BibleReferenceRecord {
    fn from(reference: BibleReference) -> Self {
        let verses = serde_json::from_value::<Vec<BibleVerseWire>>(reference.verses)
            .unwrap_or_default()
            .into_iter()
            .map(|verse| BibleVerseRecord {
                chapter: verse.chapter,
                verse: verse.verse,
                text: verse.text,
            })
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
            ServerEvent::Ping { .. } | ServerEvent::Unknown => Self::Unknown,
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
    fn invalid_configuration_has_a_typed_error() {
        assert!(matches!(
            AppleClient::new("not a URL".to_string(), "secret".to_string()),
            Err(AppleError::InvalidConfiguration)
        ));
    }
}
