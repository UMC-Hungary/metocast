use std::sync::Arc;

use crate::HostHandle;
use futures_util::StreamExt;
use obws::events::Event;
use tokio::sync::{Mutex, RwLock, broadcast, watch};
use tokio::time::Duration;

use super::{ConnectorStatus, ObsConfig};

/// Emitted by the OBS connector when a recording stops.
#[derive(Clone)]
pub struct ObsRecordingEvent {
    pub output_path: std::path::PathBuf,
}

/// Emitted whenever OBS streaming or recording state changes.
#[derive(Clone)]
pub struct ObsStateEvent {
    pub is_streaming: bool,
    pub is_recording: bool,
}

/// Snapshot of OBS output states, broadcast whenever either changes.
#[derive(Debug, Clone, Copy)]
pub struct ObsOutputState {
    pub is_streaming: bool,
    pub is_recording: bool,
}

pub struct ObsConnector {
    pub status: Arc<RwLock<ConnectorStatus>>,
    /// Last known streaming/recording state; `None` when OBS is disconnected.
    pub output_state: Arc<RwLock<Option<ObsOutputState>>>,
    /// Live OBS client; `None` when disconnected.
    pub client: Arc<Mutex<Option<Arc<obws::Client>>>>,
    /// Broadcast channel — subscribe to receive every connection status change.
    pub status_tx: broadcast::Sender<ConnectorStatus>,
    /// Broadcast channel — subscribe to receive OBS recording-stopped events.
    pub recording_tx: broadcast::Sender<ObsRecordingEvent>,
    /// Broadcast channel — subscribe to receive streaming/recording state changes (obs.state).
    pub state_tx: broadcast::Sender<ObsStateEvent>,
    /// Broadcast channel — subscribe to receive streaming/recording state changes (connector.state).
    pub output_state_tx: broadcast::Sender<ObsOutputState>,
    /// Broadcast channel — fires when a device rescan should be triggered.
    pub devices_tx: broadcast::Sender<()>,
    stop_tx: Mutex<Option<watch::Sender<bool>>>,
}

struct ObsLoop {
    config: ObsConfig,
    host: Option<HostHandle>,
    status: Arc<RwLock<ConnectorStatus>>,
    output_state: Arc<RwLock<Option<ObsOutputState>>>,
    client: Arc<Mutex<Option<Arc<obws::Client>>>>,
    status_tx: broadcast::Sender<ConnectorStatus>,
    recording_tx: broadcast::Sender<ObsRecordingEvent>,
    state_tx: broadcast::Sender<ObsStateEvent>,
    output_state_tx: broadcast::Sender<ObsOutputState>,
    devices_tx: broadcast::Sender<()>,
    stop_rx: watch::Receiver<bool>,
}

impl ObsConnector {
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        let (recording_tx, _) = broadcast::channel(16);
        let (state_tx, _) = broadcast::channel(16);
        let (output_state_tx, _) = broadcast::channel(16);
        let (devices_tx, _) = broadcast::channel(8);
        Self {
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            output_state: Arc::new(RwLock::new(None)),
            client: Arc::new(Mutex::new(None)),
            status_tx,
            recording_tx,
            state_tx,
            output_state_tx,
            devices_tx,
            stop_tx: Mutex::new(None),
        }
    }

    pub async fn start(&self, config: ObsConfig, host: Option<HostHandle>) {
        tracing::info!(host = %config.host, port = config.port, "obs: connector start requested");
        self.stop_internal().await;

        let (stop_tx, stop_rx) = watch::channel(false);
        *self.stop_tx.lock().await = Some(stop_tx);

        let obs_loop = ObsLoop {
            config,
            host,
            status: Arc::clone(&self.status),
            output_state: Arc::clone(&self.output_state),
            client: Arc::clone(&self.client),
            status_tx: self.status_tx.clone(),
            recording_tx: self.recording_tx.clone(),
            state_tx: self.state_tx.clone(),
            output_state_tx: self.output_state_tx.clone(),
            devices_tx: self.devices_tx.clone(),
            stop_rx,
        };
        tokio::spawn(async move {
            run_obs_loop(obs_loop).await;
        });
    }

    pub async fn get_output_state(&self) -> Option<ObsOutputState> {
        *self.output_state.read().await
    }

    pub async fn stop(&self) {
        tracing::info!("obs: connector stop requested");
        self.stop_internal().await;
    }

    async fn stop_internal(&self) {
        let mut guard = self.stop_tx.lock().await;
        if let Some(tx) = guard.take() {
            let _ = tx.send(true);
        }
    }

    pub async fn get_status(&self) -> ConnectorStatus {
        self.status.read().await.clone()
    }
}

impl Default for ObsConnector {
    fn default() -> Self {
        Self::new()
    }
}

async fn set_status(
    status: &Arc<RwLock<ConnectorStatus>>,
    status_tx: &broadcast::Sender<ConnectorStatus>,
    host: Option<&HostHandle>,
    new_status: ConnectorStatus,
) {
    tracing::info!(connector = "obs", status = ?new_status, "connector status");
    *status.write().await = new_status;
    let current = status.read().await.clone();
    let _ = status_tx.send(current.clone());
    if let Some(host) = host
        && let Err(e) = crate::emit(host, "connector://obs-status", &current)
    {
        tracing::warn!("Failed to emit OBS status: {e}");
    }
}

async fn run_obs_loop(obs: ObsLoop) {
    let ObsLoop {
        config,
        host,
        status,
        output_state,
        client: client_arc,
        status_tx,
        recording_tx,
        state_tx,
        output_state_tx,
        devices_tx,
        mut stop_rx,
    } = obs;
    let mut backoff = Duration::from_secs(5);
    loop {
        set_status(
            &status,
            &status_tx,
            host.as_ref(),
            ConnectorStatus::Connecting,
        )
        .await;

        let connect_result = tokio::select! {
            result = obws::Client::connect(
                config.host.as_str(),
                config.port,
                config.password.as_deref(),
            ) => result,
            result = stop_rx.changed() => {
                let _ = result;
                set_status(&status, &status_tx, host.as_ref(), ConnectorStatus::Disconnected).await;
                return;
            }
        };

        match connect_result {
            Ok(raw_client) => {
                let client = Arc::new(raw_client);
                *client_arc.lock().await = Some(Arc::clone(&client));
                backoff = Duration::from_secs(5);
                set_status(
                    &status,
                    &status_tx,
                    host.as_ref(),
                    ConnectorStatus::Connected,
                )
                .await;

                // Query initial streaming/recording state.
                let initial = query_output_state(&client).await;
                *output_state.write().await = Some(initial);
                let _ = output_state_tx.send(initial);
                let _ = state_tx.send(ObsStateEvent {
                    is_streaming: initial.is_streaming,
                    is_recording: initial.is_recording,
                });
                // Trigger initial device scan now that OBS is connected.
                let _ = devices_tx.send(());
                let current_output = Arc::clone(&output_state);

                match client.events() {
                    Ok(events) => {
                        futures_util::pin_mut!(events);

                        let mut is_streaming = initial.is_streaming;
                        let mut is_recording = initial.is_recording;

                        'events: loop {
                            tokio::select! {
                                maybe_event = events.next() => {
                                    match maybe_event {
                                        None => break 'events,
                                        Some(event) => {
                                            match &event {
                                                obws::events::Event::RecordStateChanged { state, path, .. } => {
                                                    match state {
                                                        obws::events::OutputState::Started => {
                                                            is_recording = true;
                                                            let _ = state_tx.send(ObsStateEvent { is_streaming, is_recording });
                                                        }
                                                        obws::events::OutputState::Stopped => {
                                                            is_recording = false;
                                                            let _ = state_tx.send(ObsStateEvent { is_streaming, is_recording });
                                                            if let Some(p) = path {
                                                                let _ = recording_tx.send(ObsRecordingEvent {
                                                                    output_path: std::path::PathBuf::from(p),
                                                                });
                                                            }
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                                obws::events::Event::StreamStateChanged { state, .. } => {
                                                    match state {
                                                        obws::events::OutputState::Started => {
                                                            is_streaming = true;
                                                            let _ = state_tx.send(ObsStateEvent { is_streaming, is_recording });
                                                        }
                                                        obws::events::OutputState::Stopped => {
                                                            is_streaming = false;
                                                            let _ = state_tx.send(ObsStateEvent { is_streaming, is_recording });
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                                obws::events::Event::InputCreated { .. }
                                                | obws::events::Event::InputRemoved { .. }
                                                | obws::events::Event::InputSettingsChanged { .. } => {
                                                    let _ = devices_tx.send(());
                                                }
                                                _ => {}
                                            }
                                            handle_event(&event, &current_output, &output_state_tx).await;
                                        }
                                    }
                                }
                                result = stop_rx.changed() => {
                                    let _ = result;
                                    let _ = state_tx.send(ObsStateEvent { is_streaming: false, is_recording: false });
                                    *client_arc.lock().await = None;
                                    *output_state.write().await = None;
                                    set_status(&status, &status_tx, host.as_ref(), ConnectorStatus::Disconnected).await;
                                    return;
                                }
                            }
                        }
                        // Connection dropped by OBS — reset state
                        let _ = state_tx.send(ObsStateEvent {
                            is_streaming: false,
                            is_recording: false,
                        });
                        *client_arc.lock().await = None;
                        *output_state.write().await = None;
                        set_status(
                            &status,
                            &status_tx,
                            host.as_ref(),
                            ConnectorStatus::Disconnected,
                        )
                        .await;
                    }
                    Err(e) => {
                        *client_arc.lock().await = None;
                        set_status(
                            &status,
                            &status_tx,
                            host.as_ref(),
                            ConnectorStatus::Error {
                                message: e.to_string(),
                            },
                        )
                        .await;
                    }
                }
            }
            Err(e) => {
                set_status(
                    &status,
                    &status_tx,
                    host.as_ref(),
                    ConnectorStatus::Error {
                        message: e.to_string(),
                    },
                )
                .await;
            }
        }

        if *stop_rx.borrow() {
            *client_arc.lock().await = None;
            *output_state.write().await = None;
            set_status(
                &status,
                &status_tx,
                host.as_ref(),
                ConnectorStatus::Disconnected,
            )
            .await;
            return;
        }

        tracing::info!(seconds = backoff.as_secs(), "obs: waiting before retry");
        tokio::select! {
            () = tokio::time::sleep(backoff) => {}
            result = stop_rx.changed() => {
                let _ = result;
                set_status(&status, &status_tx, host.as_ref(), ConnectorStatus::Disconnected).await;
                return;
            }
        }
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

async fn query_output_state(client: &obws::Client) -> ObsOutputState {
    let is_streaming = client
        .streaming()
        .status()
        .await
        .map(|s| s.active)
        .unwrap_or(false);
    let is_recording = client
        .recording()
        .status()
        .await
        .map(|s| s.active)
        .unwrap_or(false);
    ObsOutputState {
        is_streaming,
        is_recording,
    }
}

async fn handle_event(
    event: &Event,
    current: &Arc<RwLock<Option<ObsOutputState>>>,
    output_state_tx: &broadcast::Sender<ObsOutputState>,
) {
    let updated = match event {
        Event::StreamStateChanged { active, .. } => {
            let mut guard = current.write().await;
            let state = guard.get_or_insert(ObsOutputState {
                is_streaming: false,
                is_recording: false,
            });
            state.is_streaming = *active;
            Some(*state)
        }
        Event::RecordStateChanged { active, .. } => {
            let mut guard = current.write().await;
            let state = guard.get_or_insert(ObsOutputState {
                is_streaming: false,
                is_recording: false,
            });
            state.is_recording = *active;
            Some(*state)
        }
        _ => None,
    };
    if let Some(state) = updated {
        let _ = output_state_tx.send(state);
    }
}
