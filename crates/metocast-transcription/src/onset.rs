//! A minimal energy-based speech-onset detector, used only to measure how long after speech
//! starts the first translated text arrives.
//!
//! Church recordings rarely reach digital silence (room tone, hiss), so "silence" is relative to
//! a tracked noise floor: the floor follows quiet frames down immediately and creeps up slowly,
//! and speech is anything `ABOVE_FLOOR_DB` louder than it.

const FRAME_MS: u64 = 20;
const ABOVE_FLOOR_DB: f64 = 8.0;
const FLOOR_RISE_DB_PER_FRAME: f64 = 0.05;
const FLOOR_MIN_DB: f64 = -70.0;
const MIN_SILENCE_MS: u64 = 300;

pub struct OnsetDetector {
    frame_len: usize,
    min_silence_frames: u64,
    floor_db: Option<f64>,
    sum_squares: f64,
    filled: usize,
    frame_index: u64,
    silent_frames: u64,
}

impl OnsetDetector {
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let min_silence_frames = MIN_SILENCE_MS / FRAME_MS;
        Self {
            frame_len: (u64::from(sample_rate) * FRAME_MS / 1000) as usize,
            min_silence_frames,
            floor_db: None,
            sum_squares: 0.0,
            filled: 0,
            frame_index: 0,
            silent_frames: min_silence_frames,
        }
    }

    /// Returns the start times (ms) of speech that follows at least 300 ms of quiet.
    pub fn push(&mut self, samples: &[f32]) -> Vec<u64> {
        let mut onsets = Vec::new();
        for sample in samples {
            self.sum_squares += f64::from(*sample) * f64::from(*sample);
            self.filled += 1;
            if self.filled < self.frame_len {
                continue;
            }
            let level_db = (10.0
                * (self.sum_squares / self.frame_len as f64)
                    .max(1e-12)
                    .log10())
            .max(FLOOR_MIN_DB);
            let floor = self
                .floor_db
                .map_or(level_db, |f| (f + FLOOR_RISE_DB_PER_FRAME).min(level_db));
            self.floor_db = Some(floor);
            if level_db >= floor + ABOVE_FLOOR_DB {
                if self.silent_frames >= self.min_silence_frames {
                    onsets.push(self.frame_index * FRAME_MS);
                }
                self.silent_frames = 0;
            } else {
                self.silent_frames += 1;
            }
            self.frame_index += 1;
            self.sum_squares = 0.0;
            self.filled = 0;
        }
        onsets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_speech_after_quiet_only() {
        let rate = 16_000;
        let silence = vec![0.0_f32; rate as usize / 2];
        let loud = vec![0.3_f32; rate as usize / 2];
        let mut detector = OnsetDetector::new(rate);
        let mut onsets = detector.push(&silence);
        onsets.extend(detector.push(&loud));
        onsets.extend(detector.push(&vec![0.0; 1_600])); // 100 ms dip: not a new onset
        onsets.extend(detector.push(&loud));
        onsets.extend(detector.push(&silence));
        onsets.extend(detector.push(&loud));
        assert_eq!(onsets, vec![500, 2_100]);
    }

    #[test]
    fn treats_steady_room_tone_as_quiet() {
        let rate = 16_000;
        // Room tone around -39 dBFS with speech bursts around -20 dBFS.
        let hiss: Vec<f32> = (0..rate as usize)
            .map(|i| if i % 2 == 0 { 0.011 } else { -0.011 })
            .collect();
        let speech: Vec<f32> = (0..rate as usize / 2)
            .map(|i| if i % 2 == 0 { 0.1 } else { -0.1 })
            .collect();
        let mut detector = OnsetDetector::new(rate);
        let mut onsets = detector.push(&hiss);
        onsets.extend(detector.push(&speech));
        onsets.extend(detector.push(&hiss));
        onsets.extend(detector.push(&speech));
        assert_eq!(onsets, vec![1_000, 2_500]);
    }
}
