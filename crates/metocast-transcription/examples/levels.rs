//! Offline check: per-20 ms frame level percentiles for a slice of a media file.
use std::path::PathBuf;

use metocast_transcription::decode::AudioReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("usage: levels <file> start_s duration_s")?,
    );
    let start: f64 = args.next().map_or(Ok(0.0), |s| s.parse())?;
    let duration: f64 = args.next().map_or(Ok(60.0), |s| s.parse())?;
    let mut reader = AudioReader::open(&path, start, Some(duration))?;
    let frame = reader.sample_rate() as usize / 50;
    let mut samples = Vec::new();
    while let Some(chunk) = reader.next_chunk()? {
        samples.extend(chunk);
    }
    let mut db: Vec<f64> = samples
        .chunks_exact(frame)
        .map(|f| {
            10.0 * (f.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / frame as f64)
                .max(1e-12)
                .log10()
        })
        .collect();
    db.sort_by(f64::total_cmp);
    let p = |q: usize| db[(db.len() - 1) * q / 100];
    println!(
        "{start:>6}s: p5 {:.1}  p10 {:.1}  p25 {:.1}  p50 {:.1}  p90 {:.1} dBFS",
        p(5),
        p(10),
        p(25),
        p(50),
        p(90)
    );
    Ok(())
}
