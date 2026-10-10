//! English captions for a finished video, generated from the file itself (never mapped from the
//! live transcript, since the upload is an edit). The file is played to the realtime model at
//! 1×, cue starts are snapped to speech onsets, and the WebVTT goes to YouTube.
//!
//! Jobs run on their own `captions` queue so a 40-minute job never holds up `platform_sync`.

use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use axum::{
    Json,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use metocast_core::connectors::ConnectorConfig as _;
use metocast_core::transcription::{OPENAI_CONFIG_KEY, OpenAiConfig};
use metocast_transcription::captions::{CaptionOptions, build_cues, parse_cues, render_vtt};
use metocast_transcription::decode::AudioReader;
use metocast_transcription::framer::{FRAME_MS, Frame, Framer};
use metocast_transcription::realtime::{self, RealtimeConfig, RealtimeEvent};
use metocast_transcription::retime::{RetimeOptions, retime};
use metocast_transcription::segmenter::{Segment, SegmentEvent, Segmenter, SegmenterConfig};
use serde_json::json;
use sqlx::PgPool;
use tokio::sync::{Notify, mpsc};
use uuid::Uuid;

use super::fatal_reason;
use crate::connectors::youtube;
use crate::database::settings;
use crate::queue::{self, Job};
use crate::server::AppState;

pub const CAPTIONS_QUEUE: &str = "captions";
const JOB_TYPE: &str = "youtube.captions";
const TICK: Duration = Duration::from_secs(30);
const HEARTBEAT: Duration = Duration::from_secs(60);
const VISIBILITY_TIMEOUT_SECS: i64 = 300;
const PROGRESS_EVERY: Duration = Duration::from_secs(5);
/// Upstream sessions expire at 60 minutes; longer files use several connections.
// ponytail: the cut lands wherever 50 minutes falls, even mid-sentence; cut at a pause if it shows.
const CONNECTION_AUDIO_MS: u64 = 50 * 60 * 1_000;

static WAKE: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Queues captions for `recording_id`; a pending job for it is reused.
///
/// # Errors
/// Returns the database error.
pub async fn enqueue(pool: &PgPool, recording_id: Uuid) -> anyhow::Result<()> {
    queue::enqueue(
        pool,
        CAPTIONS_QUEUE,
        JOB_TYPE,
        Some(&recording_id.to_string()),
        json!({ "recording_id": recording_id }),
    )
    .await?;
    WAKE.notify_one();
    Ok(())
}

/// Keeps a transcript the media page placed on `recording_id` (the `.srt` written beside the
/// program) as its caption track in `language`, for the uploads to send.
pub async fn store_track(pool: &PgPool, recording_id: Uuid, language: &str, srt_path: &str) {
    let cues = std::fs::read_to_string(srt_path)
        .map_err(|e| e.to_string())
        .and_then(|text| parse_cues(&text));
    let stored = match cues {
        Ok(cues) => sqlx::query(
            "INSERT INTO caption_tracks (recording_id, language, vtt) VALUES ($1, $2, $3) \
             ON CONFLICT (recording_id, language) DO UPDATE SET vtt = EXCLUDED.vtt, updated_at = now()",
        )
        .bind(recording_id)
        .bind(language)
        .bind(render_vtt(&cues))
        .execute(pool)
        .await
        .map(drop)
        .map_err(|e| e.to_string()),
        Err(e) => Err(e),
    };
    if let Err(e) = stored {
        tracing::warn!(recording = %recording_id, "captions: cannot keep {srt_path}: {e}");
    }
}

/// Called when a YouTube upload finishes: sends the stored tracks, then has English made when
/// none was given and that is switched on.
pub async fn after_upload(pool: &PgPool, recording_id: Uuid) {
    let has_english = upload_tracks(pool, recording_id).await;
    let config: OpenAiConfig = settings::get_json(pool, OPENAI_CONFIG_KEY).await;
    if config.is_configured()
        && config.auto_youtube_captions
        && !has_english
        && let Err(e) = enqueue(pool, recording_id).await
    {
        tracing::error!("captions: cannot queue recording {recording_id}: {e}");
    }
}

/// Uploads every stored track to the recording's YouTube video. Returns whether one is English.
async fn upload_tracks(pool: &PgPool, recording_id: Uuid) -> bool {
    let tracks: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT language, vtt, youtube_caption_id FROM caption_tracks WHERE recording_id = $1",
    )
    .bind(recording_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let has_english = tracks.iter().any(|(language, ..)| language == "en");
    let video_id: Option<String> = sqlx::query_scalar(
        "SELECT video_id FROM recording_uploads WHERE recording_id = $1 \
         AND platform = 'youtube' AND state = 'completed' AND video_id <> ''",
    )
    .bind(recording_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (Some(video_id), false) = (video_id, tracks.is_empty()) else {
        return has_english;
    };
    let Some(token) = youtube::load_tokens(pool).await else {
        tracing::warn!(recording = %recording_id, "captions: YouTube is not connected");
        return has_english;
    };
    for (language, vtt, existing) in tracks {
        match youtube::upload_caption(
            &token.access_token,
            &video_id,
            existing.as_deref(),
            &vtt,
            &language,
        )
        .await
        {
            Ok(caption_id) => {
                let _ = sqlx::query(
                    "UPDATE caption_tracks SET youtube_caption_id = $1, video_id = $2, updated_at = now() \
                     WHERE recording_id = $3 AND language = $4",
                )
                .bind(&caption_id)
                .bind(&video_id)
                .bind(recording_id)
                .bind(&language)
                .execute(pool)
                .await;
                tracing::info!(recording = %recording_id, %language, "captions: sent to YouTube");
            }
            Err(e) => {
                tracing::warn!(recording = %recording_id, %language, "captions: YouTube refused: {e}")
            }
        }
    }
    has_english
}

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        loop {
            if let Err(e) =
                queue::reclaim_stuck(&state.pool, CAPTIONS_QUEUE, VISIBILITY_TIMEOUT_SECS).await
            {
                tracing::error!("captions: reclaim_stuck failed: {e}");
            }
            while let Ok(Some(job)) =
                queue::claim_batch(&state.pool, CAPTIONS_QUEUE, &state.server_id, 1)
                    .await
                    .map(|mut jobs| jobs.pop())
            {
                let heartbeat = {
                    let pool = state.pool.clone();
                    tokio::spawn(async move {
                        loop {
                            tokio::time::sleep(HEARTBEAT).await;
                            let _ = queue::heartbeat(&pool, job.id).await;
                        }
                    })
                };
                let result = run(&state, &job).await;
                heartbeat.abort();
                let recorded = match result {
                    Ok(()) => queue::complete(&state.pool, job.id).await,
                    Err(error) => {
                        tracing::error!("captions: job {} failed: {error}", job.id);
                        queue::fail(&state.pool, &job, &error).await
                    }
                };
                if let Err(e) = recorded {
                    tracing::error!("captions: could not record the outcome of {}: {e}", job.id);
                }
            }
            tokio::select! {
                () = WAKE.notified() => {}
                _ = ticker.tick() => {}
            }
        }
    });
}

async fn broadcast(
    state: &AppState,
    job: &Job,
    recording_id: Uuid,
    status: &str,
    progress: f64,
    error: Option<&str>,
) {
    let message = json!({
        "type": "captions.job.state",
        "jobId": job.id,
        "recordingId": recording_id,
        "state": status,
        "progress": progress,
        "error": error,
    })
    .to_string();
    for tx in state.ws_clients.read().await.values() {
        let _ = tx.send(axum::extract::ws::Message::Text(message.clone().into()));
    }
}

async fn run(state: &AppState, job: &Job) -> Result<(), String> {
    let recording_id: Uuid = serde_json::from_value(job.payload["recording_id"].clone())
        .map_err(|e| format!("unreadable job payload: {e}"))?;
    let result = caption(state, job, recording_id).await;
    match &result {
        Ok(()) => broadcast(state, job, recording_id, "succeeded", 1.0, None).await,
        Err(error) => broadcast(state, job, recording_id, "failed", 0.0, Some(error)).await,
    }
    result
}

async fn caption(state: &AppState, job: &Job, recording_id: Uuid) -> Result<(), String> {
    let row: Option<(String, f64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT r.file_path, r.duration_seconds, \
                (SELECT u.video_id FROM recording_uploads u WHERE u.recording_id = r.id \
                   AND u.platform = 'youtube' AND u.state = 'completed' AND u.video_id <> ''), \
                (SELECT c.youtube_caption_id FROM caption_tracks c \
                   WHERE c.recording_id = r.id AND c.language = 'en') \
         FROM recordings r WHERE r.id = $1",
    )
    .bind(recording_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| e.to_string())?;
    let Some((path, duration_seconds, video_id, caption_id)) = row else {
        return Ok(()); // deleted while queued
    };
    let config: OpenAiConfig = settings::get_json(&state.pool, OPENAI_CONFIG_KEY).await;
    if !config.is_configured() {
        return Err("OpenAI is not configured".to_string());
    }
    broadcast(state, job, recording_id, "running", 0.0, None).await;
    tracing::info!(recording = %recording_id, %path, "captions: transcribing");

    let total_ms = (duration_seconds * 1_000.0) as u64;
    let (segments, onsets) = transcribe(&config, PathBuf::from(&path), |audio_ms| {
        let progress = if total_ms == 0 {
            0.0
        } else {
            (audio_ms as f64 / total_ms as f64).min(0.99)
        };
        broadcast(state, job, recording_id, "running", progress, None)
    })
    .await?;
    let options = RetimeOptions {
        lag_ms: config.lag_ms,
        ..RetimeOptions::default()
    };
    let cues = build_cues(
        &retime(&segments, &onsets, &options),
        &CaptionOptions::default(),
    );
    let vtt = render_vtt(&cues);
    sqlx::query(
        "INSERT INTO caption_tracks (recording_id, language, video_id, vtt, offset_ms) \
         VALUES ($1, 'en', $2, $3, $4) \
         ON CONFLICT (recording_id, language) DO UPDATE SET video_id = EXCLUDED.video_id, \
             vtt = EXCLUDED.vtt, offset_ms = EXCLUDED.offset_ms, updated_at = now()",
    )
    .bind(recording_id)
    .bind(&video_id)
    .bind(&vtt)
    .bind(i64::try_from(config.lag_ms).unwrap_or(0))
    .execute(&state.pool)
    .await
    .map_err(|e| e.to_string())?;

    let Some(video_id) = video_id else {
        tracing::info!(recording = %recording_id, "captions: stored; no YouTube video to attach them to");
        return Ok(());
    };
    let token = youtube::load_tokens(&state.pool)
        .await
        .ok_or_else(|| "YouTube is not connected".to_string())?;
    let caption_id = youtube::upload_caption(
        &token.access_token,
        &video_id,
        caption_id.as_deref(),
        &vtt,
        "en",
    )
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE caption_tracks SET youtube_caption_id = $1, updated_at = now() \
         WHERE recording_id = $2 AND language = 'en'",
    )
    .bind(&caption_id)
    .bind(recording_id)
    .execute(&state.pool)
    .await
    .map_err(|e| e.to_string())?;
    tracing::info!(recording = %recording_id, %video_id, "captions: uploaded {} cues", cues.len());
    Ok(())
}

/// Plays `path` to the realtime model at 1× and returns its segments and speech onsets (ms).
async fn transcribe<F>(
    config: &OpenAiConfig,
    path: PathBuf,
    progress: impl Fn(u64) -> F,
) -> Result<(Vec<Segment>, Vec<u64>), String>
where
    F: Future<Output = ()>,
{
    let (frames_tx, mut frames) = mpsc::channel::<Result<Frame, String>>(64);
    std::thread::Builder::new()
        .name("captions-decode".into())
        .spawn(move || {
            let result = (|| {
                let mut reader = AudioReader::open(&path, 0.0, None).map_err(|e| e.to_string())?;
                let mut framer = Framer::new(reader.sample_rate());
                let started = Instant::now();
                let mut sent: u32 = 0;
                while let Some(chunk) = reader.next_chunk().map_err(|e| e.to_string())? {
                    for frame in framer.push(&chunk) {
                        // Real-time pace: the model translates live speech.
                        let due = started + Duration::from_millis(FRAME_MS) * sent;
                        std::thread::sleep(due.saturating_duration_since(Instant::now()));
                        sent += 1;
                        if frames_tx.blocking_send(Ok(frame)).is_err() {
                            return Ok(());
                        }
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                let _ = frames_tx.blocking_send(Err(error));
            }
        })
        .map_err(|e| e.to_string())?;

    let mut segmenter = Segmenter::new(SegmenterConfig::default());
    let mut segments = Vec::new();
    let mut onsets = Vec::new();
    let mut audio_ms = 0;
    let mut last_progress = Instant::now();
    let mut file_done = false;
    let mut realtime = RealtimeConfig::new(config.api_key.clone());
    realtime.source_transcription_model = None;
    while !file_done {
        let (audio_tx, audio_rx) = mpsc::channel::<Vec<i16>>(64);
        let (events_tx, mut events) = mpsc::unbounded_channel();
        let client = tokio::spawn(realtime::run(realtime.clone(), audio_rx, events_tx));
        let offset = audio_ms;
        let mut audio_tx = Some(audio_tx);
        let mut close_reason = String::new();
        let mut server_error: Option<String> = None;
        let mut commit = |events: Vec<SegmentEvent>| {
            segments.extend(events.into_iter().filter_map(|event| match event {
                SegmentEvent::Committed(segment) => Some(segment),
                SegmentEvent::Partial(_) => None,
            }));
        };
        loop {
            tokio::select! {
                frame = frames.recv(), if audio_tx.is_some() => match frame {
                    Some(Ok(frame)) => {
                        onsets.extend(frame.onsets_ms);
                        audio_ms += FRAME_MS;
                        if let Some(tx) = &audio_tx
                            && tx.send(frame.pcm).await.is_err()
                        {
                            audio_tx = None;
                        }
                        if audio_ms - offset >= CONNECTION_AUDIO_MS {
                            audio_tx = None;
                        }
                        if last_progress.elapsed() >= PROGRESS_EVERY {
                            last_progress = Instant::now();
                            progress(audio_ms).await;
                        }
                    }
                    Some(Err(error)) => return Err(error),
                    None => {
                        file_done = true;
                        audio_tx = None;
                    }
                },
                event = events.recv() => {
                    let Some(event) = event else { break };
                    match event {
                        RealtimeEvent::AudioSent { at_ms } => commit(segmenter.tick(offset + at_ms)),
                        RealtimeEvent::TargetDelta { at_ms, text } => commit(segmenter.push(offset + at_ms, &text)),
                        RealtimeEvent::Server { kind, raw, .. } if kind == "error" => {
                            server_error = raw["error"]["code"].as_str().map(str::to_string);
                        }
                        RealtimeEvent::Closed { reason, .. } => close_reason = reason,
                        _ => {}
                    }
                }
            }
        }
        let error = match client.await {
            Ok(Ok(())) => close_reason,
            Ok(Err(e)) => e.to_string(),
            Err(e) => e.to_string(),
        };
        if let Some(reason) = fatal_reason(&error, server_error.as_deref()) {
            return Err(match reason.as_str() {
                "noCredit" => "OpenAI has no credit left".to_string(),
                _ => "OpenAI rejected the API key".to_string(),
            });
        }
        if audio_tx.is_some() {
            return Err(format!("the translation connection dropped: {error}"));
        }
    }
    segments.extend(
        segmenter
            .finish(audio_ms)
            .into_iter()
            .filter_map(|event| match event {
                SegmentEvent::Committed(segment) => Some(segment),
                SegmentEvent::Partial(_) => None,
            }),
    );
    Ok((segments, onsets))
}

/// `POST /api/recordings/{id}/captions`: queue English captions for a recording.
pub async fn enqueue_route(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let exists: Result<bool, sqlx::Error> =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM recordings WHERE id = $1)")
            .bind(id)
            .fetch_one(&state.pool)
            .await;
    match exists {
        Ok(true) => match enqueue(&state.pool, id).await {
            Ok(()) => (StatusCode::ACCEPTED, Json(json!({ "queued": true }))).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Recording not found" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// `GET /api/recordings/{id}/captions.vtt`: the last generated English track.
pub async fn vtt_route(State(state): State<AppState>, Path(id): Path<Uuid>) -> Response {
    let vtt: Result<Option<String>, sqlx::Error> = sqlx::query_scalar(
        "SELECT vtt FROM caption_tracks WHERE recording_id = $1 AND language = 'en'",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await;
    match vtt {
        Ok(Some(vtt)) => ([(header::CONTENT_TYPE, "text/vtt; charset=utf-8")], vtt).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "No captions yet" })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
