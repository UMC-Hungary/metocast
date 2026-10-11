#!/usr/bin/env bash
# Xcode build phase for apple-app: builds the headless metocast-server and embeds it
# in the Mac app as Contents/MacOS/metocast-server, the helper that server mode runs.
# iOS builds skip it: the iPhone app is client-only.
set -euo pipefail

[ "${PLATFORM_NAME:-}" = "macosx" ] || exit 0

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

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

profile_flag=""
profile_dir="debug"
if [ "${CONFIGURATION:-Debug}" = "Release" ]; then
  profile_flag="--release"
  profile_dir="release"
fi

host_target="$(uname -m | sed 's/arm64/aarch64/')-apple-darwin"
binaries=()
for arch in ${ARCHS}; do
  case "$arch" in
    arm64) target="aarch64-apple-darwin" ;;
    x86_64) target="x86_64-apple-darwin" ;;
    *) echo "error: unsupported macOS architecture $arch" >&2; exit 1 ;;
  esac
  # The host build shares target/<profile> with plain `cargo build`; others need --target.
  if [ "$target" = "$host_target" ]; then
    cargo build --manifest-path "$repo_root/Cargo.toml" -p metocast-server --bin metocast-server $profile_flag
    binaries+=("$repo_root/target/$profile_dir/metocast-server")
  else
    cargo build --manifest-path "$repo_root/Cargo.toml" -p metocast-server --bin metocast-server --target "$target" $profile_flag
    binaries+=("$repo_root/target/$target/$profile_dir/metocast-server")
  fi
done

destination="$TARGET_BUILD_DIR/$EXECUTABLE_FOLDER_PATH/metocast-server"
lipo -create "${binaries[@]}" -output "$destination"

if [ "${CODE_SIGNING_ALLOWED:-YES}" = "YES" ]; then
  codesign --force --options runtime --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" "$destination"
fi

# braw-decode develops Blackmagic RAW for the media pipeline. It builds against the Blackmagic RAW
# SDK's headers; without them (CI, a Mac without Blackmagic RAW) the app ships without it and the
# Video screen says what is missing.
braw_sdk="${BRAW_SDK:-/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac}"
if [ -f "$braw_sdk/Include/BlackmagicRawAPI.h" ]; then
  braw_destination="$TARGET_BUILD_DIR/$EXECUTABLE_FOLDER_PATH/braw-decode"
  ARCHS="$ARCHS" BRAW_SDK="$braw_sdk" "$repo_root/braw-decode/build.sh" "$braw_destination"
  if [ "${CODE_SIGNING_ALLOWED:-YES}" = "YES" ]; then
    codesign --force --options runtime --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" \
      --entitlements "$repo_root/braw-decode/braw-decode.entitlements" "$braw_destination"
  fi
else
  echo "warning: Blackmagic RAW SDK not found at $braw_sdk; building without braw-decode"
fi

# lip-reader measures mouth movement for the media pipeline's audio/video sync. It needs only the
# macOS SDK (AVFoundation and Vision), so every Mac build ships it.
lip_reader_destination="$TARGET_BUILD_DIR/$EXECUTABLE_FOLDER_PATH/lip-reader"
ARCHS="$ARCHS" "$repo_root/lip-reader/build.sh" "$lip_reader_destination"
if [ "${CODE_SIGNING_ALLOWED:-YES}" = "YES" ]; then
  codesign --force --options runtime --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" "$lip_reader_destination"
fi

# ffmpeg and ffprobe for the media pipeline: built from FFmpeg's official release source (LGPL,
# no outside libraries; see ffmpeg/README.md) and cached under target/ffmpeg, so only the first
# build of each version and architecture compiles them. Users install nothing.
ffmpeg_build="${TARGET_TEMP_DIR:-$repo_root/target}/ffmpeg"
clean_env ARCHS="$ARCHS" MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-14.0}" \
  "$repo_root/ffmpeg/build.sh" "$ffmpeg_build"
for program in ffmpeg ffprobe; do
  program_destination="$TARGET_BUILD_DIR/$EXECUTABLE_FOLDER_PATH/$program"
  cp "$ffmpeg_build/$program" "$program_destination"
  if [ "${CODE_SIGNING_ALLOWED:-YES}" = "YES" ]; then
    codesign --force --options runtime --sign "${EXPANDED_CODE_SIGN_IDENTITY:--}" "$program_destination"
  fi
done
# The LGPL asks for the licence to travel with the binaries.
ffmpeg_licences="$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/ffmpeg"
mkdir -p "$ffmpeg_licences"
cp "$ffmpeg_build/LICENSE.md" "$ffmpeg_build/COPYING.LGPLv2.1" "$ffmpeg_licences/"
