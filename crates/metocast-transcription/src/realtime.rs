//! Client for OpenAI's realtime speech translation endpoint (`gpt-realtime-translate`).
//!
//! Audio goes in as 24 kHz mono PCM16 frames; translated (and source) transcript deltas come
//! back stamped with the audio position that had been sent when they arrived. The caller paces
//! the audio: a live microphone paces itself, a file feeder sleeps between frames.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::{Error, Result};

pub const DEFAULT_URL: &str =
    "wss://api.openai.com/v1/realtime/translations?model=gpt-realtime-translate";
pub const SAMPLE_RATE: u32 = 24_000;

#[derive(Debug, Clone)]
pub struct RealtimeConfig {
    pub url: String,
    pub api_key: String,
    /// Output language, e.g. `en`.
    pub target_language: String,
    /// Model for the source-language transcript; `None` disables it.
    pub source_transcription_model: Option<String>,
    /// After the audio ends, close once the server has been quiet this long.
    pub drain_idle: Duration,
    /// Forward the translated speech audio as [`RealtimeEvent::TargetAudio`].
    pub forward_audio: bool,
}

impl RealtimeConfig {
    #[must_use]
    pub fn new(api_key: String) -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            api_key,
            target_language: "en".to_string(),
            source_transcription_model: Some("gpt-realtime-whisper".to_string()),
            drain_idle: Duration::from_secs(8),
            forward_audio: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RealtimeEvent {
    /// Audio up to `at_ms` has been sent.
    AudioSent {
        at_ms: u64,
    },
    /// Translated (target-language) transcript text.
    TargetDelta {
        at_ms: u64,
        text: String,
    },
    /// Source-language transcript text.
    SourceDelta {
        at_ms: u64,
        text: String,
    },
    /// Translated speech, 24 kHz mono PCM16 (only with `forward_audio`).
    TargetAudio {
        at_ms: u64,
        pcm: Vec<i16>,
    },
    /// Any other server event, without audio payloads, for logging.
    Server {
        at_ms: u64,
        kind: String,
        raw: Value,
    },
    Closed {
        at_ms: u64,
        reason: String,
    },
}

/// Streams `audio` to the API until the channel closes and the server goes quiet, or until the
/// consumer of `events` is gone.
///
/// # Errors
/// Returns an error when the connection or handshake fails (for example a bad API key) or the
/// socket breaks mid-stream.
pub async fn run(
    config: RealtimeConfig,
    mut audio: mpsc::Receiver<Vec<i16>>,
    events: mpsc::UnboundedSender<RealtimeEvent>,
) -> Result<()> {
    // A process that links both rustls backends (reqwest brings aws-lc-rs, tungstenite ring)
    // makes rustls panic at connect instead of choosing. Installing is a no-op once one is set.
    if rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .is_err()
    {
        tracing::trace!("a rustls crypto provider is already installed");
    }
    let mut request = config
        .url
        .as_str()
        .into_client_request()
        .map_err(ws_error)?;
    let authorization = HeaderValue::from_str(&format!("Bearer {}", config.api_key))
        .map_err(|_| Error::Api("the API key contains invalid characters".into()))?;
    request.headers_mut().insert(AUTHORIZATION, authorization);
    let (socket, _) = connect_async(request).await.map_err(ws_error)?;
    let (mut sink, mut source) = socket.split();

    sink.send(Message::Text(session_update(&config).to_string()))
        .await
        .map_err(ws_error)?;

    let mut sent_samples: u64 = 0;
    let at_ms = |samples: u64| samples * 1_000 / u64::from(SAMPLE_RATE);
    let mut audio_open = true;
    // A failed audio write is not the whole story: the server may have closed first (1013 no
    // credit, 3000 bad key) while audio was in flight, and what it said is still readable. Keep
    // reading, and report the write only if nothing the server sent explains it.
    let mut write_error = None;
    loop {
        let message = if audio_open {
            tokio::select! {
                frame = audio.recv() => {
                    match frame {
                        Some(frame) => {
                            sent_samples += frame.len() as u64;
                            let append = json!({
                                "type": "session.input_audio_buffer.append",
                                "audio": BASE64.encode(pcm_bytes(&frame)),
                            });
                            match sink.send(Message::Text(append.to_string())).await {
                                Ok(()) => {
                                    let sent = RealtimeEvent::AudioSent { at_ms: at_ms(sent_samples) };
                                    if !deliver(&events, sent) {
                                        return Ok(());
                                    }
                                }
                                Err(e) => {
                                    write_error = Some(e);
                                    audio_open = false;
                                }
                            }
                        }
                        None => audio_open = false,
                    }
                    continue;
                }
                message = source.next() => message,
            }
        } else {
            match tokio::time::timeout(config.drain_idle, source.next()).await {
                Ok(message) => message,
                Err(_) => break,
            }
        };
        let now = at_ms(sent_samples);
        match message {
            None => {
                if let Some(e) = write_error {
                    return Err(ws_error(e));
                }
                deliver(
                    &events,
                    RealtimeEvent::Closed {
                        at_ms: now,
                        reason: "connection ended".into(),
                    },
                );
                return Ok(());
            }
            Some(Err(e)) => return Err(ws_error(write_error.unwrap_or(e))),
            Some(Ok(Message::Close(frame))) => {
                let reason = frame.map_or_else(
                    || "closed".to_string(),
                    |f| format!("{} {}", f.code, f.reason),
                );
                deliver(&events, RealtimeEvent::Closed { at_ms: now, reason });
                return Ok(());
            }
            Some(Ok(Message::Text(text))) => {
                if !forward(&text, now, config.forward_audio, &events) {
                    return Ok(());
                }
            }
            Some(Ok(_)) => {}
        }
    }
    if let Some(e) = write_error {
        return Err(ws_error(e));
    }
    if let Err(error) = sink.send(Message::Close(None)).await {
        tracing::debug!(%error, "the close frame was not sent");
    }
    deliver(
        &events,
        RealtimeEvent::Closed {
            at_ms: at_ms(sent_samples),
            reason: "drained".into(),
        },
    );
    Ok(())
}

fn session_update(config: &RealtimeConfig) -> Value {
    let mut input = json!({ "noise_reduction": { "type": "near_field" } });
    if let Some(model) = &config.source_transcription_model {
        input["transcription"] = json!({ "model": model });
    }
    json!({
        "type": "session.update",
        "session": {
            "audio": {
                "input": input,
                "output": { "language": config.target_language },
            },
        },
    })
}

fn forward(
    text: &str,
    at_ms: u64,
    forward_audio: bool,
    events: &mpsc::UnboundedSender<RealtimeEvent>,
) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return deliver(
            events,
            RealtimeEvent::Server {
                at_ms,
                kind: "unparsable".into(),
                raw: Value::String(text.chars().take(500).collect()),
            },
        );
    };
    let kind = value["type"].as_str().unwrap_or("unknown").to_string();
    let event = match kind.as_str() {
        "session.output_transcript.delta" => RealtimeEvent::TargetDelta {
            at_ms,
            text: delta_text(&value),
        },
        "session.input_transcript.delta" => RealtimeEvent::SourceDelta {
            at_ms,
            text: delta_text(&value),
        },
        k if k.contains("audio") && k.ends_with(".delta") => {
            let pcm = forward_audio
                .then(|| value["delta"].as_str())
                .flatten()
                .and_then(|b64| BASE64.decode(b64).ok())
                .map(|bytes| {
                    bytes
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|b| i16::from_le_bytes(*b))
                        .collect()
                });
            match pcm {
                Some(pcm) => RealtimeEvent::TargetAudio { at_ms, pcm },
                None => return true,
            }
        }
        _ => RealtimeEvent::Server {
            at_ms,
            kind,
            raw: value,
        },
    };
    deliver(events, event)
}

/// Hands an event to the consumer. `false` means it dropped its receiver: nobody is listening,
/// so the stream stops instead of paying for audio no one reads.
fn deliver(events: &mpsc::UnboundedSender<RealtimeEvent>, event: RealtimeEvent) -> bool {
    let delivered = events.send(event).is_ok();
    if !delivered {
        tracing::debug!("the realtime event consumer is gone");
    }
    delivered
}

fn delta_text(value: &Value) -> String {
    ["delta", "text", "transcript"]
        .iter()
        .find_map(|field| value[*field].as_str())
        .unwrap_or_default()
        .to_string()
}

fn pcm_bytes(frame: &[i16]) -> Vec<u8> {
    frame.iter().flat_map(|s| s.to_le_bytes()).collect()
}

fn ws_error(error: WsError) -> Error {
    match error {
        WsError::Http(response) => {
            let body = response
                .body()
                .as_deref()
                .map(String::from_utf8_lossy)
                .unwrap_or_default()
                .chars()
                .take(500)
                .collect::<String>();
            Error::WebSocket(format!(
                "handshake rejected: HTTP {} {body}",
                response.status()
            ))
        }
        other => Error::WebSocket(other.to_string()),
    }
}
