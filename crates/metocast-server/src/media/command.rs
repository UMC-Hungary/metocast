//! The argument lists for one render: `braw-decode frames` (Blackmagic RAW only) piped into
//! ffmpeg. Pure, so the exact commands are unit-tested; arguments are never joined into a shell
//! line.
use std::path::Path;

use metocast_core::media::{CropRect, LutMode, MediaKind, RenderSettings};

/// Everything a render needs to know about its source, resolved by the probe.
#[derive(Debug, Clone)]
pub struct Source<'a> {
    pub path: &'a Path,
    pub kind: MediaKind,
    /// The picture the decoder (or ffmpeg) produces, before cropping.
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub frame_count: u64,
    /// Unreadable frames at the end: the decoder stops before them and ffmpeg holds the last
    /// good frame in their place, so the picture keeps its length (and the sound its sync).
    pub damaged_frames: u64,
    pub start_timecode: Option<&'a str>,
    pub audio_channels: Option<u32>,
    /// What `braw-decode frames` writes: `rgb48le` or `rgba64le`.
    pub pixel_format: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderCommands {
    /// `braw-decode` arguments; `None` when ffmpeg reads the source itself.
    pub decoder: Option<Vec<String>>,
    pub ffmpeg: Vec<String>,
}

/// Characters a LUT path may not contain, because ffmpeg's filter syntax gives them meaning.
const FILTER_SPECIAL: &[char] = &['\'', '\\', ':', ',', ';', '[', ']', '='];

pub fn check_lut_path(path: &str) -> Result<(), String> {
    if path.contains(FILTER_SPECIAL) {
        return Err(format!(
            "the LUT path may not contain any of ' \\ : , ; [ ] =: {path}"
        ));
    }
    Ok(())
}

fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The grade, as `braw-decode` flags: shared by `frames` (renders) and `still` (previews).
fn grade_flags(settings: &RenderSettings) -> Vec<String> {
    let mut args = Vec::new();
    let mut flag = |name: &str, value: String| {
        args.push(format!("--{name}"));
        args.push(value);
    };
    if let Some(sidecar) = &settings.sidecar_path {
        flag("sidecar", sidecar.clone());
    }
    flag("gamma", settings.gamma.clone());
    flag("gamut", settings.gamut.clone());
    if let Some(iso) = settings.iso {
        flag("iso", iso.to_string());
    }
    if let Some(kelvin) = settings.kelvin {
        flag("kelvin", kelvin.to_string());
    }
    if let Some(tint) = settings.tint {
        flag("tint", tint.to_string());
    }
    if let Some(exposure) = settings.exposure {
        flag("exposure", exposure.to_string());
    }
    flag(
        "highlight-recovery",
        u8::from(settings.highlight_recovery).to_string(),
    );
    flag(
        "gamut-compression",
        u8::from(settings.gamut_compression).to_string(),
    );
    let lut_mode = match settings.lut_mode {
        LutMode::Embedded => "embedded",
        LutMode::File => "file",
        LutMode::Sidecar => "sidecar",
        LutMode::None => "none",
    };
    flag("lut-mode", lut_mode.to_string());
    if settings.lut_mode == LutMode::File
        && let Some(lut) = &settings.lut_path
    {
        flag("lut", lut.clone());
    }
    args
}

fn decoder_args(
    source: &Source,
    settings: &RenderSettings,
    crop: CropRect,
    sdk: Option<&Path>,
) -> Vec<String> {
    let mut args = vec!["frames".to_string()];
    if let Some(sdk) = sdk {
        args.extend(["--sdk".to_string(), path_arg(sdk)]);
    }
    args.extend(grade_flags(settings));
    args.extend([
        "--crop".to_string(),
        format!("{},{},{},{}", crop.x, crop.y, crop.width, crop.height),
        "--count".to_string(),
        (source.frame_count - source.damaged_frames).to_string(),
        path_arg(source.path),
    ]);
    args
}

/// The `lut3d` step ffmpeg applies to video it decodes itself (Blackmagic RAW gets its LUT
/// from the decoder).
fn grade_filters(settings: &RenderSettings) -> Option<String> {
    (settings.lut_mode == LutMode::File)
        .then_some(settings.lut_path.as_ref())
        .flatten()
        .map(|lut| format!("lut3d=file={lut}:interp=tetrahedral"))
}

/// One frame as a JPEG on stdout: `braw-decode still` for Blackmagic RAW (`true`), else ffmpeg.
/// Without `settings` the frame is shown as recorded.
pub fn preview_args(
    source: &Source,
    settings: Option<&RenderSettings>,
    frame: u64,
    max_width: u32,
    sdk: Option<&Path>,
) -> (bool, Vec<String>) {
    let width = max_width.to_string();
    if source.kind == MediaKind::Braw {
        let mut args = vec!["still".to_string()];
        if let Some(sdk) = sdk {
            args.extend(["--sdk".to_string(), path_arg(sdk)]);
        }
        match settings {
            Some(settings) => args.extend(grade_flags(settings)),
            None => args.push("--as-recorded".to_string()),
        }
        args.extend([
            "--start".to_string(),
            frame.to_string(),
            "--max-width".to_string(),
            width,
            path_arg(source.path),
        ]);
        return (true, args);
    }
    let seconds = frame as f64 * f64::from(source.fps_den) / f64::from(source.fps_num);
    let mut filters: Vec<String> = settings.and_then(grade_filters).into_iter().collect();
    filters.push(format!("scale=w='min({width},iw)':h=-2"));
    let mut args: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-ss"]
        .map(String::from)
        .to_vec();
    args.extend([
        format!("{seconds:.3}"),
        "-i".to_string(),
        path_arg(source.path),
        "-frames:v".to_string(),
        "1".to_string(),
        "-vf".to_string(),
        filters.join(","),
        "-q:v".to_string(),
        "3".to_string(),
        "-f".to_string(),
        "image2pipe".to_string(),
        "-c:v".to_string(),
        "mjpeg".to_string(),
        "pipe:1".to_string(),
    ]);
    (false, args)
}

/// ffmpeg's arguments for `source` → `output`, with `-progress` key/value lines on stdout.
fn ffmpeg_args(
    source: &Source,
    settings: &RenderSettings,
    crop: CropRect,
    output: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-nostats",
        "-progress",
        "pipe:1",
    ]
    .map(String::from)
    .to_vec();
    let rate = format!("{}/{}", source.fps_num, source.fps_den);
    let mut filters = Vec::new();

    let audio_input = match source.kind {
        MediaKind::Braw => {
            if source.damaged_frames > 0 {
                filters.push(format!(
                    "tpad=stop_mode=clone:stop={}",
                    source.damaged_frames
                ));
            }
            // Frames arrive developed, graded and cropped; the source supplies only audio.
            args.extend([
                "-f".to_string(),
                "rawvideo".to_string(),
                "-pix_fmt".to_string(),
                source.pixel_format.to_string(),
                "-video_size".to_string(),
                format!("{}x{}", crop.width, crop.height),
                "-framerate".to_string(),
                rate,
                "-i".to_string(),
                "pipe:0".to_string(),
                "-i".to_string(),
                path_arg(source.path),
            ]);
            1
        }
        MediaKind::Video => {
            args.extend(["-i".to_string(), path_arg(source.path)]);
            filters.extend(grade_filters(settings));
            if (crop.width, crop.height) != (source.width, source.height) {
                filters.push(format!(
                    "crop={}:{}:{}:{}",
                    crop.width, crop.height, crop.x, crop.y
                ));
            }
            0
        }
    };

    let audio = source
        .audio_channels
        .filter(|channels| *channels > 0)
        .and_then(|available| {
            let picked: Vec<u32> = settings
                .audio_channels
                .iter()
                .map(|channel| channel.saturating_sub(1))
                .filter(|channel| *channel < available)
                .collect();
            match picked.as_slice() {
                [] => None,
                [mono] => Some(format!("pan=stereo|c0=c{mono}|c1=c{mono}")),
                [left, right, ..] => Some(format!("pan=stereo|c0=c{left}|c1=c{right}")),
            }
        });

    args.extend(["-map".to_string(), "0:v:0".to_string()]);
    if audio.is_some() {
        args.extend(["-map".to_string(), format!("{audio_input}:a:0")]);
    }

    // Tag the frames themselves: VideoToolbox writes the VUI from them, not from -color_*.
    filters.push("scale=out_color_matrix=bt709:out_range=tv".to_string());
    filters.push("format=p010le".to_string());
    filters.push(
        "setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709:range=tv".to_string(),
    );
    let bitrate = settings.bitrate_mbps;
    args.extend([
        "-vf".to_string(),
        filters.join(","),
        "-c:v".to_string(),
        "hevc_videotoolbox".to_string(),
        "-profile:v".to_string(),
        "main10".to_string(),
        "-b:v".to_string(),
        format!("{bitrate}M"),
        "-maxrate".to_string(),
        format!("{}M", bitrate * 7 / 5),
        "-bufsize".to_string(),
        format!("{}M", bitrate * 2),
        "-tag:v".to_string(),
        "hvc1".to_string(),
    ]);
    if let Some(timecode) = source.start_timecode {
        args.extend(["-timecode".to_string(), timecode.to_string()]);
    }
    match audio {
        Some(pan) => args
            .extend(["-af", &pan, "-c:a", "aac", "-b:a", "320k", "-ar", "48000"].map(String::from)),
        None => args.push("-an".to_string()),
    }
    args.extend(["-movflags", "+faststart+write_colr", "-f", "mov", "-y"].map(String::from));
    args.push(path_arg(output));
    args
}

pub fn render_commands(
    source: &Source,
    settings: &RenderSettings,
    crop: CropRect,
    sdk: Option<&Path>,
    output: &Path,
) -> RenderCommands {
    RenderCommands {
        decoder: (source.kind == MediaKind::Braw)
            .then(|| decoder_args(source, settings, crop, sdk)),
        ffmpeg: ffmpeg_args(source, settings, crop, output),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use metocast_core::media::{MediaConfig, OutputAspect, output_crop};

    fn sermon<'a>() -> Source<'a> {
        Source {
            path: Path::new("/Volumes/A002/Untitled CAM 1 05.braw"),
            kind: MediaKind::Braw,
            width: 6048,
            height: 4032,
            fps_num: 24,
            fps_den: 1,
            frame_count: 28_455,
            damaged_frames: 7,
            start_timecode: Some("10:26:20:15"),
            audio_channels: Some(4),
            pixel_format: "rgb48le",
        }
    }

    fn crop_of(source: &Source, settings: &RenderSettings) -> CropRect {
        let active = CropRect {
            x: 0,
            y: 0,
            width: source.width,
            height: source.height,
        };
        output_crop(active, settings.aspect, settings.crop_shift_y)
    }

    fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
        args.iter()
            .position(|arg| arg == flag)
            .map(|at| args[at + 1].as_str())
    }

    #[test]
    fn a_sermon_is_decoded_cropped_and_encoded_with_its_timecode() {
        let source = sermon();
        let settings = MediaConfig::default().settings().unwrap();
        let crop = crop_of(&source, &settings);
        let commands = render_commands(
            &source,
            &settings,
            crop,
            Some(Path::new("/SDK/Libraries")),
            Path::new("/out/sermon.mov.partial"),
        );

        let decoder = commands.decoder.unwrap();
        assert_eq!(decoder[0], "frames");
        assert_eq!(value_after(&decoder, "--sdk"), Some("/SDK/Libraries"));
        assert_eq!(
            value_after(&decoder, "--gamma"),
            Some("Blackmagic Design Film")
        );
        assert_eq!(value_after(&decoder, "--lut-mode"), Some("embedded"));
        assert_eq!(value_after(&decoder, "--crop"), Some("0,315,6048,3402"));
        assert_eq!(value_after(&decoder, "--iso"), None, "as shot");
        assert_eq!(value_after(&decoder, "--count"), Some("28448"));
        assert_eq!(
            decoder.last().unwrap(),
            "/Volumes/A002/Untitled CAM 1 05.braw"
        );

        let ffmpeg = &commands.ffmpeg;
        assert!(
            value_after(ffmpeg, "-vf")
                .unwrap()
                .starts_with("tpad=stop_mode=clone:stop=7,"),
            "the last good frame stands in for the damaged ones"
        );
        assert_eq!(value_after(ffmpeg, "-video_size"), Some("6048x3402"));
        assert_eq!(value_after(ffmpeg, "-pix_fmt"), Some("rgb48le"));
        assert_eq!(value_after(ffmpeg, "-framerate"), Some("24/1"));
        assert_eq!(value_after(ffmpeg, "-c:v"), Some("hevc_videotoolbox"));
        assert_eq!(value_after(ffmpeg, "-b:v"), Some("100M"));
        assert_eq!(value_after(ffmpeg, "-maxrate"), Some("140M"));
        assert_eq!(value_after(ffmpeg, "-timecode"), Some("10:26:20:15"));
        assert_eq!(value_after(ffmpeg, "-af"), Some("pan=stereo|c0=c0|c1=c1"));
        assert!(
            value_after(ffmpeg, "-vf")
                .unwrap()
                .contains("color_trc=bt709")
        );
        let maps: Vec<_> = ffmpeg
            .iter()
            .enumerate()
            .filter(|(_, arg)| *arg == "-map")
            .map(|(at, _)| ffmpeg[at + 1].as_str())
            .collect();
        assert_eq!(maps, ["0:v:0", "1:a:0"]);
        assert_eq!(ffmpeg.last().unwrap(), "/out/sermon.mov.partial");
    }

    #[test]
    fn a_resolve_lut_goes_to_the_decoder_and_overrides_are_passed_on() {
        let source = sermon();
        let settings = RenderSettings {
            lut_mode: LutMode::File,
            lut_path: Some("/Volumes/A002/sermon_look.cube".to_string()),
            iso: Some(800),
            tint: Some(-3),
            highlight_recovery: true,
            audio_channels: vec![2],
            ..MediaConfig::default().settings().unwrap()
        };
        let commands = render_commands(
            &source,
            &settings,
            crop_of(&source, &settings),
            None,
            Path::new("/out/a.mov"),
        );
        let decoder = commands.decoder.unwrap();
        assert_eq!(
            value_after(&decoder, "--lut"),
            Some("/Volumes/A002/sermon_look.cube")
        );
        assert_eq!(value_after(&decoder, "--iso"), Some("800"));
        assert_eq!(value_after(&decoder, "--tint"), Some("-3"));
        assert_eq!(value_after(&decoder, "--highlight-recovery"), Some("1"));
        assert_eq!(value_after(&decoder, "--sdk"), None);
        assert_eq!(
            value_after(&commands.ffmpeg, "-af"),
            Some("pan=stereo|c0=c1|c1=c1")
        );
    }

    #[test]
    fn plain_video_is_read_by_ffmpeg_with_the_lut_as_a_filter() {
        let source = Source {
            path: Path::new("/Movies/service.mp4"),
            kind: MediaKind::Video,
            width: 1920,
            height: 1080,
            fps_num: 30,
            fps_den: 1,
            frame_count: 900,
            damaged_frames: 0,
            start_timecode: None,
            audio_channels: None,
            pixel_format: "",
        };
        let settings = RenderSettings {
            lut_mode: LutMode::File,
            lut_path: Some("/Looks/warm.cube".to_string()),
            ..MediaConfig::default().settings().unwrap()
        };
        let commands = render_commands(
            &source,
            &settings,
            crop_of(&source, &settings),
            None,
            Path::new("/out/b.mov"),
        );
        assert!(commands.decoder.is_none());
        let filters = value_after(&commands.ffmpeg, "-vf").unwrap();
        assert!(filters.starts_with("lut3d=file=/Looks/warm.cube:interp=tetrahedral,scale="));
        assert!(!filters.contains("crop="), "16:9 source needs no crop");
        assert!(commands.ffmpeg.contains(&"-an".to_string()));
        assert_eq!(value_after(&commands.ffmpeg, "-timecode"), None);
    }

    #[test]
    fn native_framing_keeps_the_whole_picture() {
        let source = sermon();
        let settings = RenderSettings {
            aspect: OutputAspect::Native,
            ..MediaConfig::default().settings().unwrap()
        };
        let commands = render_commands(
            &source,
            &settings,
            crop_of(&source, &settings),
            None,
            Path::new("/out/c.mov"),
        );
        assert_eq!(
            value_after(commands.decoder.as_ref().unwrap(), "--crop"),
            Some("0,0,6048,4032")
        );
        assert_eq!(
            value_after(&commands.ffmpeg, "-video_size"),
            Some("6048x4032")
        );
    }

    #[test]
    fn previews_share_the_render_grade_and_show_the_original_as_recorded() {
        let source = sermon();
        let settings = RenderSettings {
            sidecar_path: Some("/Looks/resolve.sidecar".to_string()),
            lut_mode: LutMode::Sidecar,
            ..MediaConfig::default().settings().unwrap()
        };
        let (braw, result) = preview_args(&source, Some(&settings), 2000, 1600, None);
        assert!(braw);
        assert_eq!(result[0], "still");
        assert_eq!(
            value_after(&result, "--sidecar"),
            Some("/Looks/resolve.sidecar")
        );
        assert_eq!(value_after(&result, "--lut-mode"), Some("sidecar"));
        assert_eq!(value_after(&result, "--start"), Some("2000"));
        assert_eq!(
            value_after(&result, "--crop"),
            None,
            "the viewer frames the crop itself"
        );
        let render = decoder_args(&source, &settings, crop_of(&source, &settings), None);
        assert!(
            render
                .windows(2)
                .any(|pair| pair == ["--sidecar", "/Looks/resolve.sidecar"])
        );

        let (_, original) = preview_args(&source, None, 0, 800, None);
        assert!(original.contains(&"--as-recorded".to_string()));
        assert_eq!(value_after(&original, "--gamma"), None);

        let video = Source {
            path: Path::new("/Movies/service.mp4"),
            kind: MediaKind::Video,
            fps_num: 30,
            fps_den: 1,
            ..sermon()
        };
        let graded = RenderSettings {
            lut_mode: LutMode::File,
            lut_path: Some("/Looks/warm.cube".to_string()),
            ..MediaConfig::default().settings().unwrap()
        };
        let (braw, args) = preview_args(&video, Some(&graded), 90, 1600, None);
        assert!(!braw);
        assert_eq!(value_after(&args, "-ss"), Some("3.000"));
        assert_eq!(
            value_after(&args, "-vf"),
            Some("lut3d=file=/Looks/warm.cube:interp=tetrahedral,scale=w='min(1600,iw)':h=-2")
        );
    }

    #[test]
    fn lut_paths_with_filter_syntax_are_refused() {
        assert!(check_lut_path("/Volumes/A002/sermon look.cube").is_ok());
        assert!(check_lut_path("/Looks/a:b.cube").is_err());
        assert!(check_lut_path("/Looks/it's.cube").is_err());
    }
}
