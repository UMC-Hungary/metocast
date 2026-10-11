//! Streaming windowed-sinc resampler (mono) and PCM16 conversion.
//!
//! Speech only needs a clean low-pass before decimation, so a precomputed polyphase table with
//! 1024 phases is plenty and keeps this dependency-free.

use std::f64::consts::PI;

const ZERO_CROSSINGS: f64 = 16.0;
const PHASES: usize = 1024;
/// Keep the pass band slightly below the output Nyquist to leave room for the transition band.
const CUTOFF_MARGIN: f64 = 0.95;

pub struct Resampler {
    step: f64,
    half_width: usize,
    table: Vec<f32>,
    buffer: Vec<f32>,
    /// Absolute input index of `buffer[0]`.
    buffer_start: u64,
    next_output: u64,
    input_total: u64,
    output_rate: u32,
    input_rate: u32,
}

impl Resampler {
    #[must_use]
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        let ratio = f64::from(output_rate) / f64::from(input_rate);
        let bandwidth = ratio.min(1.0) * CUTOFF_MARGIN;
        let half_width = (ZERO_CROSSINGS / bandwidth).ceil() as usize;
        let taps = 2 * half_width;
        let mut table = vec![0.0_f32; PHASES * taps];
        for phase in 0..PHASES {
            let frac = phase as f64 / PHASES as f64;
            for tap in 0..taps {
                // Distance between the output position and input sample `tap`.
                let x = frac + (half_width as f64 - 1.0) - tap as f64;
                table[phase * taps + tap] = kernel(x, bandwidth, half_width as f64) as f32;
            }
        }
        Self {
            step: f64::from(input_rate) / f64::from(output_rate),
            half_width,
            table,
            buffer: Vec::new(),
            buffer_start: 0,
            next_output: 0,
            input_total: 0,
            output_rate,
            input_rate,
        }
    }

    /// Feeds input samples and returns every output sample that is now fully determined.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.buffer.extend_from_slice(input);
        self.input_total += input.len() as u64;
        self.drain(false)
    }

    /// Flushes the tail, padding with silence. Output length matches the input duration.
    pub fn finish(&mut self) -> Vec<f32> {
        self.drain(true)
    }

    fn drain(&mut self, flush: bool) -> Vec<f32> {
        let taps = 2 * self.half_width;
        let expected_total =
            (self.input_total * u64::from(self.output_rate)).div_ceil(u64::from(self.input_rate));
        let mut output = Vec::new();
        loop {
            if flush && self.next_output >= expected_total {
                break;
            }
            let position = self.next_output as f64 * self.step;
            let center = position.floor() as i64;
            let first = center - self.half_width as i64 + 1;
            let last = center + self.half_width as i64;
            let available_end = self.buffer_start as i64 + self.buffer.len() as i64;
            if !flush && last >= available_end {
                break;
            }
            let phase = (((position - center as f64) * PHASES as f64) as usize).min(PHASES - 1);
            let coefficients = &self.table[phase * taps..(phase + 1) * taps];
            let mut sum = 0.0_f32;
            for (tap, coefficient) in coefficients.iter().enumerate() {
                let index = first + tap as i64 - self.buffer_start as i64;
                if index >= 0
                    && let Some(sample) = self.buffer.get(index as usize)
                {
                    sum += sample * coefficient;
                }
            }
            output.push(sum);
            self.next_output += 1;
        }
        // Drop input that no future output can reach.
        let keep_from = ((self.next_output as f64 * self.step).floor() as i64
            - self.half_width as i64)
            .max(self.buffer_start as i64) as u64;
        let drop = (keep_from - self.buffer_start) as usize;
        if drop > 0 && drop <= self.buffer.len() {
            self.buffer.drain(..drop);
            self.buffer_start = keep_from;
        }
        output
    }
}

/// Blackman-windowed sinc low-pass, normalised for unity gain at DC.
fn kernel(x: f64, bandwidth: f64, half_width: f64) -> f64 {
    if x.abs() >= half_width {
        return 0.0;
    }
    let arg = PI * bandwidth * x;
    let sinc = if arg.abs() < 1e-12 {
        1.0
    } else {
        arg.sin() / arg
    };
    let n = (x + half_width) / (2.0 * half_width);
    let window = 0.42 - 0.5 * (2.0 * PI * n).cos() + 0.08 * (4.0 * PI * n).cos();
    bandwidth * sinc * window
}

/// Converts `[-1, 1]` floats to little-endian-ready signed 16-bit samples.
#[must_use]
pub fn to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|s| (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frequency: f64, rate: u32, seconds: f64) -> Vec<f32> {
        let count = (f64::from(rate) * seconds) as usize;
        (0..count)
            .map(|i| (2.0 * PI * frequency * i as f64 / f64::from(rate)).sin() as f32 * 0.5)
            .collect()
    }

    fn resample_all(input: &[f32], from: u32, to: u32) -> Vec<f32> {
        let mut resampler = Resampler::new(from, to);
        let mut output = Vec::new();
        for chunk in input.chunks(1000) {
            output.extend(resampler.process(chunk));
        }
        output.extend(resampler.finish());
        output
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn output_length_matches_duration() {
        let input = tone(440.0, 48_000, 1.0);
        assert_eq!(resample_all(&input, 48_000, 24_000).len(), 24_000);
        assert_eq!(resample_all(&input, 48_000, 16_000).len(), 16_000);
        let input = tone(440.0, 44_100, 1.0);
        assert_eq!(resample_all(&input, 44_100, 24_000).len(), 24_000);
    }

    #[test]
    fn passes_speech_band_and_rejects_aliases() {
        let speech = resample_all(&tone(1_000.0, 48_000, 1.0), 48_000, 24_000);
        let middle = &speech[2_000..22_000];
        assert!(
            (peak(middle) - 0.5).abs() < 0.02,
            "1 kHz peak {}",
            peak(middle)
        );

        let alias = resample_all(&tone(20_000.0, 48_000, 1.0), 48_000, 24_000);
        assert!(
            peak(&alias[2_000..22_000]) < 0.01,
            "20 kHz leaked {}",
            peak(&alias)
        );
    }

    #[test]
    fn pcm16_clamps() {
        assert_eq!(to_pcm16(&[0.0, 1.5, -1.5]), vec![0, i16::MAX, -i16::MAX]);
    }
}
