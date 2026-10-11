//! Acceptance check for final-video re-timing: compares cue starts against whisper-1's
//! audio-anchored segments, for a fixed lag shift and for onset snapping.
//!
//! `retime_check <media> <live.segments.json> <whisper.segments.json> [lag_ms]`
//! Target: p90 |error| ≤ 1.5 s after snapping.
use std::collections::HashSet;
use std::path::PathBuf;

use metocast_transcription::decode::AudioReader;
use metocast_transcription::framer::Framer;
use metocast_transcription::retime::{RetimeOptions, retime};
use metocast_transcription::segmenter::Segment;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    seq: u32,
    start_ms: u64,
    end_ms: u64,
    text: String,
}

fn load(path: &str) -> Result<Vec<Segment>, Box<dyn std::error::Error>> {
    let saved: Vec<Saved> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    Ok(saved
        .into_iter()
        .map(|s| Segment {
            seq: s.seq,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            text: s.text,
        })
        .collect())
}

fn words(text: &str) -> HashSet<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(str::to_lowercase)
        .collect()
}

/// The whisper segment that says the same thing, near where the live text appeared.
fn anchor(live: &Segment, whisper: &[Segment]) -> Option<u64> {
    let ours = words(&live.text);
    whisper
        .iter()
        .filter(|w| w.start_ms + 15_000 >= live.start_ms && w.start_ms <= live.start_ms + 3_000)
        .map(|w| (ours.intersection(&words(&w.text)).count(), w.start_ms))
        .filter(|(shared, _)| *shared * 10 >= ours.len().max(1) * 3)
        .max_by_key(|(shared, start)| (*shared, std::cmp::Reverse(start.abs_diff(live.start_ms))))
        .map(|(_, start)| start)
}

fn report(name: &str, mut errors: Vec<i64>) {
    errors.sort_unstable();
    let p = |q: usize| errors[(errors.len() - 1) * q / 100];
    let mut abs: Vec<u64> = errors.iter().map(|e| e.unsigned_abs()).collect();
    abs.sort_unstable();
    println!(
        "{name:>8}: n={} p10={} p50={} p90={} ms, p90 |err|={} ms",
        errors.len(),
        p(10),
        p(50),
        p(90),
        abs[(abs.len() - 1) * 90 / 100]
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: retime_check <media> <live.segments.json> <whisper.segments.json> [lag_ms]";
    let media = PathBuf::from(args.next().ok_or(usage)?);
    let live = load(&args.next().ok_or(usage)?)?;
    let whisper = load(&args.next().ok_or(usage)?)?;
    let lag_ms: u64 = args.next().map_or(Ok(3_200), |s| s.parse())?;

    let mut reader = AudioReader::open(&media, 0.0, None)?;
    let mut framer = Framer::new(reader.sample_rate());
    let mut onsets = Vec::new();
    while let Some(chunk) = reader.next_chunk()? {
        onsets.extend(framer.push(&chunk).into_iter().flat_map(|f| f.onsets_ms));
    }
    let options = RetimeOptions {
        lag_ms,
        ..RetimeOptions::default()
    };
    let snapped = retime(&live, &onsets, &options);
    let (mut shifted, mut retimed) = (Vec::new(), Vec::new());
    for (original, snapped) in live.iter().zip(&snapped) {
        if let Some(start) = anchor(original, &whisper) {
            let start = i64::try_from(start)?;
            shifted.push(i64::try_from(original.start_ms.saturating_sub(lag_ms))? - start);
            retimed.push(i64::try_from(snapped.start_ms)? - start);
        }
    }
    println!(
        "{} onsets, {} of {} cues anchored",
        onsets.len(),
        retimed.len(),
        live.len()
    );
    report("shift", shifted);
    report("retimed", retimed);
    Ok(())
}
