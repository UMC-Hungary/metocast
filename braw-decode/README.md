# braw-decode

Develops a Blackmagic RAW clip with the [Blackmagic RAW SDK](https://www.blackmagicdesign.com/developer/products/braw/sdk-and-software)
and writes the frames, in order, to stdout as raw 16-bit RGB for ffmpeg. Metocast's media worker
(`crates/metocast-server/src/media`) runs it piped into `ffmpeg … hevc_videotoolbox`; nothing else
needs to call it directly.

Why a separate program: ffmpeg cannot decode BRAW, free DaVinci Resolve cannot render above UHD,
and the SDK is a C++/Objective-C API. A small process keeps the SDK out of the Rust server (which
forbids `unsafe`), and a crash in it fails one render instead of the server.

The SDK does the colour work on the Metal GPU: RAW development (gamma, gamut, ISO, white balance,
exposure) and the 3D LUT — the clip's embedded one (the camera's "Gen 5 Film to Extended Video")
or a `.cube` file such as Resolve's **Color page → right-click the clip → Generate LUT**. It never
writes a `.sidecar`, so the source clips and how other apps show them stay untouched.

## Build

The SDK is not vendored. Install **Blackmagic RAW** (free, after a registration form; it puts the
SDK in `/Applications/Blackmagic RAW/Blackmagic RAW SDK`), then:

```bash
./build.sh                      # → build/braw-decode
BRAW_SDK="/path/to/Blackmagic RAW SDK/Mac" ./build.sh
```

The Mac app's build phase (`scripts/embed-apple-server.sh`) builds it into
`Contents/MacOS/braw-decode` when the SDK is installed, signed with
[`braw-decode.entitlements`](braw-decode.entitlements): the hardened runtime otherwise refuses to
load Blackmagic's framework, which another team signed. Without the SDK the app builds without it
and the Video screen says so.

At run time it loads `BlackmagicRawAPI.framework` from `--sdk DIR`, or else from
`/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac/Libraries` or the Blackmagic RAW Player app.

### The SDK on a build machine

Only the headers are needed to build, because the framework is loaded at run time from the
Blackmagic RAW install on the Mac that runs it. A Mac with Blackmagic RAW installed compiles
`braw-decode` as part of the app's build and needs nothing further. No GitHub Actions runner can:
Blackmagic's licence keeps the SDK out of this repository, and their download sits behind a
registration form. That is why builds here once shipped a Mac app with no `braw-decode` in it at
all, and why Blackmagic RAW clips failed on a downloaded app however the Mac itself was set up.

`prebuilt/braw-decode` closes that gap: a build of this helper, committed so a machine without the
SDK still has one. It links nothing outside macOS's own frameworks and loads Blackmagic's at run
time, so the same binary runs on any Apple Silicon Mac. The app's build phase prefers a fresh
build from the SDK and falls back to this one; with neither it stops, rather than shipping an app
that cannot open a `.braw` file.

Only a Mac with the SDK can refresh it, so `prebuilt/sources.sha256` holds the hash of
`src/braw_decode.mm` and `build.sh`, and the build fails when they no longer match it. After
changing either, rebuild the helper on such a Mac:

```bash
./build.sh prebuilt/braw-decode
cat src/braw_decode.mm build.sh | shasum -a 256 | awk '{print $1}' > prebuilt/sources.sha256
```

## Usage

```bash
braw-decode info [--sidecar FILE] CLIP.braw
braw-decode frames [--sidecar FILE] [--gamma NAME] [--gamut NAME] [--iso N] [--kelvin N] [--tint N]
                   [--exposure STOPS] [--highlight-recovery 0|1] [--gamut-compression 0|1]
                   [--lut-mode embedded|file|sidecar|none] [--lut FILE.cube]
                   [--crop X,Y,W,H] [--start FRAME] [--count FRAMES] CLIP.braw
braw-decode still [the frames settings | --as-recorded] [--max-width W] --start FRAME CLIP.braw
```

`info` prints JSON: the developed size, the active area, frame rate and count, `readableFrames`
(fewer than the count when a recording that ended uncleanly left unreadable frames at its end:
renders hold the last good frame from there, see below), start timecode,
camera, gamma, gamut, highlight recovery, gamut compression, the LUT mode and the embedded and
sidecar LUTs, the first frame's as-shot ISO/white balance/tint/exposure, the `options` the clip
allows (`gammas`, `gamuts`, `isos`, and `ranges` for kelvin, tint and exposure), and the pixel
format `frames` writes (`rgb48le`, or `rgba64le` on devices without 16-bit RGB).

`still` writes one frame as a JPEG at most `W` (default 1600) pixels wide, decoded at the smallest
SDK resolution that covers it (about 0.25 s for a 6K frame). `--as-recorded` ignores the settings
and keeps the clip's own look; Metocast's viewer shows that as the original.

`--sidecar` applies a `.sidecar` file, such as one Resolve saved with Update Sidecar, as if it sat
beside the clip. The SDK reads only `<clip>.sidecar`, so the file is placed beside a link to the
clip in a temporary folder; the clip and its folder are never written to. Settings given as flags
still win over the sidecar's.
Options left out keep what the clip was shot with. An `--iso` outside the clip's list (`info`'s
`isos`, which follow the gain it was recorded with) is refused, because the SDK develops it wrongly. Errors go to stderr; the exit status is 0 on
success.

By hand, the same pipe the worker runs (two seconds, 16:9 crop of the Cinema Camera 6K picture):

```bash
braw-decode frames --crop 0,315,6048,3402 --count 48 clip.braw |
  ffmpeg -f rawvideo -pix_fmt rgb48le -video_size 6048x3402 -framerate 24 -i pipe:0 -i clip.braw \
    -map 0:v -map 1:a:0 -vf "scale=out_color_matrix=bt709:out_range=tv,format=p010le,setparams=color_primaries=bt709:color_trc=bt709:colorspace=bt709:range=tv" \
    -c:v hevc_videotoolbox -profile:v main10 -b:v 100M -tag:v hvc1 -af "pan=stereo|c0=c0|c1=c1" \
    -c:a aac -b:a 320k -t 2 out.mov
```

## Findings

- The Cinema Camera 6K stores 6064×4048; the SDK develops the 6048×4032 active area, which the
  `crop_origin`/`crop_size` metadata also describe. The YouTube crop is 6048×3402 at (0, 315).
- On an M1 Pro the Metal decode alone runs at about 31 fps; with ffmpeg's RGB→10-bit YUV
  conversion and the hardware HEVC encoder, the pipe runs at about 11–12 fps (a 29-minute sermon
  in about an hour). The next speed-up is doing that conversion on the GPU here and writing P010.
- A recording that ends uncleanly can count frames it never fully wrote, scattered among good
  ones (`Untitled CAM 1 07.braw`: 28448–28450 and 28454 of 28455). The SDK fails to read them
  with `0x8000FFFF`. `info` reads every frame of the last two seconds and reports the first that
  fails as `readableFrames`; Metocast decodes up to it with `--count` and holds the last good frame
  over the rest (ffmpeg's `tpad`), so the picture keeps its length. That costs about 50 frame
  reads (300 MB) per probe; damage earlier in the clip still fails the render.
- The codec must be released before the Metal device it runs on, or the SDK crashes on exit.
