//! Notification websocket — live camera state without polling.
//!
//! [`watch`] owns a reconnect loop. The subscription set is the `properties` argument,
//! captured by the loop, so reconnect re-subscribes by construction — there is no
//! separate "desired subscriptions" state to drift out of sync.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_tungstenite::Connector;
use tokio_tungstenite::tungstenite::Message;

use crate::{Camera, Trust, tls};

const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// One `propertyValueChanged` push.
#[derive(Debug, Clone)]
pub struct PropertyChange {
    pub property: String,
    /// Shapes differ per property; deserialize into your own type with [`PropertyChange::parse`].
    pub value: serde_json::Value,
}

impl PropertyChange {
    pub fn parse<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        T::deserialize(&self.value)
    }
}

/// Connection transitions, so a UI can show per-camera status.
#[derive(Debug, Clone)]
pub enum Event {
    Connected,
    Disconnected(String),
    Changed(PropertyChange),
}

#[derive(Deserialize)]
struct Incoming {
    #[serde(default)]
    data: Option<IncomingData>,
}

#[derive(Deserialize)]
struct IncomingData {
    #[serde(default)]
    action: String,
    #[serde(default)]
    property: String,
    #[serde(default)]
    value: serde_json::Value,
}

/// Connects, subscribes to `properties`, and forwards events until the receiver is
/// dropped. Reconnects with backoff, re-subscribing each time.
///
/// Returns the receiver; the loop runs as a spawned task.
pub fn watch(camera: &Camera, trust: Trust, properties: Vec<String>) -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel(64);
    let url = camera.websocket_url();

    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        loop {
            match session(&url, &trust, &properties, &tx).await {
                // Ended because the caller dropped the receiver.
                Ok(()) => return,
                Err(e) => {
                    if tx.send(Event::Disconnected(e)).await.is_err() {
                        return;
                    }
                }
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    });

    rx
}

/// One connection's lifetime. `Ok(())` means "stop for good", `Err` means "retry".
async fn session(
    url: &str,
    trust: &Trust,
    properties: &[String],
    tx: &mpsc::Sender<Event>,
) -> Result<(), String> {
    let connector = if url.starts_with("wss://") {
        let (config, _) = tls::client_config(trust.clone());
        Some(Connector::Rustls(std::sync::Arc::new(config)))
    } else {
        None
    };

    let (mut socket, _) =
        tokio_tungstenite::connect_async_tls_with_config(url, None, false, connector)
            .await
            .map_err(|e| e.to_string())?;

    // `properties` is an array in the protocol, so the whole desired set goes in one
    // message — and re-subscribing after a reconnect is that same single message.
    let subscribe = serde_json::json!({
        "type": "request",
        "id": 1,
        "data": { "action": "subscribe", "properties": properties },
    });
    socket
        .send(Message::Text(subscribe.to_string()))
        .await
        .map_err(|e| e.to_string())?;

    if tx.send(Event::Connected).await.is_err() {
        return Ok(());
    }

    while let Some(message) = socket.next().await {
        let text = match message.map_err(|e| e.to_string())? {
            Message::Text(t) => t,
            Message::Close(_) => return Err("camera closed the connection".into()),
            _ => continue,
        };

        // Anything we can't parse is a message we don't handle, not a dropped connection.
        let Ok(Incoming { data: Some(data) }) = serde_json::from_str::<Incoming>(&text) else {
            continue;
        };
        if data.action != "propertyValueChanged" {
            continue;
        }
        let change = PropertyChange {
            property: data.property,
            value: data.value,
        };
        if tx.send(Event::Changed(change)).await.is_err() {
            return Ok(());
        }
    }

    Err("connection ended".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_property_changes_are_forwarded() {
        let push = r#"{"type":"response","data":{"action":"propertyValueChanged",
            "property":"/transports/0/record","value":{"recording":true}}}"#;
        let parsed: Incoming = serde_json::from_str(push).unwrap();
        let data = parsed.data.unwrap();
        assert_eq!(data.action, "propertyValueChanged");
        assert_eq!(data.property, "/transports/0/record");
        assert_eq!(data.value["recording"], true);

        // A subscribe acknowledgement carries no property change and must be ignored.
        let ack = r#"{"type":"response","data":{"action":"subscribe"}}"#;
        let parsed: Incoming = serde_json::from_str(ack).unwrap();
        assert_ne!(parsed.data.unwrap().action, "propertyValueChanged");

        // Junk must not blow up the loop.
        assert!(serde_json::from_str::<Incoming>("{\"nope\":1}").is_ok());
    }
}
