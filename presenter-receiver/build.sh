#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# build.sh — Build presenter-receiver for the current platform.
#
# Run this script on each target machine to get a native binary:
#
#   macOS (Apple Silicon) : ./build.sh
#   Linux ARM64 (RPi)     : ./build.sh
#   Linux x86_64          : ./build.sh
#
# After building, upload the binary somewhere (e.g. your server or GitHub
# Releases) and share the wget lines shown at the end.
# ─────────────────────────────────────────────────────────────────────────────
set -e

CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"
PKG_CONFIG_PATH_EXTRA=""

# ── Platform detection ────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}-${ARCH}" in
    Darwin-arm64)
        PLATFORM="macos-arm64"
        # Homebrew on Apple Silicon
        PKG_CONFIG_PATH_EXTRA="/opt/homebrew/lib/pkgconfig:/opt/homebrew/opt/cairo/lib/pkgconfig:/opt/homebrew/opt/pango/lib/pkgconfig"
        ;;
    Linux-aarch64)
        PLATFORM="linux-arm64"
        ;;
    Linux-x86_64)
        PLATFORM="linux-x86_64"
        ;;
    *)
        echo "Unsupported platform: ${OS}-${ARCH}"
        exit 1
        ;;
esac

echo "==> Platform: $PLATFORM"

# ── Install system dependencies ───────────────────────────────────────────────
if [[ "$OS" == "Darwin" ]]; then
    echo "==> Checking Homebrew dependencies..."
    for pkg in cairo pango pkg-config; do
        brew list "$pkg" &>/dev/null || brew install "$pkg"
    done
fi

if [[ "$OS" == "Linux" ]]; then
    echo "==> Checking apt dependencies..."
    MISSING=()
    for pkg in libcairo2-dev libpango1.0-dev pkg-config; do
        dpkg -s "$pkg" &>/dev/null || MISSING+=("$pkg")
    done
    if [[ ${#MISSING[@]} -gt 0 ]]; then
        echo "    Installing: ${MISSING[*]}"
        sudo apt-get update -qq
        sudo apt-get install -y "${MISSING[@]}"
    fi
fi

# ── Build ─────────────────────────────────────────────────────────────────────
echo "==> Building release binary..."
cd "$(dirname "$0")"

PKG_CONFIG_PATH="$PKG_CONFIG_PATH_EXTRA:$PKG_CONFIG_PATH" \
    "$CARGO" build --release

BIN="target/release/presenter-receiver"
OUT="presenter-receiver-${PLATFORM}"
cp "$BIN" "$OUT"

echo ""
echo "✓  Binary: $(pwd)/$OUT"
echo "   Size:   $(du -sh "$OUT" | cut -f1)"
echo ""

# ── Distribution instructions ─────────────────────────────────────────────────
echo "─────────────────────────────────────────────────────────────────────────"
echo " To distribute, upload $OUT to your server or GitHub Releases, then:"
echo ""
echo " # Linux ARM64 (Raspberry Pi)"
echo " wget https://YOUR_HOST/releases/presenter-receiver-linux-arm64 -O presenter-receiver"
echo " chmod +x presenter-receiver"
echo " ./presenter-receiver ws://YOUR_SERVER_IP:3737/ws"
echo ""
echo " # Linux x86_64"
echo " wget https://YOUR_HOST/releases/presenter-receiver-linux-x86_64 -O presenter-receiver"
echo " chmod +x presenter-receiver"
echo " ./presenter-receiver ws://YOUR_SERVER_IP:3737/ws"
echo ""
echo " # macOS (Apple Silicon)"
echo " curl -L https://YOUR_HOST/releases/presenter-receiver-macos-arm64 -o presenter-receiver"
echo " chmod +x presenter-receiver"
echo " ./presenter-receiver ws://YOUR_SERVER_IP:3737/ws"
echo ""
echo " Optional token (read-only, no token needed for display-only use):"
echo " ./presenter-receiver ws://IP:3737/ws --token YOUR_TOKEN"
echo "─────────────────────────────────────────────────────────────────────────"
