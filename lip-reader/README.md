# lip-reader

Measures how open the mouths in a video are, frame by frame, with Apple's on-device face-landmark
model (the Vision framework). Metocast's media worker (`crates/metocast-server/src/media/sync.rs`)
correlates that series with the voice in an audio recording to find where in the recording a
video starts, when the video has no usable sound of its own. That's the case for the church's
Cinema Camera 6K, whose audio input is unconnected during services. Nothing else needs to call it
directly.

It's a separate program because Vision and AVFoundation are Apple frameworks, and the Rust server
forbids `unsafe`. Nothing is downloaded or installed: the face model ships with macOS and runs on
the Neural Engine.

## Build

```bash
./build.sh                       # → build/lip-reader
ARCHS="arm64 x86_64" ./build.sh  # universal
```

The Mac app's build phase (`scripts/embed-apple-server.sh`) builds it into
`Contents/MacOS/lip-reader`. A development server finds it through `METOCAST_LIP_READER`.

## Usage

```bash
lip-reader [--start SECONDS] [--duration SECONDS] [--max-width PIXELS] VIDEO
```

It writes one JSON line per frame:

```text
{"t":600.0417,"faces":[{"x":0.41,"y":0.38,"w":0.12,"h":0.18,"open":0.214,"confidence":0.81}]}
```

- `t` is the frame's presentation time in seconds.
- `faces` lists up to three faces, largest first. Each has its box (normalised, origin bottom-left)
  and `open`, which is the inner lips' height divided by the outer lips' width.

AVFoundation decodes the file, so Blackmagic RAW must be rendered first: the worker reads the HEVC
render. The decoder scales the picture to at most `--max-width` pixels wide (default 1920), in
hardware. On an M1 Pro that runs about 50 fps for a 6K HEVC render and about 60 fps for 2K.

## Findings (2026-10-06)

- **Detect the face on every frame.** Reusing a detected box between detections lets the
  landmarks drift as the head moves. The drift repeats at the detection rate, and that periodic
  error swamps the speech signal.
- Matching mouth opening against the voice energy found the 6K sermon clip `Untitled CAM 1 05` in
  an hour-long Wireless PRO recording at 12–16σ. Its 1-minute windows agreed to within ±2 ms. See
  `plans/PLAN-media-pipeline.md` (phase 3).
- The un-graded log picture works as well as the graded one.
