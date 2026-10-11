#!/usr/bin/env bash
# Captures the Metocast window into docs/images/<feature>/<name>.png, then runs the privacy guard.
#
#   scripts/docs-screenshot.sh <feature> <name>
#
# The app must already be running with redaction on, so the computer name, LAN address and token
# are placeholders. Use the test copy (own bundle id, own port) and never Aron's running app:
#
#   open -n path/to/Metocast.app --env METOCAST_PORT=3738 --env METOCAST_PG_PORT=15433 \
#     --args -MetocastRedact YES
#
# Then bring up the screen to capture and run this script. See docs/mcp/README.md for an example.
set -euo pipefail

feature="${1:?feature folder name}"
name="${2:?screenshot name}"
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/docs/images/$feature/$name.png"

# Only the copy launched with redaction on, never another Metocast that is running.
if ! pids=$(pgrep -f "Metocast.app/Contents/MacOS/Metocast .*MetocastRedact"); then
  echo "No Metocast window is running with -MetocastRedact YES; refusing to capture." >&2
  exit 1
fi
pid=${pids%%$'\n'*}

window=$(PID="$pid" swift - <<'SWIFT'
import CoreGraphics
import Foundation
let wanted = Int(ProcessInfo.processInfo.environment["PID"] ?? "") ?? -1
let all = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for window in all where (window[kCGWindowOwnerPID as String] as? Int) == wanted
    && (window[kCGWindowLayer as String] as? Int) == 0 {
    print(window[kCGWindowNumber as String] ?? "")
    break
}
SWIFT
)
[ -n "$window" ] || { echo "The redacted Metocast has no visible window." >&2; exit 1; }

mkdir -p "$(dirname "$out")"
screencapture -l "$window" -o "$out"
if ! swift "$repo_root/scripts/check-screenshots.swift" "$(dirname "$out")"; then
  rm -f "$out"
  echo "Removed $out: it shows something that identifies this machine." >&2
  exit 1
fi
echo "$out"
