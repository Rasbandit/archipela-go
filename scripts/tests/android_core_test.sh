#!/usr/bin/env bash
# A failed core build must leave the previous jniLibs in place, not an empty directory (re-review N4).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
# A throwaway repo layout (the script finds its root from its own location) with a cargo that always fails.
repo="$tmp/repo"
jni="$repo/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$repo/scripts" "$repo/core/src" "$repo/core/ffi/src" "$jni" "$tmp/home/.cargo/bin" "$tmp/ndk"
cp "$here/../android_core.sh" "$here/../core_unmutated.sh" "$repo/scripts/"
printf '#!/bin/sh\nexit 1\n' >"$tmp/home/.cargo/bin/cargo"
chmod +x "$tmp/home/.cargo/bin/cargo"
echo old >"$jni/libapgo_ffi.so"
if HOME="$tmp/home" ANDROID_HOME="$tmp" ANDROID_NDK_HOME="$tmp/ndk" bash "$repo/scripts/android_core.sh" >/dev/null 2>&1; then
  echo "FAIL android_core.sh passed with a failing cargo"
  exit 1
fi
if [ "$(cat "$jni/libapgo_ffi.so" 2>/dev/null)" != old ]; then
  echo "FAIL a failed core build emptied jniLibs"
  exit 1
fi
echo "ok failed build keeps jniLibs"

# Each profile reaches cargo-ndk with its own flag: dev for debug, --release, or a named profile (`phone`: optimised dev).
printf '#!/bin/sh\necho "$*" >>"%s"\nexit 1\n' "$tmp/args" >"$tmp/home/.cargo/bin/cargo"
for p in debug dev release phone; do
  : >"$tmp/args"
  HOME="$tmp/home" ANDROID_HOME="$tmp" ANDROID_NDK_HOME="$tmp/ndk" bash "$repo/scripts/android_core.sh" "$p" >/dev/null 2>&1 || true
  args="$(cat "$tmp/args")"
  case "$p" in
    debug | dev) want="build -p apgo-ffi" bad="--" ;; # dev is cargo's name for debug (built into target/*/debug)
    release) want="build -p apgo-ffi --release" bad="--profile" ;;
    phone) want="build -p apgo-ffi --profile phone" bad="--release" ;;
  esac
  if [[ "$args" != *"$want"* ]] || [[ "$args" == *"apgo-ffi $bad"* ]]; then
    echo "FAIL $p profile ran: $args"
    exit 1
  fi
done
echo "ok each profile builds with its flag"
