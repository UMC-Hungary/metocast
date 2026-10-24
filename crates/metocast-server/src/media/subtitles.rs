//! Transcripts dropped with a program: moved onto its voice, written beside it as SRT, and
//! returned for the caption tracks its uploads send.
use std::path::{Path, PathBuf};

use metocast_core::media::SubtitleSource;
use metocast_transcription::captions::{Cue, parse_cues, render_srt};

use super::sync::{Envelope, align_activity};

/// Speech activity is compared in steps this long; cues don't need finer placement.
const STEP_SECONDS: f64 = 0.1;
/// A transcript ending this far past the program can't be timed to the picture.
const OVERHANG_SECONDS: f64 = 1.0;

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
/// frame), clipped to the program, and whether that match was clear. Without a clear match, a
/// transcript that runs past the program must be timed to the longer recording, so it moves with
/// the recording by `recording_shift` (the sync's placement); any other stays as it was. A live
/// translation's lines trail the speech by varying seconds and rarely match clearly.
pub fn place(cues: &[Cue], voice: &Envelope, recording_shift: f64) -> (Vec<Cue>, f64, bool) {
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
    let overhangs = end as f64 * STEP_SECONDS > voice.duration() + OVERHANG_SECONDS;
    let shift = match (confident, overhangs) {
        (true, _) => shift,
        (false, true) => recording_shift,
        (false, false) => 0.0,
    };
    let shift_ms = (shift * 1000.0).round() as i64;
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
    (placed, shift, confident)
}

/// `<program>.<language>.srt` beside the program.
pub fn write_srt(program: &Path, language: &str, cues: &[Cue]) -> Result<PathBuf, String> {
    let path = program.with_extension(format!("{language}.srt"));
    std::fs::write(&path, render_srt(cues))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silence(seconds: usize) -> Envelope {
        let mut voice = Envelope::new();
        voice.push(&vec![
            0.0;
            seconds * super::super::sync::ENVELOPE_RATE as usize
        ]);
        voice
    }

    fn cue(start_s: u64, end_s: u64) -> Cue {
        Cue {
            start_ms: start_s * 1000,
            end_ms: end_s * 1000,
            text: "words".to_string(),
        }
    }

    #[test]
    fn a_transcript_longer_than_the_program_moves_with_the_recording_when_the_voice_cant_say() {
        let cues = [cue(10, 20), cue(25, 90)];
        let (placed, shift, confident) = place(&cues, &silence(60), 40.5);
        assert!(!confident);
        assert_eq!(shift, 40.5);
        assert_eq!(placed.len(), 1, "the line moved past the end is dropped");
        assert_eq!((placed[0].start_ms, placed[0].end_ms), (50_500, 60_000));
    }

    #[test]
    fn a_transcript_that_fits_the_program_stays_when_the_voice_cant_say() {
        let cues = [cue(10, 20), cue(25, 59)];
        let (placed, shift, confident) = place(&cues, &silence(60), 40.5);
        assert_eq!((shift, confident), (0.0, false));
        assert_eq!(placed, cues);
    }
}
