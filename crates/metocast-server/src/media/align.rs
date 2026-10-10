//! How far processing moved the audio in time. jive-vocals' FFT and non-local-means noise
//! reduction each delay their output (about 1,200 and 384 samples at 48 kHz) without
//! compensating, and which of them run depends on the recording, so the delay is measured on
//! every job by cross-correlating windows of the source and the processed file.
use rustfft::{FftPlanner, num_complex::Complex};

/// A window's verdict: the processed audio lags the source by `lag` samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowLag {
    pub lag: i64,
    /// Normalised cross-correlation at that lag, 0–1.
    pub score: f64,
}

/// Windows below this score are noise or silence, not a match.
const MIN_SCORE: f64 = 0.3;
/// Windows agree when their lags are this close (one sample of resampling jitter either way).
const AGREE_SAMPLES: i64 = 2;

/// The lag of `processed` against `source` within ±`max_lag` samples, where `processed` covers
/// `source` plus `max_lag` extra samples on each side (so `processed[max_lag + lag + i]`
/// matches `source[i]`).
pub fn window_lag(source: &[f32], processed: &[f32], max_lag: usize) -> Option<WindowLag> {
    if source.is_empty() || processed.len() < source.len() + 2 * max_lag {
        return None;
    }
    let size = (source.len() + processed.len()).next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let forward = planner.plan_fft_forward(size);
    let inverse = planner.plan_fft_inverse(size);
    let spectrum = |signal: &[f32]| {
        let mut buffer: Vec<Complex<f64>> = signal
            .iter()
            .map(|&sample| Complex::new(f64::from(sample), 0.0))
            .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
            .take(size)
            .collect();
        forward.process(&mut buffer);
        buffer
    };
    let a = spectrum(source);
    let b = spectrum(processed);
    // corr[k] = Σ source[i]·processed[i+k]
    let mut corr: Vec<Complex<f64>> = a.iter().zip(&b).map(|(a, b)| a.conj() * b).collect();
    inverse.process(&mut corr);

    let source_energy: f64 = source.iter().map(|&s| f64::from(s).powi(2)).sum();
    let mut prefix = vec![0.0f64; processed.len() + 1];
    for (i, &sample) in processed.iter().enumerate() {
        prefix[i + 1] = prefix[i] + f64::from(sample).powi(2);
    }
    let mut best: Option<WindowLag> = None;
    for offset in 0..=2 * max_lag {
        let processed_energy = prefix[offset + source.len()] - prefix[offset];
        let norm = (source_energy * processed_energy).sqrt();
        if norm <= f64::EPSILON {
            continue;
        }
        let score = corr[offset].re / size as f64 / norm;
        if best.is_none_or(|best| score > best.score) {
            best = Some(WindowLag {
                lag: offset as i64 - max_lag as i64,
                score,
            });
        }
    }
    best
}

/// The delay most windows agree on, with how strongly they matched; `None` when they don't
/// agree, so a job never shifts audio by a guess.
pub fn consensus(windows: &[WindowLag]) -> Option<WindowLag> {
    let mut good: Vec<WindowLag> = windows
        .iter()
        .copied()
        .filter(|window| window.score >= MIN_SCORE)
        .collect();
    if good.is_empty() {
        return None;
    }
    good.sort_by_key(|window| window.lag);
    let median = good[good.len() / 2].lag;
    let agreeing: Vec<&WindowLag> = good
        .iter()
        .filter(|window| (window.lag - median).abs() <= AGREE_SAMPLES)
        .collect();
    let needed = if good.len() >= 2 { 2 } else { 1 };
    if agreeing.len() < needed || agreeing.len() * 2 < good.len() {
        return None;
    }
    let score = agreeing.iter().map(|window| window.score).sum::<f64>() / agreeing.len() as f64;
    Some(WindowLag { lag: median, score })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise that sounds nothing like itself shifted, as speech roughly doesn't.
    fn noise(len: usize, seed: u64) -> Vec<f32> {
        let mut state = seed;
        (0..len)
            .map(|_| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                ((state >> 33) as f32 / (1u64 << 31) as f32) - 0.5
            })
            .collect()
    }

    /// `processed` = `source` delayed by `delay`, with `max_lag` margin each side.
    fn delayed(signal: &[f32], start: usize, len: usize, delay: i64, max_lag: usize) -> Vec<f32> {
        let from = (start as i64 - max_lag as i64 - delay) as usize;
        signal[from..from + len + 2 * max_lag]
            .iter()
            .map(|sample| sample * 0.7)
            .collect()
    }

    #[test]
    fn the_noise_reduction_delay_is_found_to_the_sample() {
        let signal = noise(48_000, 7);
        let source = &signal[10_000..14_800];
        for delay in [1_582, 0, -37] {
            let processed = delayed(&signal, 10_000, 4_800, delay, 2_000);
            let found = window_lag(source, &processed, 2_000).unwrap();
            assert_eq!(found.lag, delay);
            assert!(found.score > 0.99, "{found:?}");
        }
    }

    #[test]
    fn silence_matches_nothing() {
        let processed = vec![0.0; 4_800 + 4_000];
        assert!(window_lag(&noise(4_800, 1), &processed, 2_000).is_none());
    }

    #[test]
    fn consensus_ignores_a_quiet_window_but_refuses_disagreement() {
        let at = |lag, score| WindowLag { lag, score };
        let found =
            consensus(&[at(1582, 0.9), at(1581, 0.8), at(136, 0.1), at(1582, 0.7)]).unwrap();
        assert_eq!(found.lag, 1582);
        assert!((found.score - 0.8).abs() < 1e-9);
        assert!(consensus(&[at(1582, 0.9), at(400, 0.9)]).is_none());
        assert!(consensus(&[at(1582, 0.1)]).is_none());
        assert_eq!(consensus(&[at(12, 0.6)]).map(|w| w.lag), Some(12));
    }
}
