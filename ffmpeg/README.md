# ffmpeg for the Mac app

The media pipeline (Server Tools → Video and Audio) runs `ffmpeg` and `ffprobe`. The Mac app
ships its own copies next to `metocast-server`, so nobody installs anything.

`build.sh` builds them when Xcode builds the app (`scripts/embed-apple-server.sh`):

- **Source:** FFmpeg's official release tarball from `ffmpeg.org/releases`, pinned by SHA-256.
  When `gpg` is installed the build also checks FFmpeg's release signature (key
  `FCF986EA15E6E293A5644F10B4322F04D67658D8`). The 9.0.2 tarball was also compared with the
  `n9.0.2` tag of github.com/FFmpeg/FFmpeg when it was pinned; they match.
- **Licence:** LGPL 2.1 or later. The configuration has no `--enable-gpl`, no `--enable-nonfree`
  and no outside libraries. `--disable-autodetect` keeps Homebrew libraries on the build Mac out
  of the binaries; only macOS's own frameworks and libraries are used (VideoToolbox,
  AudioToolbox, zlib, bzip2). `LICENSE.md` and `COPYING.LGPLv2.1` are copied into the app's
  `Resources/ffmpeg`, and the source is the pinned tarball named above.
- **What Metocast uses:** FFmpeg's own decoders and filters, VideoToolbox's HEVC encoder, the
  native AAC encoder, and Broadcast WAV output.
- **Cache:** built once (about 2½ minutes on an M1 Pro) and kept in `target/ffmpeg`, keyed by the
  source, flags and deployment target.
- **Apple Silicon only:** Metocast no longer builds for Intel Macs, so there is one arm64 build.

To update, change `version` and `sha256` in `build.sh` and check the new tarball's signature.
Run it on its own with `./build.sh [output-dir]`.

Outside the app, `METOCAST_FFMPEG` points the server at another ffmpeg (its `ffprobe` must sit
beside it), and the Media Processing setting `ffmpegPath` overrides both; a bare checkout falls
back to Homebrew's.
