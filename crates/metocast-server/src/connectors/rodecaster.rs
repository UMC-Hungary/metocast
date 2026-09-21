use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use rodecaster::{Device, Event, MuteChange, Profile};
use tokio::sync::{broadcast, oneshot};

use super::{ConnectorStatus, RodecasterConfig};

const RETRY: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuteEvent {
    pub channel: usize,
    pub label: String,
    pub muted: bool,
    pub remote: bool,
    pub notify: bool,
}

enum Control {
    Start(RodecasterConfig),
    Stop,
    SetMute {
        channel: usize,
        mute: bool,
        reply: oneshot::Sender<Result<(), String>>,
    },
}

/// Every hidapi call in the process happens on this connector's single worker
/// thread, which lives as long as the process. `hid_enumerate` mutates hidapi's
/// process-global `IOHIDManager`, so two threads enumerating at once corrupt it and
/// kill the process with SIGTRAP — reproduced in four lines against this hidapi.
pub struct RodecasterConnector {
    pub status: Arc<RwLock<ConnectorStatus>>,
    pub profile: Arc<RwLock<Option<Profile>>>,
    pub status_tx: broadcast::Sender<ConnectorStatus>,
    pub profile_tx: broadcast::Sender<Profile>,
    pub mute_tx: broadcast::Sender<MuteEvent>,
    control: Mutex<mpsc::Sender<Control>>,
}

impl RodecasterConnector {
    #[must_use]
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        let (profile_tx, _) = broadcast::channel(16);
        let (mute_tx, _) = broadcast::channel(64);
        let (control, commands) = mpsc::channel();

        let shared = Shared {
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            profile: Arc::new(RwLock::new(None)),
            status_tx: status_tx.clone(),
            profile_tx: profile_tx.clone(),
            mute_tx: mute_tx.clone(),
        };
        let connector = Self {
            status: Arc::clone(&shared.status),
            profile: Arc::clone(&shared.profile),
            status_tx,
            profile_tx,
            mute_tx,
            control: Mutex::new(control),
        };
        std::thread::Builder::new()
            .name("rodecaster".into())
            .spawn(move || shared.run(&commands))
            .map(|_| ())
            .unwrap_or_else(|e| tracing::error!("rodecaster: cannot spawn worker: {e}"));
        connector
    }

    pub fn start(&self, config: RodecasterConfig) {
        tracing::info!("rodecaster: connector start requested");
        self.send(Control::Start(config));
    }

    pub fn stop(&self) {
        tracing::info!("rodecaster: connector stop requested");
        self.send(Control::Stop);
    }

    #[must_use]
    pub fn get_status(&self) -> ConnectorStatus {
        self.status.read().expect("status lock").clone()
    }

    #[must_use]
    pub fn get_profile(&self) -> Option<Profile> {
        self.profile.read().expect("profile lock").clone()
    }

    /// # Errors
    /// Returns a message when the device is not connected or the write failed.
    pub async fn set_mute(&self, channel: usize, mute: bool) -> Result<(), String> {
        if self.get_status() != ConnectorStatus::Connected {
            return Err("rodecaster_not_connected".to_string());
        }
        let (reply, answer) = oneshot::channel();
        self.send(Control::SetMute {
            channel,
            mute,
            reply,
        });
        answer
            .await
            .unwrap_or_else(|_| Err("rodecaster_not_connected".to_string()))
    }

    fn send(&self, control: Control) {
        let _ = self.control.lock().expect("control lock").send(control);
    }
}

impl Default for RodecasterConnector {
    fn default() -> Self {
        Self::new()
    }
}

struct Shared {
    status: Arc<RwLock<ConnectorStatus>>,
    profile: Arc<RwLock<Option<Profile>>>,
    status_tx: broadcast::Sender<ConnectorStatus>,
    profile_tx: broadcast::Sender<Profile>,
    mute_tx: broadcast::Sender<MuteEvent>,
}

impl Shared {
    fn run(self, commands: &mpsc::Receiver<Control>) {
        let mut config: Option<RodecasterConfig> = None;
        let mut device: Option<Device> = None;

        loop {
            let waited = match (config.is_some(), device.is_some()) {
                (false, _) => commands.recv().map_err(|_| RecvTimeoutError::Disconnected),
                (true, false) => commands.recv_timeout(RETRY),
                (true, true) => commands.recv_timeout(POLL),
            };
            match waited {
                Ok(Control::Start(next)) => config = Some(next),
                Ok(Control::Stop) => {
                    config = None;
                    device = None;
                    self.clear();
                }
                Ok(Control::SetMute {
                    channel,
                    mute,
                    reply,
                }) => {
                    let result = device.as_mut().map_or_else(
                        || Err("rodecaster_not_connected".to_string()),
                        |d| d.set_mute(channel, mute).map_err(|e| e.to_string()),
                    );
                    // The desk echoes a mute it made itself, never one it was told to
                    // make, so a write we accepted is published here or not at all.
                    if result.is_ok() {
                        self.publish_mute(
                            MuteChange {
                                channel,
                                muted: mute,
                                remote: false,
                            },
                            &config,
                        );
                    }
                    let _ = reply.send(result);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }

            if config.is_none() {
                continue;
            }
            let Some(open) = device.as_mut() else {
                device = self.open();
                continue;
            };
            match open.poll(POLL) {
                Event::Mute(change) => self.publish_mute(change, &config),
                Event::Resynced => self.publish_profile(open),
                Event::Idle => {}
                Event::Disconnected(reason) => {
                    tracing::warn!("rodecaster: {reason}");
                    device = None;
                    *self.profile.write().expect("profile lock") = None;
                    self.set_status(ConnectorStatus::Error { message: reason });
                }
            }
        }
    }

    fn open(&self) -> Option<Device> {
        match Device::open() {
            Ok(device) => {
                self.set_status(ConnectorStatus::Connected);
                self.publish_profile(&device);
                Some(device)
            }
            Err(rodecaster::Error::NotFound) => {
                self.set_status(ConnectorStatus::Disconnected);
                None
            }
            Err(e) => {
                self.set_status(ConnectorStatus::Error {
                    message: e.to_string(),
                });
                None
            }
        }
    }

    fn clear(&self) {
        *self.profile.write().expect("profile lock") = None;
        self.set_status(ConnectorStatus::Disconnected);
    }

    fn publish_profile(&self, device: &Device) {
        match device.profile() {
            Ok(profile) => {
                *self.profile.write().expect("profile lock") = Some(profile.clone());
                let _ = self.profile_tx.send(profile);
            }
            Err(e) => tracing::warn!("rodecaster: cannot read profile: {e}"),
        }
    }

    fn publish_mute(&self, change: MuteChange, config: &Option<RodecasterConfig>) {
        let mut guard = self.profile.write().expect("profile lock");
        let channel = guard
            .as_mut()
            .and_then(|p| p.channels.get_mut(change.channel));
        let label = channel.map_or_else(
            || format!("Channel {}", change.channel + 1),
            |channel| {
                if change.remote {
                    channel.wireless_mute = change.muted;
                } else {
                    channel.mute = change.muted;
                }
                channel.label.clone()
            },
        );
        drop(guard);

        let _ = self.mute_tx.send(MuteEvent {
            channel: change.channel,
            label,
            muted: change.muted,
            remote: change.remote,
            notify: config.as_ref().is_some_and(|c| c.notify_on_mute),
        });
    }

    fn set_status(&self, status: ConnectorStatus) {
        let mut current = self.status.write().expect("status lock");
        if *current == status {
            return;
        }
        *current = status.clone();
        drop(current);
        let _ = self.status_tx.send(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_mute_is_refused_while_the_desk_is_not_connected() {
        let connector = RodecasterConnector::new();
        assert_eq!(
            connector.set_mute(0, true).await,
            Err("rodecaster_not_connected".to_string())
        );
    }

    /// Enabling and re-saving used to spawn a fresh worker thread each time, while
    /// the outgoing one could still be inside `Device::open`. Two threads enumerating
    /// hidapi's shared `IOHIDManager` at once killed the process with SIGTRAP.
    ///
    /// This is the only test here that reaches a real enumeration, and it must stay
    /// that way: a second one would run on a second thread and bring the crash back,
    /// in the test binary this time.
    #[tokio::test]
    async fn re_saving_the_config_does_not_move_hidapi_to_another_thread() {
        let connector = RodecasterConnector::new();
        let settle = Duration::from_millis(150);

        connector.start(RodecasterConfig {
            enabled: true,
            notify_on_mute: false,
            ..RodecasterConfig::default()
        });
        tokio::time::sleep(settle).await;

        connector.start(RodecasterConfig {
            enabled: true,
            notify_on_mute: true,
            ..RodecasterConfig::default()
        });
        tokio::time::sleep(settle).await;

        connector.stop();
        connector.start(RodecasterConfig {
            enabled: true,
            notify_on_mute: true,
            ..RodecasterConfig::default()
        });
        tokio::time::sleep(settle).await;

        connector.stop();
        tokio::time::sleep(settle).await;
        assert_eq!(connector.get_status(), ConnectorStatus::Disconnected);
    }

    #[tokio::test]
    async fn a_stopped_connector_reports_disconnected_and_holds_no_profile() {
        let connector = RodecasterConnector::new();
        connector.stop();
        assert_eq!(connector.get_status(), ConnectorStatus::Disconnected);
        assert!(connector.get_profile().is_none());
    }
}
