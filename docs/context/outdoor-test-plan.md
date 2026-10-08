# Context Doc: outdoor test plan and diagnostics

_Written 2026-10-07. First outdoor test = SOLO on the device. Archipelago second (its end-to-end flow was not re-tested after the goals rework)._

## Before leaving
1. Phone (Pixel 8 Pro): install the debug APK (`just android-run`, or `adb -s <phone> install -r android/app/build/outputs/apk/debug/app-debug.apk`).
2. Grant location **"While using the app"/Precise**, Physical activity, Notifications when asked.
3. Settings > Apps > Archipela-Go 2 > Battery > **Unrestricted** (otherwise Doze can pause tracking). The heartbeat log shows `unrestricted`.
4. Open a realm around where you will walk, scan it, start a **solo** game (Walk zone, a few easy quest kinds: Point, Away, Steps, Dwell), open Play. A "Quest tracking" notification must appear.
5. Turn "Real GPS" on (header must say real, not SIMULATED). Do not use the DEV buttons outside.

## What to try (about 30 minutes)
- Walk with the screen off in a pocket for 10 min, then look at the map: the trace line should cover the walk.
- A Point quest (walk to it), an Away quest (be away from home for the minutes it asks), a Steps quest.
- Stand still 3 min inside a Dwell place: progress must continue (no distance filter while playing).
- Leave the app (Home), come back after more than 1 min: the "While you were out" dialog shows time, distance, points, events.

## Bring the data back
```
scripts/pull_diag.sh            # -> diag/<timestamp>/  (diag log, journal.db, games, logcat); ANDROID_SERIAL=<phone> if several devices
python3 scripts/diag_report.py diag/<timestamp>    # quick summary; or just hand me the folder
```
`diag/` holds personal location data: keep it out of git (`.gitignore`).

## What is in the data
- `diag/diag-NNNN.jsonl` (JSON lines, rotating, 2 MB x 10): app start (device, SDK), lifecycle (foreground/background, `away_ms`), permissions, sensors
  (rate, providers), service start/stop, game opened, every quest event, quest progress per 10%, Archipelago status changes, errors and crashes with stack
  traces, core (Rust) messages such as journal write failures, and a **heartbeat every 60 s** while a game is open: fixes and rejected fixes that minute,
  last fix age/accuracy/provider, steps, battery %, screen on, Doze, power save, battery-unrestricted.
- `files/journal.db`: every accepted GPS point (real/simulated) and the audit events (SQLite; tables `points`, `events`).
- `files/games/*.json`: full game state. `logcat.txt`: system log (may be short).
- Reading it: a missing heartbeat = process frozen or killed; a heartbeat with `fixes=0` = the phone delivered no usable location (check `screen_on`, `doze`,
  `last_acc_m`); gaps in the journal track with heartbeats present = location provider problem.

## Known limits
No mode proof (Activity Recognition) yet; no export UI (adb pull only); trace reloads every 10 s; events only logged while a game is open; real GPS never tested outdoors with this build.
