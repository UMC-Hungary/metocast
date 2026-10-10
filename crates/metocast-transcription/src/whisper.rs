//! Batch baseline: OpenAI `whisper-1` translations (any language → English) with segment
//! timestamps, sent as WAV chunks under the 25 MB upload limit.

use std::ops::Range;

use reqwest::multipart::{Form, Part};
use serde::Deserialize;

use crate::segmenter::Segment;
use crate::{Error, Result};

pub const DEFAULT_URL: &str = "https://api.openai.com/v1/audio/translations";
pub const SAMPLE_RATE: u32 = 16_000;
/// 12 minutes of 16 kHz mono PCM16 is about 23 MB.
pub const MAX_CHUNK_SECONDS: u32 = 720;
const SEARCH_SECONDS: u32 = 30;
const QUIET_WINDOW_MS: u32 = 200;

#[derive(Deserialize)]
struct TranslationResponse {
    #[serde(default)]
    segments: Vec<ResponseSegment>,
    #[serde(default)]
    text: String,
}

#[derive(Deserialize)]
struct ResponseSegment {
    start: f64,
    end: f64,
    text: String,
}

/// Splits `pcm` into chunks of at most `max_samples`, cutting at the quietest
/// `QUIET_WINDOW_MS` window in the last `SEARCH_SECONDS` before each limit so words aren't split.
#[must_use]
pub fn plan_chunks(pcm: &[i16], sample_rate: u32, max_samples: usize) -> Vec<Range<usize>> {
    let search = (sample_rate * SEARCH_SECONDS) as usize;
    let window = (sample_rate * QUIET_WINDOW_MS / 1_000) as usize;
    let mut chunks = Vec::new();
    let mut start = 0;
    while pcm.len() - start > max_samples {
        let limit = start + max_samples;
        let from = limit.saturating_sub(search).max(start + window);
        let cut = (from..=limit - window)
            .step_by(window / 2)
            .min_by_key(|&i| {
                pcm[i..i + window]
                    .iter()
                    .map(|s| u64::from(s.unsigned_abs()))
                    .sum::<u64>()
            })
            .map_or(limit, |i| i + window / 2);
        chunks.push(start..cut);
        start = cut;
    }
    chunks.push(start..pcm.len());
    chunks
}

#[must_use]
pub fn wav_bytes(pcm: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1_u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend(pcm.iter().flat_map(|s| s.to_le_bytes()));
    out
}

/// Translates 16 kHz mono PCM16 to English segments with times relative to the start of `pcm`.
///
/// # Errors
/// Returns an error when a request fails or the API rejects a chunk.
pub async fn translate(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    pcm: &[i16],
    mut on_chunk: impl FnMut(usize, usize),
) -> Result<Vec<Segment>> {
    let chunks = plan_chunks(pcm, SAMPLE_RATE, (SAMPLE_RATE * MAX_CHUNK_SECONDS) as usize);
    let mut segments = Vec::new();
    for (index, range) in chunks.iter().enumerate() {
        on_chunk(index + 1, chunks.len());
        let offset_ms = range.start as u64 * 1_000 / u64::from(SAMPLE_RATE);
        let file = Part::bytes(wav_bytes(&pcm[range.clone()], SAMPLE_RATE))
            .file_name(format!("chunk-{index}.wav"))
            .mime_str("audio/wav")
            .map_err(|e| Error::Http(e.to_string()))?;
        let form = Form::new()
            .text("model", "whisper-1")
            .text("response_format", "verbose_json")
            .part("file", file);
        let response = client
            .post(url)
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| Error::Http(e.without_url().to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| Error::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Error::Api(format!(
                "HTTP {status}: {}",
                body.chars().take(500).collect::<String>()
            )));
        }
        let parsed: TranslationResponse = serde_json::from_str(&body)
            .map_err(|e| Error::Api(format!("unexpected response: {e}")))?;
        if parsed.segments.is_empty() && !parsed.text.trim().is_empty() {
            segments.push(Segment {
                seq: 0,
                start_ms: offset_ms,
                end_ms: offset_ms + (range.len() as u64 * 1_000 / u64::from(SAMPLE_RATE)),
                text: parsed.text.trim().to_string(),
            });
        }
        for s in parsed.segments {
            let text = s.text.trim();
            if text.is_empty() {
                continue;
            }
            segments.push(Segment {
                seq: 0,
                start_ms: offset_ms + (s.start * 1_000.0) as u64,
                end_ms: offset_ms + (s.end * 1_000.0) as u64,
                text: text.to_string(),
            });
        }
    }
    for (i, segment) in segments.iter_mut().enumerate() {
        segment.seq = i as u32 + 1;
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_respect_limit_and_cut_in_silence() {
        let rate = 1_000;
        // 100 s of "speech" with a silent gap at 55 s.
        let mut pcm = vec![10_000_i16; 100 * rate];
        pcm[55 * rate..55 * rate + 500].fill(0);
        let chunks = plan_chunks(&pcm, rate as u32, 60 * rate);
        assert_eq!(chunks.len(), 2);
        assert!(chunks[0].end >= 55 * rate && chunks[0].end <= 55 * rate + 500);
        assert_eq!(chunks[1].end, pcm.len());
        assert!(chunks.iter().all(|c| c.len() <= 60 * rate));
    }

    #[test]
    fn wav_header_is_well_formed() {
        let wav = wav_bytes(&[0, 1, -1], 16_000);
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 6);
    }
}
