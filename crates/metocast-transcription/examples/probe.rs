//! Offline check: decode a slice of a media file and report format and levels.
use std::path::PathBuf;
use std::time::Instant;

use metocast_transcription::decode::AudioReader;
use metocast_transcription::onset::OnsetDetector;
use metocast_transcription::resample::Resampler;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: probe <file> [start_s] [duration_s]")?,
    );
    let start: f64 = args.next().map_or(Ok(0.0), |s| s.parse())?;
    let duration: f64 = args.next().map_or(Ok(10.0), |s| s.parse())?;
    let clock = Instant::now();
    let mut reader = AudioReader::open(&path, start, Some(duration))?;
    println!(
        "rate {} Hz, {} ch, started at {:.3}s",
        reader.sample_rate(),
        reader.channels(),
        reader.start_seconds()
    );
    let mut resampler = Resampler::new(reader.sample_rate(), 24_000);
    let mut onsets = OnsetDetector::new(24_000);
    let (mut count, mut sum_sq, mut peak, mut found) = (0_usize, 0.0_f64, 0.0_f32, Vec::new());
    while let Some(chunk) = reader.next_chunk()? {
        let out = resampler.process(&chunk);
        found.extend(onsets.push(&out));
        for s in &out {
            sum_sq += f64::from(*s) * f64::from(*s);
            peak = peak.max(s.abs());
        }
        count += out.len();
    }
    let rms = (sum_sq / count.max(1) as f64).sqrt();
    println!(
        "decoded {:.1}s → {count} samples @24k in {:?}; RMS {:.1} dBFS, peak {:.1} dBFS; {} speech onsets",
        count as f64 / 24_000.0,
        clock.elapsed(),
        20.0 * rms.log10(),
        20.0 * f64::from(peak).log10(),
        found.len()
    );
    Ok(())
}
