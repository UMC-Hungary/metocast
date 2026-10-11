#!/usr/bin/env bash
# Builds the ffmpeg and ffprobe the Mac app ships, from FFmpeg's official release source.
#   ./build.sh [output-dir]        → build/ by default: ffmpeg, ffprobe and the licence files
#   ARCHS="arm64 x86_64" ./build.sh  → universal binaries (x86_64 wants nasm; see README.md)
# The source is pinned by SHA-256 (and checked against FFmpeg's release signature when gpg is
# installed). Each architecture is built once per configuration and cached under target/ffmpeg.
set -euo pipefail

version="9.0.2"
sha256="8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e"
# "FFmpeg release signing key <ffmpeg-devel@ffmpeg.org>", as published on ffmpeg.org/download.html.
signing_key="FCF986EA15E6E293A5644F10B4322F04D67658D8"

here="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$here/.." && pwd)"
out="${1:-$here/build}"
cache="${FFMPEG_CACHE:-$repo_root/target/ffmpeg}"
tarball="$cache/ffmpeg-$version.tar.xz"
deployment_target="${MACOSX_DEPLOYMENT_TARGET:-14.0}"

mkdir -p "$cache" "$out"

if [ ! -f "$tarball" ]; then
  echo "downloading FFmpeg $version source"
  curl -fsSL "https://ffmpeg.org/releases/ffmpeg-$version.tar.xz" -o "$tarball.download"
  mv "$tarball.download" "$tarball"
fi
if [ "$(shasum -a 256 "$tarball" | cut -d ' ' -f 1)" != "$sha256" ]; then
  rm -f "$tarball"
  echo "error: ffmpeg-$version.tar.xz does not match its pinned SHA-256" >&2
  exit 1
fi
if command -v gpg >/dev/null; then
  keyring="$(mktemp -d)"
  trap 'rm -rf "$keyring"' EXIT
  curl -fsSL "https://ffmpeg.org/releases/ffmpeg-$version.tar.xz.asc" -o "$keyring/source.asc"
  curl -fsSL "https://ffmpeg.org/ffmpeg-devel.asc" | GNUPGHOME="$keyring" gpg --quiet --import
  GNUPGHOME="$keyring" gpg --status-fd 1 --verify "$keyring/source.asc" "$tarball" 2>/dev/null \
    | grep -q "VALIDSIG $signing_key" || {
    echo "error: ffmpeg-$version.tar.xz is not signed by FFmpeg's release key" >&2
    exit 1
  }
fi

# LGPL only: no --enable-gpl, no --enable-nonfree, and no outside libraries. Metocast needs
# FFmpeg's own decoders and filters, VideoToolbox's HEVC encoder and the native AAC encoder.
# --disable-autodetect keeps Homebrew libraries on the build Mac out of the binary; the few
# system ones it may use are switched back on.
configure_flags=(
  --disable-autodetect
  --enable-videotoolbox --enable-audiotoolbox --enable-zlib --enable-bzlib
  --disable-shared --enable-static
  --disable-ffplay --disable-doc --disable-debug --disable-network
)

# A cached build is reused only for the same source, flags and deployment target.
build_key="$(printf '%s ' "$sha256" "${configure_flags[@]}" "$deployment_target" | shasum | cut -c 1-12)"
binaries=()
for arch in ${ARCHS:-$(uname -m)}; do
  built="$cache/$version-$arch-$build_key"
  if [ ! -x "$built/ffmpeg" ] || [ ! -x "$built/ffprobe" ]; then
    echo "building FFmpeg $version for $arch"
    source_dir="$cache/src-$arch"
    rm -rf "$source_dir" "$built"
    mkdir -p "$source_dir" "$built"
    tar -xf "$tarball" -C "$source_dir" --strip-components 1
    arch_flags=(--arch="$arch" --cc="clang -arch $arch")
    if [ "$arch" != "$(uname -m)" ]; then
      arch_flags+=(--enable-cross-compile --target-os=darwin)
    fi
    if [ "$arch" = "x86_64" ] && ! command -v nasm >/dev/null; then
      echo "warning: nasm not found; the x86_64 ffmpeg is built without assembly (slower)"
      arch_flags+=(--disable-x86asm)
    fi
    (
      cd "$source_dir"
      ./configure "${arch_flags[@]}" "${configure_flags[@]}" \
        --extra-cflags="-mmacosx-version-min=$deployment_target" \
        --extra-ldflags="-mmacosx-version-min=$deployment_target" >"$built/configure.log"
      make -j"$(sysctl -n hw.ncpu)" ffmpeg ffprobe >"$built/make.log"
    )
    cp "$source_dir/ffmpeg" "$source_dir/ffprobe" "$built/"
    rm -rf "$source_dir"
  fi
  binaries+=("$built")
done

for program in ffmpeg ffprobe; do
  inputs=()
  for built in "${binaries[@]}"; do
    inputs+=("$built/$program")
  done
  lipo -create "${inputs[@]}" -output "$out/$program"
done
tar -xf "$tarball" -C "$out" --strip-components 1 \
  "ffmpeg-$version/LICENSE.md" "ffmpeg-$version/COPYING.LGPLv2.1"
echo "built FFmpeg $version (LGPL) in $out"
