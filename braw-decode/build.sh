#!/usr/bin/env bash
# Builds braw-decode against the Blackmagic RAW SDK headers.
#   ./build.sh [output]            → build/braw-decode by default
#   BRAW_SDK=/path/to/SDK/Mac ./build.sh
# The SDK is not vendored: install Blackmagic RAW (free) or point BRAW_SDK at its Mac folder.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
sdk="${BRAW_SDK:-/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac}"
out="${1:-$here/build/braw-decode}"
include="$sdk/Include"

if [ ! -f "$include/BlackmagicRawAPI.h" ]; then
  echo "Blackmagic RAW SDK not found at $sdk: install Blackmagic RAW or set BRAW_SDK" >&2
  exit 1
fi

arch_flags=()
for arch in ${ARCHS:-$(uname -m)}; do
  arch_flags+=(-arch "$arch")
done

objects="$(mktemp -d)"
trap 'rm -rf "$objects"' EXIT
common=(-std=c++17 -O2 -Wall -Wextra -mmacosx-version-min=14.0 "${arch_flags[@]}" -I "$include")

clang++ "${common[@]}" -fobjc-arc -c "$here/src/braw_decode.mm" -o "$objects/braw_decode.o"
clang++ "${common[@]}" -Wno-unused-parameter -c "$include/BlackmagicRawAPIDispatch.cpp" -o "$objects/dispatch.o"
mkdir -p "$(dirname "$out")"
clang++ "${arch_flags[@]}" "$objects/braw_decode.o" "$objects/dispatch.o" \
  -framework CoreFoundation -framework CoreGraphics -framework Foundation -framework ImageIO -framework Metal -o "$out"
echo "built $out"
