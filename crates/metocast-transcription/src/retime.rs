//! Re-times final-video cues to speech onsets.
//!
//! Live cues trail the speech by a median of ~3.2 s with a −2…+6 s spread, so a fixed shift
//! is not enough: each cue start is moved to the detected speech onset nearest to
//! `first_text − lag_ms`, searched within `[−window_before_ms, +window_after_ms]` of that target.

use crate::segmenter::Segment;

#[derive(Debug, Clone, Copy)]
pub struct RetimeOptions {
    pub lag_ms: u64,
    pub window_before_ms: u64,
    pub window_after_ms: u64,
    pub max_cue_ms: u64,
}

impl Default for RetimeOptions {
    fn default() -> Self {
        Self {
            lag_ms: 3_200,
            window_before_ms: 4_000,
            window_after_ms: 1_000,
            max_cue_ms: 7_000,
        }
    }
}

/// Speech start for text first seen at `first_text_ms`; `onsets_ms` must be sorted ascending.
#[must_use]
pub fn snap_start(first_text_ms: u64, onsets_ms: &[u64], options: &RetimeOptions) -> u64 {
    let target = first_text_ms.saturating_sub(options.lag_ms);
    let from = target.saturating_sub(options.window_before_ms);
    let to = target + options.window_after_ms;
    let first = onsets_ms.partition_point(|&o| o < from);
    onsets_ms[first..]
        .iter()
        .take_while(|&&o| o <= to)
        .min_by_key(|&&o| o.abs_diff(target))
        .copied()
        .unwrap_or(target)
}

/// Returns segments with snapped starts; each ends at the next start or after `max_cue_ms`.
#[must_use]
pub fn retime(segments: &[Segment], onsets_ms: &[u64], options: &RetimeOptions) -> Vec<Segment> {
    let mut out: Vec<Segment> = segments
        .iter()
        .map(|s| Segment {
            start_ms: snap_start(s.start_ms, onsets_ms, options),
            ..s.clone()
        })
        .collect();
    for i in 1..out.len() {
        let floor = out[i - 1].start_ms + 1;
        if out[i].start_ms < floor {
            out[i].start_ms = floor;
        }
    }
    for i in 0..out.len() {
        let cap = out[i].start_ms + options.max_cue_ms;
        out[i].end_ms = out.get(i + 1).map_or(cap, |next| next.start_ms.min(cap));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(seq: u32, start_ms: u64) -> Segment {
        Segment {
            seq,
            start_ms,
            end_ms: start_ms + 2_000,
            text: format!("s{seq}"),
        }
    }

    #[test]
    fn snaps_to_nearest_onset_in_window() {
        let o = RetimeOptions::default();
        // target = 10_000 − 3_200 = 6_800
        assert_eq!(snap_start(10_000, &[1_000, 6_000, 7_500, 9_000], &o), 7_500);
        assert_eq!(snap_start(10_000, &[2_900, 6_500], &o), 6_500);
    }

    #[test]
    fn window_edges_are_inclusive() {
        let o = RetimeOptions::default();
        assert_eq!(snap_start(10_000, &[2_800], &o), 2_800);
        assert_eq!(snap_start(10_000, &[7_800], &o), 7_800);
        assert_eq!(snap_start(10_000, &[2_799, 7_801], &o), 6_800);
    }

    #[test]
    fn falls_back_to_lag_shift_and_clamps_at_zero() {
        let o = RetimeOptions::default();
        assert_eq!(snap_start(10_000, &[], &o), 6_800);
        assert_eq!(snap_start(1_000, &[], &o), 0);
    }

    #[test]
    fn ends_run_to_next_cue_or_cap_and_starts_stay_ordered() {
        let o = RetimeOptions::default();
        let out = retime(
            &[seg(1, 10_000), seg(2, 20_000), seg(3, 40_000)],
            &[6_500],
            &o,
        );
        assert_eq!(out[0].start_ms, 6_500);
        assert_eq!(out[1].start_ms, 16_800);
        assert_eq!(out[0].end_ms, 13_500);
        assert_eq!(out[1].end_ms, 23_800);
        assert_eq!(out[2].end_ms, out[2].start_ms + 7_000);
        let tight = retime(&[seg(1, 10_000), seg(2, 10_100)], &[6_500], &o);
        assert!(tight[1].start_ms > tight[0].start_ms);
    }
}
