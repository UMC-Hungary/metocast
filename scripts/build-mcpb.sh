#!/usr/bin/env bash
# Builds the one-click extension for Claude Desktop (an .mcpb, which is a zip with a manifest).
# It holds no binary, only a launcher for the bridge inside the Metocast app: see
# scripts/mcpb/launch.sh and docs/mcp/BRIDGE.md.
#
# usage: scripts/build-mcpb.sh <output.mcpb> [version]
set -euo pipefail

[ $# -ge 1 ] || { echo "usage: $0 <output.mcpb> [version]" >&2; exit 2; }
out="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
version="${2:-1.0.0}"
# The manifest wants x.y.z; Xcode's MARKETING_VERSION is often x.y.
[[ "$version" =~ ^[0-9]+\.[0-9]+$ ]] && version="$version.0"
source_dir="$(cd "$(dirname "$0")/mcpb" && pwd)"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir "$work/server"
sed "s/__VERSION__/$version/" "$source_dir/manifest.json" > "$work/manifest.json"
cp "$source_dir/launch.sh" "$work/server/launch.sh"
rm -f "$out"
(cd "$work" && zip -qrX "$out" manifest.json server)
