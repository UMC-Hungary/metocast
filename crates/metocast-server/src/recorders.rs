//! The Record button: which recorders it drives, what each one is doing, and a
//! `recorder.stopped` push whenever any recording stops, whoever or whatever stopped it.
//!
//! Starting and stopping go through [`recording_control::ensure`], like automations, which
//! also records each start's result here, so the button shows an automation's failures too.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::Utc;
use futures_util::future::join_all;
use metocast_core::automations::{LastStart, RecordButton, RecordTarget, RecorderRow, StopReason};
use metocast_core::connectors::{RecordStatus, RecorderStatus};
use serde_json::json;

use crate::recording_control::{self, Outcome};
use crate::server::AppState;
use crate::server::websocket::broadcast_json;

pub const SETTING: &str = "record_button";

/// A recording that stops this soon after Metocast asked it to was stopped by Metocast.
const ASKED_WITHIN: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct Recorders {
    selected: Mutex<Vec<RecordTarget>>,
    last_start: Mutex<HashMap<RecordTarget, LastStart>>,
    stop_asked: Mutex<HashMap<RecordTarget, Instant>>,
}

impl Recorders {
    pub fn selected(&self) -> Vec<RecordTarget> {
        self.selected.lock().expect("selected lock").clone()
    }

    pub fn select(&self, targets: Vec<RecordTarget>) {
        *self.selected.lock().expect("selected lock") = targets;
    }

    /// What a start by the Record button or an automation came to.
    pub fn started(&self, target: &RecordTarget, result: &Result<Outcome, String>) {
        let start = LastStart {
            at: Utc::now(),
            error: result.as_ref().err().cloned(),
        };
        self.last_start
            .lock()
            .expect("last start lock")
            .insert(target.clone(), start);
    }

    /// Metocast is about to ask `target` to stop. Middle Control is one recorder here,
    /// whichever camera was asked.
    pub fn stopping(&self, target: RecordTarget) {
        let target = match target {
            RecordTarget::Middlecontrol { .. } => RecordTarget::Middlecontrol { camera_id: None },
            other => other,
        };
        self.stop_asked
            .lock()
            .expect("stop lock")
            .insert(target, Instant::now());
    }

    fn asked_to_stop(&self, target: &RecordTarget) -> bool {
        self.stop_asked
            .lock()
            .expect("stop lock")
            .get(target)
            .is_some_and(|at| at.elapsed() < ASKED_WITHIN)
    }
}

/// Starts (`recording`) or stops every recorder the button is set to, side by side.
///
/// # Errors
/// `recorders_failed: obs: obs_not_connected; …`, naming each recorder that failed.
pub async fn press(state: &AppState, recording: bool) -> Result<(), String> {
    let targets = state.recorders.selected();
    let results = join_all(targets.iter().map(|target| async move {
        let result = recording_control::ensure(state, target, recording).await;
        // Broadcast as each recorder finishes so one slow or unreachable device never holds
        // back the others' states or errors.
        broadcast_json(&state.ws_clients, &state_message(&rows(state).await)).await;
        result
    }))
    .await;
    let failures: Vec<String> = targets
        .iter()
        .zip(results)
        .filter_map(|(target, result)| {
            result
                .err()
                .map(|error| format!("{}: {error}", name(target)))
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        let code = if recording {
            "recorders_failed"
        } else {
            "recorders_stop_failed"
        };
        Err(format!("{code}: {}", failures.join("; ")))
    }
}

/// The recorder's wire name, with Middle Control's camera: `obs`, `middlecontrol 2`.
fn name(target: &RecordTarget) -> String {
    match target {
        RecordTarget::Obs => "obs".to_string(),
        RecordTarget::Atem => "atem".to_string(),
        RecordTarget::BlackmagicCamera => "blackmagic-camera".to_string(),
        RecordTarget::Middlecontrol { camera_id: None } => "middlecontrol".to_string(),
        RecordTarget::Middlecontrol {
            camera_id: Some(id),
        } => format!("middlecontrol {id}"),
        RecordTarget::Rodecaster => "rodecaster".to_string(),
        RecordTarget::RodecasterAudio => "rodecaster-audio".to_string(),
    }
}

/// Every recorder the button could drive: each device, each camera Middle Control has, and
/// whatever is selected that isn't there now.
pub async fn rows(state: &AppState) -> Vec<RecorderRow> {
    let selected = state.recorders.selected();
    let middle = state.middlecontrol_connector.get_state().await;
    let mut targets = vec![
        RecordTarget::Obs,
        RecordTarget::Atem,
        RecordTarget::BlackmagicCamera,
        RecordTarget::Middlecontrol { camera_id: None },
    ];
    let cameras = middle
        .as_ref()
        .and_then(|state| state.connected_camera_ids.clone())
        .unwrap_or_default();
    targets.extend(cameras.into_iter().map(|id| RecordTarget::Middlecontrol {
        camera_id: Some(id),
    }));
    targets.extend([RecordTarget::Rodecaster, RecordTarget::RodecasterAudio]);
    for target in &selected {
        if !targets.contains(target) {
            targets.push(target.clone());
        }
    }
    let last_start = state
        .recorders
        .last_start
        .lock()
        .expect("last start lock")
        .clone();
    let mut rows = Vec::with_capacity(targets.len());
    for target in targets {
        let recording = recording(state, &target).await;
        rows.push(RecorderRow {
            selected: selected.contains(&target),
            available: recording.is_some(),
            recording: recording.unwrap_or(false),
            last_start: last_start.get(&target).cloned(),
            target,
        });
    }
    rows
}

/// Whether `target` records now; `None` when it isn't connected or can't record. These are
/// the connectors' cached states, so reading them costs the devices nothing.
async fn recording(state: &AppState, target: &RecordTarget) -> Option<bool> {
    match target {
        RecordTarget::Obs => state
            .obs_connector
            .get_output_state()
            .await
            .map(|output| output.is_recording),
        RecordTarget::Atem => state
            .atem_connector
            .get_state()
            .await
            .and_then(|atem| atem.recording)
            .map(|status| status == RecordStatus::Recording),
        RecordTarget::BlackmagicCamera => state
            .blackmagic_camera_connector
            .get_state()
            .await
            .map(|camera| camera.is_recording),
        RecordTarget::Middlecontrol { camera_id } => {
            let middle = state.middlecontrol_connector.get_state().await?;
            match camera_id {
                Some(id) => middle.recording_camera_ids.map(|ids| ids.contains(id)),
                None => middle.recording,
            }
        }
        RecordTarget::Rodecaster => state.rodecaster_connector.get_recorder().map(|recorder| {
            crate::connectors::rodecaster::desk_recorder(recorder).status
                == metocast_core::connectors::DeskRecorderStatus::Recording
        }),
        RecordTarget::RodecasterAudio => {
            let status = state.rodecaster_audio_recorder.state().status;
            (state.rodecaster_connector.get_profile().is_some() || status != RecorderStatus::Idle)
                .then_some(status == RecorderStatus::Recording)
        }
    }
}

/// The recorders that were recording in `before` and aren't in `now`, and whether each is
/// still connected. Middle Control's cameras are covered by its own row.
fn stopped<'a>(before: &'a [RecorderRow], now: &[RecorderRow]) -> Vec<(&'a RecordTarget, bool)> {
    before
        .iter()
        .filter(|row| {
            row.recording
                && !matches!(
                    row.target,
                    RecordTarget::Middlecontrol { camera_id: Some(_) }
                )
        })
        .filter_map(|row| {
            let now = now.iter().find(|now| now.target == row.target);
            (!now.is_some_and(|now| now.recording))
                .then_some((&row.target, now.is_some_and(|now| now.available)))
        })
        .collect()
}

pub fn state_message(rows: &[RecorderRow]) -> serde_json::Value {
    json!({ "type": "recorders.state", "recorders": rows })
}

/// What the recorder said about why it stopped, when it said anything.
async fn stop_error(state: &AppState, target: &RecordTarget) -> Option<String> {
    match target {
        RecordTarget::BlackmagicCamera => state
            .blackmagic_camera_connector
            .get_state()
            .await
            .filter(|camera| !camera.has_media)
            .map(|_| "camera_no_media".to_string()),
        RecordTarget::RodecasterAudio => state.rodecaster_audio_recorder.state().error,
        _ => None,
    }
}

/// Loads the button's recorders and, once a second, pushes `recorders.state` when a row
/// changed and `recorder.stopped` for each recording that ended.
pub async fn spawn(state: AppState) {
    let button: RecordButton = crate::database::settings::get_json(&state.pool, SETTING).await;
    state.recorders.select(button.targets);
    tokio::spawn(async move {
        let mut shown = rows(&state).await;
        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            ticker.tick().await;
            let now = rows(&state).await;
            if now == shown {
                continue;
            }
            for (target, available) in stopped(&shown, &now) {
                let reason = if state.recorders.asked_to_stop(target) {
                    StopReason::Metocast
                } else if available {
                    StopReason::Device
                } else {
                    StopReason::Disconnected
                };
                let error = stop_error(&state, target).await;
                tracing::info!(?target, ?reason, ?error, "recorders: a recording stopped");
                let message = json!({
                    "type": "recorder.stopped",
                    "target": target,
                    "reason": reason,
                    "error": error,
                });
                broadcast_json(&state.ws_clients, &message).await;
            }
            broadcast_json(&state.ws_clients, &state_message(&now)).await;
            shown = now;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(target: RecordTarget, available: bool, recording: bool) -> RecorderRow {
        RecorderRow {
            target,
            selected: false,
            available,
            recording,
            last_start: None,
        }
    }

    #[test]
    fn a_recording_that_ends_or_whose_device_goes_away_is_reported_once() {
        let camera = RecordTarget::BlackmagicCamera;
        let obs = RecordTarget::Obs;
        let one_camera = RecordTarget::Middlecontrol { camera_id: Some(2) };
        let before = [
            row(camera.clone(), true, true),
            row(obs.clone(), true, true),
            row(one_camera.clone(), true, true),
        ];
        // The camera stopped on its own, OBS dropped off, Middle Control's camera 2 is its
        // own row's business.
        let now = [
            row(camera.clone(), true, false),
            row(obs.clone(), false, false),
            row(one_camera, true, false),
        ];
        assert_eq!(stopped(&before, &now), vec![(&camera, true), (&obs, false)]);
        // Still recording, or never was: nothing to say.
        assert!(stopped(&before, &before).is_empty());
        assert!(stopped(&now, &before).is_empty());
    }
}
