#![forbid(unsafe_code)]

use std::{collections::HashMap, fmt, time::Duration};

use futures_util::{SinkExt, StreamExt};
use metocast_core::{
    connectors::ConnectorStatus,
    events::{Event, EventSummary},
    protocol::{PresenterCommand, ServerEvent},
};
use reqwest::{StatusCode, Url};
use serde::de::DeserializeOwned;
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
    Http { status: StatusCode },
    #[error("transport failed")]
    Transport(#[from] reqwest::Error),
    #[error("response did not match the Metocast contract")]
    Decode(#[from] serde_json::Error),
    #[error("client configuration is invalid")]
    Config(#[from] ConfigError),
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

    pub fn connect_websocket(&self) -> WebSocketConnection {
        WebSocketConnection::spawn(self.config.clone())
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let request = self
            .http
            .get(self.config.endpoint(path)?)
            .bearer_auth(self.config.token.expose());
        let response = self.response(request).await?;
        let bytes = response.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn response(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, ClientError> {
        let response = request.send().await?;
        match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ClientError::Authentication),
            status if !status.is_success() => Err(ClientError::Http { status }),
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
    commands: mpsc::Sender<PresenterCommand>,
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

    pub async fn send(&self, command: PresenterCommand) -> Result<(), WebSocketError> {
        self.commands
            .send(command)
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
    mut commands: mpsc::Receiver<PresenterCommand>,
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
                    let Ok(json) = serde_json::to_string(&command) else { continue };
                    if sink.send(Message::Text(json)).await.is_err() { break; }
                }
                message = stream.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(ServerEvent::Ping { ping_id }) = serde_json::from_str(&text) {
                                let pong = serde_json::json!({"type": "pong", "pingId": ping_id});
                                if sink.send(Message::Text(pong.to_string())).await.is_err() {
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
}
