//! A transcript placed on the speech it translates. The bundled `whisper-cli` (whisper.cpp)
//! translates the program's audio to English with a time for every word. The dropped transcript
//! (usually the live translation, whose lines trail the speech by a varying 2–6 s and arrive as
//! fragments) is rebuilt into whole sentences, and its words are matched in order with the ones
//! Whisper heard. Each caption starts when its first word is spoken. Whisper's own wording is
//! never shown; it is only the clock.
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;

use metocast_transcription::captions::Cue;
use serde::Deserialize;
use tokio::io::AsyncBufReadExt as _;
use tokio::process::Command;
use tokio::sync::watch;

use super::audio;

/// The speech's language. Whisper translates it to English.
// ponytail: the church's sermons are Hungarian; take it from the live service's settings if another congregation uses Metocast.
const SPEECH_LANGUAGE: &str = "hu";
/// The transcripts this can place: Whisper's reference is English.
pub const PLACEABLE_LANGUAGE: &str = "en";

/// Captions hold at most two lines of this many characters.
const LINE_CHARS: usize = 42;
const MIN_CAPTION_SECONDS: f64 = 1.2;
const MAX_CAPTION_SECONDS: f64 = 7.0;
/// After the whole-file offset, a word may meet one heard this far either side of its estimate.
const BAND_SECONDS: f64 = 8.0;
/// Words before the first matched one in a caption take about this long each.
const SECONDS_PER_WORD: f64 = 0.35;
/// The whole-file offset needs this many rare words agreeing on it…
const MIN_OFFSET_VOTES: u32 = 12;
/// …and at least this share of the transcript's content words matched afterwards.
const MIN_MATCHED_SHARE: f64 = 0.3;

/// A word Whisper heard in the program, and when (seconds from its first frame).
#[derive(Debug, Clone, PartialEq)]
pub struct Heard {
    pub text: String,
    pub at: f64,
}

/// What [`place_on_speech`] made of a transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub cues: Vec<Cue>,
    /// How far the transcript's own clock was moved as a whole, before each sentence was placed.
    pub offset_seconds: f64,
    /// Captions with at least one word matched to the speech; the rest sit between them.
    pub matched: u32,
}

// ── Running Whisper ─────────────────────────────────────────────────────────────

/// The words of `audio` (the program's sound, any WAV: whisper.cpp resamples it), translated
/// to English, each with its time. `output` is a path prefix for Whisper's JSON, removed after.
pub async fn hear(
    whisper: &Path,
    model: &Path,
    vad_model: &Path,
    audio: &Path,
    output: &Path,
    progress: &impl audio::StepProgress,
    mut cancel: watch::Receiver<bool>,
) -> Result<Vec<Heard>, String> {
    let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
    // `--vad` and `-mc 0` matter: without them a silent start (common: the picture runs before
    // the recording) sends Whisper into repeating one invented sentence for the whole file.
    // `-ml 1 -sow` gives every word its own segment, whose times are on the original timeline
    // even with `--vad` (token offsets are not). `-pp` prints `progress = N%` on stderr.
    let mut child = Command::new(whisper)
        .arg("-m")
        .arg(model)
        .arg("-f")
        .arg(audio)
        .args(["-l", SPEECH_LANGUAGE, "-tr", "-mc", "0", "--vad", "-vm"])
        .arg(vad_model)
        .args([
            "-ml",
            "1",
            "-sow",
            "-np",
            "-pp",
            "-t",
            &threads.to_string(),
            "-oj",
            "-of",
        ])
        .arg(output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("could not start whisper-cli: {e}"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or("whisper-cli gave no errors stream")?;
    let mut lines = tokio::io::BufReader::new(stderr).lines();
    let mut last_line = String::new();
    loop {
        let line = tokio::select! {
            line = lines.next_line() => line.map_err(|e| format!("reading whisper-cli: {e}"))?,
            () = audio::cancelled(&mut cancel) => return Err(audio::CANCELLED.to_string()),
        };
        let Some(line) = line else { break };
        match progress_of(&line) {
            Some(fraction) => progress.fraction(fraction).await,
            None if !line.trim().is_empty() => last_line = line,
            None => {}
        }
    }
    let status = child
        .wait()
        .await
        .map_err(|e| format!("whisper-cli: {e}"))?;
    if !status.success() {
        return Err(format!("whisper-cli failed: {}", last_line.trim()));
    }
    let json = output.with_extension("json");
    let text = std::fs::read_to_string(&json)
        .map_err(|e| format!("cannot read {}: {e}", json.display()))?;
    crate::cleanup::remove_file_if_present(&json).await;
    parse_heard(&text)
}

#[derive(Deserialize)]
struct WhisperJson {
    transcription: Vec<WhisperSegment>,
}

#[derive(Deserialize)]
struct WhisperSegment {
    offsets: WhisperOffsets,
    text: String,
}

#[derive(Deserialize)]
struct WhisperOffsets {
    from: u64,
}

/// The fraction in a `whisper_print_progress_callback: progress =  45%` line.
fn progress_of(line: &str) -> Option<f64> {
    let percent = line.split_once("progress =")?.1.trim().strip_suffix('%')?;
    percent
        .trim()
        .parse::<f64>()
        .ok()
        .map(|p| (p / 100.0).clamp(0.0, 1.0))
}

/// `whisper-cli -oj` output with one word per segment.
pub fn parse_heard(json: &str) -> Result<Vec<Heard>, String> {
    let parsed: WhisperJson =
        serde_json::from_str(json).map_err(|e| format!("unreadable whisper-cli output: {e}"))?;
    Ok(parsed
        .transcription
        .into_iter()
        .filter(|segment| !segment.text.trim().is_empty())
        .map(|segment| Heard {
            text: segment.text.trim().to_string(),
            at: segment.offsets.from as f64 / 1000.0,
        })
        .collect())
}

// ── Placing ─────────────────────────────────────────────────────────────────────

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "of", "to", "in", "on", "at", "for", "with", "by",
    "from", "as", "is", "are", "was", "were", "be", "been", "being", "it", "its", "this", "that",
    "these", "those", "i", "you", "he", "she", "we", "they", "me", "him", "her", "us", "them",
    "my", "your", "his", "our", "their", "not", "no", "so", "do", "does", "did", "have", "has",
    "had", "will", "would", "can", "could", "shall", "should", "may", "might", "must", "there",
    "here", "what", "which", "who", "whom", "whose", "when", "where", "why", "how", "all", "any",
    "each", "also", "just", "than", "then", "very", "into", "out", "up", "down", "over", "about",
];

/// A word reduced to what two translations of the same speech share: lower case, letters and
/// digits, no possessive, no common suffix. `None` for stop words and punctuation.
fn key(word: &str) -> Option<String> {
    let lower = word.to_lowercase().replace('’', "'").replace("'s", "");
    let bare: String = lower.chars().filter(|c| c.is_alphanumeric()).collect();
    if bare.is_empty() || STOP_WORDS.contains(&bare.as_str()) {
        return None;
    }
    for suffix in ["ing", "ed", "es", "s"] {
        if bare.len() > suffix.len() + 3 && bare.ends_with(suffix) {
            return Some(bare[..bare.len() - suffix.len()].to_string());
        }
    }
    Some(bare)
}

fn similarity(a: &str, b: &str) -> f64 {
    if a == b {
        2.0
    } else if a.len() >= 5 && b.len() >= 5 && a.get(..5) == b.get(..5) {
        1.0
    } else {
        0.0
    }
}

/// One word of the transcript, with when its own clock says it was spoken.
#[derive(Debug, Clone)]
struct Token {
    text: String,
    estimate: f64,
}

fn tokens(cues: &[Cue]) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    for cue in cues {
        let words: Vec<&str> = cue.text.split_whitespace().collect();
        let (start, end) = (cue.start_ms as f64 / 1000.0, cue.end_ms as f64 / 1000.0);
        for (index, word) in words.iter().enumerate() {
            let estimate = start + (end - start) * index as f64 / words.len() as f64;
            // The live translation sends a closing quote as a line of its own, or glued to the
            // next sentence: it belongs to the sentence it closes.
            let quote = |c: char| matches!(c, '"' | '“' | '”');
            if word.chars().all(quote) {
                if let Some(last) = out.last_mut() {
                    last.text.push('"');
                }
                continue;
            }
            let mut text = (*word).to_string();
            if text.starts_with(quote) && text.chars().nth(1).is_some_and(char::is_uppercase) {
                let open_quotes = out
                    .iter()
                    .map(|t| t.text.matches('"').count())
                    .sum::<usize>();
                let after_end = out
                    .last()
                    .is_some_and(|last| last.text.ends_with(['.', '?', '!', ',', ';', ':']));
                if open_quotes % 2 == 1 && after_end {
                    if let Some(last) = out.last_mut() {
                        last.text.push('"');
                    }
                    text = text.chars().skip(1).collect();
                }
            }
            out.push(Token { text, estimate });
        }
    }
    out
}

/// The offset that most rare words (five letters or more) agree on: `heard − transcript`,
/// in whole seconds, smoothed over neighbouring seconds.
fn whole_file_offset(tokens: &[Token], heard: &[Heard]) -> Option<f64> {
    let mut heard_at: HashMap<String, Vec<f64>> = HashMap::new();
    for word in heard {
        if let Some(k) = key(&word.text).filter(|k| k.len() >= 5) {
            heard_at.entry(k).or_default().push(word.at);
        }
    }
    let mut votes: HashMap<i64, u32> = HashMap::new();
    for token in tokens {
        let Some(times) = key(&token.text).and_then(|k| heard_at.get(&k)) else {
            continue;
        };
        for at in times {
            *votes
                .entry((at - token.estimate).round() as i64)
                .or_default() += 1;
        }
    }
    let smoothed = |bin: i64| {
        (-1..=1)
            .map(|d| votes.get(&(bin + d)).copied().unwrap_or(0))
            .sum()
    };
    let (best, count) = votes
        .keys()
        .map(|&bin| (bin, smoothed(bin)))
        .max_by_key(|&(bin, count): &(i64, u32)| (count, -bin.abs()))?;
    (count >= MIN_OFFSET_VOTES).then_some(best as f64)
}

/// Matches transcript words (`(key, estimate)`) to heard words in order, each within
/// [`BAND_SECONDS`] of its estimate, for the most similarity in total. Returns, per transcript
/// word, the index of its heard word.
fn match_words(words: &[(String, f64)], heard: &[(String, f64)]) -> Vec<Option<usize>> {
    let times: Vec<f64> = heard.iter().map(|(_, at)| *at).collect();
    let m = heard.len();
    // best[j]: the most similarity of a match whose last heard word comes before j.
    let mut best = vec![0.0_f64; m + 1];
    let mut rows: Vec<Vec<(usize, f64)>> = Vec::with_capacity(words.len());
    for (word, estimate) in words {
        let from = times.partition_point(|&t| t < estimate - BAND_SECONDS);
        let to = times.partition_point(|&t| t <= estimate + BAND_SECONDS);
        let row: Vec<(usize, f64)> = (from..to)
            .filter_map(|j| {
                let s = similarity(word, &heard[j].0);
                (s > 0.0).then(|| (j, best[j] + s))
            })
            .collect();
        let mut running = 0.0_f64;
        let mut next = row.iter().peekable();
        for (j, best_before) in best.iter_mut().enumerate() {
            while let Some(&&(at, score)) = next.peek() {
                if at < j {
                    running = running.max(score);
                    next.next();
                } else {
                    break;
                }
            }
            *best_before = best_before.max(running);
        }
        rows.push(row);
    }
    let mut matches = vec![None; words.len()];
    let mut target = best[m];
    let mut limit = m;
    for i in (0..words.len()).rev() {
        let found = rows[i]
            .iter()
            .filter(|&&(j, score)| j < limit && (score - target).abs() < 1e-9)
            .max_by_key(|&&(j, _)| j);
        if let Some(&(j, _)) = found {
            matches[i] = Some(j);
            target -= similarity(&words[i].0, &heard[j].0);
            limit = j;
        }
    }
    matches
}

/// Token index ranges of whole sentences: a sentence ends on `.`, `?` or `!` (a closing quote
/// may follow) when the next word starts with a capital or a quote.
fn sentences(tokens: &[Token]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, token) in tokens.iter().enumerate() {
        let text = token.text.trim_end_matches(['"', '”']);
        if text.ends_with(['.', '?', '!']) {
            let next = tokens.get(i + 1).and_then(|t| t.text.chars().next());
            if next.is_none_or(|c| c.is_uppercase() || matches!(c, '"' | '“')) {
                out.push(start..i + 1);
                start = i + 1;
            }
        }
    }
    if start < tokens.len() {
        out.push(start..tokens.len());
    }
    out
}

/// A sentence cut into captions of at most two lines, at a clause boundary where one fits.
fn captions_of(tokens: &[Token], range: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
    let length = |r: &std::ops::Range<usize>| {
        tokens[r.clone()]
            .iter()
            .map(|t| t.text.chars().count() + 1)
            .sum::<usize>()
    };
    if length(&range) <= 2 * LINE_CHARS + 1 || range.len() < 2 {
        return vec![range];
    }
    let mut cut = None;
    for at in range.start + 1..range.end {
        if length(&(range.start..at)) > 2 * LINE_CHARS + 1 {
            break;
        }
        let clause = tokens[at - 1]
            .text
            .trim_end_matches(['"', '”'])
            .ends_with([',', ';', ':']);
        let score = if clause { 2.0 } else { 0.0 } + (at - range.start) as f64 / range.len() as f64;
        if cut.is_none_or(|(best, _)| score >= best) {
            cut = Some((score, at));
        }
    }
    let at = cut
        .map_or(range.start + range.len() / 2, |(_, at)| at)
        .max(range.start + 1);
    let mut out = captions_of(tokens, range.start..at);
    out.extend(captions_of(tokens, at..range.end));
    out
}

/// Two lines at the space nearest the middle, when one line won't hold it.
fn lines(text: &str) -> String {
    if text.chars().count() <= LINE_CHARS {
        return text.to_string();
    }
    let middle = text.len() / 2;
    match text
        .match_indices(' ')
        .map(|(at, _)| at)
        .min_by_key(|&at| at.abs_diff(middle))
    {
        Some(at) => format!("{}\n{}", &text[..at], &text[at + 1..]),
        None => text.to_string(),
    }
}

/// `cues` rebuilt into sentence captions, each starting when its first word is spoken in
/// `heard`, within a program `length` seconds long. `None` when too little of the transcript
/// matches the speech to trust it (another service's transcript, or another language).
pub fn place_on_speech(cues: &[Cue], heard: &[Heard], length: f64) -> Option<Placed> {
    let all = tokens(cues);
    let offset = whole_file_offset(&all, heard)?;
    let tokens: Vec<Token> = all
        .into_iter()
        .map(|t| Token {
            estimate: t.estimate + offset,
            ..t
        })
        .filter(|t| (-1.0..=length + 1.0).contains(&t.estimate))
        .collect();
    let content: Vec<(usize, String, f64)> = tokens
        .iter()
        .enumerate()
        .filter_map(|(i, t)| key(&t.text).map(|k| (i, k, t.estimate)))
        .collect();
    let reference: Vec<(String, f64)> = heard
        .iter()
        .filter_map(|w| key(&w.text).map(|k| (k, w.at)))
        .collect();
    let words: Vec<(String, f64)> = content.iter().map(|(_, k, at)| (k.clone(), *at)).collect();
    let matches = match_words(&words, &reference);
    let matched_count = matches.iter().flatten().count();
    if content.is_empty() || (matched_count as f64) < MIN_MATCHED_SHARE * content.len() as f64 {
        return None;
    }

    // Every word's time: its heard word's, or spread between the matched words around it.
    let mut times: Vec<Option<f64>> = vec![None; tokens.len()];
    for ((token, _, _), heard_index) in content.iter().zip(&matches) {
        if let Some(j) = heard_index {
            times[*token] = Some(reference[*j].1);
        }
    }
    let anchored: Vec<bool> = times.iter().map(Option::is_some).collect();
    let known: Vec<usize> = (0..tokens.len()).filter(|&i| anchored[i]).collect();
    let times: Vec<f64> = (0..tokens.len())
        .map(|i| {
            if let Some(at) = times[i] {
                return at;
            }
            let after = known.partition_point(|&k| k < i);
            match (after.checked_sub(1).map(|b| known[b]), known.get(after)) {
                (Some(a), Some(&b)) => {
                    let (ta, tb) = (times[a].unwrap_or(0.0), times[b].unwrap_or(0.0));
                    ta + (tb - ta) * (i - a) as f64 / (b - a) as f64
                }
                (Some(a), None) => times[a].unwrap_or(0.0) + SECONDS_PER_WORD * (i - a) as f64,
                (None, Some(&b)) => times[b].unwrap_or(0.0) - SECONDS_PER_WORD * (b - i) as f64,
                (None, None) => tokens[i].estimate,
            }
        })
        .collect();

    let first_heard = heard.first().map_or(0.0, |word| word.at);
    let mut captions: Vec<(f64, String, bool)> = sentences(&tokens)
        .into_iter()
        .flat_map(|sentence| captions_of(&tokens, sentence))
        .map(|range| {
            let text = tokens[range.clone()]
                .iter()
                .map(|t| t.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            // From its first matched word: the words before it (often "And he", never matched)
            // are only a moment long, while spreading them would reach back a whole pause.
            let start = match range.clone().find(|&i| anchored[i]) {
                Some(first) => times[first] - SECONDS_PER_WORD * (first - range.start) as f64,
                None => times[range.start],
            };
            (start, text, anchored[range].iter().any(|&a| a))
        })
        .filter(|(start, text, _)| {
            (0.0..length - 0.5).contains(start) && text.chars().any(char::is_alphanumeric)
        })
        // Nothing is said before the first word Whisper heard (the picture often starts before
        // the recording), so a caption reaching back past it starts there.
        .map(|(start, text, matched)| (start.max(first_heard), text, matched))
        .collect();
    captions.sort_by(|a, b| a.0.total_cmp(&b.0));

    let to_ms = |seconds: f64| (seconds.max(0.0) * 1000.0).round() as u64;
    let placed = captions
        .iter()
        .enumerate()
        .map(|(i, (start, text, _))| {
            let next = captions.get(i + 1).map_or(length, |c| c.0);
            let end = next
                .min(start + MAX_CAPTION_SECONDS)
                .min(length)
                .max((start + MIN_CAPTION_SECONDS).min(next).min(length));
            Cue {
                start_ms: to_ms(*start),
                end_ms: to_ms(end),
                text: lines(text),
            }
        })
        .collect();
    Some(Placed {
        cues: placed,
        offset_seconds: offset,
        matched: u32::try_from(captions.iter().filter(|c| c.2).count()).unwrap_or(u32::MAX),
    })
}

/// Whisper's output for a test: one segment per word, `(word, ms)`.
#[cfg(test)]
fn whisper_json(words: &[(&str, u64)]) -> String {
    let segments: Vec<serde_json::Value> = words
        .iter()
        .map(|(text, from)| {
            serde_json::json!({ "offsets": { "from": from, "to": from + 300 }, "text": format!(" {text}") })
        })
        .collect();
    serde_json::json!({ "transcription": segments }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start: f64, end: f64, text: &str) -> Cue {
        Cue {
            start_ms: (start * 1000.0) as u64,
            end_ms: (end * 1000.0) as u64,
            text: text.to_string(),
        }
    }

    /// The live translation of Luke 10, as it arrived: fragments, lone quotes, 2–4 s late, on
    /// a clock 1900 s off the program's (a transcript downloaded from the session's start).
    fn live() -> Vec<Cue> {
        let late = 1900.0;
        vec![
            cue(
                late + 3.0,
                late + 7.0,
                "accompanying Jesus, and he asked this: \"Teacher, what must I do to inherit",
            ),
            cue(late + 7.0, late + 8.0, "eternal life?"),
            cue(late + 8.0, late + 9.5, "\""),
            cue(
                late + 12.0,
                late + 15.0,
                "And he said to him, \"What is written in the Law?",
            ),
            cue(late + 15.0, late + 16.0, "How do you read it?"),
            cue(late + 16.0, late + 17.0, "\""),
            cue(
                late + 21.0,
                late + 26.0,
                "Jesus answered, \"A Samaritan traveller came near Jericho",
            ),
            cue(
                late + 26.0,
                late + 30.0,
                "and when he saw the robbers' victim, he felt compassion.",
            ),
        ]
    }

    /// What Whisper heard: different wording, on the program's clock, earlier than the live text.
    fn heard() -> Vec<Heard> {
        let words = [
            ("He", 0.0),
            ("asked", 0.4),
            ("Jesus,", 0.8),
            ("Master,", 1.3),
            ("what", 1.7),
            ("should", 1.9),
            ("I", 2.1),
            ("do", 2.2),
            ("to", 2.3),
            ("inherit", 2.5),
            ("eternal", 2.9),
            ("life?", 3.3),
            ("And", 9.0),
            ("he", 9.2),
            ("said", 9.4),
            ("What", 9.8),
            ("is", 10.0),
            ("written", 10.2),
            ("in", 10.5),
            ("the", 10.6),
            ("Law?", 10.8),
            ("How", 11.6),
            ("do", 11.8),
            ("you", 12.0),
            ("read", 12.2),
            ("it?", 12.4),
            ("Jesus", 17.0),
            ("answered,", 17.4),
            ("A", 18.0),
            ("Samaritan", 18.2),
            ("traveller", 18.8),
            ("came", 19.3),
            ("near", 19.6),
            ("Jericho,", 20.0),
            ("and", 21.0),
            ("seeing", 21.3),
            ("the", 21.6),
            ("robbers'", 21.8),
            ("victim,", 22.3),
            ("he", 22.8),
            ("felt", 23.0),
            ("compassion.", 23.3),
        ];
        let json = whisper_json(&words.map(|(w, s)| (w, (s * 1000.0) as u64)));
        parse_heard(&json).unwrap()
    }

    fn with_rare_words(mut heard: Vec<Heard>, mut cues: Vec<Cue>) -> (Vec<Heard>, Vec<Cue>) {
        // Enough rare words for the whole-file offset to have votes: a longer passage.
        for n in 0..14 {
            let at = 40.0 + n as f64 * 3.0;
            heard.push(Heard {
                text: format!("neighbour{n}"),
                at,
            });
            cues.push(cue(
                1900.0 + at + 3.0,
                1900.0 + at + 5.0,
                &format!("neighbour{n}."),
            ));
        }
        (heard, cues)
    }

    #[test]
    fn the_live_translation_becomes_sentences_on_the_speech() {
        let (heard, cues) = with_rare_words(heard(), live());
        let placed = place_on_speech(&cues, &heard, 120.0).unwrap();
        assert!(
            (placed.offset_seconds + 1903.0).abs() <= 1.0,
            "{}",
            placed.offset_seconds
        );
        let texts: Vec<&str> = placed.cues.iter().map(|c| c.text.as_str()).collect();
        assert!(
            texts.iter().all(|t| t.chars().any(char::is_alphanumeric)),
            "no lone quotes"
        );
        let law = placed
            .cues
            .iter()
            .find(|c| c.text.starts_with("And he said to him"))
            .unwrap();
        assert!(
            law.start_ms.abs_diff(9_000) <= 500,
            "starts with the speech (\"And\" at 9 s): {law:?}"
        );
        let law_text = law.text.replace('\n', " ");
        assert!(law_text.contains("Law?") && !law_text.contains("How do you"));
        assert!(placed.cues[0].start_ms <= 1_000, "{:?}", placed.cues[0]);
        let life = placed
            .cues
            .iter()
            .find(|c| c.text.replace('\n', " ").contains("eternal life?"))
            .unwrap();
        assert!(
            life.text.ends_with("life?\""),
            "the closing quote joins it: {life:?}"
        );
        let read = placed
            .cues
            .iter()
            .find(|c| c.text.starts_with("How do you"))
            .unwrap();
        assert!(read.text.ends_with("read it?\""), "{read:?}");
        assert!(placed.matched >= 4);
    }

    #[test]
    fn a_transcript_of_other_speech_is_not_placed() {
        let (heard, _) = with_rare_words(heard(), live());
        let other = vec![cue(
            0.0,
            5.0,
            "Completely different words about weather tomorrow.",
        )];
        assert_eq!(place_on_speech(&other, &heard, 120.0), None);
    }

    #[test]
    fn long_sentences_split_at_a_clause_into_two_line_captions() {
        let text = "In reply Jesus said, a man was going down from Jerusalem to Jericho, and fell into the hands of robbers, who stripped him and beat him.";
        let tokens = tokens(&[cue(0.0, 10.0, text)]);
        let parts = captions_of(&tokens, 0..tokens.len());
        assert!(parts.len() >= 2);
        for part in &parts {
            let caption: Vec<&str> = tokens[part.clone()]
                .iter()
                .map(|t| t.text.as_str())
                .collect();
            let caption = caption.join(" ");
            assert!(caption.chars().count() <= 2 * LINE_CHARS + 1, "{caption}");
            assert!(
                lines(&caption)
                    .lines()
                    .all(|l| l.chars().count() <= LINE_CHARS + 8)
            );
        }
        let first = tokens[parts[0].clone()].last().unwrap();
        assert!(
            first.text.ends_with(','),
            "cut after a comma: {}",
            first.text
        );
    }

    #[test]
    fn whisper_progress_lines_become_fractions() {
        let line = "whisper_print_progress_callback: progress =  45%";
        assert_eq!(progress_of(line), Some(0.45));
        assert_eq!(
            progress_of("whisper_print_progress_callback: progress = 100%"),
            Some(1.0)
        );
        assert_eq!(progress_of("whisper_init_from_file: loading model"), None);
    }

    #[test]
    fn words_are_keyed_by_what_translations_share() {
        assert_eq!(key("Robbers'").as_deref(), Some("robber"));
        assert_eq!(key("Jesus,").as_deref(), Some("jesu"));
        assert_eq!(key("\"The"), None);
        assert_eq!(key("\""), None);
        assert_eq!(key("God’s").as_deref(), Some("god"));
    }
}

#[cfg(test)]
mod real {
    use super::*;
    use metocast_transcription::captions::{parse_cues, render_srt};

    /// The 2026-10-18 program against Whisper's words from it:
    ///
    /// ```sh
    /// METOCAST_TEST_VTT="/Volumes/A002/2026-10-18 10-35-32.en.vtt" \
    /// METOCAST_TEST_HEARD=".../whisper-test/words.json" METOCAST_TEST_LENGTH=1185.6 \
    /// METOCAST_TEST_SRT=/tmp/placed.srt \
    /// cargo test -p metocast-server --lib real_transcript_is_placed -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a live transcript and Whisper's words for its program"]
    fn real_transcript_is_placed() {
        let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let cues = parse_cues(&std::fs::read_to_string(env("METOCAST_TEST_VTT")).unwrap()).unwrap();
        let heard =
            parse_heard(&std::fs::read_to_string(env("METOCAST_TEST_HEARD")).unwrap()).unwrap();
        let length: f64 = env("METOCAST_TEST_LENGTH").parse().unwrap();
        let placed = place_on_speech(&cues, &heard, length).expect("placed");
        println!(
            "offset {:+.1} s, {} captions, {} matched",
            placed.offset_seconds,
            placed.cues.len(),
            placed.matched
        );
        // The same transcript as the library downloads it, counting from a session that began
        // 31 minutes before the recording: the whole-file offset must find it all the same.
        let session: Vec<Cue> = cues
            .iter()
            .map(|c| Cue {
                start_ms: c.start_ms + 1_860_000,
                end_ms: c.end_ms + 1_860_000,
                text: c.text.clone(),
            })
            .collect();
        let again = place_on_speech(&session, &heard, length).expect("placed from the session");
        println!("from the session: offset {:+.1} s", again.offset_seconds);
        assert_eq!(again.cues, placed.cues);
        std::fs::write(env("METOCAST_TEST_SRT"), render_srt(&placed.cues)).unwrap();
    }
}
