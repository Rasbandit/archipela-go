# Walk Recap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the player gets home after a walk, show a "Welcome home" recap: stats, game results and a route map.

**Architecture:** The core opens an `Outing` (start time + snapshots of the game state) when the player leaves home and closes
it when they arrive. On arrival, `Game::arrive_home` turns the snapshot diff into a `Recap` (or `None` when nothing worth showing
happened). The engine adds the route from the journal and stores the recap in the game file. Android reads `pending_recap()`,
posts a notification when off screen, and draws the recap screen.

**Tech Stack:** Rust (`apgo-core`, `apgo-ffi`, UniFFI, `geo` 0.33, serde), Kotlin/Compose, MapLibre.

**Spec:** `docs/superpowers/specs/2026-10-13-walk-recap-design.md`

## Change from the spec (read first)

The spec said to read quests, checks and items back from the journal's audit log. Its `detail` texts are human prose
(`"Hydrate! (reward for ...): ..."`), so parsing them would be fragile. Instead the outing keeps **snapshots** (done set, item
count, unlocked zones, square count) and the recap is their diff. That works without the journal and needs no new query.
The journal is used only for the route and distance. There is no `RecapReady` event: the app asks `pending_recap()` right after
`bank_at_home`. Task 1 updates the spec to match.

## Global Constraints

- Hard rule: what goes in the recap and whether to show it is decided in the Rust core; Kotlin only draws and notifies.
- Hard rule: events over polling. No timers; the outing is opened and closed by presence events and updated by step events.
- Show a recap only when away at least 5 min (`MIN_AWAY_MS = 300_000`) **and** one of: a quest done, an item received, a zone
  unlocked, a collectible banked, or at least 500 steps (`MIN_STEPS = 500`).
- New game-file fields use `#[serde(default)]`; old saves load with no outing and no recap.
- Car rides do not end an outing; only arriving home (`bank_at_home`) does.
- Steps in the recap are raw (not slowed by the backpack factor).
- Route: journal segments split at `DEFAULT_MAX_GAP_MS`, each thinned to about 5 m; gaps are not bridged in the distance.
- One pending recap; a newer one replaces it.
- Every colour is a named palette token (`ui/Palette.kt`); player-facing text lives in `ui/HelpText.kt`.
- Conventional commits, subject under 50 chars, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- `just check-rust` must pass (fmt, clippy `-D warnings`, rustdoc `-D warnings`); coverage floors only go up.

## Review Focus

1. App killed mid-walk and reopened away from home: the outing must survive the restart (it is saved, unlike `away_mark`).
2. Arriving home twice (Wi-Fi flicker, `bank_at_home` is "safe to call again"): exactly one recap, and the second arrival must
   not replace it with `None` or an empty one.
3. A trap blocks banking on arrival: the outing still closes and the recap still shows (with `banked = 0`).
4. The server item list shrinks (`sync_items` replaces `items`): `items_at_start` past the end must give no items, not panic.
5. No journal (failed to open) or an empty trace: the recap still shows with distance 0 and an empty route.

Each line has a test in the task that owns it (1: Task 2, 2: Tasks 4 and 5, 3: Task 4, 4: Task 4, 5: Task 5).

## File Structure

| File | Responsibility |
| ---- | -------------- |
| `core/src/recap.rs` (new) | `Recap` value, the "worth showing" rule, route thinning and distance |
| `core/src/game.rs` | `Outing` snapshot, opening it, counting steps, `arrive_home` |
| `core/src/lib.rs` | `pub mod recap;` |
| `core/ffi/src/engine.rs` | `bank_at_home` stores the recap with its route; `pending_recap`, `dismiss_recap`, `RecapOut` |
| `android/.../RecapNotice.kt` (new) | The "Welcome home" notification |
| `android/.../RecapScreen.kt` (new) | The recap screen (stats, results, route map) |
| `android/.../RecapFormat.kt` (new) | Plain-text pieces of the recap (pure, unit-tested) |
| `android/.../AppModel.kt`, `PresenceController.kt`, `AppRoot.kt` | Hold the recap, read it on arrival and on foreground, show it |
| `android/.../ui/HelpText.kt` | Recap texts |

---

### Task 1: `Recap` value and route helpers (core, pure)

**Files:**

- Create: `core/src/recap.rs`
- Modify: `core/src/lib.rs` (add `pub mod recap;` in alphabetical order, after `pub mod realm;`)
- Modify: `docs/superpowers/specs/2026-10-13-walk-recap-design.md` (record the change above)

**Interfaces:**

- Produces:
  - `pub struct Recap { start_ms: i64, end_ms: i64, steps: i64, distance_m: f64, new_cells: u32, quests_done: Vec<String>,
    checks_sent: u32, items: Vec<String>, zones_unlocked: Vec<u32>, banked: u32, route: Vec<Vec<Point>> }`
    (all `pub`, `Debug, Clone, PartialEq, Serialize, Deserialize, Default`)
  - `pub const MIN_AWAY_MS: i64 = 300_000;` `pub const MIN_STEPS: i64 = 500;`
  - `impl Recap { pub fn worth_showing(&self) -> bool; pub fn set_route(&mut self, segments: &[Vec<Point>]) }`

- [ ] **Step 1: Write the failing tests** at the bottom of the new `core/src/recap.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn walk(minutes: i64) -> Recap {
        Recap { start_ms: 0, end_ms: minutes * 60_000, ..Recap::default() }
    }

    #[test]
    fn a_short_or_empty_outing_is_not_worth_showing() {
        assert!(!walk(4).worth_showing(), "under 5 min, even with steps");
        assert!(!Recap { steps: 5000, ..walk(4) }.worth_showing());
        assert!(!walk(30).worth_showing(), "long but nothing happened");
        assert!(!Recap { steps: MIN_STEPS - 1, ..walk(30) }.worth_showing());
    }

    #[test]
    fn any_one_thing_happening_makes_it_worth_showing() {
        assert!(Recap { steps: MIN_STEPS, ..walk(5) }.worth_showing());
        assert!(Recap { quests_done: vec!["q".into()], ..walk(5) }.worth_showing());
        assert!(Recap { items: vec!["i".into()], ..walk(5) }.worth_showing());
        assert!(Recap { zones_unlocked: vec![2], ..walk(5) }.worth_showing());
        assert!(Recap { banked: 1, ..walk(5) }.worth_showing());
    }

    #[test]
    fn distance_adds_up_inside_segments_and_never_bridges_a_gap() {
        let a = Point::new(40.0, -111.0);
        let line = |from: Point, n: i32| (0..n).map(|i| destination(from, 90.0, f64::from(i) * 10.0)).collect::<Vec<_>>();
        let far = destination(a, 0.0, 5000.0);
        let mut r = walk(10);
        r.set_route(&[line(a, 11), line(far, 6)]); // 100 m, then 50 m, 5 km apart
        assert!((r.distance_m - 150.0).abs() < 1.0, "{} m", r.distance_m);
        assert_eq!(r.route.len(), 2);
    }

    #[test]
    fn a_straight_line_is_thinned_to_its_ends_and_a_turn_is_kept() {
        let a = Point::new(40.0, -111.0);
        let straight: Vec<Point> = (0..50).map(|i| destination(a, 90.0, f64::from(i) * 5.0)).collect();
        let corner = destination(a, 90.0, 200.0);
        let mut turn = straight[..41].to_vec();
        turn.extend((1..20).map(|i| destination(corner, 0.0, f64::from(i) * 5.0)));
        let mut r = walk(10);
        r.set_route(&[straight.clone(), turn.clone()]);
        assert_eq!(r.route[0], vec![straight[0], *straight.last().unwrap()]);
        assert_eq!(r.route[1].len(), 3, "start, corner, end");
        assert_eq!((r.route[1][0], *r.route[1].last().unwrap()), (turn[0], *turn.last().unwrap()));
    }

    #[test]
    fn no_trace_gives_no_route_and_no_distance() {
        let mut r = walk(10);
        r.set_route(&[]);
        assert_eq!((r.distance_m, r.route.len()), (0.0, 0));
        r.set_route(&[vec![Point::new(40.0, -111.0)]]);
        assert_eq!(r.distance_m, 0.0, "a lone point has no length");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core recap::`
Expected: compile errors (`Recap` not defined).

- [ ] **Step 3: Implement** (top of `core/src/recap.rs`):

```rust
//! The walk recap ("Welcome home"): what one outing away from home came to, and whether it is worth showing.
//! Built by `Game::arrive_home` from the outing's snapshots; the engine adds the route from the journal.

use serde::{Deserialize, Serialize};

use crate::geo::{polyline_len_m, Point};

/// An outing shorter than this is never shown (a trip to the mailbox, Wi-Fi flicker at the door).
pub const MIN_AWAY_MS: i64 = 5 * 60_000;
/// Steps that make an outing worth showing on their own.
pub const MIN_STEPS: i64 = 500;
/// Route thinning tolerance in degrees of latitude (about 5 m).
const THIN_DEG: f64 = 5.0 / 111_320.0;

/// What one outing came to.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Recap {
    /// When the player left home, Unix ms.
    pub start_ms: i64,
    /// When they arrived home, Unix ms.
    pub end_ms: i64,
    /// Raw steps walked (not slowed by the backpack).
    pub steps: i64,
    /// Length of the recorded trace, gaps not bridged.
    pub distance_m: f64,
    /// Map squares seen for the first time.
    pub new_cells: u32,
    /// Names of the quests completed.
    pub quests_done: Vec<String>,
    /// Checks sent to the Archipelago server.
    pub checks_sent: u32,
    /// Items received.
    pub items: Vec<String>,
    /// Zones unlocked.
    pub zones_unlocked: Vec<u32>,
    /// Forager items brought home.
    pub banked: u32,
    /// The trace, one thinned line per recorded stretch.
    pub route: Vec<Vec<Point>>,
}

impl Recap {
    /// Long enough, and something happened.
    #[must_use]
    pub fn worth_showing(&self) -> bool {
        let happened = !self.quests_done.is_empty() || !self.items.is_empty() || !self.zones_unlocked.is_empty() || self.banked > 0 || self.steps >= MIN_STEPS;
        self.end_ms - self.start_ms >= MIN_AWAY_MS && happened
    }

    /// Set the route and distance from the recorded trace (one line per stretch; the journal splits at gaps).
    pub fn set_route(&mut self, segments: &[Vec<Point>]) {
        self.distance_m = segments.iter().map(|s| polyline_len_m(s)).sum();
        self.route = segments.iter().filter(|s| !s.is_empty()).map(|s| thin(s)).collect();
    }
}

/// Douglas-Peucker via `geo`, keeping both ends.
fn thin(line: &[Point]) -> Vec<Point> {
    use ::geo::{Coord, LineString, Simplify};
    let ls: LineString<f64> = line.iter().map(|p| Coord { x: p.lon, y: p.lat }).collect();
    ls.simplify(THIN_DEG).coords().map(|c| Point::new(c.y, c.x)).collect()
}
```

Note: if `geo` 0.33's `simplify` takes `&T`, write `ls.simplify(&THIN_DEG)`; the compiler says which. A one-point
`LineString` simplifies to itself.

- [ ] **Step 4: Run tests**

Run: `cd core && cargo test -p apgo-core recap:: && cargo clippy -p apgo-core --all-targets -- -D warnings`
Expected: 5 passed, no warnings.

- [ ] **Step 5: Update the spec.** In the spec's section 2, replace the event rows' sources (`quest_done` events etc.) with
  "outing snapshot diff (see `Outing`)", delete the `Journal::events` paragraph, and in section 3 replace the `RecapReady`
  bullet with "The app calls `pending_recap()` right after `bank_at_home`." Run
  `npx -y markdownlint-cli2 docs/superpowers/specs/2026-10-13-walk-recap-design.md` (0 issues).

- [ ] **Step 6: Commit**

```bash
git add core/src/recap.rs core/src/lib.rs docs/superpowers/specs/2026-10-13-walk-recap-design.md
git commit -m "feat: walk recap value and route thinning"
```

---

### Task 2: Open an outing on leaving home (core)

**Files:**

- Modify: `core/src/game.rs` (struct `Game` ~line 288, `credit_steps` ~874, `set_counting` ~927, `away_on_fix` ~1074, tests)

**Interfaces:**

- Consumes: nothing new.
- Produces:
  - `pub struct Outing { pub start_ms: i64, pub steps: i64, pub cells_at_start: usize, pub done_at_start: BTreeSet<i64>,
    pub items_at_start: usize, pub zones_at_start: BTreeSet<u32> }` (`Debug, Clone, PartialEq, Serialize, Deserialize`)
  - `Game` fields `#[serde(default)] pub outing: Option<Outing>` and `#[serde(default)] pub recap: Option<crate::recap::Recap>`

- [ ] **Step 1: Write the failing tests** in `game.rs` `mod tests` (next to the time-away tests, ~line 2240):

```rust
    #[test]
    fn leaving_home_opens_one_outing_that_a_car_ride_keeps_open() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.set_counting(false, 0);
        g.set_counting(true, 1_000);
        let o = g.outing.clone().expect("left home");
        assert_eq!((o.start_ms, o.steps, o.items_at_start), (1_000, 0, g.items.len()));
        g.set_counting(false, 2_000); // in the car
        g.set_counting(true, 3_000);
        assert_eq!(g.outing.as_ref().map(|o| o.start_ms), Some(1_000), "the car ride does not start a new walk");
    }

    #[test]
    fn counting_on_at_home_opens_no_outing_until_a_fix_away() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.set_counting(false, 0);
        g.counters.last_at_home = Some(true);
        g.set_counting(true, 1_000);
        assert!(g.outing.is_none());
        g.on_estimate(&fixat(destination(g.home, 90.0, 500.0), 60), None);
        assert_eq!(g.outing.as_ref().map(|o| o.start_ms), Some(60_000));
    }

    #[test]
    fn an_outing_counts_raw_steps_even_with_a_heavy_backpack() {
        let (mut g, pts) = forager_game();
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(pts[1], 1200), None);
        g.bag_size = 0; // two items over the bag: credited steps slow down
        g.set_counting(false, 1_300_000);
        g.set_counting(true, 1_400_000);
        g.on_steps(0, 1_500_000, None);
        g.on_steps(800, 1_600_000, None);
        assert_eq!(g.outing.as_ref().map(|o| o.steps), Some(800));
    }

    #[test]
    fn an_outing_survives_a_restart_and_old_saves_have_none() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.set_counting(false, 0);
        g.set_counting(true, 1_000);
        let back: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert_eq!(back.outing, g.outing, "unlike time away, the walk is kept across a restart");
        let mut old = serde_json::to_value(&g).unwrap();
        old.as_object_mut().unwrap().remove("outing");
        old.as_object_mut().unwrap().remove("recap");
        let old: Game = serde_json::from_value(old).unwrap();
        assert!(old.outing.is_none() && old.recap.is_none());
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core game::tests::outing game::tests::leaving_home game::tests::counting_on_at_home game::tests::an_outing`
Expected: compile errors (`outing` field missing).

- [ ] **Step 3: Implement.** Add above `pub struct Game`:

```rust
/// A walk away from home, open from leaving until arriving home (car rides in between do not close it). Snapshots, not a log:
/// what changed is worked out on arrival (`arrive_home`). Saved, so a restart mid-walk keeps it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outing {
    /// When the player left home, Unix ms.
    pub start_ms: i64,
    /// Raw steps walked so far (not slowed by the backpack).
    pub steps: i64,
    /// `fog.cells.len()` on leaving.
    pub cells_at_start: usize,
    /// Quests done on leaving.
    pub done_at_start: BTreeSet<i64>,
    /// `items.len()` on leaving.
    pub items_at_start: usize,
    /// Zones unlocked on leaving.
    pub zones_at_start: BTreeSet<u32>,
}
```

Add to `Game`, after `bag_size`:

```rust
    /// The walk under way, if the player is away from home (#walk-recap).
    #[serde(default)]
    pub outing: Option<Outing>,
    /// The recap of the last walk, until the player dismisses it.
    #[serde(default)]
    pub recap: Option<crate::recap::Recap>,
```

Initialise both to `None` wherever `Game` is built with a struct literal (`Game::create`; `cargo build` lists any other).

Add a method next to `set_counting`:

```rust
    // Leaving home: snapshot what the recap will compare with on arrival. An open outing (a car ride ended) is kept.
    fn open_outing(&mut self, t_ms: i64) {
        if self.outing.is_none() {
            self.outing = Some(Outing {
                start_ms: t_ms,
                steps: 0,
                cells_at_start: self.fog.cells.len(),
                done_at_start: self.done.clone(),
                items_at_start: self.items.len(),
                zones_at_start: self.unlocked_set(),
            });
        }
    }
```

Call it where time away starts:

- in `set_counting`, inside `if self.counters.away_mark.is_none() && !at_home { ... }` after setting the mark:
  `self.open_outing(t_ms);`
- in `away_on_fix`, in the final `else` branch after `self.counters.away_mark = Some(fix.t_ms);`:
  `self.open_outing(fix.t_ms);`

In `credit_steps`, after `self.counters.steps_last = Some(total);`:

```rust
        if let Some(o) = &mut self.outing {
            o.steps += gained;
        }
```

- [ ] **Step 4: Run tests**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core --all-targets -- -D warnings`
Expected: all pass (existing tests too), no warnings.

- [ ] **Step 5: Commit**

```bash
git add core/src/game.rs
git commit -m "feat: open a walk outing on leaving home"
```

---

### Task 3: Close the outing into a recap on arriving home (core)

**Files:**

- Modify: `core/src/game.rs` (next to `bank_at_home` ~line 1030, tests)

**Interfaces:**

- Consumes: `Outing` (Task 2), `Recap`, `Recap::worth_showing` (Task 1), existing `Game::bank_at_home`, `carried_total`,
  `unlocked_set`.
- Produces: `pub fn arrive_home(&mut self, t_ms: i64) -> (Vec<Event>, Option<Recap>)`. The recap has an empty route and
  distance 0; the engine sets them (Task 4).

- [ ] **Step 1: Write the failing tests** in `game.rs` `mod tests`:

```rust
    fn left_home(g: &mut Game, t_ms: i64) {
        g.set_counting(false, 0);
        g.set_counting(true, t_ms);
    }

    #[test]
    fn arriving_home_after_a_walk_gives_its_recap_once() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        left_home(&mut g, 0);
        g.on_steps(0, 1, None);
        g.on_steps(600, 2, None); // the 500-step member is done; solo reward "Hydrate!"
        let (_, r) = g.arrive_home(10 * 60_000);
        let r = r.expect("worth showing");
        assert_eq!((r.start_ms, r.end_ms, r.steps), (0, 600_000, 600));
        assert_eq!(r.quests_done.len(), 1);
        assert_eq!(r.items, vec!["Hydrate!".to_string()]);
        assert_eq!(r.checks_sent, 0, "a solo game sends no checks");
        assert!(g.outing.is_none());
        assert!(g.arrive_home(11 * 60_000).1.is_none(), "a second arrival has no outing to close");
    }

    #[test]
    fn a_short_or_idle_outing_closes_without_a_recap() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 5000 }]);
        left_home(&mut g, 0);
        g.on_steps(0, 1, None);
        g.on_steps(600, 2, None);
        assert!(g.arrive_home(4 * 60_000).1.is_none(), "under 5 min");
        assert!(g.outing.is_none(), "closed all the same");
        left_home(&mut g, 10 * 60_000);
        assert!(g.arrive_home(30 * 60_000).1.is_none(), "long, nothing happened");
    }

    #[test]
    fn a_zone_unlocked_on_the_walk_is_in_the_recap() {
        let mut g = cartographer_game();
        lock_zone_1(&mut g);
        left_home(&mut g, 0);
        g.receive_item("Progressive Zone Key", 1_000, None);
        let r = g.arrive_home(10 * 60_000).1.expect("a zone opened");
        assert_eq!(r.zones_unlocked, vec![1]);
    }

    #[test]
    fn a_shrunk_item_list_gives_no_items_instead_of_panicking() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.items = vec!["a".into(), "b".into()];
        left_home(&mut g, 0);
        g.items.clear(); // the server's list was replaced by a shorter one
        g.outing.as_mut().unwrap().steps = 900;
        assert!(g.arrive_home(10 * 60_000).1.is_some_and(|r| r.items.is_empty()));
    }

    #[test]
    fn arriving_home_banks_and_the_recap_counts_what_was_banked() {
        let (mut g, pts) = forager_game();
        left_home(&mut g, 0);
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(pts[1], 1200), None);
        g.set_counting(false, 1_250_000); // home Wi-Fi
        let r = g.arrive_home(1_300_000).1.expect("two items brought home");
        assert_eq!(r.banked, 2);
        assert_eq!(carried_banked(&g), (0, 2));
    }

    #[test]
    fn a_trap_blocking_the_bank_still_closes_the_walk() {
        let (mut g, pts) = forager_game();
        left_home(&mut g, 0);
        g.on_estimate(&fixat(pts[0], 600), None);
        g.outing.as_mut().unwrap().steps = 900;
        freeze(&mut g); // existing test helper: a Freeze Trap blocks checks, so banking
        let r = g.arrive_home(1_300_000).1.expect("steps alone are worth it");
        assert_eq!(r.banked, 0, "nothing banked under the trap");
        assert!(g.outing.is_none());
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core game::tests::arriv game::tests::a_`
Expected: compile errors (`arrive_home` not defined).

- [ ] **Step 3: Implement** next to `bank_at_home`:

```rust
    /// The player arrived home: bank what is carried (see `bank_at_home`) and close the walk. Returns the bank's events and the
    /// walk's recap when it is worth showing (route not set: the engine adds it from the journal). Safe to call again.
    pub fn arrive_home(&mut self, t_ms: i64) -> (Vec<Event>, Option<Recap>) {
        let carried = self.carried_total();
        let ev = self.bank_at_home(t_ms);
        let banked = carried.saturating_sub(self.carried_total());
        let recap = self.outing.take().map(|o| self.recap_of(&o, t_ms, banked)).filter(Recap::worth_showing);
        (ev, recap)
    }

    // What changed since the outing's snapshots.
    fn recap_of(&self, o: &Outing, t_ms: i64, banked: u32) -> Recap {
        let quests_done: Vec<String> =
            self.assignments.iter().filter(|a| self.done.contains(&a.location_id) && !o.done_at_start.contains(&a.location_id)).map(|a| a.quest_name.clone()).collect();
        let checks_sent = if self.backend == Backend::Archipelago { u32::try_from(quests_done.len()).unwrap_or(u32::MAX) } else { 0 };
        Recap {
            start_ms: o.start_ms,
            end_ms: t_ms,
            steps: o.steps,
            new_cells: u32::try_from(self.fog.cells.len().saturating_sub(o.cells_at_start)).unwrap_or(u32::MAX),
            quests_done,
            checks_sent,
            items: self.items.get(o.items_at_start..).unwrap_or_default().to_vec(),
            zones_unlocked: self.unlocked_set().difference(&o.zones_at_start).copied().collect(),
            banked,
            ..Recap::default()
        }
    }
```

Add `use crate::recap::Recap;` to the imports. If `Backend` does not derive `PartialEq`, use
`matches!(self.backend, Backend::Archipelago)`.

- [ ] **Step 4: Run tests**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core --all-targets -- -D warnings`
Expected: all pass, no warnings.

- [ ] **Step 5: Commit**

```bash
git add core/src/game.rs
git commit -m "feat: close the walk into a recap at home"
```

---

### Task 4: Engine stores the recap with its route (FFI)

**Files:**

- Modify: `core/ffi/src/engine.rs` (`bank_at_home` ~line 2308, records near `EventOut` ~609, tests near `engine_with_game` ~2598)

**Interfaces:**

- Consumes: `Game::arrive_home` (Task 3), `Recap::set_route` (Task 1), `Journal::segments`, `DEFAULT_MAX_GAP_MS`.
- Produces (UniFFI, Kotlin sees camelCase):
  - `#[derive(Debug, uniffi::Record)] pub struct RecapOut { start_ms: i64, end_ms: i64, steps: i64, distance_m: f64,
    new_cells: u32, quests_done: Vec<String>, checks_sent: u32, items: Vec<String>, zones_unlocked: Vec<u32>, banked: u32,
    route: Vec<Vec<GeoPoint>> }`
  - `Engine::pending_recap(&self) -> Option<RecapOut>`, `Engine::dismiss_recap(&self)`

- [ ] **Step 1: Write the failing tests** in `engine.rs` tests:

```rust
    #[test]
    fn arriving_home_stores_one_recap_with_its_route_until_dismissed() {
        let e = engine_with_game("recap");
        e.set_counting(false, 0);
        e.set_counting(true, 1_000);
        e.with_game(|g| g.outing.as_mut().unwrap().steps = 900);
        let east = |m: f64| destination(home(), 90.0, m);
        e.journal_do(|j| {
            (0..=10).try_for_each(|i| {
                let p = east(500.0 + f64::from(i) * 10.0);
                j.add_point("g1", &TrackPoint { t_ms: 60_000 + i64::from(i) * 5_000, lat: p.lat, lon: p.lon, accuracy_m: 5.0, simulated: false })
            })
        });
        e.set_counting(false, 599_000);
        e.bank_at_home(600_000);
        let r = e.pending_recap().expect("a 10 min walk with 900 steps");
        assert_eq!((r.steps, r.route.len(), r.route[0].len()), (900, 1, 2), "a straight line thins to its ends");
        assert!((r.distance_m - 100.0).abs() < 1.0);
        e.bank_at_home(610_000);
        assert!(e.pending_recap().is_some_and(|r| r.end_ms == 600_000), "a second arrival keeps the recap");
        let restarted = Engine::new(e.dir.to_string_lossy().into_owned());
        restarted.open_game("g1".into()).unwrap();
        assert!(restarted.pending_recap().is_some(), "saved with the game");
        restarted.dismiss_recap();
        assert!(restarted.pending_recap().is_none());
    }

    #[test]
    fn a_walk_with_no_trace_still_gets_its_recap() {
        let e = engine_with_game("recap-no-trace");
        e.set_counting(false, 0);
        e.set_counting(true, 1_000);
        e.with_game(|g| g.outing.as_mut().unwrap().steps = 900);
        e.bank_at_home(600_000);
        assert!(e.pending_recap().is_some_and(|r| r.distance_m == 0.0 && r.route.is_empty()));
    }
```

Check `engine_with_game`'s game id (`"g1"`) and that it opens the game; adjust the id in the test if it differs.

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-ffi recap`
Expected: compile errors (`pending_recap` not defined).

- [ ] **Step 3: Implement.** Add the record next to `EventOut`:

```rust
/// The walk recap ("Welcome home"), see `apgo_core::recap::Recap`.
#[derive(Debug, uniffi::Record)]
pub struct RecapOut {
    /// When the player left home, Unix ms.
    pub start_ms: i64,
    /// When they arrived home, Unix ms.
    pub end_ms: i64,
    /// Raw steps walked.
    pub steps: i64,
    /// Length of the recorded trace in metres.
    pub distance_m: f64,
    /// Map squares seen for the first time.
    pub new_cells: u32,
    /// Names of the quests completed.
    pub quests_done: Vec<String>,
    /// Checks sent to the server.
    pub checks_sent: u32,
    /// Items received.
    pub items: Vec<String>,
    /// Zones unlocked.
    pub zones_unlocked: Vec<u32>,
    /// Forager items brought home.
    pub banked: u32,
    /// The thinned trace, one line per recorded stretch.
    pub route: Vec<Vec<GeoPoint>>,
}

impl From<&Recap> for RecapOut {
    fn from(r: &Recap) -> Self {
        Self {
            start_ms: r.start_ms,
            end_ms: r.end_ms,
            steps: r.steps,
            distance_m: r.distance_m,
            new_cells: r.new_cells,
            quests_done: r.quests_done.clone(),
            checks_sent: r.checks_sent,
            items: r.items.clone(),
            zones_unlocked: r.zones_unlocked.clone(),
            banked: r.banked,
            route: r.route.iter().map(|s| s.iter().map(|p| GeoPoint { lat: p.lat, lon: p.lon }).collect()).collect(),
        }
    }
}
```

Replace `bank_at_home` (the journal is read after the game lock is released, as elsewhere in this file):

```rust
    /// The phone arrived home (home Wi-Fi or the home circle): forager quests bank what they carry and the walk closes into its
    /// recap (see `Game::arrive_home`), kept with the game until dismissed. Safe to call again.
    pub fn bank_at_home(&self, t_ms: i64) -> Vec<EventOut> {
        let Some((game_id, ev, entries, recap)) = self.with_game(|g| {
            let (ev, recap) = g.arrive_home(t_ms);
            self.save_if_due(g, t_ms, true);
            let entries = g.journal_events(&ev, t_ms, None);
            (g.id.clone(), ev, entries, recap)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&game_id, e)));
        if let Some(mut r) = recap {
            let segs = self.journal.as_ref().map(|j| j.lock().unwrap_or_else(std::sync::PoisonError::into_inner).segments(&game_id, r.start_ms, r.end_ms, DEFAULT_MAX_GAP_MS));
            let segs = match segs {
                Some(Ok(s)) => s,
                Some(Err(e)) => {
                    self.note(format!("journal read failed: {e}"));
                    Vec::new()
                }
                None => Vec::new(),
            };
            r.set_route(&segs.iter().map(|s| s.iter().map(|p| Point::new(p.lat, p.lon)).collect()).collect::<Vec<_>>());
            self.with_game(|g| {
                g.recap = Some(r);
                if let Err(e) = g.save(&self.dir) {
                    self.note(format!("could not save game {}: {e}", g.id));
                }
            });
        }
        self.events_out(ev)
    }

    /// The last walk's recap, until dismissed.
    pub fn pending_recap(&self) -> Option<RecapOut> {
        self.with_game(|g| g.recap.as_ref().map(RecapOut::from)).flatten()
    }

    /// The player closed the recap.
    pub fn dismiss_recap(&self) {
        self.with_game(|g| {
            if g.recap.take().is_some() {
                if let Err(e) = g.save(&self.dir) {
                    self.note(format!("could not save game {}: {e}", g.id));
                }
            }
        });
    }
```

Add `use apgo_core::recap::Recap;` and make sure `Point` (`apgo_core::geo::Point`) is imported.

- [ ] **Step 4: Run tests and the full Rust check**

Run: `cd core && cargo test -p apgo-ffi && cd .. && just check-rust`
Expected: all pass. If coverage fails its floor, add a test for the uncovered branch, never lower the floor.

- [ ] **Step 5: Commit**

```bash
git add core/ffi/src/engine.rs
git commit -m "feat: store the walk recap with its route"
```

---

### Task 5: Notify on arrival and hold the recap in the app (Android)

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/RecapNotice.kt`
- Test: `android/app/src/test/java/dev/apgo2/RecapNoticeTest.kt`
- Modify: `android/app/src/main/java/dev/apgo2/AppModel.kt` (state, `onForeground` ~333), `PresenceController.kt`
  (`bankAtHome` ~205), `ui/HelpText.kt`

**Interfaces:**

- Consumes: `Engine.pendingRecap(): RecapOut?`, `Engine.dismissRecap()` (Task 4, generated Kotlin bindings).
- Produces: `AppModel.recap: RecapOut?` (Compose state), `AppModel.onArrivedHome()`, `AppModel.dismissRecap()`,
  `Help.recapNoticeTitle`, `Help.recapNoticeText(recap: RecapOut): String`.

- [ ] **Step 1: Write the failing test** `RecapNoticeTest.kt` (pure JVM, like `TrapAlertTest.kt`):

```kotlin
package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RecapNoticeTest {
    // On screen the recap opens at once; off screen it waits behind a notification.
    @Test fun notifiesOnlyOffScreen() {
        assertTrue(RecapNotice.shouldPost(appVisible = false))
        assertFalse(RecapNotice.shouldPost(appVisible = true))
    }

    @Test fun textSaysHowLongAndHowManyQuests() {
        val r = recap(minutes = 42, quests = listOf("A", "B"))
        assertEquals("42 min out, 2 quests done", Help.recapNoticeText(r))
        assertEquals("42 min out", Help.recapNoticeText(recap(minutes = 42, quests = emptyList())))
        assertEquals("42 min out, 1 quest done", Help.recapNoticeText(recap(minutes = 42, quests = listOf("A"))))
    }
}

internal fun recap(
    minutes: Long,
    quests: List<String> = emptyList(),
) = uniffi.apgo_ffi.RecapOut(
    startMs = 0,
    endMs = minutes * 60_000,
    steps = 0,
    distanceM = 0.0,
    newCells = 0u,
    questsDone = quests,
    checksSent = 0u,
    items = emptyList(),
    zonesUnlocked = emptyList(),
    banked = 0u,
    route = emptyList(),
)
```

Check the generated bindings' package (look at the `import` of `EventOut` in `AppModel.kt`) and use the same one.

- [ ] **Step 2: Run to verify it fails**

Run: `just check-android` (or `cd android && ./gradlew :app:testDebugUnitTest --tests dev.apgo2.RecapNoticeTest`)
Expected: compile errors (`RecapNotice` not defined).

- [ ] **Step 3: Implement.** `HelpText.kt`, inside `object Help`:

```kotlin
    val recapNoticeTitle = "Welcome home: see your walk"

    fun recapNoticeText(r: RecapOut): String {
        val time = "${AwayFormat.duration(r.endMs - r.startMs)} out"
        val n = r.questsDone.size
        return if (n == 0) time else "$time, $n ${if (n == 1) "quest" else "quests"} done"
    }
```

`RecapNotice.kt` (same shape as `TrapAlert.kt`, quiet channel):

```kotlin
package dev.apgo2

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import dev.apgo2.ui.Help

/** "Welcome home: see your walk": posted when a walk's recap is ready while the app is off screen. Opening the app shows it. */
internal class RecapNotice(
    private val ctx: Context,
) {
    private val nm = ctx.getSystemService(NotificationManager::class.java)

    init {
        val channel =
            NotificationChannel(CHANNEL, "Walk recap", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "How your walk went, when you get home"
                enableVibration(false)
                setSound(null, null)
            }
        nm.createNotificationChannel(channel)
    }

    fun post(recap: RecapOut) {
        if (!nm.areNotificationsEnabled()) return
        val open = PendingIntent.getActivity(ctx, 0, Intent(ctx, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE)
        val n =
            Notification
                .Builder(ctx, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_dialog_info)
                .setContentTitle(Help.recapNoticeTitle)
                .setContentText(Help.recapNoticeText(recap))
                .setAutoCancel(true)
                .setContentIntent(open)
                .build()
        nm.notify(ID, n)
    }

    fun cancel() = nm.cancel(ID)

    companion object {
        private const val CHANNEL = "walk_recap_v1"
        private const val ID = 3000

        fun shouldPost(appVisible: Boolean) = !appVisible
    }
}
```

`AppModel.kt`: next to `trapAlert` (~line 88):

```kotlin
    private val recapNotice = RecapNotice(ctx)

    /** The last walk's recap on show, until dismissed (the core keeps it; this mirrors it for Compose). */
    var recap by mutableStateOf<RecapOut?>(null)
        private set

    /** Arrived home: show the walk's recap, or notify when off screen. */
    fun onArrivedHome() {
        val r = engine.pendingRecap() ?: return
        recap = r
        if (RecapNotice.shouldPost(presence.appVisible)) recapNotice.post(r)
    }

    fun dismissRecap() {
        engine.dismissRecap()
        recap = null
        recapNotice.cancel()
    }
```

In `onForeground()`, after `units.onForeground()`: `recap = engine.pendingRecap()` (tapping the notification opens the app,
so this is what shows the recap; it also survives a process restart). Do the same after a game is opened (find where
`AppModel` calls `engine.openGame`).

`PresenceController.bankAtHome()`: after `model.handle(...)` add `model.onArrivedHome()`.

- [ ] **Step 4: Run tests**

Run: `just check-android`
Expected: pass (ktlint, detekt, unit tests).

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/RecapNotice.kt android/app/src/test/java/dev/apgo2/RecapNoticeTest.kt \
  android/app/src/main/java/dev/apgo2/AppModel.kt android/app/src/main/java/dev/apgo2/PresenceController.kt \
  android/app/src/main/java/dev/apgo2/ui/HelpText.kt
git commit -m "feat: notify when a walk recap is ready"
```

---

### Task 6: Recap screen with stats and results (Android)

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/RecapFormat.kt`, `android/app/src/main/java/dev/apgo2/RecapScreen.kt`
- Test: `android/app/src/test/java/dev/apgo2/RecapFormatTest.kt`
- Modify: `android/app/src/main/java/dev/apgo2/AppRoot.kt` (`AppBody` ~line 100), `ui/HelpText.kt`

**Interfaces:**

- Consumes: `AppModel.recap`, `AppModel.dismissRecap()` (Task 5), `AwayFormat.duration`, the app's `Units` (`distance(m)`).
- Produces: `RecapFormat.stats(r, units): List<Pair<String, String>>`, `RecapFormat.sections(r): List<Pair<String, List<String>>>`,
  `@Composable fun RecapScreen(r: RecapOut, units: Units, onDone: () -> Unit)`.

- [ ] **Step 1: Write the failing test** `RecapFormatTest.kt`:

```kotlin
package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class RecapFormatTest {
    private val units = dev.apgo2.ui.Units(dev.apgo2.ui.UnitSystem.METRIC)

    @Test fun statsShowTimeDistanceStepsAndSquares() {
        val r = recap(minutes = 75).copy(steps = 6543, distanceM = 4200.0, newCells = 12u)
        assertEquals(
            listOf("Time out" to "1 h 15 min", "Distance" to units.distance(4200.0), "Steps" to "6,543", "New squares" to "12"),
            RecapFormat.stats(r, units),
        )
    }

    @Test fun emptyResultSectionsAreLeftOut() {
        val r = recap(minutes = 10, quests = listOf("Reach the park")).copy(items = listOf("Hydrate!"), banked = 3u)
        assertEquals(
            listOf("Quests completed" to listOf("Reach the park"), "Items received" to listOf("Hydrate!"), "Brought home" to listOf("3 items")),
            RecapFormat.sections(r),
        )
        assertEquals(emptyList<Pair<String, List<String>>>(), RecapFormat.sections(recap(minutes = 10)))
    }

    @Test fun zonesAndChecksAreNamed() {
        val r = recap(minutes = 10).copy(zonesUnlocked = listOf(2u), checksSent = 4u)
        assertEquals(listOf("Checks sent" to listOf("4"), "Zones unlocked" to listOf("Zone 2")), RecapFormat.sections(r))
    }
}
```

Check how `Units` is built in other tests (`android/app/src/test/java/dev/apgo2/ui/`) and match the constructor.

- [ ] **Step 2: Run to verify it fails**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests dev.apgo2.RecapFormatTest`
Expected: compile errors (`RecapFormat` not defined).

- [ ] **Step 3: Implement.** `RecapFormat.kt`:

```kotlin
package dev.apgo2

import dev.apgo2.ui.Units
import java.util.Locale

/** Plain-text pieces of the walk recap. Labels match the "while you were out" report ([AwayFormat]). */
internal object RecapFormat {
    fun stats(
        r: RecapOut,
        units: Units,
    ): List<Pair<String, String>> =
        listOf(
            "Time out" to AwayFormat.duration(r.endMs - r.startMs),
            "Distance" to units.distance(r.distanceM),
            "Steps" to "%,d".format(Locale.US, r.steps),
            "New squares" to r.newCells.toString(),
        )

    fun sections(r: RecapOut): List<Pair<String, List<String>>> =
        listOf(
            AwayFormat.kindLabel("quest_done") to r.questsDone,
            AwayFormat.kindLabel("check_sent") to if (r.checksSent > 0u) listOf(r.checksSent.toString()) else emptyList(),
            AwayFormat.kindLabel("item_received") to r.items,
            AwayFormat.kindLabel("zone_unlocked") to r.zonesUnlocked.map { "Zone $it" },
            "Brought home" to if (r.banked > 0u) listOf("${r.banked} ${if (r.banked == 1u) "item" else "items"}") else emptyList(),
        ).filter { it.second.isNotEmpty() }
}
```

`RecapScreen.kt`: a full-screen `Surface` over the app with, top to bottom: the title `Help.recapTitle` ("Welcome home"),
a row of stat tiles from `RecapFormat.stats`, one block per `RecapFormat.sections` entry (label + lines), and a "Done" button
that calls `onDone`. Use `ui/Components.kt` pieces and `ui/Palette.kt` / `MaterialTheme` colours only (no inline colours);
keep the markup minimal (one `LazyColumn`, no wrapper layouts that add nothing). `BackHandler(onBack = onDone)`.

```kotlin
@Composable
internal fun RecapScreen(
    r: RecapOut,
    units: Units,
    onDone: () -> Unit,
) {
    BackHandler(onBack = onDone)
    Surface(Modifier.fillMaxSize()) {
        LazyColumn(contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            item { Text(Help.recapTitle, style = MaterialTheme.typography.headlineMedium) }
            item {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    RecapFormat.stats(r, units).forEach { (label, value) -> StatTile(label, value) }
                }
            }
            items(RecapFormat.sections(r)) { (label, lines) ->
                Text(label, style = MaterialTheme.typography.titleSmall)
                lines.forEach { Text(it) }
            }
            item { Button(onClick = onDone, modifier = Modifier.fillMaxWidth()) { Text("Done") } }
        }
    }
}

@Composable
private fun StatTile(
    label: String,
    value: String,
) {
    Card {
        Text(value, Modifier.padding(start = 12.dp, top = 8.dp, end = 12.dp), style = MaterialTheme.typography.titleLarge)
        Text(label, Modifier.padding(start = 12.dp, end = 12.dp, bottom = 8.dp), style = MaterialTheme.typography.labelMedium)
    }
}
```

Add `val recapTitle = "Welcome home"` to `Help`. In `AppRoot.kt` `AppBody`, last thing inside the `Box(Modifier.weight(1f))`:
`m.recap?.let { RecapScreen(it, m.units, onDone = m::dismissRecap) }` (check what `m.units` is called on `AppModel`).

- [ ] **Step 4: Run tests and check on the emulator**

Run: `just check-android`, then `just emu-start && just emu-run`. With the dev simulator: leave home, walk until a quest is done,
come home. Expected: the recap screen opens (app on screen) or the notification appears (app in the background); tapping it
shows the recap; "Done" and Back close it and it does not come back.

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/RecapFormat.kt android/app/src/main/java/dev/apgo2/RecapScreen.kt \
  android/app/src/test/java/dev/apgo2/RecapFormatTest.kt android/app/src/main/java/dev/apgo2/AppRoot.kt \
  android/app/src/main/java/dev/apgo2/ui/HelpText.kt
git commit -m "feat: walk recap screen"
```

---

### Task 7: Route map on the recap screen (Android) + docs

**Files:**

- Modify: `android/app/src/main/java/dev/apgo2/RecapScreen.kt`, `android/app/src/main/java/dev/apgo2/MapGeoJson.kt`
- Test: `android/app/src/test/java/dev/apgo2/MapGeoJsonTest.kt`
- Modify: `docs/context/v1-architecture-and-status.md` (add a short "Walk recap" paragraph)

**Interfaces:**

- Consumes: `RecapOut.route` (Task 4), `MapGeoJson.lineString(points: List<Pair<Double, Double>>)` (exists), the static map
  used by `ui/RealmPreview.kt`.
- Produces: `MapGeoJson.route(segments: List<List<GeoPoint>>): JSONObject` (a FeatureCollection, one LineString per segment).

- [ ] **Step 1: Write the failing test** in `MapGeoJsonTest.kt`:

```kotlin
    @Test fun aRouteIsOneLinePerStretchAndSkipsLonePoints() {
        val a = GeoPoint(40.0, -111.0)
        val b = GeoPoint(40.001, -111.0)
        val fc = MapGeoJson.route(listOf(listOf(a, b), listOf(a)))
        val features = fc.getJSONArray("features")
        assertEquals(1, features.length())
        assertEquals("LineString", features.getJSONObject(0).getJSONObject("geometry").getString("type"))
    }
```

First read `MapGeoJson.lineString` to check whether it takes `(lat, lon)` or `(lon, lat)` pairs, and pass the right order.

- [ ] **Step 2: Run to verify it fails**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests dev.apgo2.MapGeoJsonTest`
Expected: compile error (`route` not defined).

- [ ] **Step 3: Implement.** In `MapGeoJson` (follow its existing feature/collection helpers):

```kotlin
    /** The walk recap's route: one line per recorded stretch (a lone point draws nothing). */
    fun route(segments: List<List<GeoPoint>>): JSONObject =
        featureCollection(segments.filter { it.size >= 2 }.map { s -> feature(lineString(s.map { it.lat to it.lon })) })
```

(Use the names of the helpers that `MapGeoJson` already has for a feature and a collection.)

In `RecapScreen`, add an `item` after the stats when `r.route.isNotEmpty()`: a 200 dp tall, non-interactive map built the way
`ui/RealmPreview.kt` builds its preview (same base style, gestures off), with a GeoJSON source of `MapGeoJson.route(r.route)`
and a line layer styled through `MapStyle`/`ui/Palette.kt` tokens (renderer styling, no app-side drawing). Fit the camera to
the route's bounds with padding.

- [ ] **Step 4: Run tests and check on the emulator**

Run: `just check-android`, then repeat the Task 6 emulator walk. Expected: the route line shows on the recap, fitted to the walk.

- [ ] **Step 5: Docs.** Add to `docs/context/v1-architecture-and-status.md` a paragraph: outing opened on leaving home
  (`Game::open_outing`, snapshots), closed by `Game::arrive_home` from `Engine::bank_at_home`, recap kept in `Game.recap` until
  `dismiss_recap`, route from journal segments thinned with `geo::Simplify`, Android `RecapNotice` + `RecapScreen`. Run
  `just check-hygiene`.

- [ ] **Step 6: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/RecapScreen.kt android/app/src/main/java/dev/apgo2/MapGeoJson.kt \
  android/app/src/test/java/dev/apgo2/MapGeoJsonTest.kt docs/context/v1-architecture-and-status.md
git commit -m "feat: route map on the walk recap"
```
