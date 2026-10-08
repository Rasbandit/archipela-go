# Context Doc: outdoor test plan and diagnostics

_Written 2026-10-07. First outdoor test = SOLO on the device. Archipelago second (its end-to-end flow was not re-tested after the goals rework)._

## Changes after the first outing (2026-10-07)

First outing findings: location came from three providers at once (network fixes up to 100 m off made the marker jump streets) and fog/shuffle **trap rewards** hit the
first quests. Now: one provider (fused), fixes over 35 m or implying a >100 km/h jump are dropped, distance ignores GPS wobble, **Pause tracking** button on Play
(tracking stops; the saved games list now lives on the empty Play screen), an **Activity** tab explains every quest (how it was done), reward (which quest, what it does,
honest about items not applied yet) and, with "Show GPS and app notes", why a nearby quest did not count. Deleting a game keeps its journal and archives its save.
**Switch Traps off in New Game for test games.**

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

## Presence checklist (home Wi-Fi, car, zones)

Before leaving: Realms > tap the **Home Base** tile. At home, in step 2 tick the network tagged "Connected now" (needs location permission to read the Wi-Fi name). For the car, pair it in
the phone's Bluetooth settings first, then tick it in step 3 (needs the Nearby devices permission). Open the game and read the chip next to the game name:

- **Tracking**: inside a zone, GPS every 5 s, progress counts.
- **At home, paused**: on a home network; GPS is off and nothing counts.
- **In car, not counting**: a tagged car device is connected; GPS is off and nothing counts.
- **Outside zones, saving battery**: far from every zone; GPS every 90 s, progress still counts.
- **Not playing**: no game open or tracking paused.
Verified on the emulator with the old Presence screen (home Wi-Fi only): add/remove network, At home (GPS unregistered, mock fixes ignored), back to Tracking after removing the network. The new setup flow (tap the Home Base tile, steps 1-3) ran on the emulator and on the Pixel 8 Pro: step 2 listed the connected and nearby networks with only location permission (no NEARBY_WIFI_DEVICES needed). Pausing at home with it is still to be checked outdoors.
**Untested until an outdoor run: car Bluetooth (no paired device on the emulator) and the outside-zone duty cycle with real movement.** Check: leave the home Wi-Fi
and walk off (leaving is immediate; arriving back needs 45 s of Wi-Fi), start/stop the car connection, walk out of a zone and back in.
Where to look afterwards: Activity tab, kind **Presence** ("Home Wi-Fi connected: paused", "Tracking", "Outside every zone: saving battery"); in the diag, lines with
`"tag":"presence"` (`msg` = state, `counting`, `gps` mode) and the extra heartbeat fields `presence` and `counting` (a heartbeat with `fixes=0` is expected while AtHome/InCar).

## Bring the data back

```bash
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
