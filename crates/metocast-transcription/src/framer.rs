//! Cuts mono audio at any sample rate into the 100 ms, 24 kHz PCM16 frames the realtime API
//! takes, and reports speech onsets on the same clock. Shared by the live tap and file jobs.

use crate::onset::OnsetDetector;
use crate::realtime::SAMPLE_RATE;
use crate::resample::{Resampler, to_pcm16};

pub const FRAME_MS: u64 = 100;
const FRAME_SAMPLES: usize = (SAMPLE_RATE as u64 * FRAME_MS / 1_000) as usize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub pcm: Vec<i16>,
    /// Speech onsets in this frame, in ms since the first sample pushed.
    pub onsets_ms: Vec<u64>,
}

pub struct Framer {
    resampler: Resampler,
    detector: OnsetDetector,
    buffer: Vec<f32>,
}

impl Framer {
    #[must_use]
    pub fn new(input_rate: u32) -> Self {
        Self {
            resampler: Resampler::new(input_rate, SAMPLE_RATE),
            detector: OnsetDetector::new(SAMPLE_RATE),
            buffer: Vec::new(),
        }
    }

    /// Returns every complete frame; the remainder waits for more input.
    pub fn push(&mut self, mono: &[f32]) -> Vec<Frame> {
        self.buffer.extend(self.resampler.process(mono));
        let mut frames = Vec::new();
        while self.buffer.len() >= FRAME_SAMPLES {
            let samples: Vec<f32> = self.buffer.drain(..FRAME_SAMPLES).collect();
            frames.push(Frame {
                onsets_ms: self.detector.push(&samples),
                pcm: to_pcm16(&samples),
            });
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_100ms_frames_at_24k_with_onsets() {
        let mut framer = Framer::new(48_000);
        let mut frames = framer.push(&vec![0.0; 48_000]);
        frames.extend(framer.push(&vec![0.3; 48_000]));
        assert!(frames.len() >= 19);
        assert!(frames.iter().all(|f| f.pcm.len() == FRAME_SAMPLES));
        let onsets: Vec<u64> = frames.iter().flat_map(|f| f.onsets_ms.clone()).collect();
        assert_eq!(onsets.len(), 1);
        assert!((980..=1_040).contains(&onsets[0]), "{onsets:?}");
    }
}
