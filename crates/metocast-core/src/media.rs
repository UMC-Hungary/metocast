//! Media processing: the stored grade, output and audio settings (`media` connector config),
//! the records of the `/api/media` routes, and the framing rule they share.
//!
//! The defaults are what the church's Blackmagic Cinema Camera 6K records: Blackmagic Design
//! Film gamma and gamut, the clip's embedded "Gen 5 Film to Extended Video" LUT, and a 16:9
//! crop of the 6048×4032 active area for YouTube.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::connectors::ConnectorConfig;

/// Which 3D LUT the decoder applies after the RAW development.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LutMode {
    /// The LUT the camera stored in the clip.
    #[default]
    Embedded,
    /// A `.cube` file, such as one exported from Resolve with Generate LUT.
    File,
    /// The LUT carried by the settings' `.sidecar` (or the clip's own).
    Sidecar,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutputAspect {
    /// A centred 16:9 crop of the active area: what the YouTube player shows without bars.
    #[default]
    #[serde(rename = "16:9")]
    Widescreen,
    /// The whole active area.
    #[serde(rename = "native")]
    Native,
}

/// `GET`/`PUT /api/connectors/media/config`. Text fields stay strings because the generic
/// connector form sends them that way; [`MediaConfig::settings`] validates and types them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MediaConfig {
    pub enabled: bool,
    /// Empty: the Homebrew install.
    pub ffmpeg_path: String,
    /// Folder holding `BlackmagicRawAPI.framework`. Empty: the Blackmagic RAW install.
    pub braw_sdk_path: String,
    /// Empty: `Metocast Renders` beside the source file.
    pub output_folder: String,
    pub gamma: String,
    pub gamut: String,
    /// 0 keeps the value the clip was shot with.
    pub iso: u16,
    /// 0 keeps the value the clip was shot with.
    pub kelvin: u16,
    /// Empty keeps the value the clip was shot with.
    pub tint: String,
    /// Stops; empty keeps the value the clip was shot with.
    pub exposure: String,
    pub highlight_recovery: bool,
    pub gamut_compression: bool,
    pub lut_mode: LutMode,
    pub lut_path: String,
    pub aspect: OutputAspect,
    /// Pixels to move the crop down (negative: up) from centre.
    pub crop_shift_y: String,
    pub bitrate_mbps: u16,
    /// 1-based source channels mixed into the stereo track, such as `1,2`.
    pub audio_channels: String,
    /// Integrated loudness of a processed audio master, in LUFS. YouTube plays at −14.
    pub audio_target_lufs: String,
    /// Where processed audio lands. Empty: `Metocast Audio` in the user's Music folder, since
    /// the source is often a wireless mic's own small storage.
    pub audio_output_folder: String,
    /// Empty: the copy Metocast downloads on first use.
    pub jive_vocals_path: String,
    /// A Blackmagic RAW `.sidecar` (such as Resolve's Update Sidecar) applied before the
    /// settings above. Empty: none.
    pub sidecar_path: String,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ffmpeg_path: String::new(),
            braw_sdk_path: String::new(),
            output_folder: String::new(),
            gamma: "Blackmagic Design Film".to_string(),
            gamut: "Blackmagic Design".to_string(),
            iso: 0,
            kelvin: 0,
            tint: String::new(),
            exposure: String::new(),
            highlight_recovery: false,
            gamut_compression: false,
            lut_mode: LutMode::Embedded,
            lut_path: String::new(),
            aspect: OutputAspect::Widescreen,
            crop_shift_y: "0".to_string(),
            bitrate_mbps: 100,
            audio_channels: "1,2".to_string(),
            audio_target_lufs: YOUTUBE_LUFS.to_string(),
            audio_output_folder: String::new(),
            jive_vocals_path: String::new(),
            sidecar_path: String::new(),
        }
    }
}

impl ConnectorConfig for MediaConfig {
    fn is_configured(&self) -> bool {
        self.enabled
    }
}

impl MediaConfig {
    /// The typed settings a render uses, or what is wrong with the stored text.
    pub fn settings(&self) -> Result<RenderSettings, String> {
        let optional = |value: u16| (value != 0).then_some(u32::from(value));
        let settings = RenderSettings {
            gamma: self.gamma.trim().to_string(),
            gamut: self.gamut.trim().to_string(),
            iso: optional(self.iso),
            kelvin: optional(self.kelvin),
            tint: parse_optional(&self.tint, "tint")?,
            exposure: parse_optional(&self.exposure, "exposure")?,
            highlight_recovery: self.highlight_recovery,
            gamut_compression: self.gamut_compression,
            lut_mode: self.lut_mode,
            lut_path: Some(self.lut_path.trim().to_string()).filter(|path| !path.is_empty()),
            aspect: self.aspect,
            crop_shift_y: parse_optional(&self.crop_shift_y, "crop shift")?.unwrap_or(0),
            bitrate_mbps: u32::from(self.bitrate_mbps),
            audio_channels: parse_channels(&self.audio_channels)?,
            sidecar_path: Some(self.sidecar_path.trim().to_string())
                .filter(|path| !path.is_empty()),
        };
        settings.validate()?;
        Ok(settings)
    }

    /// The typed settings an audio job uses, or what is wrong with the stored text.
    pub fn audio_settings(&self) -> Result<AudioSettings, String> {
        let settings = AudioSettings {
            channels: Vec::new(),
            target_lufs: parse_optional(&self.audio_target_lufs, "loudness target")?
                .unwrap_or(YOUTUBE_LUFS),
        };
        settings.validate()?;
        Ok(settings)
    }
}

fn parse_optional<T: std::str::FromStr>(text: &str, field: &str) -> Result<Option<T>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse()
        .map(Some)
        .map_err(|_| format!("invalid {field}: {text}"))
}

fn parse_channels(text: &str) -> Result<Vec<u32>, String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            part.parse::<u32>()
                .ok()
                .filter(|channel| (1..=64).contains(channel))
                .ok_or_else(|| format!("invalid audio channel: {part}"))
        })
        .collect()
}

/// One render's settings: the stored defaults, possibly edited for this job. Stored with the
/// job and with the finished recording, so a render can always be explained later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderSettings {
    pub gamma: String,
    pub gamut: String,
    pub iso: Option<u32>,
    pub kelvin: Option<u32>,
    pub tint: Option<i32>,
    pub exposure: Option<f32>,
    pub highlight_recovery: bool,
    pub gamut_compression: bool,
    pub lut_mode: LutMode,
    pub lut_path: Option<String>,
    pub aspect: OutputAspect,
    pub crop_shift_y: i32,
    pub bitrate_mbps: u32,
    pub audio_channels: Vec<u32>,
    /// A Blackmagic RAW `.sidecar` applied before the settings above.
    #[serde(default)]
    pub sidecar_path: Option<String>,
}

impl RenderSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.gamma.is_empty() || self.gamut.is_empty() {
            return Err("gamma and gamut are required".to_string());
        }
        if self.lut_mode == LutMode::File && self.lut_path.is_none() {
            return Err("a LUT file is required when the LUT mode is file".to_string());
        }
        if !(10..=400).contains(&self.bitrate_mbps) {
            return Err(format!(
                "bitrate must be 10–400 Mbps, not {}",
                self.bitrate_mbps
            ));
        }
        if self.audio_channels.len() > 2 {
            return Err("pick at most two audio channels".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The part of `active` a render keeps. Sizes are even, as 4:2:0 video needs; a 16:9 crop
/// keeps the full width when the picture is taller than 16:9, and the full height otherwise.
#[must_use]
pub fn output_crop(active: CropRect, aspect: OutputAspect, shift_y: i32) -> CropRect {
    let even = |value: u32| value & !1;
    let (width, height) = match aspect {
        OutputAspect::Native => (even(active.width), even(active.height)),
        OutputAspect::Widescreen => {
            let height = even(active.width * 9 / 16);
            if height <= active.height {
                (even(active.width), height)
            } else {
                (even(active.height * 16 / 9), even(active.height))
            }
        }
    };
    let spare_y = i64::from(active.height - height);
    let y = (spare_y / 2 + i64::from(shift_y)).clamp(0, spare_y);
    CropRect {
        x: active.x + (active.width - width) / 2,
        y: active.y + u32::try_from(y).unwrap_or(0),
        width,
        height,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MediaKind {
    /// Blackmagic RAW: developed by the Blackmagic RAW SDK before encoding.
    Braw,
    /// Anything ffmpeg decodes itself, such as an OBS recording.
    Video,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioInfo {
    pub codec: String,
    pub channels: u32,
    pub sample_rate: u32,
    pub bits: Option<u32>,
}

/// `POST /api/media/probe` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeRequest {
    pub path: String,
    /// A `.sidecar` (such as Resolve's) whose settings become the probe's defaults.
    #[serde(default)]
    pub sidecar_path: Option<String>,
}

/// What a file is, how it would be rendered, and what the host still lacks to render it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaProbe {
    pub path: String,
    pub file_name: String,
    pub file_size: u64,
    pub kind: MediaKind,
    pub camera: Option<String>,
    pub clip_name: Option<String>,
    pub width: u32,
    pub height: u32,
    /// The picture without the sensor's border; the whole frame when the file has none.
    pub active_area: CropRect,
    pub fps_num: u32,
    pub fps_den: u32,
    pub frame_count: u64,
    pub duration_seconds: f64,
    /// SMPTE start timecode, such as `10:26:20:15`.
    pub start_timecode: Option<String>,
    /// When the file says it was recorded, by the camera's clock (which may be wrong).
    pub recorded_at: Option<DateTime<Utc>>,
    /// When Metocast saw the camera start this clip, by the server's clock.
    pub host_record_started_at: Option<DateTime<Utc>>,
    pub audio: Option<AudioInfo>,
    pub gamma: Option<String>,
    pub gamut: Option<String>,
    pub embedded_lut: Option<String>,
    /// The stored settings, ready to edit for this render.
    pub defaults: RenderSettings,
    /// The frame those defaults produce.
    pub output: CropRect,
    /// Tools the host lacks for this file: `ffmpeg`, `braw-decode`, `blackmagic-raw-sdk`.
    pub missing: Vec<String>,
    /// What a Blackmagic RAW clip's settings may be set to; absent for other video.
    pub raw_options: Option<RawOptions>,
}

/// The choices the Blackmagic RAW SDK offers for a clip, and what it was shot with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawOptions {
    pub gammas: Vec<String>,
    pub gamuts: Vec<String>,
    pub isos: Vec<u32>,
    pub kelvin: Option<ValueRange>,
    pub tint: Option<ValueRange>,
    pub exposure: Option<ValueRange>,
    pub as_shot: AsShot,
}

impl RawOptions {
    /// Moves `settings` to an ISO this clip offers. A stored ISO meant for the camera's other
    /// gain circuit (800 on a clip shot at 1250) goes to the nearest one the clip has, and the
    /// exposure makes up the difference so the picture keeps its brightness.
    pub fn fit_iso(&self, settings: &mut RenderSettings) {
        let Some(wanted) = settings.iso.filter(|iso| !self.isos.contains(iso)) else {
            return;
        };
        let stops = |iso: u32| f64::from(iso).log2();
        let Some(nearest) = self.isos.iter().copied().min_by(|a, b| {
            (stops(*a) - stops(wanted))
                .abs()
                .total_cmp(&(stops(*b) - stops(wanted)).abs())
        }) else {
            return;
        };
        let exposure = f64::from(settings.exposure.unwrap_or(0.0)) + stops(wanted) - stops(nearest);
        settings.iso = Some(nearest);
        settings.exposure = Some(((exposure * 100.0).round() / 100.0) as f32);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsShot {
    pub iso: Option<u32>,
    pub kelvin: Option<u32>,
    pub tint: Option<i32>,
    pub exposure: Option<f32>,
}

/// `POST /api/media/preview` body: one frame of `path`, as recorded or with `settings`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub path: String,
    /// 0-based.
    pub frame: u64,
    /// The clip as recorded when absent: its own look, uncropped.
    pub settings: Option<RenderSettings>,
    /// The JPEG's widest side; 1600 when absent.
    pub max_width: Option<u32>,
}

/// `POST /api/media/preview` reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFrame {
    /// Base64 JPEG of the whole developed picture (the active area).
    pub image_jpeg: String,
    /// The picture the JPEG shows, in source pixels.
    pub active: CropRect,
    /// The part a render keeps, in the same pixels; the whole picture for an original.
    pub output: CropRect,
}

/// `POST /api/media/renders` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderRequest {
    pub path: String,
    /// The event the finished file is filed under; untracked when absent.
    pub event_id: Option<Uuid>,
    /// The stored defaults when absent.
    pub settings: Option<RenderSettings>,
}

/// `POST /api/media/renders` reply.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderQueued {
    pub job_id: Uuid,
    pub output_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RenderState {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// `media.render.progress`: sent about once a second while a render runs, and once when it ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderProgress {
    pub job_id: Uuid,
    pub state: RenderState,
    pub frame: u64,
    pub total_frames: u64,
    /// Frames encoded per second, averaged over the render so far.
    pub fps: f64,
    pub output_path: String,
    pub error: Option<String>,
}

/// YouTube's playback reference loudness.
pub const YOUTUBE_LUFS: f32 = -14.0;
/// What jive-vocals always produces, measured as a voice heard on both speakers; Metocast's
/// finishing pass moves it to the target.
pub const JIVE_VOCALS_LUFS: f32 = -16.0;
/// Every processed master is 48 kHz, the rate video uses, with the voice on both channels.
pub const AUDIO_SAMPLE_RATE: u32 = 48_000;

/// One audio job's settings: the stored defaults, possibly edited for this job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSettings {
    /// 1-based source channels mixed into the mono voice track; empty mixes them all.
    pub channels: Vec<u32>,
    pub target_lufs: f32,
}

impl AudioSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !(-30.0..=-9.0).contains(&self.target_lufs) {
            return Err(format!(
                "the loudness target must be −30 to −9 LUFS, not {}",
                self.target_lufs
            ));
        }
        if self
            .channels
            .iter()
            .any(|channel| !(1..=64).contains(channel))
        {
            return Err("audio channels are numbered from 1 to 64".to_string());
        }
        Ok(())
    }
}

/// `POST /api/media/audio/probe` body: one recording, or the files a recorder split it into,
/// in playing order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioProbeRequest {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSource {
    pub path: String,
    pub file_name: String,
    pub file_size: u64,
    pub audio: AudioInfo,
    pub duration_seconds: f64,
    /// When the file was created, by the recorder's clock: the start of its audio.
    pub recorded_at: Option<DateTime<Utc>>,
}

/// What a set of files would become, and what the host still lacks to process them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioProbe {
    pub sources: Vec<AudioSource>,
    /// All sources played back to back.
    pub duration_seconds: f64,
    pub channels: u32,
    pub recorded_at: Option<DateTime<Utc>>,
    /// The stored settings, ready to edit for this job.
    pub defaults: AudioSettings,
    /// Tools the host lacks and cannot fetch: `ffmpeg`, `jive-vocals`.
    pub missing: Vec<String>,
    /// Tools the first job downloads before it starts.
    pub downloads: Vec<String>,
}

/// `POST /api/media/audio` body. Replied to with [`RenderQueued`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRequest {
    pub paths: Vec<String>,
    /// The event the finished file is filed under; untracked when absent.
    pub event_id: Option<Uuid>,
    /// The stored defaults when absent.
    pub settings: Option<AudioSettings>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioStep {
    /// Fetching jive-vocals on first use.
    Download,
    /// Joining the files and mixing the chosen channels to mono 48 kHz.
    Prepare,
    /// jive-vocals: rumble, hiss, clicks, sibilance and levelling, normalised to −16 LUFS.
    Clean,
    /// Measuring the delay jive-vocals' noise reduction added.
    Align,
    /// Removing that delay and reaching the loudness target.
    Finish,
}

/// What processing did, kept with the recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioReport {
    pub input_lufs: f64,
    pub output_lufs: f64,
    pub output_true_peak_dbtp: f64,
    /// The delay jive-vocals added and Metocast removed, in 48 kHz samples.
    pub removed_delay_samples: i64,
    /// How clearly the processed audio matched the source (0–1).
    pub alignment_confidence: f64,
}

/// `media.audio.progress`: sent as each step starts, about once a second while a step can
/// count its progress, and once when the job ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioProgress {
    pub job_id: Uuid,
    pub state: RenderState,
    pub step: Option<AudioStep>,
    /// 0–1 through the current step; absent while a step can't tell.
    pub fraction: Option<f64>,
    pub output_path: String,
    pub error: Option<String>,
    /// Set when the job succeeds.
    pub report: Option<AudioReport>,
}

// ── Programs: a video with its audio, synced ─────────────────────────────────

/// What a program's picture goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PictureMode {
    /// Developed (RAW), graded and encoded like a render. Blackmagic RAW always is.
    Grade,
    /// The video stream copied as it is: an existing render or a finished export.
    Copy,
}

/// How a program's audio was lined up with its picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncMethod {
    /// The video's own sound is the audio: nothing to find.
    None,
    /// The video's own sound matched against the recording.
    Waveform,
    /// The speaker's lip movement matched against the voice.
    Lips,
    /// Only the clocks, because nothing matched clearly.
    Clock,
    /// Where the operator said the video starts.
    Manual,
}

/// The video's own sound, measured at a few points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSound {
    pub has_track: bool,
    /// Loud and varied enough to sync by; the Cinema Camera's unconnected input sits at −97 dBFS.
    pub usable: bool,
    /// The loudest measured stretch, in dBFS.
    pub level_dbfs: Option<f64>,
}

/// `POST /api/media/programs/probe` body.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramProbeRequest {
    pub video_path: String,
    /// The recording, or the files a recorder split it into, in playing order; empty to use the
    /// video's own sound.
    #[serde(default)]
    pub audio_paths: Vec<String>,
}

/// Where the clocks put the picture's start in the audio.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClockEstimate {
    /// Seconds into the joined audio; negative when the audio starts after the picture.
    pub offset_seconds: f64,
    /// Corrected by the clock difference a recent confident sync of the same camera found. The
    /// raw clocks can be minutes apart (the Cinema Camera ran 3:50 ahead of the Wireless PRO).
    pub learned: bool,
}

/// What a video and its audio would become, and what the host still lacks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramProbe {
    pub video: MediaProbe,
    /// The recording; absent when the video's own sound is used.
    pub audio: Option<AudioProbe>,
    pub video_sound: VideoSound,
    /// How the job will find the sync: `none`, `waveform` or `lips`.
    pub sync_method: SyncMethod,
    pub clock: Option<ClockEstimate>,
    /// What the picture goes through unless the request says otherwise.
    pub picture: PictureMode,
    /// What may go wrong, in words for the operator.
    pub warnings: Vec<String>,
    /// Why the job can't run, when it can't.
    pub blocker: Option<String>,
    /// Tools the host lacks and cannot fetch.
    pub missing: Vec<String>,
    /// Tools the first job downloads before it starts.
    pub downloads: Vec<String>,
}

/// `POST /api/media/programs` body. Replied to with [`RenderQueued`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramRequest {
    pub video_path: String,
    #[serde(default)]
    pub audio_paths: Vec<String>,
    /// The event the finished file is filed under; untracked when absent.
    pub event_id: Option<Uuid>,
    /// The probe's choice when absent.
    pub picture: Option<PictureMode>,
    /// The stored defaults when absent.
    pub settings: Option<RenderSettings>,
    /// The stored defaults when absent.
    pub audio_settings: Option<AudioSettings>,
    /// Where in the joined audio the video starts, in seconds, when the operator knows; the
    /// search is skipped. Negative when the audio starts after the video.
    #[serde(default)]
    pub start_seconds: Option<f64>,
    /// Transcripts to line up with the program and send with its uploads.
    #[serde(default)]
    pub subtitles: Vec<SubtitleSource>,
}

/// A transcript (`.srt` or `.vtt`) of the program's speech, timed to the video or to the
/// recording: the job finds which by matching it against the voice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSource {
    pub path: String,
    /// ISO 639-1, such as `hu` or `en`.
    pub language: String,
}

/// Where a transcript ended up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleReport {
    pub language: String,
    pub cues: u32,
    /// Seconds the transcript was moved: to meet the voice, or with the recording.
    pub shift_seconds: f64,
    /// The voice backed the shift clearly. Otherwise a transcript running past the program moved
    /// with the recording (by the sync), and any other was left unmoved.
    pub confident: bool,
    pub srt_path: String,
    /// Rebuilt into whole sentences, each placed where Whisper heard it spoken; `shiftSeconds`
    /// is then the offset found for the whole file, and `cues` the sentence captions written.
    #[serde(default)]
    pub on_speech: bool,
    /// With `onSpeech`: captions with a word matched to the speech; the rest sit between them.
    #[serde(default)]
    pub matched: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgramStep {
    /// Fetching jive-vocals on first use.
    Download,
    /// Grading and encoding the picture (skipped when it's copied).
    Picture,
    /// Joining the audio files and mixing them to one voice track.
    Prepare,
    /// Finding where in the audio the picture starts.
    Sync,
    /// jive-vocals on the part of the audio the picture needs, then the loudness target.
    Clean,
    /// Placing the audio under the picture and writing the file.
    Merge,
    /// Whisper listening to the program, to place a dropped English transcript on the speech.
    Captions,
}

/// One stretch of the picture the sync was measured on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncWindow {
    /// Where the stretch starts in the picture.
    pub video_seconds: f64,
    /// Where this stretch alone put the picture's start in the audio.
    pub offset_seconds: f64,
    /// How well it matched there (−1–1).
    pub score: f64,
    /// Whether it agrees with the result.
    pub agrees: bool,
}

/// How the audio was lined up, kept with the recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub method: SyncMethod,
    /// Where the picture's first frame falls in the joined audio, in seconds; negative when the
    /// audio starts after the picture.
    pub offset_seconds: f64,
    /// How much faster the audio's clock runs than the picture's, in parts per million.
    pub drift_ppm: f64,
    /// 0–1.
    pub confidence: f64,
    /// Strong enough to trust without looking; otherwise the operator should check it.
    pub confident: bool,
    /// How far the best match stands above every other position, in standard deviations.
    pub peak_sigma: Option<f64>,
    pub windows: Vec<SyncWindow>,
    /// The picture's clock minus the audio's (time of day), as this sync found it.
    pub clock_difference_seconds: Option<f64>,
    /// Where the clocks alone put the start.
    pub clock_offset_seconds: Option<f64>,
    /// Frames the audio was moved by hand after the job (positive: it plays later).
    pub manual_frames: i32,
    /// The audio file the picture starts in, and how far into it.
    pub start_file: Option<String>,
    pub start_file_seconds: Option<f64>,
}

/// What a program job did, kept with the recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramReport {
    pub sync: SyncReport,
    pub audio: AudioReport,
    /// The joined audio's length, in seconds.
    pub audio_seconds: f64,
    /// The share of the picture the audio covers (0–1).
    pub coverage: f64,
    /// Silence put under the picture because the audio starts later or ends sooner.
    pub silent_start_seconds: f64,
    pub silent_end_seconds: f64,
    /// Frames at the end of the clip that couldn't be read (a recording that didn't end cleanly),
    /// shown as the last good frame held; the picture keeps its length.
    #[serde(default)]
    pub damaged_frames: u64,
    /// The program audio on its own: the picture's length, aligned to its first frame.
    pub audio_output_path: String,
    #[serde(default)]
    pub subtitles: Vec<SubtitleReport>,
}

/// `media.program.progress`: sent as each step starts, about once a second while a step can
/// count its progress, and once when the job ends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgramProgress {
    pub job_id: Uuid,
    pub state: RenderState,
    pub step: Option<ProgramStep>,
    /// 0–1 through the current step; absent while a step can't tell.
    pub fraction: Option<f64>,
    pub output_path: String,
    pub error: Option<String>,
    /// Set when the job succeeds, and again after each nudge.
    pub report: Option<ProgramReport>,
}

/// `POST /api/media/programs/{jobId}/nudge` body. Replied to with [`RenderQueued`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NudgeRequest {
    /// Frames to move the audio; positive plays it later.
    pub frames: i32,
}

/// `POST /api/media/programs/{jobId}/subtitles` body: transcripts for a finished program, placed
/// on it as a program job would. Replied to with [`RenderQueued`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubtitlesRequest {
    pub subtitles: Vec<SubtitleSource>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACTIVE_6K: CropRect = CropRect {
        x: 8,
        y: 8,
        width: 6048,
        height: 4032,
    };

    #[test]
    fn an_iso_from_the_other_gain_circuit_moves_to_the_nearest_with_the_same_brightness() {
        let options = RawOptions {
            gammas: Vec::new(),
            gamuts: Vec::new(),
            isos: vec![1250, 1600, 2000, 2500, 3200, 4000, 5000, 6400],
            kelvin: None,
            tint: None,
            exposure: None,
            as_shot: AsShot {
                iso: Some(1250),
                kelvin: Some(4000),
                tint: Some(0),
                exposure: Some(0.0),
            },
        };
        let mut settings = RenderSettings {
            iso: Some(800),
            exposure: None,
            ..MediaConfig::default().settings().unwrap()
        };
        options.fit_iso(&mut settings);
        assert_eq!((settings.iso, settings.exposure), (Some(1250), Some(-0.64)));
        // An ISO the clip offers, or none at all, stays as it is.
        options.fit_iso(&mut settings);
        assert_eq!((settings.iso, settings.exposure), (Some(1250), Some(-0.64)));
        settings.iso = None;
        options.fit_iso(&mut settings);
        assert_eq!(settings.iso, None);
    }

    #[test]
    fn the_youtube_crop_of_the_6k_active_area_is_centred_16_by_9() {
        assert_eq!(
            output_crop(ACTIVE_6K, OutputAspect::Widescreen, 0),
            CropRect {
                x: 8,
                y: 323,
                width: 6048,
                height: 3402
            }
        );
    }

    #[test]
    fn a_crop_shift_moves_the_frame_but_never_past_the_active_area() {
        let shifted = |shift| output_crop(ACTIVE_6K, OutputAspect::Widescreen, shift).y;
        assert_eq!(shifted(-100), 223);
        assert_eq!(shifted(-10_000), 8);
        assert_eq!(shifted(10_000), 8 + 4032 - 3402);
    }

    #[test]
    fn native_keeps_the_active_area_and_wide_sources_keep_their_height() {
        assert_eq!(output_crop(ACTIVE_6K, OutputAspect::Native, 0), ACTIVE_6K);
        let scope = CropRect {
            x: 0,
            y: 0,
            width: 4096,
            height: 1716,
        };
        let crop = output_crop(scope, OutputAspect::Widescreen, 0);
        assert_eq!((crop.width, crop.height, crop.y), (3050, 1716, 0));
    }

    #[test]
    fn the_shipped_defaults_are_the_cameras_look() {
        let settings = MediaConfig::default().settings().unwrap();
        assert_eq!(settings.gamma, "Blackmagic Design Film");
        assert_eq!(settings.lut_mode, LutMode::Embedded);
        assert_eq!(settings.aspect, OutputAspect::Widescreen);
        assert_eq!(
            (settings.iso, settings.tint, settings.exposure),
            (None, None, None)
        );
        assert_eq!(settings.audio_channels, vec![1, 2]);
        assert_eq!(settings.bitrate_mbps, 100);
    }

    #[test]
    fn config_text_is_validated_and_typed() {
        let config = MediaConfig {
            tint: " -5 ".to_string(),
            exposure: "0.5".to_string(),
            crop_shift_y: "-40".to_string(),
            iso: 1600,
            ..MediaConfig::default()
        };
        let settings = config.settings().unwrap();
        assert_eq!(
            (
                settings.tint,
                settings.exposure,
                settings.crop_shift_y,
                settings.iso
            ),
            (Some(-5), Some(0.5), -40, Some(1600))
        );

        let broken = |config: MediaConfig| config.settings().unwrap_err();
        assert!(
            broken(MediaConfig {
                tint: "warm".to_string(),
                ..MediaConfig::default()
            })
            .contains("tint")
        );
        assert!(
            broken(MediaConfig {
                lut_mode: LutMode::File,
                ..MediaConfig::default()
            })
            .contains("LUT file")
        );
        assert!(
            broken(MediaConfig {
                audio_channels: "0".to_string(),
                ..MediaConfig::default()
            })
            .contains("audio channel")
        );
    }

    #[test]
    fn audio_defaults_aim_at_youtube_loudness() {
        let settings = MediaConfig::default().audio_settings().unwrap();
        assert_eq!(settings.target_lufs, YOUTUBE_LUFS);
        assert!(settings.channels.is_empty());
        let loud = MediaConfig {
            audio_target_lufs: "-3".to_string(),
            ..MediaConfig::default()
        };
        assert!(loud.audio_settings().unwrap_err().contains("loudness"));
        let quiet = MediaConfig {
            audio_target_lufs: " -16 ".to_string(),
            ..MediaConfig::default()
        };
        assert_eq!(
            quiet.audio_settings().unwrap().target_lufs,
            JIVE_VOCALS_LUFS
        );
    }

    #[test]
    fn program_records_keep_their_wire_names() {
        let request: ProgramRequest = serde_json::from_value(serde_json::json!({
            "videoPath": "/Volumes/A002/Untitled CAM 1 05.braw",
            "picture": "copy",
        }))
        .unwrap();
        assert!(request.audio_paths.is_empty());
        assert_eq!(request.picture, Some(PictureMode::Copy));
        assert_eq!(
            serde_json::to_value(SyncMethod::Lips).unwrap(),
            serde_json::json!("lips")
        );
        assert_eq!(
            serde_json::to_value(ProgramStep::Sync).unwrap(),
            serde_json::json!("sync")
        );
    }

    #[test]
    fn config_and_settings_keep_their_wire_names() {
        let stored = serde_json::json!({"enabled": true, "lutMode": "file", "aspect": "native"});
        let config: MediaConfig = serde_json::from_value(stored).unwrap();
        assert_eq!(
            (config.lut_mode, config.aspect, config.bitrate_mbps),
            (LutMode::File, OutputAspect::Native, 100)
        );
        let settings = serde_json::to_value(MediaConfig::default().settings().unwrap()).unwrap();
        assert_eq!(settings["lutMode"], "embedded");
        assert_eq!(settings["aspect"], "16:9");
        assert_eq!(settings["audioChannels"], serde_json::json!([1, 2]));
    }
}
