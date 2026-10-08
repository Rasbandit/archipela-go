# Progressive quests (chains) and presence

_Status: design approved in chat 2026-10-07; spec awaiting owner review. Next step after approval: implementation plan (`writing-plans`)._

**Part A** (sections 1-7): progressive quest chains: Step Up, Wanderlust, Cartographer. **Part B** (after section 7): presence: home Wi-Fi, car Bluetooth and a zone-based GPS duty cycle (GitHub issue #6). Build order: A first, then B. A takes a single "counts now" input so B plugs in without redesign.

## Goal

Replace "several separate quests that each count from zero" with **one bar per kind**, with a mark for each check it unlocks:

```text
Step Up   take 30,000 steps   (----|------|-----|-------|--------)   next: 5,500 (2,100 to go)
```

Today a game has 5 separate Step Up quests, 3 Wanderlust quests and 4 Cartographer quests in the Progress section, each starting its own count from the moment its
tracker is created. Progress is lost when the app restarts, and the Wanderlust members each use a different distance. The owner wants one bar per kind, steady
progress that survives restarts, and a single distance for Wanderlust.

## Decisions (from the brainstorm)

- **Approach:** derive chains from the quests already assigned. No change to generation, the apworld, `slot_data`, or Archipelago: every milestone is still its own location with its own reward. Chains are client-side grouping.
- **Steps** count only while playing (revised 2026-10-07 after the first review): while a game is open, from the phone's step counter. Steps taken while stopped never count. Progress is saved.
- **Time away** is a setting: count away-from-home time **inside a zone's area**, or **anywhere**. It can only accrue while the app is tracking (it needs GPS).
- **Wanderlust distance** is a setting: **Automatic** (scaled to the realm) or **Custom**.
- **Cartographer (new map squares)** is included (assumption confirmed in design review: same kind of progress-bar quest).
- Other quests (reach, dwell, park, trail, courier, round trip) stay single quests.

## 1. Chains

A **chain** is the set of assigned quests of one _progressive kind_ in one zone. Progressive kinds: `step_up` (target `Steps`), `wanderlust` (`Away`), `cartographer` (`Cells`).

- Chain id: `"{zone}:{kind_id}"`. The Progress section shows one row per chain with at least one unfinished member (finished chains stay visible, completed, below).
- Members are sorted by their own amount ascending (ties by `location_id`). **Milestone `i` is at the running total** of the first `i` members' own amounts:
  `at_i = amount_1 + ... + amount_i`. Each member's own amount is its existing target value (`n` steps, `n` cells, `minutes`), so the existing tier scaling carries over.
  Example: members of 500, 2,500, 5,500, 8,000 and 13,500 steps give marks at 500 / 3,000 / 8,500 / 16,500 / 30,000 and a bar titled "take 30,000 steps".
- The chain's total is its last mark. A milestone is _reached_ when the chain counter is >= its `at`. Reaching it completes that member location through the normal path
  (`Game::complete`), which pays the solo reward or sends the Archipelago check and logs the activity entry.
- Chains are derived from `assignments` whenever needed (`Game::chains()`); only the **counters** are stored.
- A reroll or Shuffle trap never re-places chain members: it skips them and re-places only unfinished non-chain quests, so chains and counters are untouched.
  (Rerolling a chain member is disabled in the UI: the chain is one thing.)
- A re-placed quest is never given a progressive kind (`AssignParams::progressive` is off for a reroll), so a reroll cannot join an existing chain,
  shift its marks or start a new one (a new Cartographer member would pay at once, since its counter is the whole game's visited cells).

## 2. Counters and counting rules

Stored in the game save (new fields, all optional so old saves load):

| Chain kind | Counter | Rule |
| -- | -- | -- |
| Step Up | `steps_acc: i64`, `steps_last: Option<i64>` | On each step-counter reading `r` while a game is open: if `steps_last` is `None`, set it to `r` (counting starts now). If `r >= last`, `acc += r - last`, else the phone rebooted: `acc += r`. Then `last = r`. **`steps_last` is reset to `None` every time the game is opened**, so steps taken while stopped are never credited. The reading arrives from the step sensor listener (kept alive in the Application). |
| Wanderlust | `away_ms: i64` (milliseconds) | On each accepted fix (after the existing accuracy and jump filters), if the previous accepted fix was within 5 minutes and **both** fixes are beyond the chain distance from home, add the interval. If the setting is "inside a zone", also require the fix to be inside the area of one of the game's zone realms. |
| Cartographer | `fog.cells.len()` (already saved) | Cells are 150 m, same as `Fog::CELL_M`. The counter is the number of distinct cells visited this game. |

- **Wanderlust distance:** `Automatic` = 40% of the farthest extent, from home, of the realm of the chain's own zone (circle: distance to the centre plus radius, polygon: the farthest corner from home), clamped to 300 m .. 3000 m. `Custom` = the player's value in metres (100 .. 20000).
  The one distance replaces each member's own `min_distance_m`; members keep only their minutes.
- **Time away does not count across gaps over 5 minutes** (unchanged rule) and not while a blocking trap is active.
- **Initial counters for an existing game:** each counter starts at the largest milestone amount among its already-done members, so nothing already earned is lost and nothing is re-earned. `steps_last` starts `None`.

## 3. Settings

New Game screen, in the zones/goals area, saved with the game (client options, not `slot_data`; they apply to Archipelago games too):

- "Time away counts": `Inside a zone` | `Anywhere` (default `Inside a zone`).
- "Away distance": `Automatic` | `Custom` (a number field in metres; default Automatic).
Existing games load with the defaults.

## 4. Engine and FFI

- `core`: `Chain`, `Milestone` (pure data and rules), `Game::chains()`, `Game::on_steps(total, t_ms)`, chain evaluation inside `Game::on_fix`; per-location `Tracker`s for `Steps`/`Away`/`Cells` targets are no longer used (they stay in `verify.rs` for other callers/tests until removed).
- The engine supplies "this fix is inside a zone area" per fix (it already loads the realm shapes).
- FFI: `Engine.chains() -> Vec<ChainOut>` with `{id, kind_id, name, rule_text, unit, counter, total, marks: Vec<MarkOut{at, location_id, reached, reward}>}`; `Engine.on_steps(total, t_ms)`.
  `QuestOut` for chain members keeps `state` (so map and activity code keep working) but chain members are excluded from the quest list's Progress rows.

## 5. UI

- **Progress section:** one row per chain: icon, name, rule ("Take 30,000 steps", "Spend 3 h away (1.2 km+)", "Visit 100 new squares"), a bar with tick marks at each milestone's proportional position, reached ticks filled with a check, and "next: 5,500 steps (2,100 to go)". Done chains show a check and "all N unlocked".
- **Tick layout** is a pure function (`marks -> fractions`, minimum visual gap) with unit tests.
- **Tap a chain row:** the popup card (same `QuestDetails` style) lists each milestone: amount, state, reward.
- **Activity log:** "Step Up milestone 3 of 5: 8,500 steps" (attribution detail on the quest-done entry).
- Chain members do not appear in "places on the map".

## 6. Testing (TDD)

Rust unit tests: chain building (grouping, ordering, running totals, ties); counters (steps first reading, normal delta, reboot reset, away accumulation, 5-minute gap rule, inside-zone vs anywhere, automatic and custom distance, clamps); milestone completion order and rewards; reroll leaves counters alone; old save loads and gets initial counters from done members; progress survives save/load. Kotlin unit tests: tick layout, row text.
Emulator: drive a game, check bars fill, marks check off, restart the app and confirm progress stays.

## 7. Out of scope for Part A

- Dwell, park, trail, courier and round-trip quests; a chain editor; per-day bars; changing generation to emit chains; any apworld or `slot_data` change.
- The collect-and-bank quest (GitHub issue #5) could reuse chains later.

## Risks and tuning notes

- Running totals make Wanderlust long: members of 30, 45, 60, 75 and 90 minutes give a 5 h bar. The first outdoor tests decide whether to cap or rescale it.
- Step totals above ~30,000 depend on the tier mix of the game; the title always shows the real total.
- "Stop playing" pauses all three chains: nothing counts while no game is open.
- Part B (presence) adds flags that suppress counting. The chain counters, and every other quest check, take a single `counting: bool` state on the game (`Game::set_counting`) so B plugs in without redesign. Until B exists it is always true while a game is open.

---

## Part B: presence (home Wi-Fi, car Bluetooth, zone-based GPS)

## B.1 Goal

Progress should only count when the player is really out playing, and the phone should not burn battery when GPS is pointless:

- **At home** (connected to a home Wi-Fi network): nothing counts and GPS is off.
- **In the car** (a tagged car Bluetooth device is connected): nothing counts and GPS is off, even inside a zone (no pickups while driving).
- **Outside every zone:** GPS runs at a low rate; steps still count (they need no GPS); time-away in "anywhere" mode still accrues.
- **Inside or near a zone:** precise GPS as today.

Decisions (owner, 2026-10-07): the three rules above; steps count outside a zone but not at home; being at home does **not** auto-"Stop playing" (it only suppresses counting and shows a status chip).

## B.2 The state machine (pure, unit-tested, Kotlin: `PresencePolicy`)

Inputs (each `Boolean?` or enum, with "unknown" meaning the signal is unavailable or not permitted):

| Signal | Source |
| -- | -- |
| `playing` | a game is open |
| `homeWifi` | connected Wi-Fi matches a saved home network |
| `carBluetooth` | a tagged device is connected |
| `zone` | `Inside` / `Near` (within 300 m of a zone realm) / `Far` / `Unknown` (no fix yet) |

Decision, first match wins:

| # | Condition | State | GPS | Counts |
| -- | -- | -- | -- | -- |
| 1 | not `playing` | Stopped | existing idle rule (only while the app is on screen) | no |
| 2 | `carBluetooth` | InCar | off | no |
| 3 | `homeWifi` | AtHome | off | no |
| 4 | `zone` is `Inside`, `Near` or `Unknown` | InZone | precise (5 s, no distance filter) | yes |
| 5 | `zone` is `Far` | OutsideZones | coarse (every 90 s, network/fused) | yes |

- **Debounce:** a change into AtHome or out of it, and into or out of InCar, only takes effect after the signal has been stable for 45 s (Wi-Fi reaches past the front door; Bluetooth flaps). Entering InZone from Far is immediate.
- Unknown signals (permission missing, no Bluetooth device tagged, no home network saved) are treated as "not present": the feature degrades to today's behaviour.
- The policy returns a `Decision(state, gpsRate?, counting)`; `Sensors` applies the rate, `AppModel` calls `Game::set_counting` (via FFI) and shows the chip.

## B.3 Signals (Kotlin, `Presence` classes)

- **Home Wi-Fi:** a `ConnectivityManager.NetworkCallback` for the Wi-Fi transport. On Android 12+ it is created with `FLAG_INCLUDE_LOCATION_INFO` so the SSID/BSSID are visible; below that, `WifiManager.connectionInfo`. SSID quoting is stripped; `<unknown ssid>` means "unknown". A saved network matches on the **BSSID or the SSID** (multi-access-point homes, and a changed router, both work). Needs location permission and location on (the app already asks for both); `ACCESS_WIFI_STATE` and `ACCESS_NETWORK_STATE` are normal permissions.
- **Car Bluetooth:** a dynamic `BroadcastReceiver` for `ACTION_ACL_CONNECTED` / `ACTION_ACL_DISCONNECTED` (registered while the service runs) plus an initial read of connected A2DP and headset devices. Android 12+ needs the runtime permission `BLUETOOTH_CONNECT` (below Android 12, the legacy `BLUETOOTH` permission with `maxSdkVersion=30`). Only devices the player **tagged as "my car"** count (earbuds and watches must not suppress progress).
- **Zone proximity:** `Engine.zone_proximity(lat, lon)` in the core: the distance to each of the open game's zone realms' shapes (circle or polygon), `Inside` at 0, `Near` within 300 m, else `Far`. Computed on every fix from the coarse or precise location.

## B.4 Core changes

- `Game::set_counting(bool)`. When it turns `false`, per-session fix state is cleared (`last_fix`, odometer anchor, outlier streak) so the first fix after a suppression is never judged against a stale one (no false jump). Step readings keep updating `steps_last` while suppressed without crediting anything, so no steps are credited for the suppressed period.
- While `counting` is false, `on_fix` and `on_steps` do nothing except keep the step baseline current. Quest checks, distance, chain counters and fog discovery are all skipped.
- Counting is not saved: a game always opens with `counting = true` until the presence layer says otherwise.
- The journal gets presence events (`presence_changed`: "Home Wi-Fi connected, paused", "Car Bluetooth connected, not counting", "Left the zone area: saving battery") for the activity log.

## B.5 Settings and UI

App-level settings (not per game), stored in Kotlin `SharedPreferences` because the identifiers are platform specific: `homeNetworks: [{ssid, bssid?}]`, `carDevices: [{name, address}]`.

- **Home Wi-Fi networks:** in the Home area of the Realms screen (the home settings): a list with a remove button and "Add current network" (reads the connected SSID/BSSID; asks to connect first if none).
- **Car Bluetooth:** a list of paired devices with a tick for "this is my car" (requests `BLUETOOTH_CONNECT` on first use). Explains why in a sentence.
- **Status chip** on Play next to the game name: "Tracking", "At home, paused", "In car, not counting", "Outside zones, saving battery".
- The Activity tab shows the presence events.
- No settings saved means the feature is inert and the chip reads "Tracking".

## B.6 Testing

- Kotlin unit tests: `PresencePolicy` as a table (every row of B.2, priority between car and home, unknown signals), the debouncer, SSID/BSSID matching and quote stripping.
- Rust unit tests: `set_counting(false)` skips checks, distance and chain counters; the first fix and step reading after resuming are not judged against stale state; zone proximity (inside, near, far, circle and polygon).
- Emulator: it reports the Wi-Fi name `AndroidWifi`, so add it as the home network and check the chip and that GPS stops; the car Bluetooth and the outside-zone duty cycle need an outdoor test (log lines for each state change and GPS rate).

## B.7 Out of scope for Part B

Per-game presence settings; learning home automatically; using Play Services geofencing or Activity Recognition (not used by the app today); an "in the car" rule for Drive-mode zones (they are hidden in the UI today); auto "Stop playing"; Wi-Fi networks beyond home (work, cafe).

## B.8 Risks

- Wi-Fi reaches outside the house, so a quest within about 50 m of home cannot be completed from home. Check the "minimum distance from home" default against it.
- At 90 s per fix outside zones, a fast cyclist covers about 400 m between fixes; the 300 m "near" buffer is a starting value to tune in the outdoor test.
- Android may throttle Bluetooth and network callbacks if the foreground service is killed; the heartbeat log records the presence state to make this visible.
