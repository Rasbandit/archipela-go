# Progressive quests (chains): Step Up, Wanderlust, Cartographer

_Status: design approved in chat 2026-10-07, spec awaiting owner review. Next step after approval: implementation plan (`writing-plans`)._

## Goal
Replace "several separate quests that each count from zero" with **one bar per kind**, with a mark for each check it unlocks:

```
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
A **chain** is the set of assigned quests of one *progressive kind* in one zone. Progressive kinds: `step_up` (target `Steps`), `wanderlust` (`Away`), `cartographer` (`Cells`).

- Chain id: `"{zone}:{kind_id}"`. The Progress section shows one row per chain with at least one unfinished member (finished chains stay visible, completed, below).
- Members are sorted by their own amount ascending (ties by `location_id`). **Milestone `i` is at the running total** of the first `i` members' own amounts:
  `at_i = amount_1 + ... + amount_i`. Each member's own amount is its existing target value (`n` steps, `n` cells, `minutes`), so the existing tier scaling carries over.
  Example: members of 500, 2,500, 5,500, 8,000 and 13,500 steps give marks at 500 / 3,000 / 8,500 / 16,500 / 30,000 and a bar titled "take 30,000 steps".
- The chain's total is its last mark. A milestone is *reached* when the chain counter is >= its `at`. Reaching it completes that member location through the normal path
  (`Game::complete`), which pays the solo reward or sends the Archipelago check and logs the activity entry.
- Chains are derived from `assignments` whenever needed (`Game::chains()`); only the **counters** are stored.
- A reroll or Shuffle trap only re-places unfinished members. Chain members are rebuilt from the new targets; counters are untouched. (Rerolling a chain member is disabled in the UI: the chain is one thing.)

## 2. Counters and counting rules
Stored in the game save (new fields, all optional so old saves load):

| Chain kind | Counter | Rule |
|--|--|--|
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

## 7. Out of scope
- Dwell, park, trail, courier and round-trip quests; a chain editor; per-day bars; changing generation to emit chains; any apworld or `slot_data` change.
- The collect-and-bank quest (GitHub issue #5) could reuse chains later.

## Risks and tuning notes
- Running totals make Wanderlust long: members of 30, 45, 60, 75 and 90 minutes give a 5 h bar. The first outdoor tests decide whether to cap or rescale it.
- Step totals above ~30,000 depend on the tier mix of the game; the title always shows the real total.
- "Stop playing" pauses all three chains: nothing counts while no game is open.
- A later "presence" feature (home Wi-Fi, car Bluetooth, zone-based GPS duty cycle; GitHub issue) will add flags that suppress counting. Chain counters should take a single "counts now" input so it can plug in without a redesign.
