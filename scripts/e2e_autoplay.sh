#!/usr/bin/env bash
# Autoplay the open game on the emulator/phone with the in-app dev simulator until the goal is met.
#   ANDROID_SERIAL=emulator-5554 scripts/e2e_autoplay.sh [max_iterations]
set -uo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
ui() { python3 -I "$root/scripts/android_ui.py" "$@"; }
max="${1:-80}"
for i in $(seq 1 "$max"); do
  ui tap "DEV: do next" >/dev/null
  sleep 4
  t="$(ui texts)"
  hud="$(echo "$t" | grep -oE "Quests [0-9]+/[0-9]+ · keys [0-9]+ · [^·]+" | head -1)"
  zones="$(echo "$t" | grep -oE "Z[0-9] (walk|run|bike|car) [^ ]+" | tr '\n' ' ')"
  [ $((i % 5)) -eq 0 ] && echo "[$i] $hud | $zones"
  case "$t" in *"You won"* | *"GOAL ACHIEVED"*) echo "[$i] WON: $hud"; exit 0 ;; esac
done
echo "stopped after $max iterations: $hud"
