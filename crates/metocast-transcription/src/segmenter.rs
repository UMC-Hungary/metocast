//! Turns a stream of translated text deltas into a live "partial" line plus committed segments.
//!
//! The speaker never stops for us, so text is cut while speech continues:
//! 1. at a sentence end (`.`, `?`, `!`, `…`) once the following whitespace has arrived, so
//!    "3.5" or "St. Paul" aren't split;
//! 2. after a pause: no new text for `gap_ms` of audio;
//! 3. when the open text exceeds `max_chars`: at the last clause break (`,` `;` `:` `–` `—`)
//!    past `min_cut_chars`, else the last space. The remainder starts the next segment.
//!
//! Times are audio positions (ms) at which the deltas arrived. They include the model's lag;
//! callers shift them when rendering captions.

use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct SegmenterConfig {
    pub gap_ms: u64,
    pub max_chars: usize,
    pub min_cut_chars: usize,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            gap_ms: 1_200,
            max_chars: 200,
            min_cut_chars: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub seq: u32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentEvent {
    /// The open, still-growing text. Empty means "clear the partial line".
    Partial(String),
    Committed(Segment),
}

const SENTENCE_ENDS: [char; 4] = ['.', '?', '!', '…'];
const CLAUSE_BREAKS: [char; 5] = [',', ';', ':', '–', '—'];
const CLOSERS: [char; 4] = ['"', '\'', ')', '”'];
const ABBREVIATIONS: [&str; 12] = [
    "mr", "mrs", "ms", "dr", "st", "jr", "sr", "vs", "e.g", "i.e", "cf", "rev",
];

pub struct Segmenter {
    config: SegmenterConfig,
    buffer: String,
    first_at: u64,
    last_at: u64,
    next_seq: u32,
}

impl Segmenter {
    #[must_use]
    pub fn new(config: SegmenterConfig) -> Self {
        Self {
            config,
            buffer: String::new(),
            first_at: 0,
            last_at: 0,
            next_seq: 1,
        }
    }

    /// Adds a text delta that arrived at audio position `at_ms`.
    pub fn push(&mut self, at_ms: u64, delta: &str) -> Vec<SegmentEvent> {
        if delta.is_empty() {
            return Vec::new();
        }
        if self.buffer.trim().is_empty() {
            self.buffer.clear();
            self.first_at = at_ms;
        }
        // A new sentence glued to the previous one ("Amen." + "Grace") still ends it.
        let ends_sentence = self.buffer.ends_with(SENTENCE_ENDS);
        if ends_sentence && delta.starts_with(char::is_uppercase) {
            self.buffer.push(' ');
        }
        self.buffer.push_str(delta);
        self.last_at = at_ms;

        let mut events = Vec::new();
        loop {
            let cut = sentence_cut(&self.buffer).or_else(|| {
                (self.buffer.chars().count() > self.config.max_chars).then(|| {
                    length_cut(
                        &self.buffer,
                        self.config.max_chars,
                        self.config.min_cut_chars,
                    )
                })
            });
            let Some(cut) = cut else { break };
            if let Some(segment) = self.commit(cut, at_ms) {
                events.push(SegmentEvent::Committed(segment));
            }
        }
        events.push(SegmentEvent::Partial(self.buffer.trim().to_string()));
        events
    }

    /// No text is open, so the stream can be cut here without splitting a line.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.buffer.trim().is_empty()
    }

    /// Advances the audio clock; commits the open text after a pause.
    pub fn tick(&mut self, now_ms: u64) -> Vec<SegmentEvent> {
        if self.buffer.trim().is_empty() || now_ms < self.last_at + self.config.gap_ms {
            return Vec::new();
        }
        self.finish(self.last_at)
    }

    /// Commits whatever is open (end of stream).
    pub fn finish(&mut self, now_ms: u64) -> Vec<SegmentEvent> {
        let mut events = Vec::new();
        let end = self.buffer.len();
        if let Some(segment) = self.commit(end, now_ms.max(self.last_at)) {
            events.push(SegmentEvent::Committed(segment));
            events.push(SegmentEvent::Partial(String::new()));
        }
        events
    }

    fn commit(&mut self, cut: usize, at_ms: u64) -> Option<Segment> {
        let rest = self.buffer.split_off(cut);
        let text = std::mem::replace(&mut self.buffer, rest.trim_start().to_string());
        let start_ms = self.first_at;
        self.first_at = at_ms;
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let segment = Segment {
            seq: self.next_seq,
            start_ms,
            end_ms: at_ms.max(start_ms + 1),
            text: text.to_string(),
        };
        self.next_seq += 1;
        Some(segment)
    }
}

/// Byte index just after the first complete sentence, if its trailing whitespace has arrived.
fn sentence_cut(text: &str) -> Option<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for (i, &(byte, c)) in chars.iter().enumerate() {
        if !SENTENCE_ENDS.contains(&c) {
            continue;
        }
        let mut end = i + 1;
        while end < chars.len()
            && (SENTENCE_ENDS.contains(&chars[end].1) || CLOSERS.contains(&chars[end].1))
        {
            end += 1;
        }
        let Some(&(next_byte, next)) = chars.get(end) else {
            return None; // wait for what follows
        };
        if !next.is_whitespace() || (c == '.' && is_abbreviation(&text[..byte])) {
            continue;
        }
        return Some(next_byte);
    }
    None
}

fn is_abbreviation(before_dot: &str) -> bool {
    let word = before_dot
        .rsplit(|c: char| c.is_whitespace() || c == '(' || c == '"')
        .next()
        .unwrap_or_default();
    let lower = word.to_lowercase();
    (word.chars().count() == 1 && word.chars().all(char::is_uppercase))
        || ABBREVIATIONS.contains(&lower.as_str())
}

/// Byte index to cut an over-long text at, preferring clause breaks over plain spaces.
fn length_cut(text: &str, max_chars: usize, min_cut_chars: usize) -> usize {
    let limit = text
        .char_indices()
        .nth(max_chars)
        .map_or(text.len(), |(b, _)| b);
    let min_byte = text.char_indices().nth(min_cut_chars).map_or(0, |(b, _)| b);
    let window = &text[..limit];
    let after_clause = window
        .char_indices()
        .rev()
        .filter(|&(b, c)| b >= min_byte && CLAUSE_BREAKS.contains(&c))
        .map(|(b, c)| b + c.len_utf8())
        .find(|&b| text[b..].starts_with(char::is_whitespace));
    if let Some(cut) = after_clause {
        return cut;
    }
    window
        .char_indices()
        .rev()
        .find(|&(b, c)| b > 0 && c.is_whitespace())
        .map_or(limit, |(b, _)| b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn committed(events: &[SegmentEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                SegmentEvent::Committed(s) => Some(s.text.clone()),
                SegmentEvent::Partial(_) => None,
            })
            .collect()
    }

    #[test]
    fn commits_sentences_once_following_space_arrives() {
        let mut s = Segmenter::new(SegmenterConfig::default());
        assert!(committed(&s.push(1_000, "Grace to you")).is_empty());
        assert!(committed(&s.push(1_500, " and peace.")).is_empty());
        let events = s.push(2_000, " Let us pray");
        assert_eq!(committed(&events), vec!["Grace to you and peace."]);
        assert_eq!(
            events.last(),
            Some(&SegmentEvent::Partial("Let us pray".into()))
        );
        let SegmentEvent::Committed(first) = &events[0] else {
            unreachable!()
        };
        assert_eq!((first.start_ms, first.end_ms), (1_000, 2_000));
    }

    #[test]
    fn does_not_split_numbers_or_abbreviations() {
        let mut s = Segmenter::new(SegmenterConfig::default());
        let events = s.push(0, "Verse 3.5 says St. Paul wrote to Mr. Smith. Then");
        assert_eq!(
            committed(&events),
            vec!["Verse 3.5 says St. Paul wrote to Mr. Smith."]
        );
    }

    #[test]
    fn sentence_glued_to_the_next_still_commits() {
        let mut s = Segmenter::new(SegmenterConfig::default());
        s.push(0, "Amen.");
        let events = s.push(100, "Grace to you");
        assert_eq!(committed(&events), vec!["Amen."]);
        assert_eq!(
            events.last(),
            Some(&SegmentEvent::Partial("Grace to you".into()))
        );
        s.push(200, " in verse 3.");
        assert!(committed(&s.push(300, "5 today")).is_empty());
    }

    #[test]
    fn pause_commits_open_text() {
        let mut s = Segmenter::new(SegmenterConfig::default());
        s.push(1_000, "and so we wait");
        assert!(s.tick(2_000).is_empty());
        assert_eq!(committed(&s.tick(2_300)), vec!["and so we wait"]);
        assert!(s.tick(9_000).is_empty());
    }

    #[test]
    fn long_text_cuts_at_clause_then_space() {
        let config = SegmenterConfig {
            gap_ms: 1_200,
            max_chars: 40,
            min_cut_chars: 10,
        };
        let mut s = Segmenter::new(config);
        let events = s.push(
            0,
            "when the Lord came down, he spoke to the people gathered there",
        );
        assert_eq!(committed(&events), vec!["when the Lord came down,"]);
        let events = s.push(100, " and they listened to every single word he said");
        assert_eq!(
            committed(&events),
            vec![
                "he spoke to the people gathered there",
                "and they listened to every single word"
            ]
        );
        assert_eq!(
            events.last(),
            Some(&SegmentEvent::Partial("he said".into()))
        );
    }
}
