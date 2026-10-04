#!/usr/bin/env bash
# Xcode build phase for apple-app: builds the headless metocast-server and embeds it
# in the Mac app as Contents/MacOS/metocast-server, the helper that server mode runs.
# iOS builds skip it: the iPhone app is client-only.
set -euo pipefail

[ "${PLATFORM_NAME:-}" = "macosx" ] || exit 0

repo_root="$(cd "$(dirname "$0")/.." && pwd)"

# Cargo runs without Xcode's environment (SDK, deployment target and toolchain settings for
# the app): it breaks the C dependencies' builds and would invalidate cargo's cache on every
# switch between Xcode and terminal builds.
cargo() {
  env -i HOME="$HOME" USER="${USER:-}" TMPDIR="${TMPDIR:-/tmp}" \
    PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    cargo "$@"
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
