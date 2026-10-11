#!/usr/bin/env bash
# Builds lip-reader, which needs only the macOS SDK (AVFoundation and Vision).
#   ./build.sh [output]            → build/lip-reader by default
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="${1:-$here/build/lip-reader}"
sdk="$(xcrun --sdk macosx --show-sdk-path)"

mkdir -p "$(dirname "$out")"
xcrun --sdk macosx swiftc -O -sdk "$sdk" -target arm64-apple-macos14.0 \
  -module-name lip_reader "$here/src/main.swift" -o "$out"
echo "built $out"
