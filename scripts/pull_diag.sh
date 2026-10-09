#!/usr/bin/env bash
# Pull everything needed to analyse an outing off the phone: the diagnostics log and raw track, the journal (track + audit), the game
# files and the realm atlases.
# Usage: scripts/pull_diag.sh [out-dir]      (honors ANDROID_SERIAL; debug build only: uses run-as)
# Then: python3 scripts/diag_report.py <out-dir>
set -euo pipefail
pkg=dev.apgo2.app
out="${1:-$(cd "$(dirname "$0")/.." && pwd)/diag/$(date +%Y%m%d-%H%M%S)}"
mkdir -p "$out/files/games" "$out/files/atlas" "$out/diag/raw"

# adb joins its arguments into one string for the phone's shell, so names are quoted for that shell here.
# The names in an app folder, one per line; none when the folder is missing (its error never reaches stdout, even on an adb that
# merges stderr into stdout).
app_ls() { adb shell "run-as $pkg sh -c 'cd $1 2>/dev/null && ls -1'" 2>/dev/null | tr -d '\r' || true; }
# Copy one app file to $2; a file that cannot be read is skipped, not fatal.
app_cat() { adb exec-out "run-as $pkg cat $(printf %q "$1")" >"$2" 2>/dev/null || rm -f "$2"; }
# Copy every file of app folder $1 into $2 (the raw/ subfolder is pulled on its own).
app_cp() {
  while IFS= read -r f; do
    if [ -n "$f" ] && [ "$f" != raw ]; then app_cat "$1/$f" "$2/$f"; fi
  done < <(app_ls "$1")
}

# Internal files: journal (WAL needs all three files), games, realms.
for f in journal.db journal.db-wal journal.db-shm home.json realms.json; do
  app_cat "files/$f" "$out/files/$f"
done
app_cp files/games "$out/files/games"
# Realm atlases (street graph for map matching in the replay bench).
app_cp files/atlas "$out/files/atlas"
# Diagnostics log, internal only (app-specific external storage is readable by other apps on Android 8 and 9).
app_cp files/diag "$out/diag"
# Raw track (debug builds): fixes, steps and compass for the replay bench.
app_cp files/diag/raw "$out/diag/raw"
adb logcat -d -v threadtime > "$out/logcat.txt" 2>/dev/null || true
adb shell getprop ro.product.model > "$out/device.txt" 2>/dev/null || true
echo "pulled to $out"
find "$out/diag" -mindepth 1 -maxdepth 1 -printf '%M %s %TY-%Tm-%Td %TH:%TM %f\n'
