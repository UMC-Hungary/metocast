use metocast_core::best_effort::LogGone;
use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;

use chrono::{SecondsFormat, Utc};
use futures_util::{StreamExt, stream};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{Mutex, RwLock, broadcast, oneshot, watch};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use super::preset_tracker::{Motion, Observation, PresetEvent, PresetTracker, Recall};
use super::{
    ConnectorConfig, ConnectorStatus, LastPreset, MiddlecontrolConfig, MiddlecontrolState,
};

const INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Without it a host that drops packets holds the attempt for the OS's ~75 s.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const DISCOVERY_PORTS: [u16; 3] = [11584, 11581, 11580];
const DISCOVERY_CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
const DISCOVERY_FEEDBACK_TIMEOUT: Duration = Duration::from_millis(1200);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredMiddlecontrol {
    pub host: String,
    pub port: u16,
}

pub struct MiddlecontrolConnector {
    pub status: Arc<RwLock<ConnectorStatus>>,
    pub state: Arc<RwLock<Option<MiddlecontrolState>>>,
    pub writer: Arc<Mutex<Option<OwnedWriteHalf>>>,
    pub status_tx: broadcast::Sender<ConnectorStatus>,
    pub state_tx: broadcast::Sender<MiddlecontrolState>,
    /// Preset moves Metocast watched start and run to their end, wherever they were clicked.
    pub preset_tx: broadcast::Sender<PresetEvent>,
    motion_tx: Arc<watch::Sender<Motion>>,
    tracker: Arc<std::sync::Mutex<PresetTracker>>,
    reconnect_tx: broadcast::Sender<String>,
    stop_tx: Mutex<Option<(watch::Sender<bool>, oneshot::Receiver<()>)>>,
}

impl MiddlecontrolConnector {
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        let (state_tx, _) = broadcast::channel(16);
        let (preset_tx, _) = broadcast::channel(32);
        let (reconnect_tx, _) = broadcast::channel(1);
        Self {
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            state: Arc::new(RwLock::new(None)),
            writer: Arc::new(Mutex::new(None)),
            status_tx,
            state_tx,
            preset_tx,
            motion_tx: Arc::new(watch::channel(Motion::default()).0),
            tracker: Arc::new(std::sync::Mutex::new(PresetTracker::new())),
            reconnect_tx,
            stop_tx: Mutex::new(None),
        }
    }

    pub async fn start(&self, config: MiddlecontrolConfig) {
        self.stop_internal().await;
        if !config.is_configured() {
            self.clear_connection().await;
            self.set_status(ConnectorStatus::Disconnected).await;
            return;
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        let (done_tx, done_rx) = oneshot::channel();
        *self.stop_tx.lock().await = Some((stop_tx, done_rx));
        let worker = Worker {
            config,
            status: Arc::clone(&self.status),
            state: Arc::clone(&self.state),
            writer: Arc::clone(&self.writer),
            status_tx: self.status_tx.clone(),
            state_tx: self.state_tx.clone(),
            preset_tx: self.preset_tx.clone(),
            motion_tx: Arc::clone(&self.motion_tx),
            tracker: Arc::clone(&self.tracker),
            reconnect_rx: self.reconnect_tx.subscribe(),
        };
        tokio::spawn(async move {
            worker.run(stop_rx).await;
            done_tx.send(()).log_gone();
        });
    }

    pub async fn stop(&self) {
        self.stop_internal().await;
        self.clear_connection().await;
        self.set_status(ConnectorStatus::Disconnected).await;
    }

    pub async fn get_status(&self) -> ConnectorStatus {
        self.status.read().await.clone()
    }

    pub async fn get_state(&self) -> Option<MiddlecontrolState> {
        self.state.read().await.clone()
    }

    pub async fn select_camera(&self, camera_id: u8) -> Result<(), String> {
        self.write_command(select_camera_command(camera_id)?).await
    }

    pub async fn start_recording(&self, camera_id: Option<u8>) -> Result<(), String> {
        self.write_command(recording_command(true, camera_id)?)
            .await
    }

    pub async fn stop_recording(&self, camera_id: Option<u8>) -> Result<(), String> {
        self.write_command(recording_command(false, camera_id)?)
            .await
    }

    pub async fn start_recording_all(&self) -> Result<(), String> {
        self.write_command(recording_all_command(true)).await
    }

    pub async fn stop_recording_all(&self) -> Result<(), String> {
        self.write_command(recording_all_command(false)).await
    }

    pub async fn recall_preset(&self, preset: u8, camera_id: Option<u8>) -> Result<(), String> {
        self.write_command(preset_command(preset, camera_id)?).await
    }

    /// Recalls a preset for the rules in `chain`: the move it causes is reported with them, so
    /// a rule that move triggers can tell it was set off by those rules.
    pub async fn recall_preset_for(
        &self,
        preset: u8,
        camera_id: Option<u8>,
        chain: Vec<Uuid>,
    ) -> Result<(), String> {
        let command = preset_command(preset, camera_id)?;
        self.tracker.lock().expect("tracker lock").note_recall(
            Recall {
                preset,
                camera_id,
                chain,
            },
            Instant::now(),
        );
        let result = self.write_command(command).await;
        if result.is_err() {
            self.tracker.lock().expect("tracker lock").forget_recall();
        }
        result
    }

    /// What the frames last said about the selected camera's position and any move; the
    /// default while Middle Control is not connected.
    #[must_use]
    pub fn motion(&self) -> watch::Receiver<Motion> {
        self.motion_tx.subscribe()
    }

    #[must_use]
    pub fn current_motion(&self) -> Motion {
        *self.motion_tx.borrow()
    }

    async fn write_command(&self, command: String) -> Result<(), String> {
        if !matches!(*self.status.read().await, ConnectorStatus::Connected) {
            return Err("middlecontrol_not_connected".to_string());
        }
        let result = match self.writer.lock().await.as_mut() {
            Some(writer) => writer.write_all(command.as_bytes()).await,
            None => return Err("middlecontrol_not_connected".to_string()),
        };
        if let Err(error) = result {
            let message = format!("Middle Control write failed: {error}");
            *self.writer.lock().await = None;
            self.set_status(ConnectorStatus::Error {
                message: message.clone(),
            })
            .await;
            self.reconnect_tx.send(message.clone()).log_gone();
            return Err(message);
        }
        Ok(())
    }

    async fn stop_internal(&self) {
        let control = self.stop_tx.lock().await.take();
        if let Some((tx, done_rx)) = control {
            tx.send_replace(true);
            done_rx.await.log_gone();
        }
    }

    async fn clear_connection(&self) {
        *self.writer.lock().await = None;
        *self.state.write().await = None;
        self.tracker.lock().expect("tracker lock").reset();
        self.motion_tx.send_replace(Motion::default());
    }

    async fn set_status(&self, new_status: ConnectorStatus) {
        let mut status = self.status.write().await;
        if *status != new_status {
            *status = new_status.clone();
            self.status_tx.send(new_status).log_gone();
        }
    }
}

impl Default for MiddlecontrolConnector {
    fn default() -> Self {
        Self::new()
    }
}

/// Looks for Middle Control on localhost and the server's local /24 network.
/// A TCP listener only counts when it sends a valid feedback frame.
pub async fn discover() -> Vec<DiscoveredMiddlecontrol> {
    let probes = discovery_hosts()
        .into_iter()
        .flat_map(|host| DISCOVERY_PORTS.map(|port| (host, port)));

    let mut found: Vec<_> = stream::iter(probes)
        .map(|(host, port)| async move {
            probe_endpoint(host, port)
                .await
                .then(|| DiscoveredMiddlecontrol {
                    host: if host.is_loopback() {
                        "localhost".to_string()
                    } else {
                        host.to_string()
                    },
                    port,
                })
        })
        .buffer_unordered(64)
        .filter_map(|endpoint| async move { endpoint })
        .collect()
        .await;
    found.sort_by(|left, right| (&left.host, left.port).cmp(&(&right.host, right.port)));
    found
}

fn discovery_hosts() -> Vec<Ipv4Addr> {
    discovery_hosts_for(&crate::broadlink::get_local_ipv4_addresses())
}

fn discovery_hosts_for(local_addresses: &[Ipv4Addr]) -> Vec<Ipv4Addr> {
    let mut hosts = BTreeSet::from([Ipv4Addr::LOCALHOST]);
    // ponytail: a /24 scan covers normal booth LANs; manual host entry covers routed/VLAN networks.
    for local in local_addresses {
        let [a, b, c, _] = local.octets();
        hosts.extend((1..=254).map(|last| Ipv4Addr::new(a, b, c, last)));
    }
    hosts.into_iter().collect()
}

async fn probe_endpoint(host: Ipv4Addr, port: u16) -> bool {
    let address = SocketAddr::new(IpAddr::V4(host), port);
    let Ok(Ok(stream)) = timeout(DISCOVERY_CONNECT_TIMEOUT, TcpStream::connect(address)).await
    else {
        return false;
    };
    let mut lines = BufReader::new(stream).lines();
    timeout(DISCOVERY_FEEDBACK_TIMEOUT, async {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) if parse_feedback_frame(&line).is_some() => return true,
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return false,
            }
        }
    })
    .await
    .unwrap_or(false)
}

enum Ended {
    ByCaller,
    ByError(String),
}

struct Worker {
    config: MiddlecontrolConfig,
    status: Arc<RwLock<ConnectorStatus>>,
    state: Arc<RwLock<Option<MiddlecontrolState>>>,
    writer: Arc<Mutex<Option<OwnedWriteHalf>>>,
    status_tx: broadcast::Sender<ConnectorStatus>,
    state_tx: broadcast::Sender<MiddlecontrolState>,
    preset_tx: broadcast::Sender<PresetEvent>,
    motion_tx: Arc<watch::Sender<Motion>>,
    tracker: Arc<std::sync::Mutex<PresetTracker>>,
    reconnect_rx: broadcast::Receiver<String>,
}

impl Worker {
    async fn run(mut self, mut stop_rx: watch::Receiver<bool>) {
        let mut backoff = INITIAL_BACKOFF;
        // Only the first attempt shows Connecting: a retry keeps the last error on screen.
        self.set_status(ConnectorStatus::Connecting).await;
        loop {
            let (ended, was_connected) = self.session(&mut stop_rx).await;
            *self.writer.lock().await = None;
            *self.state.write().await = None;
            // A move already running when the connection comes back is not one whose start
            // was seen.
            self.tracker.lock().expect("tracker lock").reset();
            self.motion_tx.send_replace(Motion::default());

            match ended {
                Ended::ByCaller => {
                    self.set_status(ConnectorStatus::Disconnected).await;
                    return;
                }
                Ended::ByError(message) => {
                    self.set_status(ConnectorStatus::Error { message }).await;
                }
            }

            if was_connected {
                backoff = INITIAL_BACKOFF;
            }
            tokio::select! {
                () = tokio::time::sleep(backoff) => {}
                _ = stop_rx.changed() => {
                    self.set_status(ConnectorStatus::Disconnected).await;
                    return;
                }
            }
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    async fn session(&mut self, stop_rx: &mut watch::Receiver<bool>) -> (Ended, bool) {
        let connect = TcpStream::connect((&*self.config.host, self.config.port));
        let stream = match timeout(CONNECT_TIMEOUT, connect).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(error)) => return (Ended::ByError(error.to_string()), false),
            Err(_) => return (Ended::ByError("middlecontrol_timeout".to_string()), false),
        };
        let (read_half, write_half) = stream.into_split();
        *self.writer.lock().await = Some(write_half);
        let mut lines = BufReader::new(read_half).lines();
        let mut was_connected = false;

        loop {
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(line)) => {
                        let Some(frame) = parse_feedback_frame(&line) else { continue };
                        if !was_connected {
                            was_connected = true;
                            self.set_status(ConnectorStatus::Connected).await;
                        }
                        self.apply_frame(frame).await;
                    }
                    Ok(None) => return (Ended::ByError("Middle Control closed the connection".to_string()), was_connected),
                    Err(error) => return (Ended::ByError(error.to_string()), was_connected),
                },
                result = self.reconnect_rx.recv() => {
                    let message = result.unwrap_or_else(|_| "Middle Control command write failed".to_string());
                    return (Ended::ByError(message), was_connected);
                }
                _ = stop_rx.changed() => {
                    return (Ended::ByCaller, was_connected);
                }
            }
        }
    }

    async fn apply_frame(&self, frame: Frame) {
        let events = self.tracker.lock().expect("tracker lock").observe(
            &frame.observation(),
            Instant::now(),
            Utc::now(),
        );
        self.motion_tx.send_replace(frame.motion());

        let mut new_state = frame.state;
        let mut state = self.state.write().await;
        new_state.last_preset = state.as_ref().and_then(|state| state.last_preset.clone());
        for event in &events {
            if let PresetEvent::Finished(finished) = event {
                new_state.last_preset = Some(LastPreset {
                    preset: finished.preset,
                    camera_id: finished.camera_id,
                    at: finished.at.to_rfc3339_opts(SecondsFormat::Millis, true),
                });
            }
        }
        if state.as_ref() != Some(&new_state) {
            *state = Some(new_state.clone());
            self.state_tx.send(new_state).log_gone();
        }
        drop(state);
        for event in events {
            self.preset_tx.send(event).log_gone();
        }
    }

    async fn set_status(&self, new_status: ConnectorStatus) {
        let mut status = self.status.write().await;
        if *status != new_status {
            tracing::info!(connector = "middlecontrol", status = ?new_status, "connector status");
            *status = new_status.clone();
            self.status_tx.send(new_status).log_gone();
        }
    }
}

/// One feedback frame: the public state, and what only preset tracking needs.
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    state: MiddlecontrolState,
    /// `PRES_ACTIVE`: `Some(0)` at rest, `Some(n)` while preset `n` runs.
    active: Option<u8>,
    /// `PRES_D`: how long a preset move takes, in seconds.
    preset_seconds: Option<f32>,
    /// `PRESETSEQ_RUN`: Middle Control's own preset sequence is running.
    sequence_running: Option<bool>,
    /// `aPAN` and `aTILT`, in tenths of a degree; absent without an APC-R.
    pan: Option<i32>,
    tilt: Option<i32>,
}

impl Frame {
    fn observation(&self) -> Observation {
        Observation {
            camera: self.state.selected_camera,
            active: self.active,
            preset_seconds: self.preset_seconds,
            sequence_running: self.sequence_running,
        }
    }

    fn motion(&self) -> Motion {
        Motion {
            camera: self.state.selected_camera,
            active: self.active,
            preset_seconds: self.preset_seconds,
            pan: self.pan,
            tilt: self.tilt,
        }
    }
}

fn parse_feedback_frame(line: &str) -> Option<Frame> {
    let frame = line.trim();
    let inner = frame.strip_prefix('{')?.strip_suffix('}')?;
    let mut parsed = Frame {
        state: MiddlecontrolState {
            selected_camera: None,
            recording: None,
            recording_camera_ids: None,
            connected_camera_ids: None,
            connected_apcr_ids: None,
            preset_move_active: None,
            running_preset: None,
            last_preset: None,
        },
        active: None,
        preset_seconds: None,
        sequence_running: None,
        pan: None,
        tilt: None,
    };
    let mut recognized = false;

    for token in inner.split(';') {
        let state = &mut parsed.state;
        if let Some(value) = token.strip_prefix("REC_LIST") {
            state.recording_camera_ids = parse_id_list(value)?;
            recognized = true;
        } else if let Some(value) = token.strip_prefix("CAM_CON_LIST") {
            state.connected_camera_ids = parse_id_list(value)?;
            recognized = true;
        } else if let Some(value) = token.strip_prefix("APCR_CON_LIST") {
            state.connected_apcr_ids = parse_id_list(value)?;
            recognized = true;
        } else if let Some(value) = token.strip_prefix("PRES_ACTIVE") {
            // Never worth dropping the frame over: it also carries the camera and the
            // recording state, which froze while this was parsed as a flag.
            parsed.active = value.parse().ok();
            recognized = true;
        } else if let Some(value) = token.strip_prefix("PRES_D") {
            parsed.preset_seconds = value
                .parse::<f32>()
                .ok()
                .filter(|seconds| seconds.is_finite() && *seconds >= 0.0);
        } else if let Some(value) = token.strip_prefix("PRESETSEQ_RUN") {
            parsed.sequence_running = match value {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            };
        } else if let Some(value) = token.strip_prefix("aPAN") {
            parsed.pan = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("aTILT") {
            parsed.tilt = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("CAM") {
            state.selected_camera = parse_id(value)?;
            recognized = true;
        } else if let Some(value) = token.strip_prefix("REC") {
            state.recording = parse_bool(value)?;
            recognized = true;
        }
    }
    parsed.state.running_preset = parsed.active.filter(|preset| (1..=12).contains(preset));
    parsed.state.preset_move_active = parsed.active.map(|preset| preset != 0);
    recognized.then_some(parsed)
}

fn parse_id(value: &str) -> Option<Option<u8>> {
    if value == "-" {
        return Some(None);
    }
    value
        .parse::<u8>()
        .ok()
        .filter(|id| (1..=99).contains(id))
        .map(Some)
}

fn parse_bool(value: &str) -> Option<Option<bool>> {
    match value {
        "0" => Some(Some(false)),
        "1" => Some(Some(true)),
        "-" => Some(None),
        _ => None,
    }
}

fn parse_id_list(value: &str) -> Option<Option<Vec<u8>>> {
    if value == "-" {
        return Some(None);
    }
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    if inner.is_empty() {
        return Some(Some(Vec::new()));
    }
    inner
        .split(',')
        .map(|id| parse_id(id).and_then(|id| id))
        .collect::<Option<Vec<_>>>()
        .map(Some)
}

fn validate_camera_id(camera_id: u8) -> Result<u8, String> {
    (1..=99)
        .contains(&camera_id)
        .then_some(camera_id)
        .ok_or_else(|| "camera_id must be between 1 and 99".to_string())
}

fn select_camera_command(camera_id: u8) -> Result<String, String> {
    Ok(format!("CAM{}\n", validate_camera_id(camera_id)?))
}

fn recording_command(start: bool, camera_id: Option<u8>) -> Result<String, String> {
    let action = if start { "START" } else { "STOP" };
    match camera_id {
        Some(id) => Ok(format!("REC_{action}@C{}\n", validate_camera_id(id)?)),
        None => Ok(format!("REC_{action}\n")),
    }
}

fn recording_all_command(start: bool) -> String {
    format!("REC_{}_ALL\n", if start { "START" } else { "STOP" })
}

fn preset_command(preset: u8, camera_id: Option<u8>) -> Result<String, String> {
    if !(1..=12).contains(&preset) {
        return Err("preset must be between 1 and 12".to_string());
    }
    match camera_id {
        Some(id) => Ok(format!("PRESET{preset}@C{}\n", validate_camera_id(id)?)),
        None => Ok(format!("PRESET{preset}\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_observed_full_frame() {
        let state = parse_feedback_frame(
            "{CAM2;aWB5600;PRES_ACTIVE0;REC_LIST[2,5];CAM_CON_LIST[1,2,5];APCR_CON_LIST[1]}",
        )
        .unwrap()
        .state;
        assert_eq!(state.selected_camera, Some(2));
        assert_eq!(state.recording, None);
        assert_eq!(state.recording_camera_ids, Some(vec![2, 5]));
        assert_eq!(state.connected_camera_ids, Some(vec![1, 2, 5]));
        assert_eq!(state.connected_apcr_ids, Some(vec![1]));
        assert_eq!(state.preset_move_active, Some(false));
        assert_eq!(state.running_preset, None);
    }

    /// Frames as Middle Control 4.0.4 sends them on 11584 (tokens in the order it sends them,
    /// from the API page's example frame with the values captured on 2026-10-07).
    const MOVING: &str = "{CAM1;aWB5600;aSAT50;aCONT50;SHAKE_RUN0;ZLOOP_RUN0;PRESETSEQ_RUN0;\
        BRANG_RUN0;PRESETSEQ_BANK1;aPAN-32;aTILT0;aROLL0;aZOOM0;PTS85;ZS24;PRES_D8.0;\
        PRES_ACTIVE2;PRES_E50;REC_LIST[];CAM_CON_LIST[];APCR_CON_LIST[1]}";

    #[test]
    fn a_frame_while_preset_2_moves_is_read_not_dropped() {
        let frame = parse_feedback_frame(MOVING).unwrap();
        assert_eq!(frame.active, Some(2));
        assert_eq!(frame.state.running_preset, Some(2));
        assert_eq!(frame.state.preset_move_active, Some(true));
        assert_eq!(frame.state.selected_camera, Some(1));
        assert_eq!(frame.state.connected_apcr_ids, Some(vec![1]));
        assert_eq!(frame.preset_seconds, Some(8.0));
        assert_eq!(frame.sequence_running, Some(false));
        assert_eq!((frame.pan, frame.tilt), (Some(-32), Some(0)));
    }

    #[test]
    fn at_rest_there_is_no_running_preset() {
        let frame = parse_feedback_frame(&MOVING.replace("PRES_ACTIVE2", "PRES_ACTIVE0")).unwrap();
        assert_eq!(frame.active, Some(0));
        assert_eq!(frame.state.running_preset, None);
        assert_eq!(frame.state.preset_move_active, Some(false));
    }

    #[test]
    fn every_preset_number_is_read() {
        for preset in 1..=12 {
            let frame = parse_feedback_frame(
                &MOVING.replace("PRES_ACTIVE2", &format!("PRES_ACTIVE{preset}")),
            )
            .unwrap();
            assert_eq!(frame.state.running_preset, Some(preset));
        }
    }

    #[test]
    fn a_number_that_is_no_preset_still_counts_as_a_move_without_naming_one() {
        let frame = parse_feedback_frame(&MOVING.replace("PRES_ACTIVE2", "PRES_ACTIVE13")).unwrap();
        assert_eq!(frame.state.running_preset, None);
        assert_eq!(frame.state.preset_move_active, Some(true));
    }

    #[test]
    fn dashes_in_the_preset_tokens_mean_no_answer() {
        let frame =
            parse_feedback_frame("{CAM1;PRESETSEQ_RUN-;aPAN-;aTILT-;PRES_D-;PRES_ACTIVE-;PRES_E-}")
                .unwrap();
        assert_eq!(frame.active, None);
        assert_eq!(frame.state.running_preset, None);
        assert_eq!(frame.state.preset_move_active, None);
        assert_eq!(frame.preset_seconds, None);
        assert_eq!(frame.sequence_running, None);
        assert_eq!((frame.pan, frame.tilt), (None, None));
        assert_eq!(frame.state.selected_camera, Some(1));
    }

    /// The old parser read `PRES_ACTIVE` as a flag and threw the whole frame away for any
    /// other value, which froze Metocast's view of Middle Control while preset 2–12 moved.
    #[test]
    fn an_unreadable_preset_token_never_costs_the_rest_of_the_frame() {
        let frame = parse_feedback_frame(
            "{CAM3;REC_LIST[3];PRES_ACTIVEx;PRES_Dfast;PRESETSEQ_RUNmaybe;aPANleft;aTILTup}",
        )
        .unwrap();
        assert_eq!(frame.state.selected_camera, Some(3));
        assert_eq!(frame.state.recording_camera_ids, Some(vec![3]));
        assert_eq!(frame.active, None);
        assert_eq!(frame.preset_seconds, None);
    }

    #[test]
    fn a_negative_or_absurd_transition_time_is_no_answer() {
        for token in ["PRES_D-3.0", "PRES_Dinf", "PRES_DNaN"] {
            let frame = parse_feedback_frame(&format!("{{CAM1;{token}}}")).unwrap();
            assert_eq!(frame.preset_seconds, None, "{token}");
        }
    }

    #[test]
    fn accepts_empty_lists_absent_tokens_and_dashes() {
        let empty = parse_feedback_frame("{CAM5;REC_LIST[];CAM_CON_LIST[];APCR_CON_LIST[]}")
            .unwrap()
            .state;
        assert_eq!(empty.recording_camera_ids, Some(Vec::new()));
        assert_eq!(empty.connected_camera_ids, Some(Vec::new()));
        assert_eq!(empty.connected_apcr_ids, Some(Vec::new()));
        assert_eq!(empty.recording, None);

        let unavailable = parse_feedback_frame("{CAM-;REC-;PRES_ACTIVE-}")
            .unwrap()
            .state;
        assert_eq!(unavailable.selected_camera, None);
        assert_eq!(unavailable.recording, None);
        assert_eq!(unavailable.preset_move_active, None);
    }

    #[test]
    fn malformed_data_is_discarded() {
        assert!(parse_feedback_frame("{CAM2;REC_LIST[1,bad]}").is_none());
        assert!(parse_feedback_frame("{CAM2").is_none());
        assert!(parse_feedback_frame("{UNKNOWN1}").is_none());
        assert!(parse_feedback_frame("garbage").is_none());
    }

    #[test]
    fn discovery_scans_localhost_and_the_local_24() {
        let hosts = discovery_hosts_for(&[Ipv4Addr::new(192, 168, 1, 42)]);
        assert_eq!(hosts.first(), Some(&Ipv4Addr::LOCALHOST));
        assert!(hosts.contains(&Ipv4Addr::new(192, 168, 1, 1)));
        assert!(hosts.contains(&Ipv4Addr::new(192, 168, 1, 254)));
    }

    #[tokio::test]
    async fn discovery_requires_a_valid_feedback_frame() {
        async fn probe_with(payload: &'static [u8]) -> bool {
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                .await
                .unwrap();
            let port = listener.local_addr().unwrap().port();
            tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                stream.write_all(payload).await.unwrap();
            });
            probe_endpoint(Ipv4Addr::LOCALHOST, port).await
        }

        assert!(probe_with(b"{CAM1;REC_LIST[];CAM_CON_LIST[1]}\n").await);
        assert!(!probe_with(b"not-middle-control\n").await);
    }

    #[test]
    fn command_strings_are_exact_and_ranges_are_validated() {
        assert_eq!(select_camera_command(3).unwrap(), "CAM3\n");
        assert_eq!(recording_command(true, None).unwrap(), "REC_START\n");
        assert_eq!(recording_command(true, Some(2)).unwrap(), "REC_START@C2\n");
        assert_eq!(recording_command(false, None).unwrap(), "REC_STOP\n");
        assert_eq!(recording_command(false, Some(2)).unwrap(), "REC_STOP@C2\n");
        assert_eq!(recording_all_command(true), "REC_START_ALL\n");
        assert_eq!(recording_all_command(false), "REC_STOP_ALL\n");
        assert_eq!(preset_command(4, None).unwrap(), "PRESET4\n");
        assert_eq!(preset_command(1, Some(5)).unwrap(), "PRESET1@C5\n");
        assert!(select_camera_command(0).is_err());
        assert!(select_camera_command(100).is_err());
        assert!(recording_command(true, Some(0)).is_err());
        assert!(preset_command(0, None).is_err());
        assert!(preset_command(13, None).is_err());
    }

    fn worker() -> (
        Worker,
        broadcast::Receiver<MiddlecontrolState>,
        broadcast::Receiver<PresetEvent>,
    ) {
        let (status_tx, _) = broadcast::channel(1);
        let (state_tx, state_rx) = broadcast::channel(8);
        let (preset_tx, preset_rx) = broadcast::channel(8);
        let (_reconnect_tx, reconnect_rx) = broadcast::channel(1);
        let worker = Worker {
            config: MiddlecontrolConfig::default(),
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            state: Arc::new(RwLock::new(None)),
            writer: Arc::new(Mutex::new(None)),
            status_tx,
            state_tx,
            preset_tx,
            motion_tx: Arc::new(watch::channel(Motion::default()).0),
            tracker: Arc::new(std::sync::Mutex::new(PresetTracker::new())),
            reconnect_rx,
        };
        (worker, state_rx, preset_rx)
    }

    #[tokio::test]
    async fn state_is_broadcast_only_when_it_changes() {
        let (worker, mut state_rx, _) = worker();
        let frame = parse_feedback_frame("{CAM1;REC_LIST[]}").unwrap();
        worker.apply_frame(frame.clone()).await;
        worker.apply_frame(frame.clone()).await;

        assert_eq!(state_rx.try_recv().unwrap(), frame.state);
        assert!(matches!(
            state_rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn a_preset_move_is_published_as_state_events_and_the_last_preset() {
        let (worker, mut state_rx, mut preset_rx) = worker();
        // A 0.2 s transition, so the test need not wait eight seconds.
        let frame = |active: u8| {
            parse_feedback_frame(&format!("{{CAM1;PRES_D0.2;PRES_ACTIVE{active}}}")).unwrap()
        };
        worker.apply_frame(frame(0)).await;
        worker.apply_frame(frame(3)).await;
        tokio::time::sleep(Duration::from_millis(250)).await;
        worker.apply_frame(frame(0)).await;

        assert_eq!(state_rx.try_recv().unwrap().running_preset, None);
        let moving = state_rx.try_recv().unwrap();
        assert_eq!(moving.running_preset, Some(3));
        assert_eq!(moving.preset_move_active, Some(true));
        assert_eq!(moving.last_preset, None);
        let reached = state_rx.try_recv().unwrap();
        assert_eq!(reached.running_preset, None);
        let last = reached.last_preset.expect("the move finished");
        assert_eq!((last.preset, last.camera_id), (3, 1));

        assert!(matches!(
            preset_rx.try_recv(),
            Ok(PresetEvent::Started(start)) if (start.preset, start.camera_id) == (3, 1)
        ));
        assert!(matches!(
            preset_rx.try_recv(),
            Ok(PresetEvent::Finished(done)) if (done.preset, done.camera_id) == (3, 1)
        ));

        // The last move is kept while later frames only repeat the rest state.
        worker
            .apply_frame(parse_feedback_frame("{CAM1;PRES_D0.2;PRES_ACTIVE0;REC_LIST[1]}").unwrap())
            .await;
        assert_eq!(state_rx.try_recv().unwrap().last_preset, Some(last));
    }

    #[tokio::test]
    async fn the_motion_follows_every_frame_and_resets_with_the_connection() {
        let connector = MiddlecontrolConnector::new();
        let motion = connector.motion();
        assert_eq!(*motion.borrow(), Motion::default());

        let worker = Worker {
            config: MiddlecontrolConfig::default(),
            status: Arc::clone(&connector.status),
            state: Arc::clone(&connector.state),
            writer: Arc::clone(&connector.writer),
            status_tx: connector.status_tx.clone(),
            state_tx: connector.state_tx.clone(),
            preset_tx: connector.preset_tx.clone(),
            motion_tx: Arc::clone(&connector.motion_tx),
            tracker: Arc::clone(&connector.tracker),
            reconnect_rx: connector.reconnect_tx.subscribe(),
        };
        worker
            .apply_frame(parse_feedback_frame(MOVING).unwrap())
            .await;
        assert_eq!(
            *motion.borrow(),
            Motion {
                camera: Some(1),
                active: Some(2),
                preset_seconds: Some(8.0),
                pan: Some(-32),
                tilt: Some(0),
            }
        );

        connector.stop().await;
        assert_eq!(*motion.borrow(), Motion::default());
    }

    #[tokio::test(start_paused = true)]
    async fn a_failing_connection_shows_its_error_while_retrying() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let connector = MiddlecontrolConnector::new();
        let mut status_rx = connector.status_tx.subscribe();
        let worker = Worker {
            config: MiddlecontrolConfig {
                host: "127.0.0.1".to_string(),
                port,
                ..MiddlecontrolConfig::default()
            },
            status: Arc::clone(&connector.status),
            state: Arc::clone(&connector.state),
            writer: Arc::clone(&connector.writer),
            status_tx: connector.status_tx.clone(),
            state_tx: connector.state_tx.clone(),
            preset_tx: connector.preset_tx.clone(),
            motion_tx: Arc::clone(&connector.motion_tx),
            tracker: Arc::clone(&connector.tracker),
            reconnect_rx: connector.reconnect_tx.subscribe(),
        };
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(worker.run(stop_rx));
        // Several retries' worth of backoff.
        tokio::time::sleep(Duration::from_secs(300)).await;
        stop_tx.send(true).unwrap();
        task.await.unwrap();

        let mut seen = Vec::new();
        while let Ok(status) = status_rx.try_recv() {
            seen.push(status);
        }
        assert_eq!(seen.first(), Some(&ConnectorStatus::Connecting));
        assert!(matches!(seen.get(1), Some(ConnectorStatus::Error { .. })));
        assert_eq!(seen.last(), Some(&ConnectorStatus::Disconnected));
        assert!(
            !seen[1..].contains(&ConnectorStatus::Connecting),
            "{seen:?}"
        );
    }

    #[tokio::test]
    async fn a_recall_with_nothing_connected_fails_and_leaves_no_mark() {
        let connector = MiddlecontrolConnector::new();
        assert_eq!(
            connector
                .recall_preset_for(3, None, vec![Uuid::new_v4()])
                .await,
            Err("middlecontrol_not_connected".to_string())
        );
        assert_eq!(
            connector.recall_preset_for(13, None, Vec::new()).await,
            Err("preset must be between 1 and 12".to_string())
        );
    }
}
