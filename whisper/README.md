# whisper-cli for the Mac app

When a program is produced with an English transcript dropped next to the video, the media
pipeline places each sentence on the speech it translates
(`crates/metocast-server/src/media/whisper.rs`). For that it runs `whisper-cli` from
[whisper.cpp](https://github.com/ggml-org/whisper.cpp) on the program's audio. The Mac app
ships it next to `metocast-server`, so nobody installs anything.

`build.sh` builds it when Xcode builds the app (`scripts/embed-apple-server.sh`):

- **Source:** whisper.cpp's release tarball (`v1.9.5` from GitHub), pinned by SHA-256.
- **Licence:** MIT. `LICENSE` is copied into the app's `Resources/whisper`.
- **One file:**
  - Metal with its shaders embedded (`GGML_METAL_EMBED_LIBRARY`), and Accelerate.
  - No OpenMP, SDL2, curl or ccache, and `GGML_NATIVE=OFF`, so it runs on every Apple Silicon
    Mac rather than only on the build Mac's CPU.
  - The script fails if the binary links any library outside macOS.
- **CMake:** whisper.cpp's `CMakeLists.txt` still declares `cmake_minimum_required(VERSION 3.5)`,
  which CMake 4 reports as deprecated. The script rewrites it to the `3.5...4.4` range CMake asks
  for, and stops if that line ever changes.
- **Cache:** built once (about 30 s on an M1) and kept in `target/whisper`, keyed by the source
  and flags.

The models are not in the app. The server downloads them the first time a program needs them,
pinned by SHA-256, into its tools folder (`media/tools.rs`):

| File                     | Size   | Source                                                     |
| ------------------------ | ------ | ---------------------------------------------------------- |
| `ggml-large-v3.bin`      | 3.1 GB | `huggingface.co/ggerganov/whisper.cpp`, commit `5359861c…` |
| `ggml-silero-v5.1.2.bin` | 0.9 MB | `huggingface.co/ggml-org/whisper-vad`, commit `9ffd54a1…`  |

`large-v3` is used because it translates Hungarian to English well; `large-v3-turbo` was not
trained to translate. Silero is the voice-activity model behind `--vad`.

To update, change `version` and `sha256` in `build.sh`. Run it on its own with
`./build.sh [output-dir]`. Outside the app, `METOCAST_WHISPER` points the server at another
`whisper-cli`.
