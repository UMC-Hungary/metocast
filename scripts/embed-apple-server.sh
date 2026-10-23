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
strip_helper=false
if [ "${CONFIGURATION:-Debug}" = "Release" ]; then
  profile_flag="--release"
  strip_helper=true
fi

# The host build shares its target directory with plain `cargo build`. Copy the binary cargo says it
# built: CARGO_TARGET_DIR or a `build.target-dir` in ~/.cargo/config.toml can move that directory
# away from target/, and copying a guessed target/<profile> would ship an old server.
build_output=$(cargo build --manifest-path "$repo_root/Cargo.toml" -p metocast-server --bin metocast-server \
  $profile_flag --message-format=json-render-diagnostics)
helper_bin=$(printf '%s\n' "$build_output" | sed -n 's/.*"executable":"\([^"]*\/metocast-server\)".*/\1/p' | tail -n 1)
if [ -z "$helper_bin" ]; then
  echo "error: cargo reported no metocast-server binary" >&2
  exit 1
fi
cp "$helper_bin" "$helpers/metocast-server"

# A shipped build leaves out the local symbol table, about a fifth of the binary and only useful
# to a debugger. Stripping drops the signature: sign it again, ad hoc here and with Xcode's
# identity below when signing is on, so an arm64 binary is never left unsigned.
if $strip_helper; then
  strip -x "$helpers/metocast-server"
  codesign --force --sign - "$helpers/metocast-server"
fi
sign "$helpers/metocast-server"

# braw-decode develops Blackmagic RAW for the media pipeline. It compiles against the Blackmagic
# RAW SDK's headers, which Blackmagic's licence keeps out of this repository, so a Mac with the SDK
# installed builds it from source and every other machine uses braw-decode/prebuilt, built by
# someone who had it. The binary loads Blackmagic's framework at run time, so it runs anywhere.
# Never silent: an app with no braw-decode cannot open a .braw file at all, so this stops instead.
braw_sdk="${BRAW_SDK:-/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac}"
if [ -f "$braw_sdk/Include/BlackmagicRawAPI.h" ]; then
  BRAW_SDK="$braw_sdk" "$repo_root/braw-decode/build.sh" "$helpers/braw-decode"
elif [ -x "$repo_root/braw-decode/prebuilt/braw-decode" ]; then
  cp "$repo_root/braw-decode/prebuilt/braw-decode" "$helpers/braw-decode"
else
  echo "error: no Blackmagic RAW SDK at $braw_sdk and no braw-decode/prebuilt/braw-decode" >&2
  exit 1
fi
sign --entitlements "$repo_root/braw-decode/braw-decode.entitlements" "$helpers/braw-decode"

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

# The server's web pages (/transcript, /presenter): the helper serves them from
# METOCAST_STATIC_DIR, which ServerHost points at Resources/web. `pnpm build` makes them; it runs
# outside Xcode because Node isn't on this phase's PATH. Never silent: without them every page 404s.
if [ ! -f "$repo_root/build/transcript.html" ]; then
  echo "error: no web build at $repo_root/build; run pnpm build first" >&2
  exit 1
fi
web="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/web"
rm -rf "$web"
cp -R "$repo_root/build" "$web"

# The one-click extension for Claude Desktop (a launcher for the bridge in this app; see
# docs/mcp/BRIDGE.md). The app's Connectors screen opens it.
"$repo_root/scripts/build-mcpb.sh" "$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/Metocast.mcpb" "${MARKETING_VERSION:-1.0.0}"
