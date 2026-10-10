//! Local WebSocket broadcast of the simulated live stream plus a tiny HTTP viewer page.
//!
//! Message shapes mirror the planned `/ws` protocol: `transcription.snapshot`,
//! `transcription.partial`, `transcription.segment`, `transcription.state`. Audio goes as binary
//! frames: one tag byte ([`ORIGINAL_AUDIO`] or [`TRANSLATED_AUDIO`]) then 24 kHz mono PCM16 LE.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt as _, StreamExt as _};
use metocast_transcription::segmenter::Segment;
use serde_json::json;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::Message;

const VIEWER_HTML: &str = include_str!("viewer.html");
pub const ORIGINAL_AUDIO: u8 = 0;
pub const TRANSLATED_AUDIO: u8 = 1;

#[derive(Default)]
struct Snapshot {
    segments: Vec<Segment>,
    partial: String,
    status: String,
}

#[derive(Clone)]
pub struct Broadcaster {
    tx: broadcast::Sender<Message>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl Broadcaster {
    /// Binds the WebSocket on `addr` and the viewer page on the next port.
    pub async fn start(addr: SocketAddr) -> std::io::Result<(Self, SocketAddr)> {
        let ws_listener = TcpListener::bind(addr).await?;
        let mut http_addr = addr;
        http_addr.set_port(addr.port() + 1);
        let http_listener = TcpListener::bind(http_addr).await?;
        let (tx, _) = broadcast::channel(4_096);
        let broadcaster = Self {
            tx,
            snapshot: Arc::new(Mutex::new(Snapshot {
                status: "running".into(),
                ..Snapshot::default()
            })),
        };

        let ws = broadcaster.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = ws_listener.accept().await {
                let client = ws.clone();
                tokio::spawn(async move { client.serve_socket(stream).await });
            }
        });

        let page = VIEWER_HTML.replace("__WS_PORT__", &addr.port().to_string());
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = http_listener.accept().await {
                let page = page.clone();
                tokio::spawn(async move {
                    let mut request = [0_u8; 4_096];
                    let _ = stream.read(&mut request).await;
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                         Content-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{page}",
                        page.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        Ok((broadcaster, http_addr))
    }

    async fn serve_socket(&self, stream: tokio::net::TcpStream) {
        let Ok(socket) = tokio_tungstenite::accept_async(stream).await else {
            return;
        };
        let (mut sink, mut source) = socket.split();
        let mut rx = self.tx.subscribe();
        let snapshot = {
            let s = self.snapshot.lock().expect("snapshot lock");
            json!({
                "type": "transcription.snapshot",
                "status": s.status,
                "segments": s.segments,
                "partial": s.partial,
            })
            .to_string()
        };
        if sink.send(Message::Text(snapshot)).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                message = rx.recv() => match message {
                    Ok(message) => if sink.send(message).await.is_err() { return },
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                incoming = source.next() => if !matches!(incoming, Some(Ok(_))) { return },
            }
        }
    }

    pub fn partial(&self, text: &str, at_ms: u64) {
        self.snapshot.lock().expect("snapshot lock").partial = text.to_string();
        self.send_json(&json!({ "type": "transcription.partial", "text": text, "atMs": at_ms }));
    }

    pub fn segment(&self, segment: &Segment) {
        self.snapshot
            .lock()
            .expect("snapshot lock")
            .segments
            .push(segment.clone());
        self.send_json(&json!({ "type": "transcription.segment", "segment": segment }));
    }

    pub fn status(&self, status: &str) {
        self.snapshot.lock().expect("snapshot lock").status = status.to_string();
        self.send_json(&json!({ "type": "transcription.state", "status": status }));
    }

    pub fn audio(&self, tag: u8, pcm: &[i16]) {
        if self.tx.receiver_count() == 0 {
            return;
        }
        let mut frame = Vec::with_capacity(1 + pcm.len() * 2);
        frame.push(tag);
        frame.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
        let _ = self.tx.send(Message::Binary(frame));
    }

    fn send_json(&self, value: &serde_json::Value) {
        let _ = self.tx.send(Message::Text(value.to_string()));
    }
}
