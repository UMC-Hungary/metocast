//! Programs: a video and its audio in one file, the video the master. Drop a camera clip and
//! the recording of the same service (or a video alone, to use its own sound) and the `media`
//! queue makes one file whose audio covers exactly the picture's length.
//!
//! The `produce_program` job runs in these steps:
//! 1. **Picture:** the phase 1 render (Blackmagic RAW always; other video when asked), or the
//!    video stream as it is.
//! 2. **Prepare:** the audio files joined and mixed to one 48 kHz voice track (as in [`audio`]).
//! 3. **Sync:** where in that track the picture's first frame falls (see [`sync`]): by the
//!    video's own sound when it has any, else by the speaker's lips, else by the clocks.
//! 4. **Clean:** jive-vocals on just the stretch the picture needs, its delay removed, lifted to
//!    the loudness target.
//! 5. **Merge:** that audio placed under the picture (drift corrected, silence where it doesn't
//!    reach), written beside it as the program's own WAV and muxed with the copied picture.
//!
//! A `nudge_program` job moves a finished program's audio by whole frames: the WAV is shifted
//! and the picture copied again, which takes seconds.
use metocast_core::best_effort::{LogFailed, LogGone};
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use std::time::Instant;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, watch};
use uuid::Uuid;

use metocast_core::media::{
    AUDIO_SAMPLE_RATE, AudioReport, AudioSettings, ClockEstimate, LutMode, MediaConfig, MediaKind,
    MediaProbe, NudgeRequest, PORTRAIT_MAX_BITRATE_MBPS, PORTRAIT_SOURCE, PictureMode,
    ProgramProbe, ProgramProbeRequest, ProgramProgress, ProgramReport, ProgramRequest, ProgramStep,
    RenderQueued, RenderSettings, RenderState, SubtitleReport, SubtitleSource, SubtitlesRequest,
    SyncMethod, SyncReport, VideoSound, output_crop, portrait_crop,
};

use super::audio::{self, CANCELLED, HEARTBEAT, StepProgress, arg, strings};
use super::sync::{self, ENVELOPE_RATE, Envelope};
use super::{
    MEDIA_QUEUE, MediaError, PortraitOutput, Probed, RENDERS_FOLDER, Tools, command,
    file_recording, load_config, partial_path, probe_source, subtitles, tools, validate_source,
    whisper, worker,
};
use crate::queue::{self, Job};
use crate::server::AppState;

pub const PROGRAM_JOB: &str = "produce_program";
pub const NUDGE_JOB: &str = "nudge_program";
pub const SUBTITLES_JOB: &str = "subtitle_program";

/// Lip windows: four 90-second stretches find a sermon in an hour of audio at 12σ or more.
const LIP_WINDOWS: u32 = 4;
const LIP_WINDOW_SECONDS: f64 = 90.0;
/// The video's own sound matches far more strongly than lips, so shorter stretches do.
const SOUND_WINDOWS: u32 = 4;
const SOUND_WINDOW_SECONDS: f64 = 60.0;
/// Audio kept either side of the picture while cleaning, so the filters settle before it.
const SPAN_MARGIN_SECONDS: f64 = 2.0;
/// The quietest sound worth syncing by; the Cinema Camera's unconnected input reads −97.
const USABLE_SOUND_DBFS: f64 = -75.0;
const SOUND_CHECK_SECONDS: f64 = 5.0;
/// How long a learned clock difference between a camera and a recorder is trusted.
const LEARNED_CLOCK_DAYS: i64 = 7;
/// A clip shorter than this can't stand out over a whole recording by its lips alone.
const SHORT_CLIP_SECONDS: f64 = 120.0;
const MAX_NUDGE_FRAMES: i32 = 240;
/// Drift larger than this is a measuring error, not a clock.
const MAX_DRIFT_PPM: f64 = 300.0;
/// Timecodes that mean "no clock": what editors and recorders start at by default.
const PLACEHOLDER_TIMECODES: &[&str] = &["00:00:00:00", "01:00:00:00"];

// ── Probe ─────────────────────────────────────────────────────────────────────

/// What a video and its audio are, plus what the job needs that the wire record leaves out.
struct Facts {
    probe: ProgramProbe,
    video: Probed,
    video_path: PathBuf,
    /// The files the audio comes from: the recording's, or the video itself when it's alone.
    audio_sources: Vec<PathBuf>,
    /// `None` when the video is alone and has no sound.
    audio: Option<audio::ProbedAudio>,
    video_clock: Option<f64>,
    audio_clock: Option<f64>,
    /// Where the raw clocks put the start.
    raw_clock_offset: Option<f64>,
    /// Where the clocks put it, corrected by a recent confident sync of the same camera.
    learned_clock_offset: Option<f64>,
}

impl Facts {
    fn alone(&self) -> bool {
        self.probe.audio.is_none()
    }

    fn frame_seconds(&self) -> f64 {
        f64::from(self.video.probe.fps_den) / f64::from(self.video.probe.fps_num)
    }

    fn audio_recorded_at(&self) -> Option<DateTime<Utc>> {
        self.audio
            .as_ref()
            .and_then(|audio| audio.probe.recorded_at)
    }
}

/// The picture's start as seconds since local midnight, by its own clock: the host's record
/// log, else the camera's timecode. A container's creation time only counts without a timecode,
/// since an export's is when it was exported.
fn video_clock(probe: &MediaProbe) -> Option<f64> {
    if let Some(started) = probe.host_record_started_at {
        return Some(sync::local_seconds(started));
    }
    let fps = f64::from(probe.fps_num) / f64::from(probe.fps_den);
    match probe.start_timecode.as_deref() {
        Some(tc) if PLACEHOLDER_TIMECODES.contains(&tc) => None,
        Some(tc) => sync::timecode_seconds(tc, fps),
        None => probe.recorded_at.map(sync::local_seconds),
    }
}

/// The mean level (dBFS) `ffmpeg volumedetect` reports.
fn parse_mean_volume(stderr: &str) -> Option<f64> {
    let line = stderr.lines().find(|line| line.contains("mean_volume:"))?;
    line.split("mean_volume:")
        .nth(1)?
        .trim()
        .trim_end_matches("dB")
        .trim()
        .parse()
        .ok()
}

/// The video's own sound, measured over three short stretches.
async fn measure_sound(ffmpeg: Option<&FsPath>, video: &FsPath, probe: &MediaProbe) -> VideoSound {
    let has_track = probe.audio.as_ref().is_some_and(|audio| audio.channels > 0);
    let Some(ffmpeg) = ffmpeg.filter(|_| has_track) else {
        return VideoSound {
            has_track,
            usable: false,
            level_dbfs: None,
        };
    };
    let mut loudest: Option<f64> = None;
    for share in [0.25, 0.5, 0.75] {
        let start = (probe.duration_seconds * share - SOUND_CHECK_SECONDS / 2.0).max(0.0);
        let mut args = strings(&["-hide_banner", "-nostdin", "-nostats", "-ss"]);
        args.push(format!("{start:.3}"));
        args.extend(["-t".to_string(), format!("{SOUND_CHECK_SECONDS}")]);
        args.extend(["-i".to_string(), arg(video)]);
        args.extend(strings(&[
            "-map",
            "0:a:0",
            "-af",
            "volumedetect",
            "-f",
            "null",
            "-",
        ]));
        let Ok(output) = Command::new(ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
        else {
            continue;
        };
        if let Some(level) = parse_mean_volume(&String::from_utf8_lossy(&output.stderr)) {
            loudest = Some(loudest.map_or(level, |loudest: f64| loudest.max(level)));
        }
    }
    VideoSound {
        has_track,
        usable: loudest.is_some_and(|level| level > USABLE_SOUND_DBFS),
        level_dbfs: loudest.map(|level| (level * 10.0).round() / 10.0),
    }
}

/// The clock difference (picture minus audio) a confident sync of the same camera found in the
/// last few days around `around`.
async fn learned_difference(
    state: &AppState,
    camera: Option<&str>,
    around: DateTime<Utc>,
) -> Option<f64> {
    let days = ChronoDuration::days(LEARNED_CLOCK_DAYS);
    let found: Result<Option<f64>, sqlx::Error> = sqlx::query_scalar(
        "SELECT (metadata->'report'->'sync'->>'clockDifferenceSeconds')::float8 FROM ( \
           SELECT metadata, detected_at FROM recordings WHERE source = 'program' \
           UNION ALL \
           SELECT metadata, detected_at FROM untracked_recordings WHERE metadata->>'source' = 'program' \
         ) programs \
         WHERE metadata->'report'->'sync'->>'confident' = 'true' \
           AND metadata->'report'->'sync'->>'clockDifferenceSeconds' IS NOT NULL \
           AND metadata->'timeline'->>'camera' IS NOT DISTINCT FROM $1 \
           AND (metadata->'timeline'->>'audioRecordedAt')::timestamptz BETWEEN $2 AND $3 \
         ORDER BY detected_at DESC LIMIT 1",
    )
    .bind(camera)
    .bind(around - days)
    .bind(around + days)
    .fetch_optional(&state.pool)
    .await;
    match found {
        Ok(difference) => difference,
        Err(e) => {
            tracing::warn!("media: could not look up a learned clock difference: {e}");
            None
        }
    }
}

/// `5:47`, `1:05:47` past an hour, with a minus sign before the start.
fn minutes(seconds: f64) -> String {
    let sign = if seconds < -0.5 { "−" } else { "" };
    let total = seconds.abs().round() as u64;
    match total / 3600 {
        0 => format!("{sign}{}:{:02}", total / 60, total % 60),
        hours => format!("{sign}{hours}:{:02}:{:02}", total / 60 % 60, total % 60),
    }
}

async fn examine(
    state: &AppState,
    video_path: &str,
    audio_paths: &[String],
) -> Result<Facts, MediaError> {
    let video_path = validate_source(video_path)?;
    let video = probe_source(state, &video_path, None).await?;
    let alone = audio_paths.is_empty();
    let (audio_sources, audio) = if alone {
        let sources = vec![video_path.clone()];
        let probed = audio::probe_sources(state, &sources).await.ok();
        (sources, probed)
    } else {
        let sources = audio::validate_sources(audio_paths)?;
        if sources.contains(&video_path) {
            return Err(MediaError::BadRequest(
                "the video can't also be one of the audio files".to_string(),
            ));
        }
        let probed = audio::probe_sources(state, &sources).await?;
        (sources, Some(probed))
    };
    let video_sound = measure_sound(video.tools.ffmpeg.as_deref(), &video_path, &video.probe).await;
    let sync_method = if alone {
        SyncMethod::None
    } else if video_sound.usable {
        SyncMethod::Waveform
    } else {
        SyncMethod::Lips
    };

    let video_clock = video_clock(&video.probe);
    let recorded_at = audio.as_ref().and_then(|audio| audio.probe.recorded_at);
    let audio_clock = recorded_at.map(sync::local_seconds);
    let (raw_clock_offset, learned_clock_offset) = match (alone, video_clock, audio_clock) {
        (false, Some(video_clock), Some(audio_clock)) => {
            let raw = sync::wrap_day(video_clock - audio_clock);
            let learned = match recorded_at {
                Some(at) => learned_difference(state, video.probe.camera.as_deref(), at)
                    .await
                    .map(|difference| sync::wrap_day(video_clock - difference - audio_clock)),
                None => None,
            };
            (Some(raw), learned)
        }
        _ => (None, None),
    };
    let clock = learned_clock_offset
        .map(|offset| ClockEstimate {
            offset_seconds: offset,
            learned: true,
        })
        .or(raw_clock_offset.map(|offset| ClockEstimate {
            offset_seconds: offset,
            learned: false,
        }));

    let mut warnings = Vec::new();
    if video.damaged_frames > 0 {
        warnings.push(format!(
            "The recording didn't end cleanly: {} frames near the end of the clip can't be read, so the last good frame is held over them. The sound and transcripts are unaffected.",
            video.damaged_frames
        ));
    }
    let mut blocker = None;
    if alone && !video_sound.usable {
        blocker = Some(
            "This video has no usable sound of its own. Drop the audio recording too.".to_string(),
        );
    }
    if let (Some(clock), Some(audio)) = (&clock, &audio)
        && !alone
    {
        let (covered, silent_start, silent_end) = sync::coverage(
            clock.offset_seconds,
            0.0,
            video.probe.duration_seconds,
            audio.probe.duration_seconds,
        );
        let about = if clock.learned { "" } else { "about " };
        if covered == 0.0 {
            warnings.push(
                "The clocks put this video outside the recording, so they're wrong or the files are from different services. The lips are still searched across the whole recording."
                    .to_string(),
            );
        } else if silent_end > 5.0 {
            warnings.push(format!(
                "By the clocks, the recording ends {about}{} before the video does. If the recorder split it into files, add the next one too.",
                minutes(silent_end)
            ));
        }
        if silent_start > 5.0 {
            warnings.push(format!(
                "By the clocks, the recording starts {about}{} after the video does. If the recorder split it into files, add the earlier one too.",
                minutes(silent_start)
            ));
        }
    }
    if sync_method == SyncMethod::Lips
        && video.probe.duration_seconds < SHORT_CLIP_SECONDS
        && learned_clock_offset.is_none()
    {
        warnings.push(
            "This clip is short, so its lips may not be enough to place it in the recording. Sync a longer clip of the same service first, so Metocast learns the clocks."
                .to_string(),
        );
    }

    let mut missing = video.probe.missing.clone();
    let mut downloads = Vec::new();
    if let Some(audio) = &audio {
        for tool in &audio.probe.missing {
            if !missing.contains(tool) {
                missing.push(tool.clone());
            }
        }
        downloads.clone_from(&audio.probe.downloads);
    }
    if sync_method != SyncMethod::None && video.tools.lip_reader.is_none() {
        missing.push("lip-reader".to_string());
    }
    // Only a job with an English transcript needs it; the Media page says so when one is dropped.
    let tools_dir = &state.media.tools_dir;
    if video.tools.whisper.is_some()
        && !(tools::WHISPER_MODEL.path(tools_dir).is_file()
            && tools::WHISPER_VAD.path(tools_dir).is_file())
    {
        downloads.push("whisper".to_string());
    }
    let picture = if video.probe.kind == MediaKind::Braw {
        PictureMode::Grade
    } else {
        PictureMode::Copy
    };
    let probe = ProgramProbe {
        video: video.probe.clone(),
        audio: if alone {
            None
        } else {
            audio.as_ref().map(|audio| audio.probe.clone())
        },
        video_sound,
        sync_method,
        clock,
        picture,
        warnings,
        blocker,
        missing,
        downloads,
    };
    Ok(Facts {
        probe,
        video,
        video_path,
        audio_sources,
        audio,
        video_clock,
        audio_clock,
        raw_clock_offset,
        learned_clock_offset,
    })
}

/// Where a program lands: the video beside its source in `Metocast Renders` (or the configured
/// folder), with its own WAV next to it. Neither may exist yet.
fn program_paths(config: &MediaConfig, video: &FsPath) -> (PathBuf, PathBuf) {
    let parent = video.parent().unwrap_or_else(|| FsPath::new("/"));
    let folder = match config.output_folder.trim() {
        // A render dropped back in stays in its own folder.
        "" if parent
            .file_name()
            .is_some_and(|name| name == RENDERS_FOLDER) =>
        {
            parent.to_path_buf()
        }
        "" => parent.join(RENDERS_FOLDER),
        configured => PathBuf::from(configured),
    };
    let stem = video.file_stem().map_or_else(
        || "program".to_string(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let taken = |path: &FsPath| {
        let portrait = super::portrait_path(path);
        [path, portrait.as_path()]
            .iter()
            .any(|file| file.exists() || partial_path(file).exists())
    };
    (1..)
        .map(|n| {
            let name = match n {
                1 => stem.clone(),
                n => format!("{stem} ({n})"),
            };
            (
                folder.join(format!("{name}.mov")),
                folder.join(format!("{name}.wav")),
            )
        })
        .find(|(video, wav)| !taken(video) && !taken(wav))
        .unwrap_or_else(|| {
            (
                folder.join(format!("{stem}.mov")),
                folder.join(format!("{stem}.wav")),
            )
        })
}

/// Free space where `folder` is (or would be), from `df`.
async fn free_bytes(folder: &FsPath) -> Option<u64> {
    let existing = folder.ancestors().find(|dir| dir.is_dir())?;
    let output = Command::new("/bin/df")
        .arg("-k")
        .arg(existing)
        .stdin(Stdio::null())
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let kilobytes: u64 = text
        .lines()
        .nth(1)?
        .split_whitespace()
        .nth(3)?
        .parse()
        .ok()?;
    Some(kilobytes * 1024)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProgramPayload {
    video_path: String,
    audio_paths: Vec<String>,
    event_id: Option<Uuid>,
    picture: PictureMode,
    settings: RenderSettings,
    audio_settings: AudioSettings,
    output_path: String,
    audio_output_path: String,
    /// The operator's start; the search is skipped when set.
    #[serde(default)]
    start_seconds: Option<f64>,
    #[serde(default)]
    subtitles: Vec<SubtitleSource>,
    #[serde(default)]
    portrait: Option<PortraitOutput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NudgePayload {
    program_job_id: Uuid,
    output_path: String,
    audio_output_path: String,
    #[serde(default)]
    portrait_output_path: Option<String>,
    frames: i32,
}

/// Transcripts dropped on a finished program, placed as its own job would have.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubtitlesPayload {
    program_job_id: Uuid,
    output_path: String,
    audio_output_path: String,
    #[serde(default)]
    portrait_output_path: Option<String>,
    subtitles: Vec<SubtitleSource>,
}

// ── REST handlers ─────────────────────────────────────────────────────────────

pub async fn probe_program(
    State(state): State<AppState>,
    Json(body): Json<ProgramProbeRequest>,
) -> impl IntoResponse {
    match examine(&state, &body.video_path, &body.audio_paths).await {
        Ok(facts) => {
            tracing::info!(
                video = %body.video_path,
                audio = ?body.audio_paths,
                method = ?facts.probe.sync_method,
                sound = ?facts.probe.video_sound.level_dbfs,
                clock = ?facts.probe.clock,
                "media: probed program"
            );
            Json(facts.probe).into_response()
        }
        Err(error) => {
            tracing::warn!(video = %body.video_path, "media: program probe refused: {error}");
            error.into_response()
        }
    }
}

pub async fn start_program(
    State(state): State<AppState>,
    Json(body): Json<ProgramRequest>,
) -> impl IntoResponse {
    let video = body.video_path.clone();
    match queue_program(&state, body).await {
        Ok(queued) => {
            tracing::info!(job = %queued.job_id, output = %queued.output_path, "media: program queued");
            (StatusCode::ACCEPTED, Json(queued)).into_response()
        }
        Err(error) => {
            tracing::warn!(%video, "media: program refused: {error}");
            error.into_response()
        }
    }
}

async fn queue_program(state: &AppState, body: ProgramRequest) -> Result<RenderQueued, MediaError> {
    let config = load_config(&state.pool).await;
    if !config.enabled {
        return Err(MediaError::Unavailable(
            "Media Processing is turned off in its connector settings".to_string(),
        ));
    }
    let facts = examine(state, &body.video_path, &body.audio_paths).await?;
    if let Some(blocker) = facts.probe.blocker {
        return Err(MediaError::BadRequest(blocker));
    }
    let picture = body.picture.unwrap_or(facts.probe.picture);
    let mut missing = facts.probe.missing.clone();
    if picture == PictureMode::Copy {
        // Copying the picture needs neither braw-decode nor the SDK.
        missing.retain(|tool| tool != "braw-decode" && tool != "blackmagic-raw-sdk");
    }
    if !missing.is_empty() {
        return Err(MediaError::Unavailable(format!(
            "this Mac is missing: {}",
            missing.join(", ")
        )));
    }
    if picture == PictureMode::Copy && facts.video.probe.kind == MediaKind::Braw {
        return Err(MediaError::BadRequest(
            "Blackmagic RAW can't be copied as it is; it must be graded".to_string(),
        ));
    }
    let settings = body
        .settings
        .unwrap_or_else(|| facts.video.probe.defaults.clone());
    super::check_settings(&settings)?;
    let audio = facts
        .audio
        .as_ref()
        .ok_or_else(|| MediaError::BadRequest("the video has no sound".to_string()))?;
    let audio_settings = body
        .audio_settings
        .unwrap_or_else(|| audio.probe.defaults.clone());
    audio::check_settings(&audio_settings, audio.probe.channels)?;
    if let Some(start) = body.start_seconds {
        if facts.alone() {
            return Err(MediaError::BadRequest(
                "a start in the recording needs a recording; the video's own sound starts with it"
                    .to_string(),
            ));
        }
        let video_seconds = facts.video.probe.duration_seconds;
        if !start.is_finite() || start <= -video_seconds || start >= audio.probe.duration_seconds {
            return Err(MediaError::BadRequest(format!(
                "the video can't start {start:.2} s into {:.2} s of audio",
                audio.probe.duration_seconds
            )));
        }
    }
    check_subtitles(&body.subtitles)?;
    if let Some(event_id) = body.event_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE id = $1)")
            .bind(event_id)
            .fetch_one(&state.pool)
            .await?;
        if !exists {
            return Err(MediaError::NotFound("no such event".to_string()));
        }
    }
    let video_path = facts.video_path.to_string_lossy().into_owned();
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM job_queue WHERE queue = $1 AND job_type = $2 \
         AND status IN ('pending', 'processing') AND payload->>'videoPath' = $3)",
    )
    .bind(MEDIA_QUEUE)
    .bind(PROGRAM_JOB)
    .bind(&video_path)
    .fetch_one(&state.pool)
    .await?;
    if busy {
        return Err(MediaError::Conflict(
            "this video is already queued or being produced".to_string(),
        ));
    }

    let (output, audio_output) = program_paths(&config, &facts.video_path);
    if let Some(folder) = output.parent() {
        // A graded picture is written twice (the render, then the program); a copy once.
        let picture_bytes = match picture {
            PictureMode::Grade => {
                2.0 * facts.video.probe.duration_seconds * f64::from(settings.bitrate_mbps) * 1e6
                    / 8.0
            }
            PictureMode::Copy => facts.video.probe.file_size as f64 * 1.05,
        };
        let audio_bytes = facts.video.probe.duration_seconds * f64::from(AUDIO_SAMPLE_RATE) * 6.0;
        // A portrait copy is also written twice: its encode, then its merge with the sound.
        let portrait_bytes = if body.portrait.is_some() {
            2.0 * facts.video.probe.duration_seconds * f64::from(PORTRAIT_MAX_BITRATE_MBPS) * 1e6
                / 8.0
        } else {
            0.0
        };
        let needed = (picture_bytes + audio_bytes + portrait_bytes) as u64;
        if let Some(free) = free_bytes(folder).await
            && free < needed
        {
            return Err(MediaError::Unavailable(format!(
                "{} needs about {:.1} GB free, but has {:.1} GB",
                folder.display(),
                needed as f64 / 1e9,
                free as f64 / 1e9
            )));
        }
    }
    let payload = ProgramPayload {
        video_path,
        audio_paths: if facts.alone() {
            Vec::new()
        } else {
            facts
                .audio_sources
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect()
        },
        event_id: body.event_id,
        picture,
        settings,
        audio_settings,
        output_path: output.to_string_lossy().into_owned(),
        audio_output_path: audio_output.to_string_lossy().into_owned(),
        start_seconds: body.start_seconds,
        subtitles: body.subtitles,
        portrait: PortraitOutput::beside(&output, body.portrait),
    };
    let job_id: Uuid = sqlx::query_scalar(
        "INSERT INTO job_queue (queue, job_type, payload, max_attempts) \
         VALUES ($1, $2, $3, 1) RETURNING id",
    )
    .bind(MEDIA_QUEUE)
    .bind(PROGRAM_JOB)
    .bind(serde_json::to_value(&payload).map_err(|e| MediaError::Internal(e.to_string()))?)
    .fetch_one(&state.pool)
    .await?;
    state.media.wake.notify_one();
    Ok(RenderQueued {
        job_id,
        output_path: payload.output_path,
    })
}

/// Each transcript readable, and one per language.
fn check_subtitles(sources: &[SubtitleSource]) -> Result<(), MediaError> {
    for (at, source) in sources.iter().enumerate() {
        subtitles::read(source).map_err(MediaError::BadRequest)?;
        if sources[..at]
            .iter()
            .any(|earlier| earlier.language == source.language)
        {
            return Err(MediaError::BadRequest(format!(
                "one transcript per language: two are {}",
                source.language
            )));
        }
    }
    Ok(())
}

pub async fn nudge_program(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<NudgeRequest>,
) -> impl IntoResponse {
    match queue_nudge(&state, id, body.frames).await {
        Ok(queued) => {
            tracing::info!(program = %id, frames = body.frames, job = %queued.job_id, "media: nudge queued");
            (StatusCode::ACCEPTED, Json(queued)).into_response()
        }
        Err(error) => {
            tracing::warn!(program = %id, "media: nudge refused: {error}");
            error.into_response()
        }
    }
}

async fn queue_nudge(state: &AppState, id: Uuid, frames: i32) -> Result<RenderQueued, MediaError> {
    if frames == 0 || frames.abs() > MAX_NUDGE_FRAMES {
        return Err(MediaError::BadRequest(format!(
            "move the audio by 1 to {MAX_NUDGE_FRAMES} frames either way"
        )));
    }
    let program = finished_program(state, id).await?;
    let payload = NudgePayload {
        program_job_id: id,
        output_path: program.output_path,
        audio_output_path: program.audio_output_path,
        portrait_output_path: program.portrait.map(|portrait| portrait.output_path),
        frames,
    };
    let job_id: Uuid = sqlx::query_scalar(
        "INSERT INTO job_queue (queue, job_type, payload, max_attempts) \
         VALUES ($1, $2, $3, 1) RETURNING id",
    )
    .bind(MEDIA_QUEUE)
    .bind(NUDGE_JOB)
    .bind(serde_json::to_value(&payload).map_err(|e| MediaError::Internal(e.to_string()))?)
    .fetch_one(&state.pool)
    .await?;
    state.media.wake.notify_one();
    Ok(RenderQueued {
        job_id,
        output_path: payload.output_path,
    })
}

/// A finished program's job payload, while its files are where it wrote them and nothing else is
/// adjusting it (a nudge or added transcripts).
async fn finished_program(state: &AppState, id: Uuid) -> Result<ProgramPayload, MediaError> {
    let payload: Option<Value> = sqlx::query_scalar(
        "SELECT payload FROM job_queue WHERE id = $1 AND queue = $2 AND job_type = $3 \
         AND status = 'succeeded'",
    )
    .bind(id)
    .bind(MEDIA_QUEUE)
    .bind(PROGRAM_JOB)
    .fetch_optional(&state.pool)
    .await?;
    let program: ProgramPayload = payload
        .and_then(|payload| serde_json::from_value(payload).ok())
        .ok_or_else(|| MediaError::NotFound("no finished program with that id".to_string()))?;
    if !FsPath::new(&program.output_path).is_file()
        || !FsPath::new(&program.audio_output_path).is_file()
    {
        return Err(MediaError::NotFound(
            "the program's files have been moved or deleted".to_string(),
        ));
    }
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM job_queue WHERE queue = $1 AND job_type = ANY($2) \
         AND status IN ('pending', 'processing') AND payload->>'programJobId' = $3)",
    )
    .bind(MEDIA_QUEUE)
    .bind([NUDGE_JOB, SUBTITLES_JOB])
    .bind(id.to_string())
    .fetch_one(&state.pool)
    .await?;
    if busy {
        return Err(MediaError::Conflict(
            "this program is already being adjusted".to_string(),
        ));
    }
    Ok(program)
}

pub async fn add_program_subtitles(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SubtitlesRequest>,
) -> impl IntoResponse {
    match queue_subtitles(&state, id, body.subtitles).await {
        Ok(queued) => {
            tracing::info!(program = %id, job = %queued.job_id, "media: transcripts queued for a finished program");
            (StatusCode::ACCEPTED, Json(queued)).into_response()
        }
        Err(error) => {
            tracing::warn!(program = %id, "media: transcripts refused: {error}");
            error.into_response()
        }
    }
}

async fn queue_subtitles(
    state: &AppState,
    id: Uuid,
    sources: Vec<SubtitleSource>,
) -> Result<RenderQueued, MediaError> {
    if sources.is_empty() {
        return Err(MediaError::BadRequest(
            "add at least one transcript".to_string(),
        ));
    }
    check_subtitles(&sources)?;
    let program = finished_program(state, id).await?;
    let payload = SubtitlesPayload {
        program_job_id: id,
        output_path: program.output_path,
        audio_output_path: program.audio_output_path,
        portrait_output_path: program.portrait.map(|portrait| portrait.output_path),
        subtitles: sources,
    };
    let job_id: Uuid = sqlx::query_scalar(
        "INSERT INTO job_queue (queue, job_type, payload, max_attempts) \
         VALUES ($1, $2, $3, 1) RETURNING id",
    )
    .bind(MEDIA_QUEUE)
    .bind(SUBTITLES_JOB)
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

/// A recording's voice band at [`ENVELOPE_RATE`] as mono float on stdout, for the sync: all of
/// it, or `range` (start, seconds).
fn envelope_args(file: &FsPath, range: Option<(f64, f64)>) -> Vec<String> {
    let mut args = strings(&["-hide_banner", "-nostdin", "-nostats", "-v", "error"]);
    if let Some((start, seconds)) = range {
        args.extend(["-ss".to_string(), format!("{start:.6}")]);
        args.extend(["-t".to_string(), format!("{seconds:.6}")]);
    }
    args.extend(["-i".to_string(), arg(file)]);
    args.extend(strings(&["-map", "0:a:0", "-ac", "1", "-af"]));
    args.push(format!(
        "highpass=f=200,lowpass=f=3400,aresample={ENVELOPE_RATE}"
    ));
    args.extend(strings(&["-f", "f32le", "-"]));
    args
}

/// The stretch of the prepared audio the picture needs, by sample.
fn span_args(prepared: &FsPath, start: u64, end: u64, output: &FsPath) -> Vec<String> {
    let mut args = strings(&["-hide_banner", "-nostdin", "-nostats", "-v", "error", "-y"]);
    args.extend(["-i".to_string(), arg(prepared), "-af".to_string()]);
    args.push(format!("atrim=start_sample={start}:end_sample={end}"));
    args.extend(strings(&[
        "-c:a",
        "pcm_f32le",
        "-rf64",
        "auto",
        "-f",
        "wav",
    ]));
    args.push(arg(output));
    args
}

/// The program's audio: `cleaned` with the picture's first frame at sample `first` (negative:
/// silence first), its clock corrected by `drift_ppm`, cut or padded to exactly `samples`.
fn program_audio_args(
    cleaned: &FsPath,
    first: i64,
    drift_ppm: Option<f64>,
    samples: u64,
    started_at: Option<DateTime<Utc>>,
    output: &FsPath,
) -> Vec<String> {
    let mut chain = Vec::new();
    match first {
        0 => {}
        late if late > 0 => {
            chain.push(format!("atrim=start_sample={late}"));
            chain.push("asetpts=PTS-STARTPTS".to_string());
        }
        early => chain.push(format!("adelay=delays={}S:all=1", -early)),
    }
    if let Some(ppm) = drift_ppm {
        // asetrate takes whole hertz; at ten times the rate a hertz is 2 ppm.
        let fine = AUDIO_SAMPLE_RATE * 10;
        chain.push(format!("aresample={fine}"));
        chain.push(format!(
            "asetrate={}",
            (f64::from(fine) * (1.0 + ppm / 1e6)).round() as u64
        ));
        chain.push(format!("aresample={AUDIO_SAMPLE_RATE}"));
    }
    chain.push(format!("apad=whole_len={samples}"));
    chain.push(format!("atrim=end_sample={samples}"));
    let mut args = strings(&["-hide_banner", "-nostdin", "-nostats", "-v", "error", "-y"]);
    args.extend(strings(&["-progress", "pipe:1", "-i"]));
    args.push(arg(cleaned));
    args.extend(["-af".to_string(), chain.join(",")]);
    args.extend(strings(&["-c:a", "pcm_s24le", "-rf64", "auto"]));
    args.extend(audio::bext_metadata(started_at));
    args.extend(strings(&["-f", "wav"]));
    args.push(arg(output));
    args
}

/// The program's audio moved by `shift` samples (positive: later), the same length.
fn shift_args(wav: &FsPath, shift: i64, samples: u64, output: &FsPath) -> Vec<String> {
    let mut chain = Vec::new();
    if shift > 0 {
        chain.push(format!("adelay=delays={shift}S:all=1"));
    } else {
        chain.push(format!("atrim=start_sample={}", -shift));
        chain.push("asetpts=PTS-STARTPTS".to_string());
    }
    chain.push(format!("apad=whole_len={samples}"));
    chain.push(format!("atrim=end_sample={samples}"));
    let mut args = strings(&["-hide_banner", "-nostdin", "-nostats", "-v", "error", "-y"]);
    args.extend(["-i".to_string(), arg(wav)]);
    args.extend(["-af".to_string(), chain.join(",")]);
    args.extend(strings(&[
        "-c:a",
        "pcm_s24le",
        "-rf64",
        "auto",
        "-write_bext",
        "1",
        "-f",
        "wav",
    ]));
    args.push(arg(output));
    args
}

/// The picture copied as it is, with the program audio as AAC for YouTube.
fn mux_args(
    picture: &FsPath,
    wav: &FsPath,
    timecode: Option<&str>,
    output: &FsPath,
) -> Vec<String> {
    let mut args = strings(&["-hide_banner", "-nostdin", "-nostats", "-v", "error", "-y"]);
    args.extend(strings(&["-progress", "pipe:1", "-i"]));
    args.push(arg(picture));
    args.push("-i".to_string());
    args.push(arg(wav));
    args.extend(strings(&[
        "-map", "0:v:0", "-map", "1:a:0", "-c:v", "copy", "-c:a", "aac", "-b:a", "320k", "-ar",
        "48000",
    ]));
    if let Some(timecode) = timecode {
        args.extend(["-timecode".to_string(), timecode.to_string()]);
    }
    args.extend(strings(&["-movflags", "+faststart", "-f", "mov"]));
    args.push(arg(output));
    args
}

/// Which file of the joined recording `offset` seconds falls in, and how far into it.
fn start_file(facts: &Facts, offset: f64) -> (Option<String>, Option<f64>) {
    let Some(audio) = &facts.audio else {
        return (None, None);
    };
    let mut before = 0.0;
    for (index, source) in audio.probe.sources.iter().enumerate() {
        let last = index + 1 == audio.probe.sources.len();
        if offset < before + source.duration_seconds || last {
            return (
                Some(source.file_name.clone()),
                Some(((offset - before) * 1000.0).round() / 1000.0),
            );
        }
        before += source.duration_seconds;
    }
    (None, None)
}

// ── The job ───────────────────────────────────────────────────────────────────

struct Reporter<'a> {
    state: &'a AppState,
    job_id: Uuid,
    output_path: String,
}

impl Reporter<'_> {
    async fn send(
        &self,
        state: RenderState,
        step: Option<ProgramStep>,
        fraction: Option<f64>,
        error: Option<String>,
        report: Option<ProgramReport>,
    ) {
        let progress = ProgramProgress {
            job_id: self.job_id,
            state,
            step,
            fraction,
            output_path: self.output_path.clone(),
            error,
            report,
        };
        crate::server::websocket::broadcast_media_program_progress(
            &self.state.ws_clients,
            &progress,
        )
        .await;
    }

    async fn step(&self, step: ProgramStep, fraction: Option<f64>) {
        self.send(RenderState::Running, Some(step), fraction, None, None)
            .await;
    }

    async fn ended(&self, result: &Result<ProgramReport, String>) {
        match result {
            Ok(report) => {
                self.send(
                    RenderState::Succeeded,
                    None,
                    Some(1.0),
                    None,
                    Some(report.clone()),
                )
                .await;
            }
            Err(error) if error == CANCELLED => {
                self.send(RenderState::Cancelled, None, None, None, None)
                    .await;
            }
            Err(error) => {
                self.send(RenderState::Failed, None, None, Some(error.clone()), None)
                    .await;
            }
        }
    }
}

struct Step<'a> {
    reporter: &'a Reporter<'a>,
    step: ProgramStep,
}

/// Logs how long each step took, so a slow service can be explained afterwards.
struct StepClock {
    job_id: Uuid,
    since: Instant,
}

impl StepClock {
    fn new(job_id: Uuid) -> Self {
        Self {
            job_id,
            since: Instant::now(),
        }
    }

    fn done(&mut self, step: &str) {
        tracing::info!(job = %self.job_id, step, seconds = self.since.elapsed().as_secs(), "media: program step done");
        self.since = Instant::now();
    }
}

impl StepProgress for Step<'_> {
    async fn fraction(&self, fraction: f64) {
        self.reporter.step(self.step, Some(fraction)).await;
    }
}

/// A running job's cancel signal, heartbeat and entry in the cancel route's table.
struct Running {
    cancel: watch::Receiver<bool>,
    tasks: [tokio::task::JoinHandle<()>; 2],
    key: Uuid,
}

impl Running {
    /// `key` is what the cancel route is called with: the program's job, also for a nudge.
    async fn start(state: &AppState, job_id: Uuid, key: Uuid) -> Self {
        let (stop, stopped) = oneshot::channel();
        state.media.running.lock().await.insert(key, stop);
        let (cancel_tx, cancel) = watch::channel(false);
        let relay = tokio::spawn(async move {
            if stopped.await.is_ok() {
                cancel_tx.send_replace(true);
            }
        });
        let pool = state.pool.clone();
        let heartbeat = tokio::spawn(async move {
            let mut beat = tokio::time::interval(HEARTBEAT);
            loop {
                beat.tick().await;
                if let Err(e) = queue::heartbeat(&pool, job_id).await {
                    tracing::warn!("media: heartbeat for {job_id} failed: {e}");
                }
            }
        });
        Self {
            cancel,
            tasks: [relay, heartbeat],
            key,
        }
    }

    async fn finish(self, state: &AppState) {
        for task in &self.tasks {
            task.abort();
        }
        state.media.running.lock().await.remove(&self.key);
    }
}

/// Runs one `produce_program` job; the worker records the outcome.
pub(super) async fn process(state: &AppState, job: &Job) -> Result<(), String> {
    let payload: ProgramPayload = serde_json::from_value(job.payload.clone())
        .map_err(|e| format!("unreadable job payload: {e}"))?;
    let output = PathBuf::from(&payload.output_path);
    let audio_output = PathBuf::from(&payload.audio_output_path);
    let folder = output.parent().map(FsPath::to_path_buf).unwrap_or_default();
    let work = folder.join(format!(".metocast-program-{}", job.id));
    let running = Running::start(state, job.id, job.id).await;
    let reporter = Reporter {
        state,
        job_id: job.id,
        output_path: payload.output_path.clone(),
    };
    tracing::info!(job = %job.id, video = %payload.video_path, audio = ?payload.audio_paths, output = %payload.output_path, "media: producing program");
    let result = produce(
        state,
        job.id,
        &payload,
        &reporter,
        &work,
        running.cancel.clone(),
    )
    .await;
    running.finish(state).await;
    metocast_core::best_effort::remove_dir_all_if_present(&work);
    if result.is_err() {
        metocast_core::best_effort::remove_file_if_present(partial_path(&output));
        metocast_core::best_effort::remove_file_if_present(partial_path(&audio_output));
        if let Some(portrait) = &payload.portrait {
            metocast_core::best_effort::remove_file_if_present(partial_path(FsPath::new(
                &portrait.output_path,
            )));
        }
    }
    match &result {
        Ok(report) => {
            tracing::info!(job = %job.id, sync = ?report.sync.method, offset = report.sync.offset_seconds, sigma = ?report.sync.peak_sigma, "media: program produced")
        }
        Err(error) => tracing::warn!(job = %job.id, "media: program failed: {error}"),
    }
    reporter.ended(&result).await;
    result.map(|_| ())
}

async fn produce(
    state: &AppState,
    job_id: Uuid,
    payload: &ProgramPayload,
    reporter: &Reporter<'_>,
    work: &FsPath,
    cancel: watch::Receiver<bool>,
) -> Result<ProgramReport, String> {
    let facts = examine(state, &payload.video_path, &payload.audio_paths)
        .await
        .map_err(MediaError::message)?;
    if let Some(blocker) = &facts.probe.blocker {
        return Err(blocker.clone());
    }
    let audio_probed = facts.audio.as_ref().ok_or("the video has no sound")?;
    let tools = &facts.video.tools;
    let ffmpeg = tools.ffmpeg.clone().ok_or("ffmpeg is not installed")?;
    let ffprobe = tools.ffprobe.clone().ok_or("ffprobe is not installed")?;
    std::fs::create_dir_all(work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    let mut clock = StepClock::new(job_id);

    // Download
    let jive = match audio_probed.tools.jive_vocals.clone() {
        Some(path) => path,
        None => {
            reporter.step(ProgramStep::Download, None).await;
            let path = tools::ensure(&tools::JIVE_VOCALS, &state.media.tools_dir).await?;
            clock.done("download");
            path
        }
    };

    // 1. Picture
    let picture = match payload.picture {
        PictureMode::Copy => facts.video_path.clone(),
        PictureMode::Grade => {
            let picture = work.join("picture.mov");
            render_picture(
                state,
                job_id,
                &facts,
                PictureTarget {
                    settings: &payload.settings,
                    portrait_shift: None,
                    output: &picture,
                },
                reporter,
                cancel.clone(),
            )
            .await?;
            clock.done("picture");
            picture
        }
    };

    // 2. Prepare
    reporter.step(ProgramStep::Prepare, Some(0.0)).await;
    let prepared = work.join("prepared.wav");
    let inputs: Vec<(&FsPath, u32)> = facts
        .audio_sources
        .iter()
        .zip(&audio_probed.probe.sources)
        .map(|(path, source)| (path.as_path(), source.audio.channels))
        .collect();
    audio::run_ffmpeg(
        &ffmpeg,
        &audio::prepare_args(&inputs, &payload.audio_settings.channels, &prepared),
        Some((
            audio_probed.probe.duration_seconds,
            &Step {
                reporter,
                step: ProgramStep::Prepare,
            },
        )),
        cancel.clone(),
    )
    .await?;
    let audio_samples = audio::sample_count(&ffprobe, &prepared).await?;
    let audio_seconds = audio_samples as f64 / f64::from(AUDIO_SAMPLE_RATE);
    clock.done("prepare");

    // 3. Sync
    reporter.step(ProgramStep::Sync, Some(0.0)).await;
    let sync = find_sync(
        &ffmpeg,
        &facts,
        &picture,
        &prepared,
        payload.start_seconds,
        reporter,
        cancel.clone(),
    )
    .await?;
    clock.done("sync");
    let video_seconds = facts.video.probe.duration_seconds;
    let frame = facts.frame_seconds();
    let drift = (sync.drift_ppm.abs() <= MAX_DRIFT_PPM
        && (sync.drift_ppm * 1e-6 * video_seconds).abs() >= frame / 2.0)
        .then_some(sync.drift_ppm);
    let speed = 1.0 + drift.unwrap_or(0.0) / 1e6;

    // 4. Clean
    reporter.step(ProgramStep::Clean, None).await;
    let span_start = (sync.offset_seconds - SPAN_MARGIN_SECONDS).max(0.0);
    let span_end =
        (sync.offset_seconds + video_seconds * speed + SPAN_MARGIN_SECONDS).min(audio_seconds);
    if span_end - span_start < 4.0 {
        return Err(format!(
            "the audio and the video barely overlap (the video starts {:.1} s into {:.1} s of audio); check that they're from the same service",
            sync.offset_seconds, audio_seconds
        ));
    }
    let rate = f64::from(AUDIO_SAMPLE_RATE);
    let start_sample = (span_start * rate).round() as u64;
    let end_sample = (span_end * rate).round() as u64;
    let span_samples = end_sample - start_sample;
    let clean_dir = work.join("clean");
    std::fs::create_dir_all(&clean_dir)
        .map_err(|e| format!("cannot create {}: {e}", clean_dir.display()))?;
    // jive-vocals names its outputs after this file, and `jive_outputs` looks for that name.
    let span = clean_dir.join("prepared.wav");
    let clean_step = Step {
        reporter,
        step: ProgramStep::Clean,
    };
    audio::run_ffmpeg(
        &ffmpeg,
        &span_args(&prepared, start_sample, end_sample, &span),
        None::<(f64, &Step)>,
        cancel.clone(),
    )
    .await?;
    audio::run_jive_vocals(&jive, &span, &clean_dir, cancel.clone()).await?;
    let (processed, report_json, _) = audio::jive_outputs(&clean_dir)?;
    let jive_report: Value = serde_json::from_slice(
        &std::fs::read(&report_json)
            .map_err(|e| format!("cannot read jive-vocals' report: {e}"))?,
    )
    .map_err(|e| format!("unreadable jive-vocals report: {e}"))?;
    let (input_lufs, cleaned_lufs) = audio::parse_jive_report(&jive_report)?;
    let span_seconds = span_samples as f64 / rate;
    let delay = audio::measure_delay(
        &ffmpeg,
        &span,
        &processed,
        span_seconds,
        &clean_step,
        cancel.clone(),
    )
    .await?;
    let cleaned = work.join("clean.wav");
    let gain_db = f64::from(payload.audio_settings.target_lufs) - cleaned_lufs;
    let stderr = audio::run_ffmpeg(
        &ffmpeg,
        &audio::finish_args(&processed, delay.lag, span_samples, gain_db, None, &cleaned),
        Some((span_seconds, &clean_step)),
        cancel.clone(),
    )
    .await?;
    let (output_lufs, true_peak) =
        audio::parse_ebur128(&stderr).ok_or("ffmpeg did not report the final loudness")?;
    clock.done("clean");

    // 5. Merge
    let merge_step = Step {
        reporter,
        step: ProgramStep::Merge,
    };
    reporter.step(ProgramStep::Merge, Some(0.0)).await;
    let video_samples = (facts.video.probe.frame_count as f64 * frame * rate).round() as u64;
    let first = (sync.offset_seconds * rate).round() as i64 - start_sample as i64;
    let output = PathBuf::from(&payload.output_path);
    let audio_output = PathBuf::from(&payload.audio_output_path);
    let (output_partial, audio_partial) = (partial_path(&output), partial_path(&audio_output));
    let started_at = facts
        .audio_recorded_at()
        .map(|at| at + ChronoDuration::milliseconds((sync.offset_seconds * 1000.0).round() as i64));
    audio::run_ffmpeg(
        &ffmpeg,
        &program_audio_args(
            &cleaned,
            first,
            drift,
            video_samples,
            started_at,
            &audio_partial,
        ),
        Some((video_seconds, &merge_step)),
        cancel.clone(),
    )
    .await?;
    audio::run_ffmpeg(
        &ffmpeg,
        &mux_args(
            &picture,
            &audio_partial,
            facts.video.probe.start_timecode.as_deref(),
            &output_partial,
        ),
        Some((video_seconds, &merge_step)),
        cancel.clone(),
    )
    .await?;
    clock.done("merge");

    // 6. Portrait: the same sound under a second picture, before either file lands.
    if let Some(portrait) = &payload.portrait {
        let picture = work.join("portrait.mov");
        let mut settings = payload.settings.clone();
        if payload.picture == PictureMode::Copy {
            // A kept picture has its look already.
            settings.lut_mode = LutMode::None;
        }
        render_picture(
            state,
            job_id,
            &facts,
            PictureTarget {
                settings: &settings,
                portrait_shift: Some(portrait.shift_x),
                output: &picture,
            },
            reporter,
            cancel.clone(),
        )
        .await?;
        audio::run_ffmpeg(
            &ffmpeg,
            &mux_args(
                &picture,
                &audio_partial,
                facts.video.probe.start_timecode.as_deref(),
                &partial_path(FsPath::new(&portrait.output_path)),
            ),
            None::<(f64, &Step)>,
            cancel.clone(),
        )
        .await?;
        clock.done("portrait");
    }

    std::fs::rename(&audio_partial, &audio_output)
        .map_err(|e| format!("cannot move the program audio into place: {e}"))?;
    std::fs::rename(&output_partial, &output)
        .map_err(|e| format!("cannot move the program into place: {e}"))?;
    if let Some(portrait) = &payload.portrait {
        let portrait = FsPath::new(&portrait.output_path);
        std::fs::rename(partial_path(portrait), portrait)
            .map_err(|e| format!("cannot move the portrait copy into place: {e}"))?;
    }

    let (coverage, silent_start, silent_end) = sync::coverage(
        sync.offset_seconds,
        drift.unwrap_or(0.0),
        video_seconds,
        audio_seconds,
    );
    // The recording's time 0 falls this far into the picture (drift, at most a few ppm, aside).
    let recording_shift = -sync.offset_seconds;
    let heard = hear_program(
        state,
        reporter,
        tools,
        &payload.subtitles,
        &audio_output,
        work,
        cancel.clone(),
    )
    .await?;
    if heard.is_some() {
        clock.done("captions");
    }
    let report = ProgramReport {
        sync,
        audio: AudioReport {
            input_lufs,
            output_lufs,
            output_true_peak_dbtp: true_peak,
            removed_delay_samples: delay.lag,
            alignment_confidence: delay.score.clamp(0.0, 1.0),
        },
        audio_seconds,
        coverage,
        silent_start_seconds: silent_start,
        silent_end_seconds: silent_end,
        damaged_frames: facts.video.damaged_frames,
        audio_output_path: payload.audio_output_path.clone(),
        subtitles: place_subtitles(
            &ffmpeg,
            &payload.subtitles,
            &output,
            &audio_output,
            recording_shift,
            heard.as_deref(),
            cancel,
        )
        .await,
    };
    register(state, payload, &facts, &report, &output).await;
    Ok(report)
}

/// The English words Whisper hears in the program, when an English transcript was dropped to be
/// placed on them. The models download on first use. Placing falls back to the voice match when
/// this can't run (no `whisper-cli` on this host, a failed download), so only cancelling fails.
async fn hear_program(
    state: &AppState,
    reporter: &Reporter<'_>,
    tools: &Tools,
    sources: &[SubtitleSource],
    audio_output: &FsPath,
    work: &FsPath,
    cancel: watch::Receiver<bool>,
) -> Result<Option<Vec<whisper::Heard>>, String> {
    let wanted = sources
        .iter()
        .any(|source| source.language == whisper::PLACEABLE_LANGUAGE);
    let Some(whisper_cli) = tools.whisper.as_ref().filter(|_| wanted) else {
        if wanted {
            tracing::warn!(
                "media: no whisper-cli here; the transcript is placed by its voice match"
            );
        }
        return Ok(None);
    };
    let tools_dir = &state.media.tools_dir;
    let models = async {
        if !tools::WHISPER_MODEL.path(tools_dir).is_file()
            || !tools::WHISPER_VAD.path(tools_dir).is_file()
        {
            reporter.step(ProgramStep::Download, None).await;
        }
        Ok::<_, String>((
            tools::ensure(&tools::WHISPER_MODEL, tools_dir).await?,
            tools::ensure(&tools::WHISPER_VAD, tools_dir).await?,
        ))
    };
    let (model, vad) = match models.await {
        Ok(models) => models,
        Err(e) => {
            tracing::warn!("media: Whisper's model is unavailable, placing by voice instead: {e}");
            return Ok(None);
        }
    };
    reporter.step(ProgramStep::Captions, Some(0.0)).await;
    let step = Step {
        reporter,
        step: ProgramStep::Captions,
    };
    match whisper::hear(
        whisper_cli,
        &model,
        &vad,
        audio_output,
        &work.join("heard"),
        &step,
        cancel,
    )
    .await
    {
        Ok(heard) => Ok(Some(heard)),
        Err(e) if e == CANCELLED => Err(e),
        Err(e) => {
            tracing::warn!("media: Whisper failed, placing the transcript by voice instead: {e}");
            Ok(None)
        }
    }
}

/// Each transcript placed on the program and written beside it: an English one on the speech
/// Whisper heard (`heard`), as whole sentences, and otherwise moved as a whole onto the voice.
/// The program is already in place, so a transcript that fails is logged and left out rather
/// than failing the job.
async fn place_subtitles(
    ffmpeg: &FsPath,
    sources: &[SubtitleSource],
    output: &FsPath,
    audio_output: &FsPath,
    recording_shift: f64,
    heard: Option<&[whisper::Heard]>,
    cancel: watch::Receiver<bool>,
) -> Vec<SubtitleReport> {
    if sources.is_empty() {
        return Vec::new();
    }
    let voice = match read_envelope(ffmpeg, audio_output, None, cancel).await {
        Ok(voice) => voice,
        Err(e) => {
            tracing::warn!("media: subtitles skipped, the program audio is unreadable: {e}");
            return Vec::new();
        }
    };
    sources
        .iter()
        .filter_map(|source| {
            let placed = subtitles::read(source).and_then(|cues| {
                let on_speech = heard
                    .filter(|_| source.language == whisper::PLACEABLE_LANGUAGE)
                    .and_then(|heard| whisper::place_on_speech(&cues, heard, voice.duration()));
                let (cues, shift_seconds, confident, matched) = match on_speech {
                    Some(placed) => (
                        placed.cues,
                        placed.offset_seconds,
                        true,
                        Some(placed.matched),
                    ),
                    None => {
                        let (cues, shift, confident) =
                            subtitles::place(&cues, &voice, recording_shift);
                        (cues, shift, confident, None)
                    }
                };
                let srt = subtitles::write_srt(output, &source.language, &cues)?;
                Ok(SubtitleReport {
                    language: source.language.clone(),
                    cues: u32::try_from(cues.len()).unwrap_or(u32::MAX),
                    shift_seconds,
                    confident,
                    srt_path: srt.to_string_lossy().into_owned(),
                    on_speech: matched.is_some(),
                    matched: matched.unwrap_or(0),
                })
            });
            placed
                .inspect_err(|e| tracing::warn!("media: subtitle {} skipped: {e}", source.path))
                .ok()
        })
        .collect()
}

/// Which picture [`render_picture`] makes, and where.
struct PictureTarget<'a> {
    settings: &'a RenderSettings,
    portrait_shift: Option<i32>,
    output: &'a FsPath,
}

/// A render of the picture alone, its frame count reported as the step's progress: the landscape
/// picture, or the 9:16 copy when `portrait_shift` is set.
async fn render_picture(
    state: &AppState,
    job_id: Uuid,
    facts: &Facts,
    target: PictureTarget<'_>,
    reporter: &Reporter<'_>,
    cancel: watch::Receiver<bool>,
) -> Result<(), String> {
    let PictureTarget {
        settings,
        portrait_shift,
        output,
    } = target;
    let step = if portrait_shift.is_some() {
        ProgramStep::Portrait
    } else {
        ProgramStep::Picture
    };
    reporter.step(step, Some(0.0)).await;
    let probe = &facts.video.probe;
    let source = command::Source {
        path: &facts.video_path,
        kind: probe.kind,
        width: probe.width,
        height: probe.height,
        fps_num: probe.fps_num,
        fps_den: probe.fps_den,
        frame_count: probe.frame_count,
        damaged_frames: facts.video.damaged_frames,
        start_timecode: probe.start_timecode.as_deref(),
        // The program's audio comes later; the render carries none.
        audio_channels: None,
        pixel_format: &facts.video.pixel_format,
    };
    let sdk = facts.video.tools.braw_sdk.as_deref();
    let commands = match portrait_shift {
        Some(shift) => command::portrait_commands(
            &source,
            settings,
            portrait_crop(probe.active_area, shift),
            sdk,
            output,
        ),
        None => command::render_commands(
            &source,
            settings,
            output_crop(probe.active_area, settings.aspect, settings.crop_shift_y),
            sdk,
            output,
        ),
    };
    let (progress, mut updates) = mpsc::unbounded_channel::<worker::Progress>();
    let (stop, stopped) = oneshot::channel();
    let mut watching = cancel;
    let relay = tokio::spawn(async move {
        audio::cancelled(&mut watching).await;
        stop.send(()).log_gone();
    });
    let total = probe.frame_count.max(1);
    let forward = async {
        while let Some(update) = updates.recv().await {
            reporter
                .step(
                    step,
                    Some((update.frame as f64 / total as f64).clamp(0.0, 1.0)),
                )
                .await;
        }
    };
    let (result, ()) = tokio::join!(
        worker::execute(
            &facts.video.tools,
            &commands,
            probe.frame_count,
            Some((&state.pool, job_id)),
            progress,
            stopped,
        ),
        forward
    );
    relay.abort();
    result
}

/// Reads a recording's (or a stretch of a video's) voice-band energy.
async fn read_envelope(
    ffmpeg: &FsPath,
    file: &FsPath,
    range: Option<(f64, f64)>,
    mut cancel: watch::Receiver<bool>,
) -> Result<Envelope, String> {
    let mut child = Command::new(ffmpeg)
        .args(envelope_args(file, range))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not start ffmpeg: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("ffmpeg gave no output")?;
    let mut stderr = child.stderr.take().ok_or("ffmpeg gave no errors stream")?;
    let errors = tokio::spawn(async move {
        let mut text = String::new();
        stderr.read_to_string(&mut text).await.log_failed();
        text
    });
    let mut envelope = Envelope::new();
    let mut buffer = vec![0u8; 1 << 16];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let read = tokio::select! {
            read = stdout.read(&mut buffer) => read.map_err(|e| format!("reading ffmpeg: {e}"))?,
            () = audio::cancelled(&mut cancel) => {
                child.start_kill().log_gone();
                return Err(CANCELLED.to_string());
            }
        };
        if read == 0 {
            break;
        }
        carry.extend_from_slice(&buffer[..read]);
        let whole = carry.len() / 4 * 4;
        envelope.push(&audio::samples_of(&carry[..whole]));
        carry.drain(..whole);
    }
    let status = child.wait().await.map_err(|e| format!("ffmpeg: {e}"))?;
    let errors = errors.await.unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "ffmpeg could not read the sound of {}: {}",
            file.display(),
            errors.trim().lines().last().unwrap_or_default()
        ));
    }
    Ok(envelope)
}

/// `lip-reader`'s lines for one stretch of the picture.
async fn read_lips(
    lip_reader: &FsPath,
    picture: &FsPath,
    start: f64,
    seconds: f64,
    mut cancel: watch::Receiver<bool>,
) -> Result<String, String> {
    let child = Command::new(lip_reader)
        .args([
            "--start".to_string(),
            format!("{start:.6}"),
            "--duration".to_string(),
            format!("{seconds:.6}"),
            arg(picture),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not start lip-reader: {e}"))?;
    let output = tokio::select! {
        output = child.wait_with_output() => output.map_err(|e| format!("lip-reader: {e}"))?,
        () = audio::cancelled(&mut cancel) => return Err(CANCELLED.to_string()),
    };
    if !output.status.success() {
        let errors = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "lip-reader failed: {}",
            errors
                .lines()
                .rev()
                .find(|line| line.starts_with("lip-reader:"))
                .unwrap_or_else(|| errors.trim())
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Step 3: where the picture's first frame falls in the prepared audio.
async fn find_sync(
    ffmpeg: &FsPath,
    facts: &Facts,
    picture: &FsPath,
    prepared: &FsPath,
    start_seconds: Option<f64>,
    reporter: &Reporter<'_>,
    cancel: watch::Receiver<bool>,
) -> Result<SyncReport, String> {
    let report = |method, offset: f64, found: Option<&sync::Found>| {
        let (start_file, start_file_seconds) = start_file(facts, offset);
        let clock_difference_seconds = match (method, facts.video_clock, facts.audio_clock) {
            (
                SyncMethod::Lips | SyncMethod::Waveform | SyncMethod::Manual,
                Some(video),
                Some(audio),
            ) => Some((sync::wrap_day(video - audio - offset) * 1000.0).round() / 1000.0),
            _ => None,
        };
        SyncReport {
            method,
            offset_seconds: (offset * 1e4).round() / 1e4,
            drift_ppm: found.map_or(0.0, |found| found.drift_ppm),
            // A clock learned from another clip of the same recording is usually within a
            // frame or two; raw clocks can be minutes out.
            confidence: found.map_or(
                match method {
                    SyncMethod::None | SyncMethod::Manual => 1.0,
                    _ if facts.learned_clock_offset.is_some() => 0.5,
                    _ => 0.1,
                },
                |found| found.confidence,
            ),
            confident: matches!(method, SyncMethod::None | SyncMethod::Manual)
                || found.is_some_and(|found| found.confident),
            peak_sigma: found.map(|found| found.peak_sigma),
            windows: found.map(|found| found.windows.clone()).unwrap_or_default(),
            clock_difference_seconds,
            clock_offset_seconds: facts.raw_clock_offset,
            manual_frames: 0,
            start_file,
            start_file_seconds,
        }
    };
    if facts.alone() {
        return Ok(report(SyncMethod::None, 0.0, None));
    }
    if let Some(start) = start_seconds {
        return Ok(report(SyncMethod::Manual, start, None));
    }

    let started = Instant::now();
    let audio = read_envelope(ffmpeg, prepared, None, cancel.clone()).await?;
    tracing::info!(
        seconds = started.elapsed().as_secs_f64(),
        audio_seconds = audio.duration(),
        "media: read the recording's voice band"
    );
    let frame = facts.frame_seconds();
    let video_seconds = facts.video.probe.duration_seconds;
    let prior = facts.learned_clock_offset;
    let frames_in = |seconds: f64| (seconds / frame).floor() as usize;

    if facts.probe.video_sound.usable {
        let plan = sync::plan_windows(video_seconds, SOUND_WINDOWS, SOUND_WINDOW_SECONDS, 0);
        let mut windows = Vec::with_capacity(plan.len());
        for (index, (start, seconds)) in plan.iter().enumerate() {
            let sound = read_envelope(
                ffmpeg,
                &facts.video_path,
                Some((*start, *seconds)),
                cancel.clone(),
            )
            .await?;
            windows.push(sync::sound_window(
                &sound,
                *start,
                frame,
                frames_in(*seconds) - 1,
            ));
            reporter
                .step(
                    ProgramStep::Sync,
                    Some((index + 1) as f64 / plan.len() as f64 / 2.0),
                )
                .await;
        }
        let envelope = audio.clone();
        let found = tokio::task::spawn_blocking(move || {
            sync::find_offset(&windows, &envelope, frame, prior)
        })
        .await
        .map_err(|e| format!("sync failed: {e}"))?;
        if let Some(found) = found {
            tracing::info!(?found, "media: synced by the video's own sound");
            return Ok(report(
                SyncMethod::Waveform,
                found.offset_seconds,
                Some(&found),
            ));
        }
        tracing::info!("media: the video's own sound didn't match; trying the lips");
    }

    if let Some(lip_reader) = facts.video.tools.lip_reader.clone() {
        let mut windows = Vec::new();
        let mut found = None;
        for round in 0..2 {
            let plan = sync::plan_windows(video_seconds, LIP_WINDOWS, LIP_WINDOW_SECONDS, round);
            if plan.is_empty() {
                break;
            }
            for (index, (start, seconds)) in plan.iter().enumerate() {
                let started = Instant::now();
                let lines =
                    read_lips(&lip_reader, picture, *start, *seconds, cancel.clone()).await?;
                windows.push(sync::lips_window(
                    &lines,
                    *start,
                    frame,
                    frames_in(*seconds),
                ));
                tracing::info!(
                    start,
                    seconds = started.elapsed().as_secs_f64(),
                    faces = windows.last().map_or(0, |window| window.candidates.len()),
                    "media: read the lips"
                );
                let done = (index + 1) as f64 / plan.len() as f64;
                reporter
                    .step(ProgramStep::Sync, Some((f64::from(round) + done) / 2.0))
                    .await;
            }
            let attempt = windows.clone();
            let envelope = audio.clone();
            found = tokio::task::spawn_blocking(move || {
                sync::find_offset(&attempt, &envelope, frame, prior)
            })
            .await
            .map_err(|e| format!("sync failed: {e}"))?;
            if found.as_ref().is_some_and(|found| found.confident) {
                break;
            }
        }
        if let Some(found) = found {
            tracing::info!(?found, "media: synced by the lips");
            return Ok(report(SyncMethod::Lips, found.offset_seconds, Some(&found)));
        }
    }

    // The clocks only count when they put the video inside the recording; raw clocks can be
    // hours out (an export's creation time, a recorder that was never set).
    let clock = facts.learned_clock_offset.or(facts.raw_clock_offset);
    let inside =
        |offset: f64| sync::coverage(offset, 0.0, video_seconds, audio.duration()).0 >= 0.5;
    match clock {
        Some(offset) if inside(offset) => {
            tracing::warn!(offset, "media: nothing matched; placing the audio by the clocks");
            Ok(report(SyncMethod::Clock, offset, None))
        }
        Some(offset) => Err(format!(
            "couldn't find where the video starts in the audio: the lips don't match the voice clearly enough, and the clocks put the video {} into a {} recording, outside it. Check that the files are from the same service, or enter where the video starts in the recording.",
            minutes(offset),
            minutes(audio.duration())
        )),
        None => Err(
            "couldn't find where the video starts in the audio: the lips don't match the voice clearly enough, and neither file has a usable clock. Check that the files are from the same service, or enter where the video starts in the recording."
                .to_string(),
        ),
    }
}

/// Files the program under its event, or as untracked, with the timeline and sync facts the
/// subtitles and the next clip of the same camera use.
async fn register(
    state: &AppState,
    payload: &ProgramPayload,
    facts: &Facts,
    report: &ProgramReport,
    output: &FsPath,
) {
    let probe = &facts.video.probe;
    let metadata = json!({
        "source": "program",
        "timeline": {
            "startTimecode": probe.start_timecode,
            "fps": format!("{}/{}", probe.fps_num, probe.fps_den),
            "tcSource": if probe.kind == MediaKind::Braw { "camera_tmcd" } else { "container" },
            "sourcePath": payload.video_path,
            "camera": probe.camera,
            "clipName": probe.clip_name,
            "recordedAt": probe.recorded_at,
            "hostRecordStartedAt": probe.host_record_started_at,
            "audioSources": payload.audio_paths,
            "audioRecordedAt": facts.audio_recorded_at(),
        },
        "picture": payload.picture,
        "render": (payload.picture == PictureMode::Grade).then_some(&payload.settings),
        "audio": payload.audio_settings,
        "report": report,
        "processedWith": format!("jive-vocals {}; lip-reader (Apple Vision)", tools::JIVE_VOCALS.version),
    });
    let recording = file_recording(
        state,
        payload.event_id,
        output,
        probe.duration_seconds,
        "video",
        "program",
        &metadata,
    )
    .await;
    // ponytail: unassigned programs keep only the .srt files; carry the tracks over on assign if that's common.
    if let Some(recording_id) = recording {
        for subtitle in &report.subtitles {
            crate::transcription::captions_job::store_track(
                &state.pool,
                recording_id,
                &subtitle.language,
                &subtitle.srt_path,
            )
            .await;
        }
    }
    if let Some(portrait) = &payload.portrait {
        // Its own recording, so it can go to its own platforms. The sync's report stays with the
        // landscape file: the portrait's sound simply follows it.
        let metadata = json!({
            "source": PORTRAIT_SOURCE,
            "timeline": metadata["timeline"],
            "frame": portrait_crop(probe.active_area, portrait.shift_x),
            "landscapeFile": payload.output_path,
        });
        let recording = file_recording(
            state,
            payload.event_id,
            FsPath::new(&portrait.output_path),
            probe.duration_seconds,
            "video",
            PORTRAIT_SOURCE,
            &metadata,
        )
        .await;
        if let Some(recording_id) = recording {
            for subtitle in &report.subtitles {
                crate::transcription::captions_job::store_track(
                    &state.pool,
                    recording_id,
                    &subtitle.language,
                    &subtitle.srt_path,
                )
                .await;
            }
        }
    }
}

// ── Adjusting a finished program ──────────────────────────────────────────────

/// A finished program's recording: its metadata, whether it is filed under an event (else it is
/// untracked), and the report the program job wrote into it.
async fn stored_program(
    state: &AppState,
    output_path: &str,
) -> Result<(Value, bool, ProgramReport), String> {
    let stored: Option<(Value, bool)> = sqlx::query_as(
        "SELECT metadata, true FROM recordings WHERE file_path = $1 \
         UNION ALL SELECT metadata, false FROM untracked_recordings WHERE file_path = $1 \
         LIMIT 1",
    )
    .bind(output_path)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;
    let (metadata, in_event) = stored.ok_or("the program's recording is no longer listed")?;
    let report: ProgramReport = serde_json::from_value(metadata["report"].clone())
        .map_err(|e| format!("the program's report is unreadable: {e}"))?;
    Ok((metadata, in_event, report))
}

/// Runs one `subtitle_program` job: transcripts dropped on a finished program, placed on it.
pub(super) async fn caption(state: &AppState, job: &Job) -> Result<(), String> {
    let payload: SubtitlesPayload = serde_json::from_value(job.payload.clone())
        .map_err(|e| format!("unreadable job payload: {e}"))?;
    let running = Running::start(state, job.id, payload.program_job_id).await;
    let reporter = Reporter {
        state,
        job_id: payload.program_job_id,
        output_path: payload.output_path.clone(),
    };
    let output = PathBuf::from(&payload.output_path);
    let work = output
        .parent()
        .unwrap_or(FsPath::new("."))
        .join(format!(".metocast-subtitles-{}", job.id));
    let result = caption_program(state, &payload, &reporter, &work, running.cancel.clone()).await;
    running.finish(state).await;
    metocast_core::best_effort::remove_dir_all_if_present(&work);
    reporter.ended(&result).await;
    result.map(|_| ())
}

async fn caption_program(
    state: &AppState,
    payload: &SubtitlesPayload,
    reporter: &Reporter<'_>,
    work: &FsPath,
    cancel: watch::Receiver<bool>,
) -> Result<ProgramReport, String> {
    reporter.step(ProgramStep::Captions, None).await;
    let config = load_config(&state.pool).await;
    let tools = Tools::find(&config, &state.media.tools_dir);
    let ffmpeg = tools.ffmpeg.clone().ok_or("ffmpeg is not installed")?;
    let (_, in_event, mut report) = stored_program(state, &payload.output_path).await?;
    std::fs::create_dir_all(work).map_err(|e| format!("cannot create {}: {e}", work.display()))?;
    let output = PathBuf::from(&payload.output_path);
    let audio_output = PathBuf::from(&payload.audio_output_path);
    let heard = hear_program(
        state,
        reporter,
        &tools,
        &payload.subtitles,
        &audio_output,
        work,
        cancel.clone(),
    )
    .await?;
    let placed = place_subtitles(
        &ffmpeg,
        &payload.subtitles,
        &output,
        &audio_output,
        -report.sync.offset_seconds,
        heard.as_deref(),
        cancel,
    )
    .await;
    if placed.is_empty() {
        return Err("the transcript couldn't be placed on this program".to_string());
    }
    // A transcript in a language the program already has replaces that one.
    report
        .subtitles
        .retain(|old| !placed.iter().any(|new| new.language == old.language));
    report.subtitles.extend(placed.iter().cloned());

    let table = if in_event {
        "recordings"
    } else {
        "untracked_recordings"
    };
    let report_json = serde_json::to_value(&report).map_err(|e| e.to_string())?;
    let recording_id: Option<Uuid> = sqlx::query_scalar(&format!(
        "UPDATE {table} SET metadata = jsonb_set(metadata, '{{report}}', $2) \
         WHERE file_path = $1 RETURNING id"
    ))
    .bind(&payload.output_path)
    .bind(&report_json)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;
    // ponytail: unassigned programs keep only the .srt files, as a program job leaves them.
    if let Some(recording_id) = recording_id.filter(|_| in_event) {
        for subtitle in &placed {
            crate::transcription::captions_job::store_track(
                &state.pool,
                recording_id,
                &subtitle.language,
                &subtitle.srt_path,
            )
            .await;
        }
        // An upload that already finished gets the new tracks now.
        crate::transcription::captions_job::after_upload(&state.pool, recording_id).await;
        let portrait: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM recordings WHERE file_path = $1")
                .bind(&payload.portrait_output_path)
                .fetch_optional(&state.pool)
                .await
                .map_err(|e| format!("database error: {e}"))?;
        if let Some(portrait) = portrait {
            for subtitle in &placed {
                crate::transcription::captions_job::store_track(
                    &state.pool,
                    portrait,
                    &subtitle.language,
                    &subtitle.srt_path,
                )
                .await;
            }
            crate::transcription::captions_job::after_upload(&state.pool, portrait).await;
        }
    }
    tracing::info!(program = %payload.program_job_id, transcripts = placed.len(), "media: transcripts added to a finished program");
    Ok(report)
}

// ── Nudging ───────────────────────────────────────────────────────────────────

/// Runs one `nudge_program` job: the program's audio moved by whole frames.
pub(super) async fn nudge(state: &AppState, job: &Job) -> Result<(), String> {
    let payload: NudgePayload = serde_json::from_value(job.payload.clone())
        .map_err(|e| format!("unreadable job payload: {e}"))?;
    let running = Running::start(state, job.id, payload.program_job_id).await;
    let reporter = Reporter {
        state,
        job_id: payload.program_job_id,
        output_path: payload.output_path.clone(),
    };
    let result = shift_program(state, &payload, &reporter, running.cancel.clone()).await;
    running.finish(state).await;
    if result.is_err() {
        metocast_core::best_effort::remove_file_if_present(partial_path(FsPath::new(
            &payload.output_path,
        )));
        metocast_core::best_effort::remove_file_if_present(partial_path(FsPath::new(
            &payload.audio_output_path,
        )));
        if let Some(portrait) = &payload.portrait_output_path {
            metocast_core::best_effort::remove_file_if_present(partial_path(FsPath::new(portrait)));
        }
    }
    reporter.ended(&result).await;
    result.map(|_| ())
}

async fn shift_program(
    state: &AppState,
    payload: &NudgePayload,
    reporter: &Reporter<'_>,
    cancel: watch::Receiver<bool>,
) -> Result<ProgramReport, String> {
    reporter.step(ProgramStep::Merge, Some(0.0)).await;
    let config = load_config(&state.pool).await;
    let tools = super::Tools::find(&config, &state.media.tools_dir);
    let ffmpeg = tools.ffmpeg.ok_or("ffmpeg is not installed")?;
    let ffprobe = tools.ffprobe.ok_or("ffprobe is not installed")?;
    let (metadata, in_event, mut report) = stored_program(state, &payload.output_path).await?;
    let frame = metadata["timeline"]["fps"]
        .as_str()
        .and_then(|fps| fps.split_once('/'))
        .and_then(|(num, den)| Some(den.parse::<f64>().ok()? / num.parse::<f64>().ok()?))
        .ok_or("the program's frame rate is unknown")?;
    let timecode = metadata["timeline"]["startTimecode"]
        .as_str()
        .map(str::to_string);

    let output = PathBuf::from(&payload.output_path);
    let wav = PathBuf::from(&payload.audio_output_path);
    let (output_partial, wav_partial) = (partial_path(&output), partial_path(&wav));
    let samples = audio::sample_count(&ffprobe, &wav).await?;
    let seconds = samples as f64 / f64::from(AUDIO_SAMPLE_RATE);
    let shift = (f64::from(payload.frames) * frame * f64::from(AUDIO_SAMPLE_RATE)).round() as i64;
    let step = Step {
        reporter,
        step: ProgramStep::Merge,
    };
    audio::run_ffmpeg(
        &ffmpeg,
        &shift_args(&wav, shift, samples, &wav_partial),
        None::<(f64, &Step)>,
        cancel.clone(),
    )
    .await?;
    audio::run_ffmpeg(
        &ffmpeg,
        &mux_args(&output, &wav_partial, timecode.as_deref(), &output_partial),
        Some((seconds, &step)),
        cancel.clone(),
    )
    .await?;
    // The portrait copy shares the sound, so it moves too; one deleted by hand is left out.
    let portrait = payload
        .portrait_output_path
        .as_deref()
        .map(FsPath::new)
        .filter(|portrait| portrait.is_file());
    if let Some(portrait) = portrait {
        audio::run_ffmpeg(
            &ffmpeg,
            &mux_args(
                portrait,
                &wav_partial,
                timecode.as_deref(),
                &partial_path(portrait),
            ),
            Some((seconds, &step)),
            cancel,
        )
        .await?;
    }
    std::fs::rename(&wav_partial, &wav)
        .map_err(|e| format!("cannot move the program audio into place: {e}"))?;
    std::fs::rename(&output_partial, &output)
        .map_err(|e| format!("cannot move the program into place: {e}"))?;
    if let Some(portrait) = portrait {
        std::fs::rename(partial_path(portrait), portrait)
            .map_err(|e| format!("cannot move the portrait copy into place: {e}"))?;
        let size = i64::try_from(std::fs::metadata(portrait).map_or(0, |meta| meta.len()))
            .unwrap_or(i64::MAX);
        sqlx::query("UPDATE recordings SET file_size = $2 WHERE file_path = $1")
            .bind(portrait.to_string_lossy().as_ref())
            .bind(size)
            .execute(&state.pool)
            .await
            .map_err(|e| format!("database error: {e}"))?;
        sqlx::query("UPDATE untracked_recordings SET file_size = $2 WHERE file_path = $1")
            .bind(portrait.to_string_lossy().as_ref())
            .bind(size)
            .execute(&state.pool)
            .await
            .map_err(|e| format!("database error: {e}"))?;
    }

    // Playing the audio later means taking it from earlier in the recording.
    let moved = f64::from(payload.frames) * frame;
    let sync = &mut report.sync;
    sync.offset_seconds = ((sync.offset_seconds - moved) * 1e4).round() / 1e4;
    sync.manual_frames += payload.frames;
    sync.start_file_seconds = sync
        .start_file_seconds
        .map(|at| ((at - moved) * 1000.0).round() / 1000.0);
    sync.clock_difference_seconds = sync
        .clock_difference_seconds
        .map(|difference| difference + moved);
    let (coverage, silent_start, silent_end) =
        sync::coverage(sync.offset_seconds, 0.0, seconds, report.audio_seconds);
    report.coverage = coverage;
    report.silent_start_seconds = silent_start;
    report.silent_end_seconds = silent_end;

    let report_json = serde_json::to_value(&report).map_err(|e| e.to_string())?;
    let size =
        i64::try_from(std::fs::metadata(&output).map_or(0, |meta| meta.len())).unwrap_or(i64::MAX);
    let table = if in_event {
        "recordings"
    } else {
        "untracked_recordings"
    };
    sqlx::query(&format!(
        "UPDATE {table} SET metadata = jsonb_set(metadata, '{{report}}', $2), file_size = $3 \
         WHERE file_path = $1"
    ))
    .bind(&payload.output_path)
    .bind(&report_json)
    .bind(size)
    .execute(&state.pool)
    .await
    .map_err(|e| format!("database error: {e}"))?;
    tracing::info!(program = %payload.program_job_id, frames = payload.frames, offset = report.sync.offset_seconds, "media: program nudged");
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_audio_starts_at_the_first_frame_and_lasts_exactly_the_picture() {
        let args = program_audio_args(
            FsPath::new("/w/clean.wav"),
            96_000,
            None,
            82_824_000,
            None,
            FsPath::new("/r/x.wav.partial"),
        );
        let chain = &args[args.iter().position(|a| a == "-af").unwrap() + 1];
        assert_eq!(
            chain,
            "atrim=start_sample=96000,asetpts=PTS-STARTPTS,apad=whole_len=82824000,atrim=end_sample=82824000"
        );
        assert!(args.windows(2).any(|w| w == ["-c:a", "pcm_s24le"]));
        assert_eq!(args.last().unwrap(), "/r/x.wav.partial");

        // The audio starts 1.5 s after the picture: silence first.
        let early = program_audio_args(
            FsPath::new("/c.wav"),
            -72_000,
            None,
            480,
            None,
            FsPath::new("/o"),
        );
        assert!(
            early
                .iter()
                .any(|a| a.starts_with("adelay=delays=72000S:all=1,"))
        );
    }

    #[test]
    fn drift_is_corrected_at_two_ppm_resolution() {
        let args = program_audio_args(
            FsPath::new("/c.wav"),
            0,
            Some(25.0),
            4_800,
            None,
            FsPath::new("/o"),
        );
        let chain = &args[args.iter().position(|a| a == "-af").unwrap() + 1];
        assert_eq!(
            chain,
            "aresample=480000,asetrate=480012,aresample=48000,apad=whole_len=4800,atrim=end_sample=4800"
        );
    }

    #[test]
    fn a_nudge_keeps_the_length() {
        let later = shift_args(FsPath::new("/a.wav"), 2_000, 9_600, FsPath::new("/b"));
        let chain = &later[later.iter().position(|a| a == "-af").unwrap() + 1];
        assert_eq!(
            chain,
            "adelay=delays=2000S:all=1,apad=whole_len=9600,atrim=end_sample=9600"
        );
        let earlier = shift_args(FsPath::new("/a.wav"), -2_000, 9_600, FsPath::new("/b"));
        let chain = &earlier[earlier.iter().position(|a| a == "-af").unwrap() + 1];
        assert_eq!(
            chain,
            "atrim=start_sample=2000,asetpts=PTS-STARTPTS,apad=whole_len=9600,atrim=end_sample=9600"
        );
    }

    #[test]
    fn the_merge_copies_the_picture_and_keeps_the_timecode() {
        let args = mux_args(
            FsPath::new("/w/picture.mov"),
            FsPath::new("/r/x.wav.partial"),
            Some("10:26:20:15"),
            FsPath::new("/r/x.mov.partial"),
        );
        for pair in [
            ["-map", "0:v:0"],
            ["-map", "1:a:0"],
            ["-c:v", "copy"],
            ["-c:a", "aac"],
            ["-timecode", "10:26:20:15"],
            ["-f", "mov"],
        ] {
            assert!(args.windows(2).any(|w| w == pair), "{pair:?} in {args:?}");
        }
    }

    #[test]
    fn the_voice_band_is_read_at_the_envelope_rate() {
        let args = envelope_args(FsPath::new("/v.braw"), Some((600.0, 90.0)));
        assert!(args.windows(2).any(|w| w == ["-ss", "600.000000"]));
        assert!(args.windows(2).any(|w| w == ["-map", "0:a:0"]));
        assert!(
            args.iter()
                .any(|a| a == "highpass=f=200,lowpass=f=3400,aresample=8000")
        );
    }

    #[test]
    fn positions_read_as_clock_times() {
        assert_eq!(minutes(346.75), "5:47");
        assert_eq!(minutes(15_143.1), "4:12:23");
        assert_eq!(minutes(-12.0), "−0:12");
    }

    #[test]
    fn volumedetect_and_placeholder_timecodes_are_read() {
        let stderr = "[Parsed_volumedetect_0 @ 0x1] n_samples: 960000\n\
                      [Parsed_volumedetect_0 @ 0x1] mean_volume: -97.0 dB\n\
                      [Parsed_volumedetect_0 @ 0x1] max_volume: -91.2 dB\n";
        assert_eq!(parse_mean_volume(stderr), Some(-97.0));
        assert_eq!(parse_mean_volume("nothing"), None);
        assert!(PLACEHOLDER_TIMECODES.contains(&"01:00:00:00"));
    }

    /// Finds a real video in a real recording by its lips, the way the job does. It needs the
    /// files and the tools, so it runs only on request:
    ///
    /// ```text
    /// METOCAST_LIP_READER=lip-reader/build/lip-reader \
    /// METOCAST_TEST_SYNC_VIDEO=…/proxy05.mov \
    /// METOCAST_TEST_SYNC_AUDIO=~/Music/Metocast\ Audio/00015_Wireless_PRO.wav \
    /// METOCAST_TEST_SYNC_OFFSET=946.75 \
    /// cargo test -p metocast-server real_lips -- --ignored --nocapture
    /// ```
    ///
    /// `METOCAST_TEST_SYNC_PRIOR` adds a learned clock estimate, for a clip too short alone.
    #[tokio::test]
    #[ignore = "needs lip-reader, ffmpeg, a video and its recording"]
    async fn real_lips_find_the_video_in_its_recording() {
        tracing_subscriber::fmt()
            .with_test_writer()
            .try_init()
            .log_gone();
        let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let video = PathBuf::from(env("METOCAST_TEST_SYNC_VIDEO"));
        let recording = PathBuf::from(env("METOCAST_TEST_SYNC_AUDIO"));
        let expected: f64 = env("METOCAST_TEST_SYNC_OFFSET").parse().unwrap();
        let prior = std::env::var("METOCAST_TEST_SYNC_PRIOR")
            .ok()
            .map(|prior| prior.parse::<f64>().unwrap());
        let tools = super::super::Tools::find(&MediaConfig::default(), FsPath::new("/nonexistent"));
        let (ffmpeg, ffprobe) = (tools.ffmpeg.unwrap(), tools.ffprobe.unwrap());
        let lip_reader = tools.lip_reader.expect("METOCAST_LIP_READER");
        let facts = super::super::probe::ffprobe(&ffprobe, &video)
            .await
            .unwrap();
        let frame = f64::from(facts.fps_den) / f64::from(facts.fps_num);
        let (_keep, cancel) = watch::channel(false);

        let started = std::time::Instant::now();
        let audio = read_envelope(&ffmpeg, &recording, None, cancel.clone())
            .await
            .unwrap();
        let mut windows = Vec::new();
        for (start, seconds) in
            sync::plan_windows(facts.duration_seconds, LIP_WINDOWS, LIP_WINDOW_SECONDS, 0)
        {
            let lines = read_lips(&lip_reader, &video, start, seconds, cancel.clone())
                .await
                .unwrap();
            windows.push(sync::lips_window(
                &lines,
                start,
                frame,
                (seconds / frame) as usize,
            ));
        }
        let elapsed = started.elapsed().as_secs_f64();
        match (sync::find_offset(&windows, &audio, frame, prior), prior) {
            (Some(found), _) => {
                println!("{found:#?}\nin {elapsed:.1} s");
                assert!(
                    (found.offset_seconds - expected).abs() < 0.045,
                    "found {} instead of {expected}",
                    found.offset_seconds
                );
            }
            // The job places such a clip by the learned clock and asks for a check.
            (None, Some(prior)) => {
                println!(
                    "no clear lip match in {elapsed:.1} s; the job would use the clock: {prior}"
                );
            }
            (None, None) => panic!("no match"),
        }
    }

    #[test]
    fn programs_land_beside_the_video_and_never_overwrite() {
        let dir = std::env::temp_dir().join(format!("metocast-program-{}", Uuid::new_v4()));
        let renders = dir.join(RENDERS_FOLDER);
        std::fs::create_dir_all(&renders).unwrap();
        let config = MediaConfig::default();
        let clip = dir.join("Untitled CAM 1 05.braw");
        assert_eq!(
            program_paths(&config, &clip),
            (
                renders.join("Untitled CAM 1 05.mov"),
                renders.join("Untitled CAM 1 05.wav")
            )
        );
        // A render dropped back in: the program goes next to it, under a new name.
        let render = renders.join("Untitled CAM 1 04.mov");
        std::fs::write(&render, b"").unwrap();
        assert_eq!(
            program_paths(&config, &render),
            (
                renders.join("Untitled CAM 1 04 (2).mov"),
                renders.join("Untitled CAM 1 04 (2).wav")
            )
        );
        // A WAV of the same name blocks the name too.
        std::fs::write(renders.join("Untitled CAM 1 05.wav"), b"").unwrap();
        assert_eq!(
            program_paths(&config, &clip).0,
            renders.join("Untitled CAM 1 05 (2).mov")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
