#!/usr/bin/env bash
# Generate the Kotlin UniFFI bindings from a host build (no NDK). Enough for
# lint, detekt and JVM unit tests; the phone build still uses android_core.sh.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/core"
cargo build -q -p apgo-ffi
out="$root/android/app/src/main/kotlin"
rm -rf "$out/uniffi"
cargo run -q -p apgo-ffi --bin uniffi-bindgen -- generate \
  --library target/debug/libapgo_ffi.so --language kotlin --no-format --out-dir "$out"
