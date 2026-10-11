//! The `media` queue's worker: one job at a time, since a 6K render already keeps the GPU, the
//! media engine and the source disk busy, and audio cleaning keeps the CPU busy. It runs renders
//! here, and audio masters ([`super::audio`]) and programs ([`super::program`]) in their modules.
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::{Duration, Instant};

use serde_json::json;
use sqlx::PgPool;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use metocast_core::media::{MediaKind, MediaProbe, RenderProgress, RenderState, output_crop};

use super::{
    MEDIA_QUEUE, RenderPayload, Tools, broadcast_progress, command, partial_path, probe_source,
};
use crate::queue::{self, Job};
use crate::server::AppState;

const TICK: Duration = Duration::from_secs(30);
const HEARTBEAT: Duration = Duration::from_secs(30);
/// A live render refreshes its lock every [`HEARTBEAT`]; one silent this long has died.
const VISIBILITY_TIMEOUT_SECS: i64 = 600;
const PROGRESS_EVERY: Duration = Duration::from_secs(1);
const STDERR_TAIL_BYTES: usize = 4000;
const CANCELLED: &str = "cancelled";

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        loop {
            if let Err(e) =
                queue::reclaim_stuck(&state.pool, MEDIA_QUEUE, VISIBILITY_TIMEOUT_SECS).await
            {
                tracing::error!("media: reclaim_stuck failed: {e}");
            }
            loop {
                let job =
                    match queue::claim_batch(&state.pool, MEDIA_QUEUE, &state.server_id, 1).await {
                        Ok(mut jobs) => match jobs.pop() {
                            Some(job) => job,
                            None => break,
                        },
                        Err(e) => {
                            tracing::error!("media: claim failed: {e}");
                            break;
                        }
                    };
                let outcome = match job.job_type.as_str() {
                    super::audio::AUDIO_JOB => super::audio::process(&state, &job).await,
                    super::program::PROGRAM_JOB => super::program::process(&state, &job).await,
                    super::program::NUDGE_JOB => super::program::nudge(&state, &job).await,
                    _ => render(&state, &job).await,
                };
                let recorded = match outcome {
                    Ok(()) => queue::complete(&state.pool, job.id).await,
                    Err(error) => {
                        tracing::error!("media: job {} failed: {error}", job.id);
                        queue::fail(&state.pool, &job, &error).await
                    }
                };
                if let Err(e) = recorded {
                    tracing::error!("media: could not record the outcome of {}: {e}", job.id);
                }
            }
            tokio::select! {
                () = state.media.wake.notified() => {}
                _ = ticker.tick() => {}
            }
        }
    });
}

async fn render(state: &AppState, job: &Job) -> Result<(), String> {
    let payload: RenderPayload = serde_json::from_value(job.payload.clone())
        .map_err(|e| format!("unreadable job payload: {e}"))?;
    let output = PathBuf::from(&payload.output_path);
    let partial = partial_path(&output);
    let progress =
        |state_now: RenderState, frame: u64, total: u64, error: Option<String>| RenderProgress {
            job_id: job.id,
            state: state_now,
            frame,
            total_frames: total,
            fps: 0.0,
            output_path: payload.output_path.clone(),
            error,
        };

    let (stop, stopped) = oneshot::channel();
    state.media.running.lock().await.insert(job.id, stop);
    tracing::info!(job = %job.id, source = %payload.path, output = %payload.output_path, "media: rendering");
    let result = async {
        let probed = probe_source(state, Path::new(&payload.path), None)
            .await
            .map_err(|e| e.message())?;
        if !probed.probe.missing.is_empty() {
            return Err(format!(
                "this Mac is missing: {}",
                probed.probe.missing.join(", ")
            ));
        }
        if let Some(folder) = output.parent() {
            std::fs::create_dir_all(folder)
                .map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
        }
        let crop = output_crop(
            probed.probe.active_area,
            payload.settings.aspect,
            payload.settings.crop_shift_y,
        );
        let source = command::Source {
            path: Path::new(&payload.path),
            kind: probed.probe.kind,
            width: probed.probe.width,
            height: probed.probe.height,
            fps_num: probed.probe.fps_num,
            fps_den: probed.probe.fps_den,
            start_timecode: probed.probe.start_timecode.as_deref(),
            audio_channels: probed.probe.audio.as_ref().map(|audio| audio.channels),
            pixel_format: &probed.pixel_format,
        };
        let commands = command::render_commands(
            &source,
            &payload.settings,
            crop,
            probed.tools.braw_sdk.as_deref(),
            &partial,
        );
        run(
            state,
            job.id,
            &probed.tools,
            &commands,
            &probed.probe,
            &payload,
            stopped,
        )
        .await?;
        std::fs::rename(&partial, &output)
            .map_err(|e| format!("cannot move the render into place: {e}"))?;
        register(state, &payload, &probed.probe, &output, crop).await;
        Ok(probed.probe.frame_count)
    }
    .await;
    state.media.running.lock().await.remove(&job.id);

    match result {
        Ok(frames) => {
            tracing::info!(job = %job.id, frames, output = %payload.output_path, "media: rendered");
            broadcast_progress(
                state,
                progress(RenderState::Succeeded, frames, frames, None),
            )
            .await;
            Ok(())
        }
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            let ended = if error == CANCELLED {
                progress(RenderState::Cancelled, 0, 0, None)
            } else {
                progress(RenderState::Failed, 0, 0, Some(error.clone()))
            };
            broadcast_progress(state, ended).await;
            Err(error)
        }
    }
}

/// Frames encoded so far and the average rate, as ffmpeg reports them.
#[derive(Debug, Clone, Copy)]
pub(super) struct Progress {
    pub frame: u64,
    pub fps: f64,
}

/// [`execute`] for a queued job: keeps its lock fresh and broadcasts its progress.
async fn run(
    state: &AppState,
    job_id: Uuid,
    tools: &Tools,
    commands: &command::RenderCommands,
    probe: &MediaProbe,
    payload: &RenderPayload,
    stopped: oneshot::Receiver<()>,
) -> Result<(), String> {
    let (progress, mut updates) = mpsc::unbounded_channel::<Progress>();
    let forwarder = {
        let state = state.clone();
        let output_path = payload.output_path.clone();
        let total_frames = probe.frame_count;
        tokio::spawn(async move {
            while let Some(update) = updates.recv().await {
                broadcast_progress(
                    &state,
                    RenderProgress {
                        job_id,
                        state: RenderState::Running,
                        frame: update.frame,
                        total_frames,
                        fps: update.fps,
                        output_path: output_path.clone(),
                        error: None,
                    },
                )
                .await;
            }
        })
    };
    let result = execute(
        tools,
        commands,
        probe.frame_count,
        Some((&state.pool, job_id)),
        progress,
        stopped,
    )
    .await;
    let _ = forwarder.await;
    result
}

/// Runs `braw-decode | ffmpeg` (or ffmpeg alone) to completion, cancellation or failure.
pub(super) async fn execute(
    tools: &Tools,
    commands: &command::RenderCommands,
    total_frames: u64,
    heartbeat: Option<(&PgPool, Uuid)>,
    progress: mpsc::UnboundedSender<Progress>,
    mut stopped: oneshot::Receiver<()>,
) -> Result<(), String> {
    let ffmpeg_path = tools.ffmpeg.as_deref().ok_or("ffmpeg is not installed")?;
    let mut decoder = match &commands.decoder {
        Some(args) => {
            let program = tools
                .braw_decode
                .as_deref()
                .ok_or("braw-decode is missing")?;
            Some(
                Command::new(program)
                    .args(args)
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|e| format!("could not start braw-decode: {e}"))?,
            )
        }
        None => None,
    };
    let frames_in: Stdio = match decoder.as_mut().and_then(|decoder| decoder.stdout.take()) {
        Some(stdout) => stdout
            .try_into()
            .map_err(|e| format!("could not connect braw-decode to ffmpeg: {e}"))?,
        None => Stdio::null(),
    };
    let mut ffmpeg = Command::new(ffmpeg_path)
        .args(&commands.ffmpeg)
        .stdin(frames_in)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not start ffmpeg: {e}"))?;

    let decoder_errors = decoder
        .as_mut()
        .and_then(|decoder| decoder.stderr.take())
        .map(|stderr| tokio::spawn(tail(stderr)));
    let ffmpeg_errors = ffmpeg
        .stderr
        .take()
        .map(|stderr| tokio::spawn(tail(stderr)));
    let reporter = ffmpeg
        .stdout
        .take()
        .map(|stdout| tokio::spawn(report_progress(progress, stdout)));

    let mut beat = tokio::time::interval(HEARTBEAT);
    let ffmpeg_status = loop {
        tokio::select! {
            status = ffmpeg.wait() => break status.map_err(|e| format!("ffmpeg: {e}"))?,
            _ = beat.tick(), if heartbeat.is_some() => {
                if let Some((pool, job_id)) = heartbeat
                    && let Err(e) = queue::heartbeat(pool, job_id).await
                {
                    tracing::warn!("media: heartbeat for {job_id} failed: {e}");
                }
            }
            Ok(()) = &mut stopped => {
                stop(&mut ffmpeg).await;
                if let Some(decoder) = decoder.as_mut() {
                    stop(decoder).await;
                }
                return Err(CANCELLED.to_string());
            }
        }
    };
    // Once ffmpeg is gone the decoder ends too (its pipe closes); don't let it linger.
    let decoder_status = match decoder.as_mut() {
        Some(decoder) => {
            match tokio::time::timeout(Duration::from_secs(10), decoder.wait()).await {
                Ok(status) => Some(status.map_err(|e| format!("braw-decode: {e}"))?),
                Err(_) => {
                    stop(decoder).await;
                    None
                }
            }
        }
        None => None,
    };
    let frames = match reporter {
        Some(task) => task.await.unwrap_or(0),
        None => 0,
    };
    let collect = |task: Option<tokio::task::JoinHandle<String>>| async move {
        match task {
            Some(task) => task.await.unwrap_or_default(),
            None => String::new(),
        }
    };
    let (decoder_errors, ffmpeg_errors) =
        (collect(decoder_errors).await, collect(ffmpeg_errors).await);

    // The decoder's own failure explains a short or broken encode best, so it is checked first.
    if let Some(status) = decoder_status
        && !status.success()
    {
        return Err(failure("braw-decode", status, &decoder_errors));
    }
    if !ffmpeg_status.success() {
        return Err(failure("ffmpeg", ffmpeg_status, &ffmpeg_errors));
    }
    if frames + 1 < total_frames {
        return Err(format!(
            "only {frames} of {total_frames} frames were encoded"
        ));
    }
    Ok(())
}

fn failure(program: &str, status: ExitStatus, stderr: &str) -> String {
    let detail = stderr.trim();
    let detail = detail.lines().last().unwrap_or(detail);
    match status.code() {
        Some(code) => format!("{program} exited with {code}: {detail}"),
        None => format!("{program} was stopped: {detail}"),
    }
}

async fn stop(child: &mut Child) {
    let _ = child.start_kill();
    let _ = child.wait().await;
}

/// The last few kilobytes a program wrote, for the error message.
async fn tail(mut reader: impl AsyncRead + Unpin) -> String {
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

/// Reads ffmpeg's `-progress` lines, passing them on about once a second. Returns the last frame.
async fn report_progress(
    progress: mpsc::UnboundedSender<Progress>,
    stdout: impl AsyncRead + Unpin,
) -> u64 {
    let started = Instant::now();
    let mut last_sent: Option<Instant> = None;
    let mut frame = 0u64;
    let mut lines = BufReader::new(stdout).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(value) = line.strip_prefix("frame=") {
            frame = value.trim().parse().unwrap_or(frame);
        } else if line.starts_with("progress=")
            && last_sent.is_none_or(|sent| sent.elapsed() >= PROGRESS_EVERY)
        {
            last_sent = Some(Instant::now());
            let seconds = started.elapsed().as_secs_f64();
            let fps = if seconds > 0.0 {
                frame as f64 / seconds
            } else {
                0.0
            };
            let _ = progress.send(Progress { frame, fps });
        }
    }
    frame
}

/// Files the finished render under its event, or as untracked, with the timeline facts the
/// later sync and subtitle steps line up against.
async fn register(
    state: &AppState,
    payload: &RenderPayload,
    probe: &MediaProbe,
    output: &Path,
    frame: metocast_core::media::CropRect,
) {
    let source = match probe.kind {
        MediaKind::Braw => "braw_render",
        MediaKind::Video => "video_render",
    };
    let metadata = json!({
        "source": source,
        "timeline": {
            "startTimecode": probe.start_timecode,
            "fps": format!("{}/{}", probe.fps_num, probe.fps_den),
            "tcSource": if probe.kind == MediaKind::Braw { "camera_tmcd" } else { "container" },
            "sourcePath": payload.path,
            "camera": probe.camera,
            "clipName": probe.clip_name,
            "recordedAt": probe.recorded_at,
            "hostRecordStartedAt": probe.host_record_started_at,
        },
        "render": payload.settings,
        "frame": frame,
    });

    super::file_recording(
        state,
        payload.event_id,
        output,
        probe.duration_seconds,
        "video",
        source,
        &metadata,
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::super::probe;
    use super::*;
    use metocast_core::media::{CropRect, MediaConfig};

    /// Renders two seconds of a real clip through braw-decode and ffmpeg, then cancels a longer
    /// render. It needs the tools and a clip, so it runs only on request:
    ///
    /// ```text
    /// METOCAST_BRAW_DECODE=braw-decode/build/braw-decode \
    /// METOCAST_TEST_BRAW=/Volumes/A002/A002_09160323_C001.braw \
    /// METOCAST_TEST_BRAW_SDK="/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac/Libraries" \
    /// cargo test -p metocast-server real_clip -- --ignored
    /// ```
    #[tokio::test]
    #[ignore = "needs braw-decode, ffmpeg and a .braw clip"]
    async fn a_real_clip_renders_through_the_pipe_and_can_be_cancelled() {
        let clip = PathBuf::from(std::env::var("METOCAST_TEST_BRAW").expect("METOCAST_TEST_BRAW"));
        let config = MediaConfig {
            braw_sdk_path: std::env::var("METOCAST_TEST_BRAW_SDK").unwrap_or_default(),
            ..MediaConfig::default()
        };
        let tools = Tools::find(&config, Path::new("/nonexistent"));
        assert!(tools.missing(MediaKind::Braw).is_empty(), "{tools:?}");
        let ffprobe = tools.ffprobe.clone().unwrap();
        let facts = probe::ffprobe(&ffprobe, &clip).await.unwrap();
        let info = probe::braw_info(
            tools.braw_decode.as_deref().unwrap(),
            tools.braw_sdk.as_deref(),
            None,
            &clip,
        )
        .await
        .unwrap();
        let settings = config.settings().unwrap();
        let crop = output_crop(info.active_area, settings.aspect, settings.crop_shift_y);
        let source = command::Source {
            path: &clip,
            kind: MediaKind::Braw,
            width: info.width,
            height: info.height,
            fps_num: facts.fps_num,
            fps_den: facts.fps_den,
            start_timecode: facts.start_timecode.as_deref(),
            audio_channels: facts.audio.as_ref().map(|audio| audio.channels),
            pixel_format: &info.pixel_format,
        };
        let folder = std::env::temp_dir().join(format!("metocast-render-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&folder).unwrap();
        let output = folder.join("two-seconds.mov");
        let mut commands =
            command::render_commands(&source, &settings, crop, tools.braw_sdk.as_deref(), &output);
        // Two seconds: 48 frames from the decoder, and the audio cut to match.
        let decoder = commands.decoder.as_mut().unwrap();
        let clip_at = decoder.len() - 1;
        decoder.splice(clip_at..clip_at, ["--count".to_string(), "48".to_string()]);
        let output_at = commands.ffmpeg.len() - 1;
        commands
            .ffmpeg
            .splice(output_at..output_at, ["-t".to_string(), "2".to_string()]);

        let (progress, mut updates) = mpsc::unbounded_channel();
        let (_keep, stopped) = oneshot::channel();
        execute(&tools, &commands, 48, None, progress, stopped)
            .await
            .unwrap();
        assert!(updates.recv().await.is_some(), "progress was reported");

        let rendered = probe::ffprobe(&ffprobe, &output).await.unwrap();
        assert_eq!(rendered.kind, MediaKind::Video);
        let rendered_frame = CropRect {
            x: crop.x,
            y: crop.y,
            width: rendered.width,
            height: rendered.height,
        };
        assert_eq!(rendered_frame, crop);
        assert_eq!(rendered.frame_count, 48);
        assert_eq!(rendered.start_timecode, facts.start_timecode);
        assert_eq!(rendered.audio.map(|audio| audio.channels), Some(2));

        // A full-length render, stopped after a second.
        let cancelled_output = folder.join("cancelled.mov");
        let full = command::render_commands(
            &source,
            &settings,
            crop,
            tools.braw_sdk.as_deref(),
            &cancelled_output,
        );
        let (progress, _updates) = mpsc::unbounded_channel();
        let (stop, stopped) = oneshot::channel();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = stop.send(());
        });
        let cancelled = execute(&tools, &full, info.frame_count, None, progress, stopped).await;
        assert_eq!(cancelled, Err(CANCELLED.to_string()));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
