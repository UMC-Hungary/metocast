//! Audio masters for video: any recording (a wireless mic's WAV, the RØDECaster's FLAC, a
//! camera file's sound), cleaned by jive-vocals and handed back sample-aligned with its source,
//! so the later merge can line it up with the picture.
//!
//! The job runs on the `media` queue in five steps:
//! 1. **Download** jive-vocals on first use (see [`super::tools`]).
//! 2. **Prepare**: ffmpeg joins the files (recorders split long takes) and mixes the chosen
//!    channels to mono 48 kHz float.
//! 3. **Clean**: jive-vocals' four passes (analyse, filter, measure, normalise to −16 LUFS).
//! 4. **Align**: its noise reduction delays the audio by a recording-dependent amount, so the
//!    delay is measured against the prepared source (see [`super::align`]).
//! 5. **Finish**: ffmpeg removes that delay, keeps the exact source length, lifts the level to
//!    the target with a limiter, and writes a 24-bit Broadcast WAV stamped with the start time,
//!    with the voice on both channels (jive-vocals' loudness assumes that, and so does video).
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use chrono::{DateTime, Local, Timelike, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{oneshot, watch};
use uuid::Uuid;

use metocast_core::media::{
    AUDIO_SAMPLE_RATE, AudioProbe, AudioProbeRequest, AudioProgress, AudioReport, AudioRequest,
    AudioSettings, AudioSource, AudioStep, MediaConfig, RenderQueued, RenderState,
};

use super::{MEDIA_QUEUE, MediaError, Tools, align, load_config, partial_path, probe, tools};
use crate::queue::{self, Job};
use crate::server::AppState;

pub const AUDIO_JOB: &str = "process_audio";
const AUDIO_FOLDER: &str = "Metocast Audio";
const SOURCE_EXTENSIONS: &[&str] = &[
    "wav", "flac", "mp3", "m4a", "aac", "aif", "aiff", "caf", "ogg", "opus", "mov", "mp4", "mkv",
    "braw",
];
const MAX_SOURCES: usize = 32;
pub(super) const CANCELLED: &str = "cancelled";
pub(super) const HEARTBEAT: Duration = Duration::from_secs(30);
const PROGRESS_EVERY: Duration = Duration::from_secs(1);
const STDERR_TAIL_BYTES: usize = 4000;
/// How far the alignment looks either way: far beyond any filter delay, short of a word.
const MAX_LAG_SECONDS: u32 = 1;
const ALIGN_WINDOWS: u32 = 7;
const ALIGN_WINDOW_SECONDS: f64 = 10.0;
/// The limiter's ceiling, a little under the −1 dBTP true-peak target because it limits samples.
const LIMIT_DBFS: f64 = -1.5;

// ── Probe ─────────────────────────────────────────────────────────────────────

/// An absolute path to an existing file of a kind ffmpeg can take audio from.
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
        return Err(MediaError::BadRequest(format!(
            "unsupported audio file: {}",
            path.display()
        )));
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

pub(super) fn validate_sources(paths: &[String]) -> Result<Vec<PathBuf>, MediaError> {
    if paths.is_empty() {
        return Err(MediaError::BadRequest("pick at least one file".to_string()));
    }
    if paths.len() > MAX_SOURCES {
        return Err(MediaError::BadRequest(format!(
            "join at most {MAX_SOURCES} files"
        )));
    }
    paths.iter().map(|path| validate_source(path)).collect()
}

/// What the probe found, plus what the job needs that the wire record leaves out.
pub(super) struct ProbedAudio {
    pub probe: AudioProbe,
    pub tools: Tools,
}

pub(super) async fn probe_sources(
    state: &AppState,
    sources: &[PathBuf],
) -> Result<ProbedAudio, MediaError> {
    let config = load_config(&state.pool).await;
    let tools = Tools::find(&config, &state.media.tools_dir);
    let ffprobe = tools.ffprobe.clone().ok_or_else(|| {
        MediaError::Unavailable("ffmpeg is missing from this build of Metocast".to_string())
    })?;
    let defaults = config.audio_settings().map_err(|e| {
        MediaError::BadRequest(format!(
            "the stored Media Processing settings are invalid: {e}"
        ))
    })?;
    let mut probed = Vec::with_capacity(sources.len());
    for source in sources {
        let facts = probe::ffprobe_audio(&ffprobe, source)
            .await
            .map_err(|e| MediaError::BadRequest(format!("{}: {e}", source.display())))?;
        let meta = std::fs::metadata(source).ok();
        // Recorders without a clock chunk (the Wireless PRO) still stamp the file's birth.
        let created = meta
            .as_ref()
            .and_then(|meta| meta.created().ok())
            .map(DateTime::<Utc>::from);
        probed.push(AudioSource {
            path: source.to_string_lossy().into_owned(),
            file_name: source
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            file_size: meta.map_or(0, |meta| meta.len()),
            audio: facts.audio,
            duration_seconds: facts.duration_seconds,
            recorded_at: facts.recorded_at.or(created),
        });
    }
    let channels = probed
        .iter()
        .map(|source| source.audio.channels)
        .min()
        .unwrap_or(0);
    let mut missing = Vec::new();
    let mut downloads = Vec::new();
    if tools.ffmpeg.is_none() {
        missing.push("ffmpeg".to_string());
    }
    match (
        &tools.jive_vocals,
        config.jive_vocals_path.trim().is_empty(),
    ) {
        (Some(_), _) => {}
        (None, true) => downloads.push(tools::JIVE_VOCALS.name.to_string()),
        (None, false) => missing.push(tools::JIVE_VOCALS.name.to_string()),
    }
    let probe = AudioProbe {
        duration_seconds: probed.iter().map(|source| source.duration_seconds).sum(),
        channels,
        recorded_at: probed.first().and_then(|source| source.recorded_at),
        sources: probed,
        defaults,
        missing,
        downloads,
    };
    Ok(ProbedAudio { probe, tools })
}

pub(super) fn check_settings(settings: &AudioSettings, channels: u32) -> Result<(), MediaError> {
    settings.validate().map_err(MediaError::BadRequest)?;
    if let Some(channel) = settings.channels.iter().find(|&&c| c > channels) {
        return Err(MediaError::BadRequest(format!(
            "channel {channel} doesn't exist: the files have {channels}"
        )));
    }
    Ok(())
}

/// Where an audio master lands: the configured folder, or `Metocast Audio` in the user's
/// Music folder. Never an existing file.
fn output_path(config: &MediaConfig, first_source: &FsPath) -> PathBuf {
    let folder = match config.audio_output_folder.trim() {
        "" => std::env::var_os("HOME")
            .map_or_else(std::env::temp_dir, PathBuf::from)
            .join("Music")
            .join(AUDIO_FOLDER),
        configured => PathBuf::from(configured),
    };
    let stem = first_source.file_stem().map_or_else(
        || "audio".to_string(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    (1..)
        .map(|n| match n {
            1 => folder.join(format!("{stem}.wav")),
            n => folder.join(format!("{stem} ({n}).wav")),
        })
        .find(|path| !path.exists() && !partial_path(path).exists())
        .unwrap_or_else(|| folder.join(format!("{stem}.wav")))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AudioPayload {
    paths: Vec<String>,
    event_id: Option<Uuid>,
    settings: AudioSettings,
    output_path: String,
}

// ── REST handlers ─────────────────────────────────────────────────────────────

pub async fn probe_audio(
    State(state): State<AppState>,
    Json(body): Json<AudioProbeRequest>,
) -> impl IntoResponse {
    let result = async {
        let sources = validate_sources(&body.paths)?;
        Ok::<_, MediaError>(probe_sources(&state, &sources).await?.probe)
    }
    .await;
    match result {
        Ok(probe) => {
            tracing::info!(
                files = probe.sources.len(),
                seconds = probe.duration_seconds,
                missing = ?probe.missing,
                "media: probed audio"
            );
            Json(probe).into_response()
        }
        Err(error) => {
            tracing::warn!(paths = ?body.paths, "media: audio probe refused: {error}");
            error.into_response()
        }
    }
}

pub async fn start_audio(
    State(state): State<AppState>,
    Json(body): Json<AudioRequest>,
) -> impl IntoResponse {
    let paths = body.paths.clone();
    match queue_audio(&state, body).await {
        Ok(queued) => {
            tracing::info!(job = %queued.job_id, output = %queued.output_path, "media: audio queued");
            (StatusCode::ACCEPTED, Json(queued)).into_response()
        }
        Err(error) => {
            tracing::warn!(?paths, "media: audio refused: {error}");
            error.into_response()
        }
    }
}

async fn queue_audio(state: &AppState, body: AudioRequest) -> Result<RenderQueued, MediaError> {
    let sources = validate_sources(&body.paths)?;
    let config = load_config(&state.pool).await;
    if !config.enabled {
        return Err(MediaError::Unavailable(
            "Media Processing is turned off in its connector settings".to_string(),
        ));
    }
    let probed = probe_sources(state, &sources).await?;
    if !probed.probe.missing.is_empty() {
        return Err(MediaError::Unavailable(format!(
            "this Mac is missing: {}",
            probed.probe.missing.join(", ")
        )));
    }
    let settings = body.settings.unwrap_or(probed.probe.defaults);
    check_settings(&settings, probed.probe.channels)?;
    if let Some(event_id) = body.event_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE id = $1)")
            .bind(event_id)
            .fetch_one(&state.pool)
            .await?;
        if !exists {
            return Err(MediaError::NotFound("no such event".to_string()));
        }
    }
    let paths: Vec<String> = probed
        .probe
        .sources
        .iter()
        .map(|source| source.path.clone())
        .collect();
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM job_queue WHERE queue = $1 AND job_type = $2 \
         AND status IN ('pending', 'processing') AND payload->'paths'->>0 = $3)",
    )
    .bind(MEDIA_QUEUE)
    .bind(AUDIO_JOB)
    .bind(&paths[0])
    .fetch_one(&state.pool)
    .await?;
    if busy {
        return Err(MediaError::Conflict(
            "this recording is already queued or processing".to_string(),
        ));
    }

    let payload = AudioPayload {
        output_path: output_path(&config, &sources[0])
            .to_string_lossy()
            .into_owned(),
        paths,
        event_id: body.event_id,
        settings,
    };
    let job_id: Uuid = sqlx::query_scalar(
        "INSERT INTO job_queue (queue, job_type, payload, max_attempts) \
         VALUES ($1, $2, $3, 1) RETURNING id",
    )
    .bind(MEDIA_QUEUE)
    .bind(AUDIO_JOB)
    .bind(serde_json::to_value(&payload).map_err(|e| MediaError::Internal(e.to_string()))?)
    .fetch_one(&state.pool)
    .await?;
    state.media.wake.notify_one();
    Ok(RenderQueued {
        job_id,
        output_path: payload.output_path,
    })
}

// ── Commands (pure, unit-tested) ──────────────────────────────────────────────

pub(super) fn arg(path: &FsPath) -> String {
    path.to_string_lossy().into_owned()
}

pub(super) fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_string()).collect()
}

/// `c0=0.5*c0+0.5*c1`: the chosen 1-based channels (all of them when none are chosen) averaged.
fn mono_mix(channels: &[u32], available: u32) -> String {
    let chosen: Vec<u32> = if channels.is_empty() {
        (1..=available.max(1)).collect()
    } else {
        channels.to_vec()
    };
    let weight = 1.0 / chosen.len() as f64;
    let terms: Vec<String> = chosen
        .iter()
        .map(|channel| {
            if chosen.len() == 1 {
                format!("c{}", channel - 1)
            } else {
                format!("{weight:.6}*c{}", channel - 1)
            }
        })
        .collect();
    format!("c0={}", terms.join("+"))
}

/// Step 2: every source, in order, mixed to mono 48 kHz float and joined.
pub(super) fn prepare_args(
    sources: &[(&FsPath, u32)],
    channels: &[u32],
    prepared: &FsPath,
) -> Vec<String> {
    let mut args = strings(&["-hide_banner", "-nostdin", "-nostats", "-v", "error", "-y"]);
    args.extend(strings(&["-progress", "pipe:1"]));
    let mut graph = Vec::new();
    for (index, (path, available)) in sources.iter().enumerate() {
        args.push("-i".to_string());
        args.push(arg(path));
        graph.push(format!(
            "[{index}:a:0]aresample={AUDIO_SAMPLE_RATE},pan=mono|{}[a{index}]",
            mono_mix(channels, *available)
        ));
    }
    let inputs: String = (0..sources.len())
        .map(|index| format!("[a{index}]"))
        .collect();
    graph.push(format!("{inputs}concat=n={}:v=0:a=1[out]", sources.len()));
    args.extend(["-filter_complex".to_string(), graph.join(";")]);
    args.extend(strings(&[
        "-map",
        "[out]",
        "-c:a",
        "pcm_f32le",
        "-rf64",
        "auto",
        "-f",
        "wav",
    ]));
    args.push(arg(prepared));
    args
}

/// One alignment window: `seconds` of `file` from `start`, as mono 48 kHz float on stdout.
fn window_args(file: &FsPath, start: f64, seconds: f64) -> Vec<String> {
    let mut args = strings(&["-hide_banner", "-nostdin", "-v", "error", "-ss"]);
    args.push(format!("{start:.6}"));
    args.extend(["-i".to_string(), arg(file), "-t".to_string()]);
    args.push(format!("{seconds:.6}"));
    args.extend(strings(&["-af"]));
    args.push(format!("aresample={AUDIO_SAMPLE_RATE}"));
    args.extend(strings(&["-ac", "1", "-f", "f32le", "-"]));
    args
}

/// Broadcast WAV fields: when the audio starts, by the recorder's clock in local time.
pub(super) fn bext_metadata(recorded_at: Option<DateTime<Utc>>) -> Vec<String> {
    let mut args = strings(&["-write_bext", "1"]);
    let mut field = |key: &str, value: String| {
        args.push("-metadata".to_string());
        args.push(format!("{key}={value}"));
    };
    field("originator", "Metocast".to_string());
    if let Some(start) = recorded_at {
        let local = start.with_timezone(&Local);
        field("origination_date", local.format("%Y-%m-%d").to_string());
        field("origination_time", local.format("%H:%M:%S").to_string());
        let since_midnight = u64::from(local.num_seconds_from_midnight());
        field(
            "time_reference",
            (since_midnight * u64::from(AUDIO_SAMPLE_RATE)).to_string(),
        );
    }
    args
}

/// Step 5: undo `delay` samples (48 kHz), keep exactly `samples`, add `gain_db`, write 24-bit
/// dual-mono stereo.
pub(super) fn finish_args(
    processed: &FsPath,
    delay: i64,
    samples: u64,
    gain_db: f64,
    recorded_at: Option<DateTime<Utc>>,
    output: &FsPath,
) -> Vec<String> {
    let mut chain = vec![format!("aresample={AUDIO_SAMPLE_RATE}")];
    match delay {
        0 => {}
        late if late > 0 => {
            chain.push(format!("atrim=start_sample={late}"));
            chain.push("asetpts=PTS-STARTPTS".to_string());
        }
        early => chain.push(format!("adelay=delays={}S:all=1", -early)),
    }
    chain.push(format!("apad=whole_len={samples}"));
    chain.push(format!("atrim=end_sample={samples}"));
    if gain_db.abs() >= 0.05 {
        chain.push(format!("volume={gain_db:.2}dB"));
    }
    if gain_db > 0.0 {
        // latency=1 compensates the limiter's lookahead, so it moves nothing in time.
        let limit = 10f64.powf(LIMIT_DBFS / 20.0);
        chain.push(format!(
            "alimiter=limit={limit:.4}:level=disabled:latency=1:attack=5:release=50"
        ));
    }
    // Two identical channels: jive-vocals' −16 LUFS assumes a voice heard on both speakers, and
    // a mono track would measure 3 dB quieter than that (and play inconsistently) in a video.
    chain.push("pan=stereo|c0=c0|c1=c0".to_string());
    chain.push("ebur128=peak=true:framelog=quiet".to_string());

    let mut args = strings(&["-hide_banner", "-nostdin", "-nostats", "-v", "info", "-y"]);
    args.extend(strings(&["-progress", "pipe:1", "-i"]));
    args.push(arg(processed));
    args.extend(["-af".to_string(), chain.join(",")]);
    args.extend(strings(&["-c:a", "pcm_s24le", "-rf64", "auto"]));
    args.extend(bext_metadata(recorded_at));
    args.extend(strings(&["-f", "wav"]));
    args.push(arg(output));
    args
}

/// Where the windows sit: spread through the middle of the recording, clear of both ends.
fn window_starts(duration: f64) -> Result<(f64, Vec<f64>), String> {
    let margin = f64::from(MAX_LAG_SECONDS);
    let length = ALIGN_WINDOW_SECONDS.min(duration - 2.0 * margin - 0.5);
    if length < 1.0 {
        return Err("the recording is too short to process (under 3.5 seconds)".to_string());
    }
    let span = duration - 2.0 * margin - length;
    let starts = (1..=ALIGN_WINDOWS)
        .map(|n| (margin + span * f64::from(n) / f64::from(ALIGN_WINDOWS + 1)).floor())
        .collect();
    Ok((length, starts))
}

// ── Reading tool output ──────────────────────────────────────────────────────

/// `I:` and the true `Peak:` from ffmpeg's ebur128 summary.
pub(super) fn parse_ebur128(stderr: &str) -> Option<(f64, f64)> {
    let summary = &stderr[stderr.rfind("Summary:")?..];
    let value = |section: &str, key: &str| -> Option<f64> {
        let rest = &summary[summary.find(section)?..];
        let line = rest
            .lines()
            .find(|line| line.trim_start().starts_with(key))?;
        line.trim_start()
            .trim_start_matches(key)
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    };
    Some((
        value("Integrated loudness", "I:")?,
        value("True peak", "Peak:")?,
    ))
}

/// Input and output integrated loudness from jive-vocals' JSON report.
pub(super) fn parse_jive_report(report: &Value) -> Result<(f64, f64), String> {
    let stage = |name: &str| report["loudness"]["stages"][name]["integrated_lufs"].as_f64();
    match (stage("input"), stage("final")) {
        (Some(input), Some(output)) => Ok((input, output)),
        _ => Err("jive-vocals' report has no loudness figures".to_string()),
    }
}

pub(super) fn samples_of(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

// ── The job ───────────────────────────────────────────────────────────────────

/// Runs one `process_audio` job; the worker records the outcome.
pub(super) async fn process(state: &AppState, job: &Job) -> Result<(), String> {
    let payload: AudioPayload = serde_json::from_value(job.payload.clone())
        .map_err(|e| format!("unreadable job payload: {e}"))?;
    let output = PathBuf::from(&payload.output_path);
    let partial = partial_path(&output);
    let folder = output.parent().map(FsPath::to_path_buf).unwrap_or_default();
    let work = folder.join(format!(".metocast-audio-{}", job.id));

    let (stop, stopped) = oneshot::channel();
    state.media.running.lock().await.insert(job.id, stop);
    let (cancel_tx, cancel) = watch::channel(false);
    let cancel_relay = tokio::spawn(async move {
        if stopped.await.is_ok() {
            let _ = cancel_tx.send(true);
        }
    });
    let heartbeat = {
        let pool = state.pool.clone();
        let job_id = job.id;
        tokio::spawn(async move {
            let mut beat = tokio::time::interval(HEARTBEAT);
            loop {
                beat.tick().await;
                if let Err(e) = queue::heartbeat(&pool, job_id).await {
                    tracing::warn!("media: heartbeat for {job_id} failed: {e}");
                }
            }
        })
    };
    tracing::info!(job = %job.id, sources = ?payload.paths, output = %payload.output_path, "media: processing audio");

    let reporter = Reporter {
        state,
        job_id: job.id,
        output_path: payload.output_path.clone(),
    };
    let result = run_steps(state, &payload, &reporter, &work, &partial, &output, cancel).await;
    heartbeat.abort();
    cancel_relay.abort();
    state.media.running.lock().await.remove(&job.id);
    let _ = std::fs::remove_dir_all(&work);

    match result {
        Ok(report) => {
            tracing::info!(job = %job.id, output = %payload.output_path, ?report, "media: audio processed");
            reporter
                .send(RenderState::Succeeded, None, Some(1.0), None, Some(report))
                .await;
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            if error == CANCELLED {
                reporter
                    .send(RenderState::Cancelled, None, None, None, None)
                    .await;
            } else {
                reporter
                    .send(RenderState::Failed, None, None, Some(error.clone()), None)
                    .await;
            }
            Err(error)
        }
    }
}

struct Reporter<'a> {
    state: &'a AppState,
    job_id: Uuid,
    output_path: String,
}

/// Where a running step reports how far it got (0–1). The audio job and the program job each
/// turn that into their own progress message.
pub(super) trait StepProgress: Sync {
    fn fraction(&self, fraction: f64) -> impl Future<Output = ()> + Send;
}

/// One of the audio job's steps, reporting through its [`Reporter`].
struct AudioStepProgress<'a> {
    reporter: &'a Reporter<'a>,
    step: AudioStep,
}

impl StepProgress for AudioStepProgress<'_> {
    async fn fraction(&self, fraction: f64) {
        self.reporter.step(self.step, Some(fraction)).await;
    }
}

impl Reporter<'_> {
    async fn send(
        &self,
        state: RenderState,
        step: Option<AudioStep>,
        fraction: Option<f64>,
        error: Option<String>,
        report: Option<AudioReport>,
    ) {
        let progress = AudioProgress {
            job_id: self.job_id,
            state,
            step,
            fraction,
            output_path: self.output_path.clone(),
            error,
            report,
        };
        crate::server::websocket::broadcast_media_audio_progress(&self.state.ws_clients, &progress)
            .await;
    }

    async fn step(&self, step: AudioStep, fraction: Option<f64>) {
        self.send(RenderState::Running, Some(step), fraction, None, None)
            .await;
    }
}

async fn run_steps(
    state: &AppState,
    payload: &AudioPayload,
    reporter: &Reporter<'_>,
    work: &FsPath,
    partial: &FsPath,
    output: &FsPath,
    cancel: watch::Receiver<bool>,
) -> Result<AudioReport, String> {
    let sources: Vec<PathBuf> = validate_sources(&payload.paths).map_err(MediaError::message)?;
    let probed = probe_sources(state, &sources)
        .await
        .map_err(MediaError::message)?;
    check_settings(&payload.settings, probed.probe.channels).map_err(MediaError::message)?;
    let ffmpeg = probed
        .tools
        .ffmpeg
        .clone()
        .ok_or("ffmpeg is not installed")?;
    std::fs::create_dir_all(work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;

    // 1. Download
    let jive = match probed.tools.jive_vocals.clone() {
        Some(path) => path,
        None => {
            reporter.step(AudioStep::Download, None).await;
            tools::ensure(&tools::JIVE_VOCALS, &state.media.tools_dir).await?
        }
    };

    // 2. Prepare
    reporter.step(AudioStep::Prepare, Some(0.0)).await;
    let prepared = work.join("prepared.wav");
    let inputs: Vec<(&FsPath, u32)> = sources
        .iter()
        .zip(&probed.probe.sources)
        .map(|(path, source)| (path.as_path(), source.audio.channels))
        .collect();
    let total = probed.probe.duration_seconds;
    run_ffmpeg(
        &ffmpeg,
        &prepare_args(&inputs, &payload.settings.channels, &prepared),
        Some((
            total,
            &AudioStepProgress {
                reporter,
                step: AudioStep::Prepare,
            },
        )),
        cancel.clone(),
    )
    .await?;
    let ffprobe = probed
        .tools
        .ffprobe
        .clone()
        .ok_or("ffprobe is not installed")?;
    let samples = sample_count(&ffprobe, &prepared).await?;
    let duration = samples as f64 / f64::from(AUDIO_SAMPLE_RATE);

    // 3. Clean
    reporter.step(AudioStep::Clean, None).await;
    run_jive_vocals(&jive, &prepared, work, cancel.clone()).await?;
    let (processed, report_json, report_md) = jive_outputs(work)?;
    let jive_report: Value = serde_json::from_slice(
        &std::fs::read(&report_json)
            .map_err(|e| format!("cannot read jive-vocals' report: {e}"))?,
    )
    .map_err(|e| format!("unreadable jive-vocals report: {e}"))?;
    let (input_lufs, cleaned_lufs) = parse_jive_report(&jive_report)?;

    // 4. Align
    reporter.step(AudioStep::Align, Some(0.0)).await;
    let delay = measure_delay(
        &ffmpeg,
        &prepared,
        &processed,
        duration,
        &AudioStepProgress {
            reporter,
            step: AudioStep::Align,
        },
        cancel.clone(),
    )
    .await?;

    // 5. Finish
    reporter.step(AudioStep::Finish, Some(0.0)).await;
    let gain_db = f64::from(payload.settings.target_lufs) - cleaned_lufs;
    let stderr = run_ffmpeg(
        &ffmpeg,
        &finish_args(
            &processed,
            delay.lag,
            samples,
            gain_db,
            probed.probe.recorded_at,
            partial,
        ),
        Some((
            duration,
            &AudioStepProgress {
                reporter,
                step: AudioStep::Finish,
            },
        )),
        cancel,
    )
    .await?;
    let (output_lufs, true_peak) =
        parse_ebur128(&stderr).ok_or("ffmpeg did not report the final loudness")?;
    std::fs::rename(partial, output)
        .map_err(|e| format!("cannot move the audio into place: {e}"))?;
    let report_copy = output.with_extension("jive-vocals.md");
    if let Err(e) = std::fs::copy(&report_md, &report_copy) {
        tracing::warn!("media: could not keep jive-vocals' report: {e}");
    }

    let report = AudioReport {
        input_lufs,
        output_lufs,
        output_true_peak_dbtp: true_peak,
        removed_delay_samples: delay.lag,
        alignment_confidence: delay.score.clamp(0.0, 1.0),
    };
    register(state, payload, &probed.probe, output, duration, &report).await;
    Ok(report)
}

pub(super) async fn sample_count(ffprobe: &FsPath, file: &FsPath) -> Result<u64, String> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=duration_ts",
        ])
        .args(["-of", "default=noprint_wrappers=1:nokey=1"])
        .arg(file)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("could not run ffprobe: {e}"))?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()
        .filter(|samples| *samples > 0)
        .ok_or_else(|| "the prepared audio has no samples".to_string())
}

/// jive-vocals names its outputs after the input: `prepared-LUFS-16-processed.{flac,json,md}`.
pub(super) fn jive_outputs(work: &FsPath) -> Result<(PathBuf, PathBuf, PathBuf), String> {
    let entries =
        std::fs::read_dir(work).map_err(|e| format!("cannot read {}: {e}", work.display()))?;
    let flac = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension().is_some_and(|ext| ext == "flac")
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with("prepared-"))
        })
        .ok_or("jive-vocals produced no audio")?;
    Ok((
        flac.clone(),
        flac.with_extension("json"),
        flac.with_extension("md"),
    ))
}

/// How far jive-vocals delayed `processed` against `prepared`, in 48 kHz samples.
pub(super) async fn measure_delay(
    ffmpeg: &FsPath,
    prepared: &FsPath,
    processed: &FsPath,
    duration: f64,
    progress: &impl StepProgress,
    cancel: watch::Receiver<bool>,
) -> Result<align::WindowLag, String> {
    let (length, starts) = window_starts(duration)?;
    let margin = f64::from(MAX_LAG_SECONDS);
    let max_lag = (MAX_LAG_SECONDS * AUDIO_SAMPLE_RATE) as usize;
    let mut windows = Vec::with_capacity(starts.len());
    for (index, start) in starts.iter().enumerate() {
        if *cancel.borrow() {
            return Err(CANCELLED.to_string());
        }
        let source = read_window(ffmpeg, prepared, *start, length).await?;
        let cleaned = read_window(ffmpeg, processed, start - margin, length + 2.0 * margin).await?;
        let found =
            tokio::task::spawn_blocking(move || align::window_lag(&source, &cleaned, max_lag))
                .await
                .map_err(|e| format!("alignment failed: {e}"))?;
        if let Some(found) = found {
            windows.push(found);
        }
        progress
            .fraction((index + 1) as f64 / starts.len() as f64)
            .await;
    }
    tracing::info!(?windows, "media: alignment windows");
    align::consensus(&windows).ok_or_else(|| {
        "could not line the cleaned audio up with the source; nothing was written".to_string()
    })
}

async fn read_window(
    ffmpeg: &FsPath,
    file: &FsPath,
    start: f64,
    seconds: f64,
) -> Result<Vec<f32>, String> {
    let output = Command::new(ffmpeg)
        .args(window_args(file, start, seconds))
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("could not run ffmpeg: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "ffmpeg could not read {}: {}",
            file.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(samples_of(&output.stdout))
}

/// Runs ffmpeg to the end, reporting `out_time` against `total` seconds; returns its stderr tail.
pub(super) async fn run_ffmpeg(
    ffmpeg: &FsPath,
    args: &[String],
    progress: Option<(f64, &impl StepProgress)>,
    mut cancel: watch::Receiver<bool>,
) -> Result<String, String> {
    let mut child = Command::new(ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not start ffmpeg: {e}"))?;
    let errors = child.stderr.take().map(|stderr| tokio::spawn(tail(stderr)));
    let mut lines = child
        .stdout
        .take()
        .map(|stdout| BufReader::new(stdout).lines());
    let mut last_sent: Option<Instant> = None;
    let status = loop {
        tokio::select! {
            status = child.wait() => break status.map_err(|e| format!("ffmpeg: {e}"))?,
            Ok(Some(line)) = async {
                match lines.as_mut() {
                    Some(lines) => lines.next_line().await,
                    None => std::future::pending().await,
                }
            } => {
                if let (Some((total, progress)), Some(micros)) =
                    (progress, line.strip_prefix("out_time_us="))
                    && last_sent.is_none_or(|sent| sent.elapsed() >= PROGRESS_EVERY)
                    && let Ok(micros) = micros.trim().parse::<f64>()
                    && total > 0.0
                {
                    last_sent = Some(Instant::now());
                    progress.fraction((micros / 1e6 / total).clamp(0.0, 1.0)).await;
                }
            }
            () = cancelled(&mut cancel) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(CANCELLED.to_string());
            }
        }
    };
    let stderr = match errors {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    if !status.success() {
        let detail = stderr.trim().lines().last().unwrap_or_default().to_string();
        return Err(format!("ffmpeg failed: {detail}"));
    }
    Ok(stderr)
}

/// jive-vocals draws a terminal UI and refuses to start without a terminal, so it runs under
/// `script`, which gives it a pseudo-terminal. Its own process group lets a cancel stop both.
pub(super) async fn run_jive_vocals(
    jive: &FsPath,
    input: &FsPath,
    work: &FsPath,
    mut cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let mut child = Command::new("/usr/bin/script")
        .arg("-q")
        .arg("/dev/null")
        .arg(jive)
        .arg(input)
        .current_dir(work)
        .env("TERM", "xterm-256color")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not start jive-vocals: {e}"))?;
    let screen = child.stdout.take().map(|stdout| tokio::spawn(tail(stdout)));
    let status = tokio::select! {
        status = child.wait() => status.map_err(|e| format!("jive-vocals: {e}"))?,
        () = cancelled(&mut cancel) => {
            if let Some(group) = child.id() {
                let _ = Command::new("/bin/kill")
                    .args(["-KILL", "--", &format!("-{group}")])
                    .status()
                    .await;
            }
            let _ = child.wait().await;
            return Err(CANCELLED.to_string());
        }
    };
    let screen = match screen {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    if !status.success() {
        let detail = plain_text(&screen);
        let detail = detail
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        return Err(format!("jive-vocals failed: {}", detail.trim()));
    }
    Ok(())
}

/// Resolves once the job is cancelled; never, if the cancel handle is gone.
pub(super) async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    if cancel.wait_for(|stop| *stop).await.is_err() {
        std::future::pending::<()>().await;
    }
}

/// A terminal screen without its escape sequences.
fn plain_text(screen: &str) -> String {
    let mut text = String::with_capacity(screen.len());
    let mut chars = screen.chars();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() || c == '~' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    for c in chars.by_ref() {
                        if c == '\u{7}' {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\r' => text.push('\n'),
            c => text.push(c),
        }
    }
    text
}

async fn tail(mut reader: impl tokio::io::AsyncRead + Unpin) -> String {
    let mut kept: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    while let Ok(read) = reader.read(&mut chunk).await {
        if read == 0 {
            break;
        }
        kept.extend_from_slice(&chunk[..read]);
        if kept.len() > STDERR_TAIL_BYTES {
            kept.drain(..kept.len() - STDERR_TAIL_BYTES);
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// Files the master under its event, or as untracked, with the timeline facts the merge uses.
async fn register(
    state: &AppState,
    payload: &AudioPayload,
    probe: &AudioProbe,
    output: &FsPath,
    duration: f64,
    report: &AudioReport,
) {
    let metadata = json!({
        "source": "audio_master",
        "timeline": {
            "recordedAt": probe.recorded_at,
            "clockSource": "file_created",
            "sampleRate": AUDIO_SAMPLE_RATE,
            "sourcePaths": payload.paths,
            "sourceDurations": probe.sources.iter().map(|s| s.duration_seconds).collect::<Vec<_>>(),
        },
        "audio": payload.settings,
        "report": report,
        "processedWith": format!("jive-vocals {}", tools::JIVE_VOCALS.version),
    });

    super::file_recording(
        state,
        payload.event_id,
        output,
        duration,
        "audio",
        "audio_master",
        &metadata,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn audio_sources_must_be_absolute_existing_media_files() {
        assert!(matches!(
            validate_source("mic.wav"),
            Err(MediaError::BadRequest(_))
        ));
        assert!(matches!(
            validate_source("/etc/passwd"),
            Err(MediaError::BadRequest(_))
        ));
        assert!(matches!(
            validate_source("/nowhere/00015_Wireless_PRO.WAV"),
            Err(MediaError::NotFound(_))
        ));
        assert!(matches!(
            validate_sources(&[]),
            Err(MediaError::BadRequest(_))
        ));
    }

    #[test]
    fn channels_are_averaged_into_one_voice_track() {
        assert_eq!(mono_mix(&[], 1), "c0=c0");
        assert_eq!(mono_mix(&[], 2), "c0=0.500000*c0+0.500000*c1");
        assert_eq!(mono_mix(&[3], 4), "c0=c2");
    }

    #[test]
    fn split_recordings_are_joined_in_order_at_48_khz() {
        let args = prepare_args(
            &[
                (FsPath::new("/Volumes/WirelessPRO/00015.WAV"), 1),
                (FsPath::new("/Volumes/WirelessPRO/00016.WAV"), 1),
            ],
            &[],
            FsPath::new("/work/prepared.wav"),
        );
        let graph = &args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1];
        assert_eq!(
            graph,
            "[0:a:0]aresample=48000,pan=mono|c0=c0[a0];[1:a:0]aresample=48000,pan=mono|c0=c0[a1];\
             [a0][a1]concat=n=2:v=0:a=1[out]"
        );
        assert_eq!(args.last().unwrap(), "/work/prepared.wav");
        assert!(args.windows(2).any(|w| w == ["-c:a", "pcm_f32le"]));
    }

    #[test]
    fn finishing_removes_the_delay_keeps_the_length_and_lifts_to_the_target() {
        let args = finish_args(
            FsPath::new("/work/prepared-LUFS-16-processed.flac"),
            1582,
            5_763_072,
            2.0,
            None,
            FsPath::new("/out/00015.wav.partial"),
        );
        let chain = &args[args.iter().position(|a| a == "-af").unwrap() + 1];
        assert_eq!(
            chain,
            "aresample=48000,atrim=start_sample=1582,asetpts=PTS-STARTPTS,\
             apad=whole_len=5763072,atrim=end_sample=5763072,volume=2.00dB,\
             alimiter=limit=0.8414:level=disabled:latency=1:attack=5:release=50,\
             pan=stereo|c0=c0|c1=c0,ebur128=peak=true:framelog=quiet"
        );
        assert!(args.windows(2).any(|w| w == ["-c:a", "pcm_s24le"]));
        assert!(args.windows(2).any(|w| w == ["-write_bext", "1"]));

        let early = finish_args(
            FsPath::new("/p.flac"),
            -40,
            100,
            0.0,
            None,
            FsPath::new("/o"),
        );
        let chain = &early[early.iter().position(|a| a == "-af").unwrap() + 1];
        assert!(chain.contains("adelay=delays=40S:all=1"));
        assert!(!chain.contains("volume") && !chain.contains("alimiter"));
    }

    #[test]
    fn the_start_time_is_stamped_as_samples_since_local_midnight() {
        let start = Local
            .with_ymd_and_hms(2026, 10, 4, 10, 16, 44)
            .unwrap()
            .with_timezone(&Utc);
        let args = bext_metadata(Some(start));
        assert!(args.contains(&"origination_date=2026-10-04".to_string()));
        assert!(args.contains(&"origination_time=10:16:44".to_string()));
        let samples = (10 * 3600 + 16 * 60 + 44) * 48_000u64;
        assert!(args.contains(&format!("time_reference={samples}")));
    }

    #[test]
    fn alignment_windows_stay_clear_of_the_ends() {
        let (length, starts) = window_starts(5_400.0).unwrap();
        assert_eq!(length, ALIGN_WINDOW_SECONDS);
        assert_eq!(starts.len(), ALIGN_WINDOWS as usize);
        assert!(
            starts
                .iter()
                .all(|s| *s >= 1.0 && s + length + 1.0 <= 5_400.0)
        );
        let (short, _) = window_starts(6.0).unwrap();
        assert!((short - 3.5).abs() < 1e-9);
        assert!(window_starts(3.0).is_err());
    }

    #[test]
    fn the_final_loudness_is_read_from_ebur128() {
        let stderr = "[Parsed_ebur128_7 @ 0x1] Summary:\n\n  Integrated loudness:\n    I:         -14.1 LUFS\n    Threshold: -24.6 LUFS\n\n  Loudness range:\n    LRA:         8.7 LU\n\n  True peak:\n    Peak:       -1.3 dBFS\n";
        assert_eq!(parse_ebur128(stderr), Some((-14.1, -1.3)));
        assert_eq!(parse_ebur128("no summary"), None);
    }

    #[test]
    fn jive_vocals_report_gives_the_loudness_before_and_after() {
        let report = json!({"loudness": {"target_i_lufs": -16, "stages": {
            "input": {"integrated_lufs": -41.837}, "final": {"integrated_lufs": -16.242}}}});
        assert_eq!(parse_jive_report(&report).unwrap(), (-41.837, -16.242));
        assert!(parse_jive_report(&json!({})).is_err());
    }

    #[test]
    fn terminal_escapes_are_stripped_from_jive_vocals_output() {
        let screen = "\u{1b}]11;?\u{7}\u{1b}[?2026$pError: no audio\r\n";
        assert_eq!(plain_text(screen).trim(), "Error: no audio");
    }

    #[test]
    fn masters_go_to_the_music_folder_unless_configured() {
        let config = MediaConfig {
            audio_output_folder: "/Volumes/A002/Audio".to_string(),
            ..MediaConfig::default()
        };
        assert_eq!(
            output_path(
                &config,
                FsPath::new("/Volumes/WirelessPRO 1/00015_Wireless_PRO.WAV")
            ),
            PathBuf::from("/Volumes/A002/Audio/00015_Wireless_PRO.wav")
        );
        let default = output_path(&MediaConfig::default(), FsPath::new("/x/mic.flac"));
        assert!(default.ends_with("Music/Metocast Audio/mic.wav"));
    }

    /// Runs the whole job on a real recording with the real tools, then checks the result is
    /// sample-aligned with the source. It needs ffmpeg, jive-vocals and a file (or several parts
    /// separated by `|`, joined in order), so it runs only on request:
    ///
    /// ```text
    /// METOCAST_TEST_AUDIO=/path/to/slice.wav METOCAST_JIVE_VOCALS=/path/to/jive-vocals \
    /// cargo test -p metocast-server real_audio -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "needs ffmpeg, jive-vocals and an audio file"]
    async fn a_real_audio_file_comes_out_clean_loud_and_aligned() {
        let sources: Vec<PathBuf> = std::env::var("METOCAST_TEST_AUDIO")
            .expect("METOCAST_TEST_AUDIO")
            .split('|')
            .map(PathBuf::from)
            .collect();
        let jive =
            PathBuf::from(std::env::var("METOCAST_JIVE_VOCALS").expect("METOCAST_JIVE_VOCALS"));
        let tools = Tools::find(&MediaConfig::default(), FsPath::new("/nonexistent"));
        let (ffmpeg, ffprobe) = (tools.ffmpeg.unwrap(), tools.ffprobe.unwrap());
        let mut inputs = Vec::new();
        for source in &sources {
            let facts = probe::ffprobe_audio(&ffprobe, source).await.unwrap();
            inputs.push((source.as_path(), facts.audio.channels));
        }
        let work = std::env::temp_dir().join(format!("metocast-audio-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&work).unwrap();
        let (_keep, cancel) = watch::channel(false);

        let prepared = work.join("prepared.wav");
        run_ffmpeg(
            &ffmpeg,
            &prepare_args(&inputs, &[], &prepared),
            None::<(f64, &AudioStepProgress)>,
            cancel.clone(),
        )
        .await
        .unwrap();
        let samples = sample_count(&ffprobe, &prepared).await.unwrap();
        run_jive_vocals(&jive, &prepared, &work, cancel.clone())
            .await
            .unwrap();
        let (processed, report, _) = jive_outputs(&work).unwrap();
        let report: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        let (_, cleaned) = parse_jive_report(&report).unwrap();

        let duration = samples as f64 / 48_000.0;
        let (length, starts) = window_starts(duration).unwrap();
        let max_lag = 48_000;
        let mut windows = Vec::new();
        for start in &starts {
            let a = read_window(&ffmpeg, &prepared, *start, length)
                .await
                .unwrap();
            let b = read_window(&ffmpeg, &processed, start - 1.0, length + 2.0)
                .await
                .unwrap();
            windows.extend(align::window_lag(&a, &b, max_lag));
        }
        let delay = align::consensus(&windows).expect("windows agree");
        assert!(delay.lag > 0, "jive-vocals delays its output: {windows:?}");

        let output = work.join("master.wav");
        let stderr = run_ffmpeg(
            &ffmpeg,
            &finish_args(
                &processed,
                delay.lag,
                samples,
                -14.0 - cleaned,
                None,
                &output,
            ),
            None::<(f64, &AudioStepProgress)>,
            cancel,
        )
        .await
        .unwrap();
        let (lufs, peak) = parse_ebur128(&stderr).unwrap();
        assert!((lufs + 14.0).abs() < 0.6, "{lufs} LUFS");
        assert!(peak <= -0.9, "{peak} dBTP");
        assert_eq!(sample_count(&ffprobe, &output).await.unwrap(), samples);

        // The master lines up with the source to the sample.
        let mut after = Vec::new();
        for start in &starts {
            let a = read_window(&ffmpeg, &prepared, *start, length)
                .await
                .unwrap();
            let b = read_window(&ffmpeg, &output, start - 1.0, length + 2.0)
                .await
                .unwrap();
            after.extend(align::window_lag(&a, &b, max_lag));
        }
        assert_eq!(
            align::consensus(&after).map(|w| w.lag),
            Some(0),
            "{after:?}"
        );
        std::fs::remove_dir_all(&work).unwrap();
    }
}
