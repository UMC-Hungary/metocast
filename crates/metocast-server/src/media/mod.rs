//! Media processing on the `media` job queue. Video: probe a file on the server's disk, then
//! render it into a graded HEVC master for YouTube. Blackmagic RAW is developed by
//! `braw-decode` (the Blackmagic RAW SDK, which also applies the LUT) and encoded by ffmpeg;
//! other video goes straight to ffmpeg. See `braw-decode/README.md`. Audio: see [`audio`].
mod align;
mod audio;
mod camera_clips;
mod command;
mod probe;
mod program;
mod subtitles;
mod sync;
mod tools;
mod whisper;
mod worker;

use metocast_core::best_effort::LogGone;
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::{Mutex, Notify, oneshot};
use uuid::Uuid;

use base64::Engine as _;
use metocast_core::media::{
    CropRect, MediaConfig, MediaKind, MediaProbe, PreviewFrame, PreviewRequest, ProbeRequest,
    RenderProgress, RenderQueued, RenderRequest, RenderSettings, RenderState, output_crop,
};

use crate::database::settings;
use crate::server::AppState;

pub use audio::{probe_audio, start_audio};
pub use camera_clips::spawn_camera_clip_log;
pub use program::{add_program_subtitles, nudge_program, probe_program, start_program};
pub use worker::spawn_worker;

pub const MEDIA_QUEUE: &str = "media";
const RENDER_JOB: &str = "render_video";
pub const CONFIG_KEY: &str = "media_config";
const RENDERS_FOLDER: &str = "Metocast Renders";
const SOURCE_EXTENSIONS: &[&str] = &["braw", "mov", "mp4"];

const FFMPEG_CANDIDATES: &[&str] = &[
    "/opt/homebrew/bin/ffmpeg",
    "/usr/local/bin/ffmpeg",
    "/usr/bin/ffmpeg",
];
const BRAW_SDK_CANDIDATES: &[&str] = &[
    "/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac/Libraries",
    "/Applications/Blackmagic RAW/Blackmagic RAW Player.app/Contents/Frameworks",
];

/// The worker's wake-up, the jobs the cancel route can stop, and where downloaded tools live.
pub struct MediaRuntime {
    wake: Notify,
    running: Mutex<HashMap<Uuid, oneshot::Sender<()>>>,
    tools_dir: PathBuf,
    // ponytail: one-entry cache, as the viewer scrubs one clip; key by path if several get open at once.
    last_preview_probe: Mutex<Option<(PathBuf, std::time::SystemTime, Probed)>>,
}

impl MediaRuntime {
    /// `data_dir` is the server's own folder; downloaded tools go in its `tools` subfolder.
    pub fn new(data_dir: &FsPath) -> Self {
        Self {
            wake: Notify::new(),
            running: Mutex::new(HashMap::new()),
            tools_dir: data_dir.join("tools"),
            last_preview_probe: Mutex::new(None),
        }
    }
}

/// The external programs a render uses, where this host has them.
#[derive(Debug, Clone, Default)]
struct Tools {
    ffmpeg: Option<PathBuf>,
    ffprobe: Option<PathBuf>,
    braw_decode: Option<PathBuf>,
    braw_sdk: Option<PathBuf>,
    /// The configured copy, or the one downloaded earlier; `None` until then.
    jive_vocals: Option<PathBuf>,
    /// Reads lip movement for audio/video sync; shipped next to the server in the Mac app.
    lip_reader: Option<PathBuf>,
    /// Places a dropped transcript on the speech; shipped next to the server in the Mac app.
    whisper: Option<PathBuf>,
}

impl Tools {
    fn find(config: &MediaConfig, tools_dir: &FsPath) -> Self {
        let existing = |path: PathBuf| path.is_file().then_some(path);
        let beside_server = |name: &str| {
            std::env::current_exe()
                .ok()
                .map(|exe| exe.with_file_name(name))
        };
        // The Mac app ships its own LGPL build next to the server (see `ffmpeg/README.md`);
        // METOCAST_FFMPEG points a dev build at one, and Homebrew's serves a bare checkout.
        let ffmpeg = if config.ffmpeg_path.trim().is_empty() {
            std::env::var_os("METOCAST_FFMPEG")
                .map(PathBuf::from)
                .and_then(existing)
                .or_else(|| beside_server("ffmpeg").and_then(existing))
                .or_else(|| {
                    FFMPEG_CANDIDATES
                        .iter()
                        .find_map(|candidate| existing(PathBuf::from(candidate)))
                })
        } else {
            existing(PathBuf::from(config.ffmpeg_path.trim()))
        };
        let ffprobe = ffmpeg
            .as_ref()
            .and_then(|ffmpeg| existing(ffmpeg.with_file_name("ffprobe")));
        // Shipped next to the server in the Mac app; METOCAST_BRAW_DECODE points a dev build at it.
        let braw_decode = std::env::var_os("METOCAST_BRAW_DECODE")
            .map(PathBuf::from)
            .or_else(|| beside_server("braw-decode"))
            .and_then(existing);
        let has_framework = |dir: &FsPath| dir.join("BlackmagicRawAPI.framework").is_dir();
        let braw_sdk = if config.braw_sdk_path.trim().is_empty() {
            BRAW_SDK_CANDIDATES
                .iter()
                .map(PathBuf::from)
                .find(|dir| has_framework(dir))
        } else {
            Some(PathBuf::from(config.braw_sdk_path.trim())).filter(|dir| has_framework(dir))
        };
        let jive_vocals = match config.jive_vocals_path.trim() {
            "" => existing(tools::JIVE_VOCALS.path(tools_dir)),
            configured => existing(PathBuf::from(configured)),
        };
        // METOCAST_LIP_READER points a dev build at `lip-reader/build/lip-reader`.
        let lip_reader = std::env::var_os("METOCAST_LIP_READER")
            .map(PathBuf::from)
            .or_else(|| beside_server("lip-reader"))
            .and_then(existing);
        // METOCAST_WHISPER points a dev build at `whisper/build/whisper-cli`.
        let whisper = std::env::var_os("METOCAST_WHISPER")
            .map(PathBuf::from)
            .or_else(|| beside_server("whisper-cli"))
            .and_then(existing);
        Self {
            ffmpeg,
            ffprobe,
            braw_decode,
            braw_sdk,
            jive_vocals,
            lip_reader,
            whisper,
        }
    }

    fn missing(&self, kind: MediaKind) -> Vec<String> {
        let mut missing = Vec::new();
        if self.ffmpeg.is_none() || self.ffprobe.is_none() {
            missing.push("ffmpeg".to_string());
        }
        if kind == MediaKind::Braw {
            if self.braw_decode.is_none() {
                missing.push("braw-decode".to_string());
            }
            if self.braw_sdk.is_none() {
                missing.push("blackmagic-raw-sdk".to_string());
            }
        }
        missing
    }
}

/// A probe plus what only a render needs from it.
#[derive(Clone)]
struct Probed {
    probe: MediaProbe,
    tools: Tools,
    /// What `braw-decode frames` writes.
    pixel_format: String,
    /// Frames at the clip's end that can't be read; renders hold the last good frame over them.
    damaged_frames: u64,
}

#[derive(Debug)]
enum MediaError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Unavailable(String),
    Internal(String),
}

impl MediaError {
    fn message(self) -> String {
        match self {
            Self::BadRequest(m)
            | Self::NotFound(m)
            | Self::Conflict(m)
            | Self::Unavailable(m)
            | Self::Internal(m) => m,
        }
    }
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(m)
            | Self::NotFound(m)
            | Self::Conflict(m)
            | Self::Unavailable(m)
            | Self::Internal(m) => f.write_str(m),
        }
    }
}

impl IntoResponse for MediaError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, m),
            Self::Conflict(m) => (StatusCode::CONFLICT, m),
            Self::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, m),
            Self::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<sqlx::Error> for MediaError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!("media: database: {error}");
        Self::Internal("database error".to_string())
    }
}

/// An absolute path to an existing `.braw`, `.mov` or `.mp4` file.
fn validate_source(path: &str) -> Result<PathBuf, MediaError> {
    let path = FsPath::new(path);
    if !path.is_absolute() {
        return Err(MediaError::BadRequest(
            "the path must be absolute".to_string(),
        ));
    }
    let supported = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            SOURCE_EXTENSIONS
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        });
    if !supported {
        return Err(MediaError::BadRequest(
            "only .braw, .mov and .mp4 files can be rendered".to_string(),
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| MediaError::NotFound(format!("no such file: {}", path.display())))?;
    if !canonical.is_file() {
        return Err(MediaError::BadRequest(format!(
            "not a file: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

async fn load_config(pool: &PgPool) -> MediaConfig {
    settings::get_json::<MediaConfig>(pool, CONFIG_KEY).await
}

/// Validates a config the generic connector route is about to store.
pub fn validate_config(config: &MediaConfig) -> Result<(), String> {
    let settings = config.settings()?;
    if let Some(lut) = &settings.lut_path {
        command::check_lut_path(lut)?;
    }
    config.audio_settings()?;
    for (folder, what) in [
        (&config.output_folder, "output folder"),
        (&config.audio_output_folder, "audio output folder"),
        (&config.jive_vocals_path, "jive-vocals path"),
        (&config.sidecar_path, "sidecar path"),
    ] {
        let folder = folder.trim();
        if !folder.is_empty() && !FsPath::new(folder).is_absolute() {
            return Err(format!("the {what} must be an absolute path"));
        }
    }
    Ok(())
}

/// An absolute path to an existing Blackmagic RAW `.sidecar` file.
fn validate_sidecar(path: &str) -> Result<PathBuf, MediaError> {
    let path = FsPath::new(path);
    let is_sidecar = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("sidecar"));
    if !path.is_absolute() || !is_sidecar || !path.is_file() {
        return Err(MediaError::BadRequest(format!(
            "not a .sidecar file: {}",
            path.display()
        )));
    }
    Ok(path.to_path_buf())
}

fn check_settings(settings: &RenderSettings) -> Result<(), MediaError> {
    settings.validate().map_err(MediaError::BadRequest)?;
    if let Some(sidecar) = &settings.sidecar_path {
        validate_sidecar(sidecar)?;
    }
    if let Some(lut) = &settings.lut_path {
        command::check_lut_path(lut).map_err(MediaError::BadRequest)?;
        if !FsPath::new(lut).is_file() {
            return Err(MediaError::BadRequest(format!("no such LUT file: {lut}")));
        }
    }
    Ok(())
}

async fn probe_source(
    state: &AppState,
    source: &FsPath,
    sidecar: Option<&FsPath>,
) -> Result<Probed, MediaError> {
    let pool = &state.pool;
    let config = load_config(pool).await;
    let tools = Tools::find(&config, &state.media.tools_dir);
    let ffprobe = tools.ffprobe.clone().ok_or_else(|| {
        MediaError::Unavailable("ffmpeg is missing from this build of Metocast".to_string())
    })?;
    let facts = probe::ffprobe(&ffprobe, source)
        .await
        .map_err(MediaError::BadRequest)?;
    let mut defaults = config.settings().map_err(|e| {
        MediaError::BadRequest(format!(
            "the stored Media Processing settings are invalid: {e}"
        ))
    })?;

    let full = CropRect {
        x: 0,
        y: 0,
        width: facts.width,
        height: facts.height,
    };
    let (width, height, active_area, frame_count, pixel_format, raw_options, damaged_frames) =
        match (facts.kind, &tools.braw_decode) {
            (MediaKind::Braw, Some(braw_decode)) if tools.braw_sdk.is_some() => {
                let info =
                    probe::braw_info(braw_decode, tools.braw_sdk.as_deref(), sidecar, source)
                        .await
                        .map_err(MediaError::BadRequest)?;
                if let Some(sidecar) = sidecar {
                    info.apply_to(&mut defaults);
                    defaults.sidecar_path = Some(sidecar.to_string_lossy().into_owned());
                }
                let readable = info.readable_frames.unwrap_or(info.frame_count);
                (
                    info.width,
                    info.height,
                    info.active_area,
                    info.frame_count,
                    info.pixel_format.clone(),
                    Some(info.raw_options()),
                    info.frame_count.saturating_sub(readable),
                )
            }
            _ => (
                facts.width,
                facts.height,
                full,
                facts.frame_count,
                "rgb48le".to_string(),
                None,
                0,
            ),
        };
    if let Some(options) = &raw_options {
        options.fit_iso(&mut defaults);
    }
    let host_record_started_at = camera_clips::started_at(
        pool,
        facts.clip_name.as_deref(),
        facts.start_timecode.as_deref(),
    )
    .await?;

    let probe = MediaProbe {
        path: source.to_string_lossy().into_owned(),
        file_name: source
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        file_size: std::fs::metadata(source).map_or(0, |meta| meta.len()),
        kind: facts.kind,
        camera: facts.camera,
        clip_name: facts.clip_name,
        width,
        height,
        active_area,
        fps_num: facts.fps_num,
        fps_den: facts.fps_den,
        frame_count,
        duration_seconds: frame_count as f64 * f64::from(facts.fps_den) / f64::from(facts.fps_num),
        start_timecode: facts.start_timecode,
        recorded_at: facts.recorded_at,
        host_record_started_at,
        audio: facts.audio,
        gamma: facts.gamma,
        gamut: facts.gamut,
        embedded_lut: facts.embedded_lut,
        output: output_crop(active_area, defaults.aspect, defaults.crop_shift_y),
        defaults,
        missing: tools.missing(facts.kind),
        raw_options,
    };
    Ok(Probed {
        probe,
        tools,
        pixel_format,
        damaged_frames,
    })
}

/// Where a render of `source` lands: the configured folder, or `Metocast Renders` beside the
/// source. Never an existing file: a second render of the same clip gets a numbered name.
fn output_path(config: &MediaConfig, source: &FsPath) -> PathBuf {
    let folder = match config.output_folder.trim() {
        "" => source
            .parent()
            .unwrap_or_else(|| FsPath::new("/"))
            .join(RENDERS_FOLDER),
        configured => PathBuf::from(configured),
    };
    let stem = source.file_stem().map_or_else(
        || "render".to_string(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    (1..)
        .map(|n| match n {
            1 => folder.join(format!("{stem}.mov")),
            n => folder.join(format!("{stem} ({n}).mov")),
        })
        .find(|path| !path.exists() && !partial_path(path).exists())
        .unwrap_or_else(|| folder.join(format!("{stem}.mov")))
}

fn partial_path(output: &FsPath) -> PathBuf {
    let mut name = output.as_os_str().to_owned();
    name.push(".partial");
    PathBuf::from(name)
}

/// Files a finished file under its event, or as untracked when there is none (or it was deleted
/// while the job ran), and tells the clients. Returns the recording's id when it went under
/// the event.
async fn file_recording(
    state: &AppState,
    event_id: Option<Uuid>,
    output: &FsPath,
    duration_seconds: f64,
    media_kind: &str,
    source: &str,
    metadata: &serde_json::Value,
) -> Option<Uuid> {
    let file_name = output
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let file_path = output.to_string_lossy().into_owned();
    let file_size = std::fs::metadata(output).map_or(0, |meta| meta.len());
    let file_size = i64::try_from(file_size).unwrap_or(i64::MAX);
    if let Some(event_id) = event_id {
        let inserted = sqlx::query(
            "INSERT INTO recordings (event_id, file_path, file_name, file_size, duration_seconds, \
             media_kind, source, metadata, detected_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now()) RETURNING id",
        )
        .bind(event_id)
        .bind(&file_path)
        .bind(&file_name)
        .bind(file_size)
        .bind(duration_seconds)
        .bind(media_kind)
        .bind(source)
        .bind(metadata)
        .fetch_one(&state.pool)
        .await;
        match inserted {
            Ok(row) => {
                let title = crate::models::event::fetch_event(event_id, &state.pool)
                    .await
                    .ok()
                    .flatten()
                    .map(|event| event.published_title().to_string());
                crate::server::websocket::broadcast_recording_detected(
                    &state.ws_clients,
                    &file_name,
                    title.as_deref(),
                )
                .await;
                return sqlx::Row::try_get(&row, "id").ok();
            }
            // The event was deleted while the job ran: keep the file findable.
            Err(e) => tracing::warn!("media: filing under event {event_id} failed: {e}"),
        }
    }
    let inserted = sqlx::query(
        "INSERT INTO untracked_recordings (file_path, file_name, file_size, duration_seconds, \
         metadata, detected_at) VALUES ($1, $2, $3, $4, $5, now())",
    )
    .bind(&file_path)
    .bind(&file_name)
    .bind(file_size)
    .bind(duration_seconds)
    .bind(metadata)
    .execute(&state.pool)
    .await;
    match inserted {
        Ok(_) => {
            crate::server::websocket::broadcast_recording_detected(
                &state.ws_clients,
                &file_name,
                None,
            )
            .await;
        }
        Err(e) => tracing::error!("media: could not record {file_path}: {e}"),
    }
    None
}

/// The job payload: everything the worker needs, fixed when the render was queued.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderPayload {
    path: String,
    event_id: Option<Uuid>,
    settings: RenderSettings,
    output_path: String,
}

// ── REST handlers ─────────────────────────────────────────────────────────────

pub async fn probe_media(
    State(state): State<AppState>,
    Json(body): Json<ProbeRequest>,
) -> impl IntoResponse {
    let result = async {
        let source = validate_source(&body.path)?;
        let sidecar = body
            .sidecar_path
            .as_deref()
            .map(validate_sidecar)
            .transpose()?;
        Ok::<_, MediaError>(
            probe_source(&state, &source, sidecar.as_deref())
                .await?
                .probe,
        )
    }
    .await;
    match result {
        Ok(probe) => {
            tracing::info!(
                path = %probe.path,
                kind = ?probe.kind,
                missing = ?probe.missing,
                "media: probed"
            );
            Json(probe).into_response()
        }
        Err(error) => {
            tracing::warn!(path = %body.path, "media: probe refused: {error}");
            error.into_response()
        }
    }
}

pub async fn start_render(
    State(state): State<AppState>,
    Json(body): Json<RenderRequest>,
) -> impl IntoResponse {
    let path = body.path.clone();
    match queue_render(&state, body).await {
        Ok(queued) => {
            tracing::info!(job = %queued.job_id, output = %queued.output_path, "media: render queued");
            (StatusCode::ACCEPTED, Json(queued)).into_response()
        }
        Err(error) => {
            tracing::warn!(%path, "media: render refused: {error}");
            error.into_response()
        }
    }
}

async fn queue_render(state: &AppState, body: RenderRequest) -> Result<RenderQueued, MediaError> {
    let source = validate_source(&body.path)?;
    if !load_config(&state.pool).await.enabled {
        return Err(MediaError::Unavailable(
            "Media Processing is turned off in its connector settings".to_string(),
        ));
    }
    let probed = probe_source(state, &source, None).await?;
    if !probed.probe.missing.is_empty() {
        return Err(MediaError::Unavailable(format!(
            "this Mac is missing: {}",
            probed.probe.missing.join(", ")
        )));
    }
    let settings = body.settings.unwrap_or(probed.probe.defaults);
    check_settings(&settings)?;
    if let Some(event_id) = body.event_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE id = $1)")
            .bind(event_id)
            .fetch_one(&state.pool)
            .await?;
        if !exists {
            return Err(MediaError::NotFound("no such event".to_string()));
        }
    }

    let path = probed.probe.path.clone();
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM job_queue WHERE queue = $1 \
         AND status IN ('pending', 'processing') AND payload->>'path' = $2)",
    )
    .bind(MEDIA_QUEUE)
    .bind(&path)
    .fetch_one(&state.pool)
    .await?;
    if busy {
        return Err(MediaError::Conflict(
            "this file is already queued or rendering".to_string(),
        ));
    }

    let config = load_config(&state.pool).await;
    let output = output_path(&config, &source);
    let payload = RenderPayload {
        path,
        event_id: body.event_id,
        settings,
        output_path: output.to_string_lossy().into_owned(),
    };
    // One attempt: a failed render needs a person to look at it, not five retries.
    let job_id: Uuid = sqlx::query_scalar(
        "INSERT INTO job_queue (queue, job_type, payload, max_attempts) \
         VALUES ($1, $2, $3, 1) RETURNING id",
    )
    .bind(MEDIA_QUEUE)
    .bind(RENDER_JOB)
    .bind(serde_json::to_value(&payload).map_err(|e| MediaError::Internal(e.to_string()))?)
    .fetch_one(&state.pool)
    .await?;
    state.media.wake.notify_one();
    Ok(RenderQueued {
        job_id,
        output_path: payload.output_path,
    })
}

/// `POST /api/media/preview`: one frame as recorded, or with the settings, for the Media
/// viewer. It runs beside the queue (a render can take an hour), and a dropped request kills
/// its decoder.
pub async fn preview_media(
    State(state): State<AppState>,
    Json(body): Json<PreviewRequest>,
) -> Response {
    preview(&state, body).await.into_response()
}

async fn preview(state: &AppState, body: PreviewRequest) -> Result<Json<PreviewFrame>, MediaError> {
    let source = validate_source(&body.path)?;
    if let Some(settings) = &body.settings {
        check_settings(settings)?;
    }
    let modified = std::fs::metadata(&source)
        .and_then(|meta| meta.modified())
        .map_err(|e| MediaError::NotFound(e.to_string()))?;
    let cached = state
        .media
        .last_preview_probe
        .lock()
        .await
        .clone()
        .filter(|(path, at, _)| *path == source && *at == modified);
    let probed = match cached {
        Some((_, _, probed)) => probed,
        None => {
            let probed = probe_source(state, &source, None).await?;
            *state.media.last_preview_probe.lock().await =
                Some((source.clone(), modified, probed.clone()));
            probed
        }
    };
    let probe = &probed.probe;
    let program = match probe.kind {
        MediaKind::Braw => probed.tools.braw_decode.as_ref(),
        MediaKind::Video => probed.tools.ffmpeg.as_ref(),
    };
    let Some(program) = program.filter(|_| probe.missing.is_empty()) else {
        return Err(MediaError::Unavailable(format!(
            "this Mac can't show this file yet: {}",
            probe.missing.join(", ")
        )));
    };
    let command_source = command::Source {
        path: &source,
        kind: probe.kind,
        width: probe.width,
        height: probe.height,
        fps_num: probe.fps_num,
        fps_den: probe.fps_den,
        frame_count: probe.frame_count,
        damaged_frames: probed.damaged_frames,
        start_timecode: None,
        audio_channels: None,
        pixel_format: &probed.pixel_format,
    };
    let (_, args) = command::preview_args(
        &command_source,
        body.settings.as_ref(),
        body.frame.min(
            probe
                .frame_count
                .saturating_sub(probed.damaged_frames)
                .saturating_sub(1),
        ),
        body.max_width.unwrap_or(1600).clamp(160, 3840),
        probed.tools.braw_sdk.as_deref(),
    );
    let output = tokio::process::Command::new(program)
        .args(&args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| MediaError::Internal(format!("could not run {}: {e}", program.display())))?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err(MediaError::BadRequest(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let active = probe.active_area;
    Ok(Json(PreviewFrame {
        image_jpeg: base64::engine::general_purpose::STANDARD.encode(&output.stdout),
        active,
        output: body.settings.as_ref().map_or(active, |settings| {
            output_crop(active, settings.aspect, settings.crop_shift_y)
        }),
    }))
}

pub async fn cancel_render(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    let not_started = sqlx::query_scalar::<_, String>(
        "UPDATE job_queue SET status = 'dead', last_error = 'cancelled', locked_at = NULL, \
         locked_by = NULL, updated_at = now() \
         WHERE id = $1 AND queue = $2 AND status = 'pending' RETURNING job_type",
    )
    .bind(id)
    .bind(MEDIA_QUEUE)
    .fetch_optional(&state.pool)
    .await;
    match not_started {
        Ok(Some(job_type)) => {
            if job_type == program::PROGRAM_JOB {
                crate::server::websocket::broadcast_media_program_progress(
                    &state.ws_clients,
                    &metocast_core::media::ProgramProgress {
                        job_id: id,
                        state: RenderState::Cancelled,
                        step: None,
                        fraction: None,
                        output_path: String::new(),
                        error: None,
                        report: None,
                    },
                )
                .await;
            } else if job_type == program::NUDGE_JOB {
                // The program itself stays as it was; nothing to tell.
            } else if job_type == audio::AUDIO_JOB {
                crate::server::websocket::broadcast_media_audio_progress(
                    &state.ws_clients,
                    &metocast_core::media::AudioProgress {
                        job_id: id,
                        state: RenderState::Cancelled,
                        step: None,
                        fraction: None,
                        output_path: String::new(),
                        error: None,
                        report: None,
                    },
                )
                .await;
            } else {
                broadcast_progress(
                    &state,
                    RenderProgress {
                        job_id: id,
                        state: RenderState::Cancelled,
                        frame: 0,
                        total_frames: 0,
                        fps: 0.0,
                        output_path: String::new(),
                        error: None,
                    },
                )
                .await;
            }
            return StatusCode::NO_CONTENT.into_response();
        }
        Ok(None) => {}
        Err(error) => return MediaError::from(error).into_response(),
    }
    match state.media.running.lock().await.remove(&id) {
        Some(stop) => {
            stop.send(()).log_gone();
            StatusCode::NO_CONTENT.into_response()
        }
        None => MediaError::NotFound("no queued or running media job with that id".to_string())
            .into_response(),
    }
}

async fn broadcast_progress(state: &AppState, progress: RenderProgress) {
    crate::server::websocket::broadcast_media_render_progress(&state.ws_clients, &progress).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_must_be_absolute_existing_video_files() {
        assert!(matches!(
            validate_source("clip.braw"),
            Err(MediaError::BadRequest(_))
        ));
        assert!(matches!(
            validate_source("/etc/passwd"),
            Err(MediaError::BadRequest(_))
        ));
        assert!(matches!(
            validate_source("/nowhere/clip.braw"),
            Err(MediaError::NotFound(_))
        ));
    }

    #[test]
    fn renders_land_beside_the_source_without_overwriting() {
        let dir = std::env::temp_dir().join(format!("metocast-media-{}", Uuid::new_v4()));
        let renders = dir.join(RENDERS_FOLDER);
        std::fs::create_dir_all(&renders).unwrap();
        let source = dir.join("Untitled CAM 1 05.braw");
        let config = MediaConfig::default();

        assert_eq!(
            output_path(&config, &source),
            renders.join("Untitled CAM 1 05.mov")
        );
        std::fs::write(renders.join("Untitled CAM 1 05.mov"), b"").unwrap();
        assert_eq!(
            output_path(&config, &source),
            renders.join("Untitled CAM 1 05 (2).mov")
        );
        let elsewhere = MediaConfig {
            output_folder: "/Volumes/Renders".to_string(),
            ..MediaConfig::default()
        };
        assert_eq!(
            output_path(&elsewhere, &source),
            PathBuf::from("/Volumes/Renders/Untitled CAM 1 05.mov")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stored_config_is_checked_before_saving() {
        assert!(validate_config(&MediaConfig::default()).is_ok());
        assert!(
            validate_config(&MediaConfig {
                output_folder: "Renders".to_string(),
                ..MediaConfig::default()
            })
            .is_err()
        );
        assert!(
            validate_config(&MediaConfig {
                lut_path: "/a:b.cube".to_string(),
                lut_mode: metocast_core::media::LutMode::File,
                ..MediaConfig::default()
            })
            .is_err()
        );
    }
}
