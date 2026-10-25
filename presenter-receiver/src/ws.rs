use crate::ConnectionState;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

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
    #[serde(rename = "error")]
    Error { message: String },
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

/// Matches `MAX_WS_MESSAGE_BYTES` in metocast-core: a deck with photos arrives as one
/// `presenter.state` message larger than tungstenite's default 16 MiB frame limit.
const MAX_WS_MESSAGE_BYTES: usize = 64 << 20;

/// How long to wait for a `presenter.frame` before drawing the slide here: a server older than
/// the command ignores it.
const FRAME_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// SVG slides are drawn by the server (`presenter.frame`): resvg takes seconds to draw a photo
/// on a Raspberry Pi, decoding the server's JPEG a fraction of that. Replies come in order, so
/// the last one answers the latest request.
struct FrameRequests {
    /// Asked for and not yet answered.
    pending: u32,
    /// The slide the latest request was for.
    slide: Option<u32>,
    sent_at: std::time::Instant,
    /// False once the server turns out not to draw frames.
    supported: bool,
}

impl FrameRequests {
    /// Counts a reply in; true when it answers the latest request.
    fn answered(&mut self) -> bool {
        if self.pending == 0 {
            return false;
        }
        self.pending -= 1;
        self.pending == 0
    }
}

fn websocket_config() -> WebSocketConfig {
    WebSocketConfig {
        max_message_size: Some(MAX_WS_MESSAGE_BYTES),
        max_frame_size: Some(MAX_WS_MESSAGE_BYTES),
        ..WebSocketConfig::default()
    }
}

async fn connect_and_receive(
    url: &str,
    tx: &std::sync::mpsc::Sender<Frame>,
    dims: DisplayDims,
    conn_state: Arc<Mutex<ConnectionState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (ws_stream, _) = connect_async_with_config(url, Some(websocket_config()), false).await?;
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
    let mut frames = FrameRequests {
        pending: 0,
        slide: None,
        sent_at: std::time::Instant::now(),
        supported: true,
    };

    let idle_timeout = std::time::Duration::from_secs(60);

    loop {
        let wait = if frames.pending > 0 {
            FRAME_TIMEOUT.saturating_sub(frames.sent_at.elapsed())
        } else {
            idle_timeout
        };
        let raw = match tokio::time::timeout(wait, read.next()).await {
            Ok(Some(msg)) => msg?,
            Ok(None) => break, // stream ended cleanly
            Err(_) if frames.pending > 0 => {
                eprintln!("[frame] no reply from the server; drawing slides here");
                frames.pending = 0;
                frames.supported = false;
                render_state(
                    &presenter_state,
                    presenter_theme,
                    tx,
                    dims,
                    &mut svg_frame_cache,
                    false,
                )?;
                continue;
            }
            Err(_) => return Err("read timeout — connection lost".into()),
        };
        let text = match raw {
            Message::Text(t) => t,
            Message::Binary(jpeg) => {
                if !frames.answered() {
                    continue;
                }
                if let Some(slide) = frames.slide.take() {
                    let started = std::time::Instant::now();
                    match crate::renderer::decode_frame(&jpeg, dims.width, dims.height) {
                        Ok(frame) => {
                            eprintln!(
                                "[frame] slide {slide}: {} bytes {:?} after asking, decoded in {:?}",
                                jpeg.len(),
                                frames.sent_at.elapsed(),
                                started.elapsed()
                            );
                            svg_frame_cache.insert(slide, frame);
                        }
                        Err(err) => eprintln!("[frame] {err}; drawing it here"),
                    }
                    render_state(
                        &presenter_state,
                        presenter_theme,
                        tx,
                        dims,
                        &mut svg_frame_cache,
                        false,
                    )?;
                }
                continue;
            }
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
                show(
                    &mut write,
                    &presenter_state,
                    presenter_theme,
                    tx,
                    dims,
                    &mut svg_frame_cache,
                    &mut frames,
                )
                .await?;
            }
            ServerMsg::PresenterState { state } => {
                presenter_state = state;
                log_presenter_state("state", &presenter_state);
                svg_frame_cache.clear();
                frames.slide = None;
                show(
                    &mut write,
                    &presenter_state,
                    presenter_theme,
                    tx,
                    dims,
                    &mut svg_frame_cache,
                    &mut frames,
                )
                .await?;
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
                show(
                    &mut write,
                    &presenter_state,
                    presenter_theme,
                    tx,
                    dims,
                    &mut svg_frame_cache,
                    &mut frames,
                )
                .await?;
            }
            ServerMsg::Ping { ping_id } => {
                write
                    .send(Message::Text(
                        json!({"type": "pong", "ping_id": ping_id}).to_string(),
                    ))
                    .await?;
            }
            ServerMsg::Error { message } => {
                eprintln!("[ws] server error: {message}");
                // `unauthorized`: a server older than `presenter.frame`.
                frames.supported &= message != "unauthorized";
                if frames.answered() {
                    frames.slide = None;
                    render_state(
                        &presenter_state,
                        presenter_theme,
                        tx,
                        dims,
                        &mut svg_frame_cache,
                        false,
                    )?;
                }
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

/// Shows the current slide, asking the server to draw it when it is an SVG slide not drawn yet.
async fn show(
    write: &mut (impl SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin),
    state: &PresenterState,
    theme: PresenterTheme,
    tx: &std::sync::mpsc::Sender<Frame>,
    dims: DisplayDims,
    svg_frame_cache: &mut HashMap<u32, Frame>,
    frames: &mut FrameRequests,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ask_server = frames.supported && frames.slide != Some(state.current_slide);
    if render_state(state, theme, tx, dims, svg_frame_cache, ask_server)? {
        write
            .send(Message::Text(
                json!({
                    "type": "presenter.frame",
                    "slide": state.current_slide,
                    "width": dims.width,
                    "height": dims.height,
                })
                .to_string(),
            ))
            .await?;
        frames.pending += 1;
        frames.slide = Some(state.current_slide);
        frames.sent_at = std::time::Instant::now();
    }
    Ok(())
}

/// Sends the current slide's frame to the display. With `ask_server`, an SVG slide not drawn
/// yet is left to the server instead and the result is true; the screen keeps what it shows.
/// Fails when the display thread is gone and nothing is left to show the frame.
fn render_state(
    state: &PresenterState,
    theme: PresenterTheme,
    tx: &std::sync::mpsc::Sender<Frame>,
    dims: DisplayDims,
    svg_frame_cache: &mut HashMap<u32, Frame>,
    ask_server: bool,
) -> Result<bool, std::sync::mpsc::SendError<Frame>> {
    if state.muted {
        tx.send(vec![0u32; (dims.width * dims.height) as usize])?;
        return Ok(false);
    }

    if theme == PresenterTheme::Classic && state.render_mode == PresenterRenderMode::Svg {
        if ask_server && !svg_frame_cache.contains_key(&state.current_slide) {
            return Ok(true);
        }
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
        tx.send(frame)?;
        return Ok(false);
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
    tx.send(crate::renderer::rgb_to_u32(&rgb))?;
    Ok(false)
}
