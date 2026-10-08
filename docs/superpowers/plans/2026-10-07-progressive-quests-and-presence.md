# Progressive Quests and Presence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn Step Up, Wanderlust and Cartographer into one progress bar per kind with a mark per unlocked check (Part A), and add presence rules (home Wi-Fi, car Bluetooth, zone-based GPS) that decide when progress counts (Part B).

**Architecture:** Chains are derived from the quests already assigned (`core/src/chain.rs`), with saved counters in the game file; completion goes through the existing `Game::complete`. Part B is a pure Kotlin state machine (`PresencePolicy`) that sets GPS rate and one `counting` flag in the core; the core never knows about Wi-Fi or Bluetooth.

**Tech Stack:** Rust core (`apgo-core`, `apgo-ffi` via UniFFI), Kotlin/Compose Android app (min SDK 26, target 36), JUnit4 unit tests, MapLibre.

**Spec:** `docs/superpowers/specs/2026-10-07-progressive-quests-design.md` (Part A sections 1-7, Part B sections B.1-B.8). GitHub issue #6 is Part B; close it with the last task.

## Global Constraints
- TDD: write the failing test first, run it and see it fail for the right reason, then implement. Never edit a test to fit bad code.
- Commits: conventional (`feat:`, `fix:`, `docs:`), **subject at most 50 characters, imperative mood** ("add", not "adds"); body lines at most 72 characters; end with the line `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Hooks run `typos` (avoid abbreviated or misspelled identifiers), `gitleaks`, and `committed`.
- Rust: `cd core && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test -q` must be clean before each Rust commit.
- Android: `cd android && ./gradlew :app:testDebugUnitTest :app:assembleDebug -q` prints nothing on success; any `e:` or `FAILED` line is a failure. After changing `core/` rebuild the library and bindings first: from the repo root `APGO_ABIS="arm64-v8a x86_64" scripts/android_core.sh debug`.
- Saved games must keep loading: every new `Game` field is `#[serde(default)]` (or `skip_serializing` with a default function) and never required.
- Kotlin colours come from `ApgoPalette`, icons from `ApgoIcons`, map pins only through `MapMarkers` (see `docs/context/ui-design-system.md`).
- Location is "Allow all the time" plus the foreground service; nothing in this plan may start work when no game is open.
- Never work on `main`; the current branch is `feat/adaptive-gps-interval`.

## Review Focus
Failure modes the spec implies but no happy-path task would exercise (each has a named test in the task that owns it):
1. **Phone reboot mid-game** resets the step counter below the last reading: credit the new value, never a negative. (Task 4)
2. **A game with none of these quest kinds, or a chain with one member**: no chain, no panic, one-mark bar. (Task 1)
3. **Steps taken while the game is stopped**, then a huge reading after reopening: nothing credited for the gap. (Task 4)
4. **Reroll or Shuffle trap on a chain**: counters untouched, chain members cannot be rerolled. (Task 7)
5. **Old saved game with finished Step Up/Wanderlust/Cartographer quests**: nothing lost, nothing earned twice. (Task 7)
6. **Home Wi-Fi flapping at the edge of range** must not toggle GPS repeatedly. (Task 15)
7. **Permission denied or signal unavailable** (Bluetooth, Wi-Fi name `<unknown ssid>`): treated as "not present", the app behaves as today. (Tasks 15, 16)
8. **Custom away distance typed as empty text or nonsense**: falls back to a sane value, never crashes. (Task 11)

## File Structure
Part A (core): `core/src/chain.rs` (new: chain model, derivation, text), `core/src/geo.rs` (+`distance_to_segment_m`), `core/src/realm.rs` (+`Shape::distance_m`), `core/src/game.rs` (counters, away config, counting, views), `core/src/journal.rs` (unchanged API), `core/ffi/src/engine.rs` (records, `chains`, `on_steps`, zone shapes, start parameters).
Part A (Android): `ui/ChainFormat.kt` (new: pure text and tick maths), `ui/ChainBar.kt` (new: the bar composable), `PlayLayout.kt` (chain members leave the lists), `Screens.kt` (Progress section, chain popup), `AppModel.kt`, `Sensors.kt`, `NewGame.kt`, `ui/HelpText.kt`, `AwaySettings.kt` (new: parse the distance field).
Part B (core): `game.rs` (+`counting`), `engine.rs` (+`zone_proximity`, `set_counting`, `log_presence`), `journal.rs` (+kind).
Part B (Android): `presence/PresencePolicy.kt` (new, pure), `presence/PresenceSignals.kt` (new, pure matching/parsing), `presence/PresenceSettings.kt`, `presence/PresenceMonitor.kt` (Android signal sources), `AppModel.kt`, `Sensors.kt`, `GpsPolicy.kt`, `Screens.kt` (status chip), `PresenceScreen.kt` (settings), `AndroidManifest.xml`.

---

# Part A: progressive quest chains

### Task 1: Chain model and derivation

**Files:**
- Create: `core/src/chain.rs`
- Modify: `core/src/lib.rs` (add `pub mod chain;` after `pub mod catalog;`)

**Interfaces:**
- Produces: `ChainUnit {Steps, Minutes, Cells}`, `Milestone {location_id: i64, at: f64}`, `Chain {id: String, zone: u32, kind_id: String, name: String, unit: ChainUnit, marks: Vec<Milestone>}`, `amount_of(&Target) -> Option<(ChainUnit, f64)>`, `is_chain_target(&Target) -> bool`, `derive(&[Assignment]) -> Vec<Chain>`, `Chain::{total, reached(counter) -> Vec<i64>, position_of(location_id) -> Option<usize>, rule_text(away_m), amount_text(at)}`, `thousands(u64)`, `minutes_text(f64)`, `distance_text(f64)`.

- [ ] **Step 1: Write the failing tests and stubs.** Create `core/src/chain.rs` with the content below (types and `todo!()` bodies first, tests at the bottom), and register the module.

```rust
//! Progressive quests: the quests of one kind in one zone that share a counter (steps, minutes away, map squares),
//! shown as one bar with a mark for each check they unlock.

use std::collections::BTreeMap;

use crate::assign::{Assignment, Target};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainUnit {
    Steps,
    Minutes,
    Cells,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Milestone {
    pub location_id: i64,
    /// The counter value at which this check unlocks (a running total of the members' own amounts).
    pub at: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    /// `"{zone}:{kind_id}"`.
    pub id: String,
    pub zone: u32,
    pub kind_id: String,
    pub name: String,
    pub unit: ChainUnit,
    pub marks: Vec<Milestone>,
}

/// The unit and amount a quest contributes to a chain; `None` for quests that are not progressive.
pub fn amount_of(_t: &Target) -> Option<(ChainUnit, f64)> {
    todo!()
}

pub fn is_chain_target(t: &Target) -> bool {
    amount_of(t).is_some()
}

/// 30000 -> "30,000".
pub fn thousands(_n: u64) -> String {
    todo!()
}

/// 270 -> "4 h 30 min", 45 -> "45 min", 120 -> "2 h".
pub fn minutes_text(_m: f64) -> String {
    todo!()
}

/// 850 -> "850 m", 1200 -> "1.2 km".
pub fn distance_text(_m: f64) -> String {
    todo!()
}

impl Chain {
    pub fn total(&self) -> f64 {
        todo!()
    }

    /// Location ids whose mark is at or below `counter`, in mark order.
    pub fn reached(&self, _counter: f64) -> Vec<i64> {
        todo!()
    }

    /// 1-based position of a member among the marks ("milestone 3 of 5").
    pub fn position_of(&self, _location_id: i64) -> Option<usize> {
        todo!()
    }

    /// "Take 30,000 steps" / "Spend 4 h 30 min at least 1.2 km from home" / "Visit 60 new map squares".
    pub fn rule_text(&self, _away_m: f64) -> String {
        todo!()
    }

    /// The amount at a mark: "8,500 steps", "1 h 30 min", "40 squares".
    pub fn amount_text(&self, _at: f64) -> String {
        todo!()
    }
}

/// Group the progressive quests by zone and kind. Members are ordered by their own amount (ties by location id); each mark is the running total.
pub fn derive(_assignments: &[Assignment]) -> Vec<Chain> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Mode;
    use crate::geo::Point;

    pub(crate) fn member(id: i64, zone: u32, kind: &str, target: Target) -> Assignment {
        Assignment {
            location_id: id,
            zone,
            mode: Mode::Walk,
            family: "steps".into(),
            kind_id: kind.into(),
            quest_name: kind.replace('_', " "),
            blurb: String::new(),
            place: "Anywhere".into(),
            tier: 1,
            effort_min: 10.0,
            target,
            fallback: false,
            boss: false,
        }
    }

    fn steps(id: i64, n: u32) -> Assignment {
        member(id, 1, "step_up", Target::Steps { n })
    }

    #[test]
    fn amount_of_maps_only_the_three_progressive_targets() {
        assert_eq!(amount_of(&Target::Steps { n: 500 }), Some((ChainUnit::Steps, 500.0)));
        assert_eq!(amount_of(&Target::Away { min_distance_m: 900.0, minutes: 45.0 }), Some((ChainUnit::Minutes, 45.0)));
        assert_eq!(amount_of(&Target::Cells { n: 12, cell_m: 150.0 }), Some((ChainUnit::Cells, 12.0)));
        assert_eq!(amount_of(&Target::Point { p: Point::new(0.0, 0.0), r: 40.0 }), None);
        assert!(is_chain_target(&Target::Steps { n: 1 }));
    }

    #[test]
    fn marks_are_running_totals_of_the_members_sorted_by_amount() {
        let chains = derive(&[steps(30, 5500), steps(10, 500), steps(20, 2500)]);
        assert_eq!(chains.len(), 1);
        let c = &chains[0];
        assert_eq!(c.id, "1:step_up");
        assert_eq!((c.zone, c.unit, c.name.as_str()), (1, ChainUnit::Steps, "step up"));
        assert_eq!(c.marks, vec![Milestone { location_id: 10, at: 500.0 }, Milestone { location_id: 20, at: 3000.0 }, Milestone { location_id: 30, at: 8500.0 }]);
        assert_eq!(c.total(), 8500.0);
    }

    #[test]
    fn equal_amounts_are_ordered_by_location_id() {
        let c = &derive(&[steps(7, 1000), steps(3, 1000)])[0];
        assert_eq!(c.marks.iter().map(|m| m.location_id).collect::<Vec<_>>(), vec![3, 7]);
        assert_eq!(c.marks.iter().map(|m| m.at).collect::<Vec<_>>(), vec![1000.0, 2000.0]);
    }

    #[test]
    fn zones_and_kinds_make_separate_chains_and_other_quests_are_ignored() {
        let p = Point::new(40.0, -111.0);
        let list = vec![
            steps(1, 500),
            member(2, 2, "step_up", Target::Steps { n: 500 }),
            member(3, 1, "wanderlust", Target::Away { min_distance_m: 900.0, minutes: 30.0 }),
            member(4, 1, "street_smarts", Target::Point { p, r: 40.0 }),
        ];
        let ids: Vec<String> = derive(&list).into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["1:step_up", "1:wanderlust", "2:step_up"]);
    }

    #[test]
    fn no_progressive_quests_means_no_chains_and_one_member_is_a_one_mark_bar() {
        assert!(derive(&[]).is_empty());
        let c = &derive(&[steps(1, 800)])[0];
        assert_eq!(c.marks.len(), 1);
        assert_eq!(c.total(), 800.0);
    }

    #[test]
    fn reached_lists_the_marks_at_or_below_the_counter() {
        let c = &derive(&[steps(10, 500), steps(20, 2500), steps(30, 5500)])[0];
        assert!(c.reached(499.0).is_empty());
        assert_eq!(c.reached(3000.0), vec![10, 20]);
        assert_eq!(c.reached(1e9), vec![10, 20, 30]);
        assert_eq!(c.position_of(20), Some(2));
        assert_eq!(c.position_of(99), None);
    }

    #[test]
    fn texts_read_naturally() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(950), "950");
        assert_eq!(thousands(30_000), "30,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(minutes_text(45.0), "45 min");
        assert_eq!(minutes_text(120.0), "2 h");
        assert_eq!(minutes_text(270.0), "4 h 30 min");
        assert_eq!(distance_text(850.0), "850 m");
        assert_eq!(distance_text(1200.0), "1.2 km");
        let s = &derive(&[steps(10, 500), steps(20, 29_500)])[0];
        assert_eq!(s.rule_text(0.0), "Take 30,000 steps");
        assert_eq!(s.amount_text(8500.0), "8,500 steps");
        let a = &derive(&[member(1, 1, "wanderlust", Target::Away { min_distance_m: 1.0, minutes: 270.0 })])[0];
        assert_eq!(a.rule_text(1200.0), "Spend 4 h 30 min at least 1.2 km from home");
        assert_eq!(a.amount_text(90.0), "1 h 30 min");
        let c = &derive(&[member(1, 1, "cartographer", Target::Cells { n: 60, cell_m: 150.0 })])[0];
        assert_eq!(c.rule_text(0.0), "Visit 60 new map squares");
        assert_eq!(c.amount_text(40.0), "40 squares");
    }
}
```

- [ ] **Step 2: Run the tests and see them fail.** Run: `cd core && cargo test -q chain 2>&1 | tail -15`. Expected: tests FAIL with `not yet implemented`.

- [ ] **Step 3: Implement.** Replace each `todo!()` body:

```rust
pub fn amount_of(t: &Target) -> Option<(ChainUnit, f64)> {
    match t {
        Target::Steps { n } => Some((ChainUnit::Steps, f64::from(*n))),
        Target::Away { minutes, .. } => Some((ChainUnit::Minutes, *minutes)),
        Target::Cells { n, .. } => Some((ChainUnit::Cells, f64::from(*n))),
        _ => None,
    }
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn minutes_text(m: f64) -> String {
    let m = m.max(0.0).round() as u64;
    match (m / 60, m % 60) {
        (0, r) => format!("{r} min"),
        (h, 0) => format!("{h} h"),
        (h, r) => format!("{h} h {r} min"),
    }
}

pub fn distance_text(m: f64) -> String {
    if m >= 1000.0 {
        format!("{:.1} km", m / 1000.0)
    } else {
        format!("{m:.0} m")
    }
}

impl Chain {
    pub fn total(&self) -> f64 {
        self.marks.last().map_or(0.0, |m| m.at)
    }

    pub fn reached(&self, counter: f64) -> Vec<i64> {
        self.marks.iter().filter(|m| m.at <= counter + 1e-9).map(|m| m.location_id).collect()
    }

    pub fn position_of(&self, location_id: i64) -> Option<usize> {
        self.marks.iter().position(|m| m.location_id == location_id).map(|i| i + 1)
    }

    pub fn rule_text(&self, away_m: f64) -> String {
        let t = self.total();
        match self.unit {
            ChainUnit::Steps => format!("Take {} steps", thousands(t.round() as u64)),
            ChainUnit::Minutes => format!("Spend {} at least {} from home", minutes_text(t), distance_text(away_m)),
            ChainUnit::Cells => format!("Visit {} new map squares", t.round() as u64),
        }
    }

    pub fn amount_text(&self, at: f64) -> String {
        match self.unit {
            ChainUnit::Steps => format!("{} steps", thousands(at.round() as u64)),
            ChainUnit::Minutes => minutes_text(at),
            ChainUnit::Cells => format!("{} squares", at.round() as u64),
        }
    }
}

pub fn derive(assignments: &[Assignment]) -> Vec<Chain> {
    let mut groups: BTreeMap<(u32, String), Vec<(i64, String, ChainUnit, f64)>> = BTreeMap::new();
    for a in assignments {
        if let Some((unit, amount)) = amount_of(&a.target) {
            groups.entry((a.zone, a.kind_id.clone())).or_default().push((a.location_id, a.quest_name.clone(), unit, amount));
        }
    }
    groups
        .into_iter()
        .map(|((zone, kind_id), mut members)| {
            members.sort_by(|a, b| a.3.total_cmp(&b.3).then(a.0.cmp(&b.0)));
            let (name, unit) = (members[0].1.clone(), members[0].2);
            let mut running = 0.0;
            let marks = members
                .iter()
                .map(|(location_id, _, _, amount)| {
                    running += amount;
                    Milestone { location_id: *location_id, at: running }
                })
                .collect();
            Chain { id: format!("{zone}:{kind_id}"), zone, kind_id, name, unit, marks }
        })
        .collect()
}
```

- [ ] **Step 4: Run and see green.** Run: `cd core && cargo test -q chain 2>&1 | tail -5`. Expected: `test result: ok`.

- [ ] **Step 5: Lint and commit.**
```bash
cd core && cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | grep -E "^(warning|error)" -A6 | head
cd .. && git add core/src/chain.rs core/src/lib.rs && git commit -m "feat: add progressive quest chains" -m "Group steps, time-away and map-square quests by zone and kind into one chain with running-total marks, plus the text for them." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Distance from a point to a zone shape

**Files:**
- Modify: `core/src/geo.rs` (add `distance_to_segment_m`), `core/src/realm.rs` (add `Shape::distance_m`)

**Interfaces:**
- Produces: `geo::distance_to_segment_m(p: Point, a: Point, b: Point) -> f64`; `Shape::distance_m(&self, p: Point) -> f64` (0 when inside).

- [ ] **Step 1: Write the failing tests.** Append to the `tests` module of `core/src/realm.rs` (find it with `grep -n "mod tests" core/src/realm.rs`) :

```rust
    #[test]
    fn distance_to_a_circle_is_zero_inside_and_the_gap_outside() {
        let c = Point::new(40.0, -111.0);
        let s = Shape::Circle { center: c, radius_m: 500.0 };
        assert_eq!(s.distance_m(crate::geo::destination(c, 90.0, 300.0)), 0.0);
        let d = s.distance_m(crate::geo::destination(c, 90.0, 800.0));
        assert!((d - 300.0).abs() < 2.0, "got {d}");
    }

    #[test]
    fn distance_to_a_polygon_is_zero_inside_and_measured_to_the_nearest_edge_outside() {
        let a = Point::new(40.0, -111.0);
        let b = crate::geo::destination(a, 90.0, 1000.0);
        let c = crate::geo::destination(b, 0.0, 1000.0);
        let d = crate::geo::destination(a, 0.0, 1000.0);
        let s = Shape::Polygon { vertices: vec![a, b, c, d] };
        let inside = crate::geo::destination(crate::geo::destination(a, 90.0, 500.0), 0.0, 500.0);
        assert_eq!(s.distance_m(inside), 0.0);
        let east = crate::geo::destination(crate::geo::destination(a, 90.0, 1300.0), 0.0, 500.0);
        assert!((s.distance_m(east) - 300.0).abs() < 3.0);
        let corner = crate::geo::destination(crate::geo::destination(b, 90.0, 300.0), 180.0, 400.0);
        assert!((s.distance_m(corner) - 300.0).abs() < 3.0, "nearest edge is the east side");
    }
```
Use `use super::*;` as the module already does; if `Point` is not imported there add `use crate::geo::Point;` inside the test module.

- [ ] **Step 2: Run, see it fail to compile** (`no method named distance_m`): `cd core && cargo test -q realm 2>&1 | tail -8`.

- [ ] **Step 3: Implement.** In `core/src/geo.rs` add:

```rust
/// Shortest distance from `p` to the segment `a`-`b`, in metres (flat approximation around `p`, fine at city scale).
pub fn distance_to_segment_m(p: Point, a: Point, b: Point) -> f64 {
    let k = 111_195.0;
    let cos_lat = p.lat.to_radians().cos();
    let xy = |q: Point| ((q.lon - p.lon) * k * cos_lat, (q.lat - p.lat) * k);
    let ((ax, ay), (bx, by)) = (xy(a), xy(b));
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0) };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    (cx * cx + cy * cy).sqrt()
}
```
In `core/src/realm.rs`, inside `impl Shape` (next to `farthest_m`):

```rust
    /// How far outside the shape `p` is, in metres; 0 when it is inside.
    pub fn distance_m(&self, p: Point) -> f64 {
        match self {
            Shape::Circle { center, radius_m } => (crate::geo::distance_m(p, *center) - radius_m).max(0.0),
            Shape::Polygon { vertices } => {
                if crate::geo::point_in_polygon(p, vertices) || vertices.len() < 2 {
                    return 0.0;
                }
                (0..vertices.len()).map(|i| crate::geo::distance_to_segment_m(p, vertices[i], vertices[(i + 1) % vertices.len()])).fold(f64::INFINITY, f64::min)
            }
        }
    }
```

- [ ] **Step 4: Run, see green.** `cd core && cargo test -q realm 2>&1 | tail -4` (expect `ok`).
- [ ] **Step 5: Lint and commit** (`cargo fmt && cargo clippy ...` as in Task 1; message `feat: measure distance to a zone shape`).

---

### Task 3: Away settings, counters and per-zone distances in the game

**Files:**
- Modify: `core/src/game.rs` (types, `Game` fields, `NewGame.away`, `Game::create`, the two `NewGame { ... }` literals in the tests module at about lines 720 and 850), `core/ffi/src/engine.rs` (the two `NewGame { ... }` literals; pass `away: AwayOptions::default()` for now)

**Interfaces:**
- Produces: `AwayOptions {zone_only: bool, custom_m: Option<f64>}` with `Default` (zone_only true) and `resolve(&self, farthest_m: f64) -> f64`; `AwayConfig {zone_only: bool, distance_m: BTreeMap<u32, f64>}` with `distance_for(zone) -> f64`; `Counters {progress: BTreeMap<String, f64>, steps_last: Option<i64>}`; `Game.away: AwayConfig`, `Game.counters: Counters` (both `#[serde(default)]`); `NewGame.away: AwayOptions`; constants `DEFAULT_AWAY_M = 1000.0`.

- [ ] **Step 1: Write the failing tests** in the `tests` module of `core/src/game.rs`:

```rust
    #[test]
    fn automatic_away_distance_is_40_percent_of_the_realm_reach_within_limits() {
        let auto = AwayOptions { zone_only: true, custom_m: None };
        assert_eq!(auto.resolve(1000.0), 400.0);
        assert_eq!(auto.resolve(100.0), 300.0, "never below 300 m");
        assert_eq!(auto.resolve(50_000.0), 3000.0, "never above 3 km");
    }

    #[test]
    fn a_custom_away_distance_is_used_but_kept_sane() {
        let custom = |m| AwayOptions { zone_only: false, custom_m: Some(m) };
        assert_eq!(custom(1500.0).resolve(1000.0), 1500.0);
        assert_eq!(custom(5.0).resolve(1000.0), 100.0);
        assert_eq!(custom(1e9).resolve(1000.0), 20_000.0);
    }

    #[test]
    fn a_new_game_gets_a_distance_for_every_zone_and_empty_counters() {
        let g = game(&reach_only(&[Mode::Walk, Mode::Bike], 20, "all_trips"), Backend::Solo, 4);
        assert!(g.away.zone_only);
        assert_eq!(g.away.distance_m.len(), g.slot.zones.len());
        assert!(g.away.distance_m.values().all(|d| (300.0..=3000.0).contains(d)), "{:?}", g.away);
        assert!(g.counters.progress.is_empty() && g.counters.steps_last.is_none());
    }

    #[test]
    fn a_saved_game_from_before_chains_loads_with_defaults() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let mut v = serde_json::to_value(&g).unwrap();
        v.as_object_mut().unwrap().remove("away");
        v.as_object_mut().unwrap().remove("counters");
        let back: Game = serde_json::from_value(v).unwrap();
        assert_eq!(back.away, AwayConfig::default());
        assert!(back.counters.progress.is_empty());
        assert_eq!(back.away.distance_for(1), DEFAULT_AWAY_M);
    }
```

- [ ] **Step 2: Run and see it fail to compile.** `cd core && cargo test -q game 2>&1 | tail -8` (unresolved `AwayOptions`).

- [ ] **Step 3: Implement.** In `core/src/game.rs` near `Stats` add:

```rust
/// Distance from home for time-away chains when nothing better is known (old saves).
pub const DEFAULT_AWAY_M: f64 = 1000.0;
const AUTO_AWAY_SHARE: f64 = 0.4;
const AUTO_AWAY_MIN_M: f64 = 300.0;
const AUTO_AWAY_MAX_M: f64 = 3000.0;
const CUSTOM_AWAY_MIN_M: f64 = 100.0;
const CUSTOM_AWAY_MAX_M: f64 = 20_000.0;

/// What the player chose in New Game for time-away quests.
#[derive(Debug, Clone, PartialEq)]
pub struct AwayOptions {
    /// Count time away only inside a zone's area (false: anywhere).
    pub zone_only: bool,
    /// A fixed distance in metres; `None` picks one from the realm's size.
    pub custom_m: Option<f64>,
}

impl Default for AwayOptions {
    fn default() -> Self {
        AwayOptions { zone_only: true, custom_m: None }
    }
}

impl AwayOptions {
    /// The away distance for a zone whose realm reaches `farthest_m` from home.
    pub fn resolve(&self, farthest_m: f64) -> f64 {
        match self.custom_m {
            Some(m) => m.clamp(CUSTOM_AWAY_MIN_M, CUSTOM_AWAY_MAX_M),
            None => (farthest_m * AUTO_AWAY_SHARE).clamp(AUTO_AWAY_MIN_M, AUTO_AWAY_MAX_M),
        }
    }
}

/// The saved form of [`AwayOptions`]: the distance is resolved per zone when the game is created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AwayConfig {
    pub zone_only: bool,
    pub distance_m: BTreeMap<u32, f64>,
}

impl Default for AwayConfig {
    fn default() -> Self {
        AwayConfig { zone_only: true, distance_m: BTreeMap::new() }
    }
}

impl AwayConfig {
    pub fn distance_for(&self, zone: u32) -> f64 {
        self.distance_m.get(&zone).copied().unwrap_or(DEFAULT_AWAY_M)
    }
}

/// Saved progress of the progressive quests.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Counters {
    /// Chain id -> steps credited or minutes away (map squares come from `Fog::cells`).
    pub progress: BTreeMap<String, f64>,
    /// The last step-counter reading seen this session (reset when a game is opened).
    pub steps_last: Option<i64>,
}
```
Add to `struct Game` (after `avoid_stairs`): `#[serde(default)] pub away: AwayConfig,` and `#[serde(default)] pub counters: Counters,`. Add to `struct NewGame`: `pub away: AwayOptions,`. In `Game::create`, before `Ok(Game { ... })` compute and then set fields:

```rust
        let away = AwayConfig { zone_only: n.away.zone_only, distance_m: zones.iter().map(|z| (z.zone, n.away.resolve(z.realm.shape.farthest_m(n.home)))).collect() };
```
and in the struct literal: `away, counters: Counters::default(),`. Add `away: AwayOptions::default(),` to every `NewGame { ... }` literal (in `core/src/game.rs` tests and both in `core/ffi/src/engine.rs`).

- [ ] **Step 4: Run, see green.** `cd core && cargo test -q 2>&1 | grep -E "^test result|FAILED"` and `cargo clippy --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** (`feat: save away settings and chain counters`).

---

### Task 4: Step counting and milestone completion

**Files:**
- Modify: `core/src/game.rs`

**Interfaces:**
- Consumes: Task 1 (`chain::derive`, `is_chain_target`, `ChainUnit`, `Chain`), Task 3 (`counters`).
- Produces: `Game::chains(&self) -> Vec<Chain>`; `Game::on_steps(&mut self, total: i64, t_ms: i64) -> Vec<Event>`; `on_fix` credits steps and completes reached marks; `Game::load` resets `counters.steps_last`; per-location trackers skip chain targets.

- [ ] **Step 1: Write the failing tests** in the `tests` module of `core/src/game.rs`. First a shared helper:

```rust
    /// A solo game whose quests are replaced by the given targets (all in zone 1, kind `kind`), each rewarding "Hydrate!".
    fn chain_game(kind: &str, targets: Vec<Target>) -> Game {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        g.assignments = targets
            .into_iter()
            .enumerate()
            .map(|(i, t)| crate::chain::tests_support::member(1000 + i as i64, 1, kind, t))
            .collect();
        g.done.clear();
        g.solo_rewards = g.assignments.iter().map(|a| (a.location_id, "Hydrate!".to_string())).collect();
        g
    }

    fn done_ids(ev: &[Event]) -> Vec<i64> {
        ev.iter().filter_map(|e| if let Event::QuestDone { location_id, .. } = e { Some(*location_id) } else { None }).collect()
    }
```
This requires exposing the Task 1 helper: in `core/src/chain.rs` add (outside `tests`) `#[cfg(test)] pub(crate) mod tests_support { pub(crate) use super::tests::member; }` and make `tests::member` `pub(crate)` (it already is). Then the tests:

```rust
    #[test]
    fn steps_count_from_the_first_reading_and_unlock_marks_in_order() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]); // marks at 500 and 1500
        assert!(g.on_steps(10_000, 1).is_empty(), "the first reading only sets the baseline");
        assert!(g.on_steps(10_400, 2).is_empty());
        assert_eq!(done_ids(&g.on_steps(10_520, 3)), vec![1000]);
        assert_eq!(done_ids(&g.on_steps(11_600, 4)), vec![1001]);
        assert!(g.on_steps(20_000, 5).is_empty(), "nothing is paid twice");
        assert_eq!(g.counters.progress["1:step_up"], 10_000.0);
    }

    #[test]
    fn a_reboot_that_resets_the_counter_credits_the_new_reading_and_never_goes_negative() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 100_000 }]);
        g.on_steps(50_000, 1);
        g.on_steps(50_200, 2);
        g.on_steps(300, 3); // rebooted: the counter started again from zero
        assert_eq!(g.counters.progress["1:step_up"], 500.0);
        assert_eq!(g.counters.steps_last, Some(300));
    }

    #[test]
    fn steps_taken_while_the_game_is_closed_are_not_credited() {
        let dir = std::env::temp_dir().join(format!("apgo-steps-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = chain_game("step_up", vec![Target::Steps { n: 100_000 }]);
        g.on_steps(1_000, 1);
        g.on_steps(1_300, 2);
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert_eq!(back.counters.progress["1:step_up"], 300.0, "progress is kept");
        assert_eq!(back.counters.steps_last, None, "the session baseline is not");
        back.on_steps(90_000, 3); // a whole day of walking with the game closed
        assert_eq!(back.counters.progress["1:step_up"], 300.0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_fix_with_a_step_reading_also_credits_steps() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_fix(Fix { accuracy_m: 5.0, ..fixat(home(), 1) }, Some(5_000));
        let ev = g.on_fix(Fix { accuracy_m: 5.0, ..fixat(home(), 20) }, Some(5_600));
        assert_eq!(done_ids(&ev), vec![1000]);
    }

    #[test]
    fn marks_wait_while_a_trap_blocks_checks_and_pay_when_it_ends() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_fix(Fix { accuracy_m: 5.0, ..fixat(home(), 1) }, Some(1_000));
        g.traps.trigger("Freeze Trap", 0, Some(home()), home(), &g.trap_pool.clone(), &mut rand::SeedableRng::seed_from_u64(1));
        assert!(g.on_steps(1_600, 2).is_empty(), "frozen: no check counts");
        g.traps.active.clear();
        assert_eq!(done_ids(&g.on_steps(1_601, 3)), vec![1000], "the counter kept the steps");
    }
```
(The last test may need `use rand::SeedableRng;`; `Traps::active` is `pub` in `traps.rs` — if not, expose it `pub(crate)`.)

- [ ] **Step 2: Run and see failures** (`no method named on_steps`): `cd core && cargo test -q game 2>&1 | tail -10`.

- [ ] **Step 3: Implement** in `core/src/game.rs`. Imports: `use crate::chain::{self, is_chain_target, Chain, ChainUnit};`. Add to `impl Game`:

```rust
    pub fn chains(&self) -> Vec<Chain> {
        chain::derive(&self.assignments)
    }

    fn counter_of(&self, c: &Chain) -> f64 {
        match c.unit {
            ChainUnit::Cells => self.fog.cells.len() as f64,
            _ => self.counters.progress.get(&c.id).copied().unwrap_or(0.0),
        }
    }

    /// Credit the steps since the last reading of the phone's cumulative step counter. The first reading of a session only sets the baseline;
    /// a reading below the last one means the phone restarted its counter.
    fn credit_steps(&mut self, total: i64) {
        let gained = match self.counters.steps_last {
            None => 0,
            Some(last) if total >= last => total - last,
            Some(_) => total,
        };
        self.counters.steps_last = Some(total);
        if gained == 0 {
            return;
        }
        for c in self.chains().into_iter().filter(|c| c.unit == ChainUnit::Steps) {
            *self.counters.progress.entry(c.id).or_insert(0.0) += gained as f64;
        }
    }

    /// A step-counter reading outside a fix (the sensor reports on its own).
    pub fn on_steps(&mut self, total: i64, t_ms: i64) -> Vec<Event> {
        self.credit_steps(total);
        self.complete_reached(t_ms, self.last_pos())
    }

    /// Complete every chain member whose mark the counter has passed (in unlocked zones, and not while a trap blocks checks).
    fn complete_reached(&mut self, t_ms: i64, pos: Option<Point>) -> Vec<Event> {
        if pos.is_some_and(|p| self.traps.blocks_checks(p).is_some()) {
            return Vec::new();
        }
        let mut ev = Vec::new();
        for c in self.chains() {
            if !self.zone_unlocked(c.zone) {
                continue;
            }
            let counter = self.counter_of(&c);
            for id in c.reached(counter) {
                if !self.done.contains(&id) {
                    ev.extend(self.complete(id, t_ms, pos));
                }
            }
        }
        ev
    }
```
In `Game::on_fix`: (a) first line after `let mut ev = Vec::new();` add `if let Some(total) = steps_total { self.credit_steps(total); }`; (b) before `let mut finished = Vec::new();` add `let in_chain: BTreeSet<i64> = self.assignments.iter().filter(|a| is_chain_target(&a.target)).map(|a| a.location_id).collect();` and inside the per-quest loop, right after the `done`/zone check, add `if in_chain.contains(&id) { continue; }`; (c) after `for id in finished { ... }` add `ev.extend(self.complete_reached(fix.t_ms, Some(pos)));`. In `Game::load` replace the final line with:

```rust
        let mut g: Game = serde_json::from_str(&s).map_err(|e| format!("corrupt game file: {e}"))?;
        g.counters.steps_last = None; // steps taken while the game was closed are never credited
        Ok(g)
```
Make `Traps::active` `pub(crate)` in `core/src/traps.rs` if the compiler requires it.

- [ ] **Step 4: Run, see green.** `cd core && cargo test -q 2>&1 | grep -E "^test result|FAILED|panicked"`.
- [ ] **Step 5: Lint and commit** (`feat: count steps into chain milestones`).

---

### Task 5: Time away and map squares

**Files:**
- Modify: `core/src/game.rs`

**Interfaces:**
- Produces: `Game::set_in_zone(&mut self, bool)`; away minutes accrue in `on_fix`; Cartographer chain completes from `fog.cells`.

- [ ] **Step 1: Write the failing tests** (game.rs tests module; reuse `chain_game`, `done_ids`):

```rust
    fn away_game(minutes: &[f64], zone_only: bool, distance_m: f64) -> Game {
        let mut g = chain_game("wanderlust", minutes.iter().map(|m| Target::Away { min_distance_m: 1.0, minutes: *m }).collect());
        g.away = AwayConfig { zone_only, distance_m: [(1, distance_m)].into() };
        g
    }

    /// Fixes every 60 s at `dist` metres east of home, `n` of them.
    fn away_for(g: &mut Game, dist: f64, from_s: i64, n: i64) -> Vec<Event> {
        let p = destination(g.home, 90.0, dist);
        (0..n).flat_map(|i| g.on_fix(Fix { accuracy_m: 5.0, ..fixat(p, from_s + i * 60) }, None)).collect()
    }

    #[test]
    fn minutes_away_accrue_only_beyond_the_distance_and_unlock_marks() {
        let mut g = away_game(&[3.0, 2.0], false, 1000.0); // marks at 2 and 5 minutes
        away_for(&mut g, 400.0, 0, 10);
        assert!(g.counters.progress.get("1:wanderlust").is_none_or(|m| *m == 0.0), "400 m is not away");
        let ev = away_for(&mut g, 1500.0, 10_000, 4); // 3 intervals = 3 minutes
        assert_eq!(done_ids(&ev), vec![1001], "the 2-minute member unlocks first");
        assert!((g.counters.progress["1:wanderlust"] - 3.0).abs() < 0.01);
        assert_eq!(done_ids(&away_for(&mut g, 1500.0, 20_000, 4)), vec![1000]);
    }

    #[test]
    fn a_gap_over_five_minutes_does_not_count_as_time_away() {
        let mut g = away_game(&[10.0], false, 1000.0);
        away_for(&mut g, 1500.0, 0, 2); // 1 minute
        away_for(&mut g, 1500.0, 3600, 1); // an hour later: the interval is not counted
        assert!((g.counters.progress["1:wanderlust"] - 1.0).abs() < 0.01);
    }

    #[test]
    fn inside_a_zone_mode_needs_the_engine_to_say_the_player_is_inside() {
        let mut g = away_game(&[10.0], true, 1000.0);
        g.set_in_zone(false);
        away_for(&mut g, 1500.0, 0, 5);
        assert!(g.counters.progress.get("1:wanderlust").is_none_or(|m| *m == 0.0));
        g.set_in_zone(true);
        away_for(&mut g, 1500.0, 1000, 5);
        assert!(g.counters.progress["1:wanderlust"] > 3.0);
    }

    #[test]
    fn new_map_squares_unlock_marks_from_the_cells_the_game_has_seen() {
        let mut g = chain_game("cartographer", vec![Target::Cells { n: 3, cell_m: 150.0 }, Target::Cells { n: 2, cell_m: 150.0 }]); // marks at 2 and 5
        let mut done = Vec::new();
        for i in 0..8 {
            let p = destination(g.home, 90.0, 200.0 * f64::from(i)); // a new 150 m cell every fix
            done.extend(done_ids(&g.on_fix(Fix { accuracy_m: 5.0, ..fixat(p, 1000 + i64::from(i) * 150) }, None)));
        }
        assert_eq!(done, vec![1001, 1000], "two cells unlock the 2-cell member, five the next");
    }
```

- [ ] **Step 2: Run, see failures** (`set_in_zone` missing / no completion): `cd core && cargo test -q game 2>&1 | tail -8`.

- [ ] **Step 3: Implement.** Add fields to `Game`: `#[serde(skip_serializing, default = "yes")] in_zone: bool,` and a free function `fn yes() -> bool { true }`; set `in_zone: true` in `Game::create`. Add:

```rust
    /// The engine tells the game whether the player is inside the area of one of its zones.
    pub fn set_in_zone(&mut self, inside: bool) {
        self.in_zone = inside;
    }

    /// Add the time between two accepted fixes to every time-away chain whose rules the player meets.
    fn accrue_away(&mut self, prev: &Fix, fix: &Fix) {
        let dt = fix.t_ms - prev.t_ms;
        if dt <= 0 || dt > AWAY_MAX_GAP_MS || (self.away.zone_only && !self.in_zone) {
            return;
        }
        for c in self.chains().into_iter().filter(|c| c.unit == ChainUnit::Minutes) {
            let d = self.away.distance_for(c.zone);
            if distance_m(prev.point(), self.home) >= d && distance_m(fix.point(), self.home) >= d {
                *self.counters.progress.entry(c.id).or_insert(0.0) += dt as f64 / 60_000.0;
            }
        }
    }
```
with `const AWAY_MAX_GAP_MS: i64 = 5 * 60_000;`. In `on_fix`, just before `self.last_fix = Some(fix);` (and before the `complete_reached` call added in Task 4, so move that call to after this) add `if let Some(prev) = self.last_fix { self.accrue_away(&prev, &fix); }`. Order at the end of `on_fix` must be: finished loop, `accrue_away`, `complete_reached`, `self.last_fix = Some(fix)`.

- [ ] **Step 4: Run, see green.** `cd core && cargo test -q 2>&1 | grep -E "^test result|FAILED|panicked"`.
- [ ] **Step 5: Lint and commit** (`feat: count time away and new squares in chains`).

---

### Task 6: Chain views, quest progress and milestone attribution

**Files:**
- Modify: `core/src/game.rs`

**Interfaces:**
- Consumes: Tasks 1, 4, 5.
- Produces: `MarkView {at, location_id, reached, reward: Option<String>}`; `ChainView {id, zone, kind_id, name, family, unit: ChainUnit, counter, total, rule, marks: Vec<MarkView>}`; `Game::chain_views(&self) -> Vec<ChainView>`; `QuestView.chain_id: Option<String>`; chain members' `QuestView.progress` = progress toward their mark; QuestDone journal detail for members: `"Step Up milestone 3 of 5: 8,500 steps"`.

- [ ] **Step 1: Write the failing tests:**

```rust
    #[test]
    fn chain_views_report_counter_marks_rewards_and_the_rule() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1);
        g.on_steps(600, 2); // first mark reached
        let v = &g.chain_views()[0];
        assert_eq!((v.id.as_str(), v.total, v.counter), ("1:step_up", 1500.0, 600.0));
        assert_eq!(v.rule, "Take 1,500 steps");
        assert_eq!(v.marks.iter().map(|m| (m.at, m.reached, m.reward.is_some())).collect::<Vec<_>>(), vec![(500.0, true, true), (1500.0, false, false)]);
    }

    #[test]
    fn chain_members_carry_their_chain_id_and_their_own_progress() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1);
        g.on_steps(1000, 2); // mark 1 done, 500 of the next 1000
        let views = g.quest_views();
        assert!(views.iter().all(|q| q.chain_id.as_deref() == Some("1:step_up")));
        let (a, b) = (&views[0], &views[1]);
        assert_eq!((a.state, a.progress), (QuestState::Done, 1.0));
        assert!((b.progress - 0.5).abs() < 0.01 && b.state == QuestState::InProgress, "{} {:?}", b.progress, b.state);
    }

    #[test]
    fn other_quests_have_no_chain_id() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        assert!(g.quest_views().iter().all(|q| q.chain_id.is_none()));
    }

    #[test]
    fn a_milestone_is_logged_with_its_place_in_the_chain() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1);
        let ev = g.on_steps(1600, 2);
        let log = g.journal_events(&ev, 2000, None);
        let done: Vec<&str> = log.iter().filter(|e| e.kind == "quest_done").map(|e| e.detail.as_str()).collect();
        assert_eq!(done, vec!["step up milestone 1 of 2: 500 steps", "step up milestone 2 of 2: 1,500 steps"]);
    }
```
(`chain_game` names quests by `kind.replace('_', " ")` through the Task 1 helper, hence the lower-case names.)

- [ ] **Step 2: Run, see failures.** `cd core && cargo test -q game 2>&1 | tail -8`.

- [ ] **Step 3: Implement.** Add types near `QuestView`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct MarkView {
    pub at: f64,
    pub location_id: i64,
    pub reached: bool,
    /// Solo only: what the milestone gave you, once reached.
    pub reward: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChainView {
    pub id: String,
    pub zone: u32,
    pub kind_id: String,
    pub name: String,
    pub family: String,
    pub unit: ChainUnit,
    pub counter: f64,
    pub total: f64,
    pub rule: String,
    pub marks: Vec<MarkView>,
}
```
Add `pub chain_id: Option<String>,` to `QuestView`. In `impl Game`:

```rust
    pub fn chain_views(&self) -> Vec<ChainView> {
        self.chains()
            .into_iter()
            .map(|c| {
                let counter = self.counter_of(&c);
                let family = self.assignments.iter().find(|a| c.marks.first().is_some_and(|m| m.location_id == a.location_id)).map(|a| a.family.clone()).unwrap_or_default();
                let marks = c
                    .marks
                    .iter()
                    .map(|m| {
                        let reached = self.done.contains(&m.location_id);
                        MarkView { at: m.at, location_id: m.location_id, reached, reward: if reached { self.solo_rewards.get(&m.location_id).cloned() } else { None } }
                    })
                    .collect();
                ChainView { rule: c.rule_text(self.away.distance_for(c.zone)), total: c.total(), id: c.id, zone: c.zone, kind_id: c.kind_id, name: c.name, family, unit: c.unit, counter, marks }
            })
            .collect()
    }

    /// Progress of a chain member toward its own mark (0..1): done = 1, the next unreached mark = how far through its stretch, later ones 0.
    fn member_progress(&self, chains: &[Chain], location_id: i64) -> Option<(String, f32)> {
        let c = chains.iter().find(|c| c.position_of(location_id).is_some())?;
        let i = c.position_of(location_id)? - 1;
        let counter = self.counter_of(c);
        let at = c.marks[i].at;
        let prev = if i == 0 { 0.0 } else { c.marks[i - 1].at };
        let p = if counter >= at { 1.0 } else if counter <= prev { 0.0 } else { (counter - prev) / (at - prev) };
        Some((c.id.clone(), p as f32))
    }
```
In `quest_views`, compute `let chains = self.chains();` once before the map; for each assignment: `let member = self.member_progress(&chains, a.location_id);` then `progress` = `member.as_ref().map_or(<existing tracker progress>, |(_, p)| *p)` and `chain_id: member.map(|(id, _)| id)`. In `journal_events`, in the `Event::QuestDone` arm, before the existing detail, add:

```rust
                        if let Some(c) = chains.iter().find(|c| c.position_of(*location_id).is_some()) {
                            let i = c.position_of(*location_id).unwrap_or(1);
                            j.detail = format!("{name} milestone {i} of {}: {}", c.marks.len(), c.amount_text(c.marks[i - 1].at));
                        } else if let Some(a) = quest(location_id) { ...existing... }
```
with `let chains = self.chains();` at the top of `journal_events`.

- [ ] **Step 4: Run, see green; also fix any existing test that constructs `QuestView`** (add `chain_id: None`). `cd core && cargo test -q 2>&1 | grep -E "^test result|FAILED|panicked|^error"`.
- [ ] **Step 5: Lint and commit** (`feat: show chains and attribute milestones`).

---

### Task 7: Reroll guard and old-save counters

**Files:**
- Modify: `core/src/game.rs`

- [ ] **Step 1: Write the failing tests:**

```rust
    #[test]
    fn chain_members_cannot_be_rerolled_and_counters_survive_a_reroll() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1);
        g.on_steps(400, 2);
        let before = g.assignments.iter().map(|a| a.target.clone()).collect::<Vec<_>>();
        let realms = vec![realm("r0", Mode::Walk)];
        let n = g.reroll(&[1000, 1001], &realms, 9, &Catalog::builtin()).unwrap();
        assert_eq!(n, 0, "nothing re-placed");
        assert_eq!(g.assignments.iter().map(|a| a.target.clone()).collect::<Vec<_>>(), before);
        assert_eq!(g.counters.progress["1:step_up"], 400.0);
    }

    #[test]
    fn an_old_save_with_finished_chain_quests_keeps_them_and_earns_nothing_twice() {
        let dir = std::env::temp_dir().join(format!("apgo-oldsave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }, Target::Steps { n: 2000 }]);
        g.done.insert(1000); // finished the old way, before chains existed
        g.done.insert(1001);
        g.counters = Counters::default();
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert!(back.done.contains(&1000) && back.done.contains(&1001), "nothing lost");
        assert_eq!(back.counters.progress["1:step_up"], 1500.0, "the counter starts at the highest finished mark");
        back.on_steps(10, 1);
        let ev = back.on_steps(600, 2);
        assert!(done_ids(&ev).is_empty(), "the 3rd mark is at 3,500; 590 more steps do not pay anything twice");
        let _ = std::fs::remove_dir_all(&dir);
    }
```
`realm("r0", Mode::Walk)` is the existing helper in the tests module (it returns `(Realm, Atlas)`).

- [ ] **Step 2: Run, see failures.**
- [ ] **Step 3: Implement.** In `Game::reroll`, change the `todo` filter to also exclude chain members: `.filter(|i| !self.done.contains(i) && !self.assignments.iter().any(|a| a.location_id == *i && is_chain_target(&a.target)))`. In `Game::load`, after resetting `steps_last`, call `g.normalize_counters();` with:

```rust
    /// An old save has finished chain members but no counters: start each counter at its highest finished mark so nothing is lost or earned twice.
    fn normalize_counters(&mut self) {
        for c in self.chains() {
            if c.unit == ChainUnit::Cells {
                continue;
            }
            let floor = c.marks.iter().filter(|m| self.done.contains(&m.location_id)).map(|m| m.at).fold(0.0, f64::max);
            let entry = self.counters.progress.entry(c.id).or_insert(0.0);
            if *entry < floor {
                *entry = floor;
            }
        }
    }
```
- [ ] **Step 4: Run, see green** (`cargo test -q`).
- [ ] **Step 5: Lint and commit** (`feat: guard chain rerolls and old saves`).

---

### Task 8: FFI: chains, steps, zone shapes and new-game options

**Files:**
- Modify: `core/ffi/src/engine.rs`

**Interfaces:**
- Consumes: Tasks 2-7.
- Produces (UniFFI/Kotlin): records `MarkOut {at: f64, location_id: i64, reached: bool, reward: Option<String>}` and `ChainOut {id, zone: u32, kind_id, name, family, unit: String, counter: f64, total: f64, rule, marks: Vec<MarkOut>}`; `QuestOut.chain_id: Option<String>`; `Engine.chains() -> Vec<ChainOut>`; `Engine.on_steps(total: i64, t_ms: i64) -> Vec<EventOut>`; `start_solo(..., avoid_stairs: bool, away_zone_only: bool, away_distance_m: u32)` and `start_archipelago(..., avoid_stairs: bool, away_zone_only: bool, away_distance_m: u32)` (`0` = automatic); `Engine.on_fix` now tells the game whether the player is inside a zone.

- [ ] **Step 1: Add the records and the QuestOut field.**

```rust
#[derive(Debug, uniffi::Record)]
pub struct MarkOut {
    pub at: f64,
    pub location_id: i64,
    pub reached: bool,
    pub reward: Option<String>,
}

#[derive(Debug, uniffi::Record)]
pub struct ChainOut {
    pub id: String,
    pub zone: u32,
    pub kind_id: String,
    pub name: String,
    pub family: String,
    /// steps | minutes | cells
    pub unit: String,
    pub counter: f64,
    pub total: f64,
    pub rule: String,
    pub marks: Vec<MarkOut>,
}
```
Add `pub chain_id: Option<String>,` to `QuestOut` and `chain_id: q.chain_id,` in the `QuestOut { ... }` construction inside `quests()`.

- [ ] **Step 2: Zone shapes and `install`.** Add to `Engine`: `zone_shapes: Mutex<Vec<Shape>>,` (initialise `Mutex::new(Vec::new())` in `Engine::new`). Add a private helper in `impl Engine`:

```rust
    /// Make `game` the open game and remember the shapes of its zones' realms (for "inside a zone" checks).
    fn install(&self, game: Game) {
        let store = self.store();
        let shapes = game.zone_realms.iter().filter_map(|id| store.get(id)).map(|r| r.shape).collect();
        *self.zone_shapes.lock().unwrap_or_else(|e| e.into_inner()) = shapes;
        *self.game.lock().unwrap_or_else(|e| e.into_inner()) = Some(game);
    }

    /// Distance in metres from a point to the nearest zone area of the open game (0 inside), or `None` with no game.
    fn zone_distance_m(&self, p: Point) -> Option<f64> {
        let shapes = self.zone_shapes.lock().unwrap_or_else(|e| e.into_inner());
        shapes.iter().map(|s| s.distance_m(p)).reduce(f64::min)
    }
```
Replace the three `*self.game.lock()... = Some(...)` assignments (in `start_solo`, `start_archipelago`, `open_game`) with `self.install(game)` / `self.install(g)`. In `on_fix`, before the `with_game` call: `let inside = self.zone_distance_m(Point::new(lat, lon)).is_none_or(|d| d == 0.0);` and inside the closure before `g.on_fix`: `g.set_in_zone(inside);`.

- [ ] **Step 3: New-game options.** Add `away_zone_only: bool, away_distance_m: u32` after `avoid_stairs` in both `start_solo` and `start_archipelago`, and in their `NewGame { ... }` literals replace `away: AwayOptions::default()` with `away: AwayOptions { zone_only: away_zone_only, custom_m: (away_distance_m > 0).then_some(f64::from(away_distance_m)) }`. Import `AwayOptions` from `apgo_core::game`.

- [ ] **Step 4: The two new calls.**

```rust
    /// The progressive quests of the open game, one entry per bar.
    pub fn chains(&self) -> Vec<ChainOut> {
        self.with_game(|g| {
            g.chain_views()
                .into_iter()
                .map(|c| ChainOut {
                    id: c.id,
                    zone: c.zone,
                    kind_id: c.kind_id,
                    name: c.name,
                    family: c.family,
                    unit: match c.unit {
                        ChainUnit::Steps => "steps",
                        ChainUnit::Minutes => "minutes",
                        ChainUnit::Cells => "cells",
                    }
                    .into(),
                    counter: c.counter,
                    total: c.total,
                    rule: c.rule,
                    marks: c.marks.into_iter().map(|m| MarkOut { at: m.at, location_id: m.location_id, reached: m.reached, reward: m.reward }).collect(),
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// A step-counter reading from the phone (cumulative since boot). Only counts while a game is open.
    pub fn on_steps(&self, total: i64, t_ms: i64) -> Vec<EventOut> {
        let dir = self.dir.clone();
        let Some((game_id, ev, entries)) = self.with_game(|g| {
            let ev = g.on_steps(total, t_ms);
            if !ev.is_empty() {
                let _ = g.save(&dir);
            }
            let entries = g.journal_events(&ev, t_ms, None);
            (g.id.clone(), ev, entries)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&game_id, e)));
        ev.into_iter().map(ev_out).collect()
    }
```
Import `apgo_core::chain::ChainUnit`.

- [ ] **Step 5: Build, lint, rebuild bindings, commit.**
```bash
cd core && cargo fmt && cargo clippy --all-targets -- -D warnings 2>&1 | grep -E "^(warning|error)" -A6 | head; cargo test -q 2>&1 | grep -E "^test result|FAILED"
cd .. && APGO_ABIS="arm64-v8a x86_64" scripts/android_core.sh debug 2>&1 | tail -1
git add core && git commit -m "feat: expose chains and steps over FFI" -m "Chains, on_steps, away options on game start, zone shapes for the inside-a-zone check, chain_id on quests." -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```
(Kotlin will not compile until Task 11 passes the new `startSolo`/`startArchipelago` arguments; commit the Kotlin side right after in Tasks 9-11 and do not run Gradle in between.)

---

### Task 9: Kotlin chain text and tick maths (pure)

**Files:**
- Create: `android/app/src/main/java/dev/apgo2/ui/ChainFormat.kt`, `android/app/src/test/java/dev/apgo2/ui/ChainFormatTest.kt`
- Modify: `android/app/src/main/java/dev/apgo2/AppModel.kt` (only the call sites needed to compile, see Task 11 step 3)

**Interfaces:**
- Consumes: generated `uniffi.apgo_ffi.ChainOut`, `MarkOut`.
- Produces: `ChainFormat.thousands(Long)`, `ChainFormat.amount(unit: String, value: Double)`, `ChainFormat.next(c: ChainOut): String`, `ChainFormat.fractions(marks: List<Double>, total: Double): List<Float>`, `ChainFormat.fill(counter: Double, total: Double): Float`.

- [ ] **Step 1: Write the failing test** (`ChainFormatTest.kt`):

```kotlin
package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.apgo_ffi.ChainOut
import uniffi.apgo_ffi.MarkOut

class ChainFormatTest {
    private fun chain(unit: String, counter: Double, vararg marks: Pair<Double, Boolean>) = ChainOut(
        id = "1:x", zone = 1u, kindId = "x", name = "X", family = "steps", unit = unit, counter = counter, total = marks.last().first, rule = "r",
        marks = marks.mapIndexed { i, (at, reached) -> MarkOut(at = at, locationId = i.toLong(), reached = reached, reward = null) },
    )

    @Test fun amountsReadNaturallyPerUnit() {
        assertEquals("8,500 steps", ChainFormat.amount("steps", 8500.0))
        assertEquals("1 h 30 min", ChainFormat.amount("minutes", 90.0))
        assertEquals("45 min", ChainFormat.amount("minutes", 45.0))
        assertEquals("40 squares", ChainFormat.amount("cells", 40.0))
        assertEquals("1,234,567", ChainFormat.thousands(1_234_567))
    }

    @Test fun nextNamesTheFirstUnreachedMarkAndHowFarItIs() {
        val c = chain("steps", 3400.0, 500.0 to true, 3000.0 to true, 8500.0 to false, 16500.0 to false)
        assertEquals("next: 8,500 steps (5,100 to go)", ChainFormat.next(c))
    }

    @Test fun aFinishedChainSaysSo() {
        val c = chain("steps", 9000.0, 500.0 to true, 8500.0 to true)
        assertEquals("all 2 unlocked", ChainFormat.next(c))
    }

    @Test fun ticksSitAtTheirShareOfTheTotalAndStayInsideTheBar() {
        assertEquals(listOf(0.1f, 0.5f, 1.0f), ChainFormat.fractions(listOf(10.0, 50.0, 100.0), 100.0))
        assertEquals(listOf(1.0f), ChainFormat.fractions(listOf(250.0), 100.0))
        assertEquals(emptyList<Float>(), ChainFormat.fractions(emptyList(), 100.0))
        assertEquals("a zero total must not divide by zero", listOf(0f), ChainFormat.fractions(listOf(5.0), 0.0))
    }

    @Test fun theFillIsTheCounterShareClamped() {
        assertEquals(0.25f, ChainFormat.fill(25.0, 100.0))
        assertEquals(1f, ChainFormat.fill(500.0, 100.0))
        assertEquals(0f, ChainFormat.fill(-3.0, 100.0))
        assertEquals(0f, ChainFormat.fill(10.0, 0.0))
    }
}
```

- [ ] **Step 2: Run and see compile failure.** `cd android && ./gradlew :app:testDebugUnitTest -q 2>&1 | grep -E "^e:" | head -3` (Gradle will not compile the app until Task 11 fixes the call sites; do Task 11 step 3 first if the build breaks for that reason, then return).
- [ ] **Step 3: Implement** `ChainFormat.kt`:

```kotlin
package dev.apgo2.ui

import uniffi.apgo_ffi.ChainOut

/** Text and bar maths for a progressive quest (a chain). Pure, so it is unit-tested. */
object ChainFormat {
    fun thousands(n: Long): String = "%,d".format(java.util.Locale.US, n)

    /** The number with its unit word left off: "5,100", "1 h 30 min", "40". */
    private fun bare(unit: String, value: Double): String = when (unit) {
        "steps" -> thousands(value.toLong())
        "minutes" -> minutes(value)
        else -> value.toLong().toString()
    }

    /** "8,500 steps", "1 h 30 min", "40 squares". */
    fun amount(unit: String, value: Double): String = when (unit) {
        "steps" -> "${bare(unit, value)} steps"
        "minutes" -> bare(unit, value)
        else -> "${bare(unit, value)} squares"
    }

    private fun minutes(m: Double): String {
        val total = m.coerceAtLeast(0.0).toLong()
        val (h, r) = total / 60 to total % 60
        return when {
            h == 0L -> "$r min"
            r == 0L -> "$h h"
            else -> "$h h $r min"
        }
    }

    /** "next: 8,500 steps (5,100 to go)", or "all 4 unlocked" when every mark is reached. */
    fun next(c: ChainOut): String {
        val mark = c.marks.firstOrNull { !it.reached } ?: return "all ${c.marks.size} unlocked"
        val left = (mark.at - c.counter).coerceAtLeast(0.0)
        return "next: ${amount(c.unit, mark.at)} (${bare(c.unit, left)} to go)"
    }

    /** Each mark's position along the bar, 0..1. */
    fun fractions(marks: List<Double>, total: Double): List<Float> =
        marks.map { if (total <= 0.0) 0f else (it / total).toFloat().coerceIn(0f, 1f) }

    fun fill(counter: Double, total: Double): Float = if (total <= 0.0) 0f else (counter / total).toFloat().coerceIn(0f, 1f)
}
```

- [ ] **Step 4: Run, see green.**
- [ ] **Step 5: Commit** (`feat: add chain text and tick maths`).

---

### Task 10: The chain bar, Progress section and popup

**Files:**
- Create: `android/app/src/main/java/dev/apgo2/ui/ChainBar.kt`
- Modify: `PlayLayout.kt`, `PlayLayoutTest.kt` (chain members leave both lists), `AppModel.kt`, `Sensors.kt`, `Screens.kt`

**Interfaces:**
- Consumes: Task 9, Task 8 (`engine.chains()`, `engine.onSteps`).
- Produces: `ChainBar(counter, total, fractions, reached: List<Boolean>, modifier)` composable; `AppModel.chains: List<ChainOut>`, `AppModel.selectedChain: String?`, `AppModel.onSteps(total: Long)`; `PlayLayout.split` excludes quests with a `chainId`.

- [ ] **Step 1: Failing test.** In `PlayLayoutTest.kt` change the `q(...)` helper to take `chainId: String? = null` (add `chainId = chainId` to the `QuestOut(...)` constructor call) and add:

```kotlin
    @Test fun chainMembersAreInNeitherListBecauseTheChainRowShowsThem() {
        val s = PlayLayout.split(listOf(q(1, "steps", chainId = "1:step_up"), q(2, "away", "progress", 0.4f, chainId = "1:wanderlust"), q(3, "point"), q(4, "steps")))
        assertEquals(listOf(4L), ids(s.progress))
        assertEquals(listOf(3L), ids(s.places))
    }
```
Update the helper signature to `private fun q(id: Long, shape: String, state: String = "open", progress: Float = 0f, chainId: String? = null)`. Run: expect FAIL (members still listed). `cd android && ./gradlew :app:testDebugUnitTest -q`.

- [ ] **Step 2: Implement `PlayLayout.split`:** begin with `val quests = quests.filter { it.chainId == null }`; keep the rest unchanged. Run, see green.

- [ ] **Step 3: `ChainBar.kt`.**

```kotlin
package dev.apgo2.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp

/** One bar with a mark at each check it unlocks: filled up to [fill], reached marks solid green, the others hollow. */
@Composable
fun ChainBar(fill: Float, fractions: List<Float>, reached: List<Boolean>, modifier: Modifier = Modifier) {
    val track = ApgoPalette.mint
    val filled = ApgoPalette.teal
    val done = ApgoPalette.questDone
    Canvas(modifier.fillMaxWidth().height(18.dp)) {
        val h = 6.dp.toPx()
        val top = (size.height - h) / 2
        drawRoundRect(track, Offset(0f, top), Size(size.width, h), CornerRadius(h / 2))
        drawRoundRect(filled, Offset(0f, top), Size(size.width * fill, h), CornerRadius(h / 2))
        val r = 6.dp.toPx()
        fractions.forEachIndexed { i, f ->
            val x = (size.width * f).coerceIn(r, size.width - r)
            val c = Offset(x, size.height / 2)
            drawCircle(ApgoPalette.onMap, r, c)
            if (reached.getOrElse(i) { false }) drawCircle(done, r - 1.5.dp.toPx(), c) else drawCircle(ApgoPalette.muted, r - 1.5.dp.toPx(), c, style = Stroke(1.5.dp.toPx()))
        }
    }
}
```
(`ApgoPalette` is in the same package `dev.apgo2.ui`.)

- [ ] **Step 4: Model.** In `AppModel.kt`: `import uniffi.apgo_ffi.ChainOut`; add state `var chains by mutableStateOf<List<ChainOut>>(emptyList())` and `var selectedChain by mutableStateOf<String?>(null)`; in `refreshPlay()` set `chains = engine.chains()` in the game branch and `emptyList()` in the else branch (and `selectedChain = null` there); add:

```kotlin
    /** The phone's step counter changed: credit it to the open game (the engine ignores it when no game is open). */
    fun onSteps(total: Long) {
        stepsTotal = total
        if (!engine.hasGame()) return
        handle(engine.onSteps(total, now()))
        refreshPlay(withTrace = false)
    }
```
In `Sensors.kt` change the step listener body to `model.onSteps(e.values[0].toLong())`.

- [ ] **Step 5: Screens.** In `PlayScreen`, in the panel Column (the one with `weight(0.45f)`), render the chain rows first inside the Progress section: replace `if (layout.progress.isNotEmpty()) { ... }` by a block that shows chains then the remaining in-progress quests:

```kotlin
        val chainRows = m.chains.sortedBy { c -> c.marks.all { it.reached } }
        if (chainRows.isNotEmpty() || layout.progress.isNotEmpty()) Text("Progress", style = MaterialTheme.typography.titleSmall)
        chainRows.forEach { c -> ChainRow(c) { m.selectedChain = c.id; m.selected = null } }
        (if (allProgress) layout.progress else layout.progress.take(PROGRESS_ROWS)).forEach { q -> ProgressRow(q) { m.selected = q.locationId; m.selectedChain = null } }
        if (layout.progress.size > PROGRESS_ROWS) TextButton(onClick = { allProgress = !allProgress }) { Text(if (allProgress) "Show fewer" else "Show all ${layout.progress.size}", fontSize = 11.sp) }
```
Add the row composable at the end of the file:

```kotlin
/** One progressive quest: name and rule, a bar with a mark per check, and what is next. Tap for the list of marks. */
@Composable
private fun ChainRow(c: uniffi.apgo_ffi.ChainOut, onClick: () -> Unit) {
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = 2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(ApgoIcons.forKind(c.kindId, c.family), contentDescription = null, tint = ApgoPalette.family(c.family), modifier = Modifier.size(16.dp))
            Text(c.name, fontSize = 13.sp, maxLines = 1)
            Text(c.rule, fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        }
        ChainBar(ChainFormat.fill(c.counter, c.total), ChainFormat.fractions(c.marks.map { it.at }, c.total), c.marks.map { it.reached })
        Text(ChainFormat.next(c), fontSize = 10.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
```
and the popup: inside the map `Box` after the existing `selected?.let { ... }` add

```kotlin
            m.chains.firstOrNull { it.id == m.selectedChain }?.let { c ->
                MapOverlayCard(Modifier.align(Alignment.BottomCenter)) { ChainDetails(c) { m.selectedChain = null } }
            }
```
```kotlin
@Composable
private fun ColumnScope.ChainDetails(c: uniffi.apgo_ffi.ChainOut, onClose: () -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Icon(ApgoIcons.forKind(c.kindId, c.family), contentDescription = null, tint = ApgoPalette.family(c.family), modifier = Modifier.size(24.dp))
        Column(Modifier.weight(1f).padding(horizontal = 8.dp)) {
            Text(c.name, style = MaterialTheme.typography.titleSmall)
            Text(c.rule, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        IconButton(onClick = onClose) { Icon(ApgoIcons.Close, contentDescription = "Close") }
    }
    c.marks.forEachIndexed { i, mk ->
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(if (mk.reached) ApgoIcons.Check else ApgoIcons.Locked, contentDescription = null, tint = if (mk.reached) ApgoPalette.questDone else ApgoPalette.muted, modifier = Modifier.size(14.dp))
            Text("${i + 1}.  ${ChainFormat.amount(c.unit, mk.at)}", fontSize = 12.sp, modifier = Modifier.weight(1f))
            mk.reward?.let { Text(it, fontSize = 11.sp, color = ApgoPalette.success) }
        }
    }
    Text(ChainFormat.next(c), fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
}
```
Add imports for `dev.apgo2.ui.ChainBar`, `dev.apgo2.ui.ChainFormat`. In `QuestDetails`, hide the Reroll button for chain members: `if (q.state != "done" && q.chainId == null) OutlinedButton(...)`.

- [ ] **Step 6: Build, test, commit.** `cd android && ./gradlew :app:testDebugUnitTest :app:assembleDebug -q 2>&1 | grep -E "^e:|FAILED"`; commit `feat: show progressive quests as one bar`.

---

### Task 11: New Game settings for time away

**Files:**
- Create: `android/app/src/main/java/dev/apgo2/AwaySettings.kt`, `android/app/src/test/java/dev/apgo2/AwaySettingsTest.kt`
- Modify: `NewGame.kt`, `AppModel.kt`, `ui/HelpText.kt`

**Interfaces:**
- Produces: `AwaySettings.distance(auto: Boolean, text: String): UInt` (0 = automatic), `AppModel.startSolo(opts, zoneRealms, name, awayZoneOnly: Boolean, awayDistanceM: UInt)`, `AppModel.startArchipelagoGame(...)` taking the same two values (find the exact method that calls `engine.startArchipelago`, around line 500).

- [ ] **Step 1: Failing test:**

```kotlin
package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class AwaySettingsTest {
    @Test fun automaticMeansZero() = assertEquals(0u, AwaySettings.distance(auto = true, text = "1500"))

    @Test fun aCustomDistanceIsTheTypedNumber() = assertEquals(1500u, AwaySettings.distance(auto = false, text = "1500"))

    @Test fun nonsenseOrEmptyTextFallsBackToTheDefaultKilometre() {
        assertEquals(1000u, AwaySettings.distance(auto = false, text = ""))
        assertEquals(1000u, AwaySettings.distance(auto = false, text = "abc"))
        assertEquals(1000u, AwaySettings.distance(auto = false, text = "0"))
    }

    @Test fun hugeNumbersAreCappedAtTwentyKilometres() = assertEquals(20_000u, AwaySettings.distance(auto = false, text = "999999999"))
}
```
Run: FAIL (unresolved reference).

- [ ] **Step 2: Implement** `AwaySettings.kt`:

```kotlin
package dev.apgo2

/** The "time away" distance chosen in New Game. 0 asks the core to pick one from the realm's size. */
object AwaySettings {
    private const val DEFAULT_M = 1000u
    private const val MAX_M = 20_000u

    fun distance(auto: Boolean, text: String): UInt {
        if (auto) return 0u
        val n = text.trim().toULongOrNull() ?: return DEFAULT_M
        return if (n == 0uL) DEFAULT_M else minOf(n, MAX_M.toULong()).toUInt()
    }
}
```
Run, see green.

- [ ] **Step 3: Pass the values through.** In `AppModel.startSolo` add parameters `awayZoneOnly: Boolean, awayDistanceM: UInt` and pass them after `avoidStairs` in `engine.startSolo(...)`; do the same for the Archipelago start method (`engine.startArchipelago(...)`) and its callers. Add two `HelpTopic`s in `ui/HelpText.kt` following the existing pattern (`val awayZone = HelpTopic("Time away", "Count the time you spend away from home only while you are inside one of your game's zones, or anywhere. It needs GPS, so it only counts while you are playing.")`, `val awayDistance = HelpTopic("Away distance", "How far from home counts as away. Automatic picks a distance from the size of your realm (about 40% of the way to its far edge, between 300 m and 3 km). Switch it off to type your own.")`).
In `NewGame.kt`, near the zones section add state and controls and pass them to the start calls (find the `m.startSolo(...)` and AP calls in this file):

```kotlin
    var awayZoneOnly by remember { mutableStateOf(true) }
    var awayAuto by remember { mutableStateOf(true) }
    var awayMeters by remember { mutableStateOf("1000") }
    ...
        Text("Time away", style = MaterialTheme.typography.titleMedium)
        SwitchRow("Only count time inside a zone", Help.awayZone, awayZoneOnly) { awayZoneOnly = it }
        SwitchRow("Pick the distance automatically", Help.awayDistance, awayAuto) { awayAuto = it }
        if (!awayAuto) OutlinedTextField(awayMeters, { awayMeters = it.filter(Char::isDigit).take(5) }, label = { Text("Away distance (metres)") }, singleLine = true, keyboardOptions = androidx.compose.foundation.text.KeyboardOptions(keyboardType = androidx.compose.ui.text.input.KeyboardType.Number), modifier = Modifier.fillMaxWidth())
```
and call `m.startSolo(opts, zoneRealms, name, awayZoneOnly, AwaySettings.distance(awayAuto, awayMeters))`.

- [ ] **Step 4: Build, test, commit** (`feat: choose how time away counts`).

---

### Task 12: Verify Part A on the emulator, document, commit

- [ ] **Step 1: Install and start a fresh solo game** on the emulator (`emulator-5554`): `./gradlew :app:assembleDebug -q; adb -s emulator-5554 install -r app/build/outputs/apk/debug/app-debug.apk`. In New Game leave "Time away" defaults, start; open Play. Expected: a "Progress" section with one Step Up row (bar with several marks), one Wanderlust row and one Cartographer row; none of their members appear in "Show places on the map".
- [ ] **Step 2: Drive progress.** Move the emulator with `adb -s emulator-5554 emu geo fix <lon> <lat>` away from home in 150 m+ jumps with 6 s pauses (each new cell fills Cartographer); watch the bar fill and marks turn green and rewards appear in the Activity tab ("... milestone N of M: ...").
- [ ] **Step 3: Restart test.** Force-stop and relaunch the app, open the game: the bars keep their progress (steps resume from the new baseline, nothing is credited for the closed time).
- [ ] **Step 4: Update docs.** In `docs/context/v1-architecture-and-status.md` add a "Progressive chains" paragraph (what exists, where the counters live, that steps count only while a game is open) and remove "Effort Reduction ... Collection" nothing; update `docs/context/quest-catalog.md` only if regenerated. Commit `docs: describe progressive quest chains`.

---

# Part B: presence

### Task 13: Core `counting` flag

**Files:** Modify `core/src/game.rs`.

**Interfaces:** Produces `Game::set_counting(&mut self, bool)`; while false `on_fix` and `on_steps` credit nothing except keeping `steps_last` current.

- [ ] **Step 1: Failing tests** (game.rs tests module):

```rust
    #[test]
    fn nothing_counts_while_counting_is_off_and_no_steps_are_credited_for_that_time() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_steps(1_000, 1);
        g.set_counting(false);
        assert!(g.on_steps(5_000, 2).is_empty(), "4,000 steps at home");
        assert_eq!(g.counters.progress.get("1:step_up").copied().unwrap_or(0.0), 0.0);
        g.set_counting(true);
        assert!(g.on_steps(5_100, 3).is_empty(), "only the 100 steps since counting resumed");
        assert_eq!(g.counters.progress["1:step_up"], 100.0);
    }

    #[test]
    fn a_reach_quest_does_not_complete_while_counting_is_off() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        g.set_counting(false);
        assert!(g.on_fix(Fix { accuracy_m: 5.0, ..fixat(q.anchor.unwrap(), 100) }, None).is_empty());
        assert!(!g.done.contains(&q.location_id));
        g.set_counting(true);
        assert!(!g.on_fix(Fix { accuracy_m: 5.0, ..fixat(q.anchor.unwrap(), 200) }, None).is_empty());
    }

    #[test]
    fn the_first_fix_after_resuming_is_not_judged_against_a_stale_one() {
        let (mut g, id, p0) = start_near_a_quest();
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| crate::fog::anchor(&a.target)).unwrap();
        g.set_counting(false);
        g.set_counting(true);
        // 200 m from the last fix 3 s later would be dropped as a jump if the old fix were kept
        let ev = g.on_fix(Fix { accuracy_m: 5.0, ..fixat(target, 1003) }, None);
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { location_id, .. } if *location_id == id)), "{ev:?} from {p0:?}");
    }

    #[test]
    fn distance_is_not_added_while_counting_is_off() {
        let (mut g, _, p0) = start_near_a_quest();
        let before = g.stats.distance_m;
        g.set_counting(false);
        for i in 1..=10 {
            g.on_fix(Fix { accuracy_m: 5.0, ..fixat(destination(p0, 90.0, 30.0 * f64::from(i)), 1000 + i64::from(i) * 10) }, None);
        }
        assert_eq!(g.stats.distance_m, before);
    }
```

- [ ] **Step 2: Run, see compile failure.**
- [ ] **Step 3: Implement.** Field `#[serde(skip_serializing, default = "yes")] counting: bool,` (initialise `true` in `create`). Add:

```rust
    /// Presence rules (at home, in the car) switch counting off: nothing is checked, credited or added while it is off.
    pub fn set_counting(&mut self, on: bool) {
        if self.counting == on {
            return;
        }
        self.counting = on;
        // Whatever the player did while it was off must not be compared with what they do next.
        self.last_fix = None;
        self.odo_anchor = None;
        self.outlier_streak = 0;
    }
```
In `on_fix`, at the top after `let mut ev = Vec::new();`: `if !self.counting { if let Some(t) = steps_total { self.counters.steps_last = Some(t); } return ev; }`. In `on_steps`: `if !self.counting { self.counters.steps_last = Some(total); return Vec::new(); }` before `credit_steps`.
- [ ] **Step 4: Run, see green; fmt/clippy.**
- [ ] **Step 5: Commit** (`feat: switch counting off for presence rules`).

---

### Task 14: FFI: zone proximity, counting and presence events

**Files:** Modify `core/ffi/src/engine.rs`, `core/src/journal.rs` (kind constant).

**Interfaces:** Produces `Engine.zone_proximity(lat: f64, lon: f64) -> String` (`"inside"` | `"near"` | `"far"` | `"unknown"`), `Engine.set_counting(on: Boolean)`, `Engine.log_presence(text: String, t_ms: i64)`; `journal::kind::PRESENCE = "presence"`; near threshold `NEAR_ZONE_M = 300.0`.

- [ ] **Step 1: Failing test** for the classification as a pure core function. In `core/src/realm.rs` tests add:

```rust
    #[test]
    fn proximity_is_inside_near_or_far_with_a_300_m_buffer() {
        let c = Point::new(40.0, -111.0);
        let s = Shape::Circle { center: c, radius_m: 500.0 };
        assert_eq!(s.proximity(crate::geo::destination(c, 0.0, 100.0)), Proximity::Inside);
        assert_eq!(s.proximity(crate::geo::destination(c, 0.0, 700.0)), Proximity::Near);
        assert_eq!(s.proximity(crate::geo::destination(c, 0.0, 900.0)), Proximity::Far);
    }
```
- [ ] **Step 2: Implement** in `core/src/realm.rs`:

```rust
/// Where a point is relative to a zone area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proximity {
    Inside,
    /// Within `NEAR_ZONE_M` of the area: precise GPS starts here so arrival is not missed.
    Near,
    Far,
}

pub const NEAR_ZONE_M: f64 = 300.0;

impl Shape {
    pub fn proximity(&self, p: Point) -> Proximity {
        let d = self.distance_m(p);
        if d == 0.0 {
            Proximity::Inside
        } else if d <= NEAR_ZONE_M {
            Proximity::Near
        } else {
            Proximity::Far
        }
    }
}
```
Run the test, see green. In `journal.rs` add `pub const PRESENCE: &str = "presence";` (and a label in Kotlin later).
- [ ] **Step 3: Engine calls** (`core/ffi/src/engine.rs`):

```rust
    /// "inside" | "near" | "far" for the open game's zones, "unknown" with no game or no zones.
    pub fn zone_proximity(&self, lat: f64, lon: f64) -> String {
        let p = Point::new(lat, lon);
        let shapes = self.zone_shapes.lock().unwrap_or_else(|e| e.into_inner());
        let best = shapes.iter().map(|s| s.proximity(p)).min_by_key(|x| match x {
            Proximity::Inside => 0,
            Proximity::Near => 1,
            Proximity::Far => 2,
        });
        match best {
            Some(Proximity::Inside) => "inside",
            Some(Proximity::Near) => "near",
            Some(Proximity::Far) => "far",
            None => "unknown",
        }
        .into()
    }

    /// Presence rules (home Wi-Fi, car) turn counting off and on.
    pub fn set_counting(&self, on: bool) {
        self.with_game(|g| g.set_counting(on));
    }

    /// Record a presence change ("Home Wi-Fi connected, paused") in the activity log.
    pub fn log_presence(&self, text: String, t_ms: i64) {
        let Some(id) = self.game_id() else { return };
        self.journal_do(|j| j.log(&id, &JournalEvent { t_ms, kind: kind::PRESENCE.into(), detail: text, at: None }));
    }
```
Import `apgo_core::realm::Proximity`.
- [ ] **Step 4: Build, clippy, rebuild bindings, commit** (`feat: expose zone proximity and counting`).

---

### Task 15: Presence policy and debouncer (pure Kotlin)

**Files:**
- Create: `android/app/src/main/java/dev/apgo2/presence/PresencePolicy.kt`, `android/app/src/test/java/dev/apgo2/presence/PresencePolicyTest.kt`

**Interfaces:**
- Produces:
```kotlin
enum class Zone { Inside, Near, Far, Unknown }
enum class PresenceState { Stopped, InCar, AtHome, InZone, OutsideZones }
data class Signals(val playing: Boolean, val homeWifi: Boolean?, val carBluetooth: Boolean?, val zone: Zone)
sealed interface GpsMode { object Off : GpsMode; data class Rate(val intervalMs: Long, val minDistanceM: Float) : GpsMode }
data class Decision(val state: PresenceState, val gps: GpsMode, val counting: Boolean)
object PresencePolicy { fun decide(s: Signals): Decision; const val COARSE_MS = 90_000L }
class Debouncer(private val holdMs: Long = 45_000) { fun feed(raw: Boolean?, nowMs: Long): Boolean? }
```
`Debouncer.feed` returns the **stable** value: it adopts a changed raw value only after the raw value has stayed the same for `holdMs`; `null` (unknown) is adopted immediately and the first value ever is adopted immediately.

- [ ] **Step 1: Failing tests:**

```kotlin
package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PresencePolicyTest {
    private fun s(playing: Boolean = true, home: Boolean? = false, car: Boolean? = false, zone: Zone = Zone.Inside) = Signals(playing, home, car, zone)

    @Test fun notPlayingIsStoppedAndNothingCounts() {
        val d = PresencePolicy.decide(s(playing = false))
        assertEquals(PresenceState.Stopped, d.state)
        assertEquals(false, d.counting)
    }

    @Test fun theCarBeatsEverythingElse() {
        val d = PresencePolicy.decide(s(car = true, home = true, zone = Zone.Inside))
        assertEquals(Decision(PresenceState.InCar, GpsMode.Off, counting = false), d)
    }

    @Test fun homeWifiTurnsGpsOffAndStopsCounting() {
        assertEquals(Decision(PresenceState.AtHome, GpsMode.Off, counting = false), PresencePolicy.decide(s(home = true, zone = Zone.Far)))
    }

    @Test fun insideNearOrUnknownZoneIsPreciseAndCounts() {
        for (z in listOf(Zone.Inside, Zone.Near, Zone.Unknown)) {
            assertEquals("$z", Decision(PresenceState.InZone, GpsMode.Rate(5_000L, 0f), counting = true), PresencePolicy.decide(s(zone = z)))
        }
    }

    @Test fun farFromEveryZoneIsCoarseButStillCounts() {
        assertEquals(Decision(PresenceState.OutsideZones, GpsMode.Rate(PresencePolicy.COARSE_MS, 0f), counting = true), PresencePolicy.decide(s(zone = Zone.Far)))
    }

    @Test fun unknownSignalsAreTreatedAsNotPresent() {
        assertEquals(PresenceState.InZone, PresencePolicy.decide(s(home = null, car = null, zone = Zone.Inside)).state)
    }
}

class DebouncerTest {
    @Test fun theFirstValueIsAdoptedAtOnce() = assertEquals(true, Debouncer(45_000).feed(true, 0))

    @Test fun aChangeOnlyTakesEffectAfterItHasHeldLongEnough() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        assertEquals("just changed", false, d.feed(true, 1_000))
        assertEquals("still inside the hold", false, d.feed(true, 30_000))
        assertEquals("held for 45 s", true, d.feed(true, 46_001))
    }

    @Test fun flappingNeverSettles() {
        val d = Debouncer(45_000)
        d.feed(false, 0)
        var t = 1_000L
        repeat(20) { d.feed(it % 2 == 0, t); t += 10_000 }
        assertEquals(false, d.feed(false, t))
    }

    @Test fun unknownIsAdoptedImmediatelyAndNeverHeldBack() {
        val d = Debouncer(45_000)
        d.feed(true, 0)
        assertNull(d.feed(null, 1_000))
    }
}
```
- [ ] **Step 2: Run, see failure** (unresolved references).
- [ ] **Step 3: Implement** `PresencePolicy.kt`:

```kotlin
package dev.apgo2.presence

enum class Zone { Inside, Near, Far, Unknown }

enum class PresenceState { Stopped, InCar, AtHome, InZone, OutsideZones }

/** What the phone currently knows. `null` for a signal means unavailable or not permitted: treated as "not present". */
data class Signals(val playing: Boolean, val homeWifi: Boolean?, val carBluetooth: Boolean?, val zone: Zone)

sealed interface GpsMode {
    object Off : GpsMode
    data class Rate(val intervalMs: Long, val minDistanceM: Float) : GpsMode
}

data class Decision(val state: PresenceState, val gps: GpsMode, val counting: Boolean)

/** Presence rules (spec Part B): which state the player is in decides how GPS runs and whether progress counts. First match wins. */
object PresencePolicy {
    const val COARSE_MS = 90_000L
    private const val PRECISE_MS = 5_000L

    fun decide(s: Signals): Decision = when {
        !s.playing -> Decision(PresenceState.Stopped, GpsMode.Off, counting = false)
        s.carBluetooth == true -> Decision(PresenceState.InCar, GpsMode.Off, counting = false)
        s.homeWifi == true -> Decision(PresenceState.AtHome, GpsMode.Off, counting = false)
        s.zone == Zone.Far -> Decision(PresenceState.OutsideZones, GpsMode.Rate(COARSE_MS, 0f), counting = true)
        else -> Decision(PresenceState.InZone, GpsMode.Rate(PRECISE_MS, 0f), counting = true)
    }
}

/** Holds a signal steady: a change only becomes the stable value after it has lasted [holdMs] (Wi-Fi reaches past the door, Bluetooth flaps). */
class Debouncer(private val holdMs: Long = 45_000) {
    private var stable: Boolean? = null
    private var started = false
    private var candidate: Boolean? = null
    private var since = 0L

    fun feed(raw: Boolean?, nowMs: Long): Boolean? {
        if (!started || raw == null) {
            started = true
            stable = raw
            candidate = raw
            since = nowMs
            return stable
        }
        if (raw != candidate) {
            candidate = raw
            since = nowMs
        }
        if (candidate != stable && nowMs - since >= holdMs) stable = candidate
        return stable
    }
}
```
Note `Stopped` carries `GpsMode.Off` in the decision; `Sensors` keeps its existing "idle while the app is on screen" rule for that state (Task 18), it does not use the decision's GPS mode for `Stopped`.
- [ ] **Step 4: Run, see green.**
- [ ] **Step 5: Commit** (`feat: add the presence policy and debouncer`).

---

### Task 16: Wi-Fi and Bluetooth matching (pure Kotlin)

**Files:**
- Create: `presence/PresenceSignals.kt`, `android/app/src/test/java/dev/apgo2/presence/PresenceSignalsTest.kt`

**Interfaces:**
- Produces: `data class WifiId(val ssid: String?, val bssid: String?)`, `data class HomeNetwork(val ssid: String, val bssid: String?)`, `PresenceSignals.cleanSsid(raw: String?): String?`, `PresenceSignals.isHome(current: WifiId?, saved: List<HomeNetwork>): Boolean?` (null when `current` is null/unknown or nothing is saved), `data class CarDevice(val name: String, val address: String)`, `PresenceSignals.carConnected(connectedAddresses: Set<String>?, saved: List<CarDevice>): Boolean?`.

- [ ] **Step 1: Failing tests:**

```kotlin
package dev.apgo2.presence

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class PresenceSignalsTest {
    @Test fun ssidQuotesAreStrippedAndUnknownNamesBecomeNull() {
        assertEquals("HomeNet", PresenceSignals.cleanSsid("\"HomeNet\""))
        assertEquals("HomeNet", PresenceSignals.cleanSsid("HomeNet"))
        assertNull(PresenceSignals.cleanSsid("<unknown ssid>"))
        assertNull(PresenceSignals.cleanSsid("\"<unknown ssid>\""))
        assertNull(PresenceSignals.cleanSsid(""))
        assertNull(PresenceSignals.cleanSsid(null))
    }

    @Test fun aSavedNetworkMatchesByBssidOrSsid() {
        val saved = listOf(HomeNetwork("HomeNet", "aa:bb:cc:dd:ee:01"))
        assertEquals(true, PresenceSignals.isHome(WifiId("HomeNet", null), saved))
        assertEquals("same router, new name, case-insensitive", true, PresenceSignals.isHome(WifiId("Renamed", "AA:BB:CC:DD:EE:01"), saved))
        assertEquals(false, PresenceSignals.isHome(WifiId("CafeWifi", "11:22:33:44:55:66"), saved))
    }

    @Test fun aBssidMismatchWithTheSameSsidStillCountsBecauseHomesHaveSeveralAccessPoints() {
        val saved = listOf(HomeNetwork("HomeNet", "aa:bb:cc:dd:ee:01"))
        assertEquals(true, PresenceSignals.isHome(WifiId("HomeNet", "aa:bb:cc:dd:ee:02"), saved))
    }

    @Test fun noConnectionOrNothingSavedIsUnknownNotFalse() {
        assertNull(PresenceSignals.isHome(null, listOf(HomeNetwork("HomeNet", null))))
        assertNull(PresenceSignals.isHome(WifiId(null, null), listOf(HomeNetwork("HomeNet", null))))
        assertNull(PresenceSignals.isHome(WifiId("HomeNet", null), emptyList()))
    }

    @Test fun theCarIsConnectedWhenATaggedDeviceIsConnected() {
        val saved = listOf(CarDevice("Subaru", "AA:AA:AA:AA:AA:01"))
        assertEquals(true, PresenceSignals.carConnected(setOf("aa:aa:aa:aa:aa:01", "BB:BB:BB:BB:BB:02"), saved))
        assertEquals(false, PresenceSignals.carConnected(setOf("BB:BB:BB:BB:BB:02"), saved))
        assertEquals(false, PresenceSignals.carConnected(emptySet(), saved))
    }

    @Test fun noTaggedCarOrAnUnreadableListIsUnknown() {
        assertNull(PresenceSignals.carConnected(setOf("AA"), emptyList()))
        assertNull(PresenceSignals.carConnected(null, listOf(CarDevice("Subaru", "AA"))))
    }
}
```
- [ ] **Step 2: Run, see failure.**
- [ ] **Step 3: Implement** `PresenceSignals.kt`:

```kotlin
package dev.apgo2.presence

data class WifiId(val ssid: String?, val bssid: String?)
data class HomeNetwork(val ssid: String, val bssid: String?)
data class CarDevice(val name: String, val address: String)

/** Turning what Android reports into the `Boolean?` signals the policy reads. Pure. */
object PresenceSignals {
    /** Android quotes SSIDs and reports `<unknown ssid>` when it may not tell. */
    fun cleanSsid(raw: String?): String? {
        val s = raw?.trim()?.removeSurrounding("\"") ?: return null
        return if (s.isEmpty() || s.equals("<unknown ssid>", ignoreCase = true)) null else s
    }

    /** `true` at home, `false` on another network, `null` when unknown (not connected, name hidden, or no home network saved). */
    fun isHome(current: WifiId?, saved: List<HomeNetwork>): Boolean? {
        if (saved.isEmpty()) return null
        val ssid = cleanSsid(current?.ssid)
        val bssid = current?.bssid?.lowercase()?.takeIf { it.isNotBlank() && it != "02:00:00:00:00:00" }
        if (ssid == null && bssid == null) return null
        return saved.any { h -> (bssid != null && h.bssid?.lowercase() == bssid) || (ssid != null && h.ssid == ssid) }
    }

    fun carConnected(connectedAddresses: Set<String>?, saved: List<CarDevice>): Boolean? {
        if (saved.isEmpty() || connectedAddresses == null) return null
        val connected = connectedAddresses.map { it.lowercase() }.toSet()
        return saved.any { it.address.lowercase() in connected }
    }
}
```
- [ ] **Step 4: Run, see green. Step 5: Commit** (`feat: match home Wi-Fi and car Bluetooth`).

---

### Task 17: Android signal sources, settings storage and permissions

**Files:**
- Create: `presence/PresenceSettings.kt`, `presence/PresenceMonitor.kt`
- Modify: `app/src/main/AndroidManifest.xml`

**Interfaces:**
- Produces: `PresenceSettings(ctx)` with `homeNetworks: List<HomeNetwork>`, `carDevices: List<CarDevice>`, `addHome(HomeNetwork)`, `removeHome(ssid: String)`, `setCar(List<CarDevice>)` persisted in `SharedPreferences("presence")` as JSON strings (use `org.json`); `PresenceMonitor(ctx, onChange: () -> Unit)` with `start()`, `stop()`, `currentWifi: WifiId?` and `connectedCarCandidates: Set<String>?` (addresses of connected Bluetooth devices, `null` when `BLUETOOTH_CONNECT` is not granted), `fun currentNetwork(): WifiId?` for the "Add current network" button.

- [ ] **Step 1: Manifest.** Add inside `<manifest>`: `<uses-permission android:name="android.permission.ACCESS_WIFI_STATE" />`, `<uses-permission android:name="android.permission.ACCESS_NETWORK_STATE" />`, `<uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />`, `<uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30" />`.
- [ ] **Step 2: `PresenceSettings.kt`:**

```kotlin
package dev.apgo2.presence

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/** App-level presence settings (platform identifiers, so they live in preferences, not in the core). */
class PresenceSettings(ctx: Context) {
    private val prefs = ctx.getSharedPreferences("presence", Context.MODE_PRIVATE)

    var homeNetworks: List<HomeNetwork>
        get() = runCatching {
            val a = JSONArray(prefs.getString("home", "[]"))
            (0 until a.length()).map { a.getJSONObject(it).let { o -> HomeNetwork(o.getString("ssid"), o.optString("bssid").takeIf { b -> b.isNotBlank() }) } }
        }.getOrDefault(emptyList())
        private set(v) = prefs.edit().putString("home", JSONArray(v.map { JSONObject().put("ssid", it.ssid).put("bssid", it.bssid ?: "") }).toString()).apply()

    var carDevices: List<CarDevice>
        get() = runCatching {
            val a = JSONArray(prefs.getString("car", "[]"))
            (0 until a.length()).map { a.getJSONObject(it).let { o -> CarDevice(o.getString("name"), o.getString("address")) } }
        }.getOrDefault(emptyList())
        private set(v) = prefs.edit().putString("car", JSONArray(v.map { JSONObject().put("name", it.name).put("address", it.address) }).toString()).apply()

    fun addHome(n: HomeNetwork) { homeNetworks = homeNetworks.filterNot { it.ssid == n.ssid } + n }
    fun removeHome(ssid: String) { homeNetworks = homeNetworks.filterNot { it.ssid == ssid } }
    fun setCar(devices: List<CarDevice>) { carDevices = devices }
}
```
- [ ] **Step 3: `PresenceMonitor.kt`.** One class that registers (a) a `ConnectivityManager.NetworkCallback` for `NetworkCapabilities.TRANSPORT_WIFI`, created with `ConnectivityManager.NetworkCallback(ConnectivityManager.NetworkCallback.FLAG_INCLUDE_LOCATION_INFO)` on API 31+ and the plain constructor below, reading `(caps.transportInfo as? WifiInfo)` for `ssid`/`bssid` in `onCapabilitiesChanged`, clearing on `onLost`; below API 31 read `WifiManager.connectionInfo`; (b) a `BroadcastReceiver` for `BluetoothDevice.ACTION_ACL_CONNECTED` / `ACTION_ACL_DISCONNECTED` keeping a `MutableSet<String>` of addresses, plus an initial read of connected A2DP and HEADSET devices through `BluetoothAdapter.getProfileProxy` (only when `checkSelfPermission(BLUETOOTH_CONNECT)` is granted, else `connectedCarCandidates = null`). Both call `onChange()` on the main thread when anything changes. Expose `fun currentNetwork(): WifiId?` for the settings button. Skeleton:

```kotlin
package dev.apgo2.presence

import android.annotation.SuppressLint
import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.WifiInfo
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Handler
import android.os.Looper

/** Watches the Wi-Fi network and the Bluetooth connections that decide presence. Needs location permission for Wi-Fi names and BLUETOOTH_CONNECT for devices. */
class PresenceMonitor(private val ctx: Context, private val onChange: () -> Unit) {
    private val main = Handler(Looper.getMainLooper())
    private val cm = ctx.getSystemService(ConnectivityManager::class.java)
    var currentWifi: WifiId? = null
        private set
    var connectedBluetooth: Set<String>? = null
        private set
    private val bt = mutableSetOf<String>()
    private var started = false

    private val netCallback: ConnectivityManager.NetworkCallback =
        if (Build.VERSION.SDK_INT >= 31) object : ConnectivityManager.NetworkCallback(FLAG_INCLUDE_LOCATION_INFO) {
            override fun onCapabilitiesChanged(n: Network, caps: NetworkCapabilities) = update((caps.transportInfo as? WifiInfo)?.let { WifiId(it.ssid, it.bssid) })
            override fun onLost(n: Network) = update(null)
        } else object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(n: Network, caps: NetworkCapabilities) = update(legacyWifi())
            override fun onLost(n: Network) = update(null)
        }

    @Suppress("DEPRECATION")
    @SuppressLint("MissingPermission")
    private fun legacyWifi(): WifiId? = ctx.applicationContext.getSystemService(WifiManager::class.java)?.connectionInfo?.let { WifiId(it.ssid, it.bssid) }

    private fun update(w: WifiId?) = main.post { if (w != currentWifi) { currentWifi = w; onChange() } }

    private val btReceiver = object : BroadcastReceiver() {
        override fun onReceive(c: Context, i: Intent) {
            val d = if (Build.VERSION.SDK_INT >= 33) i.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE, BluetoothDevice::class.java) else @Suppress("DEPRECATION") i.getParcelableExtra(BluetoothDevice.EXTRA_DEVICE)
            val a = d?.address ?: return
            if (i.action == BluetoothDevice.ACTION_ACL_CONNECTED) bt.add(a) else bt.remove(a)
            connectedBluetooth = bt.toSet()
            onChange()
        }
    }

    private fun btAllowed() = Build.VERSION.SDK_INT < 31 || ctx.checkSelfPermission(android.Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED

    @SuppressLint("MissingPermission")
    fun start() {
        if (started) return
        started = true
        cm.registerNetworkCallback(NetworkRequest.Builder().addTransportType(NetworkCapabilities.TRANSPORT_WIFI).build(), netCallback)
        if (btAllowed()) {
            ctx.registerReceiver(btReceiver, IntentFilter().apply { addAction(BluetoothDevice.ACTION_ACL_CONNECTED); addAction(BluetoothDevice.ACTION_ACL_DISCONNECTED) })
            connectedBluetooth = bt.toSet()
            val adapter = ctx.getSystemService(BluetoothManager::class.java)?.adapter
            for (profile in intArrayOf(BluetoothProfile.A2DP, BluetoothProfile.HEADSET)) {
                adapter?.getProfileProxy(ctx, object : BluetoothProfile.ServiceListener {
                    override fun onServiceConnected(p: Int, proxy: BluetoothProfile) {
                        runCatching { proxy.connectedDevices.forEach { bt.add(it.address) } }
                        connectedBluetooth = bt.toSet()
                        adapter.closeProfileProxy(p, proxy)
                        main.post(onChange)
                    }
                    override fun onServiceDisconnected(p: Int) {}
                }, profile)
            }
        }
    }

    fun stop() {
        if (!started) return
        started = false
        runCatching { cm.unregisterNetworkCallback(netCallback) }
        runCatching { ctx.unregisterReceiver(btReceiver) }
    }

    /** The network the phone is on right now, for "Add current network". */
    fun currentNetwork(): WifiId? = currentWifi ?: legacyWifi().takeIf { PresenceSignals.cleanSsid(it?.ssid) != null }
}
```
- [ ] **Step 4: Build.** `cd android && ./gradlew :app:assembleDebug -q 2>&1 | grep -E "^e:|FAILED"`. No unit test here (Android framework glue); the pure parts are tested in Tasks 15-16.
- [ ] **Step 5: Commit** (`feat: watch Wi-Fi and Bluetooth for presence`).

---

### Task 18: Wire presence into the app

**Files:** Modify `AppModel.kt`, `GpsPolicy.kt`, `GpsPolicyTest.kt`, `Sensors.kt`, `MainActivity.kt`, `AwayFormat.kt`.

**Interfaces:**
- Consumes: Tasks 13-17.
- Produces: `AppModel.presence: Decision` (state shown by the chip), `AppModel.settings: PresenceSettings`, `AppModel.monitor: PresenceMonitor`; `GpsPolicy.forDecision(d: Decision, appVisible: Boolean): Rate?` (null = location off).

- [ ] **Step 1: Failing test** (`GpsPolicyTest.kt`):

```kotlin
class GpsDecisionTest {
    @Test fun aRateDecisionBecomesThatRate() {
        val d = Decision(PresenceState.InZone, GpsMode.Rate(5_000L, 0f), counting = true)
        assertEquals(GpsPolicy.Rate(5_000L, 0f), GpsPolicy.forDecision(d, appVisible = false))
    }

    @Test fun offMeansNoLocationEvenWhenTheAppIsOnScreen() {
        val d = Decision(PresenceState.AtHome, GpsMode.Off, counting = false)
        assertEquals(null, GpsPolicy.forDecision(d, appVisible = true))
    }

    @Test fun stoppedUsesTheIdleRuleOnlyWhileTheAppIsVisible() {
        val d = Decision(PresenceState.Stopped, GpsMode.Off, counting = false)
        assertEquals(GpsPolicy.forState(playing = false), GpsPolicy.forDecision(d, appVisible = true))
        assertEquals(null, GpsPolicy.forDecision(d, appVisible = false))
    }
}
```
(import `dev.apgo2.presence.*`.) Run, see failure.
- [ ] **Step 2: Implement** in `GpsPolicy.kt`:

```kotlin
    /** The location rate a presence decision asks for; `null` means location is off. Stopped keeps the old "map marker while the app is on screen" rule. */
    fun forDecision(d: dev.apgo2.presence.Decision, appVisible: Boolean): Rate? = when {
        d.state == dev.apgo2.presence.PresenceState.Stopped -> if (appVisible) forState(playing = false) else null
        d.gps is dev.apgo2.presence.GpsMode.Rate -> Rate(d.gps.intervalMs, d.gps.minDistanceM)
        else -> null
    }
```
Run, see green.
- [ ] **Step 3: The app model.** In `AppModel.kt` add: `val settings = dev.apgo2.presence.PresenceSettings(ctx)`; `val monitor = dev.apgo2.presence.PresenceMonitor(ctx) { evaluatePresence() }`; `var presence by mutableStateOf(Decision(PresenceState.Stopped, GpsMode.Off, false))`; `private val homeDebounce = Debouncer()`, `private val carDebounce = Debouncer()`; `private var zone = Zone.Unknown`; and:

```kotlin
    /** Recompute the presence decision from the current signals and apply it: GPS rate, the core's counting flag, and an activity-log line on change. */
    fun evaluatePresence() {
        val t = now()
        val home = homeDebounce.feed(PresenceSignals.isHome(monitor.currentWifi, settings.homeNetworks), t)
        val car = carDebounce.feed(PresenceSignals.carConnected(monitor.connectedBluetooth, settings.carDevices), t)
        val d = PresencePolicy.decide(Signals(playing = hud != null, homeWifi = home, carBluetooth = car, zone = zone))
        if (d == presence) return
        val changedState = d.state != presence.state
        presence = d
        engine.setCounting(d.counting)
        if (changedState) {
            Diag.i("presence", d.state.name, "counting" to d.counting, "gps" to d.gps.toString())
            if (hud != null) engine.logPresence(presenceText(d.state), t)
        }
        applyLocation()
    }

    private fun presenceText(s: PresenceState) = when (s) {
        PresenceState.AtHome -> "Home Wi-Fi connected: paused"
        PresenceState.InCar -> "Car Bluetooth connected: not counting"
        PresenceState.OutsideZones -> "Outside every zone: saving battery"
        PresenceState.InZone -> "Tracking"
        PresenceState.Stopped -> "Stopped playing"
    }

    var appVisible = true
    /** Start, change or stop location to match the presence decision. */
    fun applyLocation() {
        val rate = GpsPolicy.forDecision(presence, appVisible)
        if (rate == null) sensors.stopLocation() else sensors.startLocation(rate)
    }
```
Update `onFix` to refresh the zone from each fix and re-evaluate: after the `engine.onFix(...)` call add `zone = when (engine.zoneProximity(loc.latitude, loc.longitude)) { "inside" -> Zone.Inside; "near" -> Zone.Near; "far" -> Zone.Far; else -> Zone.Unknown }; evaluatePresence()`. In `openGame`/start paths and `pause()` call `evaluatePresence()` after `refreshAll()` (the `playing` signal changes with `hud`). Add the activity label `"presence" to "Presence"` in `AwayFormat.LABELS`.
- [ ] **Step 4: Activity wiring.** In `MainActivity.kt` replace the `LaunchedEffect(permitted, rate, visible, playingNow)` block with: `LaunchedEffect(permitted, model.hud != null, visible, model.presence) { model.appVisible = visible; if (permitted) model.applyLocation() else model.sensors.stopLocation() }`, and start the monitor once: `DisposableEffect(permitted) { if (permitted) { model.monitor.start(); model.evaluatePresence() }; onDispose { model.monitor.stop() } }`. Keep the foreground service rule (`playing`) unchanged. In `Sensors.startLocation` nothing changes (it already takes a `Rate`).
- [ ] **Step 5: Heartbeat and journal.** Add `"presence" to presence.state.name, "counting" to presence.counting` to the heartbeat fields; keep the 60 s beat running while a game is open.
- [ ] **Step 6: Build, test, commit** (`feat: apply presence rules to GPS and counting`). If `engine.zoneProximity`/`setCounting`/`logPresence` are unresolved, rebuild the bindings (`scripts/android_core.sh debug`).

---

### Task 19: Settings UI and status chip

**Files:** Create `PresenceScreen.kt`; modify `Screens.kt` (chip, Home card entry), `MainActivity.kt` (Bluetooth permission launcher), `ui/HelpText.kt`.

- [ ] **Step 1: Status chip text as a tested pure function.** Add to `presence/PresenceSignals.kt` an object `PresenceText` with `fun chip(state: PresenceState, configured: Boolean): String` returning "Tracking" (InZone, or any state while `!configured`), "At home, paused" (AtHome), "In car, not counting" (InCar), "Outside zones, saving battery" (OutsideZones), "Not playing" (Stopped). Test in `PresenceSignalsTest`:

```kotlin
    @Test fun theChipSaysWhatIsHappeningAndIsPlainWhenNothingIsConfigured() {
        assertEquals("At home, paused", PresenceText.chip(PresenceState.AtHome, configured = true))
        assertEquals("In car, not counting", PresenceText.chip(PresenceState.InCar, configured = true))
        assertEquals("Outside zones, saving battery", PresenceText.chip(PresenceState.OutsideZones, configured = true))
        assertEquals("Tracking", PresenceText.chip(PresenceState.InZone, configured = true))
        assertEquals("Tracking", PresenceText.chip(PresenceState.OutsideZones, configured = false))
        assertEquals("Not playing", PresenceText.chip(PresenceState.Stopped, configured = true))
    }
```
Run (fail), implement, run (green).
- [ ] **Step 2: Chip on Play.** In the Play header `Row`, under the game name add `Text(PresenceText.chip(m.presence.state, m.settings.homeNetworks.isNotEmpty() || m.settings.carDevices.isNotEmpty()), fontSize = 11.sp, color = MaterialTheme.colorScheme.primary)`.
- [ ] **Step 3: `PresenceScreen.kt`** and its entry point. Add `var showPresence by mutableStateOf(false)` to `AppModel`. In the Home card on the Realms screen add `OutlinedButton(onClick = { m.showPresence = true }) { Text("Presence") }`. At the top of `AppRoot` add `if (m.showPresence) { BackHandler { m.showPresence = false }; Surface(Modifier.fillMaxSize()) { PresenceScreen(m) }; return }`. Create `PresenceScreen.kt`:

```kotlin
package dev.apgo2

import android.Manifest
import android.bluetooth.BluetoothManager
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.apgo2.presence.CarDevice
import dev.apgo2.presence.HomeNetwork
import dev.apgo2.presence.PresenceSignals
import dev.apgo2.ui.ApgoIcons

/** Where the player says which Wi-Fi is home and which Bluetooth device is the car. */
@Composable
fun PresenceScreen(m: AppModel) {
    val ctx = LocalContext.current
    var home by remember { mutableStateOf(m.settings.homeNetworks) }
    var car by remember { mutableStateOf(m.settings.carDevices) }
    var btOk by remember { mutableStateOf(Build.VERSION.SDK_INT < 31 || ctx.checkSelfPermission(Manifest.permission.BLUETOOTH_CONNECT) == PackageManager.PERMISSION_GRANTED) }
    val askBt = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { btOk = it }
    val paired = remember(btOk) {
        if (!btOk) emptyList()
        else runCatching { ctx.getSystemService(BluetoothManager::class.java)?.adapter?.bondedDevices?.map { CarDevice(it.name ?: it.address, it.address) } ?: emptyList() }.getOrDefault(emptyList())
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
            Text("Presence", style = MaterialTheme.typography.titleLarge)
            TextButton(onClick = { m.showPresence = false }) { Text("Done") }
        }
        Text("Home Wi-Fi networks", style = MaterialTheme.typography.titleMedium)
        Text("While connected to one of these, nothing counts and GPS is off.", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        home.forEach { n ->
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(n.ssid, Modifier.weight(1f))
                IconButton(onClick = { m.settings.removeHome(n.ssid); home = m.settings.homeNetworks; m.evaluatePresence() }) { Icon(ApgoIcons.Close, contentDescription = "Remove ${n.ssid}") }
            }
        }
        OutlinedButton(onClick = {
            val w = m.monitor.currentNetwork()
            val ssid = PresenceSignals.cleanSsid(w?.ssid)
            if (ssid == null) m.status = "Connect to your home Wi-Fi first (and allow location)"
            else { m.settings.addHome(HomeNetwork(ssid, w?.bssid)); home = m.settings.homeNetworks; m.evaluatePresence() }
        }) { Text("Add current network") }
        HorizontalDivider()
        Text("Car Bluetooth", style = MaterialTheme.typography.titleMedium)
        Text("While one of these devices is connected, nothing counts (no pickups while driving).", fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (!btOk) OutlinedButton(onClick = { askBt.launch(Manifest.permission.BLUETOOTH_CONNECT) }) { Text("Allow Bluetooth to pick your car") }
        paired.forEach { d ->
            val on = car.any { it.address == d.address }
            Row(Modifier.fillMaxWidth().clickable { toggleCar(m, d, !on) { car = it } }, verticalAlignment = Alignment.CenterVertically) {
                Checkbox(on, { toggleCar(m, d, it) { updated -> car = updated } })
                Text(d.name)
            }
        }
    }
}

private fun toggleCar(m: AppModel, d: CarDevice, on: Boolean, done: (List<CarDevice>) -> Unit) {
    val now = m.settings.carDevices
    m.settings.setCar(if (on) now.filterNot { it.address == d.address } + d else now.filterNot { it.address == d.address })
    done(m.settings.carDevices)
    m.evaluatePresence()
}
```
- [ ] **Step 4: Build, test, commit** (`feat: add presence settings and status chip`).

---

### Task 20: Verify Part B, document, close the issue

- [ ] **Step 1: Emulator home rule.** The emulator reports the Wi-Fi name `AndroidWifi`. Open Presence settings, tap "Add current network" (permit location when asked). Open a game: the chip must read "At home, paused" after about 45 s, `adb logcat`/`scripts/pull_diag.sh` must show `presence AtHome`, and the diag heartbeat must show no new fixes while the emulator is fed `adb emu geo fix`. Remove the network: the chip returns to "Tracking" after about 45 s and fixes resume.
- [ ] **Step 2: Counting really stops.** While "At home", complete nothing (feed fixes at a quest target, none are processed); after removing the network the same fix completes the quest.
- [ ] **Step 3: Docs.** Update `docs/context/outdoor-test-plan.md` with the presence checklist (add home Wi-Fi, tag the car, what each chip means, and that Bluetooth and the outside-zone duty cycle are untested until an outdoor run) and `docs/context/v1-architecture-and-status.md` (state machine, counting flag, settings storage). Add the presence signals to the diagnostics section of the test plan (`presence` log lines, heartbeat fields).
- [ ] **Step 4: Commit and close.**
```bash
git add -A && git commit -m "docs: describe presence and chains for testing" -m "Closes #6" -m "Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
gh issue close 6 --comment "Implemented: PresencePolicy, home Wi-Fi, car Bluetooth, zone-based GPS duty cycle (see spec Part B)."
```
- [ ] **Step 5: Install on the Pixel** (`adb -s adb-3A131FDJG001L0-xa29Iw._adb-tls-connect._tcp install -r ...`) and hand over the outdoor checklist: home Wi-Fi saved, car tagged, chips as expected, then `scripts/pull_diag.sh` after the walk.
