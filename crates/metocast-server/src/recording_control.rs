//! Starting and stopping the recorders, for the WebSocket handlers and for automations.
//!
//! Each target has two layers:
//! - the plain calls (`obs_record`, `camera_start`, `audio_stop`, …) do exactly what the
//!   WebSocket handlers always did, with the same errors, so a command's reply doesn't change;
//! - [`ensure`] reads the target's state first, so an automation never starts a recorder that
//!   is already recording or stops an idle one.

use std::sync::Arc;

use chrono::Utc;
use metocast_core::{automations::RecordTarget, connectors::RecorderStatus};
use sqlx::PgPool;
use uuid::Uuid;

use crate::automations::matching::{Decision, Observed, decide};
use crate::connectors::RodecasterConfig;
use crate::connectors::atem::AtemCommand;
use crate::connectors::rodecaster_audio::{AudioRecorderState, FinalizedAudioFile};
use crate::models::event::find_current_event;
use crate::server::AppState;

pub async fn obs_record(state: &AppState, on: bool) -> Result<(), String> {
    state.obs_connector.set_recording(on).await
}

pub async fn atem_record(state: &AppState, on: bool) -> Result<(), String> {
    state.atem_connector.send(AtemCommand::Record(on)).await
}

pub async fn camera_start(state: &AppState, clip_name: Option<&str>) -> Result<(), String> {
    let Some(camera) = state.blackmagic_camera_connector.client().await else {
        return Err("blackmagic_camera_not_connected".to_string());
    };
    camera
        .start_recording(clip_name)
        .await
        .map_err(|error| error.to_string())
}

pub async fn camera_stop(state: &AppState) -> Result<(), String> {
    let Some(camera) = state.blackmagic_camera_connector.client().await else {
        return Err("blackmagic_camera_not_connected".to_string());
    };
    camera
        .stop_recording()
        .await
        .map_err(|error| error.to_string())
}

/// Starts or stops the recording of one camera, or of the one Middle Control has selected when
/// there is none.
pub async fn middlecontrol_record(
    state: &AppState,
    on: bool,
    camera_id: Option<u8>,
) -> Result<(), String> {
    if on {
        state
            .middlecontrol_connector
            .start_recording(camera_id)
            .await
    } else {
        state
            .middlecontrol_connector
            .stop_recording(camera_id)
            .await
    }
}

pub async fn middlecontrol_record_all(state: &AppState, on: bool) -> Result<(), String> {
    if on {
        state.middlecontrol_connector.start_recording_all().await
    } else {
        state.middlecontrol_connector.stop_recording_all().await
    }
}

/// The RØDECaster's own SD or USB recorder.
pub async fn desk_record(state: &AppState, on: bool) -> Result<(), String> {
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
/// goes under the server's current event.
///
/// # Errors
/// The connector's error code (`obs_not_connected`, `atem_recording_unsupported`,
/// `rodecaster_no_media`, …), `no event today`, or the device's own words.
pub async fn ensure(
    state: &AppState,
    target: &RecordTarget,
    recording: bool,
) -> Result<Outcome, String> {
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
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "no event today".to_string())?;
            audio_start(state, Some(event.id)).await.map(drop)
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
