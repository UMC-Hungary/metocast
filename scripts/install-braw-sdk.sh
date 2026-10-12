#!/usr/bin/env bash
# Puts the Blackmagic RAW SDK where a build can compile braw-decode against it, and prints the
# folder to use as BRAW_SDK (the SDK's `Mac` folder, the one holding `Include/BlackmagicRawAPI.h`).
#
#   scripts/install-braw-sdk.sh [destination]     # destination defaults to target/braw-sdk
#
# Where it looks, in order:
#   1. An installed Blackmagic RAW: /Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac. Any Mac
#      that works with Blackmagic RAW already has it, so a build there needs nothing from here.
#   2. BRAW_SDK_URL: an archive holding the SDK (.zip, .dmg, .pkg or .tar.gz), for a machine with
#      no install, such as a GitHub Actions runner. BRAW_SDK_TOKEN, when set, is sent as a bearer
#      token, which a release asset in a private repository needs.
#
# Blackmagic's licence keeps the SDK out of this repository, and their download sits behind a
# registration form, so a build machine cannot fetch it from Blackmagic itself: BRAW_SDK_URL has
# to name a copy you host. Only the headers are needed to build; the framework braw-decode loads
# at run time comes from the Mac it runs on, where the person installed Blackmagic RAW.
#
# In GitHub Actions the folder is also written to $GITHUB_ENV as BRAW_SDK, which the app's build
# phase (scripts/embed-apple-server.sh) reads.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
destination="${1:-$repo_root/target/braw-sdk}"
mkdir -p "$(dirname "$destination")"
destination="$(cd "$(dirname "$destination")" && pwd)/$(basename "$destination")"
installed="/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac"

# The folder to hand the compiler is wherever the SDK's headers are: an archive may hold the whole
# SDK, its Mac folder or only Include, and each unpacks to a different depth.
sdk_folder() {
  local header include
  header="$(find "$1" -type f -name BlackmagicRawAPI.h -print -quit)"
  [ -n "$header" ] || return 1
  include="$(dirname "$header")"
  # braw-decode compiles the dispatch source, which loads the framework at run time, alongside it.
  [ -f "$include/BlackmagicRawAPIDispatch.cpp" ] || return 1
  dirname "$include"
}

# Says where the SDK is, and hands it to the steps that follow in a GitHub Actions job.
announce() {
  echo "Blackmagic RAW SDK: $1"
  if [ -n "${GITHUB_ENV:-}" ]; then
    echo "BRAW_SDK=$1" >>"$GITHUB_ENV"
  fi
}

if [ -f "$installed/Include/BlackmagicRawAPI.h" ]; then
  announce "$installed"
  exit 0
fi

if [ -z "${BRAW_SDK_URL:-}" ]; then
  cat >&2 <<'MESSAGE'
error: no Blackmagic RAW SDK on this machine, and BRAW_SDK_URL is not set.

braw-decode is compiled against the SDK's headers, so a build without them ships an app that
cannot develop Blackmagic RAW clips. Either install Blackmagic RAW (free, from blackmagicdesign.com)
on this machine, or set BRAW_SDK_URL to an archive of the SDK that this build can download.

For GitHub Actions, see "The SDK on a build machine" in braw-decode/README.md: host a .zip of
"/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac" somewhere private, then add the repository
secrets BRAW_SDK_URL (and BRAW_SDK_TOKEN if the download needs an Authorization header).
MESSAGE
  exit 1
fi

work="$(mktemp -d)"
mounts="$work/mounts"
mkdir -p "$mounts"
cleanup() {
  # A disk image has to be unmounted before its folder can go, and a failure here would otherwise
  # be invisible: the next build on this runner would find the image still attached.
  for mounted in "$mounts"/*; do
    if [ -d "$mounted" ] && ! hdiutil detach "$mounted" -force; then
      echo "the disk image mounted at $mounted could not be unmounted" >&2
    fi
  done
  rm -rf "$work"
}
trap cleanup EXIT

# The suffix decides how to unpack, and a download URL may carry a query string (a signed link) or
# none at all (a GitHub release asset is /releases/assets/<id>), so sniff the file as well.
archive="$work/blackmagic-raw-sdk"
case "${BRAW_SDK_URL%%\?*}" in
  *.zip) kind=zip ;;
  *.dmg) kind=dmg ;;
  *.pkg | *.mpkg) kind=pkg ;;
  *.tar.gz | *.tgz | *.tar) kind=tar ;;
  *) kind=sniff ;;
esac

echo "Downloading the Blackmagic RAW SDK"
curl_args=(--fail --location --retry 3 --retry-connrefused --output "$archive")
if [ -n "${BRAW_SDK_TOKEN:-}" ]; then
  # What a GitHub release asset in a private repository needs; harmless for a signed link.
  curl_args+=(--header "Authorization: Bearer $BRAW_SDK_TOKEN" --header "Accept: application/octet-stream")
fi
curl "${curl_args[@]}" "$BRAW_SDK_URL"

if [ "$kind" = sniff ]; then
  description="$(file -b "$archive")"
  case "$description" in
    *"Zip archive"*) kind=zip ;;
    *"xar archive"*) kind=pkg ;;
    *"Apple Disk Image"*) kind=dmg ;;
    *"gzip compressed"* | *"POSIX tar"*) kind=tar ;;
    *)
      echo "error: BRAW_SDK_URL downloaded a file this script cannot open: $description" >&2
      exit 1
      ;;
  esac
  echo "The download is a $kind ($description)"
fi

# pkgutil and hdiutil go by the file's name, and a download has none of its own.
mv "$archive" "$archive.$kind"
archive="$archive.$kind"

unpacked="$work/unpacked"
mkdir -p "$unpacked"
case "$kind" in
  zip) ditto -x -k "$archive" "$unpacked" ;;
  tar) tar -xf "$archive" -C "$unpacked" ;;
  pkg) pkgutil --expand-full "$archive" "$unpacked/package" ;;
  dmg)
    mount="$mounts/image"
    mkdir -p "$mount"
    hdiutil attach "$archive" -readonly -nobrowse -mountpoint "$mount"
    ditto "$mount" "$unpacked"
    ;;
esac

# Blackmagic's own installer is a package inside a disk image, so the headers can sit a layer
# further in; a plain .zip of the SDK holds no package and the loop does nothing.
expanded="$work/expanded"
mkdir -p "$expanded"
while IFS= read -r package; do
  echo "Expanding $(basename "$package")"
  pkgutil --expand-full "$package" "$expanded/$(basename "$package")"
done < <(find "$unpacked" -type f \( -name '*.pkg' -o -name '*.mpkg' \))

found=""
for room in "$unpacked" "$expanded"; do
  if candidate="$(sdk_folder "$room")"; then
    found="$candidate"
    break
  fi
done

if [ -z "$found" ]; then
  echo "error: BRAW_SDK_URL downloaded no Blackmagic RAW SDK." >&2
  echo "A build needs Include/BlackmagicRawAPI.h and Include/BlackmagicRawAPIDispatch.cpp; a .zip" >&2
  echo "of \"/Applications/Blackmagic RAW/Blackmagic RAW SDK/Mac\" holds them. The download unpacked to:" >&2
  find "$unpacked" -maxdepth 3 >&2
  exit 1
fi

rm -rf "$destination"
mkdir -p "$(dirname "$destination")"
ditto "$found" "$destination"
announce "$destination"
