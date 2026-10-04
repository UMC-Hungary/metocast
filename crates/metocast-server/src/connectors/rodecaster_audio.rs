use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;

use chrono::{DateTime, Utc};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, InterfaceType, Sample, SampleFormat, SupportedStreamConfig};
use flac_bound::FlacEncoder;
use serde::Serialize;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::{
    RecorderStatus, RodecasterAudioOutputMode, RodecasterAudioRecordingConfig,
    RodecasterAudioSource,
};

const FLAC_CHANNEL_LIMIT: usize = 8;
const FLAC_BITS_PER_SAMPLE: u32 = 24;
const QUEUE_CAPACITY: usize = 128;
const COMBINED_GAIN_NUMERATOR: i64 = 1;
const COMBINED_GAIN_DENOMINATOR: i64 = 8;
const I24_MIN: i64 = -(1 << 23);
const I24_MAX: i64 = (1 << 23) - 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AudioLayout {
    Stereo,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioSourceMapping {
    pub identity: RodecasterAudioSource,
    pub label: String,
    pub layout: AudioLayout,
    /// Zero-based channel indexes in the host's interleaved input stream.
    pub host_channels: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioEndpoint {
    pub id: String,
    pub name: String,
    pub model: String,
    pub firmware: String,
    pub sample_rate: u32,
    pub channels: usize,
    pub sample_format: String,
    pub sources: Vec<AudioSourceMapping>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AudioDiscovery {
    pub endpoint: Option<AudioEndpoint>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FinalizedAudioFile {
    pub path: String,
    pub file_name: String,
    pub file_size: u64,
    pub duration_seconds: f64,
    pub media_kind: String,
    pub source: String,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioRecorderState {
    pub status: RecorderStatus,
    pub session_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
    pub output_mode: Option<RodecasterAudioOutputMode>,
    pub started_at: Option<DateTime<Utc>>,
    pub endpoint: Option<AudioEndpoint>,
    pub error: Option<String>,
    pub partial_files: Vec<String>,
    pub finalized_files: Vec<FinalizedAudioFile>,
}

impl Default for AudioRecorderState {
    fn default() -> Self {
        Self {
            status: RecorderStatus::Idle,
            session_id: None,
            event_id: None,
            output_mode: None,
            started_at: None,
            endpoint: None,
            error: None,
            partial_files: Vec::new(),
            finalized_files: Vec::new(),
        }
    }
}

pub struct RodecasterAudioRecorder {
    state: Arc<RwLock<AudioRecorderState>>,
    session: Mutex<Option<ActiveSession>>,
    pub state_tx: broadcast::Sender<AudioRecorderState>,
}

impl RodecasterAudioRecorder {
    #[must_use]
    pub fn new() -> Self {
        let (state_tx, _) = broadcast::channel(32);
        Self {
            state: Arc::new(RwLock::new(AudioRecorderState::default())),
            session: Mutex::new(None),
            state_tx,
        }
    }

    #[must_use]
    pub fn state(&self) -> AudioRecorderState {
        self.state.read().expect("recorder state lock").clone()
    }

    #[must_use]
    pub fn discover(&self, profile: &rodecaster::Profile) -> AudioDiscovery {
        match select_endpoint(profile) {
            Ok(selected) => AudioDiscovery {
                endpoint: Some(selected.endpoint),
                unavailable_reason: None,
            },
            Err(reason) => AudioDiscovery {
                endpoint: None,
                unavailable_reason: Some(reason),
            },
        }
    }

    /// Starts one core-owned capture. Repeated starts return the active state.
    ///
    /// # Errors
    /// Returns an actionable validation, discovery, stream, or writer error.
    pub fn start(
        &self,
        event_id: Uuid,
        config: &RodecasterAudioRecordingConfig,
        profile: &rodecaster::Profile,
    ) -> Result<AudioRecorderState, String> {
        let mut session_guard = self.session.lock().expect("recorder session lock");
        if session_guard.is_some() {
            return Ok(self.state());
        }
        validate_config(config)?;
        let selected = select_endpoint(profile)?;
        let selected_sources = resolve_sources(config, &selected.endpoint)?;
        let session_id = Uuid::new_v4();
        let started_at = Utc::now();
        let directory = Path::new(&config.directory)
            .canonicalize()
            .map_err(|e| format!("recording_directory_unavailable: {e}"))?;
        let plans = writer_plans(
            &directory,
            session_id,
            started_at,
            config,
            &selected_sources,
            &selected.endpoint,
        )?;
        let partial_files = plans
            .iter()
            .map(|plan| plan.partial.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let (samples_tx, samples_rx) = mpsc::sync_channel(QUEUE_CAPACITY);
        let failed = Arc::new(AtomicBool::new(false));
        let stream = build_input_stream(
            &selected.device,
            selected.config,
            samples_tx.clone(),
            Arc::clone(&failed),
            Arc::clone(&self.state),
            self.state_tx.clone(),
        )?;
        let (ready_tx, ready_rx) = mpsc::channel();
        let writer_failed = Arc::clone(&failed);
        let writer_state = Arc::clone(&self.state);
        let writer_state_tx = self.state_tx.clone();
        let input_channels = selected.endpoint.channels;
        let sample_rate = selected.endpoint.sample_rate;
        let writer = std::thread::Builder::new()
            .name("rodecaster-flac".into())
            .spawn(move || {
                let result = writer_loop(
                    plans,
                    samples_rx,
                    input_channels,
                    sample_rate,
                    writer_failed,
                    ready_tx,
                );
                if let Err(error) = &result {
                    mark_failed_once(&failed, &writer_state, &writer_state_tx, error);
                }
                result
            })
            .map_err(|e| format!("recording_writer_unavailable: {e}"))?;

        let ready = ready_rx
            .recv()
            .unwrap_or_else(|_| Err("recording_writer_unavailable".to_string()));
        if let Err(error) = ready {
            drop(stream);
            drop(samples_tx);
            let _ = writer.join();
            remove_failed_start_files(&partial_files);
            return Err(error);
        }
        if let Err(error) = stream.play() {
            drop(stream);
            drop(samples_tx);
            let _ = writer.join();
            remove_failed_start_files(&partial_files);
            return Err(format!("recording_stream_start_failed: {error}"));
        }

        let state = AudioRecorderState {
            status: RecorderStatus::Recording,
            session_id: Some(session_id),
            event_id: Some(event_id),
            output_mode: Some(config.output_mode),
            started_at: Some(started_at),
            endpoint: Some(selected.endpoint),
            error: None,
            partial_files,
            finalized_files: Vec::new(),
        };
        *self.state.write().expect("recorder state lock") = state.clone();
        let _ = self.state_tx.send(state.clone());
        *session_guard = Some(ActiveSession {
            stream,
            samples_tx,
            writer,
        });
        Ok(state)
    }

    /// Stops and finalizes the active capture. Repeated stops are idempotent.
    ///
    /// # Errors
    /// Returns a writer or finalization failure; non-empty partials remain recoverable.
    pub fn stop(&self) -> Result<Vec<FinalizedAudioFile>, String> {
        let session = self.session.lock().expect("recorder session lock").take();
        let Some(session) = session else {
            return Ok(self.state().finalized_files);
        };
        drop(session.stream);
        drop(session.samples_tx);
        let result = session
            .writer
            .join()
            .map_err(|_| "recording_writer_panicked".to_string())?;
        match result {
            Ok(files) => {
                let mut state = self.state.write().expect("recorder state lock");
                state.status = RecorderStatus::Idle;
                state.error = None;
                state.partial_files.clear();
                state.finalized_files.clone_from(&files);
                let snapshot = state.clone();
                drop(state);
                let _ = self.state_tx.send(snapshot);
                Ok(files)
            }
            Err(error) => {
                let mut state = self.state.write().expect("recorder state lock");
                state.status = RecorderStatus::Failed;
                state.error = Some(error.clone());
                let snapshot = state.clone();
                drop(state);
                let _ = self.state_tx.send(snapshot);
                Err(error)
            }
        }
    }
}

impl Default for RodecasterAudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

struct ActiveSession {
    stream: cpal::Stream,
    samples_tx: SyncSender<Vec<i32>>,
    writer: JoinHandle<Result<Vec<FinalizedAudioFile>, String>>,
}

struct SelectedEndpoint {
    device: cpal::Device,
    config: SupportedStreamConfig,
    endpoint: AudioEndpoint,
}

fn select_endpoint(profile: &rodecaster::Profile) -> Result<SelectedEndpoint, String> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| format!("audio_device_discovery_failed: {e}"))?;
    let mut candidates = Vec::new();
    for device in devices {
        let Ok(description) = device.description() else {
            continue;
        };
        let name = description.name().to_string();
        let normalized = name.to_lowercase();
        let is_rodecaster = normalized.contains("rødecaster")
            || normalized.contains("rodecaster")
            || (normalized.contains("rode") && normalized.contains("caster"));
        let is_usb = matches!(
            description.interface_type(),
            InterfaceType::Usb | InterfaceType::Unknown
        );
        if !is_rodecaster || !is_usb {
            continue;
        }
        let config = device.supported_input_configs().ok().and_then(|configs| {
            configs
                .filter(|range| range.channels() > 2 && pcm_format(range.sample_format()))
                .max_by_key(|range| {
                    (
                        range.channels(),
                        range.contains_rate(cpal::SAMPLE_RATE_48K),
                        sample_format_rank(range.sample_format()),
                    )
                })
                .map(|range| {
                    range
                        .try_with_standard_sample_rate()
                        .unwrap_or_else(|| range.with_max_sample_rate())
                })
        });
        if let Some(config) = config {
            candidates.push((device, description, config));
        }
    }
    if candidates.is_empty() {
        return Err(
            "rodecaster_multitrack_endpoint_missing: connect USB 1 and enable multitrack"
                .to_string(),
        );
    }
    if candidates.len() != 1 {
        let names = candidates
            .iter()
            .map(|(_, description, _)| description.name())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("rodecaster_multitrack_endpoint_ambiguous: {names}"));
    }
    let (device, description, config) = candidates.remove(0);
    let channels = usize::from(config.channels());
    let mut sources = vec![AudioSourceMapping {
        identity: RodecasterAudioSource::MainMix,
        label: "Main Mix".to_string(),
        layout: AudioLayout::Stereo,
        host_channels: vec![0, 1],
    }];
    sources.extend(profile.channels.iter().filter_map(|channel| {
        let first = 2 + channel.channel * 2;
        (first + 1 < channels).then(|| AudioSourceMapping {
            identity: RodecasterAudioSource::FaderSlot {
                number: channel.channel + 1,
            },
            label: channel.label.clone(),
            layout: AudioLayout::Stereo,
            host_channels: vec![first, first + 1],
        })
    }));
    let id = device
        .id()
        .map_err(|e| format!("audio_device_identity_failed: {e}"))?
        .to_string();
    let endpoint = AudioEndpoint {
        id,
        name: description.name().to_string(),
        model: profile.model.clone(),
        firmware: profile.firmware.clone(),
        sample_rate: config.sample_rate(),
        channels,
        sample_format: config.sample_format().to_string(),
        sources,
    };
    Ok(SelectedEndpoint {
        device,
        config,
        endpoint,
    })
}

fn pcm_format(format: SampleFormat) -> bool {
    matches!(
        format,
        SampleFormat::I16
            | SampleFormat::I24
            | SampleFormat::I32
            | SampleFormat::F32
            | SampleFormat::F64
    )
}

fn sample_format_rank(format: SampleFormat) -> u8 {
    match format {
        SampleFormat::I24 => 5,
        SampleFormat::F32 => 4,
        SampleFormat::I32 => 3,
        SampleFormat::I16 => 2,
        SampleFormat::F64 => 1,
        _ => 0,
    }
}

pub fn validate_config(config: &RodecasterAudioRecordingConfig) -> Result<(), String> {
    if config.schema_version != 1 {
        return Err("unsupported_audio_recording_schema".to_string());
    }
    if !config.enabled {
        return Err("rodecaster_audio_recording_disabled".to_string());
    }
    let directory = Path::new(&config.directory);
    if !directory.is_dir() {
        return Err("recording_directory_not_found".to_string());
    }
    let probe = directory.join(format!(".metocast-write-check-{}", Uuid::new_v4()));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .map_err(|e| format!("recording_directory_not_writable: {e}"))?;
    fs::remove_file(&probe).map_err(|e| format!("recording_directory_not_writable: {e}"))?;

    if config.output_mode == RodecasterAudioOutputMode::MainMix {
        return Ok(());
    }
    if config.sources.is_empty() {
        return Err("recording_sources_required".to_string());
    }
    let mut slots = HashSet::new();
    for source in &config.sources {
        match source {
            RodecasterAudioSource::MainMix => {
                return Err("main_mix_cannot_be_combined_with_fader_sources".to_string());
            }
            RodecasterAudioSource::FaderSlot { number } if *number == 0 => {
                return Err("fader_slot_must_be_one_based".to_string());
            }
            RodecasterAudioSource::FaderSlot { number } if !slots.insert(*number) => {
                return Err("recording_sources_must_be_unique".to_string());
            }
            RodecasterAudioSource::FaderSlot { .. } => {}
        }
    }
    if config.output_mode == RodecasterAudioOutputMode::MultichannelFlac
        && config.sources.len() * 2 > FLAC_CHANNEL_LIMIT
    {
        return Err("multichannel_flac_supports_at_most_8_channels; use separate".to_string());
    }
    Ok(())
}

fn resolve_sources(
    config: &RodecasterAudioRecordingConfig,
    endpoint: &AudioEndpoint,
) -> Result<Vec<AudioSourceMapping>, String> {
    if config.output_mode == RodecasterAudioOutputMode::MainMix {
        return endpoint
            .sources
            .iter()
            .find(|source| source.identity == RodecasterAudioSource::MainMix)
            .cloned()
            .map(|source| vec![source])
            .ok_or_else(|| "rodecaster_main_mix_mapping_missing".to_string());
    }
    config
        .sources
        .iter()
        .map(|identity| {
            endpoint
                .sources
                .iter()
                .find(|source| &source.identity == identity)
                .cloned()
                .ok_or_else(|| "configured_rodecaster_source_is_unavailable".to_string())
        })
        .collect()
}

#[derive(Clone)]
enum Route {
    Channels(Vec<usize>),
    CombinedStereo(Vec<[usize; 2]>),
}

struct WriterPlan {
    partial: PathBuf,
    final_path: PathBuf,
    route: Route,
    channels: usize,
    media_kind: &'static str,
    metadata: serde_json::Value,
}

fn writer_plans(
    directory: &Path,
    session_id: Uuid,
    started_at: DateTime<Utc>,
    config: &RodecasterAudioRecordingConfig,
    sources: &[AudioSourceMapping],
    endpoint: &AudioEndpoint,
) -> Result<Vec<WriterPlan>, String> {
    let prefix = format!("{}-{session_id}", started_at.format("%Y%m%dT%H%M%SZ"));
    let metadata = |kind: &str, selected: &[AudioSourceMapping]| {
        serde_json::json!({
            "schemaVersion": 1,
            "source": "rodecaster",
            "mediaKind": kind,
            "outputMode": config.output_mode,
            "processingMode": config.processing_mode,
            "endpoint": endpoint,
            "sources": selected,
            "combination": (config.output_mode == RodecasterAudioOutputMode::CombinedStereo).then(|| serde_json::json!({
                "algorithm": "fixedGainHardClip",
                "gainNumerator": COMBINED_GAIN_NUMERATOR,
                "gainDenominator": COMBINED_GAIN_DENOMINATOR,
            })),
        })
    };
    let make = |suffix: &str,
                route: Route,
                channels: usize,
                media_kind: &'static str,
                selected: &[AudioSourceMapping]| {
        let final_path = directory.join(format!("{prefix}-{suffix}.flac"));
        let partial = PathBuf::from(format!("{}.partial", final_path.to_string_lossy()));
        WriterPlan {
            partial,
            final_path,
            route,
            channels,
            media_kind,
            metadata: metadata(media_kind, selected),
        }
    };
    match config.output_mode {
        RodecasterAudioOutputMode::MainMix => Ok(vec![make(
            "main-mix",
            Route::Channels(sources[0].host_channels.clone()),
            2,
            "deviceMix",
            sources,
        )]),
        RodecasterAudioOutputMode::Separate => Ok(sources
            .iter()
            .map(|source| {
                let RodecasterAudioSource::FaderSlot { number } = &source.identity else {
                    unreachable!("validated individual source")
                };
                make(
                    &format!("fader-{number}"),
                    Route::Channels(source.host_channels.clone()),
                    source.host_channels.len(),
                    "deviceTrack",
                    std::slice::from_ref(source),
                )
            })
            .collect()),
        RodecasterAudioOutputMode::CombinedStereo => Ok(vec![make(
            "combined-stereo",
            Route::CombinedStereo(
                sources
                    .iter()
                    .map(|source| [source.host_channels[0], source.host_channels[1]])
                    .collect(),
            ),
            2,
            "derivedMix",
            sources,
        )]),
        RodecasterAudioOutputMode::MultichannelFlac => {
            let channels = sources
                .iter()
                .flat_map(|source| source.host_channels.iter().copied())
                .collect::<Vec<_>>();
            if channels.len() > FLAC_CHANNEL_LIMIT {
                return Err(
                    "multichannel_flac_supports_at_most_8_channels; use separate".to_string(),
                );
            }
            Ok(vec![make(
                "multichannel",
                Route::Channels(channels.clone()),
                channels.len(),
                "deviceMultichannel",
                sources,
            )])
        }
    }
}

fn build_input_stream(
    device: &cpal::Device,
    supported: SupportedStreamConfig,
    samples_tx: SyncSender<Vec<i32>>,
    failed: Arc<AtomicBool>,
    state: Arc<RwLock<AudioRecorderState>>,
    state_tx: broadcast::Sender<AudioRecorderState>,
) -> Result<cpal::Stream, String> {
    let stream_config = supported.config();
    macro_rules! stream {
        ($sample:ty) => {{
            let tx = samples_tx.clone();
            let callback_failed = Arc::clone(&failed);
            let callback_state = Arc::clone(&state);
            let callback_state_tx = state_tx.clone();
            let error_failed = Arc::clone(&failed);
            let error_state = Arc::clone(&state);
            let error_state_tx = state_tx.clone();
            device.build_input_stream::<$sample, _, _>(
                stream_config,
                move |samples, _| {
                    let converted = samples.iter().copied().map(to_i24).collect();
                    if let Err(TrySendError::Full(_)) = tx.try_send(converted) {
                        mark_failed_once(
                            &callback_failed,
                            &callback_state,
                            &callback_state_tx,
                            "recording_input_overrun",
                        );
                    }
                },
                move |error| {
                    mark_failed_once(
                        &error_failed,
                        &error_state,
                        &error_state_tx,
                        &format!("recording_stream_failed: {error}"),
                    );
                },
                None,
            )
        }};
    }
    let stream = match supported.sample_format() {
        SampleFormat::I16 => stream!(i16),
        SampleFormat::I24 => stream!(cpal::I24),
        SampleFormat::I32 => stream!(i32),
        SampleFormat::F32 => stream!(f32),
        SampleFormat::F64 => stream!(f64),
        other => return Err(format!("unsupported_audio_sample_format: {other}")),
    };
    stream.map_err(|e| format!("recording_stream_open_failed: {e}"))
}

fn to_i24<T>(sample: T) -> i32
where
    i32: FromSample<T>,
{
    i32::from_sample(sample) >> 8
}

fn mark_failed_once(
    failed: &AtomicBool,
    state: &RwLock<AudioRecorderState>,
    state_tx: &broadcast::Sender<AudioRecorderState>,
    error: &str,
) {
    if failed.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut state = state.write().expect("recorder state lock");
    state.status = RecorderStatus::Failed;
    state.error = Some(error.to_string());
    let snapshot = state.clone();
    drop(state);
    let _ = state_tx.send(snapshot);
}

fn writer_loop(
    plans: Vec<WriterPlan>,
    samples_rx: Receiver<Vec<i32>>,
    input_channels: usize,
    sample_rate: u32,
    failed: Arc<AtomicBool>,
    ready_tx: mpsc::Sender<Result<(), String>>,
) -> Result<Vec<FinalizedAudioFile>, String> {
    let mut writers = match plans
        .into_iter()
        .map(|plan| FlacWriter::new(plan, sample_rate))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(writers) => writers,
        Err(error) => {
            let _ = ready_tx.send(Err(error.clone()));
            return Err(error);
        }
    };
    let _ = ready_tx.send(Ok(()));
    for samples in samples_rx {
        if failed.load(Ordering::SeqCst) {
            return Err("recording_failed; partial files retained".to_string());
        }
        for writer in &mut writers {
            writer.process(&samples, input_channels)?;
        }
    }
    if failed.load(Ordering::SeqCst) {
        return Err("recording_failed; partial files retained".to_string());
    }
    writers
        .into_iter()
        .map(FlacWriter::finish)
        .collect::<Result<Vec<_>, _>>()
}

struct FlacWriter {
    encoder: FlacEncoder<'static>,
    plan: WriterPlan,
    sample_rate: u32,
    frames: u64,
}

impl FlacWriter {
    fn new(plan: WriterPlan, sample_rate: u32) -> Result<Self, String> {
        if plan.partial.exists() {
            return Err("recording_file_already_exists".to_string());
        }
        if plan.partial.to_str().is_none() {
            return Err("recording_filename_invalid".to_string());
        }
        let channels =
            u32::try_from(plan.channels).map_err(|_| "flac_channel_count_invalid".to_string())?;
        let encoder = FlacEncoder::new()
            .ok_or_else(|| "flac_encoder_init_failed".to_string())?
            .channels(channels)
            .bits_per_sample(FLAC_BITS_PER_SAMPLE)
            .sample_rate(sample_rate)
            .compression_level(0)
            .init_file(&plan.partial)
            .map_err(|error| format!("flac_encoder_init_failed: {error:?}"))?;
        Ok(Self {
            encoder,
            plan,
            sample_rate,
            frames: 0,
        })
    }

    fn process(&mut self, input: &[i32], input_channels: usize) -> Result<(), String> {
        let output = route_samples(&self.plan.route, input, input_channels)?;
        let frames = output.len() / self.plan.channels;
        self.encoder
            .process_interleaved(
                &output,
                u32::try_from(frames).map_err(|_| "audio_buffer_too_large".to_string())?,
            )
            .map_err(|()| format!("flac_encode_failed: {:?}", self.encoder.state()))?;
        self.frames += frames as u64;
        Ok(())
    }

    fn finish(self) -> Result<FinalizedAudioFile, String> {
        let FlacWriter {
            encoder,
            plan,
            sample_rate,
            frames,
        } = self;
        encoder.finish().map_err(|encoder| {
            format!(
                "flac_finalize_failed: {:?}; partial retained",
                encoder.state()
            )
        })?;
        File::open(&plan.partial)
            .and_then(|file| file.sync_all())
            .map_err(|e| format!("flac_sync_failed: {e}; partial retained"))?;
        fs::rename(&plan.partial, &plan.final_path)
            .map_err(|e| format!("flac_rename_failed: {e}; partial retained"))?;
        let file_size = fs::metadata(&plan.final_path)
            .map_err(|e| format!("flac_metadata_failed: {e}"))?
            .len();
        let file_name = plan
            .final_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "recording_filename_invalid".to_string())?
            .to_string();
        Ok(FinalizedAudioFile {
            path: plan.final_path.to_string_lossy().into_owned(),
            file_name,
            file_size,
            duration_seconds: frames as f64 / f64::from(sample_rate),
            media_kind: plan.media_kind.to_string(),
            source: "rodecaster".to_string(),
            metadata: plan.metadata,
        })
    }
}

fn route_samples(route: &Route, input: &[i32], input_channels: usize) -> Result<Vec<i32>, String> {
    if input_channels == 0 || !input.len().is_multiple_of(input_channels) {
        return Err("unaligned_audio_buffer".to_string());
    }
    let frames = input.len() / input_channels;
    match route {
        Route::Channels(channels) => {
            if channels.iter().any(|channel| *channel >= input_channels) {
                return Err("audio_channel_mapping_out_of_range".to_string());
            }
            let mut output = Vec::with_capacity(frames * channels.len());
            for frame in input.chunks_exact(input_channels) {
                output.extend(channels.iter().map(|channel| frame[*channel]));
            }
            Ok(output)
        }
        Route::CombinedStereo(pairs) => {
            if pairs
                .iter()
                .flatten()
                .any(|channel| *channel >= input_channels)
            {
                return Err("audio_channel_mapping_out_of_range".to_string());
            }
            let mut output = Vec::with_capacity(frames * 2);
            for frame in input.chunks_exact(input_channels) {
                for side in 0..2 {
                    let sum = pairs.iter().fold(0_i64, |sum, pair| {
                        sum + i64::from(frame[pair[side]]) * COMBINED_GAIN_NUMERATOR
                            / COMBINED_GAIN_DENOMINATOR
                    });
                    output.push(sum.clamp(I24_MIN, I24_MAX) as i32);
                }
            }
            Ok(output)
        }
    }
}

fn remove_failed_start_files(paths: &[String]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_channels_keep_frame_alignment_and_order() {
        let input = vec![10, 11, 20, 21, 30, 31, 40, 41];
        assert_eq!(
            route_samples(&Route::Channels(vec![2, 3, 0]), &input, 4),
            Ok(vec![20, 21, 10, 40, 41, 30])
        );
    }

    #[test]
    fn combined_stereo_has_fixed_headroom_and_hard_clips() {
        let full = I24_MAX as i32;
        let input = vec![full; 18];
        let pairs = (0..9).map(|slot| [slot * 2, slot * 2 + 1]).collect();
        assert_eq!(
            route_samples(&Route::CombinedStereo(pairs), &input, 18),
            Ok(vec![I24_MAX as i32, I24_MAX as i32])
        );
    }

    #[test]
    fn multichannel_limit_is_rejected_without_dropping_sources() {
        let directory = std::env::temp_dir();
        let config = RodecasterAudioRecordingConfig {
            schema_version: 1,
            enabled: true,
            directory: directory.to_string_lossy().into_owned(),
            processing_mode: crate::connectors::RodecasterProcessingMode::PreFader,
            output_mode: RodecasterAudioOutputMode::MultichannelFlac,
            sources: (1..=5)
                .map(|number| RodecasterAudioSource::FaderSlot { number })
                .collect(),
        };
        assert_eq!(
            validate_config(&config),
            Err("multichannel_flac_supports_at_most_8_channels; use separate".to_string())
        );
    }

    #[test]
    fn repeated_sources_are_rejected() {
        let directory = std::env::temp_dir();
        let config = RodecasterAudioRecordingConfig {
            schema_version: 1,
            enabled: true,
            directory: directory.to_string_lossy().into_owned(),
            processing_mode: crate::connectors::RodecasterProcessingMode::PreFader,
            output_mode: RodecasterAudioOutputMode::Separate,
            sources: vec![
                RodecasterAudioSource::FaderSlot { number: 2 },
                RodecasterAudioSource::FaderSlot { number: 2 },
            ],
        };
        assert_eq!(
            validate_config(&config),
            Err("recording_sources_must_be_unique".to_string())
        );
    }

    #[test]
    fn main_mix_does_not_require_sources() {
        let directory = std::env::temp_dir();
        let config = RodecasterAudioRecordingConfig {
            schema_version: 1,
            enabled: true,
            directory: directory.to_string_lossy().into_owned(),
            processing_mode: crate::connectors::RodecasterProcessingMode::PreFader,
            output_mode: RodecasterAudioOutputMode::MainMix,
            sources: Vec::new(),
        };
        assert_eq!(validate_config(&config), Ok(()));
    }

    #[test]
    fn one_to_eight_channel_flac_encoders_initialize() {
        let root = std::env::temp_dir().join(format!("metocast-flac-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).expect("create test directory");
        for channels in 1..=FLAC_CHANNEL_LIMIT {
            let partial = root.join(format!("{channels}.flac.partial"));
            let plan = WriterPlan {
                final_path: root.join(format!("{channels}.flac")),
                partial,
                route: Route::Channels((0..channels).collect()),
                channels,
                media_kind: "test",
                metadata: serde_json::Value::Null,
            };
            let mut writer = FlacWriter::new(plan, 48_000).expect("valid encoder");
            writer
                .process(&vec![0; channels * 16], channels)
                .expect("encode silence");
            let file = writer.finish().expect("finalize FLAC");
            assert!(file.file_size > 4);
        }
        fs::remove_dir_all(root).expect("remove test directory");
    }
}
