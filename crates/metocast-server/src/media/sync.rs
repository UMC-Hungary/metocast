//! Where in an audio recording a video starts.
//!
//! The picture supplies a series per video frame, and the recording supplies its voice energy.
//! A series is either the speaker's mouth opening, read by `lip-reader` (Apple Vision's face
//! landmarks), or, when the video has sound of its own, that sound's energy. Lips open and close
//! with the syllables, so the series and the recording's energy rise and fall together at exactly
//! one position.
//!
//! 1. **Coarse:** every window of the picture is cross-correlated (FFT) against the whole
//!    recording on the frame grid. The window curves are summed by the offset they imply, and
//!    the peak must stand far above every other position: 12–16σ on a 6K sermon against an hour
//!    of Wireless PRO audio. A raw clock never narrows this search, because the camera and the
//!    recorder were 3:50 apart.
//! 2. **Fine:** each agreeing window is matched again in 1 ms steps around the peak. Their median
//!    is the offset, and a straight line through them gives the clock drift.
//!
//! Lip movement leads the sound it makes, so the lips' raw match comes out late by a roughly
//! constant lead, which is subtracted. The lead is not yet calibrated against true ground truth:
//! see `plans/PLAN-media-pipeline.md`.
use std::collections::HashMap;

use chrono::{DateTime, Local, Timelike, Utc};
use rustfft::{FftPlanner, num_complex::Complex};
use serde::Deserialize;

use metocast_core::media::SyncWindow;

/// The rate the recording is band-passed and resampled to before its energy is measured.
pub const ENVELOPE_RATE: u32 = 8_000;
/// The energy is kept per millisecond, the fine search's step.
const BINS_PER_SECOND: u32 = 1_000;
/// Frames either side of the moving average a series loses, so only syllable-rate motion is
/// compared (±0.5 s at 24 fps).
const HIGH_PASS_FRAMES: usize = 12;
/// How far either side of the coarse peak the fine search looks.
const FINE_SPAN_SECONDS: f64 = 0.25;
/// A window agrees with the result when it puts the start this close to it…
const AGREE_SECONDS: f64 = 1.0;
/// …or when its own match there stands this far above the rest of its curve: a small face's
/// window can be too weak to find the start alone, yet clearly back the one all windows found.
const SUPPORT_SIGMA: f64 = 2.0;
/// How far around a learned clock estimate a short clip is searched.
pub const PRIOR_SPAN_SECONDS: f64 = 10.0;
/// The coarse peak must stand this far above the rest of the recording to be trusted.
const CONFIDENT_SIGMA: f64 = 6.0;
/// …or, without enough picture for that, this far above the rest of the prior's span.
const CONFIDENT_LOCAL_SIGMA: f64 = 5.0;
const USABLE_SIGMA: f64 = 4.5;
const USABLE_LOCAL_SIGMA: f64 = 3.0;

/// How much the mouth opening leads the voice energy it produces, as the spike measured it
/// against hand-synced exports (+56 and +95 ms): placed so that any error stays within EBU
/// R37's window (audio at most 40 ms early, 60 ms late) until a ground-truth clip calibrates it.
pub const LIP_OPEN_LEAD_SECONDS: f64 = 0.060;
/// The opening rate peaks a little later in each syllable than the opening itself.
pub const LIP_RATE_LEAD_SECONDS: f64 = 0.082;

// ── The recording's energy ───────────────────────────────────────────────────

/// A recording's energy per millisecond, summed so any stretch's mean is two lookups.
#[derive(Debug, Clone, Default)]
pub struct Envelope {
    prefix: Vec<f64>,
    pending: f64,
    pending_samples: u32,
}

impl Envelope {
    pub fn new() -> Self {
        Self {
            prefix: vec![0.0],
            ..Self::default()
        }
    }

    /// Adds samples at [`ENVELOPE_RATE`].
    pub fn push(&mut self, samples: &[f32]) {
        let per_bin = ENVELOPE_RATE / BINS_PER_SECOND;
        for &sample in samples {
            self.pending += f64::from(sample) * f64::from(sample);
            self.pending_samples += 1;
            if self.pending_samples == per_bin {
                let total =
                    self.prefix.last().copied().unwrap_or(0.0) + self.pending / f64::from(per_bin);
                self.prefix.push(total);
                self.pending = 0.0;
                self.pending_samples = 0;
            }
        }
    }

    #[cfg(test)]
    pub fn from_samples(samples: &[f32]) -> Self {
        let mut envelope = Self::new();
        envelope.push(samples);
        envelope
    }

    pub fn duration(&self) -> f64 {
        (self.prefix.len().saturating_sub(1)) as f64 / f64::from(BINS_PER_SECOND)
    }

    /// The log mean energy of the `width` seconds centred on `center`; `None` outside the
    /// recording.
    pub fn level(&self, center: f64, width: f64) -> Option<f64> {
        let bins = f64::from(BINS_PER_SECOND);
        let from = ((center - width / 2.0) * bins).round();
        let to = ((center + width / 2.0) * bins).round();
        let last = (self.prefix.len() - 1) as f64;
        if from < 0.0 || to > last || to <= from {
            return None;
        }
        let (from, to) = (from as usize, to as usize);
        Some(((self.prefix[to] - self.prefix[from]) / (to - from) as f64 + 1e-9).ln())
    }
}

// ── What the picture supplies ────────────────────────────────────────────────

/// One measure per frame, and how far it leads the sound it goes with.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub values: Vec<Option<f64>>,
    pub lead_seconds: f64,
}

/// A stretch of the picture: one candidate per face (or one for the video's own sound), each a
/// set of features over the same frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    /// Where the window's first frame sits in the picture, in seconds.
    pub start: f64,
    pub candidates: Vec<Vec<Feature>>,
}

/// One line of `lip-reader` output.
#[derive(Debug, Deserialize)]
struct LipFrame {
    t: f64,
    faces: Vec<LipFace>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct LipFace {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    open: f64,
}

/// A face followed through a window by where its box is.
struct Track {
    open: Vec<Option<f64>>,
    center: (f64, f64),
    size: f64,
    last_frame: usize,
    area: f64,
}

/// Faces must be in at least this share of a window's frames to be the speaker.
const MIN_TRACK_COVERAGE: f64 = 0.4;
/// A face lost for longer than this starts a new track.
const MAX_TRACK_GAP_FRAMES: usize = 12;
const MAX_CANDIDATES: usize = 3;

/// The faces of `lip-reader`'s output for a window of `frames` frames starting at `start`
/// seconds, as candidates with two features each: the mouth opening and how fast it opens.
pub fn lips_window(output: &str, start: f64, frame: f64, frames: usize) -> Window {
    let mut tracks: Vec<Track> = Vec::new();
    for line in output.lines() {
        let Ok(parsed) = serde_json::from_str::<LipFrame>(line) else {
            continue;
        };
        let index = ((parsed.t - start) / frame).round();
        if index < 0.0 || index >= frames as f64 {
            continue;
        }
        let index = index as usize;
        let mut taken = vec![false; tracks.len()];
        for face in &parsed.faces {
            let center = (face.x + face.w / 2.0, face.y + face.h / 2.0);
            let size = face.w.max(face.h);
            let nearest = tracks
                .iter()
                .enumerate()
                .filter(|(at, track)| {
                    !taken[*at]
                        && index > track.last_frame
                        && index - track.last_frame <= MAX_TRACK_GAP_FRAMES
                })
                .map(|(at, track)| {
                    let distance = ((track.center.0 - center.0).powi(2)
                        + (track.center.1 - center.1).powi(2))
                    .sqrt();
                    (at, distance / track.size.max(size))
                })
                .filter(|(_, distance)| *distance < 0.5)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            let at = match nearest {
                Some((at, _)) => at,
                None => {
                    tracks.push(Track {
                        open: vec![None; frames],
                        center,
                        size,
                        last_frame: index,
                        area: 0.0,
                    });
                    taken.push(false);
                    tracks.len() - 1
                }
            };
            let track = &mut tracks[at];
            track.open[index] = Some(face.open);
            track.center = center;
            track.size = size;
            track.last_frame = index;
            track.area += face.w * face.h;
            taken[at] = true;
        }
    }
    let mut kept: Vec<(f64, Vec<Option<f64>>)> = tracks
        .into_iter()
        .filter(|track| {
            track.open.iter().flatten().count() as f64 >= MIN_TRACK_COVERAGE * frames as f64
        })
        .map(|track| (track.area, track.open))
        .collect();
    kept.sort_by(|a, b| b.0.total_cmp(&a.0));
    let candidates = kept
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, open)| {
            let rate = open
                .iter()
                .enumerate()
                .map(|(at, value)| {
                    match (at.checked_sub(1).and_then(|before| open[before]), value) {
                        (Some(before), Some(now)) => Some((now - before).max(0.0)),
                        _ => None,
                    }
                })
                .collect();
            vec![
                Feature {
                    values: open,
                    lead_seconds: LIP_OPEN_LEAD_SECONDS,
                },
                Feature {
                    values: rate,
                    lead_seconds: LIP_RATE_LEAD_SECONDS,
                },
            ]
        })
        .collect();
    Window { start, candidates }
}

/// The video's own sound as a window: its energy per frame, which leads nothing.
pub fn sound_window(sound: &Envelope, start: f64, frame: f64, frames: usize) -> Window {
    let values = (0..frames)
        .map(|at| sound.level(at as f64 * frame, frame))
        .collect();
    Window {
        start,
        candidates: vec![vec![Feature {
            values,
            lead_seconds: 0.0,
        }]],
    }
}

/// Where the windows go: `count` stretches of up to `length` seconds spread through the
/// picture, or the whole picture cut into at most `count` pieces when it's shorter than that.
/// A second `round` falls between the first round's windows.
pub fn plan_windows(duration: f64, count: u32, length: f64, round: u32) -> Vec<(f64, f64)> {
    let count = count.max(1);
    if duration <= f64::from(count) * length {
        if round > 0 {
            return Vec::new();
        }
        let pieces = (duration / length).ceil().max(1.0);
        let piece = duration / pieces;
        return (0..pieces as u32)
            .map(|n| (f64::from(n) * piece, piece))
            .collect();
    }
    let step = (duration - length) / f64::from(count);
    let shift = if round > 0 { 0.5 } else { 0.0 };
    (0..count)
        .map(|n| ((f64::from(n) + 0.25 + shift) * step, length))
        .filter(|(start, length)| start + length <= duration)
        .collect()
}

// ── Signal helpers ───────────────────────────────────────────────────────────

/// Each available value minus the mean of the available values around it, scaled to unit
/// variance; missing values become 0, so they add nothing to a correlation.
fn normalise(values: &[Option<f64>]) -> Option<Vec<f64>> {
    let mut prefix = vec![(0.0f64, 0usize)];
    for value in values {
        let (sum, count) = *prefix.last().unwrap_or(&(0.0, 0));
        prefix.push(match value {
            Some(value) => (sum + value, count + 1),
            None => (sum, count),
        });
    }
    let passed: Vec<Option<f64>> = values
        .iter()
        .enumerate()
        .map(|(at, value)| {
            let value = (*value)?;
            let from = at.saturating_sub(HIGH_PASS_FRAMES);
            let to = (at + HIGH_PASS_FRAMES + 1).min(values.len());
            let sum = prefix[to].0 - prefix[from].0;
            let count = prefix[to].1 - prefix[from].1;
            Some(value - sum / count as f64)
        })
        .collect();
    let present: Vec<f64> = passed.iter().flatten().copied().collect();
    if present.len() < 2 * HIGH_PASS_FRAMES {
        return None;
    }
    let mean = present.iter().sum::<f64>() / present.len() as f64;
    let deviation =
        (present.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / present.len() as f64).sqrt();
    if deviation <= 1e-12 {
        return None;
    }
    Some(
        passed
            .iter()
            .map(|value| value.map_or(0.0, |value| (value - mean) / deviation))
            .collect(),
    )
}

/// `corr[j] = Σ series[k]·signal[j + k] / n` for every `j` that keeps the series inside the
/// signal.
fn correlate(series: &[f64], signal: &[f64], planner: &mut FftPlanner<f64>) -> Vec<f64> {
    if series.is_empty() || signal.len() < series.len() {
        return Vec::new();
    }
    let size = (series.len() + signal.len()).next_power_of_two();
    let forward = planner.plan_fft_forward(size);
    let inverse = planner.plan_fft_inverse(size);
    let spectrum = |values: &[f64]| {
        let mut buffer: Vec<Complex<f64>> = values
            .iter()
            .map(|&value| Complex::new(value, 0.0))
            .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
            .take(size)
            .collect();
        forward.process(&mut buffer);
        buffer
    };
    let a = spectrum(series);
    let b = spectrum(signal);
    let mut product: Vec<Complex<f64>> = a.iter().zip(&b).map(|(a, b)| a.conj() * b).collect();
    inverse.process(&mut product);
    let scale = size as f64 * series.len() as f64;
    product[..=signal.len() - series.len()]
        .iter()
        .map(|value| value.re / scale)
        .collect()
}

/// Where `activity` (1 while someone speaks, 0 between, `step` seconds apart from 0) best meets
/// the voice in `audio`, at any lag: the seconds to add to its times, and whether that match
/// stands out clearly. Zeros either side let a transcript of a longer recording hang over.
pub fn align_activity(
    activity: &[Option<f64>],
    audio: &Envelope,
    step: f64,
) -> Option<(f64, bool)> {
    let series = normalise(activity)?;
    let grid: Vec<Option<f64>> = (0..(audio.duration() / step) as usize)
        .map(|at| audio.level(at as f64 * step, step))
        .collect();
    let voice = normalise(&grid)?;
    let pad = vec![0.0; series.len()];
    let signal: Vec<f64> = pad.iter().chain(&voice).chain(&pad).copied().collect();
    let scores = correlate(&series, &signal, &mut FftPlanner::new());
    let peak = argmax(&scores)?;
    Some((
        (peak as f64 - series.len() as f64) * step,
        sigma(&scores, peak) >= CONFIDENT_SIGMA,
    ))
}

/// How far `values[peak]` stands above the rest, in standard deviations.
fn sigma(values: &[f64], peak: usize) -> f64 {
    if values.len() < 3 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let deviation =
        (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64).sqrt();
    if deviation <= 1e-12 {
        return 0.0;
    }
    (values[peak] - mean) / deviation
}

fn argmax(values: &[f64]) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(at, _)| at)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// Pearson correlation of two equally long series, the second normalised here.
fn pearson(normalised: &[f64], other: &[Option<f64>]) -> Option<f64> {
    let other = normalise(other)?;
    Some(
        normalised
            .iter()
            .zip(&other)
            .map(|(a, b)| a * b)
            .sum::<f64>()
            / normalised.len() as f64,
    )
}

// ── The search ───────────────────────────────────────────────────────────────

/// What the search found.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// Where the picture's first frame falls in the recording, in seconds.
    pub offset_seconds: f64,
    pub drift_ppm: f64,
    pub confidence: f64,
    pub confident: bool,
    pub peak_sigma: f64,
    /// The search only looked around the prior.
    pub used_prior: bool,
    pub windows: Vec<SyncWindow>,
}

/// Matches `windows` of the picture (frames `frame` seconds apart) against `audio`. `prior` is a
/// learned clock estimate of the offset, used only when the picture is too short to stand out
/// over the whole recording. `None` when nothing matched well enough to use.
pub fn find_offset(
    windows: &[Window],
    audio: &Envelope,
    frame: f64,
    prior: Option<f64>,
) -> Option<Found> {
    let grid: Vec<Option<f64>> = (0..(audio.duration() / frame) as usize)
        .map(|at| audio.level(at as f64 * frame, frame))
        .collect();
    let grid = normalise(&grid)?;
    let mut planner = FftPlanner::new();

    // Coarse: each window's curves, one per feature plus their mix, from the candidate face
    // whose mix matches best somewhere.
    struct Coarse<'a> {
        window: &'a Window,
        features: &'a [Feature],
        first_frame: i64,
        /// Per feature; `None` where a feature is flat.
        curves: Vec<Option<Vec<f64>>>,
        mix: Vec<f64>,
    }
    impl Coarse<'_> {
        /// The curve a measure gives: one feature's, or (`None`) their mix.
        fn curve(&self, measure: Option<usize>) -> Option<&Vec<f64>> {
            match measure {
                None => Some(&self.mix),
                Some(feature) => self.curves.get(feature).and_then(Option::as_ref),
            }
        }
    }
    let mut coarse: Vec<Coarse> = Vec::new();
    for window in windows {
        let first_frame = (window.start / frame).round() as i64;
        let best = window
            .candidates
            .iter()
            .filter_map(|features| {
                let curves: Vec<Option<Vec<f64>>> = features
                    .iter()
                    .map(|feature| {
                        normalise(&feature.values)
                            .map(|series| correlate(&series, &grid, &mut planner))
                            .filter(|curve| !curve.is_empty())
                    })
                    .collect();
                let present: Vec<&Vec<f64>> = curves.iter().flatten().collect();
                let length = present.first()?.len();
                let mix: Vec<f64> = (0..length)
                    .map(|at| {
                        present.iter().map(|curve| curve[at]).sum::<f64>() / present.len() as f64
                    })
                    .collect();
                let peak = argmax(&mix)?;
                Some((sigma(&mix, peak), features.as_slice(), curves, mix))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, features, curves, mix)) = best {
            coarse.push(Coarse {
                window,
                features,
                first_frame,
                curves,
                mix,
            });
        }
    }
    if coarse.is_empty() {
        return None;
    }

    // Sum the window curves by the offset (in frames) each position implies, for the mix and for
    // each feature alone, and keep the measure that stands out most. On a close shot both
    // features work; on a wide shot with a small face only the opening rate does, and averaging
    // in the noise of the other would bury it.
    let feature_count = coarse
        .iter()
        .map(|window| window.curves.len())
        .max()
        .unwrap_or(0);
    let (measure, offsets, scores, global_sigma) = std::iter::once(None)
        .chain((0..feature_count).map(Some))
        .filter_map(|measure| {
            let mut combined: HashMap<i64, f64> = HashMap::new();
            for window in &coarse {
                for (at, score) in window.curve(measure)?.iter().enumerate() {
                    *combined.entry(at as i64 - window.first_frame).or_default() +=
                        score / coarse.len() as f64;
                }
            }
            let mut offsets: Vec<(i64, f64)> = combined.into_iter().collect();
            offsets.sort_by_key(|(offset, _)| *offset);
            let scores: Vec<f64> = offsets.iter().map(|(_, score)| *score).collect();
            let peak = argmax(&scores)?;
            let peak_sigma = sigma(&scores, peak);
            Some((measure, offsets, scores, peak_sigma))
        })
        .max_by(|a, b| a.3.total_cmp(&b.3))?;
    let peak = argmax(&scores)?;
    let mut best_offset = offsets[peak].0;
    let mut peak_sigma = global_sigma;
    let mut used_prior = false;
    let mut needed_sigma = (CONFIDENT_SIGMA, USABLE_SIGMA);

    let own_offset = |window: &Coarse| -> Option<i64> {
        argmax(window.curve(measure)?).map(|at| at as i64 - window.first_frame)
    };
    let agrees_with = |window: &Coarse, offset: i64| -> bool {
        let close = own_offset(window)
            .is_some_and(|own| ((own - offset) as f64 * frame).abs() <= AGREE_SECONDS);
        let backs = window.curve(measure).is_some_and(|curve| {
            usize::try_from(offset + window.first_frame)
                .ok()
                .filter(|&at| at < curve.len())
                .is_some_and(|at| sigma(curve, at) >= SUPPORT_SIGMA)
        });
        close || backs
    };
    let agreeing_with = |offset: i64| -> usize {
        coarse
            .iter()
            .filter(|window| agrees_with(window, offset))
            .count()
    };
    let majority =
        |agreeing: usize| agreeing * 2 > coarse.len() && (coarse.len() < 3 || agreeing >= 2);
    if !(global_sigma >= CONFIDENT_SIGMA && majority(agreeing_with(best_offset)))
        && let Some(prior) = prior
    {
        let near: Vec<usize> = offsets
            .iter()
            .enumerate()
            .filter(|(_, (offset, _))| (*offset as f64 * frame - prior).abs() <= PRIOR_SPAN_SECONDS)
            .map(|(at, _)| at)
            .collect();
        let local: Vec<f64> = near.iter().map(|&at| scores[at]).collect();
        if let Some(local_peak) = argmax(&local) {
            best_offset = offsets[near[local_peak]].0;
            peak_sigma = sigma(&local, local_peak);
            used_prior = true;
            needed_sigma = (CONFIDENT_LOCAL_SIGMA, USABLE_LOCAL_SIGMA);
        }
    }
    let agreeing = if used_prior {
        coarse.len()
    } else {
        agreeing_with(best_offset)
    };
    tracing::info!(
        measure = ?measure,
        peak_sigma,
        used_prior,
        agreeing,
        windows = coarse.len(),
        offset_seconds = best_offset as f64 * frame,
        "media: sync search"
    );
    if peak_sigma < needed_sigma.1 || !(used_prior || majority(agreeing)) {
        return None;
    }

    // Fine: each agreeing window again in 1 ms steps; one result per feature.
    let coarse_offset = best_offset as f64 * frame;
    let steps = (FINE_SPAN_SECONDS * f64::from(BINS_PER_SECOND)) as i64;
    let mut per_feature: Vec<Vec<(f64, f64, f64)>> = Vec::new();
    let mut reports: Vec<SyncWindow> = Vec::new();
    for window in &coarse {
        let own = own_offset(window).map_or(coarse_offset, |own| own as f64 * frame);
        let agrees = used_prior || agrees_with(window, best_offset);
        let mut window_report = SyncWindow {
            video_seconds: window.window.start,
            offset_seconds: own,
            score: window
                .curve(measure)
                .map_or(0.0, |curve| curve.iter().copied().fold(f64::MIN, f64::max)),
            agrees,
        };
        if agrees {
            for (index, feature) in window.features.iter().enumerate() {
                // A feature the coarse search found wanting isn't trusted for the timing either.
                if measure.is_some_and(|chosen| chosen != index) {
                    continue;
                }
                let Some(series) = normalise(&feature.values) else {
                    continue;
                };
                let frames = feature.values.len();
                let best = (-steps..=steps)
                    .filter_map(|step| {
                        let offset = coarse_offset + step as f64 / f64::from(BINS_PER_SECOND);
                        let levels: Vec<Option<f64>> = (0..frames)
                            .map(|at| {
                                audio.level(offset + window.window.start + at as f64 * frame, frame)
                            })
                            .collect();
                        if levels.iter().any(Option::is_none) {
                            return None;
                        }
                        pearson(&series, &levels).map(|score| (offset, score))
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((offset, score)) = best {
                    if per_feature.len() <= index {
                        per_feature.resize(index + 1, Vec::new());
                    }
                    per_feature[index].push((
                        window.window.start,
                        offset - feature.lead_seconds,
                        score,
                    ));
                    if index == measure.unwrap_or(0) {
                        window_report.offset_seconds = offset - feature.lead_seconds;
                        window_report.score = score;
                    }
                }
            }
        }
        reports.push(window_report);
    }
    // The feature whose windows agree most closely gives the answer.
    let (results, _) = per_feature
        .iter()
        .filter(|results| !results.is_empty())
        .map(|results| {
            let mut offsets: Vec<f64> = results.iter().map(|(_, offset, _)| *offset).collect();
            let middle = median(&mut offsets);
            let mut spread: Vec<f64> = offsets
                .iter()
                .map(|offset| (offset - middle).abs())
                .collect();
            (results, median(&mut spread))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    let mut offsets: Vec<f64> = results.iter().map(|(_, offset, _)| *offset).collect();
    let offset_seconds = median(&mut offsets);
    let drift_ppm = drift(results).unwrap_or(0.0);

    let confident = peak_sigma >= needed_sigma.0;
    let confidence = ((peak_sigma - needed_sigma.1) / (2.0 * needed_sigma.0 - needed_sigma.1))
        .clamp(0.0, 1.0)
        * agreeing as f64
        / coarse.len() as f64;
    Some(Found {
        offset_seconds,
        drift_ppm,
        confidence: (confidence.max(if confident { 0.5 } else { 0.0 }) * 1000.0).round() / 1000.0,
        confident,
        peak_sigma: (peak_sigma * 10.0).round() / 10.0,
        used_prior,
        windows: reports,
    })
}

/// How fast the offset grows through the picture, in parts per million: the slope of a straight
/// line through the windows. `None` with too few windows, too close together, or a slope the
/// windows' scatter could explain (under three standard errors), so noise is never "corrected".
fn drift(results: &[(f64, f64, f64)]) -> Option<f64> {
    if results.len() < 3 {
        return None;
    }
    let first = results.iter().map(|r| r.0).fold(f64::MAX, f64::min);
    let last = results.iter().map(|r| r.0).fold(f64::MIN, f64::max);
    if last - first < 60.0 {
        return None;
    }
    let n = results.len() as f64;
    let mean_t = results.iter().map(|r| r.0).sum::<f64>() / n;
    let mean_o = results.iter().map(|r| r.1).sum::<f64>() / n;
    let covariance: f64 = results
        .iter()
        .map(|r| (r.0 - mean_t) * (r.1 - mean_o))
        .sum();
    let variance: f64 = results.iter().map(|r| (r.0 - mean_t).powi(2)).sum();
    let slope = covariance / variance;
    let residual: f64 = results
        .iter()
        .map(|r| (r.1 - mean_o - slope * (r.0 - mean_t)).powi(2))
        .sum();
    let standard_error = (residual / (n - 2.0)).sqrt() / variance.sqrt();
    (slope.abs() >= 3.0 * standard_error).then(|| (slope * 1e6 * 10.0).round() / 10.0)
}

// ── Clocks and coverage ──────────────────────────────────────────────────────

/// `HH:MM:SS:FF` (or `;` before the frames, for drop-frame) as seconds since midnight.
pub fn timecode_seconds(timecode: &str, fps: f64) -> Option<f64> {
    let parts: Vec<f64> = timecode
        .split([':', ';', '.'])
        .map(|part| part.parse::<f64>().ok())
        .collect::<Option<_>>()?;
    let [hours, minutes, seconds, frames] = parts.as_slice() else {
        return None;
    };
    Some(hours * 3600.0 + minutes * 60.0 + seconds + frames / fps.round().max(1.0))
}

/// Seconds since local midnight.
pub fn local_seconds(at: DateTime<Utc>) -> f64 {
    let local = at.with_timezone(&Local);
    f64::from(local.num_seconds_from_midnight()) + f64::from(local.nanosecond()) / 1e9
}

/// A time-of-day difference folded into ±12 hours, so a camera with the wrong date still counts.
pub fn wrap_day(seconds: f64) -> f64 {
    let day = 86_400.0;
    let wrapped = seconds.rem_euclid(day);
    if wrapped > day / 2.0 {
        wrapped - day
    } else {
        wrapped
    }
}

/// What share of the picture the audio covers, and the silence before and after it.
pub fn coverage(
    offset_seconds: f64,
    drift_ppm: f64,
    video_seconds: f64,
    audio_seconds: f64,
) -> (f64, f64, f64) {
    let speed = 1.0 + drift_ppm / 1e6;
    let silent_start = (-offset_seconds / speed).clamp(0.0, video_seconds);
    let audio_end = (audio_seconds - offset_seconds) / speed;
    let silent_end = (video_seconds - audio_end).clamp(0.0, video_seconds);
    let covered = (video_seconds - silent_start - silent_end).max(0.0);
    let share = if video_seconds > 0.0 {
        covered / video_seconds
    } else {
        0.0
    };
    (share, silent_start, silent_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transcript_three_seconds_late_is_moved_back_onto_the_voice() {
        // Two minutes of bursts of "speech" (noise) and pauses, from a fixed generator.
        let mut seed = 7u64;
        let mut next = || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 33) as f64 / f64::from(1u32 << 31)
        };
        let rate = f64::from(ENVELOPE_RATE);
        let mut samples = Vec::new();
        let mut spoken = Vec::new();
        while (samples.len() as f64) < 120.0 * rate {
            let (talk, pause) = (1.0 + 4.0 * next(), 0.3 + 1.5 * next());
            spoken.push((
                samples.len() as f64 / rate,
                samples.len() as f64 / rate + talk,
            ));
            samples.extend((0..(talk * rate) as usize).map(|_| (next() as f32 - 0.5) * 0.6));
            samples.extend((0..(pause * rate) as usize).map(|_| (next() as f32 - 0.5) * 0.002));
        }
        let voice = Envelope::from_samples(&samples);
        let step = 0.1;
        let activity: Vec<Option<f64>> = (0..1_230)
            .map(|at| {
                let t = at as f64 * step - 3.0;
                Some(f64::from(u8::from(
                    spoken.iter().any(|(a, b)| (*a..*b).contains(&t)),
                )))
            })
            .collect();
        let (shift, confident) = align_activity(&activity, &voice, step).unwrap();
        assert!((shift + 3.0).abs() <= step, "{shift}");
        assert!(confident);
    }

    const FPS: f64 = 24.0;
    const FRAME: f64 = 1.0 / FPS;

    /// A deterministic "voice": syllables of varying length and loudness with pauses, as 8 kHz
    /// noise bursts, plus the syllable envelope itself per millisecond.
    fn speech(seconds: f64, seed: u64) -> (Vec<f32>, Vec<f64>) {
        let mut state = seed;
        let mut random = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            (state >> 33) as f64 / (1u64 << 31) as f64
        };
        let rate = f64::from(ENVELOPE_RATE);
        let total = (seconds * rate) as usize;
        let mut loudness = vec![0.0f64; (seconds * 1000.0) as usize + 1];
        let mut at = 0.0;
        while at < seconds {
            let length = 0.12 + random() * 0.25;
            let level = if random() < 0.2 { 0.0 } else { 0.2 + random() };
            let from = (at * 1000.0) as usize;
            let to = (((at + length) * 1000.0) as usize).min(loudness.len());
            for (offset, slot) in loudness[from..to].iter_mut().enumerate() {
                let phase = offset as f64 / (to - from) as f64;
                *slot = level * (std::f64::consts::PI * phase).sin();
            }
            at += length + random() * 0.08;
        }
        let samples = (0..total)
            .map(|n| {
                let envelope = loudness[(n as f64 / rate * 1000.0) as usize];
                ((random() - 0.5) * envelope + (random() - 0.5) * 0.001) as f32
            })
            .collect();
        (samples, loudness)
    }

    /// What a camera filming the voice from `start` seconds would see: the mouth opening per
    /// frame, `lead` seconds ahead of the sound, with some noise.
    fn mouth(
        loudness: &[f64],
        start: f64,
        frames: usize,
        lead: f64,
        seed: u64,
    ) -> Vec<Option<f64>> {
        let mut state = seed;
        (0..frames)
            .map(|at| {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let noise = ((state >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 0.3;
                let ms = ((start + at as f64 * FRAME + lead) * 1000.0) as usize;
                loudness
                    .get(ms)
                    .map(|level| 0.05 + level * 0.4 + noise * 0.1)
            })
            .collect()
    }

    fn lips(values: Vec<Option<f64>>, start: f64) -> Window {
        Window {
            start,
            candidates: vec![vec![Feature {
                values,
                lead_seconds: LIP_OPEN_LEAD_SECONDS,
            }]],
        }
    }

    #[test]
    fn lips_are_found_in_an_hour_of_audio_to_a_few_milliseconds() {
        let (samples, loudness) = speech(3_600.0, 7);
        let audio = Envelope::from_samples(&samples);
        let truth = 1_234.567;
        let windows: Vec<Window> = [0.0, 400.0, 800.0, 1_200.0]
            .iter()
            .map(|&start| {
                lips(
                    mouth(
                        &loudness,
                        truth + start,
                        90 * 24,
                        LIP_OPEN_LEAD_SECONDS,
                        start as u64 + 1,
                    ),
                    start,
                )
            })
            .collect();
        let found = find_offset(&windows, &audio, FRAME, None).unwrap();
        assert!((found.offset_seconds - truth).abs() < 0.005, "{found:?}");
        assert!(found.confident && found.peak_sigma > 10.0, "{found:?}");
        assert!(found.windows.iter().all(|window| window.agrees));
        assert!(found.drift_ppm.abs() < 20.0, "{found:?}");
    }

    #[test]
    fn a_clock_drift_is_measured() {
        let (samples, loudness) = speech(2_400.0, 3);
        let audio = Envelope::from_samples(&samples);
        let (truth, ppm) = (100.0, 50.0);
        let windows: Vec<Window> = [0.0, 600.0, 1_200.0, 1_800.0]
            .iter()
            .map(|&start| {
                let drifted = truth + start * (1.0 + ppm / 1e6);
                lips(
                    mouth(&loudness, drifted, 90 * 24, LIP_OPEN_LEAD_SECONDS, 9),
                    start,
                )
            })
            .collect();
        let found = find_offset(&windows, &audio, FRAME, None).unwrap();
        assert!((found.drift_ppm - ppm).abs() < 10.0, "{found:?}");
    }

    #[test]
    fn a_feature_that_only_adds_noise_is_left_out() {
        // A small face: the opening is noise, but the opening rate still follows the voice.
        let (samples, loudness) = speech(1_800.0, 13);
        let audio = Envelope::from_samples(&samples);
        let truth = 600.0;
        let windows: Vec<Window> = [0.0, 75.0, 150.0, 225.0]
            .iter()
            .map(|&start| {
                let voice = mouth(&loudness, truth + start, 75 * 24, LIP_RATE_LEAD_SECONDS, 3);
                let (_, other) = speech(100.0, start as u64 + 50);
                let noise = mouth(&other, 1.0, 75 * 24, 0.0, 4);
                Window {
                    start,
                    candidates: vec![vec![
                        Feature {
                            values: noise.iter().map(|v| v.map(|v| v * 3.0)).collect(),
                            lead_seconds: LIP_OPEN_LEAD_SECONDS,
                        },
                        Feature {
                            values: voice,
                            lead_seconds: LIP_RATE_LEAD_SECONDS,
                        },
                    ]],
                }
            })
            .collect();
        let found = find_offset(&windows, &audio, FRAME, None).unwrap();
        assert!((found.offset_seconds - truth).abs() < 0.01, "{found:?}");
    }

    #[test]
    fn scatter_is_not_mistaken_for_drift() {
        // The real sermon's four windows: ±9 ms of scatter over 225 s.
        let windows = [
            (0.0, 946.7393, 0.23),
            (75.0, 946.7563, 0.20),
            (150.0, 946.7513, 0.22),
            (225.0, 946.7483, 0.24),
        ];
        assert_eq!(drift(&windows), None);
        let steady: Vec<(f64, f64, f64)> = (0..5)
            .map(|n| {
                let t = f64::from(n) * 400.0;
                (
                    t,
                    100.0 + t * 40e-6 + if n % 2 == 0 { 0.001 } else { -0.001 },
                    0.3,
                )
            })
            .collect();
        let found = drift(&steady).unwrap();
        assert!((found - 40.0).abs() < 2.0, "{found}");
    }

    #[test]
    fn the_speaking_face_wins_over_a_bigger_silent_one() {
        let (samples, loudness) = speech(1_200.0, 11);
        let audio = Envelope::from_samples(&samples);
        let truth = 300.25;
        let speaker = mouth(&loudness, truth, 120 * 24, LIP_OPEN_LEAD_SECONDS, 2);
        // Someone listening: their mouth moves, but not with this voice.
        let (_, other) = speech(200.0, 99);
        let listener = mouth(&other, 10.0, 120 * 24, 0.0, 3);
        let to_lines = |faces: &[(f64, &Vec<Option<f64>>)]| -> String {
            (0..120 * 24)
                .map(|at| {
                    let faces: Vec<String> = faces
                        .iter()
                        .filter_map(|(x, values)| {
                            values[at].map(|open| {
                                format!(r#"{{"x":{x},"y":0.4,"w":0.2,"h":0.3,"open":{open},"confidence":0.9}}"#)
                            })
                        })
                        .collect();
                    format!(r#"{{"t":{},"faces":[{}]}}"#, at as f64 * FRAME, faces.join(","))
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        // The listener is listed first, as the bigger face would be.
        let output = to_lines(&[(0.1, &listener), (0.6, &speaker)]);
        let window = lips_window(&output, 0.0, FRAME, 120 * 24);
        assert_eq!(window.candidates.len(), 2);
        let found = find_offset(&[window], &audio, FRAME, None).unwrap();
        assert!((found.offset_seconds - truth).abs() < 0.01, "{found:?}");
    }

    #[test]
    fn clocks_only_help_a_clip_too_short_to_stand_out() {
        let (samples, loudness) = speech(3_600.0, 5);
        let audio = Envelope::from_samples(&samples);
        let truth = 302.35;
        // A clear 90-second match ignores a clock that is minutes off.
        let clear = lips(
            mouth(&loudness, truth, 90 * 24, LIP_OPEN_LEAD_SECONDS, 4),
            0.0,
        );
        let found = find_offset(std::slice::from_ref(&clear), &audio, FRAME, Some(532.0)).unwrap();
        assert!(
            !found.used_prior && (found.offset_seconds - truth).abs() < 0.01,
            "{found:?}"
        );
        // Six noisy seconds can't stand out over an hour, but can within a learned estimate.
        let noisy: Vec<Option<f64>> = mouth(&loudness, truth, 6 * 24, LIP_OPEN_LEAD_SECONDS, 4)
            .into_iter()
            .enumerate()
            .map(|(at, value)| value.map(|v| v + ((at * 7_919) % 13) as f64 * 0.012))
            .collect();
        let short = lips(noisy, 0.0);
        let alone = find_offset(std::slice::from_ref(&short), &audio, FRAME, None);
        assert!(
            alone.as_ref().is_none_or(|found| !found.confident),
            "{alone:?}"
        );
        let helped = find_offset(std::slice::from_ref(&short), &audio, FRAME, Some(300.0)).unwrap();
        assert!(helped.used_prior, "{helped:?}");
        assert!((helped.offset_seconds - truth).abs() < 0.05, "{helped:?}");
    }

    #[test]
    fn lips_unrelated_to_the_voice_match_nowhere() {
        let (samples, _) = speech(3_600.0, 5);
        let audio = Envelope::from_samples(&samples);
        let (_, other) = speech(100.0, 77);
        let unrelated = lips(mouth(&other, 5.0, 60 * 24, 0.0, 6), 0.0);
        assert_eq!(find_offset(&[unrelated], &audio, FRAME, None), None);
    }

    #[test]
    fn the_videos_own_sound_matches_without_a_lead() {
        let (samples, _) = speech(900.0, 21);
        let audio = Envelope::from_samples(&samples);
        let truth = 61.5;
        let from = (truth * f64::from(ENVELOPE_RATE)) as usize;
        let camera: Vec<f32> = samples[from..from + 120 * ENVELOPE_RATE as usize]
            .iter()
            .map(|sample| sample * 0.3)
            .collect();
        let window = sound_window(&Envelope::from_samples(&camera), 0.0, FRAME, 119 * 24);
        let found = find_offset(&[window], &audio, FRAME, None).unwrap();
        assert!((found.offset_seconds - truth).abs() < 0.003, "{found:?}");
        assert!(found.confident);
    }

    #[test]
    fn windows_spread_through_the_picture() {
        let windows = plan_windows(1_725.5, 4, 90.0, 0);
        assert_eq!(windows.len(), 4);
        assert!(
            windows
                .iter()
                .all(|(start, length)| *start >= 0.0 && start + length <= 1_725.5)
        );
        let between = plan_windows(1_725.5, 4, 90.0, 1);
        assert_eq!(between.len(), 4);
        assert!(between[0].0 > windows[0].0 && between[0].0 < windows[1].0);
        assert_eq!(plan_windows(43.2, 4, 90.0, 0), vec![(0.0, 43.2)]);
        assert_eq!(plan_windows(200.0, 4, 90.0, 0).len(), 3);
        assert!(plan_windows(200.0, 4, 90.0, 1).is_empty());
    }

    #[test]
    fn clocks_compare_by_time_of_day() {
        let tc = timecode_seconds("10:26:20:15", 24.0).unwrap();
        assert!((tc - 37_580.625).abs() < 1e-9);
        assert_eq!(timecode_seconds("01:00:00;00", 29.97), Some(3_600.0));
        assert_eq!(timecode_seconds("garbage", 24.0), None);
        assert!((wrap_day(86_000.0) - -400.0).abs() < 1e-9);
        assert!((wrap_day(-229.8) - -229.8).abs() < 1e-9);
    }

    #[test]
    fn coverage_counts_the_silence_on_either_side() {
        let (share, start, end) = coverage(346.81, 0.0, 1_725.5, 3_600.0);
        assert_eq!((share, start, end), (1.0, 0.0, 0.0));
        // The audio starts 10 s into the picture and lasts a minute: silence for the last 30 s.
        let (share, start, end) = coverage(-10.0, 0.0, 100.0, 60.0);
        assert_eq!((start, end), (10.0, 30.0));
        assert!((share - 0.6).abs() < 1e-9);
    }

    #[test]
    fn the_envelope_measures_any_stretch() {
        let samples = vec![0.5f32; ENVELOPE_RATE as usize * 2];
        let envelope = Envelope::from_samples(&samples);
        assert!((envelope.duration() - 2.0).abs() < 1e-9);
        let level = envelope.level(1.0, FRAME).unwrap();
        assert!((level - (0.25f64 + 1e-9).ln()).abs() < 1e-6);
        assert_eq!(envelope.level(0.01, FRAME), None);
        assert_eq!(envelope.level(1.99, FRAME), None);
    }
}
