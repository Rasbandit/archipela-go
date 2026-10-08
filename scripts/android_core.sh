#!/usr/bin/env bash
# Build the Rust core for Android (cargo-ndk) and regenerate the Kotlin UniFFI bindings.
# Usage: scripts/android_core.sh [debug|release]   (default: debug)
# ABIs: APGO_ABIS="arm64-v8a" (default, phone) or "arm64-v8a x86_64" (also emulator)
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
profile="${1:-debug}"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$(find "$ANDROID_HOME/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)}"
export PATH="$HOME/.cargo/bin:$PATH"

flag=()
[ "$profile" = release ] && flag=(--release)

cd "$root/core"
targets=()
for abi in ${APGO_ABIS:-arm64-v8a}; do targets+=(-t "$abi"); done
cargo ndk "${targets[@]}" -o "$root/android/app/src/main/jniLibs" build -p apgo-ffi "${flag[@]}"

out="$root/android/app/src/main/kotlin"
rm -rf "$out/uniffi"
cargo run -q -p apgo-ffi --bin uniffi-bindgen -- generate \
  --library "target/aarch64-linux-android/$profile/libapgo_ffi.so" \
  --language kotlin --no-format --out-dir "$out"
echo "core ($profile) built; bindings in android/app/src/main/kotlin/uniffi"
