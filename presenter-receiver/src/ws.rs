use crate::ConnectionState;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

// ── Message types from the server ────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphContent {
    /// New format: explicit lines from the PPTX parser.
    #[serde(default)]
    pub lines: Vec<String>,
    /// Legacy format (old binaries): single text string.
    pub text: Option<String>,
    pub align: String,
    #[serde(default)]
    pub font_size_pt: f64,
}

impl ParagraphContent {
    pub fn display_text(&self) -> String {
        if !self.lines.is_empty() {
            self.lines.join("\n")
        } else {
            self.text.clone().unwrap_or_default()
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SlideContent {
    pub index: u32,
    pub paragraphs: Vec<ParagraphContent>,
}

#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PresenterRenderMode {
    #[default]
    Text,
    Svg,
}

#[derive(Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PresenterTheme {
    #[default]
    Classic,
    Editorial,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SvgSlideContent {
    pub index: u32,
    pub svg: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PresenterState {
    pub current_slide: u32,
    #[serde(default)]
    pub render_mode: PresenterRenderMode,
    #[serde(default)]
    pub slides: Vec<SlideContent>,
    #[serde(default)]
    pub svg_slides: Vec<SvgSlideContent>,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
enum ServerMsg {
    #[serde(rename = "presentation.settings")]
    PresentationSettings {
        #[serde(rename = "presenterTheme", default)]
        presenter_theme: PresenterTheme,
    },
    #[serde(rename = "presenter.state")]
    PresenterState { state: PresenterState },
    #[serde(rename = "presenter.slide_changed")]
    SlideChanged {
        #[serde(rename = "currentSlide")]
        current_slide: u32,
    },
    #[serde(rename = "ping")]
    Ping {
        #[serde(rename = "pingId")]
        ping_id: i64,
    },
    #[serde(other)]
    Unknown,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Connect to the presenter WebSocket, render slides, and send pixel frames
/// down `tx` whenever the current slide changes. Reconnects automatically.
pub async fn run(
    url: String,
    tx: std::sync::mpsc::Sender<Frame>,
    dims: DisplayDims,
    conn_state: Arc<Mutex<ConnectionState>>,
) {
    let mut fail_count = 0u32;
    loop {
        *conn_state.lock().unwrap() = ConnectionState::Connecting;
        eprintln!("[ws] Connecting to {url}...");
        match connect_and_receive(&url, &tx, dims, Arc::clone(&conn_state)).await {
            Ok(()) => {
                eprintln!("[ws] Connection closed.");
                fail_count = 0;
            }
            Err(e) => {
                eprintln!("[ws] Error: {e}");
                fail_count += 1;
            }
        }
        *conn_state.lock().unwrap() = if fail_count >= 5 {
            ConnectionState::Failed
        } else {
            ConnectionState::Reconnecting
        };
        eprintln!("[ws] Reconnecting in 3 s...");
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

// ── Frame type sent over the channel ─────────────────────────────────────────

/// Rendered pixel frame: u32 values in 0x00RRGGBB order (minifb / fb compatible).
pub type Frame = Vec<u32>;

/// Display dimensions needed by the renderer.
#[derive(Clone, Copy)]
pub struct DisplayDims {
    pub width: u32,
    pub height: u32,
}

// ── Internal connection loop ──────────────────────────────────────────────────

async fn connect_and_receive(
    url: &str,
    tx: &std::sync::mpsc::Sender<Frame>,
    dims: DisplayDims,
    conn_state: Arc<Mutex<ConnectionState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (ws_stream, _) = connect_async(url).await?;
    *conn_state.lock().unwrap() = ConnectionState::Connected;
    eprintln!("[ws] Connected.");

    let (mut write, mut read) = ws_stream.split();

    // Announce ourselves and ask for current state immediately
    let hostname = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    write
        .send(Message::Text(
            json!({"type": "presenter.register", "label": "Presenter Receiver", "hostname": hostname})
                .to_string(),
        ))
        .await?;
    write
        .send(Message::Text(
            json!({"type": "presenter.status"}).to_string(),
        ))
        .await?;

    let mut presenter_state = PresenterState {
        current_slide: 0,
        render_mode: PresenterRenderMode::Text,
        slides: Vec::new(),
        svg_slides: Vec::new(),
        muted: false,
    };
    let mut presenter_theme = PresenterTheme::Classic;
    let mut svg_frame_cache: HashMap<u32, Frame> = HashMap::new();

    let idle_timeout = std::time::Duration::from_secs(60);

    loop {
        let raw = match tokio::time::timeout(idle_timeout, read.next()).await {
            Ok(Some(msg)) => msg?,
            Ok(None) => break, // stream ended cleanly
            Err(_) => return Err("read timeout — connection lost".into()),
        };
        let text = match raw {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };

        let msg: ServerMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue,
        };

        match msg {
            ServerMsg::PresentationSettings {
                presenter_theme: next_theme,
            } => {
                presenter_theme = next_theme;
                render_state(
                    &presenter_state,
                    presenter_theme,
                    tx,
                    dims,
                    &mut svg_frame_cache,
                );
            }
            ServerMsg::PresenterState { state } => {
                presenter_state = state;
                log_presenter_state("state", &presenter_state);
                svg_frame_cache.clear();
                render_state(
                    &presenter_state,
                    presenter_theme,
                    tx,
                    dims,
                    &mut svg_frame_cache,
                );
            }
            ServerMsg::SlideChanged {
                current_slide: new_slide,
            } => {
                presenter_state.current_slide = new_slide;
                eprintln!(
                    "[presenter] slide changed: current={} mode={:?} textSlides={} svgSlides={}",
                    presenter_state.current_slide,
                    presenter_state.render_mode,
                    presenter_state.slides.len(),
                    presenter_state.svg_slides.len()
                );
                render_state(
                    &presenter_state,
                    presenter_theme,
                    tx,
                    dims,
                    &mut svg_frame_cache,
                );
            }
            ServerMsg::Ping { ping_id } => {
                write
                    .send(Message::Text(
                        json!({"type": "pong", "ping_id": ping_id}).to_string(),
                    ))
                    .await?;
            }
            ServerMsg::Unknown => {}
        }
    }

    Ok(())
}

fn log_presenter_state(source: &str, state: &PresenterState) {
    eprintln!(
        "[presenter] {source}: current={} mode={:?} muted={} textSlides={} svgSlides={}",
        state.current_slide,
        state.render_mode,
        state.muted,
        state.slides.len(),
        state.svg_slides.len()
    );
}

// ── Render + send ─────────────────────────────────────────────────────────────

fn render_state(
    state: &PresenterState,
    theme: PresenterTheme,
    tx: &std::sync::mpsc::Sender<Frame>,
    dims: DisplayDims,
    svg_frame_cache: &mut HashMap<u32, Frame>,
) {
    if state.muted {
        let _ = tx.send(vec![0u32; (dims.width * dims.height) as usize]);
        return;
    }

    if theme == PresenterTheme::Classic && state.render_mode == PresenterRenderMode::Svg {
        let frame = svg_frame_cache
            .entry(state.current_slide)
            .or_insert_with(|| {
                let Some(slide) = state
                    .svg_slides
                    .iter()
                    .find(|s| s.index == state.current_slide)
                else {
                    eprintln!(
                        "[render] SVG slide missing: current={} available={}",
                        state.current_slide,
                        state.svg_slides.len()
                    );
                    return vec![0u32; (dims.width * dims.height) as usize];
                };
                crate::renderer::render_svg_slide(&slide.svg, dims.width, dims.height)
                    .map_err(|err| eprintln!("[render] SVG render failed: {err}"))
                    .unwrap_or_else(|_| vec![0u32; (dims.width * dims.height) as usize])
            })
            .clone();
        let _ = tx.send(frame);
        return;
    }

    let owned: Vec<(String, String, f64)> = state
        .slides
        .iter()
        .find(|s| s.index == state.current_slide)
        .map(|s| {
            s.paragraphs
                .iter()
                .map(|p| (p.display_text(), p.align.clone(), p.font_size_pt))
                .collect()
        })
        .unwrap_or_default();
    let paragraphs: Vec<(&str, &str, f64)> = owned
        .iter()
        .map(|(t, a, pt)| (t.as_str(), a.as_str(), *pt))
        .collect();

    let rgb = crate::renderer::render_slide(
        &paragraphs,
        theme,
        state.current_slide,
        state.slides.len() as u32,
        dims.width,
        dims.height,
    );
    let _ = tx.send(crate::renderer::rgb_to_u32(&rgb));
}
