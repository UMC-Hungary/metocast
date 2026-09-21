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

for target in aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release -p metocast-apple --target "$target"
done

cargo build --release -p metocast-apple
cargo run -p metocast-apple --bin uniffi-bindgen -- \
  generate \
  --library target/release/libmetocast_apple.a \
  --language swift \
  --out-dir "$generated_dir"

cp "$generated_dir"/*.swift "$package_dir/Sources/Metocast/Metocast.swift"
cp "$generated_dir"/*.h "$headers_dir/"
cp "$generated_dir"/*.modulemap "$headers_dir/module.modulemap"

lipo -create \
  target/aarch64-apple-darwin/release/libmetocast_apple.a \
  target/x86_64-apple-darwin/release/libmetocast_apple.a \
  -output target/libmetocast_apple_macos.a

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libmetocast_apple.a -headers "$headers_dir" \
  -library target/aarch64-apple-ios-sim/release/libmetocast_apple.a -headers "$headers_dir" \
  -library target/libmetocast_apple_macos.a -headers "$headers_dir" \
  -output "$package_dir/MetocastFFI.xcframework"

swift test --package-path "$package_dir"
