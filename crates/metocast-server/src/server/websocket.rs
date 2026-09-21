use axum::{
    extract::{
        FromRequestParts, Query, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Local, Utc};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::future::poll_fn;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::{RwLock, mpsc};
use tokio_postgres::AsyncMessage;
use uuid::Uuid;

use sqlx::{PgPool, Row};

use crate::connectors::atem::AtemCommand;
use crate::connectors::rodecaster_audio::{FinalizedAudioFile, RecorderStatus};
use crate::connectors::{ConnectorStatus, RodecasterConfig, facebook, youtube};
use crate::models::{
    activity,
    cron_job::{self, CreateCronJob, UpdateCronJob},
    device_listener::DeviceListener,
    event::{
        CreateBibleReference, CreateConnection, CreateEvent, Event, EventSummary, UpdateEvent,
        current_event, fetch_event, fetch_event_summaries,
    },
    recording::{CreateRecording, FlagUploadItem, Recording, RecordingUpload},
    untracked_recording,
};
use crate::server::AppState;
use crate::server::ppt;
use crate::server::presenter;

// ── Connected client registry ─────────────────────────────────────────────────

/// Metadata about a single connected WebSocket client.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsClientInfo {
    pub id: Uuid,
    /// Human-readable label set by the client via `presenter.register`.
    pub label: String,
    pub user_agent: Option<String>,
    /// Hostname of the machine running the client, if reported via `presenter.register`.
    pub hostname: Option<String>,
    pub connected_at: chrono::DateTime<Utc>,
    pub last_pong_at: Option<chrono::DateTime<Utc>>,
    pub latency_ms: Option<i64>,
    /// Timestamp when the last ping was sent — not serialised.
    #[serde(skip)]
    pub ping_sent_at: Option<chrono::DateTime<Utc>>,
}

// ── Incoming WebSocket command types ─────────────────────────────────────────

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WsCommand {
    // ── Keynote (macOS only) ─────────────────────────────────────────────────
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.open")]
    KeynoteOpen { file_path: String },
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.next")]
    KeynoteNext,
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.prev")]
    KeynotePrev,
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.first")]
    KeynoteFirst,
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.last")]
    KeynoteLast,
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.goto")]
    KeynoteGoto { slide: u32 },
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.start")]
    KeynoteStart,
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.stop")]
    KeynoteStop,
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.close_all")]
    KeynoteCloseAll,
    #[cfg(target_os = "macos")]
    #[serde(rename = "keynote.status")]
    KeynoteStatus,
    // ── PPT ──────────────────────────────────────────────────────────────────
    #[serde(rename = "ppt.search")]
    PptSearch { filter: String },
    #[serde(rename = "ppt.folders.list")]
    PptFoldersList,
    #[serde(rename = "ppt.folders.add")]
    PptFoldersAdd { path: String, name: String },
    #[serde(rename = "ppt.folders.remove")]
    PptFoldersRemove { id: Uuid },
    // ── Events ───────────────────────────────────────────────────────────────
    #[serde(rename = "events.list")]
    EventsList,
    #[serde(rename = "events.presenter_list")]
    EventsPresenterList,
    #[serde(rename = "events.get")]
    EventsGet { id: Uuid },
    #[serde(rename = "events.create")]
    EventsCreate {
        title: String,
        computed_title: Option<String>,
        date_time: chrono::DateTime<Utc>,
        speaker: Option<String>,
        description: Option<String>,
        auto_upload_enabled: Option<bool>,
        connections: Option<Vec<CreateConnection>>,
        bible_references: Option<Vec<CreateBibleReference>>,
    },
    #[serde(rename = "events.update")]
    EventsUpdate {
        id: Uuid,
        title: String,
        computed_title: Option<String>,
        date_time: chrono::DateTime<Utc>,
        speaker: Option<String>,
        description: Option<String>,
        auto_upload_enabled: Option<bool>,
        connections: Option<Vec<CreateConnection>>,
        bible_references: Option<Vec<CreateBibleReference>>,
    },
    #[serde(rename = "events.delete")]
    EventsDelete { id: Uuid },
    // ── Recordings ───────────────────────────────────────────────────────────
    #[serde(rename = "recordings.list")]
    RecordingsList { event_id: Uuid },
    #[serde(rename = "recordings.list_all")]
    RecordingsListAll { filter: Option<String> },
    #[serde(rename = "recordings.create")]
    RecordingsCreate {
        event_id: Uuid,
        file_path: String,
        file_name: String,
        file_size: Option<i64>,
        duration_seconds: Option<f64>,
        custom_title: Option<String>,
        custom_description: Option<String>,
    },
    #[serde(rename = "recordings.delete")]
    RecordingsDelete {
        event_id: Uuid,
        recording_id: Uuid,
        delete_file: Option<bool>,
    },
    #[serde(rename = "recordings.flag_upload")]
    RecordingsFlagUpload {
        event_id: Uuid,
        recordings: Vec<FlagUploadItem>,
    },
    // ── Untracked recordings ─────────────────────────────────────────────────
    #[serde(rename = "recordings.untracked.list")]
    RecordingsUntrackedList,
    #[serde(rename = "recordings.untracked.assign")]
    RecordingsUntrackedAssign { id: Uuid, event_id: Uuid },
    #[serde(rename = "recordings.untracked.delete")]
    RecordingsUntrackedDelete { id: Uuid, delete_file: Option<bool> },
    // ── Activities ───────────────────────────────────────────────────────────
    #[serde(rename = "activities.list")]
    ActivitiesList { event_id: Uuid },
    #[serde(rename = "activities.create")]
    ActivitiesCreate {
        event_id: Uuid,
        activity_type: String,
        message: Option<String>,
    },
    #[serde(rename = "activities.delete")]
    ActivitiesDelete { event_id: Uuid, activity_id: Uuid },
    // ── Cron jobs ────────────────────────────────────────────────────────────
    #[serde(rename = "cron_jobs.list")]
    CronJobsList,
    #[serde(rename = "cron_jobs.create")]
    CronJobsCreate {
        name: String,
        cron_expression: String,
        enabled: bool,
        pull_youtube: bool,
        auto_upload: bool,
    },
    #[serde(rename = "cron_jobs.update")]
    CronJobsUpdate {
        id: Uuid,
        name: String,
        cron_expression: String,
        enabled: bool,
        pull_youtube: bool,
        auto_upload: bool,
    },
    #[serde(rename = "cron_jobs.delete")]
    CronJobsDelete { id: Uuid },
    // ── Uploads ──────────────────────────────────────────────────────────────
    #[serde(rename = "uploads.trigger")]
    UploadsTrigger,
    // ── Connectors ───────────────────────────────────────────────────────────
    #[serde(rename = "connectors.status")]
    ConnectorsStatus,
    #[serde(rename = "connectors.state")]
    ConnectorsState,
    #[serde(rename = "connectors.youtube.schedule")]
    ConnectorsYoutubeSchedule { event_id: Uuid },
    #[serde(rename = "connectors.facebook.schedule")]
    ConnectorsFacebookSchedule { event_id: Uuid },
    #[serde(rename = "connectors.youtube.stream_key")]
    ConnectorsYoutubeStreamKey,
    #[serde(rename = "connectors.facebook.stream_key")]
    ConnectorsFacebookStreamKey,
    #[serde(rename = "connectors.youtube.content")]
    ConnectorsYoutubeContent,
    // ── Auth ─────────────────────────────────────────────────────────────────
    #[serde(rename = "auth.youtube.url")]
    AuthYoutubeUrl,
    #[serde(rename = "auth.youtube.logout")]
    AuthYoutubeLogout,
    #[serde(rename = "auth.facebook.url")]
    AuthFacebookUrl,
    #[serde(rename = "auth.facebook.logout")]
    AuthFacebookLogout,
    // ── Broadlink ────────────────────────────────────────────────────────────
    #[serde(rename = "broadlink.status")]
    BroadlinkStatus,
    #[serde(rename = "broadlink.devices.list")]
    BroadlinkDevicesList,
    #[serde(rename = "broadlink.devices.add")]
    BroadlinkDevicesAdd {
        name: String,
        host: String,
        mac: String,
        device_type: String,
        model: Option<String>,
    },
    #[serde(rename = "broadlink.devices.remove")]
    BroadlinkDevicesRemove { id: Uuid },
    #[serde(rename = "broadlink.discover")]
    BroadlinkDiscover,
    #[serde(rename = "broadlink.commands.list")]
    BroadlinkCommandsList {
        device_id: Option<Uuid>,
        category: Option<String>,
    },
    #[serde(rename = "broadlink.commands.add")]
    BroadlinkCommandsAdd {
        device_id: Option<Uuid>,
        name: String,
        slug: String,
        code: String,
        code_type: String,
        category: Option<String>,
    },
    #[serde(rename = "broadlink.commands.update")]
    BroadlinkCommandsUpdate {
        id: Uuid,
        name: Option<String>,
        slug: Option<String>,
        code: Option<String>,
        code_type: Option<String>,
        category: Option<String>,
    },
    #[serde(rename = "broadlink.commands.remove")]
    BroadlinkCommandsRemove { id: Uuid },
    #[serde(rename = "broadlink.learn.start")]
    BroadlinkLearnStart {
        device_id: Uuid,
        signal_type: Option<String>,
    },
    #[serde(rename = "broadlink.learn.cancel")]
    BroadlinkLearnCancel,
    #[serde(rename = "broadlink.commands.send")]
    BroadlinkCommandsSend { id: Uuid },
    // ── Blackmagic camera ────────────────────────────────────────────────────
    /// mDNS scan for cameras on the LAN. Returns nothing on networks where
    /// multicast does not arrive — a camera added by host still works.
    #[serde(rename = "blackmagic-camera.discover")]
    BlackmagicCameraDiscover { timeout_secs: Option<u64> },
    #[serde(rename = "blackmagic-camera.record.start")]
    BlackmagicCameraRecordStart { clip_name: Option<String> },
    #[serde(rename = "blackmagic-camera.record.stop")]
    BlackmagicCameraRecordStop,
    #[serde(rename = "blackmagic-camera.stream.start")]
    BlackmagicCameraStreamStart,
    #[serde(rename = "blackmagic-camera.stream.stop")]
    BlackmagicCameraStreamStop,
    /// Copy the channel's RTMP ingestion address and stream key into the camera's
    /// livestream settings. Does not go live — send `blackmagic.stream.start` for that.
    #[serde(rename = "blackmagic-camera.stream.push_youtube")]
    BlackmagicCameraStreamPushYoutube,
    // ── Middle Control ────────────────────────────────────────────────────────
    #[serde(rename = "middlecontrol.camera.select")]
    MiddlecontrolCameraSelect { camera_id: u8 },
    #[serde(rename = "middlecontrol.record.start")]
    MiddlecontrolRecordStart { camera_id: Option<u8> },
    #[serde(rename = "middlecontrol.record.stop")]
    MiddlecontrolRecordStop { camera_id: Option<u8> },
    #[serde(rename = "middlecontrol.record.start_all")]
    MiddlecontrolRecordStartAll,
    #[serde(rename = "middlecontrol.record.stop_all")]
    MiddlecontrolRecordStopAll,
    #[serde(rename = "middlecontrol.preset.recall")]
    MiddlecontrolPresetRecall { preset: u8, camera_id: Option<u8> },
    #[serde(rename = "atem.program.set")]
    AtemProgramSet { input: u16 },
    #[serde(rename = "atem.preview.set")]
    AtemPreviewSet { input: u16 },
    #[serde(rename = "atem.cut")]
    AtemCut,
    #[serde(rename = "atem.auto")]
    AtemAuto,
    #[serde(rename = "atem.record.start")]
    AtemRecordStart,
    #[serde(rename = "atem.record.stop")]
    AtemRecordStop,
    #[serde(rename = "atem.stream.start")]
    AtemStreamStart,
    #[serde(rename = "atem.stream.stop")]
    AtemStreamStop,
    #[serde(rename = "atem.stream.push_youtube")]
    AtemStreamPushYoutube,
    // ── RØDECaster ───────────────────────────────────────────────────────────
    #[serde(rename = "rodecaster.profile")]
    RodecasterProfile,
    #[serde(rename = "rodecaster.mute.set")]
    RodecasterMuteSet { channel: usize, mute: bool },
    #[serde(rename = "rodecaster.audio.discover")]
    RodecasterAudioDiscover,
    #[serde(rename = "rodecaster.audio.record.state")]
    RodecasterAudioRecordState,
    #[serde(rename = "rodecaster.audio.record.start")]
    RodecasterAudioRecordStart { event_id: Uuid },
    #[serde(rename = "rodecaster.audio.record.stop")]
    RodecasterAudioRecordStop,
    // ── Presenter ────────────────────────────────────────────────────────────
    /// Register a human-readable label and hostname for this connection (shown in the UI).
    #[serde(rename = "presenter.register")]
    PresenterRegister {
        label: String,
        hostname: Option<String>,
    },
    /// Request the list of currently connected clients (reply to sender only).
    #[serde(rename = "clients.list")]
    ClientsList,
    /// Send a ping to a specific connected client.
    #[serde(rename = "clients.ping")]
    ClientsPing { client_id: Uuid },
    /// Pong reply from a client in response to a `ping` message.
    #[serde(rename = "pong")]
    Pong { ping_id: i64 },
    #[serde(rename = "presenter.load")]
    PresenterLoad {
        file_path: String,
        render_mode: Option<presenter::PresenterRenderMode>,
    },
    #[serde(rename = "presenter.load_bible_reference")]
    PresenterLoadBibleReference {
        event_id: Option<Uuid>,
        reference_type: presenter::BibleReferenceType,
    },
    #[serde(rename = "presenter.unload")]
    PresenterUnload,
    #[serde(rename = "presenter.next")]
    PresenterNext,
    #[serde(rename = "presenter.prev")]
    PresenterPrev,
    #[serde(rename = "presenter.first")]
    PresenterFirst,
    #[serde(rename = "presenter.last")]
    PresenterLast,
    #[serde(rename = "presenter.goto")]
    PresenterGoto { slide: u32 },
    #[serde(rename = "presenter.status")]
    PresenterStatus,
    #[serde(rename = "presenter.mute")]
    PresenterMute,
    #[serde(rename = "presenter.unmute")]
    PresenterUnmute,
    /// Update the raw text content of a single slide (1-based index).
    #[serde(rename = "presenter.slide.update")]
    PresenterSlideUpdate {
        slide_index: u32,
        texts: Vec<String>,
    },
    // ── Unified Presentation (auto-routes to active backend) ─────────────────
    /// Request the current presentation settings (use_web_presenter flag).
    #[serde(rename = "presentation.get_settings")]
    PresentationGetSettings,
    /// Request unified presentation status (works for both web presenter and Keynote).
    #[serde(rename = "presentation.status")]
    PresentationStatus,
    /// Toggle the active presentation backend; closes any running presentation first.
    #[serde(rename = "presentation.set_use_web_presenter")]
    PresentationSetUseWebPresenter { enabled: bool },
    /// Select the visual treatment used by text-mode song and Bible slides.
    #[serde(rename = "presentation.set_presenter_theme")]
    PresentationSetPresenterTheme { theme: presenter::PresenterTheme },
    /// Open a file: routes to web presenter or Keynote based on the stored setting.
    #[serde(rename = "presentation.open")]
    PresentationOpen {
        file_path: String,
        render_mode: Option<presenter::PresenterRenderMode>,
    },
    /// Start the slideshow (Keynote only; shows notification in web presenter mode).
    #[serde(rename = "presentation.start")]
    PresentationStart,
    /// Stop the Keynote slideshow (shows notification in web presenter mode; does NOT close).
    #[serde(rename = "presentation.stop")]
    PresentationStop,
    /// Close/unload the active presentation: unloads web presenter or closes all in Keynote.
    #[serde(rename = "presentation.close")]
    PresentationClose,
    /// Close all presentations (Keynote only; shows notification in web presenter mode).
    #[serde(rename = "presentation.close_all")]
    PresentationCloseAll,
    #[serde(rename = "presentation.next")]
    PresentationNext,
    #[serde(rename = "presentation.prev")]
    PresentationPrev,
    #[serde(rename = "presentation.first")]
    PresentationFirst,
    #[serde(rename = "presentation.last")]
    PresentationLast,
    #[serde(rename = "presentation.goto")]
    PresentationGoto { slide: u32 },
    /// Mute (blank) the active presentation display.
    #[serde(rename = "presentation.mute")]
    PresentationMute,
    /// Unmute the active presentation display.
    #[serde(rename = "presentation.unmute")]
    PresentationUnmute,
    // ── OBS Devices ──────────────────────────────────────────────────────────
    #[serde(rename = "obs.devices.scan")]
    ObsDevicesScan,
    #[serde(rename = "obs.devices.available")]
    ObsDevicesAvailable,
    #[serde(rename = "obs.listeners.list")]
    ObsListenersList,
    #[serde(rename = "obs.listeners.create")]
    ObsListenersCreate {
        connector_type: String,
        category: String,
        device_item_value: String,
        device_item_name: String,
        friendly_name: String,
    },
    #[serde(rename = "obs.listeners.update")]
    ObsListenersUpdate { id: Uuid, friendly_name: String },
    #[serde(rename = "obs.listeners.delete")]
    ObsListenersDelete { id: Uuid },
}

/// Upsert or delete bible references inside an open transaction (mirrors routes.rs logic).
async fn ws_upsert_bible_references(
    event_id: Uuid,
    refs: &Option<Vec<CreateBibleReference>>,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> anyhow::Result<()> {
    let Some(refs) = refs else { return Ok(()) };
    for br in refs {
        let reference = br.reference.as_deref().unwrap_or("").trim();
        if reference.is_empty() {
            sqlx::query("DELETE FROM event_bible_references WHERE event_id = $1 AND type = $2")
                .bind(event_id)
                .bind(&br.r#type)
                .execute(&mut **tx)
                .await?;
        } else {
            let translation = br.translation.as_deref().unwrap_or("UF");
            let verses = br.verses.clone().unwrap_or_else(|| serde_json::json!([]));
            sqlx::query(
                "INSERT INTO event_bible_references (event_id, type, reference, translation, verses) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (event_id, type) DO UPDATE SET \
                   reference = EXCLUDED.reference, translation = EXCLUDED.translation, \
                   verses = EXCLUDED.verses, updated_at = NOW()",
            )
            .bind(event_id)
            .bind(&br.r#type)
            .bind(reference)
            .bind(translation)
            .bind(verses)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn make_presentation_settings(state: &AppState) -> String {
    let enabled = state.use_web_presenter.load(Ordering::Relaxed);
    let presenter_theme = *state.presenter_theme.read().await;
    json!({
        "type": "presentation.settings",
        "useWebPresenter": enabled,
        "presenterTheme": presenter_theme,
    })
    .to_string()
}

/// Build a unified `presentation.status` JSON string from current backend state.
async fn make_presentation_status(state: &AppState) -> String {
    let (app_running, slideshow_active, current_slide, total_slides, document_name, blanked) =
        if state.use_web_presenter.load(Ordering::Relaxed) {
            let ps = state.presenter_state.read().await;
            let doc = ps
                .file_path
                .as_ref()
                .and_then(|p| p.split('/').next_back())
                .map(str::to_owned);
            (
                ps.loaded,
                ps.loaded,
                if ps.loaded {
                    Some(ps.current_slide)
                } else {
                    None
                },
                if ps.loaded {
                    Some(ps.total_slides)
                } else {
                    None
                },
                doc,
                ps.muted,
            )
        } else {
            #[cfg(target_os = "macos")]
            {
                let s = state.keynote_connector.get_status().await;
                (
                    s.app_running,
                    s.slideshow_active,
                    s.current_slide,
                    s.total_slides,
                    s.document_name,
                    false,
                )
            }
            #[cfg(not(target_os = "macos"))]
            {
                (
                    false,
                    false,
                    None::<u32>,
                    None::<u32>,
                    None::<String>,
                    false,
                )
            }
        };
    json!({
        "type": "presentation.status",
        "status": {
            "appRunning": app_running,
            "slideshowActive": slideshow_active,
            "currentSlide": current_slide,
            "totalSlides": total_slides,
            "documentName": document_name,
            "blanked": blanked,
        }
    })
    .to_string()
}

async fn broadcast_presentation_status(
    ws_clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    state: &AppState,
) {
    let msg = make_presentation_status(state).await;
    let clients = ws_clients.read().await;
    for tx in clients.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

async fn broadcast_notification(
    ws_clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    level: &str,
    message: &str,
) {
    let msg = json!({ "type": "notification", "level": level, "message": message }).to_string();
    let clients = ws_clients.read().await;
    for tx in clients.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

fn ws_ok(tx: &mpsc::UnboundedSender<Message>) {
    let _ = tx.send(Message::Text(json!({"type":"ok"}).to_string().into()));
}

fn ws_error(tx: &mpsc::UnboundedSender<Message>, msg: &str) {
    let _ = tx.send(Message::Text(
        json!({"type":"error","message":msg}).to_string().into(),
    ));
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresenterEventList {
    events: Vec<EventSummary>,
    selected_event_id: Option<Uuid>,
}

async fn fetch_presenter_event_list(pool: &PgPool) -> Result<PresenterEventList, sqlx::Error> {
    let now = Utc::now();
    let mut events = fetch_event_summaries(pool).await?;
    events.sort_by(|a, b| compare_presenter_events(a, b, now));
    let selected_event_id = current_event(&events, now).map(|event| event.id);
    Ok(PresenterEventList {
        events,
        selected_event_id,
    })
}

fn compare_presenter_events(
    a: &EventSummary,
    b: &EventSummary,
    now: DateTime<Utc>,
) -> std::cmp::Ordering {
    let today = now.with_timezone(&Local).date_naive();
    let a_bucket = presenter_event_bucket(a.date_time, now, today);
    let b_bucket = presenter_event_bucket(b.date_time, now, today);
    if a_bucket != b_bucket {
        return a_bucket.cmp(&b_bucket);
    }

    match a_bucket {
        0 => {
            let a_future = a.date_time >= now;
            let b_future = b.date_time >= now;
            if a_future != b_future {
                return b_future.cmp(&a_future);
            }
            if a_future {
                a.date_time.cmp(&b.date_time)
            } else {
                b.date_time.cmp(&a.date_time)
            }
        }
        1 => a.date_time.cmp(&b.date_time),
        _ => b.date_time.cmp(&a.date_time),
    }
}

fn presenter_event_bucket(
    event_time: DateTime<Utc>,
    now: DateTime<Utc>,
    today: chrono::NaiveDate,
) -> u8 {
    if event_time.with_timezone(&Local).date_naive() == today {
        0
    } else if event_time >= now {
        1
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blackmagic_camera_commands_parse_from_their_wire_names() {
        let parsed = |text: &str| serde_json::from_str::<WsCommand>(text).expect(text);

        assert!(matches!(
            parsed(r#"{"type":"blackmagic-camera.discover"}"#),
            WsCommand::BlackmagicCameraDiscover { timeout_secs: None }
        ));
        assert!(matches!(
            parsed(r#"{"type":"blackmagic-camera.record.start","clip_name":"service"}"#),
            WsCommand::BlackmagicCameraRecordStart { clip_name: Some(name) } if name == "service"
        ));
        assert!(matches!(
            parsed(r#"{"type":"blackmagic-camera.record.stop"}"#),
            WsCommand::BlackmagicCameraRecordStop
        ));
        assert!(matches!(
            parsed(r#"{"type":"blackmagic-camera.stream.start"}"#),
            WsCommand::BlackmagicCameraStreamStart
        ));
        assert!(matches!(
            parsed(r#"{"type":"blackmagic-camera.stream.push_youtube"}"#),
            WsCommand::BlackmagicCameraStreamPushYoutube
        ));
        // The old un-suffixed names must not resolve — `blackmagic` alone is ambiguous
        // once an ATEM connector exists.
        assert!(serde_json::from_str::<WsCommand>(r#"{"type":"blackmagic.record.stop"}"#).is_err());
    }

    #[test]
    fn atem_commands_parse_from_their_wire_names() {
        let parsed = |text: &str| serde_json::from_str::<WsCommand>(text).expect(text);

        assert!(matches!(
            parsed(r#"{"type":"atem.program.set","input":3}"#),
            WsCommand::AtemProgramSet { input: 3 }
        ));
        assert!(matches!(
            parsed(r#"{"type":"atem.preview.set","input":1000}"#),
            WsCommand::AtemPreviewSet { input: 1000 }
        ));
        assert!(matches!(
            parsed(r#"{"type":"atem.cut"}"#),
            WsCommand::AtemCut
        ));
        assert!(matches!(
            parsed(r#"{"type":"atem.auto"}"#),
            WsCommand::AtemAuto
        ));
        assert!(matches!(
            parsed(r#"{"type":"atem.record.start"}"#),
            WsCommand::AtemRecordStart
        ));
        assert!(matches!(
            parsed(r#"{"type":"atem.stream.stop"}"#),
            WsCommand::AtemStreamStop
        ));
        assert!(matches!(
            parsed(r#"{"type":"atem.stream.push_youtube"}"#),
            WsCommand::AtemStreamPushYoutube
        ));
    }

    #[test]
    fn middlecontrol_commands_parse_from_their_wire_names() {
        let parsed = |text: &str| serde_json::from_str::<WsCommand>(text).expect(text);

        assert!(matches!(
            parsed(r#"{"type":"middlecontrol.camera.select","camera_id":3}"#),
            WsCommand::MiddlecontrolCameraSelect { camera_id: 3 }
        ));
        assert!(matches!(
            parsed(r#"{"type":"middlecontrol.record.start"}"#),
            WsCommand::MiddlecontrolRecordStart { camera_id: None }
        ));
        assert!(matches!(
            parsed(r#"{"type":"middlecontrol.record.stop","camera_id":2}"#),
            WsCommand::MiddlecontrolRecordStop { camera_id: Some(2) }
        ));
        assert!(matches!(
            parsed(r#"{"type":"middlecontrol.record.start_all"}"#),
            WsCommand::MiddlecontrolRecordStartAll
        ));
        assert!(matches!(
            parsed(r#"{"type":"middlecontrol.record.stop_all"}"#),
            WsCommand::MiddlecontrolRecordStopAll
        ));
        assert!(matches!(
            parsed(r#"{"type":"middlecontrol.preset.recall","preset":4,"camera_id":5}"#),
            WsCommand::MiddlecontrolPresetRecall {
                preset: 4,
                camera_id: Some(5)
            }
        ));
    }

    #[test]
    fn rodecaster_commands_parse_from_their_wire_names() {
        let parsed = |text: &str| serde_json::from_str::<WsCommand>(text).expect(text);

        assert!(matches!(
            parsed(r#"{"type":"rodecaster.profile"}"#),
            WsCommand::RodecasterProfile
        ));
        assert!(matches!(
            parsed(r#"{"type":"rodecaster.mute.set","channel":3,"mute":true}"#),
            WsCommand::RodecasterMuteSet {
                channel: 3,
                mute: true
            }
        ));
        assert!(serde_json::from_str::<WsCommand>(r#"{"type":"rodecaster.mute.set"}"#).is_err());
        assert!(matches!(
            parsed(r#"{"type":"rodecaster.audio.discover"}"#),
            WsCommand::RodecasterAudioDiscover
        ));
        assert!(matches!(
            parsed(r#"{"type":"rodecaster.audio.record.state"}"#),
            WsCommand::RodecasterAudioRecordState
        ));
        let event_id = Uuid::new_v4();
        assert!(matches!(
            parsed(&format!(
                r#"{{"type":"rodecaster.audio.record.start","event_id":"{event_id}"}}"#
            )),
            WsCommand::RodecasterAudioRecordStart { event_id: parsed_id } if parsed_id == event_id
        ));
        assert!(matches!(
            parsed(r#"{"type":"rodecaster.audio.record.stop"}"#),
            WsCommand::RodecasterAudioRecordStop
        ));
    }
}

async fn handle_ws_command(
    cmd: WsCommand,
    state: &AppState,
    client_tx: &mpsc::UnboundedSender<Message>,
    client_id: Uuid,
) {
    match cmd {
        // ── Keynote (macOS only) ─────────────────────────────────────────────
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteOpen { file_path } => {
            let _ = state.keynote_connector.open_file(&file_path).await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteNext => {
            let _ = state.keynote_connector.next().await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynotePrev => {
            let _ = state.keynote_connector.prev().await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteFirst => {
            let _ = state.keynote_connector.first().await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteLast => {
            let _ = state.keynote_connector.last().await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteGoto { slide } => {
            let _ = state.keynote_connector.goto(slide).await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteStart => {
            let _ = state.keynote_connector.start_slideshow().await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteStop => {
            let _ = state.keynote_connector.stop_slideshow().await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteCloseAll => {
            let _ = state.keynote_connector.close_all().await;
        }
        #[cfg(target_os = "macos")]
        WsCommand::KeynoteStatus => {
            let status = state.keynote_connector.get_status().await;
            let msg = json!({ "type": "keynote.status", "status": status }).to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        // ── PPT ──────────────────────────────────────────────────────────────
        WsCommand::PptSearch { filter } => {
            let files = ppt::search_files_internal(&state.pool, &filter).await;
            let msg = json!({ "type": "ppt.search_results", "files": files, "filter": filter })
                .to_string();
            let clients = state.ws_clients.read().await;
            for tx in clients.values() {
                let _ = tx.send(Message::Text(msg.clone().into()));
            }
        }
        WsCommand::PptFoldersList => {
            match sqlx::query_as::<_, ppt::PptFolder>(
                "SELECT id, path, name, sort_order FROM ppt_folders ORDER BY sort_order, name",
            )
            .fetch_all(&state.pool)
            .await
            {
                Ok(folders) => {
                    let msg = json!({ "type": "ppt.folders.list", "folders": folders }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::PptFoldersAdd { path, name } => {
            let result = sqlx::query_as::<_, ppt::PptFolder>(
                "INSERT INTO ppt_folders (path, name) VALUES ($1, $2) \
                 ON CONFLICT (path) DO UPDATE SET name = EXCLUDED.name \
                 RETURNING id, path, name, sort_order",
            )
            .bind(&path)
            .bind(&name)
            .fetch_optional(&state.pool)
            .await;
            match result {
                Ok(folder) => {
                    broadcast_ppt_folders_changed(&state.ws_clients).await;
                    let msg = json!({ "type": "ppt.folders.add", "folder": folder }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::PptFoldersRemove { id } => {
            match sqlx::query("DELETE FROM ppt_folders WHERE id = $1")
                .bind(id)
                .execute(&state.pool)
                .await
            {
                Ok(_) => {
                    broadcast_ppt_folders_changed(&state.ws_clients).await;
                    ws_ok(client_tx);
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── Client registry & ping ───────────────────────────────────────────
        WsCommand::PresenterRegister { label, hostname } => {
            {
                let mut info = state.ws_client_info.write().await;
                if let Some(client) = info.get_mut(&client_id) {
                    client.label = label;
                    client.hostname = hostname;
                }
            }
            broadcast_clients_updated(state).await;
        }
        WsCommand::ClientsList => {
            let clients_vec = {
                let info = state.ws_client_info.read().await;
                let mut v: Vec<WsClientInfo> = info.values().cloned().collect();
                v.sort_by_key(|c| c.connected_at);
                v
            };
            let msg = json!({ "type": "clients.list", "clients": clients_vec }).to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::ClientsPing {
            client_id: target_id,
        } => {
            let ping_sent_at = Utc::now();
            let ping_id = ping_sent_at.timestamp_millis();
            {
                let mut info = state.ws_client_info.write().await;
                if let Some(client) = info.get_mut(&target_id) {
                    client.ping_sent_at = Some(ping_sent_at);
                }
            }
            let clients = state.ws_clients.read().await;
            if let Some(target_tx) = clients.get(&target_id) {
                let ping_msg = json!({ "type": "ping", "pingId": ping_id }).to_string();
                let _ = target_tx.send(Message::Text(ping_msg.into()));
            }
        }
        WsCommand::Pong { ping_id } => {
            let now = Utc::now();
            {
                let mut info = state.ws_client_info.write().await;
                if let Some(client) = info.get_mut(&client_id) {
                    client.latency_ms = Some(now.timestamp_millis() - ping_id);
                    client.last_pong_at = Some(now);
                    client.ping_sent_at = None;
                }
            }
            broadcast_clients_updated(state).await;
        }
        // ── Presenter ────────────────────────────────────────────────────────
        WsCommand::PresenterLoad {
            file_path,
            render_mode,
        } => {
            let mode = render_mode.unwrap_or(presenter::PresenterRenderMode::Text);
            let result =
                tokio::task::spawn_blocking(move || presenter::load_pptx(&file_path, mode)).await;
            match result {
                Ok(Ok(new_state)) => {
                    *state.presenter_state.write().await = new_state;
                    broadcast_presenter_state(
                        &state.ws_clients,
                        &*state.presenter_state.read().await,
                    )
                    .await;
                }
                Ok(Err(e)) => ws_error(client_tx, &e),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterLoadBibleReference {
            event_id,
            reference_type,
        } => {
            let event_id = match event_id {
                Some(id) => id,
                None => match fetch_presenter_event_list(&state.pool).await {
                    Ok(list) => match list.selected_event_id {
                        Some(id) => id,
                        None => {
                            ws_error(client_tx, "No presenter event is available");
                            return;
                        }
                    },
                    Err(e) => {
                        ws_error(client_tx, &e.to_string());
                        return;
                    }
                },
            };
            let event = match fetch_event(event_id, &state.pool).await {
                Ok(Some(event)) => event,
                Ok(None) => {
                    ws_error(client_tx, "Event not found");
                    return;
                }
                Err(e) => {
                    ws_error(client_tx, &e.to_string());
                    return;
                }
            };
            let Some(reference) = event
                .bible_references
                .iter()
                .find(|item| item.r#type == reference_type.as_str())
            else {
                ws_error(client_tx, "Bible reference not found for selected event");
                return;
            };
            let verses: Vec<presenter::BibleVerseContent> =
                match serde_json::from_value(reference.verses.clone()) {
                    Ok(verses) => verses,
                    Err(e) => {
                        ws_error(client_tx, &format!("Invalid Bible verse payload: {e}"));
                        return;
                    }
                };
            if verses.is_empty() {
                ws_error(client_tx, "Selected Bible reference has no verses");
                return;
            }

            let new_state = presenter::PresenterState::from_bible_reference(
                &event.title,
                reference_type,
                &reference.reference,
                verses,
            );
            *state.presenter_state.write().await = new_state;
            broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterUnload => {
            *state.presenter_state.write().await = presenter::PresenterState::empty();
            broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterNext => {
            state.presenter_state.write().await.go_next();
            broadcast_presenter_slide_changed(
                &state.ws_clients,
                &*state.presenter_state.read().await,
            )
            .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterPrev => {
            state.presenter_state.write().await.go_prev();
            broadcast_presenter_slide_changed(
                &state.ws_clients,
                &*state.presenter_state.read().await,
            )
            .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterFirst => {
            state.presenter_state.write().await.go_first();
            broadcast_presenter_slide_changed(
                &state.ws_clients,
                &*state.presenter_state.read().await,
            )
            .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterLast => {
            state.presenter_state.write().await.go_last();
            broadcast_presenter_slide_changed(
                &state.ws_clients,
                &*state.presenter_state.read().await,
            )
            .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterGoto { slide } => {
            state.presenter_state.write().await.go_to(slide);
            broadcast_presenter_slide_changed(
                &state.ws_clients,
                &*state.presenter_state.read().await,
            )
            .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterStatus => {
            let ps = state.presenter_state.read().await;
            let msg = serde_json::json!({ "type": "presenter.state", "state": &*ps }).to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::PresenterMute => {
            state.presenter_state.write().await.mute();
            broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterUnmute => {
            state.presenter_state.write().await.unmute();
            broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                .await;
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresenterSlideUpdate { slide_index, texts } => {
            let mut presenter_state = state.presenter_state.write().await;
            if presenter_state.render_mode != presenter::PresenterRenderMode::Text {
                ws_error(
                    client_tx,
                    "Slide text editing is only available in text presenter mode",
                );
            } else {
                presenter_state.update_slide(slide_index, texts);
                drop(presenter_state);
                broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                    .await;
            }
        }
        // ── Unified Presentation ──────────────────────────────────────────────
        WsCommand::PresentationGetSettings => {
            let msg = make_presentation_settings(state).await;
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::PresentationStatus => {
            let msg = make_presentation_status(state).await;
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::PresentationSetUseWebPresenter { enabled } => {
            // Close the active presentation before switching backends.
            if state.use_web_presenter.load(Ordering::Relaxed) {
                *state.presenter_state.write().await = presenter::PresenterState::empty();
                broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                    .await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.stop_slideshow().await;
                }
            }
            // Persist to database.
            let _ = sqlx::query(
                "INSERT INTO app_settings (key, value) VALUES ('use_web_presenter', $1) \
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            )
            .bind(enabled.to_string())
            .execute(&state.pool)
            .await;
            // Update in-memory flag.
            state.use_web_presenter.store(enabled, Ordering::Relaxed);
            // Broadcast new setting to all clients.
            let msg = make_presentation_settings(state).await;
            let clients = state.ws_clients.read().await;
            for tx in clients.values() {
                let _ = tx.send(Message::Text(msg.clone().into()));
            }
        }
        WsCommand::PresentationSetPresenterTheme { theme } => {
            if let Err(error) = sqlx::query(
                "INSERT INTO app_settings (key, value) VALUES ('presenter_theme', $1) \
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            )
            .bind(theme.as_str())
            .execute(&state.pool)
            .await
            {
                ws_error(
                    client_tx,
                    &format!("Could not save presenter theme: {error}"),
                );
                return;
            }
            *state.presenter_theme.write().await = theme;
            let msg = make_presentation_settings(state).await;
            let clients = state.ws_clients.read().await;
            for tx in clients.values() {
                let _ = tx.send(Message::Text(msg.clone().into()));
            }
        }
        WsCommand::PresentationOpen {
            file_path,
            render_mode,
        } => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                let mode = render_mode.unwrap_or(presenter::PresenterRenderMode::Svg);
                let result =
                    tokio::task::spawn_blocking(move || presenter::load_pptx(&file_path, mode))
                        .await;
                match result {
                    Ok(Ok(new_state)) => {
                        *state.presenter_state.write().await = new_state;
                        broadcast_presenter_state(
                            &state.ws_clients,
                            &*state.presenter_state.read().await,
                        )
                        .await;
                    }
                    Ok(Err(e)) => ws_error(client_tx, &e),
                    Err(e) => ws_error(client_tx, &e.to_string()),
                }
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.open_file(&file_path).await;
                }
            }
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresentationStart => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                broadcast_notification(&state.ws_clients, "warn",
                    "▶ Play is only available in Keynote mode. Web Presenter loads and displays on open.").await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.start_slideshow().await;
                }
                broadcast_presentation_status(&state.ws_clients, state).await;
            }
        }
        WsCommand::PresentationStop => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                broadcast_notification(&state.ws_clients, "warn",
                    "⏹ Stop is only available in Keynote mode. Use the Close button to unload the web presentation.").await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.stop_slideshow().await;
                }
                broadcast_presentation_status(&state.ws_clients, state).await;
            }
        }
        WsCommand::PresentationClose => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                *state.presenter_state.write().await = presenter::PresenterState::empty();
                broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                    .await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.close_all().await;
                }
            }
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresentationCloseAll => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                broadcast_notification(&state.ws_clients, "warn",
                    "✕ Close All is only available in Keynote mode. Use the Close button to unload the web presentation.").await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.close_all().await;
                }
                broadcast_presentation_status(&state.ws_clients, state).await;
            }
        }
        WsCommand::PresentationNext => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                state.presenter_state.write().await.go_next();
                broadcast_presenter_slide_changed(
                    &state.ws_clients,
                    &*state.presenter_state.read().await,
                )
                .await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.next().await;
                }
            }
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresentationPrev => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                state.presenter_state.write().await.go_prev();
                broadcast_presenter_slide_changed(
                    &state.ws_clients,
                    &*state.presenter_state.read().await,
                )
                .await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.prev().await;
                }
            }
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresentationFirst => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                state.presenter_state.write().await.go_first();
                broadcast_presenter_slide_changed(
                    &state.ws_clients,
                    &*state.presenter_state.read().await,
                )
                .await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.first().await;
                }
            }
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresentationLast => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                state.presenter_state.write().await.go_last();
                broadcast_presenter_slide_changed(
                    &state.ws_clients,
                    &*state.presenter_state.read().await,
                )
                .await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.last().await;
                }
            }
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresentationGoto { slide } => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                state.presenter_state.write().await.go_to(slide);
                broadcast_presenter_slide_changed(
                    &state.ws_clients,
                    &*state.presenter_state.read().await,
                )
                .await;
            } else {
                #[cfg(target_os = "macos")]
                {
                    let _ = state.keynote_connector.goto(slide).await;
                }
            }
            broadcast_presentation_status(&state.ws_clients, state).await;
        }
        WsCommand::PresentationMute => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                state.presenter_state.write().await.mute();
                broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                    .await;
                broadcast_presentation_status(&state.ws_clients, state).await;
            }
        }
        WsCommand::PresentationUnmute => {
            if state.use_web_presenter.load(Ordering::Relaxed) {
                state.presenter_state.write().await.unmute();
                broadcast_presenter_state(&state.ws_clients, &*state.presenter_state.read().await)
                    .await;
                broadcast_presentation_status(&state.ws_clients, state).await;
            }
        }
        // ── Events ───────────────────────────────────────────────────────────
        WsCommand::EventsList => match fetch_event_summaries(&state.pool).await {
            Ok(events) => {
                let msg = json!({ "type": "events.list", "events": events }).to_string();
                let _ = client_tx.send(Message::Text(msg.into()));
            }
            Err(e) => ws_error(client_tx, &e.to_string()),
        },
        WsCommand::EventsPresenterList => match fetch_presenter_event_list(&state.pool).await {
            Ok(list) => {
                let msg = json!({
                    "type": "events.presenter_list",
                    "events": list.events,
                    "selectedEventId": list.selected_event_id,
                })
                .to_string();
                let _ = client_tx.send(Message::Text(msg.into()));
            }
            Err(e) => ws_error(client_tx, &e.to_string()),
        },
        WsCommand::EventsGet { id } => match fetch_event(id, &state.pool).await {
            Ok(Some(event)) => {
                let msg = json!({ "type": "events.get", "event": event }).to_string();
                let _ = client_tx.send(Message::Text(msg.into()));
            }
            Ok(None) => ws_error(client_tx, "not_found"),
            Err(e) => ws_error(client_tx, &e.to_string()),
        },
        WsCommand::EventsCreate {
            title,
            computed_title,
            date_time,
            speaker,
            description,
            auto_upload_enabled,
            connections,
            bible_references,
        } => {
            let body = CreateEvent {
                title,
                computed_title,
                date_time,
                speaker,
                description,
                auto_upload_enabled,
                connections,
                bible_references,
            };
            let result: anyhow::Result<Event> = async {
                let mut tx = state.pool.begin().await?;
                sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
                    .execute(&mut *tx)
                    .await?;
                let event_id: Uuid = sqlx::query_scalar(
                    "INSERT INTO events (title, computed_title, date_time, speaker, description, \
                     auto_upload_enabled) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
                )
                .bind(&body.title)
                .bind(body.computed_title.unwrap_or_default())
                .bind(body.date_time)
                .bind(body.speaker.unwrap_or_default())
                .bind(body.description.unwrap_or_default())
                .bind(body.auto_upload_enabled.unwrap_or(false))
                .fetch_one(&mut *tx)
                .await?;
                let mut conn_map: Vec<(String, String)> = vec![
                    ("youtube".to_string(), "private".to_string()),
                    ("facebook".to_string(), "EVERYONE".to_string()),
                ];
                if let Some(req_conns) = &body.connections {
                    for c in req_conns {
                        if let Some(entry) = conn_map.iter_mut().find(|(p, _)| p == &c.platform) {
                            if let Some(ps) = &c.privacy_status {
                                entry.1 = ps.clone();
                            }
                        } else {
                            conn_map.push((
                                c.platform.clone(),
                                c.privacy_status.clone().unwrap_or_default(),
                            ));
                        }
                    }
                }
                for (platform, privacy) in &conn_map {
                    sqlx::query(
                        "INSERT INTO event_connections (event_id, platform, privacy_status) VALUES ($1, $2, $3)",
                    )
                    .bind(event_id)
                    .bind(platform)
                    .bind(privacy)
                    .execute(&mut *tx)
                    .await?;
                }
                ws_upsert_bible_references(event_id, &body.bible_references, &mut tx).await?;
                tx.commit().await?;
                fetch_event(event_id, &state.pool)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("event not found after create"))
            }
            .await;
            match result {
                Ok(event) => {
                    let msg = json!({ "type": "events.create", "event": event }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                    broadcast_event_changed(state, "INSERT", &event).await;
                    spawn_scheduling_tasks(event, state.clone());
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::EventsUpdate {
            id,
            title,
            computed_title,
            date_time,
            speaker,
            description,
            auto_upload_enabled,
            connections,
            bible_references,
        } => {
            let body = UpdateEvent {
                title,
                computed_title,
                date_time,
                speaker,
                description,
                auto_upload_enabled,
                connections,
                bible_references,
            };
            let result: anyhow::Result<Option<Event>> = async {
                let mut tx = state.pool.begin().await?;
                sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
                    .execute(&mut *tx)
                    .await?;
                let updated_id: Option<Uuid> = sqlx::query_scalar(
                    "UPDATE events SET title=$1, computed_title=$2, date_time=$3, speaker=$4, \
                     description=$5, auto_upload_enabled=$6, updated_at=NOW() WHERE id=$7 \
                     RETURNING id",
                )
                .bind(&body.title)
                .bind(body.computed_title.unwrap_or_default())
                .bind(body.date_time)
                .bind(body.speaker.unwrap_or_default())
                .bind(body.description.unwrap_or_default())
                .bind(body.auto_upload_enabled.unwrap_or(false))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
                if updated_id.is_none() {
                    tx.rollback().await?;
                    return Ok(None);
                }
                if let Some(conns) = &body.connections {
                    for conn in conns {
                        if let Some(ps) = &conn.privacy_status {
                            sqlx::query(
                                "UPDATE event_connections SET privacy_status=$1, updated_at=NOW() \
                                 WHERE event_id=$2 AND platform=$3",
                            )
                            .bind(ps)
                            .bind(id)
                            .bind(&conn.platform)
                            .execute(&mut *tx)
                            .await?;
                        }
                    }
                }
                ws_upsert_bible_references(id, &body.bible_references, &mut tx).await?;
                tx.commit().await?;
                fetch_event(id, &state.pool).await
            }
            .await;
            match result {
                Ok(Some(event)) => {
                    let msg = json!({ "type": "events.update", "event": event }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                    broadcast_event_changed(state, "UPDATE", &event).await;
                    spawn_scheduling_tasks(event, state.clone());
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::EventsDelete { id } => {
            let event = match fetch_event(id, &state.pool).await {
                Ok(Some(e)) => e,
                Ok(None) => {
                    ws_error(client_tx, "not_found");
                    return;
                }
                Err(e) => {
                    ws_error(client_tx, &e.to_string());
                    return;
                }
            };
            let result = sqlx::query("DELETE FROM events WHERE id = $1 RETURNING id")
                .bind(id)
                .fetch_optional(&state.pool)
                .await;
            match result {
                Ok(Some(_)) => {
                    crate::queue::enqueue_youtube_delete(&state.pool, &event).await;
                    broadcast_event_changed(state, "DELETE", &event).await;
                    ws_ok(client_tx);
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── Recordings ───────────────────────────────────────────────────────
        WsCommand::RecordingsList { event_id } => {
            let mut recordings = match sqlx::query_as::<_, crate::models::recording::Recording>(
                "SELECT * FROM recordings WHERE event_id = $1 ORDER BY detected_at DESC",
            )
            .bind(event_id)
            .fetch_all(&state.pool)
            .await
            {
                Ok(r) => r,
                Err(e) => {
                    ws_error(client_tx, &e.to_string());
                    return;
                }
            };
            if !recordings.is_empty() {
                let ids: Vec<Uuid> = recordings.iter().map(|r| r.id).collect();
                let uploads = sqlx::query_as::<_, RecordingUpload>(
                    "SELECT recording_id, platform, state, progress_bytes, total_bytes, \
                     visibility, video_id, video_url, error, started_at, completed_at, updated_at \
                     FROM recording_uploads WHERE recording_id = ANY($1)",
                )
                .bind(&ids)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
                for rec in &mut recordings {
                    rec.uploads = uploads
                        .iter()
                        .filter(|u| u.recording_id == rec.id)
                        .cloned()
                        .collect();
                }
            }
            let msg = json!({ "type": "recordings.list", "recordings": recordings }).to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::RecordingsListAll { filter } => {
            let where_clause = match filter.as_deref().unwrap_or("") {
                "not_flagged" => {
                    "r.uploadable = false AND NOT EXISTS (SELECT 1 FROM recording_uploads WHERE recording_id = r.id)"
                }
                "flagged" => {
                    "r.uploadable = true AND NOT EXISTS (SELECT 1 FROM recording_uploads WHERE recording_id = r.id AND state IN ('uploading','paused','completed','failed'))"
                }
                "in_progress" => {
                    "EXISTS (SELECT 1 FROM recording_uploads WHERE recording_id = r.id AND state IN ('uploading','paused','failed'))"
                }
                "uploaded" => {
                    "EXISTS (SELECT 1 FROM recording_uploads WHERE recording_id = r.id AND state = 'completed')"
                }
                _ => "true",
            };
            let sql = format!(
                "SELECT r.*, e.title AS _event_title FROM recordings r \
                 JOIN events e ON e.id = r.event_id WHERE {where_clause} \
                 ORDER BY r.detected_at DESC LIMIT 100"
            );
            #[derive(sqlx::FromRow)]
            struct RecordingRow {
                #[sqlx(flatten)]
                recording: crate::models::recording::Recording,
                _event_title: String,
            }
            #[derive(serde::Serialize)]
            #[serde(rename_all = "camelCase")]
            struct RecordingWithEvent {
                #[serde(flatten)]
                recording: crate::models::recording::Recording,
                event_title: String,
            }
            let rows = match sqlx::query_as::<_, RecordingRow>(&sql)
                .fetch_all(&state.pool)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    ws_error(client_tx, &e.to_string());
                    return;
                }
            };
            let mut results: Vec<RecordingWithEvent> = rows
                .into_iter()
                .map(|row| RecordingWithEvent {
                    event_title: row._event_title.clone(),
                    recording: row.recording,
                })
                .collect();
            if !results.is_empty() {
                let ids: Vec<Uuid> = results.iter().map(|r| r.recording.id).collect();
                let uploads = sqlx::query_as::<_, RecordingUpload>(
                    "SELECT recording_id, platform, state, progress_bytes, total_bytes, \
                     visibility, video_id, video_url, error, started_at, completed_at, updated_at \
                     FROM recording_uploads WHERE recording_id = ANY($1)",
                )
                .bind(&ids)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
                for item in &mut results {
                    item.recording.uploads = uploads
                        .iter()
                        .filter(|u| u.recording_id == item.recording.id)
                        .cloned()
                        .collect();
                }
            }
            let msg = json!({ "type": "recordings.list_all", "recordings": results }).to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::RecordingsCreate {
            event_id,
            file_path,
            file_name,
            file_size,
            duration_seconds,
            custom_title,
            custom_description,
        } => {
            let body = CreateRecording {
                file_path,
                file_name,
                file_size,
                duration_seconds,
                custom_title,
                custom_description,
            };
            let result = sqlx::query_as::<_, crate::models::recording::Recording>(
                "INSERT INTO recordings (event_id, file_path, file_name, file_size, duration_seconds, \
                 custom_title, custom_description, detected_at) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *",
            )
            .bind(event_id)
            .bind(&body.file_path)
            .bind(&body.file_name)
            .bind(body.file_size.unwrap_or(0))
            .bind(body.duration_seconds.unwrap_or(0.0))
            .bind(body.custom_title.as_deref())
            .bind(body.custom_description.as_deref())
            .bind(Utc::now())
            .fetch_one(&state.pool)
            .await;
            match result {
                Ok(recording) => {
                    let msg =
                        json!({ "type": "recordings.create", "recording": recording }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::RecordingsDelete {
            event_id,
            recording_id,
            delete_file,
        } => {
            let row = sqlx::query_as::<_, crate::models::recording::Recording>(
                "SELECT * FROM recordings WHERE id = $1 AND event_id = $2",
            )
            .bind(recording_id)
            .bind(event_id)
            .fetch_optional(&state.pool)
            .await;
            match row {
                Ok(Some(rec)) => {
                    let del = sqlx::query("DELETE FROM recordings WHERE id = $1")
                        .bind(recording_id)
                        .execute(&state.pool)
                        .await;
                    match del {
                        Ok(_) => {
                            if delete_file.unwrap_or(false) {
                                let _ = tokio::fs::remove_file(&rec.file_path).await;
                            }
                            ws_ok(client_tx);
                        }
                        Err(e) => ws_error(client_tx, &e.to_string()),
                    }
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::RecordingsFlagUpload {
            event_id,
            recordings,
        } => {
            let result: anyhow::Result<()> = async {
                for item in &recordings {
                    sqlx::query(
                        "UPDATE recordings SET uploadable=true, \
                         custom_title=COALESCE($1, custom_title), \
                         custom_description=COALESCE($2, custom_description), \
                         updated_at=NOW() WHERE id=$3 AND event_id=$4",
                    )
                    .bind(item.custom_title.as_deref())
                    .bind(item.custom_description.as_deref())
                    .bind(item.recording_id)
                    .bind(event_id)
                    .execute(&state.pool)
                    .await?;
                    for platform in &item.platforms {
                        let visibility = if platform == "youtube" {
                            item.youtube_visibility.as_deref().unwrap_or("private").to_string()
                        } else {
                            item.facebook_visibility.as_deref().unwrap_or("ONLY_ME").to_string()
                        };
                        sqlx::query(
                            "INSERT INTO recording_uploads (recording_id, platform, state, visibility, updated_at) \
                             VALUES ($1, $2, 'pending', $3, NOW()) \
                             ON CONFLICT (recording_id, platform) DO UPDATE SET \
                                 state = CASE WHEN recording_uploads.state = 'completed' \
                                              THEN 'completed' ELSE 'pending' END, \
                                 visibility = EXCLUDED.visibility, updated_at = NOW()",
                        )
                        .bind(item.recording_id)
                        .bind(platform)
                        .bind(&visibility)
                        .execute(&state.pool)
                        .await?;
                    }
                }
                Ok(())
            }
            .await;
            match result {
                Ok(()) => ws_ok(client_tx),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── Untracked recordings ─────────────────────────────────────────────
        WsCommand::RecordingsUntrackedList => {
            match untracked_recording::list_untracked(&state.pool).await {
                Ok(recordings) => {
                    let msg =
                        json!({ "type": "recordings.untracked.list", "recordings": recordings })
                            .to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::RecordingsUntrackedAssign { id, event_id } => {
            let result: anyhow::Result<crate::models::recording::Recording> = async {
                let untracked = sqlx::query_as::<_, untracked_recording::UntrackedRecording>(
                    "SELECT * FROM untracked_recordings WHERE id = $1",
                )
                .bind(id)
                .fetch_optional(&state.pool)
                .await?
                .ok_or_else(|| anyhow::anyhow!("NOT_FOUND"))?;
                let event_exists: bool =
                    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE id = $1)")
                        .bind(event_id)
                        .fetch_one(&state.pool)
                        .await?;
                if !event_exists {
                    return Err(anyhow::anyhow!("EVENT_NOT_FOUND"));
                }
                let mut tx = state.pool.begin().await?;
                let recording = sqlx::query_as::<_, crate::models::recording::Recording>(
                    "INSERT INTO recordings (event_id, file_path, file_name, file_size, duration_seconds, detected_at) \
                     VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
                )
                .bind(event_id)
                .bind(&untracked.file_path)
                .bind(&untracked.file_name)
                .bind(untracked.file_size)
                .bind(untracked.duration_seconds)
                .bind(untracked.detected_at)
                .fetch_one(&mut *tx)
                .await?;
                sqlx::query("DELETE FROM untracked_recordings WHERE id = $1")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                Ok(recording)
            }
            .await;
            match result {
                Ok(recording) => {
                    let clients = state.ws_clients.clone();
                    tokio::spawn(async move {
                        broadcast_untracked_removed(&clients, id).await;
                    });
                    let msg =
                        json!({ "type": "recordings.untracked.assign", "recording": recording })
                            .to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) if e.to_string() == "NOT_FOUND" => ws_error(client_tx, "not_found"),
                Err(e) if e.to_string() == "EVENT_NOT_FOUND" => {
                    ws_error(client_tx, "event_not_found")
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::RecordingsUntrackedDelete { id, delete_file } => {
            let row = sqlx::query_as::<_, untracked_recording::UntrackedRecording>(
                "SELECT * FROM untracked_recordings WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(&state.pool)
            .await;
            match row {
                Ok(Some(rec)) => {
                    let del = sqlx::query("DELETE FROM untracked_recordings WHERE id = $1")
                        .bind(id)
                        .execute(&state.pool)
                        .await;
                    match del {
                        Ok(_) => {
                            if delete_file.unwrap_or(false) {
                                let _ = tokio::fs::remove_file(&rec.file_path).await;
                            }
                            let clients = state.ws_clients.clone();
                            tokio::spawn(async move {
                                broadcast_untracked_removed(&clients, id).await;
                            });
                            ws_ok(client_tx);
                        }
                        Err(e) => ws_error(client_tx, &e.to_string()),
                    }
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── Activities ───────────────────────────────────────────────────────
        WsCommand::ActivitiesList { event_id } => {
            match activity::list_activities(event_id, &state.pool).await {
                Ok(activities) => {
                    let msg =
                        json!({ "type": "activities.list", "activities": activities }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::ActivitiesCreate {
            event_id,
            activity_type,
            message,
        } => {
            let event_exists: Result<bool, _> =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE id = $1)")
                    .bind(event_id)
                    .fetch_one(&state.pool)
                    .await;
            match event_exists {
                Ok(false) | Err(_) => {
                    ws_error(client_tx, "event_not_found");
                    return;
                }
                Ok(true) => {}
            }
            let result = sqlx::query_as::<_, activity::EventActivity>(
                "INSERT INTO event_activities (event_id, activity_type, message) VALUES ($1, $2, $3) RETURNING *",
            )
            .bind(event_id)
            .bind(&activity_type)
            .bind(&message)
            .fetch_one(&state.pool)
            .await;
            match result {
                Ok(act) => {
                    let msg = json!({ "type": "activities.create", "activity": act }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::ActivitiesDelete {
            event_id,
            activity_id,
        } => {
            let result = sqlx::query(
                "DELETE FROM event_activities WHERE id = $1 AND event_id = $2 RETURNING id",
            )
            .bind(activity_id)
            .bind(event_id)
            .fetch_optional(&state.pool)
            .await;
            match result {
                Ok(Some(_)) => ws_ok(client_tx),
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── Cron jobs ────────────────────────────────────────────────────────
        WsCommand::CronJobsList => match cron_job::list_all(&state.pool).await {
            Ok(jobs) => {
                let msg = json!({ "type": "cron_jobs.list", "jobs": jobs }).to_string();
                let _ = client_tx.send(Message::Text(msg.into()));
            }
            Err(e) => ws_error(client_tx, &e.to_string()),
        },
        WsCommand::CronJobsCreate {
            name,
            cron_expression,
            enabled,
            pull_youtube,
            auto_upload,
        } => {
            if tokio_cron_scheduler::Job::new_async(cron_expression.as_str(), |_, _| {
                Box::pin(async {})
            })
            .is_err()
            {
                ws_error(client_tx, "invalid_cron_expression");
                return;
            }
            let body = CreateCronJob {
                name,
                cron_expression,
                enabled,
                pull_youtube,
                auto_upload,
            };
            let result: anyhow::Result<cron_job::CronJob> = async {
                let mut tx = state.pool.begin().await?;
                let row = sqlx::query_as::<
                    _,
                    (
                        Uuid,
                        String,
                        String,
                        bool,
                        chrono::DateTime<Utc>,
                        chrono::DateTime<Utc>,
                    ),
                >(
                    "INSERT INTO cron_jobs (name, cron_expression, enabled) \
                     VALUES ($1, $2, $3) \
                     RETURNING id, name, cron_expression, enabled, created_at, updated_at",
                )
                .bind(&body.name)
                .bind(&body.cron_expression)
                .bind(body.enabled)
                .fetch_one(&mut *tx)
                .await?;
                cron_job::sync_features(&mut tx, row.0, body.pull_youtube, body.auto_upload)
                    .await?;
                tx.commit().await?;
                Ok(cron_job::CronJob {
                    id: row.0,
                    name: row.1,
                    cron_expression: row.2,
                    enabled: row.3,
                    pull_youtube: body.pull_youtube,
                    auto_upload: body.auto_upload,
                    created_at: row.4,
                    updated_at: row.5,
                })
            }
            .await;
            match result {
                Ok(job) => {
                    let pool = state.pool.clone();
                    let clients = state.ws_clients.clone();
                    let yt = state.youtube_connector.clone();
                    let sched = state.cron_scheduler.clone();
                    let us = state.upload_service.clone();
                    tokio::spawn(async move {
                        sched.reload(pool, clients, yt, us).await;
                    });
                    let msg = json!({ "type": "cron_jobs.create", "job": job }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::CronJobsUpdate {
            id,
            name,
            cron_expression,
            enabled,
            pull_youtube,
            auto_upload,
        } => {
            if tokio_cron_scheduler::Job::new_async(cron_expression.as_str(), |_, _| {
                Box::pin(async {})
            })
            .is_err()
            {
                ws_error(client_tx, "invalid_cron_expression");
                return;
            }
            let body = UpdateCronJob {
                name,
                cron_expression,
                enabled,
                pull_youtube,
                auto_upload,
            };
            let result: anyhow::Result<Option<cron_job::CronJob>> = async {
                let mut tx = state.pool.begin().await?;
                let row = sqlx::query_as::<_, (Uuid, String, String, bool, chrono::DateTime<Utc>, chrono::DateTime<Utc>)>(
                    "UPDATE cron_jobs SET name=$1, cron_expression=$2, enabled=$3, updated_at=NOW() \
                     WHERE id=$4 RETURNING id, name, cron_expression, enabled, created_at, updated_at",
                )
                .bind(&body.name)
                .bind(&body.cron_expression)
                .bind(body.enabled)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
                let Some(row) = row else {
                    tx.rollback().await?;
                    return Ok(None);
                };
                cron_job::sync_features(&mut tx, row.0, body.pull_youtube, body.auto_upload).await?;
                tx.commit().await?;
                Ok(Some(cron_job::CronJob {
                    id: row.0,
                    name: row.1,
                    cron_expression: row.2,
                    enabled: row.3,
                    pull_youtube: body.pull_youtube,
                    auto_upload: body.auto_upload,
                    created_at: row.4,
                    updated_at: row.5,
                }))
            }
            .await;
            match result {
                Ok(Some(job)) => {
                    let pool = state.pool.clone();
                    let clients = state.ws_clients.clone();
                    let yt = state.youtube_connector.clone();
                    let sched = state.cron_scheduler.clone();
                    let us = state.upload_service.clone();
                    tokio::spawn(async move {
                        sched.reload(pool, clients, yt, us).await;
                    });
                    let msg = json!({ "type": "cron_jobs.update", "job": job }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::CronJobsDelete { id } => {
            let result = sqlx::query("DELETE FROM cron_jobs WHERE id = $1 RETURNING id")
                .bind(id)
                .fetch_optional(&state.pool)
                .await;
            match result {
                Ok(Some(_)) => {
                    let pool = state.pool.clone();
                    let clients = state.ws_clients.clone();
                    let yt = state.youtube_connector.clone();
                    let sched = state.cron_scheduler.clone();
                    let us = state.upload_service.clone();
                    tokio::spawn(async move {
                        sched.reload(pool, clients, yt, us).await;
                    });
                    ws_ok(client_tx);
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── Uploads ──────────────────────────────────────────────────────────
        WsCommand::UploadsTrigger => {
            let us = state.upload_service.clone();
            tokio::spawn(async move {
                us.run_cycle().await;
            });
            ws_ok(client_tx);
        }
        // ── Connectors ───────────────────────────────────────────────────────
        WsCommand::ConnectorsStatus => {
            let obs = state.obs_connector.get_status().await;
            let vmix = state.vmix_connector.get_status();
            let yt = state.youtube_connector.get_status().await;
            let fb = state.facebook_connector.get_status().await;
            let broadlink = state.broadlink_connector.get_status().await;
            let blackmagic_camera = state.blackmagic_camera_connector.get_status().await;
            let middlecontrol = state.middlecontrol_connector.get_status().await;
            let rodecaster = state.rodecaster_connector.get_status();
            let atem = state.atem_connector.get_status().await;
            let msg = json!({
                "type": "connectors.status",
                "atem": atem,
                "obs": obs,
                "vmix": vmix,
                "broadlink": broadlink,
                "youtube": yt,
                "facebook": fb,
                "blackmagic-camera": blackmagic_camera,
                "middlecontrol": middlecontrol,
                "rodecaster": rodecaster,
            })
            .to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::ConnectorsState => {
            let obs_output = state.obs_connector.get_output_state().await;
            let camera = state.blackmagic_camera_connector.get_state().await;
            let middlecontrol = state.middlecontrol_connector.get_state().await;
            let msg = json!({
                "type": "connectors.state",
                "obs": obs_output.map(|s| json!({"isStreaming": s.is_streaming, "isRecording": s.is_recording})),
                "blackmagic-camera": camera.map(|s| json!({"isStreaming": s.is_streaming(), "streamStatus": s.stream_status, "isRecording": s.is_recording})),
                "middlecontrol": middlecontrol,
                "atem": state.atem_connector.get_state().await,
            })
            .to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::ConnectorsYoutubeSchedule { event_id } => {
            let event = match fetch_event(event_id, &state.pool).await {
                Ok(Some(e)) => e,
                Ok(None) => {
                    ws_error(client_tx, "not_found");
                    return;
                }
                Err(e) => {
                    ws_error(client_tx, &e.to_string());
                    return;
                }
            };
            let token = match youtube::load_tokens(&state.pool).await {
                Some(t) => t,
                None => {
                    ws_error(client_tx, "not_authenticated");
                    return;
                }
            };
            let yt_conn = event.connection("youtube");
            let existing_id = yt_conn.and_then(|c| c.external_id.as_deref());
            let privacy_status = yt_conn
                .and_then(|c| c.privacy_status.as_deref())
                .unwrap_or("private");
            match youtube::schedule_event(
                &event.id.to_string(),
                event.published_title(),
                &event.date_time,
                &token.access_token,
                existing_id,
                privacy_status,
            )
            .await
            {
                Ok(result) => match write_youtube_result(state, event_id, &result).await {
                    Ok(updated) => {
                        broadcast_event_changed(state, "UPDATE", &updated).await;
                        ws_ok(client_tx);
                    }
                    Err(e) => ws_error(client_tx, &e.to_string()),
                },
                Err(e) => {
                    let _ = write_youtube_status(&state.pool, event_id, "failed").await;
                    ws_error(client_tx, &e.to_string());
                }
            }
        }
        WsCommand::ConnectorsFacebookSchedule { event_id } => {
            let event = match fetch_event(event_id, &state.pool).await {
                Ok(Some(e)) => e,
                Ok(None) => {
                    ws_error(client_tx, "not_found");
                    return;
                }
                Err(e) => {
                    ws_error(client_tx, &e.to_string());
                    return;
                }
            };
            let token = match facebook::load_tokens(&state.pool).await {
                Some(t) => t,
                None => {
                    ws_error(client_tx, "not_authenticated");
                    return;
                }
            };
            let config = state.facebook_config.read().await.clone();
            if config.page_id.is_empty() {
                ws_error(client_tx, "facebook_page_id_not_configured");
                return;
            }
            let fb_conn = event.connection("facebook");
            let privacy_status = fb_conn
                .and_then(|c| c.privacy_status.as_deref())
                .unwrap_or("EVERYONE");
            match facebook::schedule_event(
                &event.title,
                &event.date_time,
                &token.access_token,
                &config.page_id,
                privacy_status,
            )
            .await
            {
                Ok(result) => match write_facebook_result(state, event_id, &result).await {
                    Ok(updated) => {
                        broadcast_event_changed(state, "UPDATE", &updated).await;
                        ws_ok(client_tx);
                    }
                    Err(e) => ws_error(client_tx, &e.to_string()),
                },
                Err(e) => {
                    let _ = write_facebook_status(&state.pool, event_id, "failed").await;
                    ws_error(client_tx, &e.to_string());
                }
            }
        }
        WsCommand::ConnectorsYoutubeStreamKey => {
            let token = match youtube::load_tokens(&state.pool).await {
                Some(t) => t,
                None => {
                    ws_error(client_tx, "not_authenticated");
                    return;
                }
            };
            match youtube::live_stream_ingestion(&token.access_token).await {
                Ok(ingestion) => {
                    let msg = json!({ "type": "connectors.youtube.stream_key", "rtmpUrl": ingestion.rtmp_url() }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::ConnectorsFacebookStreamKey => {
            let token = match facebook::load_tokens(&state.pool).await {
                Some(t) => t,
                None => {
                    ws_error(client_tx, "not_authenticated");
                    return;
                }
            };
            let config = state.facebook_config.read().await.clone();
            if config.page_id.is_empty() {
                ws_error(client_tx, "facebook_page_id_not_configured");
                return;
            }
            #[derive(serde::Deserialize)]
            struct FbLiveVideo {
                #[serde(rename = "stream_url")]
                stream_url: Option<String>,
                #[serde(rename = "secure_stream_url")]
                secure_stream_url: Option<String>,
            }
            #[derive(serde::Deserialize)]
            struct FbList {
                data: Vec<FbLiveVideo>,
            }
            let client = reqwest::Client::new();
            let resp = client
                .get(format!(
                    "https://graph.facebook.com/v19.0/{}/live_videos",
                    config.page_id
                ))
                .query(&[
                    ("status", "SCHEDULED_UNPUBLISHED"),
                    ("fields", "stream_url,secure_stream_url"),
                    ("access_token", token.access_token.as_str()),
                ])
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => match r.json::<FbList>().await {
                    Ok(list) => match list.data.into_iter().next() {
                        Some(video) => {
                            let rtmp_url = video
                                .secure_stream_url
                                .or(video.stream_url)
                                .unwrap_or_default();
                            let msg = json!({ "type": "connectors.facebook.stream_key", "rtmpUrl": rtmp_url }).to_string();
                            let _ = client_tx.send(Message::Text(msg.into()));
                        }
                        None => ws_error(client_tx, "no_live_video_found"),
                    },
                    Err(e) => ws_error(client_tx, &e.to_string()),
                },
                Ok(r) => ws_error(client_tx, &format!("facebook_api_{}", r.status())),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::ConnectorsYoutubeContent => {
            let config = state.youtube_config.read().await.clone();
            match youtube::fetch_channel_content(&state.pool, &config).await {
                Ok(content) => {
                    let msg = json!({ "type": "connectors.youtube.content", "content": content })
                        .to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── Auth ─────────────────────────────────────────────────────────────
        WsCommand::AuthYoutubeUrl => {
            let config = state.youtube_config.read().await.clone();
            if config.client_id.is_empty() {
                ws_error(client_tx, "youtube_not_configured");
                return;
            }
            let state_token = Uuid::new_v4().to_string();
            {
                let mut states = state.oauth_states.write().await;
                states.insert(
                    state_token.clone(),
                    ("youtube".to_string(), std::time::Instant::now()),
                );
            }
            let url = format!(
                "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=https://www.googleapis.com/auth/youtube&access_type=offline&prompt=consent&state={}",
                urlencoding::encode(&config.client_id),
                urlencoding::encode(crate::server::OAUTH_REDIRECT_URI),
                urlencoding::encode(&state_token),
            );
            let msg = json!({ "type": "auth.youtube.url", "url": url }).to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::AuthYoutubeLogout => match youtube::delete_tokens(&state.pool).await {
            Ok(_) => {
                state.youtube_connector.stop().await;
                ws_ok(client_tx);
            }
            Err(e) => ws_error(client_tx, &e.to_string()),
        },
        WsCommand::AuthFacebookUrl => {
            let config = state.facebook_config.read().await.clone();
            if config.app_id.is_empty() {
                ws_error(client_tx, "facebook_not_configured");
                return;
            }
            let state_token = Uuid::new_v4().to_string();
            {
                let mut states = state.oauth_states.write().await;
                states.insert(
                    state_token.clone(),
                    ("facebook".to_string(), std::time::Instant::now()),
                );
            }
            let url = format!(
                "https://www.facebook.com/v19.0/dialog/oauth?client_id={}&redirect_uri={}&scope=pages_manage_posts,pages_read_engagement,publish_video&state={}",
                urlencoding::encode(&config.app_id),
                urlencoding::encode(crate::server::OAUTH_REDIRECT_URI),
                urlencoding::encode(&state_token),
            );
            let msg = json!({ "type": "auth.facebook.url", "url": url }).to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::AuthFacebookLogout => match facebook::delete_tokens(&state.pool).await {
            Ok(_) => {
                state.facebook_connector.stop().await;
                ws_ok(client_tx);
            }
            Err(e) => ws_error(client_tx, &e.to_string()),
        },
        // ── Broadlink ────────────────────────────────────────────────────────
        WsCommand::BroadlinkStatus => {
            let status = state.broadlink_connector.get_status().await;
            let msg = json!({ "type": "broadlink.status", "status": status }).to_string();
            let _ = client_tx.send(Message::Text(msg.into()));
        }
        WsCommand::BroadlinkDevicesList => {
            let rows = sqlx::query_as::<_, (Uuid, String, String, Option<String>, String, String, bool)>(
                "SELECT id, name, device_type, model, host, mac, is_default FROM broadlink_devices ORDER BY created_at",
            )
            .fetch_all(&state.pool)
            .await;
            match rows {
                Ok(devices) => {
                    let list: Vec<serde_json::Value> = devices
                        .into_iter()
                        .map(|(id, name, device_type, model, host, mac, is_default)| {
                            json!({ "id": id, "name": name, "deviceType": device_type, "model": model, "host": host, "mac": mac, "isDefault": is_default })
                        })
                        .collect();
                    let msg =
                        json!({ "type": "broadlink.devices.list", "devices": list }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BroadlinkDevicesAdd {
            name,
            host,
            mac,
            device_type,
            model,
        } => {
            let result = sqlx::query_as::<_, (Uuid,)>(
                "INSERT INTO broadlink_devices (name, device_type, model, host, mac) \
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
            )
            .bind(&name)
            .bind(&device_type)
            .bind(&model)
            .bind(&host)
            .bind(&mac)
            .fetch_one(&state.pool)
            .await;
            match result {
                Ok((id,)) => {
                    state
                        .broadlink_connector
                        .set_status(ConnectorStatus::Connected)
                        .await;
                    let device = json!({ "id": id, "name": name, "deviceType": device_type, "model": model, "host": host, "mac": mac, "isDefault": false });
                    let msg =
                        json!({ "type": "broadlink.devices.add", "device": device }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BroadlinkDevicesRemove { id } => {
            let result = sqlx::query("DELETE FROM broadlink_devices WHERE id = $1 RETURNING id")
                .bind(id)
                .fetch_optional(&state.pool)
                .await;
            match result {
                Ok(Some(_)) => ws_ok(client_tx),
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BroadlinkDiscover => {
            let pool = state.pool.clone();
            let connector = state.broadlink_connector.clone();
            tokio::spawn(async move {
                match crate::broadlink::discover_devices(5).await {
                    Ok(devices) => {
                        for dev in &devices {
                            let _ = sqlx::query(
                                "INSERT INTO broadlink_devices (name, device_type, model, host, mac, last_seen_at) \
                                 VALUES ($1, $2, $3, $4, $5, NOW()) \
                                 ON CONFLICT (mac) DO UPDATE SET host = EXCLUDED.host, last_seen_at = NOW()",
                            )
                            .bind(&dev.name)
                            .bind(&dev.device_type)
                            .bind(&dev.model)
                            .bind(&dev.host)
                            .bind(&dev.mac)
                            .execute(&pool)
                            .await;
                        }
                        let count: i64 =
                            sqlx::query_scalar("SELECT COUNT(*) FROM broadlink_devices")
                                .fetch_one(&pool)
                                .await
                                .unwrap_or(0);
                        let new_status = if count > 0 {
                            ConnectorStatus::Connected
                        } else {
                            ConnectorStatus::Disconnected
                        };
                        connector.set_status(new_status).await;
                    }
                    Err(e) => tracing::error!("BroadlinkDiscover WS: {e}"),
                }
            });
            ws_ok(client_tx);
        }
        WsCommand::BroadlinkCommandsList {
            device_id,
            category,
        } => {
            let rows = if let Some(did) = device_id {
                if let Some(cat) = category {
                    sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, String, String, String)>(
                        "SELECT id, device_id, name, slug, code, code_type, category \
                         FROM broadlink_commands WHERE device_id=$1 AND category=$2 ORDER BY created_at",
                    )
                    .bind(did)
                    .bind(cat)
                    .fetch_all(&state.pool)
                    .await
                } else {
                    sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, String, String, String)>(
                        "SELECT id, device_id, name, slug, code, code_type, category \
                         FROM broadlink_commands WHERE device_id=$1 ORDER BY created_at",
                    )
                    .bind(did)
                    .fetch_all(&state.pool)
                    .await
                }
            } else {
                sqlx::query_as::<_, (Uuid, Option<Uuid>, String, String, String, String, String)>(
                    "SELECT id, device_id, name, slug, code, code_type, category \
                     FROM broadlink_commands ORDER BY created_at",
                )
                .fetch_all(&state.pool)
                .await
            };
            match rows {
                Ok(commands) => {
                    let list: Vec<serde_json::Value> = commands
                        .into_iter()
                        .map(|(id, did, name, slug, code, code_type, category)| {
                            json!({ "id": id, "deviceId": did, "name": name, "slug": slug, "code": code, "codeType": code_type, "category": category })
                        })
                        .collect();
                    let msg =
                        json!({ "type": "broadlink.commands.list", "commands": list }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BroadlinkCommandsAdd {
            device_id,
            name,
            slug,
            code,
            code_type,
            category,
        } => {
            let cat = category.unwrap_or_else(|| "other".to_string());
            let result = sqlx::query_as::<_, (Uuid,)>(
                "INSERT INTO broadlink_commands (device_id, name, slug, code, code_type, category) \
                 VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            )
            .bind(device_id)
            .bind(&name)
            .bind(&slug)
            .bind(&code)
            .bind(&code_type)
            .bind(&cat)
            .fetch_one(&state.pool)
            .await;
            match result {
                Ok((id,)) => {
                    let cmd = json!({ "id": id, "deviceId": device_id, "name": name, "slug": slug, "code": code, "codeType": code_type, "category": cat });
                    let msg =
                        json!({ "type": "broadlink.commands.add", "command": cmd }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BroadlinkCommandsUpdate {
            id,
            name,
            slug,
            code,
            code_type,
            category,
        } => {
            let result = sqlx::query(
                "UPDATE broadlink_commands SET \
                 name=COALESCE($2,name), slug=COALESCE($3,slug), code=COALESCE($4,code), \
                 code_type=COALESCE($5,code_type), category=COALESCE($6,category), updated_at=NOW() \
                 WHERE id=$1 RETURNING id, device_id, name, slug, code, code_type, category",
            )
            .bind(id)
            .bind(&name)
            .bind(&slug)
            .bind(&code)
            .bind(&code_type)
            .bind(&category)
            .fetch_optional(&state.pool)
            .await;
            match result {
                Ok(Some(row)) => {
                    let cmd = json!({
                        "id": row.get::<Uuid, _>("id"),
                        "deviceId": row.get::<Option<Uuid>, _>("device_id"),
                        "name": row.get::<String, _>("name"),
                        "slug": row.get::<String, _>("slug"),
                        "code": row.get::<String, _>("code"),
                        "codeType": row.get::<String, _>("code_type"),
                        "category": row.get::<String, _>("category"),
                    });
                    let msg =
                        json!({ "type": "broadlink.commands.update", "command": cmd }).to_string();
                    let _ = client_tx.send(Message::Text(msg.into()));
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BroadlinkCommandsRemove { id } => {
            let result = sqlx::query("DELETE FROM broadlink_commands WHERE id=$1 RETURNING id")
                .bind(id)
                .fetch_optional(&state.pool)
                .await;
            match result {
                Ok(Some(_)) => ws_ok(client_tx),
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BroadlinkLearnStart {
            device_id,
            signal_type,
        } => {
            if state
                .broadlink_learn_active
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                ws_error(client_tx, "learn_already_active");
                return;
            }
            let sig_type = signal_type.unwrap_or_else(|| "ir".to_string());
            let device = sqlx::query_as::<_, (String, String, String)>(
                "SELECT host, mac, device_type FROM broadlink_devices WHERE id = $1",
            )
            .bind(device_id)
            .fetch_optional(&state.pool)
            .await;
            let (host, mac, devtype) = match device {
                Ok(Some(row)) => row,
                Ok(None) => {
                    state.broadlink_learn_active.store(false, Ordering::SeqCst);
                    ws_error(client_tx, "device_not_found");
                    return;
                }
                Err(e) => {
                    state.broadlink_learn_active.store(false, Ordering::SeqCst);
                    ws_error(client_tx, &e.to_string());
                    return;
                }
            };
            let learn_active = state.broadlink_learn_active.clone();
            let learn_tx = state.broadlink_connector.learn_tx.clone();
            tokio::spawn(async move {
                let result = crate::broadlink::learn_code(&host, &mac, &devtype, &sig_type).await;
                let event = match result {
                    Ok(lr) => crate::connectors::broadlink::BroadlinkLearnEvent {
                        code: lr.code,
                        error: lr.error,
                    },
                    Err(e) => crate::connectors::broadlink::BroadlinkLearnEvent {
                        code: None,
                        error: Some(e),
                    },
                };
                let _ = learn_tx.send(event);
                learn_active.store(false, Ordering::SeqCst);
            });
            ws_ok(client_tx);
        }
        WsCommand::BroadlinkLearnCancel => {
            crate::broadlink::cancel_learn().await;
            state.broadlink_learn_active.store(false, Ordering::SeqCst);
            ws_ok(client_tx);
        }
        WsCommand::BroadlinkCommandsSend { id } => {
            let row = sqlx::query_as::<_, (String, String, String, String)>(
                "SELECT bc.code, bd.host, bd.mac, bd.device_type \
                 FROM broadlink_commands bc \
                 JOIN broadlink_devices bd ON bc.device_id = bd.id \
                 WHERE bc.id = $1",
            )
            .bind(id)
            .fetch_optional(&state.pool)
            .await;
            let (code, host, mac, devtype) = match row {
                Ok(Some(r)) => r,
                Ok(None) => {
                    ws_error(client_tx, "not_found");
                    return;
                }
                Err(e) => {
                    ws_error(client_tx, &e.to_string());
                    return;
                }
            };
            match crate::broadlink::send_code(&host, &mac, &devtype, &code).await {
                Ok(r) if r.success => ws_ok(client_tx),
                Ok(r) => ws_error(client_tx, &r.error.unwrap_or_default()),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── OBS Devices ───────────────────────────────────────────────────────
        WsCommand::ObsDevicesScan => {
            let _ = state.obs_connector.devices_tx.send(());
            ws_ok(client_tx);
        }
        WsCommand::ObsDevicesAvailable => {
            let devices_guard = state.obs_available_devices.read().await;
            if let Some(scanned) = devices_guard.as_ref() {
                let listeners: Vec<DeviceListener> =
                    sqlx::query_as("SELECT * FROM device_listeners ORDER BY created_at")
                        .fetch_all(&state.pool)
                        .await
                        .unwrap_or_default();
                let statuses = crate::obs_devices::compute_listener_statuses(scanned, &listeners);
                let msg = serde_json::json!({
                    "type": "obs.devices.available",
                    "devices": scanned,
                    "listenerStatuses": statuses,
                })
                .to_string();
                let _ = client_tx.send(axum::extract::ws::Message::Text(msg.into()));
            } else {
                ws_error(client_tx, "no_scan_data");
            }
        }
        WsCommand::ObsListenersList => {
            let listeners: Vec<DeviceListener> =
                match sqlx::query_as("SELECT * FROM device_listeners ORDER BY created_at")
                    .fetch_all(&state.pool)
                    .await
                {
                    Ok(l) => l,
                    Err(e) => {
                        ws_error(client_tx, &e.to_string());
                        return;
                    }
                };
            let statuses = {
                let devices_guard = state.obs_available_devices.read().await;
                devices_guard
                    .as_ref()
                    .map(|d| crate::obs_devices::compute_listener_statuses(d, &listeners))
                    .unwrap_or_default()
            };
            let msg = serde_json::json!({
                "type": "obs.listeners.list",
                "listeners": listeners,
                "statuses": statuses,
            })
            .to_string();
            let _ = client_tx.send(axum::extract::ws::Message::Text(msg.into()));
        }
        WsCommand::ObsListenersCreate {
            connector_type,
            category,
            device_item_value,
            device_item_name,
            friendly_name,
        } => {
            let result = sqlx::query_as::<_, DeviceListener>(
                "INSERT INTO device_listeners \
                 (connector_type, category, device_item_value, device_item_name, friendly_name) \
                 VALUES ($1, $2, $3, $4, $5) RETURNING *",
            )
            .bind(&connector_type)
            .bind(&category)
            .bind(&device_item_value)
            .bind(&device_item_name)
            .bind(&friendly_name)
            .fetch_one(&state.pool)
            .await;
            match result {
                Ok(listener) => {
                    let broadcast_msg = serde_json::json!({
                        "type": "obs.listeners.create",
                        "listener": listener,
                    })
                    .to_string();
                    let clients = state.ws_clients.read().await;
                    for tx in clients.values() {
                        let _ = tx.send(axum::extract::ws::Message::Text(
                            broadcast_msg.clone().into(),
                        ));
                    }
                    ws_ok(client_tx);
                }
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::ObsListenersUpdate { id, friendly_name } => {
            let result = sqlx::query_as::<_, DeviceListener>(
                "UPDATE device_listeners SET friendly_name=$1, updated_at=NOW() \
                 WHERE id=$2 RETURNING *",
            )
            .bind(&friendly_name)
            .bind(id)
            .fetch_optional(&state.pool)
            .await;
            match result {
                Ok(Some(listener)) => {
                    let broadcast_msg = serde_json::json!({
                        "type": "obs.listeners.update",
                        "listener": listener,
                    })
                    .to_string();
                    let clients = state.ws_clients.read().await;
                    for tx in clients.values() {
                        let _ = tx.send(axum::extract::ws::Message::Text(
                            broadcast_msg.clone().into(),
                        ));
                    }
                    ws_ok(client_tx);
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::ObsListenersDelete { id } => {
            let result = sqlx::query("DELETE FROM device_listeners WHERE id=$1 RETURNING id")
                .bind(id)
                .fetch_optional(&state.pool)
                .await;
            match result {
                Ok(Some(_)) => {
                    let broadcast_msg = serde_json::json!({
                        "type": "obs.listeners.delete",
                        "id": id,
                    })
                    .to_string();
                    let clients = state.ws_clients.read().await;
                    for tx in clients.values() {
                        let _ = tx.send(axum::extract::ws::Message::Text(
                            broadcast_msg.clone().into(),
                        ));
                    }
                    ws_ok(client_tx);
                }
                Ok(None) => ws_error(client_tx, "not_found"),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        // ── Blackmagic camera ────────────────────────────────────────────────
        WsCommand::RodecasterProfile => match state.rodecaster_connector.get_profile() {
            Some(profile) => {
                let _ = client_tx.send(Message::Text(
                    json!({ "type": "rodecaster.profile", "profile": profile })
                        .to_string()
                        .into(),
                ));
            }
            None => ws_error(client_tx, "rodecaster_not_connected"),
        },
        WsCommand::RodecasterMuteSet { channel, mute } => {
            match state.rodecaster_connector.set_mute(channel, mute).await {
                Ok(()) => ws_ok(client_tx),
                Err(e) => ws_error(client_tx, &e),
            }
        }
        WsCommand::RodecasterAudioDiscover => {
            let Some(profile) = state.rodecaster_connector.get_profile() else {
                ws_error(client_tx, "rodecaster_not_connected");
                return;
            };
            let recorder = Arc::clone(&state.rodecaster_audio_recorder);
            match tokio::task::spawn_blocking(move || recorder.discover(&profile)).await {
                Ok(discovery) => {
                    let _ = client_tx.send(Message::Text(
                        json!({ "type": "rodecaster.audio.discovery", "discovery": discovery })
                            .to_string()
                            .into(),
                    ));
                }
                Err(error) => ws_error(client_tx, &format!("audio_discovery_failed: {error}")),
            }
        }
        WsCommand::RodecasterAudioRecordState => {
            let recorder_state = state.rodecaster_audio_recorder.state();
            let _ = client_tx.send(Message::Text(
                json!({ "type": "rodecaster.audio.record.state", "state": recorder_state })
                    .to_string()
                    .into(),
            ));
        }
        WsCommand::RodecasterAudioRecordStart { event_id } => {
            let event_available: Result<bool, sqlx::Error> = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM events e WHERE e.id = $1 AND NOT EXISTS(\
                 SELECT 1 FROM event_activities a WHERE a.event_id = e.id \
                 AND a.activity_type = 'completed'))",
            )
            .bind(event_id)
            .fetch_one(&state.pool)
            .await;
            match event_available {
                Ok(true) => {}
                Ok(false) => {
                    ws_error(client_tx, "recording_event_unavailable");
                    return;
                }
                Err(error) => {
                    ws_error(client_tx, &error.to_string());
                    return;
                }
            }
            let Some(profile) = state.rodecaster_connector.get_profile() else {
                ws_error(client_tx, "rodecaster_not_connected");
                return;
            };
            let config: RodecasterConfig =
                crate::database::settings::get_json(&state.pool, "rodecaster_config").await;
            let recorder = Arc::clone(&state.rodecaster_audio_recorder);
            match tokio::task::spawn_blocking(move || {
                recorder.start(event_id, &config.audio_recording, &profile)
            })
            .await
            {
                Ok(Ok(recorder_state)) => {
                    let _ = client_tx.send(Message::Text(
                        json!({ "type": "rodecaster.audio.record.state", "state": recorder_state })
                            .to_string()
                            .into(),
                    ));
                }
                Ok(Err(error)) => ws_error(client_tx, &error),
                Err(error) => ws_error(client_tx, &format!("recording_start_failed: {error}")),
            }
        }
        WsCommand::RodecasterAudioRecordStop => {
            let recorder_state = state.rodecaster_audio_recorder.state();
            if recorder_state.status == RecorderStatus::Idle {
                let _ = client_tx.send(Message::Text(
                    json!({ "type": "rodecaster.audio.record.state", "state": recorder_state })
                        .to_string()
                        .into(),
                ));
                return;
            }
            let Some(event_id) = recorder_state.event_id else {
                ws_error(client_tx, "recording_event_unavailable");
                return;
            };
            let recorder = Arc::clone(&state.rodecaster_audio_recorder);
            match tokio::task::spawn_blocking(move || recorder.stop()).await {
                Ok(Ok(files)) => {
                    match persist_audio_recordings(&state.pool, event_id, &files).await {
                        Ok(()) => ws_ok(client_tx),
                        Err(error) => {
                            ws_error(client_tx, &format!("recording_metadata_failed: {error}"))
                        }
                    }
                }
                Ok(Err(error)) => ws_error(client_tx, &error),
                Err(error) => ws_error(client_tx, &format!("recording_stop_failed: {error}")),
            }
        }
        WsCommand::BlackmagicCameraDiscover { timeout_secs } => {
            // Results reach every client as `blackmagic-camera.discovered`, and an
            // unconfigured camera is adopted and connected — see `discover_cameras`.
            let state = state.clone();
            let timeout = std::time::Duration::from_secs(timeout_secs.unwrap_or(5).clamp(1, 30));
            tokio::spawn(async move {
                crate::server::routes::discover_cameras(&state, timeout).await;
            });
            ws_ok(client_tx);
        }
        WsCommand::BlackmagicCameraRecordStart { clip_name } => {
            let Some(camera) = state.blackmagic_camera_connector.client().await else {
                ws_error(client_tx, "blackmagic_camera_not_connected");
                return;
            };
            match camera.start_recording(clip_name.as_deref()).await {
                Ok(()) => ws_ok(client_tx),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BlackmagicCameraRecordStop => {
            let Some(camera) = state.blackmagic_camera_connector.client().await else {
                ws_error(client_tx, "blackmagic_camera_not_connected");
                return;
            };
            match camera.stop_recording().await {
                Ok(()) => ws_ok(client_tx),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BlackmagicCameraStreamStart => {
            let Some(camera) = state.blackmagic_camera_connector.client().await else {
                ws_error(client_tx, "blackmagic_camera_not_connected");
                return;
            };
            match camera.livestream_start().await {
                Ok(()) => ws_ok(client_tx),
                Err(e) => ws_error(client_tx, &e.to_string()),
            }
        }
        WsCommand::BlackmagicCameraStreamStop => {
            let Some(camera) = state.blackmagic_camera_connector.client().await else {
                ws_error(client_tx, "blackmagic_camera_not_connected");
                return;
            };
            match camera.livestream_stop().await {
                Ok(()) => {
                    tracing::info!("camera livestream stop: accepted (204)");
                    ws_ok(client_tx)
                }
                Err(e) => {
                    tracing::info!("camera livestream stop: refused — {e}");
                    ws_error(client_tx, &e.to_string())
                }
            }
        }
        WsCommand::BlackmagicCameraStreamPushYoutube => {
            let Some(camera) = state.blackmagic_camera_connector.client().await else {
                ws_error(client_tx, "blackmagic_camera_not_connected");
                return;
            };
            match crate::server::routes::push_youtube_to_camera(&state.pool, &camera).await {
                Ok(mut payload) => {
                    payload["type"] = json!("blackmagic-camera.stream.platform");
                    let _ = client_tx.send(Message::Text(payload.to_string().into()));
                }
                Err(e) => ws_error(client_tx, &e),
            }
        }
        WsCommand::MiddlecontrolCameraSelect { camera_id } => {
            match state.middlecontrol_connector.select_camera(camera_id).await {
                Ok(()) => ws_ok(client_tx),
                Err(error) => ws_error(client_tx, &error),
            }
        }
        WsCommand::MiddlecontrolRecordStart { camera_id } => {
            match state
                .middlecontrol_connector
                .start_recording(camera_id)
                .await
            {
                Ok(()) => ws_ok(client_tx),
                Err(error) => ws_error(client_tx, &error),
            }
        }
        WsCommand::MiddlecontrolRecordStop { camera_id } => {
            match state
                .middlecontrol_connector
                .stop_recording(camera_id)
                .await
            {
                Ok(()) => ws_ok(client_tx),
                Err(error) => ws_error(client_tx, &error),
            }
        }
        WsCommand::MiddlecontrolRecordStartAll => {
            match state.middlecontrol_connector.start_recording_all().await {
                Ok(()) => ws_ok(client_tx),
                Err(error) => ws_error(client_tx, &error),
            }
        }
        WsCommand::MiddlecontrolRecordStopAll => {
            match state.middlecontrol_connector.stop_recording_all().await {
                Ok(()) => ws_ok(client_tx),
                Err(error) => ws_error(client_tx, &error),
            }
        }
        WsCommand::MiddlecontrolPresetRecall { preset, camera_id } => {
            match state
                .middlecontrol_connector
                .recall_preset(preset, camera_id)
                .await
            {
                Ok(()) => ws_ok(client_tx),
                Err(error) => ws_error(client_tx, &error),
            }
        }
        WsCommand::AtemStreamPushYoutube => {
            match crate::server::routes::push_youtube_to_atem(state).await {
                Ok(mut payload) => {
                    payload["type"] = json!("atem.stream.destination");
                    let _ = client_tx.send(Message::Text(payload.to_string().into()));
                }
                Err(error) => ws_error(client_tx, &error),
            }
        }
        WsCommand::AtemProgramSet { input } => {
            atem_send(state, client_tx, AtemCommand::Program(input)).await
        }
        WsCommand::AtemPreviewSet { input } => {
            atem_send(state, client_tx, AtemCommand::Preview(input)).await
        }
        WsCommand::AtemCut => atem_send(state, client_tx, AtemCommand::Cut).await,
        WsCommand::AtemAuto => atem_send(state, client_tx, AtemCommand::Auto).await,
        WsCommand::AtemRecordStart => atem_send(state, client_tx, AtemCommand::Record(true)).await,
        WsCommand::AtemRecordStop => atem_send(state, client_tx, AtemCommand::Record(false)).await,
        WsCommand::AtemStreamStart => atem_send(state, client_tx, AtemCommand::Stream(true)).await,
        WsCommand::AtemStreamStop => atem_send(state, client_tx, AtemCommand::Stream(false)).await,
    }
}

async fn atem_send(state: &AppState, tx: &mpsc::UnboundedSender<Message>, command: AtemCommand) {
    match state.atem_connector.send(command).await {
        Ok(()) => ws_ok(tx),
        Err(error) => ws_error(tx, &error),
    }
}

async fn persist_audio_recordings(
    pool: &PgPool,
    event_id: Uuid,
    files: &[FinalizedAudioFile],
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    for file in files {
        sqlx::query(
            "INSERT INTO recordings (event_id, file_path, file_name, file_size, \
             duration_seconds, media_kind, source, metadata, detected_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(event_id)
        .bind(&file.path)
        .bind(&file.file_name)
        .bind(i64::try_from(file.file_size).unwrap_or(i64::MAX))
        .bind(file.duration_seconds)
        .bind(&file.media_kind)
        .bind(&file.source)
        .bind(&file.metadata)
        .bind(Utc::now())
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await
}

#[derive(Deserialize, Serialize)]
struct PgNotify<T> {
    operation: String,
    record: T,
}

#[derive(Deserialize)]
pub struct WsQuery {
    token: Option<String>,
}

pub async fn ws_handler(State(state): State<AppState>, req: Request) -> Response {
    // Reject plain HTTP requests immediately with 426 rather than Axum's
    // generic 400 "Connection header did not include 'upgrade'".  This also
    // avoids having CorsLayer (applied on /api routes only) ever touching the
    // WebSocket upgrade path.
    let is_upgrade = req
        .headers()
        .get(header::CONNECTION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().contains("upgrade"));

    if !is_upgrade {
        return (
            StatusCode::UPGRADE_REQUIRED,
            [
                (header::UPGRADE, "websocket"),
                (header::CONTENT_TYPE, "application/json"),
            ],
            r#"{"error":"upgrade_required","description":"This endpoint requires a WebSocket connection, not a plain HTTP request.","connect":"ws://<host>/ws?token=<your-token>","auth":"Pass the bearer token as the 'token' query parameter."}"#,
        )
            .into_response();
    }

    let (mut parts, body) = req.into_parts();

    let query = match Query::<WsQuery>::from_request_parts(&mut parts, &state).await {
        Ok(q) => q.0,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    let current_token = state.auth_token.read().await.clone();
    let is_authenticated = query.token.as_deref() == Some(current_token.as_str());

    let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(ws) => ws,
        Err(e) => return e.into_response(),
    };

    drop(body); // WebSocket requests have no body; release it explicitly.
    let server_id = state.server_id.clone();
    let user_agent = parts
        .headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    ws.on_upgrade(move |socket| {
        handle_socket(socket, state, server_id, user_agent, is_authenticated)
    })
}

/// WS commands that read-only (unauthenticated) clients are permitted to send.
const READONLY_ALLOWED: &[&str] = &["presenter.register", "presenter.status", "pong"];

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    server_id: String,
    user_agent: Option<String>,
    is_authenticated: bool,
) {
    let client_id = Uuid::new_v4();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    {
        let mut clients = state.ws_clients.write().await;
        clients.insert(client_id, tx.clone());
    }
    {
        let mut info = state.ws_client_info.write().await;
        info.insert(
            client_id,
            WsClientInfo {
                id: client_id,
                label: "Browser".to_string(),
                user_agent,
                hostname: None,
                connected_at: Utc::now(),
                last_pong_at: None,
                latency_ms: None,
                ping_sent_at: None,
            },
        );
    }
    broadcast_clients_updated(&state).await;

    let connected_msg = json!({ "type": "connected", "serverId": server_id }).to_string();
    let _ = tx.send(Message::Text(connected_msg.into()));

    let recorder_state = state.rodecaster_audio_recorder.state();
    let recorder_msg =
        json!({ "type": "rodecaster.audio.record.state", "state": recorder_state }).to_string();
    let _ = tx.send(Message::Text(recorder_msg.into()));

    // Push current connector statuses so the client doesn't have to poll.
    let obs_status = state.obs_connector.get_status().await;
    let vmix_status = state.vmix_connector.get_status();
    let yt_status = state.youtube_connector.get_status().await;
    let fb_status = state.facebook_connector.get_status().await;
    let middlecontrol_status = state.middlecontrol_connector.get_status().await;
    let atem_status = state.atem_connector.get_status().await;
    for (connector, status) in [
        ("obs", obs_status),
        ("vmix", vmix_status),
        ("youtube", yt_status),
        ("facebook", fb_status),
        ("middlecontrol", middlecontrol_status),
        ("atem", atem_status),
    ] {
        let msg = json!({
            "type": "connector.status",
            "connector": connector,
            "status": status,
        })
        .to_string();
        let _ = tx.send(Message::Text(msg.into()));
    }

    // Push current presentation settings, status, and presenter state.
    {
        let settings_msg = make_presentation_settings(&state).await;
        let _ = tx.send(Message::Text(settings_msg.into()));
        let status_msg = make_presentation_status(&state).await;
        let _ = tx.send(Message::Text(status_msg.into()));
        let presenter_msg =
            json!({ "type": "presenter.state", "state": &*state.presenter_state.read().await })
                .to_string();
        let _ = tx.send(Message::Text(presenter_msg.into()));
    }

    // Send initial Keynote status on connection (macOS only).
    #[cfg(target_os = "macos")]
    {
        let kn_status = state.keynote_connector.get_status().await;
        let msg = json!({ "type": "keynote.status", "status": kn_status }).to_string();
        let _ = tx.send(Message::Text(msg.into()));
    }

    // Push current OBS streaming/recording state if OBS is connected.
    if let Some(output) = state.obs_connector.get_output_state().await {
        let msg = json!({
            "type": "connector.state",
            "connector": "obs",
            "isStreaming": output.is_streaming,
            "isRecording": output.is_recording,
        })
        .to_string();
        let _ = tx.send(Message::Text(msg.into()));
    }

    if let Some(middlecontrol) = state.middlecontrol_connector.get_state().await {
        let msg = json!({
            "type": "connector.state",
            "connector": "middlecontrol",
            "state": middlecontrol,
        })
        .to_string();
        let _ = tx.send(Message::Text(msg.into()));
    }

    if let Some(atem) = state.atem_connector.get_state().await {
        let msg = json!({
            "type": "connector.state",
            "connector": "atem",
            "state": atem,
        })
        .to_string();
        let _ = tx.send(Message::Text(msg.into()));
    }

    // Push cached OBS device scan result if available.
    {
        let devices_guard = state.obs_available_devices.read().await;
        if let Some(scanned) = devices_guard.as_ref() {
            let listeners: Vec<DeviceListener> =
                sqlx::query_as("SELECT * FROM device_listeners ORDER BY created_at")
                    .fetch_all(&state.pool)
                    .await
                    .unwrap_or_default();
            let statuses = crate::obs_devices::compute_listener_statuses(scanned, &listeners);
            let msg = json!({
                "type": "obs.devices.available",
                "devices": scanned,
                "listenerStatuses": statuses,
            })
            .to_string();
            let _ = tx.send(Message::Text(msg.into()));
        }
    }

    let (mut ws_sink, mut ws_stream) = socket.split();

    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    let state_recv = state.clone();
    let tx_recv = tx.clone();
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_stream.next().await {
            if let Message::Text(text) = msg {
                if !is_authenticated {
                    // Peek at the `type` field before full deserialisation.
                    let cmd_type = serde_json::from_str::<serde_json::Value>(&text)
                        .ok()
                        .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_owned))
                        .unwrap_or_default();
                    if !READONLY_ALLOWED.contains(&cmd_type.as_str()) {
                        let _ = tx_recv.send(Message::Text(
                            r#"{"type":"error","message":"unauthorized"}"#.into(),
                        ));
                        continue;
                    }
                }
                if let Ok(cmd) = serde_json::from_str::<WsCommand>(&text) {
                    handle_ws_command(cmd, &state_recv, &tx_recv, client_id).await;
                }
            }
        }
    });

    let tx_ping = tx.clone();
    let ping_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        interval.tick().await; // skip immediate first tick
        loop {
            interval.tick().await;
            if tx_ping.send(Message::Ping(vec![].into())).is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
        _ = ping_task => {}
    }

    {
        let mut clients = state.ws_clients.write().await;
        clients.remove(&client_id);
    }
    {
        let mut info = state.ws_client_info.write().await;
        info.remove(&client_id);
    }
    broadcast_clients_updated(&state).await;
}

pub async fn start_notify_listener(
    connection_url: String,
    ws_clients: Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    app_state: AppState,
) {
    let (client, mut connection) =
        match tokio_postgres::connect(&connection_url, tokio_postgres::NoTls).await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!("NOTIFY listener connect failed: {e}");
                return;
            }
        };

    let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<tokio_postgres::Notification>();

    tokio::spawn(async move {
        loop {
            match poll_fn(|cx| connection.poll_message(cx)).await {
                Some(Ok(AsyncMessage::Notification(n))) => {
                    let _ = notify_tx.send(n);
                }
                Some(Ok(_)) => {}
                None | Some(Err(_)) => break,
            }
        }
    });

    if let Err(e) = client.execute("LISTEN event_changes", &[]).await {
        tracing::error!("LISTEN event_changes failed: {e}");
        return;
    }
    if let Err(e) = client.execute("LISTEN recording_changes", &[]).await {
        tracing::error!("LISTEN recording_changes failed: {e}");
        return;
    }
    if let Err(e) = client.execute("LISTEN queue_changed", &[]).await {
        tracing::error!("LISTEN queue_changed failed: {e}");
        return;
    }

    while let Some(notification) = notify_rx.recv().await {
        let channel = notification.channel();
        let payload = notification.payload();

        let msg_text = if channel == "queue_changed" {
            // Wakes the sync worker and refreshes the Queues dashboard.
            app_state.queue_wake.notify_one();
            // The trigger fires per row, so draining a batch produces a burst of
            // these. Skip the stats aggregate entirely when nobody is watching —
            // which is every run where the dashboard isn't open.
            if ws_clients.read().await.is_empty() {
                continue;
            }
            match crate::queue::stats(&app_state.pool).await {
                Ok(queues) => json!({ "type": "queue.stats", "queues": queues }).to_string(),
                Err(e) => {
                    tracing::warn!("queue stats failed: {e}");
                    continue;
                }
            }
        } else if channel == "event_changes" {
            match serde_json::from_str::<PgNotify<Event>>(payload) {
                Ok(n) => {
                    // Spawn non-blocking scheduling tasks for INSERT/UPDATE
                    if n.operation == "INSERT" || n.operation == "UPDATE" {
                        spawn_scheduling_tasks(n.record.clone(), app_state.clone());
                    }
                    json!({ "type": "event.changed", "data": n }).to_string()
                }
                Err(e) => {
                    tracing::warn!("Failed to parse event notification: {e}");
                    continue;
                }
            }
        } else {
            match serde_json::from_str::<PgNotify<Recording>>(payload) {
                Ok(n) => json!({ "type": "recording.changed", "data": n }).to_string(),
                Err(e) => {
                    tracing::warn!("Failed to parse recording notification: {e}");
                    continue;
                }
            }
        };

        let clients = ws_clients.read().await;
        for tx in clients.values() {
            let _ = tx.send(Message::Text(msg_text.clone().into()));
        }
    }
}

/// Broadcast a `clients.updated` message containing all connected client info.
pub async fn broadcast_clients_updated(state: &AppState) {
    let clients_vec = {
        let info = state.ws_client_info.read().await;
        let mut v: Vec<WsClientInfo> = info.values().cloned().collect();
        v.sort_by_key(|c| c.connected_at);
        v
    };
    let msg = json!({ "type": "clients.updated", "clients": clients_vec }).to_string();
    let guard = state.ws_clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast a `presenter.state` message to all WebSocket clients.
pub async fn broadcast_presenter_state(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    state: &presenter::PresenterState,
) {
    let msg = json!({ "type": "presenter.state", "state": state }).to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast a `presenter.slide_changed` message to all WebSocket clients.
pub async fn broadcast_presenter_slide_changed(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    state: &presenter::PresenterState,
) {
    let msg = json!({
        "type": "presenter.slide_changed",
        "currentSlide": state.current_slide,
        "totalSlides": state.total_slides,
    })
    .to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast a `ppt.folders_changed` message when PPT folders are added/removed.
pub async fn broadcast_ppt_folders_changed(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
) {
    let msg = json!({ "type": "ppt.folders_changed" }).to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast a `recording.detected` message when OBS stops recording.
pub async fn broadcast_recording_detected(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    file_name: &str,
    event_title: Option<&str>,
) {
    let msg = json!({
        "type": "recording.detected",
        "fileName": file_name,
        "eventTitle": event_title,
    })
    .to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast a `recording.untracked.removed` message when an untracked recording is assigned.
pub async fn broadcast_untracked_removed(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    untracked_id: Uuid,
) {
    let msg = json!({
        "type": "recording.untracked.removed",
        "id": untracked_id,
    })
    .to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast an `event.changed` message to all connected WebSocket clients.
pub async fn broadcast_event_changed(state: &AppState, operation: &str, event: &Event) {
    let msg = json!({
        "type": "event.changed",
        "data": { "operation": operation, "record": event }
    })
    .to_string();
    let clients = state.ws_clients.read().await;
    for tx in clients.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Schedules the event on connected social platforms.
/// YouTube goes through the durable `platform_sync` job queue (retries, DLQ);
/// Facebook is still an inline fire-and-forget task.
pub fn spawn_scheduling_tasks(event: Event, state: AppState) {
    tokio::spawn(async move {
        crate::queue::enqueue_youtube_upsert(&state.pool, event.id).await;

        // ── Facebook ─────────────────────────────────────────────────────────
        let fb_status = state.facebook_connector.get_status().await;
        if matches!(fb_status, ConnectorStatus::Connected)
            && let Some(token) = facebook::load_tokens(&state.pool).await
        {
            let config = state.facebook_config.read().await.clone();
            let fb_conn = event.connection("facebook");
            let fb_schedule_status = fb_conn
                .map(|c| c.schedule_status.as_str())
                .unwrap_or("not_scheduled");
            let fb_privacy_status = fb_conn
                .and_then(|c| c.privacy_status.as_deref())
                .unwrap_or("EVERYONE");
            if !config.page_id.is_empty() && fb_schedule_status != "scheduled" {
                match facebook::schedule_event(
                    &event.title,
                    &event.date_time,
                    &token.access_token,
                    &config.page_id,
                    fb_privacy_status,
                )
                .await
                {
                    Ok(result) => match write_facebook_result(&state, event.id, &result).await {
                        Ok(updated) => {
                            broadcast_event_changed(&state, "UPDATE", &updated).await;
                        }
                        Err(e) => tracing::error!("Facebook DB write failed: {e}"),
                    },
                    Err(e) => {
                        tracing::error!("Facebook auto-schedule failed for {}: {e}", event.id);
                        let _ = write_facebook_status(&state.pool, event.id, "failed").await;
                    }
                }
            }
        }
    });
}

/// Broadcast `upload.progress` to all connected WebSocket clients.
pub async fn broadcast_upload_progress(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    recording_id: Uuid,
    platform: &str,
    progress_bytes: i64,
    total_bytes: i64,
) {
    let msg = json!({
        "type": "upload.progress",
        "recordingId": recording_id,
        "platform": platform,
        "progressBytes": progress_bytes,
        "totalBytes": total_bytes,
    })
    .to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast `upload.completed` to all connected WebSocket clients.
pub async fn broadcast_upload_completed(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    recording_id: Uuid,
    platform: &str,
    video_id: &str,
    video_url: &str,
) {
    let msg = json!({
        "type": "upload.completed",
        "recordingId": recording_id,
        "platform": platform,
        "videoId": video_id,
        "videoUrl": video_url,
    })
    .to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast `upload.failed` to all connected WebSocket clients.
pub async fn broadcast_upload_failed(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    recording_id: Uuid,
    platform: &str,
    error: &str,
) {
    let msg = json!({
        "type": "upload.failed",
        "recordingId": recording_id,
        "platform": platform,
        "error": error,
    })
    .to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

/// Broadcast `upload.paused` to all connected WebSocket clients.
pub async fn broadcast_upload_paused(
    clients: &Arc<RwLock<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>,
    recording_id: Uuid,
    reason: &str,
) {
    let msg = json!({
        "type": "upload.paused",
        "recordingId": recording_id,
        "reason": reason,
    })
    .to_string();
    let guard = clients.read().await;
    for tx in guard.values() {
        let _ = tx.send(Message::Text(msg.clone().into()));
    }
}

pub async fn write_youtube_result(
    state: &AppState,
    event_id: Uuid,
    result: &youtube::BroadcastResult,
) -> anyhow::Result<Event> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"INSERT INTO event_connections (event_id, platform, external_id, stream_url, schedule_status)
           VALUES ($1, 'youtube', $2, $3, 'scheduled')
           ON CONFLICT (event_id, platform) DO UPDATE SET
               external_id     = EXCLUDED.external_id,
               stream_url      = EXCLUDED.stream_url,
               schedule_status = 'scheduled',
               updated_at      = NOW()"#,
    )
    .bind(event_id)
    .bind(&result.broadcast_id)
    .bind(&result.watch_url)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
        .bind(event_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    fetch_event(event_id, &state.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("event {event_id} not found after YouTube write"))
}

pub async fn write_youtube_status(
    pool: &PgPool,
    event_id: Uuid,
    status: &str,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"INSERT INTO event_connections (event_id, platform, schedule_status)
           VALUES ($1, 'youtube', $2)
           ON CONFLICT (event_id, platform) DO UPDATE SET
               schedule_status = EXCLUDED.schedule_status,
               updated_at      = NOW()"#,
    )
    .bind(event_id)
    .bind(status)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
        .bind(event_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn write_facebook_result(
    state: &AppState,
    event_id: Uuid,
    result: &facebook::FacebookScheduleResult,
) -> anyhow::Result<Event> {
    let extra = serde_json::json!({"stream_id": result.stream_id});
    let mut tx = state.pool.begin().await?;
    sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"INSERT INTO event_connections (event_id, platform, external_id, event_url, schedule_status, extra)
           VALUES ($1, 'facebook', $2, $3, 'scheduled', $4)
           ON CONFLICT (event_id, platform) DO UPDATE SET
               external_id     = EXCLUDED.external_id,
               event_url       = EXCLUDED.event_url,
               schedule_status = 'scheduled',
               extra           = EXCLUDED.extra,
               updated_at      = NOW()"#,
    )
    .bind(event_id)
    .bind(&result.event_id)
    .bind(&result.event_url)
    .bind(extra)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
        .bind(event_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    fetch_event(event_id, &state.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("event {event_id} not found after Facebook write"))
}

pub async fn write_facebook_status(
    pool: &PgPool,
    event_id: Uuid,
    status: &str,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL app.skip_sync_notify = 'true'")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        r#"INSERT INTO event_connections (event_id, platform, schedule_status)
           VALUES ($1, 'facebook', $2)
           ON CONFLICT (event_id, platform) DO UPDATE SET
               schedule_status = EXCLUDED.schedule_status,
               updated_at      = NOW()"#,
    )
    .bind(event_id)
    .bind(status)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE events SET updated_at = NOW() WHERE id = $1")
        .bind(event_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}
