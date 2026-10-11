//! `metocast-transcribe`: Hungarian speech in a media file → English captions via OpenAI.
//!
//! `--mode realtime` simulates a live service: the file's audio is fed to
//! `gpt-realtime-translate` in 100 ms frames at `--speed` × real time, the text is cut into
//! segments while "speech" continues, and the stream is shown live in the terminal and,
//! with `--serve`, pushed over a local WebSocket to a browser viewer.
//! `--mode whisper` is the batch baseline (`whisper-1` translations).

mod serve;

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufWriter, IsTerminal as _, Write as _};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use metocast_transcription::captions::{self, CaptionOptions};
use metocast_transcription::decode::AudioReader;
use metocast_transcription::onset::OnsetDetector;
use metocast_transcription::realtime::{self, RealtimeConfig, RealtimeEvent};
use metocast_transcription::resample::{Resampler, to_pcm16};
use metocast_transcription::segmenter::{Segment, SegmentEvent, Segmenter, SegmenterConfig};
use metocast_transcription::whisper;
use serde_json::{Value, json};
use tokio::sync::mpsc;

const USAGE: &str =
    "usage: metocast-transcribe <input> [--mode realtime|whisper] [--start HH:MM:SS|secs]
       [--duration secs|HH:MM:SS] [--speed 1.0] [--lag-ms auto|N] [--out DIR]
       [--api-key-file PATH] [--serve 127.0.0.1:8787] [--url URL] [--gap-ms 1200] [--max-chars 200]

The API key is read from --api-key-file, else OPENAI_API_KEY, else ~/.config/metocast/openai.key.";

const REALTIME_USD_PER_MINUTE: f64 = 0.034;
const WHISPER_USD_PER_MINUTE: f64 = 0.006;
const FRAME_SAMPLES: usize = (realtime::SAMPLE_RATE / 10) as usize;
const TRAILING_SILENCE_FRAMES: usize = 30;
/// Onsets older than this when text arrives are treated as unanswered (music, noise).
const LATENCY_WINDOW_MS: u64 = 10_000;
const IDLE_BEFORE_ONSET_MS: u64 = 1_500;
const DEFAULT_LAG_MS: i64 = 2_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Realtime,
    Whisper,
}

#[derive(Clone, Copy)]
enum Lag {
    Auto,
    Fixed(i64),
}

struct Args {
    input: PathBuf,
    mode: Mode,
    start: f64,
    duration: Option<f64>,
    speed: f64,
    lag: Lag,
    out: PathBuf,
    api_key_file: Option<PathBuf>,
    serve: Option<SocketAddr>,
    url: Option<String>,
    segmenter: SegmenterConfig,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match run(args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> Result<(), String> {
    std::fs::create_dir_all(&args.out)
        .map_err(|e| format!("cannot create {}: {e}", args.out.display()))?;
    let api_key = read_api_key(args.api_key_file.as_deref())?;
    match args.mode {
        Mode::Realtime => run_realtime(&args, api_key).await,
        Mode::Whisper => run_whisper(&args, api_key).await,
    }
}

// ---------------------------------------------------------------------------------------------
// Realtime (simulated live) mode

async fn run_realtime(args: &Args, api_key: String) -> Result<(), String> {
    let started = Instant::now();
    let stem = output_stem(args);
    let broadcaster = match args.serve {
        Some(addr) => {
            let (broadcaster, http) = serve::Broadcaster::start(addr)
                .await
                .map_err(|e| format!("cannot serve on {addr}: {e}"))?;
            println!("viewer: http://{http}/  (WebSocket ws://{addr})");
            Some(broadcaster)
        }
        None => None,
    };

    let (audio_tx, audio_rx) = mpsc::channel::<Vec<i16>>(32);
    let (onset_tx, mut onset_rx) = mpsc::unbounded_channel::<u64>();
    let (input, start, duration, speed) =
        (args.input.clone(), args.start, args.duration, args.speed);
    let monitor = broadcaster.clone();
    let feeder = std::thread::Builder::new()
        .name("audio-feeder".into())
        .spawn(move || {
            feed(
                &input,
                start,
                duration,
                speed,
                &audio_tx,
                &onset_tx,
                monitor.as_ref(),
            )
        })
        .map_err(|e| e.to_string())?;

    let mut config = RealtimeConfig::new(api_key);
    if let Some(url) = &args.url {
        config.url.clone_from(url);
    }
    config.forward_audio = broadcaster.is_some();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let client = tokio::spawn(realtime::run(config, audio_rx, event_tx));
    let mut translated_audio_samples: u64 = 0;

    let events_path = args.out.join(format!("{stem}.realtime.events.jsonl"));
    let mut log = BufWriter::new(File::create(&events_path).map_err(|e| e.to_string())?);
    let mut segmenter = Segmenter::new(args.segmenter);
    let mut view = TerminalView::new();
    let mut live = Live {
        segments: Vec::new(),
        view: &mut view,
        broadcaster: broadcaster.as_ref(),
    };
    let mut source_text = String::new();
    let (mut target_latency, mut source_latency) = (LatencyProbe::new(), LatencyProbe::new());
    let mut last_audio_ms = 0;
    let mut close_reason = String::from("unknown");
    let mut server_errors = Vec::new();
    let mut event_kinds = std::collections::BTreeMap::<String, u32>::new();

    loop {
        tokio::select! {
            Some(onset) = onset_rx.recv() => {
                target_latency.pending.push_back(onset);
                source_latency.pending.push_back(onset);
            }
            event = event_rx.recv() => {
                let Some(event) = event else { break };
                match event {
                    RealtimeEvent::AudioSent { at_ms } => {
                        last_audio_ms = at_ms;
                        live.apply(segmenter.tick(at_ms), at_ms);
                        live.view.progress(at_ms);
                    }
                    RealtimeEvent::TargetDelta { at_ms, text } => {
                        target_latency.text_at(at_ms);
                        write_log(&mut log, &json!({ "atMs": at_ms, "kind": "target.delta", "text": text }));
                        live.apply(segmenter.push(at_ms, &text), at_ms);
                    }
                    RealtimeEvent::TargetAudio { pcm, .. } => {
                        translated_audio_samples += pcm.len() as u64;
                        if let Some(b) = &broadcaster {
                            b.audio(serve::TRANSLATED_AUDIO, &pcm);
                        }
                    }
                    RealtimeEvent::SourceDelta { at_ms, text } => {
                        source_latency.text_at(at_ms);
                        write_log(&mut log, &json!({ "atMs": at_ms, "kind": "source.delta", "text": text }));
                        source_text.push_str(&text);
                    }
                    RealtimeEvent::Server { at_ms, kind, raw } => {
                        *event_kinds.entry(kind.clone()).or_default() += 1;
                        if kind == "error" {
                            live.view.note(&format!("server error: {raw}"));
                            server_errors.push(raw.to_string());
                        }
                        write_log(&mut log, &json!({ "atMs": at_ms, "kind": kind, "raw": raw }));
                    }
                    RealtimeEvent::Closed { at_ms, reason } => {
                        write_log(&mut log, &json!({ "atMs": at_ms, "kind": "closed", "reason": reason }));
                        close_reason = reason;
                    }
                }
            }
        }
    }
    let final_events = segmenter.finish(last_audio_ms);
    live.apply(final_events, last_audio_ms);
    if let Some(broadcaster) = &broadcaster {
        broadcaster.status("finished");
    }
    log.flush().map_err(|e| e.to_string())?;

    let client_result = client.await.map_err(|e| e.to_string())?;
    let feed_stats = feeder
        .join()
        .map_err(|_| "audio feeder panicked".to_string())??;
    let segments = live.segments;

    let measured_lag = percentile(&target_latency.samples, 50);
    let shift_ms = match args.lag {
        Lag::Fixed(ms) => ms,
        Lag::Auto => measured_lag.map_or(DEFAULT_LAG_MS, |ms| ms as i64),
    };
    let written = write_outputs(&args.out, &format!("{stem}.realtime"), &segments, shift_ms)?;
    let source_path = args.out.join(format!("{stem}.realtime.hu.txt"));
    std::fs::write(&source_path, source_text.trim()).map_err(|e| e.to_string())?;

    let audio_minutes = feed_stats.sent_ms as f64 / 60_000.0;
    println!();
    println!("── summary ─────────────────────────────────────────────");
    println!(
        "input       {} (from {:.1}s, {} Hz, {} ch)",
        args.input.display(),
        feed_stats.start_seconds,
        feed_stats.source_rate,
        feed_stats.channels
    );
    println!(
        "audio       {} sent (incl. {}s trailing silence) in {} wall · speed {:.2}×",
        mmss(feed_stats.sent_ms),
        TRAILING_SILENCE_FRAMES / 10,
        mmss(started.elapsed().as_millis() as u64),
        feed_stats.sent_ms as f64 / started.elapsed().as_millis().max(1) as f64
    );
    println!(
        "segments    {} · EN chars {} · HU chars {}",
        segments.len(),
        segments
            .iter()
            .map(|s| s.text.chars().count())
            .sum::<usize>(),
        source_text.chars().count()
    );
    println!(
        "latency     EN p50 {} p95 {} (n={}) · HU p50 {} p95 {} (n={})  [speech onset → first text]",
        fmt_ms(measured_lag),
        fmt_ms(percentile(&target_latency.samples, 95)),
        target_latency.samples.len(),
        fmt_ms(percentile(&source_latency.samples, 50)),
        fmt_ms(percentile(&source_latency.samples, 95)),
        source_latency.samples.len()
    );
    println!("captions    shifted by {shift_ms} ms");
    if translated_audio_samples > 0 {
        println!(
            "interpreter {} of English speech audio",
            mmss(translated_audio_samples * 1_000 / u64::from(realtime::SAMPLE_RATE))
        );
    }
    println!(
        "connection  ended: {close_reason}; client: {}",
        client_result
            .as_ref()
            .map_or_else(ToString::to_string, |()| "ok".to_string())
    );
    println!(
        "events      {}",
        event_kinds
            .iter()
            .map(|(k, n)| format!("{k}×{n}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !server_errors.is_empty() {
        println!("errors      {}", server_errors.len());
    }
    println!(
        "est. cost   ${:.2} ({audio_minutes:.1} min × ${REALTIME_USD_PER_MINUTE}/min)",
        audio_minutes * REALTIME_USD_PER_MINUTE
    );
    for path in written.iter().chain([&source_path, &events_path]) {
        println!("wrote       {}", path.display());
    }
    client_result.map_err(|e| e.to_string())?;
    if segments.is_empty() && !server_errors.is_empty() {
        return Err(format!("the server rejected the session: {close_reason}"));
    }
    Ok(())
}

/// Routes segmenter output to the terminal, the WebSocket viewer and the segment list.
struct Live<'a> {
    segments: Vec<Segment>,
    view: &'a mut TerminalView,
    broadcaster: Option<&'a serve::Broadcaster>,
}

impl Live<'_> {
    fn apply(&mut self, events: Vec<SegmentEvent>, at_ms: u64) {
        for event in events {
            match event {
                SegmentEvent::Committed(segment) => {
                    self.view.committed(&segment);
                    if let Some(b) = self.broadcaster {
                        b.segment(&segment);
                    }
                    self.segments.push(segment);
                }
                SegmentEvent::Partial(text) => {
                    self.view.partial(&text);
                    if let Some(b) = self.broadcaster {
                        b.partial(&text, at_ms);
                    }
                }
            }
        }
    }
}

struct FeedStats {
    source_rate: u32,
    channels: usize,
    start_seconds: f64,
    sent_ms: u64,
}

/// Decodes, resamples to 24 kHz and sends 100 ms frames paced at `speed` × real time, like a
/// microphone would. Runs on its own thread.
fn feed(
    path: &Path,
    start: f64,
    duration: Option<f64>,
    speed: f64,
    audio_tx: &mpsc::Sender<Vec<i16>>,
    onset_tx: &mpsc::UnboundedSender<u64>,
    monitor: Option<&serve::Broadcaster>,
) -> Result<FeedStats, String> {
    let mut reader = AudioReader::open(path, start, duration).map_err(|e| e.to_string())?;
    let mut resampler = Resampler::new(reader.sample_rate(), realtime::SAMPLE_RATE);
    let mut onsets = OnsetDetector::new(realtime::SAMPLE_RATE);
    let mut pacer = Pacer {
        tx: audio_tx,
        monitor,
        clock: Instant::now(),
        sent_samples: 0,
        speed,
    };
    let mut pending: Vec<i16> = Vec::new();
    let mut open = true;
    loop {
        let (samples, done) = match reader.next_chunk().map_err(|e| e.to_string())? {
            Some(chunk) => (resampler.process(&chunk), false),
            None => (resampler.finish(), true),
        };
        for onset in onsets.push(&samples) {
            if onset_tx.send(onset).is_err() {
                // The session loop is gone: nobody is left to feed.
                open = false;
                break;
            }
        }
        pending.extend(to_pcm16(&samples));
        while open && pending.len() >= FRAME_SAMPLES {
            open = pacer.send(pending.drain(..FRAME_SAMPLES).collect());
        }
        if done || !open {
            break;
        }
    }
    if open && !pending.is_empty() {
        open = pacer.send(pending);
    }
    for _ in 0..TRAILING_SILENCE_FRAMES {
        if !open {
            break;
        }
        open = pacer.send(vec![0; FRAME_SAMPLES]);
    }
    Ok(FeedStats {
        source_rate: reader.sample_rate(),
        channels: reader.channels(),
        start_seconds: reader.start_seconds(),
        sent_ms: pacer.sent_samples * 1_000 / u64::from(realtime::SAMPLE_RATE),
    })
}

struct Pacer<'a> {
    tx: &'a mpsc::Sender<Vec<i16>>,
    /// Listeners hear the original audio exactly as it is sent for translation.
    monitor: Option<&'a serve::Broadcaster>,
    clock: Instant,
    sent_samples: u64,
    speed: f64,
}

impl Pacer<'_> {
    fn send(&mut self, frame: Vec<i16>) -> bool {
        let due = Duration::from_secs_f64(
            self.sent_samples as f64 / f64::from(realtime::SAMPLE_RATE) / self.speed,
        );
        if let Some(wait) = due.checked_sub(self.clock.elapsed()) {
            std::thread::sleep(wait);
        }
        self.sent_samples += frame.len() as u64;
        if let Some(monitor) = self.monitor {
            monitor.audio(serve::ORIGINAL_AUDIO, &frame);
        }
        self.tx.blocking_send(frame).is_ok()
    }
}

/// Measures speech onset → first text, but only for onsets the model met while idle: text that
/// keeps flowing across an onset is the previous sentence's tail and would read as ~0 latency.
struct LatencyProbe {
    pending: VecDeque<u64>,
    last_text_ms: Option<u64>,
    samples: Vec<u64>,
}

impl LatencyProbe {
    fn new() -> Self {
        Self {
            pending: VecDeque::new(),
            last_text_ms: None,
            samples: Vec::new(),
        }
    }

    fn text_at(&mut self, at_ms: u64) {
        let mut measured = None;
        while let Some(&onset) = self.pending.front() {
            if onset > at_ms {
                break;
            }
            self.pending.pop_front();
            let idle_before = self
                .last_text_ms
                .is_none_or(|last| last + IDLE_BEFORE_ONSET_MS <= onset);
            if measured.is_none() && idle_before && at_ms - onset <= LATENCY_WINDOW_MS {
                measured = Some(at_ms - onset);
            }
        }
        if let Some(latency) = measured {
            self.samples.push(latency);
        }
        self.last_text_ms = Some(at_ms);
    }
}

// ---------------------------------------------------------------------------------------------
// Whisper (batch baseline) mode

async fn run_whisper(args: &Args, api_key: String) -> Result<(), String> {
    let started = Instant::now();
    let stem = output_stem(args);
    let (input, start, duration) = (args.input.clone(), args.start, args.duration);
    println!("decoding {} …", input.display());
    let pcm = tokio::task::spawn_blocking(move || -> Result<Vec<i16>, String> {
        let mut reader = AudioReader::open(&input, start, duration).map_err(|e| e.to_string())?;
        let mut resampler = Resampler::new(reader.sample_rate(), whisper::SAMPLE_RATE);
        let mut pcm = Vec::new();
        while let Some(chunk) = reader.next_chunk().map_err(|e| e.to_string())? {
            pcm.extend(to_pcm16(&resampler.process(&chunk)));
        }
        pcm.extend(to_pcm16(&resampler.finish()));
        Ok(pcm)
    })
    .await
    .map_err(|e| e.to_string())??;
    let audio_ms = pcm.len() as u64 * 1_000 / u64::from(whisper::SAMPLE_RATE);
    println!(
        "decoded {} of audio in {}",
        mmss(audio_ms),
        mmss(started.elapsed().as_millis() as u64)
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(900))
        .build()
        .map_err(|e| e.to_string())?;
    let url = args.url.as_deref().unwrap_or(whisper::DEFAULT_URL);
    let segments = whisper::translate(&client, url, &api_key, &pcm, |i, n| {
        println!("translating chunk {i}/{n} …");
    })
    .await
    .map_err(|e| e.to_string())?;

    let shift_ms = match args.lag {
        Lag::Fixed(ms) => ms,
        Lag::Auto => 0,
    };
    let written = write_outputs(&args.out, &format!("{stem}.whisper"), &segments, shift_ms)?;
    let minutes = audio_ms as f64 / 60_000.0;
    println!();
    println!("── summary ─────────────────────────────────────────────");
    println!(
        "audio       {} · wall {}",
        mmss(audio_ms),
        mmss(started.elapsed().as_millis() as u64)
    );
    println!(
        "segments    {} · EN chars {}",
        segments.len(),
        segments
            .iter()
            .map(|s| s.text.chars().count())
            .sum::<usize>()
    );
    println!(
        "est. cost   ${:.2} ({minutes:.1} min × ${WHISPER_USD_PER_MINUTE}/min)",
        minutes * WHISPER_USD_PER_MINUTE
    );
    for path in &written {
        println!("wrote       {}", path.display());
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Shared helpers

fn write_outputs(
    out: &Path,
    stem: &str,
    segments: &[Segment],
    shift_ms: i64,
) -> Result<Vec<PathBuf>, String> {
    let options = CaptionOptions {
        shift_ms,
        ..CaptionOptions::default()
    };
    let cues = captions::build_cues(segments, &options);
    let files = [
        (
            format!("{stem}.segments.json"),
            serde_json::to_string_pretty(segments).map_err(|e| e.to_string())?,
        ),
        (format!("{stem}.en.vtt"), captions::render_vtt(&cues)),
        (format!("{stem}.en.srt"), captions::render_srt(&cues)),
    ];
    let mut written = Vec::new();
    for (name, contents) in files {
        let path = out.join(name);
        std::fs::write(&path, contents)
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

fn write_log(log: &mut BufWriter<File>, value: &Value) {
    if let Err(error) = writeln!(log, "{value}") {
        eprintln!("metocast-transcribe: the events log could not be written: {error}");
    }
}

fn output_stem(args: &Args) -> String {
    let stem = args.input.file_stem().map_or_else(
        || "input".to_string(),
        |s| s.to_string_lossy().replace(' ', "_"),
    );
    match (args.start > 0.0, args.duration) {
        (false, None) => stem,
        (_, duration) => format!(
            "{stem}_{}s-{}",
            args.start as u64,
            duration.map_or_else(|| "end".to_string(), |d| format!("{}s", d as u64))
        ),
    }
}

fn percentile(values: &[u64], p: usize) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((sorted.len() - 1) * p).div_ceil(100);
    sorted.get(index).copied()
}

fn fmt_ms(ms: Option<u64>) -> String {
    ms.map_or_else(
        || "n/a".to_string(),
        |ms| format!("{:.1}s", ms as f64 / 1_000.0),
    )
}

fn mmss(ms: u64) -> String {
    let seconds = ms / 1_000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

struct TerminalView {
    tty: bool,
    last_progress_ms: u64,
}

impl TerminalView {
    fn new() -> Self {
        Self {
            tty: std::io::stdout().is_terminal(),
            last_progress_ms: 0,
        }
    }

    fn clear_line(&self) {
        if self.tty {
            print!("\r\x1b[2K");
        }
    }

    fn committed(&mut self, segment: &Segment) {
        self.clear_line();
        println!("[{}] {}", mmss(segment.start_ms), segment.text);
    }

    fn partial(&mut self, text: &str) {
        if !self.tty {
            return;
        }
        let tail: String = {
            let chars: Vec<char> = text.chars().collect();
            chars[chars.len().saturating_sub(110)..].iter().collect()
        };
        print!("\r\x1b[2K\x1b[2m… {tail}\x1b[0m");
        if let Err(error) = std::io::stdout().flush() {
            eprintln!("metocast-transcribe: stdout could not be flushed: {error}");
        }
    }

    fn progress(&mut self, at_ms: u64) {
        if !self.tty && at_ms >= self.last_progress_ms + 60_000 {
            println!("-- audio {}", mmss(at_ms));
            self.last_progress_ms = at_ms;
        }
    }

    fn note(&mut self, message: &str) {
        self.clear_line();
        eprintln!("{message}");
    }
}

fn read_api_key(file: Option<&Path>) -> Result<String, String> {
    let key = if let Some(path) = file {
        std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read API key file {}: {e}", path.display()))?
    } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        key
    } else {
        let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
        let path = Path::new(&home).join(".config/metocast/openai.key");
        std::fs::read_to_string(&path).map_err(|_| {
            format!(
                "no API key: set OPENAI_API_KEY or create {} (chmod 600)",
                path.display()
            )
        })?
    };
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("the API key is empty".into());
    }
    if !key.starts_with("sk-") || key.chars().any(char::is_whitespace) {
        return Err(
            "the API key file doesn't contain an OpenAI key (expected a single sk-… token)".into(),
        );
    }
    Ok(key)
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        input: PathBuf::new(),
        mode: Mode::Realtime,
        start: 0.0,
        duration: None,
        speed: 1.0,
        lag: Lag::Auto,
        out: PathBuf::from("out"),
        api_key_file: None,
        serve: None,
        url: None,
        segmenter: SegmenterConfig::default(),
    };
    let mut input = None;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = || {
            it.next()
                .ok_or_else(|| format!("{arg} needs a value\n\n{USAGE}"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Err(USAGE.to_string()),
            "--mode" => {
                args.mode = match value()?.as_str() {
                    "realtime" => Mode::Realtime,
                    "whisper" => Mode::Whisper,
                    other => return Err(format!("unknown mode {other}")),
                }
            }
            "--start" => args.start = parse_time(&value()?)?,
            "--duration" => args.duration = Some(parse_time(&value()?)?),
            "--speed" => {
                args.speed = value()?
                    .parse()
                    .map_err(|_| "--speed must be a number".to_string())?;
                if args.speed <= 0.0 {
                    return Err("--speed must be positive".into());
                }
            }
            "--lag-ms" => {
                let v = value()?;
                args.lag = if v == "auto" {
                    Lag::Auto
                } else {
                    Lag::Fixed(
                        v.parse()
                            .map_err(|_| "--lag-ms must be auto or an integer".to_string())?,
                    )
                };
            }
            "--out" => args.out = PathBuf::from(value()?),
            "--api-key-file" => args.api_key_file = Some(PathBuf::from(value()?)),
            "--serve" => {
                args.serve = Some(
                    value()?
                        .parse()
                        .map_err(|_| "--serve needs HOST:PORT".to_string())?,
                )
            }
            "--url" => args.url = Some(value()?),
            "--gap-ms" => {
                args.segmenter.gap_ms = value()?
                    .parse()
                    .map_err(|_| "--gap-ms must be an integer".to_string())?
            }
            "--max-chars" => {
                args.segmenter.max_chars = value()?
                    .parse()
                    .map_err(|_| "--max-chars must be an integer".to_string())?
            }
            flag if flag.starts_with("--") => {
                return Err(format!("unknown option {flag}\n\n{USAGE}"));
            }
            _ => input = Some(PathBuf::from(&arg)),
        }
    }
    args.input = input.ok_or_else(|| USAGE.to_string())?;
    Ok(args)
}

fn parse_time(value: &str) -> Result<f64, String> {
    value
        .split(':')
        .try_fold(0.0, |total, part| {
            part.parse::<f64>().map(|n| total * 60.0 + n)
        })
        .map_err(|_| format!("invalid time {value}"))
}
