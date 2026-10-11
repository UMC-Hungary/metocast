//! WebVTT / SRT rendering of committed segments.

use crate::segmenter::Segment;

#[derive(Debug, Clone, Copy)]
pub struct CaptionOptions {
    /// Subtracted from every segment time (compensates the model's lag).
    pub shift_ms: i64,
    pub max_line_chars: usize,
    pub max_lines: usize,
    pub min_cue_ms: u64,
    /// Gaps shorter than this are closed by extending the earlier cue, to avoid flicker.
    pub bridge_gap_ms: u64,
}

impl Default for CaptionOptions {
    fn default() -> Self {
        Self {
            shift_ms: 0,
            max_line_chars: 42,
            max_lines: 2,
            min_cue_ms: 1_200,
            bridge_gap_ms: 1_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[must_use]
pub fn build_cues(segments: &[Segment], options: &CaptionOptions) -> Vec<Cue> {
    let mut cues = Vec::new();
    for segment in segments {
        let start = shifted(segment.start_ms, options.shift_ms);
        let end = shifted(segment.end_ms, options.shift_ms).max(start + options.min_cue_ms);
        let lines = wrap(&segment.text, options.max_line_chars);
        let groups: Vec<String> = lines
            .chunks(options.max_lines.max(1))
            .map(|g| g.join("\n"))
            .collect();
        let total_chars: usize = groups.iter().map(|g| g.chars().count().max(1)).sum();
        let mut cursor = start;
        for (i, group) in groups.iter().enumerate() {
            let share = (end - start) * group.chars().count().max(1) as u64 / total_chars as u64;
            let group_end = if i + 1 == groups.len() {
                end
            } else {
                cursor + share
            };
            cues.push(Cue {
                start_ms: cursor,
                end_ms: group_end,
                text: group.clone(),
            });
            cursor = group_end;
        }
    }
    // No overlaps; bridge short gaps.
    for i in 1..cues.len() {
        let next_start = cues[i].start_ms.max(cues[i - 1].start_ms + 1);
        cues[i].start_ms = next_start;
        cues[i].end_ms = cues[i].end_ms.max(next_start + 1);
        let previous = &mut cues[i - 1];
        if previous.end_ms > next_start || next_start - previous.end_ms < options.bridge_gap_ms {
            previous.end_ms = next_start;
        }
    }
    cues
}

#[must_use]
pub fn render_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            timestamp(cue.start_ms, '.'),
            timestamp(cue.end_ms, '.'),
            cue.text
        ));
    }
    out
}

#[must_use]
pub fn render_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, cue) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            timestamp(cue.start_ms, ','),
            timestamp(cue.end_ms, ','),
            cue.text
        ));
    }
    out
}

/// Cues of an SRT or WebVTT file: each block with a `-->` line, its text the lines after it.
/// Headers, notes, cue numbers and cue settings are skipped.
pub fn parse_cues(text: &str) -> Result<Vec<Cue>, String> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let mut lines = block.lines().skip_while(|line| !line.contains("-->"));
        let Some(timing) = lines.next() else {
            continue;
        };
        let (start, end) = timing
            .split_once("-->")
            .ok_or_else(|| format!("unreadable cue timing: {timing}"))?;
        let end = end.split_whitespace().next().unwrap_or_default();
        let text = lines.collect::<Vec<_>>().join("\n");
        cues.push(Cue {
            start_ms: parse_timestamp(start.trim())?,
            end_ms: parse_timestamp(end)?,
            text: text.trim().to_string(),
        });
    }
    if cues.is_empty() {
        return Err("no cues found: is it an .srt or .vtt file?".to_string());
    }
    Ok(cues)
}

/// `01:02:03,456`, `01:02:03.456` or WebVTT's short `02:03.456`.
fn parse_timestamp(text: &str) -> Result<u64, String> {
    let invalid = || format!("unreadable timestamp: {text}");
    let (clock, millis) = text.split_once([',', '.']).ok_or_else(invalid)?;
    let mut ms: u64 = millis.parse().map_err(|_| invalid())?;
    for (part, unit) in clock.rsplit(':').zip([1_000, 60_000, 3_600_000]) {
        ms += part.parse::<u64>().map_err(|_| invalid())? * unit;
    }
    Ok(ms)
}

fn shifted(ms: u64, shift_ms: i64) -> u64 {
    (ms as i64 - shift_ms).max(0) as u64
}

fn timestamp(ms: u64, separator: char) -> String {
    format!(
        "{:02}:{:02}:{:02}{separator}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000
    )
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_and_vtt_read_back_what_they_render() {
        let cues = vec![
            Cue {
                start_ms: 1_200,
                end_ms: 4_000,
                text: "Kedves testvérek,\nmai igénk".into(),
            },
            Cue {
                start_ms: 3_725_042,
                end_ms: 3_727_000,
                text: "Ámen.".into(),
            },
        ];
        assert_eq!(parse_cues(&render_srt(&cues)).unwrap(), cues);
        assert_eq!(parse_cues(&render_vtt(&cues)).unwrap(), cues);
        let short =
            "WEBVTT\r\n\r\nNOTE made by hand\r\n\r\n00:01.500 --> 00:03.000 align:start\r\nHi\r\n";
        assert_eq!(parse_cues(short).unwrap()[0].start_ms, 1_500);
        assert!(parse_cues("just text").is_err());
    }

    fn segment(seq: u32, start_ms: u64, end_ms: u64, text: &str) -> Segment {
        Segment {
            seq,
            start_ms,
            end_ms,
            text: text.into(),
        }
    }

    #[test]
    fn renders_vtt_and_srt_timestamps() {
        let cues = build_cues(
            &[segment(1, 3_723_456, 3_726_000, "Amen.")],
            &CaptionOptions::default(),
        );
        assert_eq!(
            render_vtt(&cues),
            "WEBVTT\n\n01:02:03.456 --> 01:02:06.000\nAmen.\n\n"
        );
        assert!(render_srt(&cues).starts_with("1\n01:02:03,456 --> 01:02:06,000\nAmen."));
    }

    #[test]
    fn shift_clamps_at_zero_and_long_text_splits_into_two_line_cues() {
        let options = CaptionOptions {
            shift_ms: 2_000,
            ..CaptionOptions::default()
        };
        let text = "In the beginning was the Word, and the Word was with God, and the Word was God. \
                    The same was in the beginning with God.";
        let cues = build_cues(&[segment(1, 1_000, 13_000, text)], &options);
        assert_eq!(cues[0].start_ms, 0);
        assert!(cues.len() >= 2);
        for cue in &cues {
            assert!(cue.text.lines().count() <= 2);
            assert!(cue.text.lines().all(|l| l.chars().count() <= 42));
        }
        assert_eq!(cues.last().map(|c| c.end_ms), Some(11_000));
    }

    #[test]
    fn removes_overlap_and_bridges_small_gaps() {
        let cues = build_cues(
            &[
                segment(1, 0, 5_000, "first"),
                segment(2, 4_000, 6_000, "second"),
                segment(3, 6_500, 8_000, "third"),
            ],
            &CaptionOptions::default(),
        );
        assert_eq!(cues[0].end_ms, 4_000);
        assert_eq!(cues[1].end_ms, 6_500);
    }
}
