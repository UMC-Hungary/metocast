#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
generated_dir="$repo_root/target/apple-bindings"
headers_dir="$repo_root/target/apple-headers"
package_dir="$repo_root/apple"

export IPHONEOS_DEPLOYMENT_TARGET=16.0
export MACOSX_DEPLOYMENT_TARGET=13.0

cd "$repo_root"
rm -rf "$generated_dir" "$headers_dir" "$package_dir/MetocastFFI.xcframework"
mkdir -p "$generated_dir" "$headers_dir" "$package_dir/Sources/Metocast"

# Ask cargo where it builds: CARGO_TARGET_DIR or a `build.target-dir` in ~/.cargo/config.toml can
# move that directory away from target/, and a guessed target/ path would find no library.
cargo_target=$(cargo metadata --format-version 1 --no-deps | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
if [ -z "$cargo_target" ]; then
  echo "error: cargo reported no target directory" >&2
  exit 1
fi

for target in aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin; do
  cargo build --release -p metocast-apple --target "$target"
done

cargo build --release -p metocast-apple
cargo run -p metocast-apple --bin uniffi-bindgen -- \
  generate \
  --library "$cargo_target/release/libmetocast_apple.a" \
  --language swift \
  --out-dir "$generated_dir"

cp "$generated_dir"/*.swift "$package_dir/Sources/Metocast/Metocast.swift"
cp "$generated_dir"/*.h "$headers_dir/"
cp "$generated_dir"/*.modulemap "$headers_dir/module.modulemap"

xcodebuild -create-xcframework \
  -library "$cargo_target/aarch64-apple-ios/release/libmetocast_apple.a" -headers "$headers_dir" \
  -library "$cargo_target/aarch64-apple-ios-sim/release/libmetocast_apple.a" -headers "$headers_dir" \
  -library "$cargo_target/aarch64-apple-darwin/release/libmetocast_apple.a" -headers "$headers_dir" \
  -output "$package_dir/MetocastFFI.xcframework"

swift test --package-path "$package_dir"
