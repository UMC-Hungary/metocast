#![forbid(unsafe_code)]

use std::{collections::HashMap, fmt, time::Duration};

use futures_util::{SinkExt, StreamExt};
use metocast_core::{
    connectors::ConnectorStatus,
    events::{
        BiblePassage, CreateEvent, Event, EventSummary, SlideFolder, TitleTemplate, UpdateEvent,
    },
    protocol::ServerEvent,
    recordings::{FlagUpload, FlagUploadItem, Recording},
    slides::{CreateSongSlides, EventSlides, PptEnvelope, PptFile, PptFolder, SongSlides},
};
use reqwest::{StatusCode, Url};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

#[derive(Clone)]
pub struct AuthToken(String);

impl AuthToken {
    pub fn new(value: impl Into<String>) -> Result<Self, ConfigError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(ConfigError::EmptyToken);
        }
        Ok(Self(value))
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for AuthToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthToken([REDACTED])")
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    base_url: Url,
    token: AuthToken,
}

impl ClientConfig {
    pub fn new(base_url: &str, token: impl Into<String>) -> Result<Self, ConfigError> {
        let mut base_url =
            Url::parse(base_url).map_err(|error| ConfigError::InvalidUrl(error.to_string()))?;
        if !matches!(base_url.scheme(), "http" | "https") || base_url.host().is_none() {
            return Err(ConfigError::UnsupportedUrl);
        }
        base_url.set_query(None);
        base_url.set_fragment(None);
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        Ok(Self {
            base_url,
            token: AuthToken::new(token)?,
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url, ConfigError> {
        self.base_url
            .join(path.trim_start_matches('/'))
            .map_err(|error| ConfigError::InvalidUrl(error.to_string()))
    }

    fn websocket_url(&self) -> Result<Url, ConfigError> {
        let mut url = self.endpoint("ws")?;
        url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
            .map_err(|()| ConfigError::UnsupportedUrl)?;
        url.query_pairs_mut()
            .append_pair("token", self.token.expose());
        Ok(url)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("base URL is invalid: {0}")]
    InvalidUrl(String),
    #[error("base URL must use http or https and include a host")]
    UnsupportedUrl,
    #[error("authentication token cannot be empty")]
    EmptyToken,
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("authentication was rejected")]
    Authentication,
    #[error("server returned HTTP {status}")]
    Http {
        status: StatusCode,
        /// The server's own `{"error": …}` text, when it sent one.
        message: Option<String>,
    },
    #[error("transport failed")]
    Transport(#[from] reqwest::Error),
    #[error("response did not match the Metocast contract")]
    Decode(#[from] serde_json::Error),
    #[error("client configuration is invalid")]
    Config(#[from] ConfigError),
}

/// The server-wide settings a client can edit.
#[derive(Debug, Clone, Default)]
pub struct ServerSettings {
    pub title_template: String,
    pub slide_folder: String,
    pub song_slide_folder: String,
}

#[derive(Clone)]
pub struct MetocastClient {
    config: ClientConfig,
    http: reqwest::Client,
}

impl MetocastClient {
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub async fn health(&self) -> Result<(), ClientError> {
        self.response(self.http.get(self.config.endpoint("health")?))
            .await?;
        Ok(())
    }

    pub async fn list_events(&self) -> Result<Vec<EventSummary>, ClientError> {
        self.get_json("api/events").await
    }

    pub async fn event(&self, id: Uuid) -> Result<Event, ClientError> {
        self.get_json(&format!("api/events/{id}")).await
    }

    pub async fn connector_statuses(
        &self,
    ) -> Result<HashMap<String, ConnectorStatus>, ClientError> {
        self.get_json("api/connectors/status").await
    }

    /// Existing discovery routes run on the server, including in client mode.
    pub async fn discover_network(
        &self,
        atem: bool,
    ) -> Result<Vec<metocast_core::discovery::NetworkDevice>, ClientError> {
        #[derive(serde::Deserialize)]
        struct Response {
            devices: Vec<metocast_core::discovery::NetworkDevice>,
        }
        let connector = if atem { "atem" } else { "middlecontrol" };
        let url = self
            .config
            .endpoint(&format!("api/connectors/{connector}/discover"))?;
        let response: Response = self
            .send_json(self.http.post(url).timeout(Duration::from_secs(30)))
            .await?;
        Ok(response.devices)
    }

    pub async fn discover_cameras(
        &self,
    ) -> Result<Vec<metocast_core::discovery::CameraDevice>, ClientError> {
        #[derive(serde::Deserialize)]
        struct Response {
            cameras: Vec<metocast_core::discovery::CameraDevice>,
        }
        let url = self
            .config
            .endpoint("api/connectors/blackmagic-camera/discover")?;
        let response: Response = self
            .send_json(self.http.post(url).timeout(Duration::from_secs(30)))
            .await?;
        Ok(response.cameras)
    }

    pub async fn discover_broadlink(&self) -> Result<(), ClientError> {
        self.response(
            self.http
                .post(self.config.endpoint("api/connectors/broadlink/discover")?)
                .bearer_auth(self.config.token.expose())
                .timeout(Duration::from_secs(10)),
        )
        .await?;
        Ok(())
    }

    pub async fn broadlink_devices(
        &self,
    ) -> Result<Vec<metocast_core::discovery::BroadlinkDevice>, ClientError> {
        self.send_json(
            self.http
                .get(self.config.endpoint("api/connectors/broadlink/devices")?)
                .timeout(Duration::from_secs(10)),
        )
        .await
    }

    /// Where to send the operator to sign in to YouTube or Facebook. The browser comes back to
    /// the server's own callback, so the app only has to open this.
    pub async fn auth_url(&self, platform: &str) -> Result<String, ClientError> {
        #[derive(serde::Deserialize)]
        struct Response {
            url: String,
        }
        let response: Response = self
            .send_json(
                self.http
                    .get(self.config.endpoint(&format!("auth/{platform}/url"))?),
            )
            .await?;
        Ok(response.url)
    }

    pub async fn sign_out(&self, platform: &str) -> Result<(), ClientError> {
        let url = self.config.endpoint(&format!("auth/{platform}/logout"))?;
        self.response(self.http.post(url).bearer_auth(self.config.token.expose()))
            .await?;
        Ok(())
    }

    /// The stored configuration of one connector, with its secrets blanked by the server.
    pub async fn connector_config(
        &self,
        connector: &str,
    ) -> Result<serde_json::Value, ClientError> {
        self.send_json(
            self.http
                .get(
                    self.config
                        .endpoint(&format!("api/connectors/{connector}/config"))?,
                )
                .timeout(Duration::from_secs(10)),
        )
        .await
    }

    /// Saves a connector's configuration and applies it: the server reconnects it, or stops it
    /// when it is no longer enabled.
    pub async fn save_connector_config(
        &self,
        connector: &str,
        body: &serde_json::Value,
    ) -> Result<(), ClientError> {
        self.response(
            self.http
                .put(
                    self.config
                        .endpoint(&format!("api/connectors/{connector}/config"))?,
                )
                .bearer_auth(self.config.token.expose())
                .json(body)
                .timeout(Duration::from_secs(10)),
        )
        .await?;
        Ok(())
    }

    pub async fn network_device_config(
        &self,
        atem: bool,
    ) -> Result<metocast_core::discovery::NetworkDeviceConfig, ClientError> {
        let connector = if atem { "atem" } else { "middlecontrol" };
        self.send_json(
            self.http
                .get(
                    self.config
                        .endpoint(&format!("api/connectors/{connector}/config"))?,
                )
                .timeout(Duration::from_secs(10)),
        )
        .await
    }

    pub async fn save_network_device(
        &self,
        atem: bool,
        config: metocast_core::discovery::NetworkDeviceConfig,
    ) -> Result<(), ClientError> {
        let connector = if atem { "atem" } else { "middlecontrol" };
        self.response(
            self.http
                .put(
                    self.config
                        .endpoint(&format!("api/connectors/{connector}/config"))?,
                )
                .bearer_auth(self.config.token.expose())
                .json(&config)
                .timeout(Duration::from_secs(10)),
        )
        .await?;
        Ok(())
    }

    pub async fn create_event(&self, event: &CreateEvent) -> Result<Event, ClientError> {
        let url = self.config.endpoint("api/events")?;
        self.send_json(self.http.post(url).json(event)).await
    }

    pub async fn update_event(&self, id: Uuid, event: &UpdateEvent) -> Result<Event, ClientError> {
        let url = self.config.endpoint(&format!("api/events/{id}"))?;
        self.send_json(self.http.put(url).json(event)).await
    }

    pub async fn delete_event(&self, id: Uuid) -> Result<(), ClientError> {
        let url = self.config.endpoint(&format!("api/events/{id}"))?;
        self.response(
            self.http
                .delete(url)
                .bearer_auth(self.config.token.expose()),
        )
        .await?;
        Ok(())
    }

    pub async fn title_template(&self) -> Result<TitleTemplate, ClientError> {
        self.get_json("api/settings/title-template").await
    }

    /// The Bible-slide and song-slide output folders on the server, and the title template.
    pub async fn server_settings(&self) -> Result<ServerSettings, ClientError> {
        let template: TitleTemplate = self.get_json("api/settings/title-template").await?;
        let slides: SlideFolder = self.get_json("api/settings/slide-folder").await?;
        let songs: SlideFolder = self.get_json("api/settings/song-slide-folder").await?;
        Ok(ServerSettings {
            title_template: template.template,
            slide_folder: slides.path,
            song_slide_folder: songs.path,
        })
    }

    /// Saves all three. The server refuses a folder that does not exist.
    pub async fn save_server_settings(&self, settings: &ServerSettings) -> Result<(), ClientError> {
        self.put_json(
            "api/settings/title-template",
            &TitleTemplate {
                template: settings.title_template.clone(),
            },
        )
        .await?;
        self.put_json(
            "api/settings/slide-folder",
            &SlideFolder {
                path: settings.slide_folder.clone(),
            },
        )
        .await?;
        self.put_json(
            "api/settings/song-slide-folder",
            &SlideFolder {
                path: settings.song_slide_folder.clone(),
            },
        )
        .await
    }

    async fn put_json(&self, path: &str, body: &impl serde::Serialize) -> Result<(), ClientError> {
        let url = self.config.endpoint(path)?;
        self.response(
            self.http
                .put(url)
                .bearer_auth(self.config.token.expose())
                .json(body),
        )
        .await?;
        Ok(())
    }

    pub async fn bible_passage(
        &self,
        reference: &str,
        translation: &str,
    ) -> Result<BiblePassage, ClientError> {
        let mut url = self.config.endpoint("api/bible/verses")?;
        url.query_pairs_mut()
            .append_pair("reference", reference)
            .append_pair("translation", translation);
        self.send_json(self.http.get(url)).await
    }

    /// The files recorded for one event, newest first.
    pub async fn recordings(&self, event_id: Uuid) -> Result<Vec<Recording>, ClientError> {
        self.get_json(&format!("api/events/{event_id}/recordings"))
            .await
    }

    /// Marks one recording to be uploaded to `platforms` ("youtube", "facebook").
    pub async fn flag_upload(
        &self,
        event_id: Uuid,
        recording_id: Uuid,
        platforms: Vec<String>,
    ) -> Result<(), ClientError> {
        let url = self
            .config
            .endpoint(&format!("api/events/{event_id}/recordings/flag-upload"))?;
        self.response(
            self.http
                .post(url)
                .bearer_auth(self.config.token.expose())
                .json(&FlagUpload {
                    recordings: vec![FlagUploadItem {
                        recording_id,
                        platforms,
                    }],
                }),
        )
        .await?;
        Ok(())
    }

    /// PowerPoint files in the watched folders whose name contains `filter`.
    pub async fn ppt_files(&self, filter: &str) -> Result<Vec<PptFile>, ClientError> {
        let mut url = self.config.endpoint("api/ppt/files")?;
        url.query_pairs_mut().append_pair("filter", filter);
        let envelope: PptEnvelope<Vec<PptFile>> = self.send_json(self.http.get(url)).await?;
        Ok(envelope.data)
    }

    pub async fn ppt_folders(&self) -> Result<Vec<PptFolder>, ClientError> {
        let envelope: PptEnvelope<Vec<PptFolder>> = self.get_json("api/ppt/folders").await?;
        Ok(envelope.data)
    }

    /// Writes one deck per Bible reference on the event into the configured slide folder.
    pub async fn create_event_slides(&self, id: Uuid) -> Result<EventSlides, ClientError> {
        let url = self.config.endpoint(&format!("api/events/{id}/slides"))?;
        self.send_json(self.http.post(url)).await
    }

    /// Turns pasted lyrics into a deck in the configured song folder.
    pub async fn create_song_slides(
        &self,
        song: &CreateSongSlides,
    ) -> Result<SongSlides, ClientError> {
        let url = self.config.endpoint("api/ppt/song")?;
        self.send_json(self.http.post(url).json(song)).await
    }

    pub fn connect_websocket(&self) -> WebSocketConnection {
        WebSocketConnection::spawn(self.config.clone())
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        self.send_json(self.http.get(self.config.endpoint(path)?))
            .await
    }

    async fn send_json<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<T, ClientError> {
        let response = self
            .response(request.bearer_auth(self.config.token.expose()))
            .await?;
        let bytes = response.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn response(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ClientError> {
        let response = request.send().await?;
        let status = response.status();
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ClientError::Authentication),
            _ if !status.is_success() => Err(ClientError::Http {
                status,
                // The server explains refusals in an `error` field; anything else stays a status.
                message: response
                    .text()
                    .await
                    .ok()
                    .and_then(|body| serde_json::from_str::<serde_json::Value>(&body).ok())
                    .and_then(|body| body.get("error")?.as_str().map(str::to_owned)),
            }),
            _ => Ok(response),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting,
    AuthenticationFailed,
    Closed,
}

#[derive(Debug, thiserror::Error)]
pub enum WebSocketError {
    #[error("websocket command queue is closed")]
    Closed,
    #[error("websocket command could not be encoded")]
    Encode(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct WebSocketConnection {
    /// Encoded command JSON, queued until the socket is up.
    commands: mpsc::Sender<String>,
    events: broadcast::Sender<ServerEvent>,
    state: watch::Receiver<ConnectionState>,
    shutdown: watch::Sender<bool>,
}

impl WebSocketConnection {
    fn spawn(config: ClientConfig) -> Self {
        let (commands, command_rx) = mpsc::channel(32);
        let (events, _) = broadcast::channel(64);
        let (state_tx, state) = watch::channel(ConnectionState::Connecting);
        let (shutdown, shutdown_rx) = watch::channel(false);
        tokio::spawn(run_websocket(
            config,
            command_rx,
            events.clone(),
            state_tx,
            shutdown_rx,
        ));
        Self {
            commands,
            events,
            state,
            shutdown,
        }
    }

    /// Sends a `PresenterCommand`, `ProductionCommand` or any other typed command.
    pub async fn send(&self, command: &impl Serialize) -> Result<(), WebSocketError> {
        self.commands
            .send(serde_json::to_string(command)?)
            .await
            .map_err(|_| WebSocketError::Closed)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.events.subscribe()
    }

    pub fn state(&self) -> watch::Receiver<ConnectionState> {
        self.state.clone()
    }

    pub fn close(&self) {
        let _ = self.shutdown.send(true);
    }
}

async fn run_websocket(
    config: ClientConfig,
    mut commands: mpsc::Receiver<String>,
    events: broadcast::Sender<ServerEvent>,
    state: watch::Sender<ConnectionState>,
    mut shutdown: watch::Receiver<bool>,
) {
    let auth_check = reqwest::Client::new()
        .get(match config.endpoint("api/events") {
            Ok(url) => url,
            Err(_) => {
                let _ = state.send(ConnectionState::Closed);
                return;
            }
        })
        .bearer_auth(config.token.expose())
        .send()
        .await;
    if auth_check.as_ref().is_ok_and(|response| {
        matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        )
    }) {
        let _ = state.send(ConnectionState::AuthenticationFailed);
        return;
    }

    let Ok(url) = config.websocket_url() else {
        let _ = state.send(ConnectionState::Closed);
        return;
    };
    let mut reconnecting = false;

    loop {
        let _ = state.send(if reconnecting {
            ConnectionState::Reconnecting
        } else {
            ConnectionState::Connecting
        });
        let socket = tokio::select! {
            socket = tokio_tungstenite::connect_async(url.as_str()) => socket,
            result = shutdown.changed() => {
                let _ = result;
                break;
            }
        };
        let Ok((socket, _)) = socket else {
            reconnecting = true;
            tokio::select! {
                () = tokio::time::sleep(Duration::from_secs(1)) => continue,
                result = shutdown.changed() => {
                    let _ = result;
                    break;
                }
            }
        };
        let _ = state.send(ConnectionState::Connected);
        let (mut sink, mut stream) = socket.split();

        loop {
            tokio::select! {
                command = commands.recv() => {
                    let Some(command) = command else { break };
                    if sink.send(Message::Text(command)).await.is_err() { break; }
                }
                message = stream.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(ServerEvent::Ping { ping_id }) = serde_json::from_str(&text) {
                                if sink.send(Message::Text(pong_message(ping_id))).await.is_err() {
                                    break;
                                }
                            } else if let Ok(event) = serde_json::from_str(&text) {
                                let _ = events.send(event);
                            }
                        }
                        Some(Ok(Message::Ping(payload))) => {
                            if sink.send(Message::Pong(payload)).await.is_err() { break; }
                        }
                        Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                        _ => {}
                    }
                }
                result = shutdown.changed() => {
                    let _ = result;
                    let _ = sink.close().await;
                    let _ = state.send(ConnectionState::Closed);
                    return;
                }
            }
        }
        reconnecting = true;
    }
    let _ = state.send(ConnectionState::Closed);
}

/// The server pings with `pingId` but reads the reply's field as `ping_id`.
fn pong_message(ping_id: i64) -> String {
    serde_json::json!({"type": "pong", "ping_id": ping_id}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_debug_is_redacted() {
        let token = AuthToken::new("super-secret").unwrap();
        assert_eq!(format!("{token:?}"), "AuthToken([REDACTED])");
        assert!(!format!("{token:?}").contains("super-secret"));
    }

    #[test]
    fn configuration_normalizes_urls() {
        let config = ClientConfig::new("https://example.com/core", "token").unwrap();
        assert_eq!(
            config.endpoint("api/events").unwrap().as_str(),
            "https://example.com/core/api/events"
        );
        assert_eq!(config.websocket_url().unwrap().scheme(), "wss");
    }

    #[test]
    fn pong_uses_the_field_name_the_server_reads() {
        let pong: serde_json::Value = serde_json::from_str(&pong_message(42)).unwrap();
        assert_eq!(pong, serde_json::json!({"type": "pong", "ping_id": 42}));
    }
}
