//! The transcriber's own input stream on the RØDECaster: one source, downmixed to mono and cut
//! into 24 kHz PCM16 frames on a worker thread.

use metocast_core::best_effort::LogGone;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use cpal::traits::StreamTrait as _;
use metocast_transcription::framer::{Frame, Framer};
use tokio::sync::mpsc as tokio_mpsc;

use crate::connectors::RodecasterAudioSource;
use crate::connectors::rodecaster_audio::{build_f32_input_stream, select_endpoint};

/// `Err` means the stream died (the desk was unplugged); the channel then closes.
pub type FrameResult = Result<Frame, String>;

/// Opens the stream and returns its frames. Dropping the receiver stops the stream.
///
/// # Errors
/// Returns why the RØDECaster's input could not be opened.
pub fn start(
    source: &RodecasterAudioSource,
    profile: &rodecaster::Profile,
) -> Result<tokio_mpsc::Receiver<FrameResult>, String> {
    let selected = select_endpoint(profile)?;
    let channels = selected.endpoint.channels;
    let picked = selected
        .endpoint
        .sources
        .iter()
        .find(|mapping| &mapping.identity == source)
        .map(|mapping| mapping.host_channels.clone())
        .ok_or_else(|| "configured_rodecaster_source_is_unavailable".to_string())?;
    let sample_rate = selected.endpoint.sample_rate;

    let (frames_tx, frames_rx) = tokio_mpsc::channel(64);
    let (ready_tx, ready_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("transcription-audio".into())
        .spawn(move || {
            let (mono_tx, mono_rx) = mpsc::sync_channel::<Vec<f32>>(64);
            let (error_tx, error_rx) = mpsc::channel::<String>();
            let stream = build_f32_input_stream(
                &selected.device,
                &selected.config,
                move |interleaved| {
                    let mono = interleaved
                        .chunks_exact(channels)
                        .map(|frame| {
                            picked.iter().map(|&c| frame[c]).sum::<f32>() / picked.len() as f32
                        })
                        .collect();
                    // ponytail: a full queue drops audio; only happens if this thread stalls.
                    mono_tx.try_send(mono).log_gone();
                },
                move |error| {
                    error_tx.send(error.to_string()).log_gone();
                },
            )
            .and_then(|stream| {
                stream
                    .play()
                    .map(|()| stream)
                    .map_err(|e| format!("audio_stream_start_failed: {e}"))
            });
            let stream = match stream {
                Ok(stream) => {
                    ready_tx.send(Ok(())).log_gone();
                    stream
                }
                Err(error) => {
                    ready_tx.send(Err(error)).log_gone();
                    return;
                }
            };
            let mut framer = Framer::new(sample_rate);
            loop {
                if let Ok(error) = error_rx.try_recv() {
                    tracing::error!("transcription audio failed: {error}");
                    frames_tx
                        .blocking_send(Err(format!("audio_stream_failed: {error}")))
                        .log_gone();
                    break;
                }
                match mono_rx.recv_timeout(Duration::from_millis(250)) {
                    Ok(mono) => {
                        for frame in framer.push(&mono) {
                            if frames_tx.blocking_send(Ok(frame)).is_err() {
                                return; // the session ended
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) if frames_tx.is_closed() => return,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            drop(stream);
        })
        .map_err(|e| format!("audio_thread_unavailable: {e}"))?;
    ready_rx
        .recv()
        .unwrap_or_else(|_| Err("audio_thread_unavailable".to_string()))?;
    Ok(frames_rx)
}
