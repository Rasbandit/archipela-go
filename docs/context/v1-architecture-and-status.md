# Context Doc: architecture and status

_Last verified: 2026-10-07 (end of the big UI session). "Verified" = run; "Not done" = not run. Read `docs/context/working-in-this-repo.md` next._

## What exists

A real-world quest game. The player saves **realms** (places, a circle or polygon), the app **scans** each realm's map data into **finds**, and a **game**
builds **zones** (a realm played by walk/run/bike) filled with quests from a 76-kind catalog. It plays **solo** or as an **Archipelago** client.
Win conditions: 12 goals, one or several, combined any / all / at least N.

## Layout

| Path | What |
| -- | -- |
| `apworld/` | Python apworld **0.3.0**, slot_data **schema 3** (`goals`, `goal_requirement`, `goal_need`). 210 tests. Contract: `apworld/docs/contract.md` |
| `core/` (`apgo-core`) | Rust engine: catalog, realms (+marks, +icon), scan + **tile grid** (`tilegrid.rs`), assign, verify, goal, fog, traps, solo, game, yaml. 132 tests + 1 regression |
| `core/ffi/` | UniFFI `Engine` (realms, finds, marks, stats, scan plan/progress, games, play) and `ApSession` (Archipelago via a patched `archipelago_rs`, see `core/vendor/PATCHES.md`) |
| `android/` | Kotlin/Compose. `AppModel`, `Screens.kt` (Realms list/editor/home picker, Play), `NewGame.kt`, `QuestMap.kt` (MapLibre), `ui/` design system |
| `scripts/` | `android_core.sh`, `emu.sh`, `android_ui.py` (adb UI driver), `ap_host.sh` (dev Archipelago server, env `APGO_GOALS`, `APGO_REQ`, `APGO_ZONES`), `e2e_*.sh` (STALE, see below) |
| `docs/context/` | Everything below; index in `CLAUDE.md` |

## Screens (all verified on the emulator; phone = Pixel 8 Pro over wireless adb)

- **Realms**: Home Base tile (green outline, house, map preview; tapping it opens the setup flow) above a Realms list. Realm cards: icon, name, finds, quest types, map-snapshot preview. Swipe to delete (confirm + Undo bar).
- **Realm editor** (full-screen map, autosave, Undo/Redo, no Save/Cancel, Done button): left toolbar = Circle|Polygon pill + Details; Area shows a stats box
  (area, farthest from home, walkable, streets, trails, finds, parks, unpaved); Details shows name, icon, search, finds list with favorite/ban, callout bubbles.
- **Home picker**: full-screen map, draggable house pin, tap to place, "Use my location".
- **New Game**: zones added once (card = travel mode + quest types with live counts), several goals + rule, tooltips everywhere (`ui/Help*.kt`), Archipelago join.
- **Play**: map with kind icons, per-goal progress, quest list, dev simulator buttons.

## Track and audit journal (new, branch `feat/adaptive-gps-interval`)

- `core/src/journal.rs`: one SQLite file `journal.db` (WAL) in the app files dir. `points` (+ `points_rt` R*Tree) = every accepted GPS fix, flagged
  simulated or real; `events` = audit log (quests, checks, rewards, traps, rejected fixes throttled to 1/min, app foreground/background).
- Trace = `Journal::segments`, split where two points are >2 min apart (phone off). Play map draws it (`trace` layer in `QuestMap.kt`).
- "While you were out": on app start, if the last `app_background` is >=60 s old, `AwayDialog` shows time, distance, points and event counts.
- GPS rate: `GpsPolicy.kt`, playing = every 5 s with NO distance filter (a filter starves Dwell/Away while standing), idle = 15 s / 20 m.
- Gaps: the simulator advances a virtual clock 10 min per jump, so sim points never form a line. Real GPS untested outdoors. The trace is reloaded in full on every
  fix (fine for a few thousand points; page or simplify later). Events are only logged while a game is open. No export/clear UI yet.

- Fix quality (core `Game::on_fix`): accuracy limit 35 m, a fix implying >100 km/h (error radii discounted) is dropped (3 in a row are believed), distance counts only after
  movement beyond GPS wobble and never across >5 min gaps. `Game::explain_near` says why a quest within 100 m does or does not count; logged as `near_miss` events.
- Activity tab + `Engine.activity`: journal entries with attribution (`Game::journal_events`, `items::blurb`). Pause tracking = `AppModel.pause()` (closes the game, stops the service).
  `delete_game` archives the save to `games-archive/` and keeps journal rows. Street snapping is NOT done yet (idea: display/trace only, sticky segment, after the retest).

## Progressive chains

Step Up, Wanderlust and Cartographer are one chain each (one bar with milestone marks, "next: ..." text, tap for the milestone list) instead of many separate quests; members are hidden from "Show places on the map". Counters live in `Game.counters` (steps, minutes away, new map squares, `steps_last` baseline); they are in the game file, old saves load with defaults. Steps count only while a game is open (`steps_last` is reset on load, so closed time is never credited). Step Up, Wanderlust and Cartographer count only while their zone is unlocked (a locked zone's bar stays still, unlocking pays nothing out at once, and a zone that locks again when the server's item list shrinks keeps what it had). Cartographer counts squares new to the whole game (`Fog::cells`, anywhere) seen while its zone is unlocked. Saves from before that (`counters.cells_counted` false) start an open zone's Cartographer at the whole game's square count and a locked zone's at 0. Away settings (only count time inside a zone, away distance auto or custom) are chosen in New Game and saved with the game. The game file is saved only when a fix produces events, so counters gained between milestones are lost on force-stop (seen on the emulator; `Engine.save_game` is never called from Kotlin).

## Key design facts

- Difficulty = active minutes; tier = ceil(minutes / minutes_per_tier); Easy 1-3, Medium 4-7, Hard 8-10. Locations `"{Easy|Medium|Hard} {Walk|Run|Bike|Drive} Quest #n"`.
- **Travel mode belongs to the zone/game, not the realm.** Any realm can serve any mode. Car is hidden in the UI (core still has `Drive`).
- A realm stores a circle and a polygon (one active, one `spare`); old files with `mode`/`modes` still load.
- Scan data is cached per **0.02 degree grid tile** (`http-cache/q-<hash>.json`, 30 days) and shared by every realm; moving a realm fetches only new tiles. Details in `scan-and-tile-cache.md`.
- Every read of an atlas is restricted to the realm's current zone (`Atlas::restrict_to`); favorites/bans live in `marks/<realm>.json` per realm.
- Trail finds are consolidated (30 m link, length gates, id-as-name ignored); a trail quest asks for the share of the line that fits the effort.
- Walkable length = sum of unique street segments (sidewalks/crossings excluded), not points.
- **Near-a-path rule (#51)**: every point a player must reach (Reach/Dwell targets, DwellArea marker, Courier A and B, RoundTrip far point, boss, Line start, Freeze thaw point) is within `near_path::NEAR_PATH_M` (30 m) of a scanned street/path (`atlas.streets` + `streets_rough`, as the surface preference allows), measured to the street between samples (`Atlas::street_links`, from the per-way `street_runs`/`rough_runs` a scan records; older atlases link neighbouring points a sample apart), at least the minimum distance from home and 40 m from other quests. `near_path::PathIndex` (30 m grid buckets of points and segments) does `near_path`/`within`, `nearest`/`nearest_on_path`, `snap_into_area[_where]` (polygon places: a path spot inside, else beside the edge), `start_near_path` (loops rotate, open lines are cut) and `share_near`; a line quest's coverage is capped at its share of samples beside a path and the line is dropped under 25 %. Places that cannot be reached from a path are not offered. A sparse zone uses the few street points it has (no grid fallback any more); when they run out a street quest shares a point, and only an empty pool gives the flagged home fallback. Applies to new games and rerolls; saved games keep their quests. Reuse `PathIndex` for future collectibles (#5).
- Zone keys + tools gate zones; every trap has an exit; anti-cheat is light (accuracy 75 m, speed caps).

## Presence (home Wi-Fi, car Bluetooth, zone duty cycle)

`android/.../presence/`: `PresencePolicy.decide(Signals)` is a pure function, first match wins: not playing = Stopped; car Bluetooth = InCar; home Wi-Fi = AtHome (all three:
GPS off, `counting=false`); zone Far = OutsideZones (GPS every 90 s, counting); otherwise InZone (GPS every 5 s, counting). `PresenceMonitor` gathers the signals
(Wi-Fi SSID, Bluetooth ACL, nearest zone), applies the decision to the location source and to the counting flag (the engine ignores fixes and steps while it is false), shows
the chip on Play and writes a "Presence" activity line and a `presence` diag line on each change; heartbeat adds `presence`/`counting`, with a 60 s heartbeat and a 5 s
re-evaluation loop. Settings (home SSIDs with optional BSSID, car device name+address) live in SharedPreferences `presence` via `PresenceSettings`, not in the core.
Setup lives in `SetupFlow` (steps in `SetupSteps.kt`; pure helpers: `presence/SetupProgress.kt` (`SetupProgress`) and `presence/Choices.kt` (`WifiChoices`/`CarChoices`)).
`PresenceSettings.setupDone` gates the first-run wizard; the Home Base tile has no button: tapping it opens the wizard (at the first missing step when it was never finished or home Wi-Fi is missing; a missing car never triggers that), and it shows a warning while home Wi-Fi is missing. Step 2 keeps search/Rescan at the top, the list scrolling in between, and "Add a network by name" pinned above the buttons. The Play chip reads
"Protection off" when nothing is configured. Wi-Fi choices come from nearby scan results because Android exposes no saved-network list.
**Home Wi-Fi offer (#11):** a player who skipped home Wi-Fi gets "You're home: add this Wi-Fi?" (`HomeWifiDialog`, text in `HomeOfferText`). The pure rule
`presence/HomeWifiOffer.decide(OfferSignals)` offers when no home network is saved, a game is open, the last real fix is within 75 m of `realmOps.homePoint()` with
accuracy at most 50 m, the Wi-Fi has a usable SSID that is not muted, no offer is showing and "Later" was not pressed in the last 10 minutes. `PresenceController`
checks it in every `evaluate()` (after each fix, each Wi-Fi change and when the wizard closes) through `HomeWifiOffer.next`, which re-decides as if nothing were
showing and keeps the offer only while the same SSID still qualifies (otherwise it is withdrawn, no cooldown). Add re-checks first, then saves (`addHome`) and
re-evaluates; "Not this one" adds the already-cleaned SSID as is to `PresenceSettings.mutedHomeOffers` (a preferences string set, no settings UI); "Later" is an
in-memory cooldown on the wall clock. The fix age uses the monotonic clock (`elapsedRealtimeNanos`, 0..120 s), so the cached last-known location loaded when GPS
starts and future-stamped fixes cannot trigger it. `AppRoot` holds the dialog back while the away, scan, YAML or background-location dialog is up (`showHomeOffer`).
Seeding at monitor start reads the signals for up to 3 s and trusts them at once (so a game opened at home shows "At home, paused" immediately); afterwards the
**arrival** into AtHome/InCar is debounced 45 s (`Debouncer`) and leaving is immediate. A missing signal (no permission, Wi-Fi off) counts as "not present".
Known limits: Bluetooth and the outside-zone duty cycle have no outdoor run yet; the SSID needs location permission; matching is by name (BSSID optional); the `gps` field in the
first diag line prints `GpsMode$Off@hash` (cosmetic, no toString).

## Verified

- Core 132 + apworld 210 tests; ruff, pyright, clippy (`-D warnings`), rustfmt clean.
- Real Archipelago `Generate.py` + `MultiServer` with 3 goals / "at least 2": the app's own reader (`cargo run --example parse_slot`) accepts the slot_data.
- Emulator: realm create/edit/undo/redo/autosave, scan with progress and cooldown, cache hit (11 of 12 requests from cache after a nudge), stats, home picker,
  New Game with zones + two goals starting a game, per-goal progress in Play, swipe delete + Undo, Back/Done/tab navigation.
- Earlier (before the UI rework): solo autoplay to a win; full Archipelago session against a local server incl. a Bike item unlocking zone 2 and the goal being reported.

## Not done / not verified (be honest in summaries)

- **No foreground service or background location**: tracking only works while the app is open and the screen on. Biggest gap before an outdoor test.
- Real outdoor GPS with the new UI is untested (phone testing so far was indoors / install only). Mode proof (Activity Recognition) is not implemented.
- `scripts/e2e_emulator.sh` is STALE (taps old labels such as "Circle around me", waits for "places"); it needs rewriting for the editor/New Game flows.
- Archipelago play was not re-run after the goals rework beyond generation + parsing; the app's AP flow (`ApSession`) was not re-tested end to end.
- `return_home`, `death_link` are ignored client-side; Effort Reduction / Collection items are received but not applied; no chat/hints/release UI.
- Per-zone quest types exist in solo; the apworld has one list, so export sends the union.
- Public Overpass servers are slow (a first scan of a new area can take minutes); a self-hosted Overpass was discussed and deferred.
- No attribution/About screen yet (OSM, OpenFreeMap, Lucide ISC are required for a store release). No APK signing; debug APK is ~90 MB.
- The apworld has no tutorial/game-info pages (WebWorld only carries option groups).
- Launcher icon is the default; our own icon is not designed. The Archipelago logo (CC BY-NC) must not be bundled (`ui-design-system.md`).
- Licensing: repo is MIT today. The owner wants to monetize the app: change the app's license before making the repo public (see `working-in-this-repo.md`).
