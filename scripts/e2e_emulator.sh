#!/usr/bin/env bash
# Full regression on the emulator, from a fresh install state:
#   fresh app data -> realm around a GPS fix -> real map scan -> solo game -> autoplay to the win.
# Needs: `just emu-start`, the app installed (`just emu-run`), network (public Overpass servers).
#   GOAL="Quest-dex" LON=-122.6795 LAT=45.5189 scripts/e2e_emulator.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
export ANDROID_SERIAL="${ANDROID_SERIAL:-emulator-5554}"
lon="${LON:--122.6795}"; lat="${LAT:-45.5189}"; goal="${GOAL:-Quest-dex}"
ui() { python3 -I "$root/scripts/android_ui.py" "$@"; }
app=dev.apgo2.app

adb shell pm clear "$app" >/dev/null
adb shell pm grant "$app" android.permission.ACCESS_FINE_LOCATION
adb shell pm grant "$app" android.permission.ACTIVITY_RECOGNITION
adb shell am start -n "$app/dev.apgo2.MainActivity" >/dev/null
sleep 6
for _ in 1 2 3; do adb emu geo fix "$lon" "$lat" >/dev/null 2>&1; sleep 2; done

echo "1/4 creating a realm and scanning (public map servers; can take a few minutes)"
ui tap "Circle around me" >/dev/null
# the realm card also has a "Scan" button, so wait for the status line instead
for _ in $(seq 1 120); do
  t="$(ui texts)"
  case "$t" in *"Scan done"* | *"Scan failed"*) break ;; esac
  sleep 3
done
case "$t" in *"Scan done"*) ;; *) echo "FAIL: scan did not finish or failed (network?)"; exit 1 ;; esac
echo "   scan: $(echo "$t" | grep -oE "[0-9]+ places · [0-9]+ quest kinds on offer|⚠[^|]*" | tr '\n' ' ')"
echo "2/4 starting a solo game"
ui tap "Play" exact >/dev/null; sleep 1
ui tap "New game" exact >/dev/null || { echo "FAIL: cannot open New Game"; exit 1; }; sleep 1
ui tap "+ Around me (walk)" >/dev/null; sleep 1
ui tap "$goal" >/dev/null; sleep 1
ui tap "Play solo" >/dev/null; sleep 8
ui texts | grep -q "Quests 0/" || { echo "FAIL: game did not start"; exit 1; }
echo "3/4 autoplaying"
"$root/scripts/e2e_autoplay.sh" "${MAX:-80}" | tail -3 | tee /tmp/apgo-e2e.out
grep -q "WON" /tmp/apgo-e2e.out || { echo "FAIL: goal not reached"; exit 1; }
echo "4/4 PASS"
