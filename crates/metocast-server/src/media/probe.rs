//! What a source file is: ffprobe for the container (timecode, audio, the camera's metadata
//! tags) and, for Blackmagic RAW, `braw-decode info` for the developed picture.
use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use metocast_core::media::{
    AsShot, AudioInfo, CropRect, LutMode, MediaKind, RawOptions, RenderSettings, ValueRange,
};

/// The container's view of a file, from `ffprobe -show_format -show_streams -of json`.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerFacts {
    pub kind: MediaKind,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub frame_count: u64,
    pub duration_seconds: f64,
    pub start_timecode: Option<String>,
    pub recorded_at: Option<DateTime<Utc>>,
    pub camera: Option<String>,
    pub clip_name: Option<String>,
    pub gamma: Option<String>,
    pub gamut: Option<String>,
    pub embedded_lut: Option<String>,
    pub audio: Option<AudioInfo>,
}

/// `braw-decode info`: the picture the SDK develops, which can be smaller than the container's
/// frame (the Cinema Camera 6K stores 6064×4048 and develops its 6048×4032 active area).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrawInfo {
    pub width: u32,
    pub height: u32,
    pub frame_count: u64,
    pub active_area: CropRect,
    pub start_timecode: String,
    pub camera: String,
    pub gamma: String,
    pub gamut: String,
    pub embedded_lut: String,
    /// `rgb48le` or `rgba64le`: what `braw-decode frames` writes.
    pub pixel_format: String,
    /// `Embedded`, `Sidecar` or `Disabled`, as the clip (or a `--sidecar`) sets it.
    #[serde(default)]
    pub lut_mode: String,
    #[serde(default)]
    pub highlight_recovery: String,
    #[serde(default)]
    pub gamut_compression: String,
    /// Frame 0's own ISO, white balance and exposure, as text.
    #[serde(default)]
    pub as_shot: HashMap<String, String>,
    #[serde(default)]
    pub options: BrawOptions,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct BrawOptions {
    #[serde(default)]
    pub gammas: Vec<String>,
    #[serde(default)]
    pub gamuts: Vec<String>,
    #[serde(default)]
    pub isos: Vec<String>,
    /// `[min, max]` as text, by setting: `kelvin`, `tint`, `exposure`.
    #[serde(default)]
    pub ranges: HashMap<String, (String, String)>,
}

impl BrawInfo {
    fn shot<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        // `0.000000` for tint and ISO comes back from the SDK as a float string.
        let text = self.as_shot.get(key)?;
        text.parse()
            .ok()
            .or_else(|| text.parse::<f64>().ok()?.round().to_string().parse().ok())
    }

    pub fn as_shot(&self) -> AsShot {
        AsShot {
            iso: self.shot("iso"),
            kelvin: self.shot("kelvin"),
            tint: self.shot("tint"),
            exposure: self.shot("exposure"),
        }
    }

    pub fn raw_options(&self) -> RawOptions {
        let range = |key: &str| {
            let (min, max) = self.options.ranges.get(key)?;
            Some(ValueRange {
                min: min.parse().ok()?,
                max: max.parse().ok()?,
            })
        };
        RawOptions {
            gammas: self.options.gammas.clone(),
            gamuts: self.options.gamuts.clone(),
            isos: self
                .options
                .isos
                .iter()
                .filter_map(|iso| iso.parse().ok())
                .collect(),
            kelvin: range("kelvin"),
            tint: range("tint"),
            exposure: range("exposure"),
            as_shot: self.as_shot(),
        }
    }

    /// `settings` as this clip (with its sidecar) says to develop it: what importing a Resolve
    /// sidecar fills the pickers with.
    pub fn apply_to(&self, settings: &mut RenderSettings) {
        let shot = self.as_shot();
        settings.gamma.clone_from(&self.gamma);
        settings.gamut.clone_from(&self.gamut);
        settings.iso = shot.iso;
        settings.kelvin = shot.kelvin;
        settings.tint = shot.tint;
        settings.exposure = shot.exposure;
        settings.highlight_recovery = self.highlight_recovery == "1";
        settings.gamut_compression = self.gamut_compression == "1";
        settings.lut_mode = match self.lut_mode.as_str() {
            "Sidecar" => LutMode::Sidecar,
            "Embedded" => LutMode::Embedded,
            _ => LutMode::None,
        };
    }
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn number<T: std::str::FromStr>(value: &Value) -> Option<T> {
    match value {
        Value::String(text) => text.parse().ok(),
        Value::Number(number) => number.to_string().parse().ok(),
        _ => None,
    }
}

/// `24/1` or `24000/1001`.
fn rational(value: &Value) -> Option<(u32, u32)> {
    let (num, den) = value.as_str()?.split_once('/')?;
    let (num, den) = (num.parse().ok()?, den.parse().ok()?);
    (num > 0 && den > 0).then_some((num, den))
}

pub fn parse_ffprobe(json: &Value, path: &Path) -> Result<ContainerFacts, String> {
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let tags = &json["format"]["tags"];
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video")
        .ok_or("the file has no video stream")?;
    let is_braw = video["codec_tag_string"] == "brxq"
        || video["codec_tag_string"]
            .as_str()
            .is_some_and(|tag| tag.starts_with("brx"))
        || path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("braw"));

    let (fps_num, fps_den) = rational(&video["r_frame_rate"])
        .or_else(|| rational(&video["avg_frame_rate"]))
        .ok_or("the video stream has no frame rate")?;
    let duration_seconds = number::<f64>(&video["duration"])
        .or_else(|| number(&json["format"]["duration"]))
        .unwrap_or(0.0);
    let frame_count = number::<u64>(&video["nb_frames"]).unwrap_or_else(|| {
        // Rounded estimate for containers that don't count frames.
        (duration_seconds * f64::from(fps_num) / f64::from(fps_den)).round() as u64
    });
    let start_timecode = streams
        .iter()
        .find_map(|stream| text(&stream["tags"]["timecode"]))
        .or_else(|| text(&tags["timecode"]));
    let audio = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio")
        .map(|stream| AudioInfo {
            codec: text(&stream["codec_name"]).unwrap_or_default(),
            channels: number(&stream["channels"]).unwrap_or(0),
            sample_rate: number(&stream["sample_rate"]).unwrap_or(0),
            bits: number(&stream["bits_per_raw_sample"])
                .or_else(|| number(&stream["bits_per_sample"]))
                .filter(|bits| *bits > 0),
        });

    Ok(ContainerFacts {
        kind: if is_braw {
            MediaKind::Braw
        } else {
            MediaKind::Video
        },
        width: number(&video["width"]).ok_or("the video stream has no width")?,
        height: number(&video["height"]).ok_or("the video stream has no height")?,
        fps_num,
        fps_den,
        frame_count,
        duration_seconds,
        start_timecode,
        recorded_at: text(&tags["creation_time"])
            .and_then(|time| DateTime::parse_from_rfc3339(&time).ok())
            .map(|time| time.with_timezone(&Utc)),
        camera: text(&tags["camera_type"]),
        clip_name: text(&tags["clip_number"]),
        gamma: text(&tags["viewing_gamma"]),
        gamut: text(&tags["viewing_gamut"]),
        embedded_lut: text(&tags["post_3dlut_embedded_title"]),
        audio,
    })
}

pub fn parse_braw_info(stdout: &[u8]) -> Result<BrawInfo, String> {
    serde_json::from_slice(stdout).map_err(|e| format!("unreadable braw-decode info: {e}"))
}

/// Runs a probe tool and returns its stdout, or the reason it failed.
async fn run(program: &Path, args: &[&std::ffi::OsStr]) -> Result<Vec<u8>, String> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("could not run {}: {e}", program.display()))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "{} failed: {}",
            program
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
            stderr.trim()
        ))
    }
}

pub async fn ffprobe(ffprobe: &Path, source: &Path) -> Result<ContainerFacts, String> {
    parse_ffprobe(&ffprobe_json(ffprobe, source).await?, source)
}

async fn ffprobe_json(ffprobe: &Path, source: &Path) -> Result<Value, String> {
    let stdout = run(
        ffprobe,
        &[
            "-v".as_ref(),
            "error".as_ref(),
            "-show_format".as_ref(),
            "-show_streams".as_ref(),
            "-of".as_ref(),
            "json".as_ref(),
            source.as_os_str(),
        ],
    )
    .await?;
    serde_json::from_slice(&stdout).map_err(|e| format!("unreadable ffprobe output: {e}"))
}

/// A file's first audio stream, for the audio pipeline: any container ffmpeg reads.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioFacts {
    pub audio: AudioInfo,
    pub duration_seconds: f64,
    /// The container's own creation time, when it records one.
    pub recorded_at: Option<DateTime<Utc>>,
}

pub fn parse_audio(json: &Value) -> Result<AudioFacts, String> {
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let stream = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio")
        .ok_or("the file has no audio")?;
    let audio = AudioInfo {
        codec: text(&stream["codec_name"]).unwrap_or_default(),
        channels: number(&stream["channels"]).unwrap_or(0),
        sample_rate: number(&stream["sample_rate"]).unwrap_or(0),
        bits: number(&stream["bits_per_raw_sample"])
            .or_else(|| number(&stream["bits_per_sample"]))
            .filter(|bits| *bits > 0),
    };
    if audio.channels == 0 || audio.sample_rate == 0 {
        return Err("the audio stream has no channels or sample rate".to_string());
    }
    let duration_seconds = number::<f64>(&stream["duration"])
        .or_else(|| number(&json["format"]["duration"]))
        .filter(|seconds| *seconds > 0.0)
        .ok_or("the audio has no duration")?;
    let recorded_at = text(&stream["tags"]["creation_time"])
        .or_else(|| text(&json["format"]["tags"]["creation_time"]))
        .and_then(|time| DateTime::parse_from_rfc3339(&time).ok())
        .map(|time| time.with_timezone(&Utc));
    Ok(AudioFacts {
        audio,
        duration_seconds,
        recorded_at,
    })
}

pub async fn ffprobe_audio(ffprobe: &Path, source: &Path) -> Result<AudioFacts, String> {
    parse_audio(&ffprobe_json(ffprobe, source).await?)
}

pub async fn braw_info(
    braw_decode: &Path,
    sdk: Option<&Path>,
    sidecar: Option<&Path>,
    source: &Path,
) -> Result<BrawInfo, String> {
    let mut args: Vec<&std::ffi::OsStr> = vec!["info".as_ref()];
    if let Some(sdk) = sdk {
        args.extend(["--sdk".as_ref(), sdk.as_os_str()]);
    }
    if let Some(sidecar) = sidecar {
        args.extend(["--sidecar".as_ref(), sidecar.as_os_str()]);
    }
    args.push(source.as_os_str());
    parse_braw_info(&run(braw_decode, &args).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Trimmed from `ffprobe` on the church camera's `A002_09160323_C001.braw`.
    fn braw_probe() -> Value {
        json!({
            "streams": [
                {"index": 0, "codec_name": "unknown", "codec_type": "video", "codec_tag_string": "brxq",
                 "width": 6064, "height": 4048, "r_frame_rate": "24/1", "avg_frame_rate": "24/1",
                 "duration": "32.250000", "nb_frames": "774", "tags": {"timecode": "03:23:25:21"}},
                {"index": 1, "codec_type": "data", "codec_tag_string": "tmcd",
                 "tags": {"timecode": "03:23:25:21"}},
                {"index": 2, "codec_name": "pcm_s24le", "codec_type": "audio", "codec_tag_string": "lpcm",
                 "sample_rate": "48000", "channels": 4, "bits_per_sample": 24}
            ],
            "format": {"duration": "32.250441", "tags": {
                "creation_time": "2026-09-16T01:23:25.000000Z",
                "camera_type": "Blackmagic Cinema Camera 6K",
                "viewing_gamma": "Blackmagic Design Film", "viewing_gamut": "Blackmagic Design",
                "post_3dlut_embedded_title": "Gen 5 Film to Extended Video",
                "clip_number": "A002_09160323_C001"}}
        })
    }

    #[test]
    fn a_braw_file_reports_the_cameras_metadata() {
        let facts = parse_ffprobe(&braw_probe(), Path::new("/Volumes/A002/clip.braw")).unwrap();
        assert_eq!(facts.kind, MediaKind::Braw);
        assert_eq!((facts.width, facts.height), (6064, 4048));
        assert_eq!(
            (facts.fps_num, facts.fps_den, facts.frame_count),
            (24, 1, 774)
        );
        assert_eq!(facts.start_timecode.as_deref(), Some("03:23:25:21"));
        assert_eq!(facts.camera.as_deref(), Some("Blackmagic Cinema Camera 6K"));
        assert_eq!(facts.clip_name.as_deref(), Some("A002_09160323_C001"));
        assert_eq!(
            facts.embedded_lut.as_deref(),
            Some("Gen 5 Film to Extended Video")
        );
        assert_eq!(
            facts.audio,
            Some(AudioInfo {
                codec: "pcm_s24le".to_string(),
                channels: 4,
                sample_rate: 48000,
                bits: Some(24)
            })
        );
        assert_eq!(
            facts.recorded_at.map(|time| time.to_rfc3339()),
            Some("2026-09-16T01:23:25+00:00".to_string())
        );
    }

    #[test]
    fn an_obs_recording_is_plain_video_with_an_estimated_frame_count() {
        let json = json!({
            "streams": [{"codec_type": "video", "codec_name": "h264", "codec_tag_string": "avc1",
                         "width": 1920, "height": 1080, "r_frame_rate": "30000/1001"}],
            "format": {"duration": "10.0"}
        });
        let facts = parse_ffprobe(&json, Path::new("/Movies/service.mp4")).unwrap();
        assert_eq!(facts.kind, MediaKind::Video);
        assert_eq!((facts.fps_num, facts.fps_den), (30000, 1001));
        assert_eq!(facts.frame_count, 300);
        assert!(facts.audio.is_none() && facts.start_timecode.is_none());
    }

    #[test]
    fn a_file_without_video_is_rejected() {
        let json = json!({"streams": [{"codec_type": "audio"}], "format": {}});
        assert!(parse_ffprobe(&json, Path::new("/a.mov")).is_err());
    }

    #[test]
    fn a_wireless_mic_recording_is_mono_float_audio() {
        // ffprobe on a RØDE Wireless PRO's onboard recording.
        let json = json!({
            "streams": [{"codec_name": "pcm_f32le", "codec_type": "audio", "sample_rate": "48000",
                         "channels": 1, "bits_per_sample": 32, "duration": "3600.232333"}],
            "format": {"format_name": "wav", "duration": "3600.232333"}
        });
        let facts = parse_audio(&json).unwrap();
        assert_eq!(
            facts.audio,
            AudioInfo {
                codec: "pcm_f32le".to_string(),
                channels: 1,
                sample_rate: 48000,
                bits: Some(32)
            }
        );
        assert!((facts.duration_seconds - 3600.232333).abs() < 1e-9);
        assert!(facts.recorded_at.is_none());
        // A video file's audio counts too; a silent one doesn't.
        assert_eq!(parse_audio(&braw_probe()).unwrap().audio.channels, 4);
        let silent = json!({"streams": [{"codec_type": "video"}], "format": {"duration": "1"}});
        assert!(parse_audio(&silent).is_err());
    }

    #[test]
    fn braw_decode_info_is_read() {
        // Shape printed by `braw-decode info` for the same clip.
        let stdout = br#"{"width":6048,"height":4032,"frameRate":24,"frameCount":774,
            "activeArea":{"x":0,"y":0,"width":6048,"height":4032},"startTimecode":"03:23:25:21",
            "camera":"Blackmagic Cinema Camera 6K","gamma":"Blackmagic Design Film",
            "gamut":"Blackmagic Design","colorScienceGen":"5","lutMode":"Disabled",
            "embeddedLut":"Gen 5 Film to Extended Video",
            "asShot":{"exposure":"0.000000","iso":"400","kelvin":"6400","tint":"0"},
            "options":{"gammas":["Blackmagic Design Film","Blackmagic Design Video"],
            "gamuts":["Blackmagic Design","Rec.709"],"isos":["100","400","1000"],
            "ranges":{"exposure":["-5.000000","5.000000"],"kelvin":["2000","50000"],"tint":["-150","150"]}},
            "pixelFormat":"rgb48le","pipeline":"metal"}"#;
        let info = parse_braw_info(stdout).unwrap();
        assert_eq!(
            (info.width, info.height, info.frame_count),
            (6048, 4032, 774)
        );
        assert_eq!(info.active_area.width, 6048);
        assert_eq!(info.pixel_format, "rgb48le");
        let options = info.raw_options();
        assert_eq!(options.isos, [100, 400, 1000]);
        assert_eq!(
            options.kelvin,
            Some(ValueRange {
                min: 2000.0,
                max: 50000.0
            })
        );
        assert_eq!(
            (options.as_shot.iso, options.as_shot.tint),
            (Some(400), Some(0))
        );
        let mut settings = metocast_core::media::MediaConfig::default()
            .settings()
            .unwrap();
        info.apply_to(&mut settings);
        assert_eq!(
            (settings.kelvin, settings.lut_mode),
            (Some(6400), LutMode::None)
        );
    }
}
