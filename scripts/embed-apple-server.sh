#!/usr/bin/env bash
# Xcode build phase for apple-app: builds the headless metocast-server and embeds it
# in the Mac app as Contents/MacOS/metocast-server, the helper that server mode runs.
# iOS builds skip it: the iPhone app is client-only.
set -euo pipefail

[ "${PLATFORM_NAME:-}" = "macosx" ] || exit 0
[ "${ARCHS:-}" = "arm64" ] || { echo "error: Metocast is Apple Silicon only, ARCHS is '${ARCHS:-}'" >&2; exit 1; }

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
helpers="$TARGET_BUILD_DIR/$EXECUTABLE_FOLDER_PATH"

# Cargo and FFmpeg's build run without Xcode's environment (SDK, deployment target and toolchain
# settings for the app): it breaks C builds and would invalidate cargo's cache on every switch
# between Xcode and terminal builds.
clean_env() {
  env -i HOME="$HOME" USER="${USER:-}" TMPDIR="${TMPDIR:-/tmp}" \
    PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    "$@"
}
cargo() {
  clean_env cargo "$@"
}

# Hardened-runtime signing with Xcode's identity; nothing when Xcode has signing switched off.
sign() {
  if [ "${CODE_SIGNING_ALLOWED:-YES}" = "YES" ]; then
    codesign --force --options runtime --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" "$@"
  fi
}

profile_flag=""
profile_dir="debug"
strip_helper=false
if [ "${CONFIGURATION:-Debug}" = "Release" ]; then
  profile_flag="--release"
  profile_dir="release"
  strip_helper=true
fi

# The host build shares target/<profile> with plain `cargo build`.
cargo build --manifest-path "$repo_root/Cargo.toml" -p metocast-server --bin metocast-server $profile_flag
cp "$repo_root/target/$profile_dir/metocast-server" "$helpers/metocast-server"

# A shipped build leaves out the local symbol table, about a fifth of the binary and only useful
# to a debugger. Stripping drops the signature: sign it again, ad hoc here and with Xcode's
# identity below when signing is on, so an arm64 binary is never left unsigned.
if $strip_helper; then
  strip -x "$helpers/metocast-server"
  codesign --force --sign - "$helpers/metocast-server"
fi
sign "$helpers/metocast-server"

# braw-decode develops Blackmagic RAW for the media pipeline. It builds against the Blackmagic RAW
# SDK's headers; without them (CI, a Mac without Blackmagic RAW) the app ships without it and the
# Video screen says what is missing.
braw_sdk="${BRAW_SDK:-/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac}"
if [ -f "$braw_sdk/Include/BlackmagicRawAPI.h" ]; then
  BRAW_SDK="$braw_sdk" "$repo_root/braw-decode/build.sh" "$helpers/braw-decode"
  sign --entitlements "$repo_root/braw-decode/braw-decode.entitlements" "$helpers/braw-decode"
else
  echo "warning: Blackmagic RAW SDK not found at $braw_sdk; building without braw-decode"
fi

# lip-reader measures mouth movement for the media pipeline's audio/video sync. It needs only the
# macOS SDK (AVFoundation and Vision), so every Mac build ships it.
"$repo_root/lip-reader/build.sh" "$helpers/lip-reader"
sign "$helpers/lip-reader"

# ffmpeg and ffprobe for the media pipeline: built from FFmpeg's official release source (LGPL,
# no outside libraries; see ffmpeg/README.md) and cached under target/ffmpeg, so only the first
# build of each version compiles them. Users install nothing.
ffmpeg_build="${TARGET_TEMP_DIR:-$repo_root/target}/ffmpeg"
clean_env MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}" \
  "$repo_root/ffmpeg/build.sh" "$ffmpeg_build"
for program in ffmpeg ffprobe; do
  cp "$ffmpeg_build/$program" "$helpers/$program"
  sign "$helpers/$program"
done
# The LGPL asks for the licence to travel with the binaries.
ffmpeg_licences="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/ffmpeg"
mkdir -p "$ffmpeg_licences"
cp "$ffmpeg_build/LICENSE.md" "$ffmpeg_build/COPYING.LGPLv2.1" "$ffmpeg_licences/"

# The one-click extension for Claude Desktop (a launcher for the bridge in this app; see
# docs/mcp/BRIDGE.md). The app's Connectors screen opens it.
"$repo_root/scripts/build-mcpb.sh" "$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/Metocast.mcpb" "${MARKETING_VERSION:-1.0.0}"
