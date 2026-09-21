use std::sync::Arc;

use crate::HostHandle;
pub use blackmagic_camera::Camera;
use blackmagic_camera::discovery;
pub use blackmagic_camera::discovery::Discovered;
use blackmagic_camera::{
    Error, LivestreamPlatform, LivestreamStatus, PlatformConfig, RecordState, Trust, notify,
};
use tokio::sync::{Mutex, RwLock, broadcast, watch};
use tokio::time::Duration;

use super::{BlackmagicCameraConfig, ConnectorStatus};

/// Record and livestream state ride the notification websocket now — `GET /event/list`
/// against real hardware lists `/transports/0/record` and `/livestreams/0` as
/// subscribable on this model (the manual's list, which the old poll-everything
/// comment here relied on, says otherwise; see blackmagic-camera/README.md). This is
/// only a liveness backstop: if the camera drops off the network, the notification
/// socket keeps retrying silently on its own schedule, so a REST call on the side is
/// what actually notices and flips the connector to `Error`.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

const INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

/// Snapshot of the camera's outputs, broadcast whenever either changes.
///
/// The livestream half keeps the camera's own word rather than a bool. `Connecting`
/// and `Flushing` are the transitions an operator has to be shown — collapsing them
/// into "not streaming" is what forced the UI to guess at them on a wall clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraState {
    /// `Idle` | `Connecting` | `Streaming` | `Flushing` | `Interrupted`, verbatim.
    pub stream_status: String,
    pub is_recording: bool,
}

/// Livestreaming is absent on some models; that reads as `Idle`, not as a failure.
pub const STREAM_IDLE: &str = "Idle";

impl CameraState {
    pub fn is_streaming(&self) -> bool {
        self.stream_status == "Streaming"
    }
}

pub struct BlackmagicCameraConnector {
    pub status: Arc<RwLock<ConnectorStatus>>,
    /// Last known record/livestream state; `None` while disconnected.
    pub state: Arc<RwLock<Option<CameraState>>>,
    /// Live camera client; `None` while disconnected.
    pub camera: Arc<Mutex<Option<Arc<Camera>>>>,
    pub status_tx: broadcast::Sender<ConnectorStatus>,
    pub state_tx: broadcast::Sender<CameraState>,
    stop_tx: Mutex<Option<watch::Sender<bool>>>,
}

impl BlackmagicCameraConnector {
    pub fn new() -> Self {
        let (status_tx, _) = broadcast::channel(16);
        let (state_tx, _) = broadcast::channel(16);
        Self {
            status: Arc::new(RwLock::new(ConnectorStatus::Disconnected)),
            state: Arc::new(RwLock::new(None)),
            camera: Arc::new(Mutex::new(None)),
            status_tx,
            state_tx,
            stop_tx: Mutex::new(None),
        }
    }

    pub async fn start(&self, config: BlackmagicCameraConfig, host: Option<HostHandle>) {
        tracing::info!(host = %config.host, "blackmagic camera: connector start requested");
        self.stop_internal().await;

        let (stop_tx, stop_rx) = watch::channel(false);
        *self.stop_tx.lock().await = Some(stop_tx);

        let worker = Worker {
            config,
            host,
            status: Arc::clone(&self.status),
            state: Arc::clone(&self.state),
            camera: Arc::clone(&self.camera),
            status_tx: self.status_tx.clone(),
            state_tx: self.state_tx.clone(),
        };
        tokio::spawn(async move { worker.run(stop_rx).await });
    }

    pub async fn stop(&self) {
        tracing::info!("blackmagic camera: connector stop requested");
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

    pub async fn get_state(&self) -> Option<CameraState> {
        self.state.read().await.clone()
    }

    /// The connected camera, for callers that drive it directly (record, livestream).
    pub async fn client(&self) -> Option<Arc<Camera>> {
        self.camera.lock().await.as_ref().map(Arc::clone)
    }
}

impl Default for BlackmagicCameraConnector {
    fn default() -> Self {
        Self::new()
    }
}

/// mDNS scan for cameras on the LAN. An empty list is a normal outcome: on
/// networks where multicast does not arrive, cameras are added by host instead.
pub async fn discover(timeout: Duration) -> Result<Vec<Discovered>, String> {
    discovery::browse(discovery::DEFAULT_SERVICE, timeout).await
}

/// A blank fingerprint means "not accepted yet" — trust whatever the camera
/// presents, so `presented_fingerprint()` can show the operator what to pin.
pub fn connect(config: &BlackmagicCameraConfig) -> Result<Camera, Error> {
    let auth = (!config.username.is_empty())
        .then_some((config.username.as_str(), config.password.as_str()));
    Camera::connect(&config.host, auth, trust_for(config))
}

fn trust_for(config: &BlackmagicCameraConfig) -> Trust {
    if config.fingerprint.is_empty() {
        Trust::OnFirstUse
    } else {
        Trust::Pinned(config.fingerprint.clone())
    }
}

/// Points the camera's livestream at YouTube. Prefers the camera's own YouTube
/// platform, which only needs the stream key; falls back to a custom RTMP URL on
/// models whose platform list has no YouTube entry.
pub async fn push_youtube(
    camera: &Camera,
    ingestion_address: &str,
    stream_key: &str,
) -> Result<LivestreamPlatform, Error> {
    let platforms = camera.livestream_platforms().await?;
    let Some(name) = platforms.iter().find(|p| is_youtube_rtmp(p)) else {
        let url = format!("{}/{stream_key}", ingestion_address.trim_end_matches('/'));
        return camera.stream_to(&url, None).await;
    };

    let config = camera.livestream_platform_config(name).await?;
    let platform = youtube_platform(config, stream_key)?;
    camera.set_livestream_active_platform(&platform).await?;
    Ok(platform)
}

/// Cameras name their entries "YouTube RTMP" and "YouTube SRT (Beta)", never a bare
/// "YouTube". Only the RTMP one takes the ingestion address and key we hold.
fn is_youtube_rtmp(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("youtube") && !name.contains("srt")
}

/// The camera's YouTube platform entry filled in with our stream key: its default
/// quality profile when it names one, otherwise the first it lists.
fn youtube_platform(config: PlatformConfig, stream_key: &str) -> Result<LivestreamPlatform, Error> {
    let name = &config.platform;
    let quality = config
        .default_profile
        .clone()
        .or_else(|| config.profiles.first().map(|p| p.profile.clone()))
        .ok_or_else(|| Error::Unexpected(format!("platform {name} lists no quality profiles")))?;
    let server = config
        .servers
        .first()
        .map(|s| s.server.clone())
        .ok_or_else(|| Error::Unexpected(format!("platform {name} lists no servers")))?;
    Ok(LivestreamPlatform {
        platform: config.platform,
        server,
        key: Some(stream_key.to_string()),
        passphrase: None,
        quality,
        url: None,
    })
}

/// Everything the camera control screen reads, in one pass: the camera's own
/// payloads forwarded as it returns them. Only the active platform's profile
/// list is fetched — the others cost a round trip each and nothing shows them.
pub async fn settings(camera: &Camera) -> Result<serde_json::Value, Error> {
    let active = camera.livestream_active_platform().await?;
    let platform: serde_json::Value = camera
        .get(&format!("/livestreams/platforms/{}", active.platform))
        .await?;
    Ok(serde_json::json!({
        "recording": camera.record_state().await?.recording,
        "record": {
            "format": camera.get::<serde_json::Value>("/system/format").await?,
            "supported": camera.get::<serde_json::Value>("/system/supportedFormats").await?,
        },
        "storage": {
            "slots": camera.get::<serde_json::Value>("/media/slots").await?,
            "workingset": camera.get::<serde_json::Value>("/media/workingset").await?,
            "active": camera.get::<serde_json::Value>("/media/active").await?,
        },
        "stream": {
            "status": camera.get::<serde_json::Value>("/livestreams/0").await?,
            "available": camera.get::<serde_json::Value>("/livestreams/0/available").await?,
            "platforms": camera.livestream_platforms().await?,
            "active": active,
            "platform": platform,
        },
    }))
}

/// What the control screen sends back. Both halves are optional: the screen
/// applies whichever the operator changed.
#[derive(serde::Deserialize)]
pub struct SettingsUpdate {
    /// The camera's own `FormatRequest` — codec, frameRate and both resolutions
    /// together, which is the only combination it validates.
    pub record: Option<serde_json::Value>,
    pub stream: Option<LivestreamPlatform>,
}

pub async fn apply_settings(camera: &Camera, update: &SettingsUpdate) -> Result<(), Error> {
    if let Some(record) = &update.record {
        camera.put("/system/format", record).await?;
    }
    if let Some(stream) = &update.stream {
        camera.set_livestream_active_platform(stream).await?;
    }
    Ok(())
}

async fn read_state(camera: &Camera) -> Result<CameraState, Error> {
    let is_recording = camera.record_state().await?.recording;
    let stream_status = match camera.livestream_status().await {
        Ok(status) => status.status,
        Err(e) if e.is_unsupported() || matches!(e, Error::NotFound(_)) => STREAM_IDLE.to_string(),
        Err(e) => return Err(e),
    };
    Ok(CameraState {
        stream_status,
        is_recording,
    })
}

/// Outcome of one connection attempt: the caller asked us to stop, or the camera
/// dropped out and the loop should back off and retry.
enum Ended {
    ByCaller,
    ByError(String),
}

struct Worker {
    config: BlackmagicCameraConfig,
    host: Option<HostHandle>,
    status: Arc<RwLock<ConnectorStatus>>,
    state: Arc<RwLock<Option<CameraState>>>,
    camera: Arc<Mutex<Option<Arc<Camera>>>>,
    status_tx: broadcast::Sender<ConnectorStatus>,
    state_tx: broadcast::Sender<CameraState>,
}

impl Worker {
    async fn run(self, mut stop_rx: watch::Receiver<bool>) {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            self.set_status(ConnectorStatus::Connecting).await;

            let ended = self.session(&mut stop_rx).await;
            let was_connected = self.camera.lock().await.take().is_some();
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
            tracing::info!(
                seconds = backoff.as_secs(),
                "blackmagic camera: waiting before retry"
            );

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

    async fn session(&self, stop_rx: &mut watch::Receiver<bool>) -> Ended {
        let camera = match connect(&self.config) {
            Ok(camera) => Arc::new(camera),
            Err(e) => return Ended::ByError(e.to_string()),
        };

        // `product` is the reachability, auth and certificate check in one call.
        if let Err(e) = camera.product().await {
            return Ended::ByError(e.to_string());
        }

        *self.camera.lock().await = Some(Arc::clone(&camera));
        self.set_status(ConnectorStatus::Connected).await;

        // The notification socket only pushes *changes*, so seed the current values
        // with one REST read before relying on it.
        if let Err(e) = self.resync_state(&camera).await {
            return Ended::ByError(e.to_string());
        }

        let mut events = notify::watch(
            &camera,
            trust_for(&self.config),
            vec![
                "/transports/0/record".to_string(),
                "/livestreams/0".to_string(),
            ],
        );

        let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
        heartbeat.tick().await; // fires immediately; consume it so the first real tick waits a full interval

        loop {
            tokio::select! {
                event = events.recv() => {
                    match event {
                        Some(notify::Event::Changed(change)) => self.apply_change(&change).await,
                        // A reconnect may have missed changes while the socket was down —
                        // catch up with one REST read rather than trusting the gap was empty.
                        Some(notify::Event::Connected) => {
                            if let Err(e) = self.resync_state(&camera).await {
                                return Ended::ByError(e.to_string());
                            }
                        }
                        // `notify::watch` retries this on its own; the heartbeat below is
                        // what decides whether the camera itself is actually still there.
                        Some(notify::Event::Disconnected(_)) => {}
                        None => return Ended::ByError("notification socket closed".into()),
                    }
                }
                _ = heartbeat.tick() => {
                    if let Err(e) = camera.product().await {
                        return Ended::ByError(e.to_string());
                    }
                }
                result = stop_rx.changed() => {
                    let _ = result;
                    return Ended::ByCaller;
                }
            }
        }
    }

    /// One-shot REST read of both halves, applied and broadcast only if either changed.
    async fn resync_state(&self, camera: &Camera) -> Result<(), Error> {
        let current = read_state(camera).await?;
        let mut guard = self.state.write().await;
        if guard.as_ref() != Some(&current) {
            let _ = self.state_tx.send(current.clone());
            *guard = Some(current);
        }
        Ok(())
    }

    /// Applies one pushed property change to whichever half of `CameraState` it names.
    async fn apply_change(&self, change: &notify::PropertyChange) {
        let mut guard = self.state.write().await;
        let Some(mut current) = guard.clone() else {
            return;
        };
        match change.property.as_str() {
            "/transports/0/record" => {
                let Ok(record) = change.parse::<RecordState>() else {
                    return;
                };
                current.is_recording = record.recording;
            }
            "/livestreams/0" => {
                let Ok(status) = change.parse::<LivestreamStatus>() else {
                    return;
                };
                // The raw word, not just the bool: "Streaming" is one of five states
                // (Idle | Connecting | Streaming | Flushing | Interrupted) and knowing
                // which one the camera passed through is the whole diagnostic.
                tracing::info!("camera livestream status: {}", status.status);
                current.stream_status = status.status;
            }
            _ => return,
        }
        if guard.as_ref() != Some(&current) {
            let _ = self.state_tx.send(current.clone());
            *guard = Some(current);
        }
    }

    async fn set_status(&self, new_status: ConnectorStatus) {
        tracing::info!(connector = "blackmagic-camera", status = ?new_status, "connector status");
        *self.status.write().await = new_status;
        let current = self.status.read().await.clone();
        let _ = self.status_tx.send(current.clone());
        if let Some(host) = &self.host
            && let Err(e) = crate::emit(host, "connector://blackmagic-camera-status", &current)
        {
            tracing::warn!("Failed to emit Blackmagic camera status: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn platform_config(json: serde_json::Value) -> PlatformConfig {
        serde_json::from_value(json).expect("platform config")
    }

    #[test]
    fn youtube_rtmp_is_picked_out_of_a_real_platform_list() {
        let platforms = [
            "YouTube RTMP",
            "YouTube SRT (Beta)",
            "Microsoft Teams",
            "Twitch",
        ];
        let picked: Vec<_> = platforms.iter().filter(|p| is_youtube_rtmp(p)).collect();
        assert_eq!(picked, ["YouTube RTMP"].iter().collect::<Vec<_>>());
    }

    #[test]
    fn youtube_platform_prefers_the_default_profile() {
        let config = platform_config(serde_json::json!({
            "platform": "YouTube",
            "servers": [{ "server": "Primary", "url": "rtmp://a.rtmp.youtube.com/live2" }],
            "profiles": [{ "profile": "1080p" }, { "profile": "720p" }],
            "defaultProfile": "720p",
        }));
        let platform = youtube_platform(config, "abcd-1234").expect("platform");
        assert_eq!(platform.quality, "720p");
        assert_eq!(platform.server, "Primary");
        assert_eq!(platform.key.as_deref(), Some("abcd-1234"));
        // A platform entry carries the key, never a custom URL.
        assert_eq!(platform.url, None);
    }

    #[test]
    fn youtube_platform_falls_back_to_the_first_profile() {
        let config = platform_config(serde_json::json!({
            "platform": "YouTube",
            "servers": [{ "server": "Primary", "url": "rtmp://a.rtmp.youtube.com/live2" }],
            "profiles": [{ "profile": "1080p" }],
        }));
        assert_eq!(youtube_platform(config, "k").unwrap().quality, "1080p");
    }

    #[test]
    fn youtube_platform_reports_a_camera_with_nothing_to_select() {
        let config = platform_config(serde_json::json!({
            "platform": "YouTube",
            "servers": [{ "server": "Primary", "url": "rtmp://x" }],
            "profiles": [],
        }));
        assert!(youtube_platform(config, "k").is_err());
    }

    fn test_worker(initial: CameraState) -> Worker {
        let (status_tx, _) = broadcast::channel(1);
        let (state_tx, _) = broadcast::channel(1);
        Worker {
            config: BlackmagicCameraConfig {
                enabled: true,
                host: String::new(),
                fingerprint: String::new(),
                username: String::new(),
                password: String::new(),
            },
            host: None,
            status: Arc::new(RwLock::new(ConnectorStatus::Connected)),
            state: Arc::new(RwLock::new(Some(initial))),
            camera: Arc::new(Mutex::new(None)),
            status_tx,
            state_tx,
        }
    }

    #[tokio::test]
    async fn apply_change_updates_only_the_named_half() {
        let worker = test_worker(CameraState {
            stream_status: STREAM_IDLE.to_string(),
            is_recording: false,
        });

        worker
            .apply_change(&notify::PropertyChange {
                property: "/transports/0/record".to_string(),
                value: serde_json::json!({ "recording": true }),
            })
            .await;
        assert_eq!(
            *worker.state.read().await,
            Some(CameraState {
                stream_status: STREAM_IDLE.to_string(),
                is_recording: true
            }),
        );

        worker
            .apply_change(&notify::PropertyChange {
                property: "/livestreams/0".to_string(),
                value: serde_json::json!({
                    "status": "Streaming",
                    "bitrate": 0,
                    "effectiveVideoFormat": "1920x1080p24",
                }),
            })
            .await;
        assert_eq!(
            *worker.state.read().await,
            Some(CameraState {
                stream_status: "Streaming".to_string(),
                is_recording: true
            }),
        );
    }

    #[tokio::test]
    async fn a_transition_survives_instead_of_reading_as_not_streaming() {
        let worker = test_worker(CameraState {
            stream_status: STREAM_IDLE.to_string(),
            is_recording: false,
        });

        // "Connecting" is the RTMP handshake. Collapsing it to a bool is what left the
        // UI unable to tell "not started" from "starting", and guessing on a timer.
        worker
            .apply_change(&notify::PropertyChange {
                property: "/livestreams/0".to_string(),
                value: serde_json::json!({
                    "status": "Connecting",
                    "bitrate": 0,
                    "effectiveVideoFormat": "1920x1080p24",
                }),
            })
            .await;

        let state = worker.state.read().await.clone().unwrap();
        assert_eq!(state.stream_status, "Connecting");
        assert!(!state.is_streaming());
    }

    #[tokio::test]
    async fn apply_change_ignores_a_property_it_does_not_track() {
        let worker = test_worker(CameraState {
            stream_status: "Streaming".to_string(),
            is_recording: true,
        });

        worker
            .apply_change(&notify::PropertyChange {
                property: "/video/iso".to_string(),
                value: serde_json::json!({ "iso": 800 }),
            })
            .await;

        assert_eq!(
            *worker.state.read().await,
            Some(CameraState {
                stream_status: "Streaming".to_string(),
                is_recording: true
            }),
        );
    }

    #[tokio::test]
    async fn apply_change_ignores_a_shape_it_cannot_parse() {
        let worker = test_worker(CameraState {
            stream_status: STREAM_IDLE.to_string(),
            is_recording: false,
        });

        // `recording` missing entirely — RecordState fails to deserialize, not a panic.
        worker
            .apply_change(&notify::PropertyChange {
                property: "/transports/0/record".to_string(),
                value: serde_json::json!({ "clipName": "A001" }),
            })
            .await;

        assert_eq!(
            *worker.state.read().await,
            Some(CameraState {
                stream_status: STREAM_IDLE.to_string(),
                is_recording: false
            }),
        );
    }
}
