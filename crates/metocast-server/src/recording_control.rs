//! Starting and stopping the recorders, for the WebSocket handlers and for automations.
//!
//! Each target has two layers:
//! - the plain calls (`obs_record`, `camera_start`, `audio_stop`, …) are what the WebSocket
//!   handlers send; the camera and Middle Control ones wait for the state they asked for, since
//!   neither device refuses a command it then ignores;
//! - [`ensure`] reads the target's state first, so an automation never starts a recorder that
//!   is already recording or stops an idle one.

use std::sync::Arc;

use chrono::Utc;
use metocast_core::{
    automations::RecordTarget,
    connectors::{MiddlecontrolState, RecorderStatus},
};
use sqlx::PgPool;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::automations::matching::{Decision, Observed, decide};
use crate::connectors::RodecasterConfig;
use crate::connectors::atem::AtemCommand;
use crate::connectors::blackmagic_camera::{Camera, CameraState};
use crate::connectors::rodecaster_audio::{AudioRecorderState, FinalizedAudioFile};
use crate::models::event::find_current_event;
use crate::server::AppState;

/// Notes a stop Metocast asks for, so the `recorder.stopped` that follows says who stopped it.
fn stopping(state: &AppState, on: bool, target: RecordTarget) {
    if !on {
        state.recorders.stopping(target);
    }
}

pub async fn obs_record(state: &AppState, on: bool) -> Result<(), String> {
    stopping(state, on, RecordTarget::Obs);
    state.obs_connector.set_recording(on).await
}

pub async fn atem_record(state: &AppState, on: bool) -> Result<(), String> {
    stopping(state, on, RecordTarget::Atem);
    state.atem_connector.send(AtemCommand::Record(on)).await
}

/// How long a device has to show the state a command asked for before the command counts as
/// ignored.
const CONFIRM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The camera answers 204 to a record or stream command it then ignores (no card or SSD, the
/// stream held by Middle Control), so a command only counts once its state arrives.
///
/// # Errors
/// `blackmagic_camera_not_connected`, `camera_no_media`, `camera_stream_held_by_middlecontrol`,
/// `camera_ignored_command`, or the camera's own words.
pub async fn camera_start(state: &AppState, clip_name: Option<&str>) -> Result<(), String> {
    let camera = camera_client(state).await?;
    if !camera_has_media(state).await {
        return Err("camera_no_media".to_string());
    }
    camera_confirm(state, camera.start_recording(clip_name), |s| s.is_recording).await
}

pub async fn camera_stop(state: &AppState) -> Result<(), String> {
    stopping(state, false, RecordTarget::BlackmagicCamera);
    let camera = camera_client(state).await?;
    camera_confirm(state, camera.stop_recording(), |s| !s.is_recording).await
}

/// Starts or ends the camera's own livestream. Starting counts once it is connecting.
pub async fn camera_stream(state: &AppState, on: bool) -> Result<(), String> {
    let camera = camera_client(state).await?;
    let result = if on {
        camera_confirm(state, camera.livestream_start(), |s| {
            s.stream_status != crate::connectors::blackmagic_camera::STREAM_IDLE
        })
        .await
    } else {
        camera_confirm(state, camera.livestream_stop(), |s| !s.is_streaming()).await
    };
    if !matches!(&result, Err(error) if error == IGNORED) {
        return result;
    }
    // ponytail: Middle Control names its camera feed "Middle…"; match its exact name if
    // another platform ever starts that way.
    match camera.livestream_active_platform().await {
        Ok(active) if active.platform.starts_with("Middle") => {
            Err("camera_stream_held_by_middlecontrol".to_string())
        }
        Ok(_) => result,
        Err(error) => Err(format!("{IGNORED}: {error}")),
    }
}

async fn camera_client(state: &AppState) -> Result<Arc<Camera>, String> {
    state
        .blackmagic_camera_connector
        .client()
        .await
        .ok_or_else(|| "blackmagic_camera_not_connected".to_string())
}

/// Unknown counts as present: only a camera that said so is refused.
async fn camera_has_media(state: &AppState) -> bool {
    state
        .blackmagic_camera_connector
        .get_state()
        .await
        .is_none_or(|camera| camera.has_media)
}

const IGNORED: &str = "camera_ignored_command";

async fn camera_confirm(
    state: &AppState,
    command: impl Future<Output = blackmagic_camera::Result<()>>,
    done: impl Fn(&CameraState) -> bool,
) -> Result<(), String> {
    let connector = &state.blackmagic_camera_connector;
    let changes = connector.state_tx.subscribe();
    command.await.map_err(|error| error.to_string())?;
    if reached(connector.get_state().await, changes, done).await {
        Ok(())
    } else {
        Err(IGNORED.to_string())
    }
}

/// Waits up to [`CONFIRM_TIMEOUT`] for a state `done` accepts: `current`, read after the
/// command, or a change from `changes`, subscribed before it so none is missed.
async fn reached<T: Clone>(
    current: Option<T>,
    mut changes: broadcast::Receiver<T>,
    done: impl Fn(&T) -> bool,
) -> bool {
    if current.as_ref().is_some_and(&done) {
        return true;
    }
    tokio::time::timeout(CONFIRM_TIMEOUT, async move {
        loop {
            match changes.recv().await {
                Ok(state) if done(&state) => return true,
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return false,
            }
        }
    })
    .await
    .unwrap_or(false)
}

#[cfg(test)]
mod reached_tests {
    use super::*;

    #[tokio::test]
    async fn a_command_counts_once_its_state_arrives_and_not_before() {
        let (tx, _) = broadcast::channel(4);
        assert!(reached(Some(true), tx.subscribe(), |on| *on).await);

        let changes = tx.subscribe();
        tx.send(false).unwrap();
        tx.send(true).unwrap();
        assert!(reached(Some(false), changes, |on| *on).await);

        // Nothing changes: the camera took the command and did nothing.
        assert!(!reached(Some(false), tx.subscribe(), |on| *on).await);
    }
}

/// Starts or stops the recording of one camera, or of the one Middle Control has selected when
/// there is none. Middle Control never answers a command, so it counts once its recording list
/// or flag says so.
///
/// # Errors
/// Middle Control's own error, `camera_no_media` when the camera it records with has no card
/// or SSD, or `middlecontrol_ignored_command`.
pub async fn middlecontrol_record(
    state: &AppState,
    on: bool,
    camera_id: Option<u8>,
) -> Result<(), String> {
    stopping(state, on, RecordTarget::Middlecontrol { camera_id });
    let connector = &state.middlecontrol_connector;
    let changes = connector.state_tx.subscribe();
    if on {
        connector.start_recording(camera_id).await?;
    } else {
        connector.stop_recording(camera_id).await?;
    }
    // A list or flag Middle Control never sent can't confirm anything, so it doesn't refuse.
    middlecontrol_confirm(state, changes, on, move |s| match camera_id {
        Some(id) => s
            .recording_camera_ids
            .as_ref()
            .is_none_or(|ids| ids.contains(&id) == on),
        None => s.recording.is_none_or(|recording| recording == on),
    })
    .await
}

pub async fn middlecontrol_record_all(state: &AppState, on: bool) -> Result<(), String> {
    stopping(state, on, RecordTarget::Middlecontrol { camera_id: None });
    let connector = &state.middlecontrol_connector;
    let changes = connector.state_tx.subscribe();
    if on {
        connector.start_recording_all().await?;
    } else {
        connector.stop_recording_all().await?;
    }
    middlecontrol_confirm(state, changes, on, |s| {
        s.recording.is_none_or(|recording| recording == on)
    })
    .await
}

async fn middlecontrol_confirm(
    state: &AppState,
    changes: broadcast::Receiver<MiddlecontrolState>,
    on: bool,
    done: impl Fn(&MiddlecontrolState) -> bool,
) -> Result<(), String> {
    if reached(
        state.middlecontrol_connector.get_state().await,
        changes,
        done,
    )
    .await
    {
        return Ok(());
    }
    // ponytail: assumes Middle Control drives the one Blackmagic camera Metocast knows;
    // map Middle Control camera ids to cameras when there is a second one.
    if on && !camera_has_media(state).await {
        return Err("camera_no_media".to_string());
    }
    Err("middlecontrol_ignored_command".to_string())
}

/// The RØDECaster's own SD or USB recorder.
pub async fn desk_record(state: &AppState, on: bool) -> Result<(), String> {
    stopping(state, on, RecordTarget::Rodecaster);
    state.rodecaster_connector.set_recording(on).await
}

/// Starts Metocast's FLAC recording of the desk's USB audio, under an event or, with none,
/// as untracked files an operator can assign later. A recording that is already running is
/// answered with its state.
///
/// # Errors
/// `recording_event_unavailable` for an event that is missing or completed,
/// `rodecaster_not_connected`, or the recorder's own error.
pub async fn audio_start(
    state: &AppState,
    event_id: Option<Uuid>,
) -> Result<AudioRecorderState, String> {
    if let Some(event_id) = event_id {
        ensure_event_open(&state.pool, event_id).await?;
    }
    let Some(profile) = state.rodecaster_connector.get_profile() else {
        return Err("rodecaster_not_connected".to_string());
    };
    let config: RodecasterConfig =
        crate::database::settings::get_json(&state.pool, "rodecaster_config").await;
    let recorder = Arc::clone(&state.rodecaster_audio_recorder);
    match tokio::task::spawn_blocking(move || {
        recorder.start(event_id, &config.audio_recording, &profile)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => Err(format!("recording_start_failed: {error}")),
    }
}

/// An event that exists and is not completed.
async fn ensure_event_open(pool: &PgPool, event_id: Uuid) -> Result<(), String> {
    let event_available: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM events e WHERE e.id = $1 AND NOT EXISTS(\
         SELECT 1 FROM event_activities a WHERE a.event_id = e.id \
         AND a.activity_type = 'completed'))",
    )
    .bind(event_id)
    .fetch_one(pool)
    .await
    .map_err(|error| error.to_string())?;
    if event_available {
        Ok(())
    } else {
        Err("recording_event_unavailable".to_string())
    }
}

pub enum AudioStop {
    /// Nothing was recording; this is the recorder's state.
    AlreadyIdle(Box<AudioRecorderState>),
    /// The files are closed and filed under the event, or as untracked recordings.
    Stopped,
}

/// Stops Metocast's audio recording and files its FLACs under the event it started with, or as
/// untracked recordings when it had none.
///
/// # Errors
/// `recording_metadata_failed: …`, or the writer's own error.
pub async fn audio_stop(state: &AppState) -> Result<AudioStop, String> {
    stopping(state, false, RecordTarget::RodecasterAudio);
    let recorder_state = state.rodecaster_audio_recorder.state();
    if recorder_state.status == RecorderStatus::Idle {
        return Ok(AudioStop::AlreadyIdle(Box::new(recorder_state)));
    }
    let event_id = recorder_state.event_id;
    let recorder = Arc::clone(&state.rodecaster_audio_recorder);
    let files = match tokio::task::spawn_blocking(move || recorder.stop()).await {
        Ok(Ok(files)) => files,
        Ok(Err(error)) => return Err(error),
        Err(error) => return Err(format!("recording_stop_failed: {error}")),
    };
    persist_audio_recordings(&state.pool, event_id, &files)
        .await
        .map_err(|error| format!("recording_metadata_failed: {error}"))?;
    if event_id.is_none() {
        for file in &files {
            crate::server::websocket::broadcast_recording_detected(
                &state.ws_clients,
                &file.file_name,
                None,
            )
            .await;
        }
    }
    Ok(AudioStop::Stopped)
}

/// Files the finished FLACs under `event_id`, or as untracked recordings when there is none.
async fn persist_audio_recordings(
    pool: &PgPool,
    event_id: Option<Uuid>,
    files: &[FinalizedAudioFile],
) -> Result<(), sqlx::Error> {
    let mut transaction = pool.begin().await?;
    for file in files {
        let file_size = i64::try_from(file.file_size).unwrap_or(i64::MAX);
        match event_id {
            Some(event_id) => {
                sqlx::query(
                    "INSERT INTO recordings (event_id, file_path, file_name, file_size, \
                     duration_seconds, media_kind, source, metadata, detected_at) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                )
                .bind(event_id)
                .bind(&file.path)
                .bind(&file.file_name)
                .bind(file_size)
                .bind(file.duration_seconds)
                .bind(&file.media_kind)
                .bind(&file.source)
                .bind(&file.metadata)
                .bind(Utc::now())
                .execute(&mut *transaction)
                .await?;
            }
            // `mediaKind` and `source` travel in the metadata, which assigning restores.
            None => {
                sqlx::query(
                    "INSERT INTO untracked_recordings (file_path, file_name, file_size, \
                     duration_seconds, metadata, detected_at) VALUES ($1, $2, $3, $4, $5, $6)",
                )
                .bind(&file.path)
                .bind(&file.file_name)
                .bind(file_size)
                .bind(file.duration_seconds)
                .bind(&file.metadata)
                .bind(Utc::now())
                .execute(&mut *transaction)
                .await?;
            }
        }
    }
    transaction.commit().await
}

/// What `ensure` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// It was already so, and why.
    Skipped(String),
}

/// What each target's state says about whether it is recording, as plain functions of the state
/// the connectors keep, so they can be tested without a device.
mod observed {
    use metocast_core::connectors::{
        AtemState, DeskRecorderStatus, MiddlecontrolState, RecordStatus, RecorderStatus,
    };

    use crate::automations::matching::Observed;
    use crate::connectors::blackmagic_camera::CameraState;
    use crate::connectors::obs::ObsOutputState;

    /// `None` is OBS not connected: the request itself then says so.
    pub fn obs(state: Option<ObsOutputState>) -> Observed {
        state.map_or(Observed::Unknown, |output| {
            if output.is_recording {
                Observed::Recording
            } else {
                Observed::Idle
            }
        })
    }

    /// An ATEM that isn't connected, or has no recording engine, is an error to report, not a
    /// state to act on.
    pub fn atem(state: Option<&AtemState>) -> Result<Observed, String> {
        let state = state.ok_or("atem_not_connected")?;
        Ok(match state.recording.ok_or("atem_recording_unsupported")? {
            RecordStatus::Recording => Observed::Recording,
            RecordStatus::Idle => Observed::Idle,
            RecordStatus::Stopping => Observed::Stopping,
        })
    }

    pub fn camera(state: Option<&CameraState>) -> Observed {
        state.map_or(Observed::Unknown, |camera| {
            if camera.is_recording {
                Observed::Recording
            } else {
                Observed::Idle
            }
        })
    }

    /// One camera is known from Middle Control's `REC_LIST`. "Every camera" is not: the list
    /// says which cameras record, not whether all of them do, so the request goes out anyway.
    pub fn middlecontrol(state: Option<&MiddlecontrolState>, camera: Option<u8>) -> Observed {
        let Some(camera) = camera else {
            return Observed::Unknown;
        };
        state
            .and_then(|state| state.recording_camera_ids.as_ref())
            .map_or(Observed::Unknown, |recording| {
                if recording.contains(&camera) {
                    Observed::Recording
                } else {
                    Observed::Idle
                }
            })
    }

    pub fn desk(status: Option<DeskRecorderStatus>) -> Observed {
        match status {
            Some(DeskRecorderStatus::Recording) => Observed::Recording,
            Some(DeskRecorderStatus::Stopped) => Observed::Idle,
            // Either way a request does something: resume, or stop.
            Some(DeskRecorderStatus::Paused) | None => Observed::Unknown,
        }
    }

    pub fn audio(status: RecorderStatus) -> Observed {
        match status {
            RecorderStatus::Recording => Observed::Recording,
            RecorderStatus::Idle => Observed::Idle,
            // A recording that failed to close still has something to stop.
            RecorderStatus::Failed => Observed::Unknown,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::automations::matching::{Decision, decide};

        /// What an automation does for a target in this state: `(start, stop)`.
        fn acts(observed: Observed) -> (bool, bool) {
            (
                decide(observed, true) == Decision::Act,
                decide(observed, false) == Decision::Act,
            )
        }

        fn middle(recording: Option<Vec<u8>>) -> MiddlecontrolState {
            MiddlecontrolState {
                selected_camera: Some(1),
                recording: None,
                recording_camera_ids: recording,
                connected_camera_ids: Some(vec![1, 2]),
                connected_apcr_ids: None,
                preset_move_active: None,
                running_preset: None,
                last_preset: None,
            }
        }

        fn atem_with(recording: Option<RecordStatus>) -> AtemState {
            AtemState {
                recording,
                ..AtemState::default()
            }
        }

        #[test]
        fn obs_is_left_alone_when_it_already_is_so_and_asked_when_it_is_not_connected() {
            let state = |is_recording| {
                Some(ObsOutputState {
                    is_streaming: false,
                    is_recording,
                })
            };
            assert_eq!(acts(obs(state(true))), (false, true));
            assert_eq!(acts(obs(state(false))), (true, false));
            // Not connected: the request fails with `obs_not_connected`, whichever way.
            assert_eq!(acts(obs(None)), (true, true));
        }

        #[test]
        fn the_atem_reports_its_recording_engine_or_says_why_not() {
            assert_eq!(
                acts(atem(Some(&atem_with(Some(RecordStatus::Recording)))).unwrap()),
                (false, true)
            );
            assert_eq!(
                acts(atem(Some(&atem_with(Some(RecordStatus::Idle)))).unwrap()),
                (true, false)
            );
            // Already stopping: a stop is redundant, and a start goes ahead.
            assert_eq!(
                acts(atem(Some(&atem_with(Some(RecordStatus::Stopping)))).unwrap()),
                (true, false)
            );
            assert_eq!(atem(None), Err("atem_not_connected".to_string()));
            assert_eq!(
                atem(Some(&atem_with(None))),
                Err("atem_recording_unsupported".to_string())
            );
        }

        #[test]
        fn the_blackmagic_camera_is_read_from_its_state() {
            let state = |is_recording| CameraState {
                stream_status: "Idle".to_string(),
                is_recording,
                has_media: true,
            };
            assert_eq!(acts(camera(Some(&state(true)))), (false, true));
            assert_eq!(acts(camera(Some(&state(false)))), (true, false));
            assert_eq!(acts(camera(None)), (true, true));
        }

        #[test]
        fn one_middle_control_camera_is_read_from_the_recording_list() {
            let recording = middle(Some(vec![2]));
            assert_eq!(
                acts(middlecontrol(Some(&recording), Some(2))),
                (false, true)
            );
            assert_eq!(
                acts(middlecontrol(Some(&recording), Some(1))),
                (true, false)
            );
            // An empty list means nothing records.
            assert_eq!(
                acts(middlecontrol(Some(&middle(Some(Vec::new()))), Some(2))),
                (true, false)
            );
        }

        #[test]
        fn middle_control_is_asked_when_the_list_is_unknown_or_for_every_camera() {
            assert_eq!(
                acts(middlecontrol(Some(&middle(None)), Some(2))),
                (true, true)
            );
            assert_eq!(acts(middlecontrol(None, Some(2))), (true, true));
            // Every camera: the list can't say that all of them record.
            assert_eq!(
                acts(middlecontrol(Some(&middle(Some(vec![1, 2]))), None)),
                (true, true)
            );
        }

        #[test]
        fn the_desks_recorder_resumes_when_paused_and_stops_whatever_it_is_doing() {
            use DeskRecorderStatus::{Paused, Recording, Stopped};
            assert_eq!(acts(desk(Some(Recording))), (false, true));
            assert_eq!(acts(desk(Some(Stopped))), (true, false));
            // A start resumes a pause and a stop ends it.
            assert_eq!(acts(desk(Some(Paused))), (true, true));
            // Not connected: the request fails with `rodecaster_not_connected`.
            assert_eq!(acts(desk(None)), (true, true));
        }

        #[test]
        fn the_audio_recording_is_read_from_its_status() {
            assert_eq!(acts(audio(RecorderStatus::Recording)), (false, true));
            assert_eq!(acts(audio(RecorderStatus::Idle)), (true, false));
            // Failed to close: both a new start and a stop have something to do.
            assert_eq!(acts(audio(RecorderStatus::Failed)), (true, true));
        }
    }
}

/// What the target is doing now, as far as the core can tell.
async fn observe(state: &AppState, target: &RecordTarget) -> Result<Observed, String> {
    Ok(match target {
        RecordTarget::Obs => observed::obs(state.obs_connector.get_output_state().await),
        RecordTarget::Atem => observed::atem(state.atem_connector.get_state().await.as_ref())?,
        RecordTarget::BlackmagicCamera => {
            observed::camera(state.blackmagic_camera_connector.get_state().await.as_ref())
        }
        RecordTarget::Middlecontrol { camera_id } => observed::middlecontrol(
            state.middlecontrol_connector.get_state().await.as_ref(),
            *camera_id,
        ),
        RecordTarget::Rodecaster => observed::desk(
            state
                .rodecaster_connector
                .get_recorder()
                .map(|recorder| crate::connectors::rodecaster::desk_recorder(recorder).status),
        ),
        RecordTarget::RodecasterAudio => {
            observed::audio(state.rodecaster_audio_recorder.state().status)
        }
    })
}

/// Starts (`recording`) or stops a recorder, unless it already is. Metocast's audio recording
/// goes under the server's current event, or is untracked when there is none, like the
/// manual Record button. A start's result is kept for the Record button's status.
///
/// # Errors
/// The connector's error code (`obs_not_connected`, `atem_recording_unsupported`,
/// `rodecaster_no_media`, …) or the device's own words.
pub async fn ensure(
    state: &AppState,
    target: &RecordTarget,
    recording: bool,
) -> Result<Outcome, String> {
    let result = act(state, target, recording).await;
    if recording {
        state.recorders.started(target, &result);
    }
    result
}

async fn act(state: &AppState, target: &RecordTarget, recording: bool) -> Result<Outcome, String> {
    match decide(observe(state, target).await?, recording) {
        Decision::Skip(reason) => return Ok(Outcome::Skipped(reason.to_string())),
        Decision::Act => {}
    }
    match target {
        RecordTarget::Obs => obs_record(state, recording).await,
        RecordTarget::Atem => atem_record(state, recording).await,
        RecordTarget::BlackmagicCamera if recording => camera_start(state, None).await,
        RecordTarget::BlackmagicCamera => camera_stop(state).await,
        RecordTarget::Middlecontrol {
            camera_id: Some(camera),
        } => middlecontrol_record(state, recording, Some(*camera)).await,
        RecordTarget::Middlecontrol { camera_id: None } => {
            middlecontrol_record_all(state, recording).await
        }
        RecordTarget::Rodecaster => desk_record(state, recording).await,
        RecordTarget::RodecasterAudio if recording => {
            if state.rodecaster_connector.get_profile().is_none() {
                return Err("rodecaster_not_connected".to_string());
            }
            let event = find_current_event(&state.pool)
                .await
                .map_err(|error| error.to_string())?;
            audio_start(state, event.map(|event| event.id))
                .await
                .map(drop)
        }
        RecordTarget::RodecasterAudio => audio_stop(state).await.map(drop),
    }?;
    Ok(Outcome::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::untracked_recording;

    /// With `METOCAST_TEST_DATABASE_URL` (a scratch database): an audio recording that had no
    /// event is filed as untracked, and assigning it later restores its source and media kind.
    #[tokio::test]
    async fn an_audio_recording_without_an_event_is_untracked_until_assigned() {
        let Ok(url) = std::env::var("METOCAST_TEST_DATABASE_URL") else {
            return;
        };
        let pool = PgPool::connect(&url).await.unwrap();
        crate::database::run_migrations(&pool).await.unwrap();
        let file_name = format!("no-event-{}.flac", Uuid::new_v4());
        let file = FinalizedAudioFile {
            path: format!("/tmp/{file_name}"),
            file_name: file_name.clone(),
            file_size: 12,
            duration_seconds: 3.5,
            media_kind: "deviceMix".to_string(),
            source: "rodecaster".to_string(),
            metadata: serde_json::json!({"source": "rodecaster", "mediaKind": "deviceMix"}),
        };
        persist_audio_recordings(&pool, None, &[file])
            .await
            .unwrap();
        let id: Uuid =
            sqlx::query_scalar("SELECT id FROM untracked_recordings WHERE file_name = $1")
                .bind(&file_name)
                .fetch_one(&pool)
                .await
                .unwrap();

        let event: Uuid = sqlx::query_scalar(
            "INSERT INTO events (title, date_time) VALUES ('assign test', now()) RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let recording = untracked_recording::assign(&pool, id, event).await.unwrap();
        assert_eq!(recording.media_kind, "deviceMix");
        assert_eq!(recording.source, "rodecaster");
        let left: i64 =
            sqlx::query_scalar("SELECT count(*) FROM untracked_recordings WHERE id = $1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(left, 0);
        // The recording goes with its event.
        sqlx::query("DELETE FROM events WHERE id = $1")
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
    }
}
