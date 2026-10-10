#!/usr/bin/env bash
# Tests for scripts/core_unmutated.sh and that every core build path runs it (re-review N4: a release must not ship a mutated core).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
sut="$here/../core_unmutated.sh"
fail=0
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/core/src" "$tmp/core/ffi/src"
echo "fn ok() {}" >"$tmp/core/src/lib.rs"
if bash "$sut" "$tmp" >/dev/null 2>&1; then echo "ok clean core passes"; else echo "FAIL clean core rejected"; fail=1; fi
echo "fn bad() {} // changed by cargo-mutants" >"$tmp/core/ffi/src/lib.rs"
if bash "$sut" "$tmp" >/dev/null 2>&1; then echo "FAIL mutated core passes"; fail=1; else echo "ok mutated core fails"; fi
if grep -q core_unmutated.sh "$here/../android_core.sh"; then echo "ok android_core.sh runs the guard"; else echo "FAIL android_core.sh skips the guard"; fail=1; fi
exit "$fail"
