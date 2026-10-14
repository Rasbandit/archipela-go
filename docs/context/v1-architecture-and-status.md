# Context Doc: architecture and status

_Last verified: 2026-10-07 (end of the big UI session). "Verified" = run; "Not done" = not run. Read `docs/context/working-in-this-repo.md` next._

## What exists

A real-world quest game. The player saves **realms** (places, a circle or polygon), the app **scans** each realm's map data into **finds**, and a **game**
builds **zones** (a realm played walking or running) filled with quests from a 74-kind catalog. It plays **solo** or as an **Archipelago** client.
Win conditions: Treasure Hunt and Completionist (Beta 1, 2026-10-13, cut the other ten; see
`docs/superpowers/specs/2026-10-13-progression-beta-design.md`), one or both, combined any / all / at least N.

## Layout

| Path | What |
| -- | -- |
| `apworld/` | Python apworld **0.5.0**, slot_data **schema 5** (`goals`, `goal_requirement`, `goal_need`, per-quest `needs`). 210 tests. Contract: `apworld/docs/contract.md` |
| `core/` (`apgo-core`) | Rust engine: catalog, realms (+marks, +icon), scan + **tile grid** (`tilegrid.rs`), assign, verify, goal, fog, traps, solo, game, yaml. 132 tests + 1 regression |
| `core/ffi/` | UniFFI `Engine` (realms, finds, marks, stats, scan plan/progress, games, play) and `ApSession` (Archipelago via a patched `archipelago_rs`, see `core/vendor/PATCHES.md`) |
| `android/` | Kotlin/Compose. `AppModel`, `Screens.kt` (Realms list/editor/home picker, Play), `NewGame.kt`, `QuestMap.kt` (MapLibre), `ui/` design system |
| `scripts/` | `android_core.sh`, `emu.sh`, `android_ui.py` (adb UI driver), `ap_host.sh` (dev Archipelago server, env `APGO_GOALS`, `APGO_REQ`, `APGO_ZONES` (a count), `APGO_RUN_SHARE`), `e2e_*.sh` (STALE, see below) |
| `docs/context/` | Everything below; index in `CLAUDE.md` |

## Screens (all verified on the emulator; phone = Pixel 8 Pro over wireless adb)

- **Realms**: Home Base tile (green outline, house, map preview; tapping it opens the setup flow) above a Realms list. Realm cards: icon, name, finds, quest types, map-snapshot preview. Swipe to delete (confirm + Undo bar).
- **Realm editor** (full-screen map, autosave, Undo/Redo, no Save/Cancel, Done button): left toolbar = Circle|Polygon pill + Details; Area shows a stats box
  (area, farthest from home, walkable, streets, trails, finds, parks, unpaved); Details shows name, icon, search, finds list with favorite/ban, callout bubbles.
- **Home picker**: full-screen map, draggable house pin, tap to place, "Use my location".
- **New Game** (no tab: opened from the "New game" button on the empty Play screen, over Play; any tab or Back closes it; `AppNav`): zones added once (card = quest types with live counts), several goals + rule, a Run quests switch and share slider, tooltips everywhere (`ui/Help*.kt`), Archipelago join.
- **Play**: map with kind icons, per-goal progress, quest list, dev simulator buttons.
  Forager quests use live items and the shared backpack (`core/src/forage.rs`, spec
  `docs/superpowers/specs/2026-10-12-forager-backpack-design.md`); `Engine::bag` feeds the card's backpack row and the panel's
  "Backpack: Heavy" line.
  Tapping a quest opens the quest card (`QuestCard.kt`, spec `docs/superpowers/specs/2026-10-11-quest-card-design.md`). All
  its text comes from the core (`core/src/card.rs`): `headline`/`card_detail`, `place_row`, `progress_block`. Quests keep the
  place they were made from (`feature_id`, `place_type`, `has_about`); `Engine::about` reads the about sheet from the realm
  atlas off the UI thread (`core/src/poi_details.rs`; stitched trails carry their ways' merged tags). Old games fill these in
  by relink after the streets build (`Game::relink_places`), so no new game is needed. Verified on the Pixel 8 Pro (dark).

## Track and audit journal (new, branch `feat/adaptive-gps-interval`)

- `core/src/journal.rs`: one SQLite file `journal.db` (WAL, `synchronous=NORMAL`) in the app files dir; a fix's point and its events are one transaction
  (`Journal::batch`). `points` = every accepted GPS fix (no spatial index: the old `points_rt` R*Tree is dropped on open), flagged
  simulated or real; `events` = audit log (quests, checks, rewards, traps, rejected fixes throttled to 1/min, app foreground/background).
- Trace = `Journal::segments`, split where two points are >2 min apart (phone off). Play map draws it in two sources: `trace-old` (before this session,
  set only when the journal reloads) and `trace` (this session, refreshed every 10 s).
- GPS rate: `GpsPolicy.kt`/`PresencePolicy.kt`, in a zone = every 1 s with the screen on and the app in front, else 5 s batched up to 20 s (flushed when
  the screen comes back), high
  accuracy, NO distance filter (the filter needs a steady stream: stationary hold, gap and reset detection); far = 90 s once moved 50 m.
  Dwell and time away still finish on one scheduled wake-up (`DueTimer`, core `next_due_ms`/`tick`). Idle = 15 s / 20 m. The journal
  stores accepted estimates (5 s or 5 m throttle), not raw fixes.
- Gaps: the simulator advances a virtual clock 10 min per jump, so sim points never form a line. Real GPS untested outdoors. The trace is reloaded in full on every
  fix (fine for a few thousand points; page or simplify later). Events are only logged while a game is open. No export/clear UI yet.

- Fix quality: quests, fog, chains, the odometer and the journal use estimates from the IMM filter (`loc::Locator`), never raw fixes;
  the map pin is matched to streets and bridged through GPS gaps. See `location-estimation.md`. `Game::explain_near` says why a quest
  within 100 m does or does not count (from the estimate's verdict); logged as `near_miss` events.
- Activity tab + `Engine.activity`: journal entries with attribution (`Game::journal_events`, `items::blurb`). Pause tracking = `AppModel.pause()` (closes the game, stops the service).
  `delete_game` archives the save to `games-archive/` and keeps journal rows. Street snapping of the pin and trace is done (display only, `location-estimation.md`).

## Progressive chains

Step Up, Wanderlust and Cartographer are one chain each (one bar with milestone marks, "next: ..." text, tap for the milestone list) instead of many separate quests; members are hidden from "Show places on the map". Counters live in `Game.counters` (steps, minutes away, new map squares, `steps_last` baseline); they are in the game file, old saves load with defaults. Steps count only while a game is open (`steps_last` is reset on load, so closed time is never credited). Step Up, Wanderlust and Cartographer count only while their zone is unlocked (a locked zone's bar stays still, unlocking pays nothing out at once, and a zone that locks again when the server's item list shrinks keeps what it had). Cartographer counts squares new to the whole game (`Fog::cells`, anywhere) seen while its zone is unlocked. Saves from before that (`counters.cells_counted` false) start an open zone's Cartographer at the whole game's square count and a locked zone's at 0. Time away (Wanderlust) is event-driven: it runs while presence counts (home Wi-Fi gone, not in the car) from `counters.away_mark`, is credited on events (a fix, counting going off, a scheduled `tick`), and is read live as credited + (now - mark); a fix within 100 m of home ends it (backup without home Wi-Fi). The app schedules one wake-up at `next_due_ms` (`DueTimer`). No away settings. The game file (whole JSON + fsync) is saved at once only for lasting events (`Event::saves_at_once`: quest done, check, reward, unlock, goal, shuffle), otherwise at most every 90 s (`SAVE_INTERVAL_MS`), and when the app leaves the screen (`AppModel.onBackground` calls `Engine.save_game`). Reveals and trap starts wait for the periodic save (a trap ending saves at once): a force-stop loses at most 90 s of counters.

## Walk recap

When time away starts, `Game::open_outing` snapshots the game into an `Outing` (steps, items, zones, banked, plus `progress_at_start` per chain counter), kept across car rides and restarts (it is saved, unlike `away_mark`); `complete` records each finished quest as an `OutingQuest` (id, time, point only; name, place, goal and reward are resolved in `recap_of`) and `credit_steps` the counted steps, both only while the outing is open (all new fields `#[serde(default)]`, old files load). `Game::arrive_home` banks, closes the outing and gates the recap by `Recap::worth_showing` (at least 5 min away and a quest, item, zone, banked collectible, a milestone crossed on a chain or 500 steps; a bar that only grew does not count). `Engine::bank_at_home` reads the journal segments before closing the outing, thins them to about 5 m (`geo::Simplify`) into the recap's route, stores `Game.recap` and saves; arriving home twice keeps the first recap. The recap holds `quests` (non-chain only), `chains` (name, unit, before, after, marks; a chain unlocked mid-walk counts from 0) and `counted_steps` next to `steps`. `pending_recap` / `dismiss_recap` expose it. Android: `RecapController` and `RecapNotice` (quiet `walk_recap_v1` channel, posted only while the app is off screen, refreshed on foreground and after `openGame`) and `RecapScreen`: title and `RecapFormat.subtitle`, a still map (route drawn by the trace layer through `RecapFeatures.route`; numbered quest pins are a GeoJSON circle + symbol layer from `RecapFeatures.questPins`, tapped via `queryRenderedFeatures`, one selected at a time, shared with the quest rows), a progress card, a quests card (rows open to goal, done at, reward) and the report sections. The progress card has one bar per chain (`ui/RecapChainBar`: before fill, this-walk fill, milestone dots) driven by one `Animatable` clock; `RecapTimeline` places the bars one after another (full bar 2600 ms, each 450-2400 ms, 380 ms gaps, soft landing over the last 12 %), a bar crossing a milestone bounces its dot and bursts confetti (once per dot), and the "walked vs counted" steps line fades in at 60 % of its bar. Reduced motion (`ANIMATOR_DURATION_SCALE == 0`) shows the end state with no confetti. Animation timing is presentation only; what the recap contains is decided in the core.

## Key design facts

- Difficulty = active minutes; tier = ceil(minutes / minutes_per_tier); Easy 1-3, Medium 4-7, Hard 8-10. Locations `"{Easy|Medium|Hard} {Walk|Run} Quest #n"`.
- **Travel mode belongs to the zone/game, not the realm.** Any realm can serve any mode. Modes are walk and run only (Bike and Drive were removed in Beta 1).
- A realm stores a circle and a polygon (one active, one `spare`); old files with `mode`/`modes` still load.
- Scan data is cached per **0.02 degree grid tile** (`http-cache/q-<hash>.json`, 30 days) and shared by every realm; moving a realm fetches only new tiles. Details in `scan-and-tile-cache.md`.
- Every read of an atlas is restricted to the realm's current zone (`Atlas::restrict_to`); favorites/bans live in `marks/<realm>.json` per realm.
- Trail finds are consolidated (30 m link, length gates, id-as-name ignored); a trail quest asks for the share of the line that fits the effort.
- Walkable length = sum of unique street segments (sidewalks/crossings excluded), not points.
- **Near-a-path rule (#51)**: every point a player must reach (Reach/Dwell targets, DwellArea marker, Courier A and B, RoundTrip far point, boss, Line start, Freeze thaw and Detour waypoint, found at trigger time in the zones' full street index the engine attaches on open) is within `near_path::NEAR_PATH_M` (30 m) of a scanned street/path (`atlas.streets` + `streets_rough`, as the surface preference allows), measured to the street between samples (`Atlas::street_links`, from the per-way `street_runs`/`rough_runs` a scan records; only straight pieces, 0.95-1.05 x 60 m x stride; an atlas without runs has no links, reports `needs_rescan()`, FFI `realm_needs_rescan`), at least the minimum distance from home and 40 m from other quests. `near_path::PathIndex` (30 m grid buckets of points and segments) does `near_path`/`within`, `nearest`/`nearest_on_path`, `snap_into_area[_where]` (polygon places: a path spot inside, else beside the edge), `start_near_path` (loops rotate, open lines are cut) and `share_near`; a line quest's coverage is capped at its share of samples beside a path and the line is dropped under 25 %. Places that cannot be reached from a path are not offered. A sparse zone uses the few street points it has (no grid fallback any more); when they run out a street quest shares a point, and only an empty pool gives the flagged home fallback. Applies to new games and rerolls; saved games keep their quests. Reuse `PathIndex` for future collectibles (#5).
- **Forager (#5)**: courier-family kind `forager` (walk, run), verify `Collect`. `2 * need` items (need 3/5/7/10 by tier, 10 above
  tier 4) are street-pool points at least 60 m apart and at least `max(min_distance_m, 125 m)` from home, spread out to the effort
  distance (`dist_for(want / 2)`), nearest first (the fog anchor). Pickup 25 m; banking on any accepted fix within `verify::HOME_RADIUS_M`,
  and on joining home Wi-Fi (`PresenceController` calls `Engine::bank_at_home` on the change to `AtHome`).
  Progress is saved in `Game::collected` (trackers are not saved). A Shuffle trap moves only unpicked items (`assign::replace_unpicked`).
  Verified by unit tests; emulator check pending; the device walk is not done yet.
- Zone keys + tools gate zones; every trap has an exit; anti-cheat is light (accuracy 75 m, speed caps).

## Presence (home Wi-Fi, car Bluetooth, zone duty cycle)

`android/.../presence/`: `PresencePolicy.decide(Signals)` is a pure function, first match wins: not playing = Stopped; car Bluetooth = InCar; home Wi-Fi = AtHome (all three:
GPS off, `counting=false`); zone Far = OutsideZones (GPS 90 s / 50 m, counting); otherwise InZone (GPS every 1 s screen on / 5 s off, counting). `PresenceMonitor` gathers the signals
(Wi-Fi SSID, Bluetooth ACL, nearest zone), applies the decision to the location source and to the counting flag (the engine ignores fixes and steps while it is false), shows
the chip on Play and writes a "Presence" activity line and a `presence` diag line on each change; heartbeat (at most once a minute, on fixes) adds `presence`/`counting`;
a pending debounce schedules one re-evaluation for when it settles. Settings (home SSIDs with optional BSSID, car device name+address) live in SharedPreferences `presence` via `PresenceSettings`, not in the core.
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
- Location stack (branch `feat/location-quality`, 2026-10-09): core unit tests and `core/tests/loc_scenarios.rs` (20 seeds per
  scenario, in `check-rust`); Android unit tests for `GpsPolicy`, `MePins`, `StepCalCodec`, compass and battery helpers; host replays
  of the 2026-10-07 and 2026-10-08 journal walks against the legacy rules (false jumps 1 vs 26 on 10-07; standing drift and odometer
  wobble lower on both; no parameter change justified). Nothing of it ran on the emulator or the phone (Tasks 13 and 15 device checks were
  not done).

## Not done / not verified (be honest in summaries)

- **No foreground service or background location**: tracking only works while the app is open and the screen on. Biggest gap before an outdoor test.
- Real outdoor GPS with the new UI is untested (phone testing so far was indoors / install only). Mode proof (Activity Recognition) is not implemented.
- `scripts/e2e_emulator.sh` is STALE (taps old labels such as "Circle around me", waits for "places"); it needs rewriting for the editor/New Game flows.
- Archipelago play was not re-run after the goals rework beyond generation + parsing; the app's AP flow (`ApSession`) was not re-tested end to end.
- `return_home`, `death_link` are ignored client-side; Effort Reduction / Collection items are received but not applied; no chat/hints/release UI.
- Per-zone quest types exist in solo; the apworld has one list, so export sends the union.
- Public Overpass servers are slow (a first scan of a new area can take minutes); a self-hosted Overpass was discussed and deferred.
- No About screen yet. The map shows its OSM/OpenMapTiles credit itself and `THIRD_PARTY_NOTICES.md` carries the MapLibre, map data
  and Lucide notices, but a store release should show those notices in the app. No APK signing; debug APK is ~90 MB.
- The apworld has no tutorial/game-info pages (WebWorld only carries option groups).
- Launcher icon is the default; our own icon is not designed. The Archipelago logo (CC BY-NC) must not be bundled (`ui-design-system.md`).
- Licensing: `android/` is PolyForm Noncommercial 1.0.0, everything else MIT; no contributor agreement yet (#82, see `working-in-this-repo.md`).
- Location stack (#117): no outdoor walk with the new build; map matching is unscored on real walks until a pull includes atlases;
  pin glide, arrow, images, landscape compass, battery step and the Play services path are unchecked on a device; iOS not started.
