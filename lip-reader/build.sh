#!/usr/bin/env bash
# Builds lip-reader, which needs only the macOS SDK (AVFoundation and Vision).
#   ./build.sh [output]            → build/lip-reader by default
#   ARCHS="arm64 x86_64" ./build.sh  → a universal binary
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="${1:-$here/build/lip-reader}"
sdk="$(xcrun --sdk macosx --show-sdk-path)"

slices=()
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
for arch in ${ARCHS:-$(uname -m)}; do
  xcrun --sdk macosx swiftc -O -sdk "$sdk" -target "$arch-apple-macos14.0" \
    -module-name lip_reader "$here/src/main.swift" -o "$work/lip-reader-$arch"
  slices+=("$work/lip-reader-$arch")
done
mkdir -p "$(dirname "$out")"
lipo -create "${slices[@]}" -output "$out"
echo "built $out"
