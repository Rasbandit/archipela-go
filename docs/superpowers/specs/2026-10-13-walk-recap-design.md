# Walk recap ("Welcome home") design

## Goal

When the player gets home after a walk, show what the walk was like: a short stats header, the game results and a small route
map. A mix of a reward moment, a game debrief and a fitness log.

## Decisions (owner, 2026-10-13)

- Content: stats + game results + route map, all in the first version.
- Delivery: on arriving home, a notification "Welcome home: see your walk". Tapping it opens the recap screen. If the app is already
  on screen, the recap opens directly.
- Only when something happened: away at least 5 minutes **and** at least one of: a check sent, an item received, a quest done, a
  zone unlocked, a collectible banked, or at least 500 steps. Otherwise no recap and no notification.
- Route: the journal's GPS trace for the walk, thinned, drawn on a small map.

## What already exists

- Arriving home: `PresenceController` calls `Engine::bank_at_home` when presence turns `AtHome` (`PresencePolicy.arrivedHome`).
- Leaving home: `Game::set_counting(true, t)` when presence starts counting (also after a car ride ends).
- The journal (`core/src/journal.rs`, SQLite, per game) keeps every accepted fix (`track`, `segments`) and an audit log of game
  events with kinds such as `quest_done`, `check_sent`, `reward`, `item_received`, `zone_unlocked`.
- `Fog::cells` holds the map squares seen in the whole game; `credit_steps` sees the raw step gain.
- `geo` 0.33 is already a core dependency (length and `Simplify`).

## What is missing

The game does not remember when the current walk started, so it cannot tell which events belong to it.

## Design

### 1. Outing bounds (core, `game.rs`)

New game-file field `outing: Option<Outing>` (`#[serde(default)]`, so old saves load with `None`):

```rust
pub struct Outing {
    pub start_ms: i64,      // when counting first went on away from home
    pub steps: i64,         // raw steps gained during the walk (not slowed by the backpack factor)
    pub cells_at_start: u64 // Fog::cells.len() when the walk started
}
```

- **Start:** `set_counting(true, t)` opens an outing when none is open and the player is not at home (same rule as `away_mark`).
- **Car rides do not end it:** `set_counting(false, ..)` leaves the outing open. Only arriving home ends it.
- **Steps:** `credit_steps` adds the raw gain to `outing.steps` while an outing is open.
- **End:** `Game::end_outing(t_ms) -> Option<OutingEnd>` takes the outing (sets it to `None`) and returns
  `{ start_ms, end_ms, steps, new_cells, banked }`, where `banked` is the forager items carried just before `bank_at_home` banks
  them (there is no bank event to read back). Called from arriving home, before banking. Calling it again returns `None` (bank_at_home is "safe to call
  again").

Events, not polling: the outing is two marks plus a counter that the step events already update.

### 2. Building the recap (core, new `recap.rs`)

A pure function, so it is easy to test with fixtures:

```rust
pub fn build(end: &OutingEnd, events: &[JournalEvent], track: &[Vec<TrackPoint>]) -> Option<Recap>
```

`Recap` (serde, stored in the game file):

| Field | Source |
| ----- | ------ |
| `start_ms`, `end_ms` | outing bounds; time away = end - start |
| `steps` | `outing.steps` |
| `distance_m` | sum of the haversine lengths of the track segments (gaps are not bridged) |
| `new_cells` | `Fog::cells.len()` now minus `cells_at_start` |
| `quests_done: Vec<String>` | `quest_done` events (names) |
| `checks_sent: u32` | `check_sent` events |
| `items: Vec<String>` | `reward` / `item_received` events (item names) |
| `zones_unlocked: Vec<u32>` | `zone_unlocked` events |
| `banked: u32` | `OutingEnd::banked` |
| `route: Vec<Vec<(f64, f64)>>` | track segments, each thinned with `geo::Simplify` (about 5 m tolerance) |

`build` returns `None` when the walk is not worth a recap (the rule under Decisions). Thresholds are named constants
(`MIN_AWAY_MS = 5 min`, `MIN_STEPS = 500`).

New journal query `Journal::events(game, from_ms, to_ms)` returns the audit-log rows in the window.

### 3. Engine / FFI (`core/ffi/src/engine.rs`)

- `Engine::bank_at_home` also ends the outing, reads the journal window, calls `recap::build` and stores the result as
  `game.recap: Option<Recap>` (`#[serde(default)]`; a newer recap replaces an unseen older one). The save happens at once, like the
  other lasting events.
- It returns a new `EventOut::RecapReady` when a recap was stored, so the app knows to notify.
- `Engine::pending_recap() -> Option<RecapOut>` and `Engine::dismiss_recap()`.
- No journal (it failed to open): the recap is still built, with no route and no events from the log; steps, time and squares
  still work. Journal read failures are reported with `note` and never block arriving home.

### 4. Notification (Android)

- On `RecapReady`: if the app is on screen, open the recap screen; otherwise post a notification on a new "Walk recap" channel
  (default importance, no buzz). Text comes from the one help-text file.
- Tapping the notification opens `MainActivity` with an extra that routes to the recap screen.
- Android only draws and notifies; what goes in the recap and whether to show it is decided in the core.

### 5. Recap screen (Android, Compose)

- Header: "Welcome home", time away, distance, steps, new squares (design-system stat tiles, palette tokens only).
- Game results: quests done, checks sent, items received, zones unlocked, banked collectibles; empty sections are hidden.
- Route map: a small non-interactive MapLibre view fit to the route bounds, route line drawn with renderer styling.
- "Done" calls `dismiss_recap()`. Back also dismisses. The screen reads `pending_recap()`, so it survives a process restart.

## Out of scope

- A history of past walks (one pending recap only).
- Sharing the recap, images or export.
- iOS screen (the core part carries over unchanged).
- Moving presence decisions into the core (#132).

## Testing

- Core (TDD, failing tests first):
  - outing opens on counting on away from home, not at home; stays open through a car pause; `end_outing` twice returns `None`
    the second time; old saves load with no outing and no recap.
  - steps add up in the outing at the raw rate (also with an overloaded backpack).
  - `recap::build`: below 5 min = `None`; 5 min with nothing happened and under 500 steps = `None`; each "something happened"
    trigger alone gives a recap; events outside the window are left out; distance over two segments does not bridge the gap;
    an empty track gives an empty route; simplified route keeps the first and last points.
  - `Journal::events` window bounds (inclusive start, inclusive end) and per-game filtering.
  - engine: `bank_at_home` stores a recap and returns `RecapReady`; a second call does not; `dismiss_recap` clears it.
- Android: emulator check with the dev simulator: walk out, do a quest, come home, see the notification and the screen.

## Build order (one task each, own commit, check in between)

1. Outing bounds in `Game` (section 1).
2. `Journal::events` + `recap::build` + the "worth it" rule (section 2).
3. Engine/FFI wiring + `RecapReady` + notification (sections 3, 4).
4. Recap screen: stats and game results (section 5, no map).
5. Recap screen: route map.
