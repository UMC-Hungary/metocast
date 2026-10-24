#!/usr/bin/env bash
# Builds the whisper-cli the Mac app ships, from whisper.cpp's release source (MIT).
#   ./build.sh [output-dir]        → build/ by default: whisper-cli and LICENSE
# The source is pinned by SHA-256. One self-contained arm64 binary: Metal with its shaders
# embedded, Accelerate, and nothing from Homebrew. Cached under target/whisper per configuration.
set -euo pipefail

version="1.9.5"
sha256="ff1a9053feb509ff9d7729703355541ae9690073a6b1c40eb692c962e0dc1720"

here="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$here/.." && pwd)"
out="${1:-$here/build}"
cache="${WHISPER_CACHE:-$repo_root/target/whisper}"
tarball="$cache/whisper.cpp-$version.tar.gz"
deployment_target="${MACOSX_DEPLOYMENT_TARGET:-14.0}"

mkdir -p "$cache" "$out"

if [ ! -f "$tarball" ]; then
  echo "downloading whisper.cpp $version source"
  curl -fsSL "https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v$version.tar.gz" -o "$tarball.download"
  mv "$tarball.download" "$tarball"
fi
if [ "$(shasum -a 256 "$tarball" | cut -d ' ' -f 1)" != "$sha256" ]; then
  rm -f "$tarball"
  echo "error: whisper.cpp-$version.tar.gz does not match its pinned SHA-256" >&2
  exit 1
fi

# GGML_NATIVE=OFF: built for every Apple Silicon Mac, not only the CPU of the Mac that builds it.
# OpenMP, SDL2 and curl are left out: the CLI needs none of them, and finding them would pull in
# Homebrew's libraries. No ccache either, so the build doesn't depend on what the Mac has.
cmake_flags=(
  -DCMAKE_BUILD_TYPE=Release
  -DCMAKE_OSX_ARCHITECTURES=arm64
  -DCMAKE_OSX_DEPLOYMENT_TARGET="$deployment_target"
  -DBUILD_SHARED_LIBS=OFF
  -DGGML_NATIVE=OFF
  -DGGML_METAL=ON
  -DGGML_METAL_EMBED_LIBRARY=ON
  -DGGML_OPENMP=OFF
  -DGGML_CCACHE=OFF
  -DWHISPER_BUILD_TESTS=OFF
  -DWHISPER_BUILD_SERVER=OFF
  -DWHISPER_SDL2=OFF
  -DWHISPER_CURL=OFF
)

build_key="$(printf '%s ' "$sha256" "${cmake_flags[@]}" | shasum | cut -c 1-12)"
built="$cache/$version-arm64-$build_key"
if [ ! -x "$built/whisper-cli" ]; then
  echo "building whisper.cpp $version for arm64"
  source_dir="$cache/src-arm64"
  rm -rf "$source_dir" "$built"
  mkdir -p "$source_dir" "$built"
  tar -xzf "$tarball" -C "$source_dir" --strip-components 1
  # whisper.cpp still declares `cmake_minimum_required(VERSION 3.5)`, which CMake 4 reports as
  # deprecated on every configure. Give it the range CMake asks for: built and tested up to 4.4.
  sed -i '' '1s/^cmake_minimum_required(VERSION 3\.5)/cmake_minimum_required(VERSION 3.5...4.4)/' \
    "$source_dir/CMakeLists.txt"
  head -1 "$source_dir/CMakeLists.txt" | grep -q '^cmake_minimum_required(VERSION 3\.5\.\.\.4\.4)' \
    || { echo "error: whisper.cpp's CMakeLists.txt no longer starts as expected" >&2; exit 1; }
  cmake -S "$source_dir" -B "$source_dir/build" "${cmake_flags[@]}" >"$built/cmake.log"
  cmake --build "$source_dir/build" --target whisper-cli -j "$(sysctl -n hw.ncpu)" >"$built/build.log"
  cp "$source_dir/build/bin/whisper-cli" "$built/"
  rm -rf "$source_dir"
fi

# Never ship a binary that loads a library from outside macOS.
if otool -L "$built/whisper-cli" | tail -n +2 | grep -vE "^\s+(/usr/lib/|/System/Library/)"; then
  echo "error: whisper-cli links a library outside macOS (see above)" >&2
  exit 1
fi

cp "$built/whisper-cli" "$out/"
tar -xzf "$tarball" -C "$out" --strip-components 1 "whisper.cpp-$version/LICENSE"
echo "built whisper.cpp $version (MIT) in $out"
