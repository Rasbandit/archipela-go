#!/usr/bin/env bash
# Pull everything needed to analyse an outing off the phone: the diagnostics log, the journal (track + audit) and the game files.
# Usage: scripts/pull_diag.sh [out-dir]      (honors ANDROID_SERIAL; debug build only: uses run-as)
# Then: python3 scripts/diag_report.py <out-dir>
set -euo pipefail
pkg=dev.apgo2.app
out="${1:-$(cd "$(dirname "$0")/.." && pwd)/diag/$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$out/files/games" "$out/diag"

# Internal files: journal (WAL needs all three files), games, realms.
for f in journal.db journal.db-wal journal.db-shm home.json realms.json; do
  adb exec-out run-as "$pkg" cat "files/$f" > "$out/files/$f" 2>/dev/null || rm -f "$out/files/$f"
done
for g in $(adb shell run-as "$pkg" ls files/games 2>/dev/null | tr -d '\r'); do
  adb exec-out run-as "$pkg" cat "files/games/$g" > "$out/files/games/$g"
done
# Diagnostics log: external app dir first, internal fallback.
ext="/sdcard/Android/data/$pkg/files/diag"
if adb shell "ls $ext" >/dev/null 2>&1; then
  adb pull "$ext/." "$out/diag/" >/dev/null
else
  for f in $(adb shell run-as "$pkg" ls files/diag 2>/dev/null | tr -d '\r'); do
    adb exec-out run-as "$pkg" cat "files/diag/$f" > "$out/diag/$f"
  done
fi
adb logcat -d -v threadtime > "$out/logcat.txt" 2>/dev/null || true
adb shell getprop ro.product.model > "$out/device.txt" 2>/dev/null || true
echo "pulled to $out"
ls -la "$out/diag" | tail -n +2
