//! Streaming audio decode from a media file (`.mov`, `.mp4`, `.m4a`, `.wav`) to mono `f32`.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use crate::{Error, Result};

/// Reads the first audio track of a file packet by packet, so multi-gigabyte video files are
/// never loaded into memory.
pub struct AudioReader {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track_id: u32,
    sample_rate: u32,
    channels: usize,
    start_seconds: f64,
    end_frame: Option<u64>,
    frames_read: u64,
}

impl AudioReader {
    /// Opens `path`, optionally seeking to `start_seconds` and stopping after
    /// `duration_seconds` of audio.
    ///
    /// # Errors
    /// Returns an error when the file can't be opened, has no decodable audio track, or the
    /// seek fails.
    pub fn open(path: &Path, start_seconds: f64, duration_seconds: Option<f64>) -> Result<Self> {
        let file = File::open(path)?;
        let source = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(extension);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                source,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| {
                Error::Decode(format!(
                    "unsupported container ({e}); on macOS convert it first with \
                     `avconvert --preset PresetAppleM4A --source <input> --output <output>.m4a`"
                ))
            })?;
        let mut format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| {
                t.codec_params.codec != CODEC_TYPE_NULL && t.codec_params.sample_rate.is_some()
            })
            .ok_or_else(|| Error::Decode("no decodable audio track".into()))?;
        let track_id = track.id;
        let params = track.codec_params.clone();
        let sample_rate = params
            .sample_rate
            .ok_or_else(|| Error::Decode("audio track has no sample rate".into()))?;
        let channels = params.channels.map_or(1, |c| c.count().max(1));
        let mut decoder = symphonia::default::get_codecs()
            .make(&params, &DecoderOptions::default())
            .map_err(|e| Error::Decode(format!("unsupported audio codec: {e}")))?;

        let mut actual_start = 0.0;
        if start_seconds > 0.0 {
            let seeked = format
                .seek(
                    SeekMode::Accurate,
                    SeekTo::Time {
                        time: Time::new(start_seconds.trunc() as u64, start_seconds.fract()),
                        track_id: Some(track_id),
                    },
                )
                .map_err(|e| Error::Decode(format!("seek failed: {e}")))?;
            decoder.reset();
            actual_start = params.time_base.map_or(start_seconds, |tb| {
                let time = tb.calc_time(seeked.actual_ts);
                time.seconds as f64 + time.frac
            });
        }

        Ok(Self {
            format,
            decoder,
            track_id,
            sample_rate,
            channels,
            start_seconds: actual_start,
            end_frame: duration_seconds.map(|d| (d * f64::from(sample_rate)).round() as u64),
            frames_read: 0,
        })
    }

    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    #[must_use]
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Where decoding actually started, in seconds from the beginning of the file.
    #[must_use]
    pub fn start_seconds(&self) -> f64 {
        self.start_seconds
    }

    /// Returns the next block of mono samples (all channels averaged), or `None` at the end.
    ///
    /// # Errors
    /// Returns an error on unrecoverable demux or decode failures; corrupt packets are skipped.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>> {
        loop {
            let remaining = self
                .end_frame
                .map(|end| end.saturating_sub(self.frames_read));
            if remaining == Some(0) {
                return Ok(None);
            }
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(Error::Decode(e.to_string())),
            };
            if packet.track_id() != self.track_id {
                continue;
            }
            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(Error::Decode(e.to_string())),
            };
            let spec = *decoded.spec();
            let channels = spec.channels.count().max(1);
            let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            buffer.copy_interleaved_ref(decoded);
            let mut mono: Vec<f32> = buffer
                .samples()
                .chunks_exact(channels)
                .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                .collect();
            if let Some(remaining) = remaining
                && mono.len() as u64 > remaining
            {
                mono.truncate(remaining as usize);
            }
            if mono.is_empty() {
                continue;
            }
            self.frames_read += mono.len() as u64;
            return Ok(Some(mono));
        }
    }
}
