//! Re-renders captions from a saved `*.segments.json` with a different time shift.
use metocast_transcription::captions::{self, CaptionOptions};
use metocast_transcription::segmenter::Segment;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    seq: u32,
    start_ms: u64,
    end_ms: u64,
    text: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .ok_or("usage: rerender <segments.json> <shift_ms> <out.vtt>")?;
    let shift_ms: i64 = args.next().ok_or("missing shift_ms")?.parse()?;
    let output = args.next().ok_or("missing out.vtt")?;
    let saved: Vec<Saved> = serde_json::from_str(&std::fs::read_to_string(input)?)?;
    let segments: Vec<Segment> = saved
        .into_iter()
        .map(|s| Segment {
            seq: s.seq,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            text: s.text,
        })
        .collect();
    let cues = captions::build_cues(
        &segments,
        &CaptionOptions {
            shift_ms,
            ..CaptionOptions::default()
        },
    );
    std::fs::write(&output, captions::render_vtt(&cues))?;
    println!("{} cues → {output}", cues.len());
    Ok(())
}
