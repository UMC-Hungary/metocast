//! Transcripts dropped with a program: moved onto its voice, written beside it as SRT, and
//! returned for the caption tracks its uploads send.
use std::path::{Path, PathBuf};

use metocast_core::media::SubtitleSource;
use metocast_transcription::captions::{Cue, parse_cues, render_srt};

use super::sync::{Envelope, align_activity};

/// Speech activity is compared in steps this long; cues don't need finer placement.
const STEP_SECONDS: f64 = 0.1;

/// The cues of `.srt` or `.vtt` `path`, refusing anything else before a job is queued.
pub fn read(source: &SubtitleSource) -> Result<Vec<Cue>, String> {
    let path = Path::new(&source.path);
    let supported = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("srt") || ext.eq_ignore_ascii_case("vtt"));
    if !path.is_absolute() || !supported {
        return Err(format!("not an .srt or .vtt file: {}", source.path));
    }
    let language = &source.language;
    if !(2..=3).contains(&language.len()) || !language.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(format!("not a language code: {language}"));
    }
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", source.path))?;
    parse_cues(&text).map_err(|e| format!("{}: {e}", source.path))
}

/// `cues` moved to where their speech matches `voice` (the program audio, from its first
/// frame), clipped to the program. The shift is kept only when the match is clear.
pub fn place(cues: &[Cue], voice: &Envelope) -> (Vec<Cue>, f64, bool) {
    let step = |ms: u64| (ms as f64 / 1000.0 / STEP_SECONDS) as usize;
    let end = cues.iter().map(|cue| step(cue.end_ms)).max().unwrap_or(0);
    let mut activity = vec![Some(0.0); end + 1];
    for cue in cues {
        let (from, to) = (step(cue.start_ms), step(cue.end_ms));
        if from < to {
            activity[from..to].fill(Some(1.0));
        }
    }
    let (shift, confident) = align_activity(&activity, voice, STEP_SECONDS).unwrap_or((0.0, false));
    let shift_ms = if confident {
        (shift * 1000.0).round() as i64
    } else {
        0
    };
    let length_ms = (voice.duration() * 1000.0) as i64;
    let placed = cues
        .iter()
        .filter_map(|cue| {
            let start = (cue.start_ms as i64 + shift_ms).max(0);
            let end = (cue.end_ms as i64 + shift_ms).min(length_ms);
            (end > start).then(|| Cue {
                start_ms: start as u64,
                end_ms: end as u64,
                text: cue.text.clone(),
            })
        })
        .collect();
    (placed, if confident { shift } else { 0.0 }, confident)
}

/// `<program>.<language>.srt` beside the program.
pub fn write_srt(program: &Path, language: &str, cues: &[Cue]) -> Result<PathBuf, String> {
    let path = program.with_extension(format!("{language}.srt"));
    std::fs::write(&path, render_srt(cues))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}
