# Context Doc: v1 Architecture and Status (overnight build, 2026-10-08)

_Last verified: 2026-10-08. Everything marked "verified" was run; everything under "Not done / not verified" was not._

## What exists
A real-world quest game. Players save **realms** (geofences tagged walk/run/bike/car), the app **scans** each realm's map data, and a game fills
the realms with quests drawn from a 76-kind catalog. It plays **standalone (solo)** or as an **Archipelago** client. Win conditions: 12 goals.

## Layout
| Path | What |
|--|--|
| `apworld/` | Python apworld v2 (zones, tools, difficulty-named locations, 12 goals, 8 trap types). 179 tests. Contract: `apworld/docs/contract.md`, schema, sample slot_data, example.yaml |
| `core/` (`apgo-core`) | Rust engine: catalog, realms, scan, assign, verify, goal, fog, traps, solo generator, game state, YAML builder. 78 unit tests + 1 regression test |
| `core/ffi/` | UniFFI facade: `Engine` (realms, scan, games, play) and `ApSession` (Archipelago websocket via a patched `archipelago_rs`) |
| `core/data/quest_catalog.json` | The quest kinds (source of truth; `scripts/catalog_doc.py` writes `docs/context/quest-catalog.md`) |
| `android/` | Kotlin/Compose app: `AppModel` (state, GPS, steps, dev simulator, AP sync), `Screens` (Realms / New Game / Play), `QuestMap` (MapLibre) |
| `scripts/` | `android_core.sh` (cargo-ndk + bindings), `emu.sh`, `e2e_emulator.sh`, `e2e_autoplay.sh`, `android_ui.py` (adb UI driver), `ap_host.sh` (dev Archipelago server) |

## Data flow
`draw/circle realm -> Engine.scan_realm (tiled Overpass queries, cached) -> Atlas (places, streets, rough streets, per-kind matches)`
`-> Game Builder: Solo (core/solo.rs generates slot_data + rewards) OR Archipelago (apworld generates; app reads slot_data)`
`-> assign (slot -> kind + real place in the zone's realm by effort tier, mode, surface pref) -> play: GPS/steps -> verify trackers -> quest done`
`-> Solo: local reward table | Archipelago: LocationChecks out, items back (ApSession.poll + syncItems) -> zone keys/tools unlock zones, traps fire`
`-> goal evaluation (core/goal.rs) -> Archipelago: StatusUpdate(Goal)`

## Key design facts
- Difficulty = estimated active minutes (`effort`). Tier = ceil(minutes / minutes_per_tier); Easy 1-3, Medium 4-7, Hard 8-10.
- Locations are named `"{Easy|Medium|Hard} {Walk|Run|Bike|Drive} Quest #n"` so AP's standard `exclude_locations` / `priority_locations` accept groups (`Hard`, `Bike`...).
- A zone's realm mode must equal the zone mode (mode tag decides which kinds can appear). Walk never needs a tool; Run/Bike/Drive zones need Running Shoes / Bike / Car.
- Anti-cheat is light by design: GPS accuracy cap (75 m), mode speed caps (walk 12, run 25, bike 50 km/h), continuous-dwell, corridor coverage for trails.
- Every trap has an exit (thaw point / waypoint / toll distance / timers). Fog reveal radius 150 m + 100 m per Scouting item.
- The scanner uses ~1.5 km tiles, 3 parallel workers, per-query disk cache (30 days), endpoint health ordering. Cold scans of a dense 1.5 km downtown took 2-5 minutes on the public servers.

## Verified (2026-10-08)
- Desktop: 78 core tests; autoplay on **real downtown Portland data** reached the quest-dex goal (`core/examples/play_sim.rs`).
- Emulator (Android 16, x86_64) with the real app: realm scan -> solo game -> autoplay to win (quest-dex; letters with fog + paved-only + avoid stairs).
- Emulator against a local **Archipelago server running the v2 apworld**: connect, slot_data, checks as `Easy Walk Quest #n`, Bike item unlocked zone 2 mid-game, goal reported, server printed "Team #1 has completed all of their games".
- Persistence: games and realms survive app restarts and reinstalls.
- Phone (Pixel 8 Pro): the earlier spike (Rust core, map, geofence, Archipelago checks) worked; the NEW UI was only tested on the emulator because the phone was locked overnight.

## Bugs real data / emulator found and fixed
Scan too slow (one big query) -> tiles + parallel + endpoint health; park center outside its polygon -> `point_inside`; trail roughness lost when stitching ways;
round-trip timer swallowed the far-point fix after a timeout and started at first fix instead of leaving home; goal not reported to the server.

## Not done / not verified (be honest)
- **No foreground service / background location**: the app only tracks while open. This is the biggest gap before real outdoor play.
- Real outdoor GPS play with the new UI is untested. Run `just android-run`, set a realm, tap "Real GPS".
- Step quests need the phone's step counter: wired (ACTIVITY_RECOGNITION + TYPE_STEP_COUNTER) but only the simulator path was exercised.
- Mode proof beyond speed caps (Activity Recognition: walking vs cycling vs vehicle) is not implemented.
- `return_home` and `death_link` options reach the app in slot_data but are not implemented client-side. Effort Reduction and Collection Distance items are received but not applied (Scouting is applied to fog).
- No UI for Archipelago chat, hints, or release/collect. No in-app warning when a realm is too small for a mode's difficulty (assignments then take the biggest thing available, flagged by effort).
- Surface preference is best effort (OSM surface tags are sparse: ~18% of paths). Old atlases need a rescan to get rough-street data.
- Public Overpass is the weak link (timeouts/504s). The prebuilt static atlas plan (`docs/context/poi-atlas-and-server-options.md`) is the real fix.
- `archipelago_rs` is vendored with a patch (Android cache dir); no upstream issue/PR filed (decision: prove first).
- Apworld: no WebWorld/website docs page; game-name spelling/ID collision checks are open items in the apworld spec.
- Debug APK is ~90 MB (two ABIs, symbols); no signing/store work.

## How to run
- Emulator (no phone): `scripts/emu.sh create` once, `just emu-start`, `just emu-run`, `just e2e`. Dev Archipelago server: `APGO_ZONES=walk,bike APGO_GOAL=quest_dex just ap-host 40`; in the emulator app use server `10.0.2.2:38281`, slot `Tester`.
- Phone: wireless debugging (`adb pair` / `adb connect`; reconnect after it drops), `just android-run`. When both phone and emulator are attached, set `ANDROID_SERIAL`.
- Dev simulator buttons in the Play screen: "DEV: do next" (completes the next quest with realistic fixes), per-quest "DEV: complete", "Real GPS".
- Emulator gotchas: only `-gpu swangle_indirect` works (others segfault); host loopback is `10.0.2.2`; `adb emu geo fix <lon> <lat>` sets GPS; the app subscribes to all location providers (emulator only feeds GPS).

## Next steps (suggested)
1. Foreground service + background location + wake handling. 2. Real outdoor test (short realm near home). 3. Prebuilt atlas/CDN to replace live scans.
4. Activity Recognition for mode proof. 5. Client-side `return_home`, DeathLink, chat/hints. 6. Timed/ordered quests from the backlog. 7. iOS (Swift over the same Rust core).
