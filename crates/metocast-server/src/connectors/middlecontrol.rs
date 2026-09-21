use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use futures_util::{StreamExt, stream};
use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{Mutex, RwLock, broadcast, oneshot, watch};
use tokio::time::{Duration, timeout};

use super::{ConnectorConfig, ConnectorStatus, MiddlecontrolConfig};

const INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const DISCOVERY_PORTS: [u16; 3] = [11584, 11581, 11580];
const DISCOVERY_CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
const DISCOVERY_FEEDBACK_TIMEOUT: Duration = Duration::from_millis(1200);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredMiddlecontrol {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MiddlecontrolState {
    pub selected_camera: Option<u8>,
    pub recording: Option<bool>,
    pub recording_camera_ids: Option<Vec<u8>>,
    pub connected_camera_ids: Option<Vec<u8>>,
    pub connected_apcr_ids: Option<Vec<u8>>,
    pub preset_move_active: Option<bool>,
}

pub struct MiddlecontrolConnector {
    pub status: Arc<RwLock<ConnectorStatus>>,
    pub state: Arc<RwLock<Option<MiddlecontrolState>>>,
    pub writer: Arc<Mutex<Option<OwnedWriteHalf>>>,
    pub status_tx: broadcast::Sender<ConnectorStatus>,
    pub state_tx: broadcast::Sender<MiddlecontrolState>,
    reconnect_tx: broadcast::Sender<String>,
    stop_tx: Mutex<Option<(watch::Sender<bool>, oneshot::Receiver<()>)>>,
}

impl MiddlecontrolConnector {
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        let (state_tx, _) = broadcast::channel(16);
        let (reconnect_tx, _) = broadcast::channel(1);
        Self {
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            state: Arc::new(RwLock::new(None)),
            writer: Arc::new(Mutex::new(None)),
            status_tx,
            state_tx,
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
            reconnect_rx: self.reconnect_tx.subscribe(),
        };
        tokio::spawn(async move {
            worker.run(stop_rx).await;
            let _ = done_tx.send(());
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
            let _ = self.reconnect_tx.send(message.clone());
            return Err(message);
        }
        Ok(())
    }

    async fn stop_internal(&self) {
        let control = self.stop_tx.lock().await.take();
        if let Some((tx, done_rx)) = control {
            let _ = tx.send(true);
            let _ = done_rx.await;
        }
    }

    async fn clear_connection(&self) {
        *self.writer.lock().await = None;
        *self.state.write().await = None;
    }

    async fn set_status(&self, new_status: ConnectorStatus) {
        let mut status = self.status.write().await;
        if *status != new_status {
            *status = new_status.clone();
            let _ = self.status_tx.send(new_status);
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
    reconnect_rx: broadcast::Receiver<String>,
}

impl Worker {
    async fn run(mut self, mut stop_rx: watch::Receiver<bool>) {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            self.set_status(ConnectorStatus::Connecting).await;
            let (ended, was_connected) = self.session(&mut stop_rx).await;
            *self.writer.lock().await = None;
            *self.state.write().await = None;

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
                result = stop_rx.changed() => {
                    let _ = result;
                    self.set_status(ConnectorStatus::Disconnected).await;
                    return;
                }
            }
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }

    async fn session(&mut self, stop_rx: &mut watch::Receiver<bool>) -> (Ended, bool) {
        let stream = match TcpStream::connect((&*self.config.host, self.config.port)).await {
            Ok(stream) => stream,
            Err(error) => return (Ended::ByError(error.to_string()), false),
        };
        let (read_half, write_half) = stream.into_split();
        *self.writer.lock().await = Some(write_half);
        let mut lines = BufReader::new(read_half).lines();
        let mut was_connected = false;

        loop {
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(line)) => {
                        let Some(state) = parse_feedback_frame(&line) else { continue };
                        if !was_connected {
                            was_connected = true;
                            self.set_status(ConnectorStatus::Connected).await;
                        }
                        self.apply_state(state).await;
                    }
                    Ok(None) => return (Ended::ByError("Middle Control closed the connection".to_string()), was_connected),
                    Err(error) => return (Ended::ByError(error.to_string()), was_connected),
                },
                result = self.reconnect_rx.recv() => {
                    let message = result.unwrap_or_else(|_| "Middle Control command write failed".to_string());
                    return (Ended::ByError(message), was_connected);
                }
                result = stop_rx.changed() => {
                    let _ = result;
                    return (Ended::ByCaller, was_connected);
                }
            }
        }
    }

    async fn apply_state(&self, new_state: MiddlecontrolState) {
        let mut state = self.state.write().await;
        if state.as_ref() != Some(&new_state) {
            *state = Some(new_state.clone());
            let _ = self.state_tx.send(new_state);
        }
    }

    async fn set_status(&self, new_status: ConnectorStatus) {
        let mut status = self.status.write().await;
        if *status != new_status {
            tracing::info!(connector = "middlecontrol", status = ?new_status, "connector status");
            *status = new_status.clone();
            let _ = self.status_tx.send(new_status);
        }
    }
}

fn parse_feedback_frame(line: &str) -> Option<MiddlecontrolState> {
    let frame = line.trim();
    let inner = frame.strip_prefix('{')?.strip_suffix('}')?;
    let mut state = MiddlecontrolState {
        selected_camera: None,
        recording: None,
        recording_camera_ids: None,
        connected_camera_ids: None,
        connected_apcr_ids: None,
        preset_move_active: None,
    };
    let mut recognized = false;

    for token in inner.split(';') {
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
            state.preset_move_active = parse_bool(value)?;
            recognized = true;
        } else if let Some(value) = token.strip_prefix("CAM") {
            state.selected_camera = parse_id(value)?;
            recognized = true;
        } else if let Some(value) = token.strip_prefix("REC") {
            state.recording = parse_bool(value)?;
            recognized = true;
        }
    }
    recognized.then_some(state)
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
        .unwrap();
        assert_eq!(state.selected_camera, Some(2));
        assert_eq!(state.recording, None);
        assert_eq!(state.recording_camera_ids, Some(vec![2, 5]));
        assert_eq!(state.connected_camera_ids, Some(vec![1, 2, 5]));
        assert_eq!(state.connected_apcr_ids, Some(vec![1]));
        assert_eq!(state.preset_move_active, Some(false));
    }

    #[test]
    fn accepts_empty_lists_absent_tokens_and_dashes() {
        let empty =
            parse_feedback_frame("{CAM5;REC_LIST[];CAM_CON_LIST[];APCR_CON_LIST[]}").unwrap();
        assert_eq!(empty.recording_camera_ids, Some(Vec::new()));
        assert_eq!(empty.connected_camera_ids, Some(Vec::new()));
        assert_eq!(empty.connected_apcr_ids, Some(Vec::new()));
        assert_eq!(empty.recording, None);

        let unavailable = parse_feedback_frame("{CAM-;REC-;PRES_ACTIVE-}").unwrap();
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

    #[tokio::test]
    async fn state_is_broadcast_only_when_it_changes() {
        let (status_tx, _) = broadcast::channel(1);
        let (state_tx, mut state_rx) = broadcast::channel(2);
        let (_reconnect_tx, reconnect_rx) = broadcast::channel(1);
        let worker = Worker {
            config: MiddlecontrolConfig::default(),
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            state: Arc::new(RwLock::new(None)),
            writer: Arc::new(Mutex::new(None)),
            status_tx,
            state_tx,
            reconnect_rx,
        };
        let state = parse_feedback_frame("{CAM1;REC_LIST[]}").unwrap();
        worker.apply_state(state.clone()).await;
        worker.apply_state(state.clone()).await;

        assert_eq!(state_rx.try_recv().unwrap(), state);
        assert!(matches!(
            state_rx.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
    }
}
