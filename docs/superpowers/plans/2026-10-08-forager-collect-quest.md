# Forager Collect Quest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to
> implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A new courier-family quest kind `forager`: the map shows 2N themed items on the street and path network, the player picks
them up (25 m) and banks what they carry on each arrival home; the quest is done once N are banked.

**Architecture:** Core (Rust) gets a catalog verify type `Collect`, a target `Target::Collect`, placement from the zone's street pool
(reusing `near_path::PathIndex` from PR #68), a tracker state that picks and banks, a persisted per-game progress map
(`Game::collected`), a Shuffle-trap path that moves only the unpicked items, and `Game::bank_at_home` for joining home Wi-Fi. FFI adds a `CollectOut` record on `QuestOut`;
Android draws one themed pin per unpicked item, a progress row text and an item list in the quest popup.

**Tech Stack:** Rust (apgo-core, apgo-ffi, UniFFI 0.32, serde), Kotlin/Compose, MapLibre, Lucide icons 2.2.1, Python (catalog doc
generator).

**Spec:** `docs/superpowers/specs/2026-10-08-forager-collect-quest-design.md` (owner-approved 2026-10-08; edited in commit `a6c2dc4` so
only the Shuffle trap re-places items).

**Execution prerequisite:** start only after PR #68 (`fix/quest-points-near-paths`) is merged and this branch is rebased on `main`.
PR #68 grew while under review: `PathIndex::with_segments`, `within`, `nearest_on_path`, `snap_into_area_where`, `share_near`;
`Atlas::street_runs`, `rough_runs`, `street_links(rough)`; `verify::LINE_SAMPLE_M`; and `ZonePaths::new(z, p: &AssignParams)` builds
its index as `PathIndex::with_segments(&pool, &links)` from `street_links(false)` plus `street_links(true)` when `uses_rough(z, surface)`.
"Near a path" now means within `NEAR_PATH_M` of a street segment, not only of a sample. Forager placement uses that segment-aware index
(`near_path` / `within`), built the same way through a helper that Task 3 extracts from `ZonePaths::new`. This plan was checked against
`git show origin/fix/quest-points-near-paths:core/src/assign.rs` at `2772649`; before Task 3, check it again against merged `main`
(`street_pool`, `uses_rough`, `ZonePaths::new`, `free_candidate`, `one`, `anchor`, and the test helpers `realm`, `atlas`, `params`,
`slot`, `at`, `must_reach`, `gap_to_paths`) and adapt names that changed before writing code. The player's Reroll button is being
removed in a separate PR: this plan has no player-reroll UI or tests; `Game::reroll` and `Engine::reroll` remain the Shuffle trap's path.

**Execution method:** subagent-driven (owner's choice).

## Global Constraints

- Kind `forager`, family `courier`, modes walk, run, bike (not drive); `Kind::is_progressive()` stays false.
- Verify: `Collect { need_by_tier: [3, 5, 7, 10], spare_factor: 2, pick_r_m: 25 }`. Map shows `2 * need` items.
- Every item lies within 30 m (`NEAR_PATH_M`) of a road or path, at least 60 m from every other item, inside the zone; placed from
  `atlas.streets` and `streets_rough` (via `street_pool`), never from a grid. Check spacing and min distance from home on the FINAL points.
- Pool too small for `2 * need` items: no forager in that slot (another courier kind is used).
- Banking: on each accepted fix within the home radius (`HOME_RADIUS_M`, 100 m, the one round trips use), and when the phone joins home
  Wi-Fi (presence enters `AtHome`, even with no GPS fix): `banked += carried`, `carried = 0`. Done when `banked >= need`. Progress
  `min(1, (banked + 0.5 * carried) / need)`. Home Wi-Fi banking is `Game::bank_at_home(t_ms)` / `Engine::bank_at_home(t_ms)`: every
  forager, completes quests that reach the need, idempotent, blocked by a trap that blocks checks.
- Inaccurate fixes, impossible jumps, counting off (home Wi-Fi, car) and a Freeze trap that blocks checks: no pickups, no fix banking.
  Carried items are never lost while counting is off.
- Shuffle trap: only unpicked items move; `carried`, `banked`, `need`, picked items and theme stay.
- Title: `"Forager: bring home {need} {theme}"`; theme random per quest from a short list (flavour only).
- No apworld, YAML or slot_data change (schema 3 stays). No CLAUDE.md edits.
- TDD: failing test first, see it fail, then implement. Never edit a test to fit bad code (a test is only changed where this feature
  changes the contract it pins, and the task says so).
- Gates: `just check-rust`, `just check-android`, `just check-hygiene` must pass; coverage floors (rust 80, kotlin 7) only go up.
- One design system (`ui/`): colours from `ApgoPalette`, icons from `ApgoIcons` (Lucide), pins only via `MapMarkers.render(MarkerSpec)`,
  help copy only in `ui/HelpText.kt`.
- Commits: conventional, subject under 50 chars, imperative, body lines under 72, body ends with `Refs #5` and the trailer
  `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push without the owner.
- `docs/context/quest-catalog.md` is generated: run `python3 scripts/catalog_doc.py` after editing `core/data/quest_catalog.json`.

## Review Focus

1. Walking home onto home Wi-Fi switches counting off before a GPS fix inside the home radius is accepted: joining home Wi-Fi must bank
   what is carried exactly once, a trap that blocks checks must block that bank, and carried items must never be lost while counting is
   off. Tests: Task 5 step 1 (`carried_items_wait_out_home_wifi_and_bank_on_the_next_accepted_home_fix`), Task 7 step 1, Task 11 step 1.
2. Fog of war hides a quest until its anchor is near, and a hidden quest's tracker is never updated: the anchor must be the item nearest
   home so the quest is revealed on the way to its first item. Test: Task 3 step 1, items sorted nearest first.
3. An item inside or next to the home radius would be picked and banked in one fix (a free quest): every item must be at least
   `max(min_distance_m, HOME_RADIUS_M + pick_r_m)` (125 m) from home, even when the slot's min distance is 0. Test: Task 3 step 1
   (`p.min_distance_m = 0.0`).
4. Slots have tiers 1 to 10 but the spec gives needs for tiers 1 to 4 only: tiers above 4 must use the last value (10), never panic or
   give 0. Test: Task 3 step 1 (tier 7 case).
5. A Shuffle trap in a zone that can no longer supply new points (atlas emptied or rescanned sparse) must leave the quest and its counts
   as they are, without error. Tests: Task 4 step 1 (`a_zone_with_too_few_street_points_leaves_a_forager_quest_as_it_is`) and Task 6 step 1.

---

## File Structure

| File | Change | Responsibility |
| --- | --- | --- |
| `core/src/catalog.rs` | modify | `Verify::Collect`, `FORAGE_THEMES`, `how()` text |
| `core/data/quest_catalog.json` | modify | the `forager` kind |
| `docs/context/quest-catalog.md` | regenerate | generated catalog table |
| `core/src/assign.rs` | modify | `Target::Collect`, `goal_text`, `zone_index`, `place_items`, `free_candidate` arm, `quest_title`, `replace_unpicked` |
| `core/src/verify.rs` | modify | `HOME_RADIUS_M` (pub), `Collected`, `State::Collect`, `collect_progress`, `Tracker::with_collected`, `Tracker::collected` |
| `core/src/fog.rs` | modify | `anchor` for `Target::Collect` |
| `core/src/game.rs` | modify | `Game::collected` (persisted), tracker resume/sync, `QuestView::collected`, progress after load, `reach_radius`, Shuffle path in `reroll`, `bank_at_home` |
| `core/examples/play_sim.rs` | modify | autoplay arm for `Target::Collect` |
| `core/ffi/src/engine.rs` | modify | `CollectOut`, `CollectItemOut`, `QuestOut::collect`, `describe` arm, anchor = first unpicked item, `Engine::bank_at_home` |
| `android/app/src/main/java/dev/apgo2/presence/PresencePolicy.kt` | modify | `PresencePolicy.arrivedHome(before, after)` (pure) |
| `android/app/src/main/java/dev/apgo2/PresenceController.kt` | modify | bank on the transition to `AtHome` |
| `android/app/src/main/java/dev/apgo2/AppModel.kt` | modify | `AppModel.bankAtHome()` |
| `android/app/src/main/java/dev/apgo2/ui/CollectFormat.kt` | create | pure text for the row and the item list |
| `android/app/src/main/java/dev/apgo2/PlayDetails.kt` | modify | row text and the item list in the popup |
| `android/app/src/main/java/dev/apgo2/ui/HelpText.kt` | modify | courier family help mentions foraging |
| `android/app/src/main/java/dev/apgo2/ui/ApgoIcons.kt` | modify | `collectible(theme)` icons, `forager` kind icon |
| `android/app/src/main/java/dev/apgo2/ui/MapMarkers.kt` | modify | `MarkerSpec.Item` |
| `android/app/src/main/java/dev/apgo2/MapGeoJson.kt` | modify | item pins, `questImages`, `QuestOut.tapPoints` |
| `android/app/src/main/java/dev/apgo2/QuestMap.kt` | modify | ensure item images |
| `android/app/src/main/java/dev/apgo2/PlayScreen.kt` | modify | taps on item pins select the quest |
| `android/app/src/main/java/dev/apgo2/DevSimulator.kt` | modify | simulate a forager walk |
| `docs/context/v1-architecture-and-status.md`, `docs/context/ui-design-system.md` | modify | record what exists |

---

### Task 1: Catalog verify type `Collect` and the theme list

**Files:**

- Modify: `core/src/catalog.rs`

**Interfaces:**

- Produces: `catalog::Verify::Collect { need_by_tier: Vec<u32>, spare_factor: u32, pick_r_m: f64 }` (serde tag `"collect"`);
  `pub const FORAGE_THEMES: [&str; 7]` in `catalog.rs`.

- [ ] **Step 1: Write the failing tests** (add to `mod tests` in `core/src/catalog.rs`)

```rust
    #[test]
    fn a_collect_rule_reads_from_json_and_explains_itself() {
        let v: Verify = serde_json::from_str(r#"{"type":"collect","need_by_tier":[3,5,7,10],"spare_factor":2,"pick_r_m":25}"#).unwrap();
        assert_eq!(v, Verify::Collect { need_by_tier: vec![3, 5, 7, 10], spare_factor: 2, pick_r_m: 25.0 });
        assert_eq!(v.how(), "Pick things up around the area and bring enough of them home.");
    }

    #[test]
    fn forage_themes_are_distinct_lowercase_plurals() {
        let mut seen = std::collections::BTreeSet::new();
        for t in FORAGE_THEMES {
            assert!(t.ends_with('s') && t.chars().all(|c| c.is_ascii_lowercase()), "{t}");
            assert!(seen.insert(t), "{t} twice");
        }
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core catalog::tests`
Expected: compile error, no variant `Collect` / cannot find `FORAGE_THEMES`.

- [ ] **Step 3: Implement**

In `enum Verify`, after `Away { .. }` and before `Boss`:

```rust
    /// Pick up items around the area and bring enough of them home.
    Collect {
        /// Items to bring home, by effort tier (tier 1 first; higher tiers use the last value).
        need_by_tier: Vec<u32>,
        /// How many items the map shows per item needed.
        spare_factor: u32,
        /// How close counts as picked up, in metres.
        pick_r_m: f64,
    },
```

In `Verify::how`, before `Self::Boss`:

```rust
            Self::Collect { .. } => "Pick things up around the area and bring enough of them home.".to_string(),
```

After `fn metres`:

```rust
/// What a forager quest's items can be: flavour only. The app has an icon for each (`ApgoIcons.collectible`); an unknown one falls back
/// to the courier icon.
pub const FORAGE_THEMES: [&str; 7] = ["pinecones", "shells", "acorns", "leaves", "feathers", "clovers", "gems"];
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd core && cargo test -p apgo-core catalog::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/catalog.rs
git commit -m "feat: add collect verify type" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `Target::Collect` and the picking and banking tracker

**Files:**

- Modify: `core/src/assign.rs` (enum `Target`, `goal_text`, `anchor`, test helper `must_reach`, `goal_text_tests`)
- Modify: `core/src/verify.rs`
- Modify: `core/src/fog.rs` (`anchor`)
- Modify: `core/ffi/src/engine.rs` (`describe`, exhaustive match)
- Modify: `core/examples/play_sim.rs` (exhaustive match)

**Interfaces:**

- Consumes: nothing new.
- Produces:
  - `assign::Target::Collect { pts: Vec<Point>, need: u32, r: f64, theme: String }`
  - `verify::HOME_RADIUS_M: f64 = 100.0` (now `pub`)
  - `verify::Collected { picked: BTreeSet<u16>, carried: u32, banked: u32 }` (`Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize`)
  - `verify::collect_progress(c: &Collected, need: u32) -> f32`
  - `Tracker::with_collected(target: Target, home: Point, saved: Collected) -> Tracker`
  - `Tracker::collected(&self) -> Option<&Collected>`
  - `fog::anchor(&Target::Collect { pts, .. }) == pts.first()`

- [ ] **Step 1: Write the failing tests**

In `core/src/verify.rs` `mod tests`, add (`BTreeSet` is already in scope through `use super::*`):

```rust
    /// A forager target: `n` items 100 m apart going north from 500 m.
    fn collect(need: u32, n: u32) -> Target {
        let pts = (0..n).map(|i| destination(home(), 0.0, 500.0 + 100.0 * f64::from(i))).collect();
        Target::Collect { pts, need, r: 25.0, theme: "acorns".into() }
    }

    fn items(t: &Target) -> Vec<Point> {
        let Target::Collect { pts, .. } = t else { panic!("not a collect target") };
        pts.clone()
    }

    #[test]
    fn collect_picks_each_item_once_within_its_radius_and_ignores_blurry_fixes() {
        let t = collect(3, 6);
        let pts = items(&t);
        let mut tr = Tracker::new(t, home());
        tr.update(&Fix { accuracy_m: 50.0, ..fix(pts[0], 0) }, None);
        assert_eq!(tr.collected().unwrap().carried, 0, "a blurry fix picks nothing");
        tr.update(&fix(destination(pts[0], 90.0, 26.0), 10), None);
        assert_eq!(tr.collected().unwrap().carried, 0, "26 m is outside the 25 m pickup radius");
        tr.update(&fix(destination(pts[0], 90.0, 24.0), 20), None);
        tr.update(&fix(pts[0], 30), None);
        let c = tr.collected().unwrap();
        assert_eq!((c.carried, c.banked), (1, 0), "one pickup per item");
        assert_eq!(c.picked, BTreeSet::from([0]));
        assert!(matches!(tr.status(), Status::Active(p) if (p - 0.5 / 3.0).abs() < 1e-6), "carried counts half: {:?}", tr.status());
    }

    #[test]
    fn collect_banks_on_each_arrival_home_and_is_done_exactly_at_the_need() {
        let t = collect(3, 6);
        let pts = items(&t);
        let mut tr = Tracker::new(t, home());
        tr.update(&fix(pts[0], 0), None);
        tr.update(&fix(pts[1], 60), None);
        tr.update(&fix(destination(home(), 0.0, 150.0), 120), None);
        assert_eq!(tr.collected().unwrap().banked, 0, "nothing is banked away from home");
        tr.update(&fix(destination(home(), 0.0, 90.0), 180), None);
        let c = tr.collected().unwrap();
        assert_eq!((c.carried, c.banked), (0, 2), "partial banking");
        tr.update(&fix(home(), 240), None);
        assert_eq!(tr.collected().unwrap().banked, 2, "being home again banks nothing new");
        // second outing: banked 2 + carried 1 reaches the need, but only banking finishes it
        assert_ne!(tr.update(&fix(pts[2], 300), None), Status::Done, "carrying enough is not done");
        tr.update(&fix(pts[3], 360), None);
        assert_eq!(tr.update(&fix(home(), 420), None), Status::Done);
        assert_eq!(tr.collected().unwrap().banked, 4);
    }

    #[test]
    fn collect_keeps_what_it_carries_through_a_pause_and_resumes_from_a_save() {
        let t = collect(3, 6);
        let pts = items(&t);
        let mut tr = Tracker::new(t.clone(), home());
        tr.update(&fix(pts[0], 0), None);
        tr.pause();
        assert_eq!(tr.collected().unwrap().carried, 1, "a pause (home Wi-Fi, car) keeps what you carry");
        let saved = Collected { picked: BTreeSet::from([0, 1]), carried: 1, banked: 1 };
        let back = Tracker::with_collected(t, home(), saved.clone());
        assert_eq!(back.collected(), Some(&saved));
        assert!(matches!(back.status(), Status::Active(p) if (p - 0.5).abs() < 1e-6), "(1 + 0.5) / 3");
        assert!(Tracker::new(Target::Point { p: home(), r: 40.0 }, home()).collected().is_none());
        assert!((collect_progress(&Collected { banked: 9, ..Collected::default() }, 3) - 1.0).abs() < f32::EPSILON, "capped at 1");
    }
```

In `core/src/assign.rs` `goal_text_tests::every_target_kind_says_what_to_do`, add to `cases`:

```rust
            (Target::Collect { pts: vec![p], need: 5, r: 25.0, theme: "acorns".into() }, "Bring home 5 acorns (pick up within 25 m)"),
```

In `core/src/fog.rs` `mod tests`, add:

```rust
    #[test]
    fn a_forager_quest_hides_behind_its_first_item() {
        let (a, b) = (destination(Point::new(40.0, -111.0), 0.0, 300.0), destination(Point::new(40.0, -111.0), 0.0, 600.0));
        assert_eq!(anchor(&Target::Collect { pts: vec![a, b], need: 1, r: 25.0, theme: "shells".into() }), Some(a));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core`
Expected: compile errors (`Target::Collect`, `Collected`, `collect_progress`, `with_collected`, `collected` not found).

- [ ] **Step 3: Implement**

`core/src/assign.rs`, in `enum Target` after `Away { .. }`:

```rust
    /// Pick up items around home and bring enough of them home (forager).
    Collect {
        /// Where the items lie: nearest home first. Indexes are stable (saved progress refers to them).
        pts: Vec<Point>,
        /// How many items must be brought home.
        need: u32,
        /// How close counts as picked up, in metres.
        r: f64,
        /// What the items are ("pinecones"), flavour only.
        theme: String,
    },
```

In `Target::goal_text`:

```rust
            Self::Collect { need, r, theme, .. } => format!("Bring home {need} {theme} (pick up within {r:.0} m)"),
```

In the private `fn anchor(t: &Target)` of assign.rs (exhaustive after #68), add before the `Cells | Steps | Away` arm:

```rust
        Target::Collect { pts, .. } => pts.first().copied(),
```

In the `tests` module's `must_reach` helper (assign.rs), add an arm (every item must be reachable):

```rust
            Target::Collect { pts, .. } => pts.clone(),
```

`core/src/fog.rs` `anchor`, add before `_ => None`:

```rust
        Target::Collect { pts, .. } => pts.first().copied(),
```

`core/src/verify.rs`:

- Add `use serde::{Deserialize, Serialize};`.
- Replace `const HOME_RADIUS_M: f64 = 100.0;` with:

```rust
/// How close to home counts as home: for round trips, and where a forager banks what it carries, in metres.
pub const HOME_RADIUS_M: f64 = 100.0;
```

- After `MAX_GAP_MS`:

```rust
/// Saved progress of a collect (forager) quest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collected {
    /// Indexes (into the quest's items) of the items picked up so far.
    pub picked: BTreeSet<u16>,
    /// Items picked up and not yet brought home.
    pub carried: u32,
    /// Items brought home so far.
    pub banked: u32,
}

/// Progress of a collect quest: what is banked, plus half of what is carried, out of `need` (at most 1).
#[must_use]
pub fn collect_progress(c: &Collected, need: u32) -> f32 {
    to_f32(((f64::from(c.banked) + 0.5 * f64::from(c.carried)) / f64::from(need.max(1))).min(1.0))
}
```

- `enum State`: add `Collect(Collected),`.
- `Tracker::new`: add `Target::Collect { .. } => State::Collect(Collected::default()),`.
- After `Tracker::new`:

```rust
    /// A tracker for a collect quest that carries on from saved progress (a reopened game or a moved quest).
    #[must_use]
    pub fn with_collected(target: Target, home: Point, saved: Collected) -> Self {
        let mut t = Self::new(target, home);
        if let (Target::Collect { need, .. }, State::Collect(c)) = (&t.target, &mut t.state) {
            *c = saved;
            t.progress = collect_progress(c, *need);
            t.done = c.banked >= *need;
        }
        t
    }

    /// The progress of a collect quest (`None` for any other quest).
    #[must_use]
    pub fn collected(&self) -> Option<&Collected> {
        match &self.state {
            State::Collect(c) => Some(c),
            _ => None,
        }
    }
```

- `Tracker::update`, before `_ => {}`:

```rust
            (Target::Collect { pts, need, r, .. }, State::Collect(c)) => {
                for (i, q) in pts.iter().enumerate() {
                    let Ok(i) = u16::try_from(i) else { break };
                    if distance_m(p, *q) <= *r && c.picked.insert(i) {
                        c.carried += 1;
                    }
                }
                if distance_m(p, self.home) <= HOME_RADIUS_M {
                    c.banked += std::mem::take(&mut c.carried);
                }
                self.progress = collect_progress(c, *need);
                self.done = c.banked >= *need;
            }
```

(`pause` needs no change: what is carried is progress, not a timer.)

`core/ffi/src/engine.rs` `describe`, add:

```rust
        Target::Collect { pts, r, .. } => ("collect", pts.first().copied(), None, *r, vec![], text),
```

and update the `QuestOut::shape` doc comment to `/// point | dwell | area | line | courier | roundtrip | collect | cells | steps | away`.

`core/examples/play_sim.rs`, in the `match &q.target`, add:

```rust
            Target::Collect { pts, need, .. } => {
                for q in pts.iter().take(*need as usize) {
                    at(*q, 700, None);
                }
                at(home, 900, None);
            }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add core/src/assign.rs core/src/verify.rs core/src/fog.rs core/ffi/src/engine.rs core/examples/play_sim.rs
git commit -m "feat: track forager pickups and banking" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Place forager quests and add the kind to the catalog

Re-check `core/src/near_path.rs` and `core/src/assign.rs` on merged `main` first (see the prerequisite in the header). Placement uses the
segment-aware `PathIndex` built exactly as `ZonePaths::new` builds it, so this task first extracts that into `zone_index`.

**Files:**

- Modify: `core/src/assign.rs` (`zone_index`, `ZonePaths::new`, `place_items`, `free_candidate`, `quest_title`, `one`, tests)
- Modify: `core/data/quest_catalog.json`
- Modify: `core/src/catalog.rs` (test)
- Regenerate: `docs/context/quest-catalog.md`

**Interfaces:**

- Consumes: `Verify::Collect`, `FORAGE_THEMES` (Task 1); `Target::Collect`, `HOME_RADIUS_M` (Task 2); `PathIndex::with_segments`,
  `PathIndex::near_path`, `NEAR_PATH_M`, `Atlas::street_links`, `uses_rough`, `street_pool`, `ZonePaths::new` (#68); `free_candidate`,
  `one` (assign.rs); `effort::{dist_for, mid, tier_for, travel_min}`; test helpers `gap_to_paths(p, &Atlas)`, `at`, `realm`, `atlas`,
  `params`, `slot`.
- Produces: `fn zone_index(z: &ZoneCtx<'_>, pool: &[Point], surface: SurfacePref) -> PathIndex` and
  `fn place_items(pool: &[Point], index: &PathIndex, home: Point, min_m: f64, far_m: f64, n: usize, keep: &[Point], rng: &mut StdRng) -> Option<Vec<Point>>`
  (both private, used by Task 4); `const ITEM_SPACING_M: f64 = 60.0`; forager `Assignment`s with `kind_id == "forager"`,
  `quest_name == "Forager: bring home {need} {theme}"`, `place == "Around home"`.

- [ ] **Step 1: Write the failing tests**

`core/src/catalog.rs` `mod tests`:

```rust
    #[test]
    fn forager_is_a_courier_kind_for_walk_run_and_bike_with_a_collect_rule() {
        let c = Catalog::builtin();
        let k = c.kind("forager").expect("forager is in the catalog");
        assert_eq!((k.family.as_str(), k.name.as_str(), k.geom), ("courier", "Forager", Geom::None));
        assert_eq!(k.modes, [Mode::Walk, Mode::Run, Mode::Bike]);
        assert_eq!(k.verify, Verify::Collect { need_by_tier: vec![3, 5, 7, 10], spare_factor: 2, pick_r_m: 25.0 });
        assert!(!k.is_progressive());
    }
```

`core/src/assign.rs` `mod tests` (after `a_big_park_gets_a_point_on_its_path_or_is_not_used`):

```rust
    /// Street points every 30 m in both directions out to `half_m` from home: room for any forager.
    fn fine_streets(half_m: f64) -> Vec<Point> {
        let n = (half_m / 30.0) as i32;
        (-n..=n).flat_map(|i| (-n..=n).map(move |j| at(home(), f64::from(i) * 30.0, f64::from(j) * 30.0))).collect()
    }

    #[test]
    fn forager_places_twice_the_need_spaced_near_paths_away_from_home_and_spread_outward() {
        let cat = Catalog::builtin();
        let k = cat.kind("forager").unwrap();
        let r = realm(Mode::Walk);
        let a = crate::scan::build_atlas("r", 0, vec![], fine_streets(3700.0), &cat);
        for (mode, tier, need) in [(Mode::Walk, 1, 3), (Mode::Walk, 2, 5), (Mode::Run, 3, 7), (Mode::Walk, 4, 10), (Mode::Bike, 3, 7), (Mode::Walk, 7, 10)] {
            let z = ZoneCtx { zone: 1, mode, realm: &r, atlas: &a };
            let pool = street_pool(&z, SurfacePref::Any);
            let mut p = params(3);
            p.min_distance_m = 0.0; // the home-radius floor must hold on its own (Review Focus 3)
            let want = mid(tier, p.minutes_per_tier);
            let (t, effort, place) = free_candidate(k, &z, &pool, &p, want, &mut StdRng::seed_from_u64(3), &[]).expect("a dense town has room");
            let Target::Collect { pts, need: n, r: pick, theme } = &t else { panic!("{t:?}") };
            assert_eq!(*n, need, "{mode:?} tier {tier} (tiers above 4 use the last value)");
            assert_eq!(pts.len(), 2 * need as usize);
            assert!((pick - 25.0).abs() < f64::EPSILON);
            assert!(crate::catalog::FORAGE_THEMES.contains(&theme.as_str()), "{theme}");
            assert_eq!(place, "Around home");
            let floor = crate::verify::HOME_RADIUS_M + 25.0;
            for (i, q) in pts.iter().enumerate() {
                assert!(gap_to_paths(*q, &a) <= NEAR_PATH_M + 1e-6, "item {i} is off the paths");
                assert!(pool.contains(q), "item {i} is not a street point of the zone");
                assert!(distance_m(home(), *q) >= floor, "item {i} is {:.0} m from home", distance_m(home(), *q));
                for o in &pts[..i] {
                    assert!(distance_m(*o, *q) >= 60.0 - 1e-6, "items {:.0} m apart", distance_m(*o, *q));
                }
            }
            let d: Vec<f64> = pts.iter().map(|q| distance_m(home(), *q)).collect();
            assert!(d.windows(2).all(|w| w[0] <= w[1]), "nearest first, so fog reveals the quest on the way (Review Focus 2)");
            let far = crate::effort::dist_for(want / 2.0, mode).max(floor + 120.0);
            let last = d[d.len() - 1];
            assert!((0.8 * far..=1.5 * far).contains(&last), "{mode:?} tier {tier}: farthest {last:.0} m, wanted about {far:.0} m");
            if far > 600.0 {
                assert!(d[0] < 0.6 * last, "spread between home and the farthest: {d:?}");
            }
            assert!((effort - 2.0 * travel_min(last, mode)).abs() < 1e-9, "effort is out to the farthest and back");
        }
    }

    #[test]
    fn a_sparse_zone_gets_no_forager_and_falls_back_to_another_courier_kind() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let streets: Vec<Point> = (0..8).map(|i| at(home(), 0.0, 600.0 + 60.0 * f64::from(i))).collect();
        let a = crate::scan::build_atlas("r", 0, vec![], streets, &cat);
        let z = ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a };
        let pool = street_pool(&z, SurfacePref::Any);
        let k = cat.kind("forager").unwrap();
        assert!(free_candidate(k, &z, &pool, &params(1), 25.0, &mut StdRng::seed_from_u64(1), &[]).is_none(), "8 points cannot hold 6 items 60 m apart");
        let slots: Vec<SlotIn> = (1..=8).map(|i| slot(i, "courier", 1 + (i % 4) as u8, Mode::Walk)).collect();
        for seed in 1..=4 {
            let out = assign(&slots, &[ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }], &cat, &params(seed));
            assert_eq!(out.len(), slots.len());
            assert!(out.iter().all(|o| o.kind_id != "forager"), "seed {seed}");
        }
    }

    #[test]
    fn forager_quests_join_the_courier_family_with_a_themed_title_but_never_drive() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, false));
        let mut seen = 0;
        for seed in 1..=8 {
            let zones = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }, ZoneCtx { zone: 2, mode: Mode::Drive, realm: &r, atlas: &a }];
            let mut slots: Vec<SlotIn> = (1..=4).map(|t| slot(i64::from(t), "courier", t, Mode::Walk)).collect();
            slots.extend((1..=4).map(|t| SlotIn { location_id: 10 + i64::from(t), zone: 2, mode: Mode::Drive, family: "courier".into(), tier: t, boss: false }));
            for o in assign(&slots, &zones, &cat, &params(seed)) {
                if o.kind_id != "forager" {
                    continue;
                }
                assert_eq!(o.zone, 1, "no forager in a drive zone");
                let Target::Collect { need, theme, .. } = &o.target else { panic!("{:?}", o.target) };
                assert_eq!(o.quest_name, format!("Forager: bring home {need} {theme}"));
                seen += 1;
            }
        }
        assert!(seen > 0, "forager is offered to courier slots");
    }
```

Contract change in an existing test (the courier family now has a third shape): in `parks_trails_and_courier_produce_the_right_target_shapes`, change the courier assertion to

```rust
        assert!(matches!(out[2].target, Target::Courier { .. } | Target::RoundTrip { .. } | Target::Collect { .. }));
```

The `use` lines in the assign tests module need `crate::effort::mid` (add `use crate::effort::{mid, tier_for};` replacing the existing
`use crate::effort::tier_for;`).

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core forager`
Expected: FAIL: `forager is in the catalog` panics; the assign tests panic on `cat.kind("forager").unwrap()`.

- [ ] **Step 3: Implement**

Add the kind (keeps the file's exact formatting: one-space indent, no trailing newline; check with `git diff`):

```bash
python3 - <<'EOF'
import json
import pathlib

p = pathlib.Path("core/data/quest_catalog.json")
c = json.loads(p.read_text(encoding="utf-8"))
i = next(i for i, k in enumerate(c["kinds"]) if k["id"] == "there_and_back")
c["kinds"].insert(i + 1, {
    "id": "forager",
    "name": "Forager",
    "family": "courier",
    "blurb": "Gather things scattered around your area and bring enough of them home.",
    "geom": "none",
    "any_of": [],
    "require_name": False,
    "verify": {"type": "collect", "need_by_tier": [3, 5, 7, 10], "spare_factor": 2, "pick_r_m": 25},
    "modes": ["walk", "run", "bike"],
    "min_features": 0,
})
p.write_text(json.dumps(c, indent=1, ensure_ascii=False), encoding="utf-8")
EOF
python3 scripts/catalog_doc.py
```

`core/src/assign.rs`:

- Imports: `use crate::catalog::{Catalog, Geom, Kind, Mode, Verify, FORAGE_THEMES};`,
  `use crate::effort::{cadence_steps_per_min, dist_for, mid, tier_for, travel_min};`, `use crate::num::{count_f64, round_u32};`,
  `use crate::verify::HOME_RADIUS_M;`.
- Extract the index building out of `ZonePaths::new` (no behaviour change; `ZonePaths::new` then calls it as
  `let index = zone_index(z, &pool, p.surface);`):

```rust
/// The segment-aware path index of zone `z` over `pool`: its street points and the streets between them (rough ones when the surface
/// preference uses them).
fn zone_index(z: &ZoneCtx<'_>, pool: &[Point], surface: SurfacePref) -> PathIndex {
    let mut links = z.atlas.street_links(false);
    if uses_rough(z, surface) {
        links.extend(z.atlas.street_links(true));
    }
    PathIndex::with_segments(pool, &links)
}
```

- After `const FAVORITE_BONUS_MIN`:

```rust
/// Least distance between two forager items (and between an item and another quest's point), in metres.
const ITEM_SPACING_M: f64 = 60.0;
/// How many shuffled street points a forager placement looks at.
const MAX_ITEM_CANDIDATES: usize = 2000;

/// `n` street points for forager items, spread outward from `home`: the k-th of them (counting from 1) about k/n of the way from `min_m`
/// to `far_m`, each at least [`ITEM_SPACING_M`] from the others and from `keep`. The rules are checked again on the final points (near a
/// street segment by `index`, at least `min_m` from home, spaced), never only on candidates. `None` when the pool cannot supply `n` such points.
#[allow(clippy::too_many_arguments)] // like free_candidate: the zone's pool and index, home, the band and the points to keep apart from
fn place_items(pool: &[Point], index: &PathIndex, home: Point, min_m: f64, far_m: f64, n: usize, keep: &[Point], rng: &mut StdRng) -> Option<Vec<Point>> {
    if n == 0 {
        return Some(Vec::new());
    }
    let far_m = far_m.max(min_m + 2.0 * ITEM_SPACING_M);
    let mut ring: Vec<(Point, f64)> = pool.iter().map(|q| (*q, distance_m(home, *q))).filter(|(_, d)| (min_m..=far_m * 1.5).contains(d)).collect();
    ring.shuffle(rng);
    ring.truncate(MAX_ITEM_CANDIDATES);
    let mut out: Vec<Point> = Vec::with_capacity(n);
    // The farthest first: it is the hardest to fit.
    for k in (1..=n).rev() {
        let want = min_m + (far_m - min_m) * count_f64(k) / count_f64(n);
        let spaced = |q: Point| keep.iter().chain(&out).all(|o| distance_m(*o, q) >= ITEM_SPACING_M);
        let (q, _) = ring.iter().filter(|(q, _)| spaced(*q)).min_by(|a, b| (a.1 - want).abs().total_cmp(&(b.1 - want).abs()))?;
        out.push(*q);
    }
    let ok = out.iter().enumerate().all(|(i, q)| {
        distance_m(home, *q) >= min_m && index.near_path(*q) && keep.iter().chain(&out[..i]).all(|o| distance_m(*o, *q) >= ITEM_SPACING_M)
    });
    ok.then_some(out)
}

/// The forager items a slot needs: by tier, tiers past the table use its last value.
fn need_for(need_by_tier: &[u32], want: f64, minutes_per_tier: f64) -> Option<u32> {
    let tier = usize::from(tier_for(want, minutes_per_tier));
    need_by_tier.get(tier.min(need_by_tier.len()).checked_sub(1)?).copied().filter(|n| *n > 0)
}

/// A quest's display name: a forager names its count and theme.
fn quest_title(kind_name: &str, t: &Target) -> String {
    match t {
        Target::Collect { need, theme, .. } => format!("{kind_name}: bring home {need} {theme}"),
        _ => kind_name.to_string(),
    }
}
```

- In `free_candidate`, before `_ => None`:

```rust
        Verify::Collect { need_by_tier, spare_factor, pick_r_m } => {
            let need = need_for(need_by_tier, want, p.minutes_per_tier)?;
            let total = usize::try_from(need.saturating_mul(*spare_factor)).ok()?;
            // Never next to home: an item there would be picked and banked in the same step.
            let min_m = p.min_distance_m.max(HOME_RADIUS_M + pick_r_m);
            let index = zone_index(z, pool, p.surface);
            let mut pts = place_items(pool, &index, p.home, min_m, dist_for(want / 2.0, mode), total, used_pts, rng)?;
            pts.sort_by(|a, b| distance_m(p.home, *a).total_cmp(&distance_m(p.home, *b)));
            let theme = (*FORAGE_THEMES.choose(rng)?).to_string();
            let farthest = pts.last().map_or(0.0, |q| distance_m(p.home, *q));
            Some((Target::Collect { pts, need, r: *pick_r_m, theme }, 2.0 * travel_min(farthest, mode), "Around home".into()))
        }
```

- In `one`, after `if let Some(a) = anchor(&c.target) { used_pts.push(a); }`:

```rust
    if let Target::Collect { pts, .. } = &c.target {
        used_pts.extend_from_slice(&pts[1..]); // the first is the anchor, pushed above
    }
```

(`anchor` already returns `pts.first()` for a forager (Task 2); `pts[1..]` is safe because a placed forager has at least 6 items.)

- In `one`, replace the `(kind_id, quest_name)` lines with:

```rust
    let title = quest_title(&c.kind.name, &c.target);
    let (kind_id, quest_name) = if s.boss { ("the_big_one".to_string(), format!("The Big One: {title}")) } else { (c.kind.id.clone(), title) };
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings`
Expected: PASS (including the drift guard `a_kind_is_progressive_exactly_when_its_free_quest_is_a_chain_target`, which now also calls
`free_candidate` for forager, and `every_quest_point_of_every_kind_and_mode_is_near_a_path`, which now checks every forager item).
Then `git diff --stat core/data/quest_catalog.json docs/context/quest-catalog.md` shows only the added kind and the regenerated table
(77 kinds).

- [ ] **Step 5: Commit**

```bash
git add core/src/assign.rs core/src/catalog.rs core/data/quest_catalog.json docs/context/quest-catalog.md
git commit -m "feat: place forager quests" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Re-place a forager's unpicked items

**Files:**

- Modify: `core/src/assign.rs`

**Interfaces:**

- Consumes: `zone_index`, `place_items`, `ITEM_SPACING_M` (Task 3); `street_pool` (#68); `Target::Collect`, `HOME_RADIUS_M` (Task 2).
- Produces: `pub fn replace_unpicked(t: &Target, picked: &BTreeSet<u16>, z: &ZoneCtx<'_>, p: &AssignParams, tier: u8, rng: &mut StdRng) -> Option<Target>`.

- [ ] **Step 1: Write the failing tests** (assign.rs `mod tests`)

```rust
    #[test]
    fn replacing_unpicked_items_keeps_the_picked_ones_and_the_rules() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let a = atlas(&cat, false);
        let z = ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a };
        let pool = street_pool(&z, SurfacePref::Any);
        let p = params(5);
        let (t, _, _) = free_candidate(cat.kind("forager").unwrap(), &z, &pool, &p, mid(2, p.minutes_per_tier), &mut StdRng::seed_from_u64(5), &[]).unwrap();
        let Target::Collect { pts: old, need: n0, r: r0, theme: th0 } = &t else { panic!("{t:?}") };
        let picked = BTreeSet::from([0u16, 3]);
        let fresh = replace_unpicked(&t, &picked, &z, &p, 2, &mut StdRng::seed_from_u64(77)).expect("a dense zone has room");
        let Target::Collect { pts, need, r: pick, theme } = &fresh else { panic!("{fresh:?}") };
        assert_eq!((need, theme), (n0, th0));
        assert!((pick - r0).abs() < f64::EPSILON);
        assert_eq!(pts.len(), old.len());
        assert_eq!((pts[0], pts[3]), (old[0], old[3]), "picked items stay where they were");
        let moved = (0..pts.len()).filter(|i| !picked.contains(&(*i as u16)) && pts[*i] != old[*i]).count();
        assert!(moved > 0, "unpicked items move");
        for (i, q) in pts.iter().enumerate() {
            assert!(pool.contains(q) && distance_m(home(), *q) >= 150.0, "item {i}");
            for o in &pts[..i] {
                assert!(distance_m(*o, *q) >= 60.0 - 1e-6, "items {:.0} m apart", distance_m(*o, *q));
            }
        }
        assert!(replace_unpicked(&Target::Point { p: home(), r: 40.0 }, &picked, &z, &p, 2, &mut StdRng::seed_from_u64(1)).is_none(), "only foragers");
    }

    #[test]
    fn a_zone_with_too_few_street_points_leaves_a_forager_quest_as_it_is() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let streets: Vec<Point> = (0..8).map(|i| at(home(), 0.0, 600.0 + 60.0 * f64::from(i))).collect();
        let a = crate::scan::build_atlas("r", 0, vec![], streets.clone(), &cat);
        let z = ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a };
        let t = Target::Collect { pts: (0..6).map(|i| at(home(), 900.0 + 100.0 * f64::from(i), 0.0)).collect(), need: 3, r: 25.0, theme: "gems".into() };
        assert!(replace_unpicked(&t, &BTreeSet::new(), &z, &params(1), 1, &mut StdRng::seed_from_u64(1)).is_none());
        assert!(replace_unpicked(&t, &BTreeSet::new(), &ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &Atlas::default() }, &params(1), 1, &mut StdRng::seed_from_u64(1)).is_none(), "an empty atlas");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core replac`
Expected: compile error, `replace_unpicked` not found.

- [ ] **Step 3: Implement** (assign.rs, after `assign`)

```rust
/// A forager quest with its unpicked items moved to new street points under the placement rules (a Shuffle trap). Picked items, `need`,
/// `r` and the theme stay, and so do the item indexes saved progress refers to. `None` for any other target, or when the zone cannot
/// supply the new points: the caller then leaves the quest as it is.
#[must_use]
pub fn replace_unpicked(t: &Target, picked: &BTreeSet<u16>, z: &ZoneCtx<'_>, p: &AssignParams, tier: u8, rng: &mut StdRng) -> Option<Target> {
    let Target::Collect { pts, need, r, theme } = t else { return None };
    let is_picked = |i: usize| u16::try_from(i).is_ok_and(|i| picked.contains(&i));
    let keep: Vec<Point> = pts.iter().enumerate().filter(|(i, _)| is_picked(*i)).map(|(_, q)| *q).collect();
    let want = mid(tier, p.minutes_per_tier);
    let min_m = p.min_distance_m.max(HOME_RADIUS_M + r);
    let pool = street_pool(z, p.surface);
    let index = zone_index(z, &pool, p.surface);
    let open = pts.len() - keep.len();
    let mut fresh = place_items(&pool, &index, p.home, min_m, dist_for(want / 2.0, z.mode), open, &keep, rng)?.into_iter();
    let pts = pts.iter().enumerate().map(|(i, q)| if is_picked(i) { Some(*q) } else { fresh.next() }).collect::<Option<Vec<Point>>>()?;
    Some(Target::Collect { pts, need: *need, r: *r, theme: theme.clone() })
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/assign.rs
git commit -m "feat: re-place unpicked forager items" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Keep forager progress in the game and its save

Trackers are `#[serde(skip)]` today (no tracker state is saved, Courier's included), so forager progress lives in a new persisted map.

**Files:**

- Modify: `core/src/game.rs`

**Interfaces:**

- Consumes: `Collected`, `collect_progress`, `Tracker::with_collected`, `Tracker::collected` (Task 2).
- Produces: `Game::collected: BTreeMap<i64, Collected>` (`#[serde(default)]`), `QuestView::collected: Option<Collected>`; quest
  progress after loading comes from `collected` until a tracker exists.

- [ ] **Step 1: Write the failing tests** (game.rs `mod tests`)

```rust
    /// A solo game whose only quest (2000) is a forager: 6 acorns 100 m apart going north from 500 m, need 3.
    fn forager_game() -> (Game, Vec<Point>) {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let pts: Vec<Point> = (0..6u32).map(|i| destination(home(), 0.0, 500.0 + 100.0 * f64::from(i))).collect();
        let mut a = chain::tests_support::member(2000, 1, "forager", Target::Collect { pts: pts.clone(), need: 3, r: 25.0, theme: "acorns".into() });
        a.family = "courier".into();
        g.assignments = vec![a];
        g.done.clear();
        g.solo_rewards = BTreeMap::from([(2000, "Hydrate!".to_string())]);
        (g, pts)
    }

    fn carried_banked(g: &Game) -> (u32, u32) {
        g.collected.get(&2000).map_or((0, 0), |c| (c.carried, c.banked))
    }

    #[test]
    fn a_forager_banks_over_several_outings_and_completes_on_the_arrival_that_reaches_the_need() {
        let (mut g, pts) = forager_game();
        g.on_fix(fixat(pts[0], 600), None);
        g.on_fix(fixat(pts[1], 1200), None);
        assert_eq!(carried_banked(&g), (2, 0));
        assert!(done_ids(&g.on_fix(fixat(home(), 1800), None)).is_empty(), "2 of 3 banked");
        assert_eq!(carried_banked(&g), (0, 2));
        g.on_fix(fixat(pts[2], 2400), None);
        assert_eq!(done_ids(&g.on_fix(fixat(home(), 3000), None)), vec![2000]);
    }

    #[test]
    fn forager_progress_survives_a_restart_and_an_old_save_without_it_loads() {
        let dir = std::env::temp_dir().join(format!("apgo-forager-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (mut g, pts) = forager_game();
        g.on_fix(fixat(pts[0], 600), None);
        g.on_fix(fixat(home(), 1200), None);
        g.on_fix(fixat(pts[1], 1800), None);
        g.save(&dir).unwrap();
        let mut back = Game::load(&dir, "g1").unwrap();
        assert_eq!(back.collected, g.collected);
        let v = back.quest_views().remove(0);
        assert!((v.progress - 0.5).abs() < 1e-6 && v.state == QuestState::InProgress, "(1 + 0.5) / 3 before any new fix: {}", v.progress);
        assert_eq!(v.collected, g.collected.get(&2000).cloned());
        back.on_fix(fixat(home(), 2400), None);
        assert_eq!(carried_banked(&back), (0, 2), "what was carried before the restart is banked");
        let _ = std::fs::remove_dir_all(&dir);

        let mut old = serde_json::to_value(&g).unwrap();
        old.as_object_mut().unwrap().remove("collected");
        let loaded: Game = serde_json::from_value(old).unwrap();
        assert!(loaded.collected.is_empty());
    }

    #[test]
    fn nothing_is_picked_or_banked_while_counting_is_off_or_a_freeze_trap_blocks_checks() {
        let (mut g, pts) = forager_game();
        g.set_counting(false);
        g.on_fix(fixat(pts[0], 600), None);
        assert_eq!(carried_banked(&g), (0, 0), "counting off: no pickup");
        g.set_counting(true);
        freeze(&mut g);
        g.on_fix(fixat(pts[0], 1200), None);
        assert_eq!(carried_banked(&g), (0, 0), "frozen: no pickup");
    }

    #[test]
    fn carried_items_wait_out_home_wifi_and_bank_on_the_next_accepted_home_fix() {
        let (mut g, pts) = forager_game();
        g.on_fix(fixat(pts[0], 600), None);
        g.set_counting(false); // home Wi-Fi before the arrival fix
        g.on_fix(fixat(home(), 1200), None);
        assert_eq!(carried_banked(&g), (1, 0), "still carried");
        g.set_counting(true); // leaving home on the next walk
        g.on_fix(fixat(destination(home(), 0.0, 60.0), 5000), None);
        assert_eq!(carried_banked(&g), (0, 1));
    }
```

(`freeze` is the existing test helper at the "steps wait under a trap" tests; it freezes at home with a thaw point elsewhere, so a fix at
`pts[0]` is blocked unless it happens to be the thaw point. If `blocks_checks` is position-dependent and the fixture's thaw point lies on
an item, pick `pts[5]` instead.)

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core forager`
Expected: compile error, no field `collected` on `Game` / `QuestView`.

- [ ] **Step 3: Implement** (`core/src/game.rs`)

- Imports: `use crate::verify::{collect_progress, implied_speed_kmh, Collected, Fix, Status, Tracker, MAX_ACCURACY_M, MAX_OUTLIER_STREAK, MAX_PLAUSIBLE_KMH};`.
- `QuestView`, after `chain_id`:

```rust
    /// A forager quest's items picked, carried and banked so far (`None` for other quests, or before anything was picked).
    pub collected: Option<Collected>,
```

- `Game`, after `counters`:

```rust
    /// Forager quests: location id -> items picked, carried and banked. Kept across restarts and Shuffle traps.
    #[serde(default)]
    pub collected: BTreeMap<i64, Collected>,
```

- `Game::create`: add `collected: BTreeMap::new(),` after `counters: ..`.
- `reach_radius`: add `| Target::Collect { r, .. }` to the first arm.
- After `fn adjusted`:

```rust
    /// Progress kept in the save for a quest that has no tracker yet this session (a forager after a restart).
    fn saved_progress(&self, a: &Assignment) -> f32 {
        match (&a.target, self.collected.get(&a.location_id)) {
            (Target::Collect { need, .. }, Some(c)) => collect_progress(c, *need),
            _ => 0.0,
        }
    }
```

- `quest_views`: replace `self.trackers.get(&a.location_id).map_or(0.0, |t| match t.status() { .. })` with
  `self.trackers.get(&a.location_id).map_or_else(|| self.saved_progress(a), |t| match t.status() { .. })` (same match body), and in the
  `QuestView { .. }` literal add `collected: self.collected.get(&a.location_id).cloned(),`.
- `on_fix`, the tracker block becomes:

```rust
                if !self.trackers.contains_key(&id) {
                    let Some(a) = self.assignments.iter().find(|a| a.location_id == id) else { continue };
                    let target = self.adjusted(&a.target);
                    let t = match self.collected.get(&id) {
                        Some(c) => Tracker::with_collected(target, self.home, c.clone()),
                        None => Tracker::new(target, self.home),
                    };
                    self.trackers.insert(id, t);
                }
                if let Some(t) = self.trackers.get_mut(&id) {
                    let status = t.update(&fix, steps_total);
                    if let Some(c) = t.collected().filter(|c| **c != Collected::default()) {
                        self.collected.insert(id, c.clone());
                    }
                    if status == Status::Done {
                        finished.push(id);
                    }
                }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/game.rs
git commit -m "feat: save forager progress with the game" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: A Shuffle trap moves only a forager's unpicked items

**Files:**

- Modify: `core/src/game.rs` (`Game::reroll`)

**Interfaces:**

- Consumes: `replace_unpicked` (Task 4), `Game::collected` (Task 5).
- Produces: `Game::reroll` keeps a forager's kind, counts and picked items; returns the number of quests changed (forager included).

- [ ] **Step 1: Write the failing tests** (game.rs `mod tests`)

```rust
    #[test]
    fn a_shuffle_trap_moves_only_the_unpicked_forager_items_and_keeps_the_counts() {
        let (mut g, pts) = forager_game();
        g.on_fix(fixat(pts[0], 600), None);
        g.on_fix(fixat(home(), 1200), None);
        g.on_fix(fixat(pts[1], 1800), None);
        let before = g.collected[&2000].clone();
        let realms = vec![realm("r0", Mode::Walk)];
        assert_eq!(g.reroll(&[2000], &realms, 9, &Catalog::builtin()).unwrap(), 1);
        assert_eq!(g.collected[&2000], before, "carried, banked and picked are kept");
        assert_eq!(g.assignments[0].kind_id, "forager");
        let Target::Collect { pts: after, need, theme, .. } = g.assignments[0].target.clone() else { panic!("still a forager") };
        assert_eq!((need, theme.as_str()), (3, "acorns"));
        assert_eq!((after[0], after[1]), (pts[0], pts[1]), "picked items stay");
        assert_ne!(after[2..], pts[2..], "unpicked items move");
        g.on_fix(fixat(after[2], 2400), None);
        assert_eq!(carried_banked(&g), (2, 1), "a moved item can be picked up");
    }

    #[test]
    fn a_shuffle_trap_leaves_a_forager_alone_when_its_zone_has_no_room() {
        let (mut g, pts) = forager_game();
        g.on_fix(fixat(pts[0], 600), None);
        let (r, _) = realm("r0", Mode::Walk);
        let realms = vec![(r, Atlas::default())];
        assert_eq!(g.reroll(&[2000], &realms, 9, &Catalog::builtin()).unwrap(), 0);
        assert!(matches!(&g.assignments[0].target, Target::Collect { pts: same, .. } if *same == pts));
        assert_eq!(carried_banked(&g), (1, 0));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core shuffle_trap`
Expected: FAIL: the forager is re-assigned (kind or counts change), or the second test sees a fallback quest instead of the old one.

- [ ] **Step 3: Implement** (`Game::reroll`)

Import `replace_unpicked` from `crate::assign`. After building `params`, before `let fresh = assign(..)`:

```rust
        // A forager keeps its kind, counts and picked items: only what is still out there moves (and stays put if the zone has no room).
        let (foragers, todo): (Vec<i64>, Vec<i64>) =
            todo.into_iter().partition(|i| self.assignments.iter().any(|a| a.location_id == *i && matches!(a.target, Target::Collect { .. })));
        let mut rng = StdRng::seed_from_u64(seed);
        let mut moved = 0;
        for id in foragers {
            let picked = self.collected.get(&id).map(|c| c.picked.clone()).unwrap_or_default();
            let Some(a) = self.assignments.iter_mut().find(|a| a.location_id == id) else { continue };
            let Some(z) = zones.iter().find(|z| z.zone == a.zone) else { continue };
            if let Some(t) = replace_unpicked(&a.target, &picked, z, &params, a.tier, &mut rng) {
                a.target = t;
                self.trackers.remove(&id); // rebuilt from `collected` on the next fix
                moved += 1;
            }
        }
```

and return `Ok(n + moved)` instead of `Ok(n)`. Update the doc comment: "Re-place unfinished quests (a Shuffle trap). Finished quests and
chain members never change; a forager keeps its picked items and counts and only its unpicked items move."

- [ ] **Step 4: Run to verify they pass**

Run: `cd core && cargo test -p apgo-core && just check-rust`
Expected: PASS, coverage at or above 80.

- [ ] **Step 5: Commit**

```bash
git add core/src/game.rs
git commit -m "feat: shuffle only unpicked forager items" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Bank on joining home Wi-Fi (core)

Owner decision: joining home Wi-Fi banks what is carried even with no GPS fix, because home Wi-Fi switches counting off, often
before a fix inside the home radius is accepted.

**Files:**

- Modify: `core/src/game.rs`

**Interfaces:**

- Consumes: `Game::collected` (Task 5), `Game::complete`, `Game::zone_unlocked`, `Traps::blocks_checks`.
- Produces: `pub fn Game::bank_at_home(&mut self, t_ms: i64) -> Vec<Event>`: for every unfinished forager in an unlocked zone,
  `banked += carried`, `carried = 0`, drops its tracker (rebuilt from `collected` on the next fix), completes it when
  `banked >= need`. Idempotent. Returns nothing when `self.traps.blocks_checks(self.home)` is `Some`. Not gated by `counting`
  (counting is off at home by definition).

- [ ] **Step 1: Write the failing tests** (game.rs `mod tests`, next to the Task 5 tests; reuses `forager_game`, `carried_banked`,
  `done_ids`, `freeze`)

```rust
    #[test]
    fn joining_home_wifi_banks_what_is_carried_once_and_completes_a_forager_at_its_need() {
        let (mut g, pts) = forager_game();
        g.on_fix(fixat(pts[0], 600), None);
        g.on_fix(fixat(pts[1], 1200), None);
        g.set_counting(false); // home Wi-Fi: no fix inside the home radius is ever accepted
        assert!(done_ids(&g.bank_at_home(1300)).is_empty(), "2 of 3 banked, not done");
        assert_eq!(carried_banked(&g), (0, 2));
        assert!(g.bank_at_home(1400).is_empty());
        assert_eq!(carried_banked(&g), (0, 2), "a second call banks nothing new");
        g.set_counting(true);
        g.on_fix(fixat(pts[2], 3000), None);
        assert_eq!(carried_banked(&g), (1, 2), "the tracker carries on from the banked state");
        g.set_counting(false);
        assert_eq!(done_ids(&g.bank_at_home(3600)), vec![2000]);
        assert_eq!(carried_banked(&g), (0, 3));
        assert!(g.bank_at_home(3700).is_empty(), "a finished quest is not completed twice");
    }

    #[test]
    fn a_trap_that_blocks_checks_blocks_banking_at_home_and_keeps_what_is_carried() {
        let (mut g, pts) = forager_game();
        g.on_fix(fixat(pts[0], 600), None);
        freeze(&mut g);
        assert!(g.bank_at_home(700).is_empty());
        assert_eq!(carried_banked(&g), (1, 0), "still carried, nothing lost");
        let mut plain = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        assert!(plain.bank_at_home(700).is_empty(), "a game without foragers has nothing to bank");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd core && cargo test -p apgo-core bank`
Expected: compile error, no method `bank_at_home` on `Game`.

- [ ] **Step 3: Implement** (`impl Game`, after `set_in_zone`)

```rust
    /// The phone joined home Wi-Fi (presence entered "at home"), with or without a GPS fix: every forager banks what it carries, and one
    /// that reaches its need is completed. Counting is off at home, so it is not checked here; a trap that blocks checks blocks this as it
    /// blocks banking on a fix. Calling it again banks nothing new.
    pub fn bank_at_home(&mut self, t_ms: i64) -> Vec<Event> {
        if self.traps.blocks_checks(self.home).is_some() {
            return Vec::new();
        }
        let mut reached = Vec::new();
        for a in &self.assignments {
            let Target::Collect { need, .. } = &a.target else { continue };
            if self.done.contains(&a.location_id) || !self.zone_unlocked(a.zone) {
                continue;
            }
            let Some(c) = self.collected.get_mut(&a.location_id) else { continue };
            if c.carried == 0 {
                continue;
            }
            c.banked += std::mem::take(&mut c.carried);
            if c.banked >= *need {
                reached.push(a.location_id);
            }
            self.trackers.remove(&a.location_id); // rebuilt from `collected` on the next fix
        }
        let home = self.home;
        reached.into_iter().flat_map(|id| self.complete(id, t_ms, Some(home))).collect()
    }
```

(Field borrows are disjoint: `self.assignments` is read while `self.collected` and `self.trackers` are changed; `zone_unlocked`
only reads.)

- [ ] **Step 4: Run to verify they pass**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/game.rs
git commit -m "feat: bank forager items on home Wi-Fi" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Forager items and counts over FFI

**Files:**

- Modify: `core/ffi/src/engine.rs`
- Modify: `android/app/src/test/java/dev/apgo2/PlayLayoutTest.kt`, `android/app/src/test/java/dev/apgo2/MapGeoJsonTest.kt` (fixture
  constructors only: the record gained a field)

**Interfaces:**

- Consumes: `QuestView::collected` (Task 5), `Target::Collect` (Task 2), `Game::bank_at_home` (Task 7).
- Produces (Kotlin, `uniffi.apgo_ffi`): `CollectItemOut(at: GeoPoint, picked: Boolean)`, `CollectOut(theme: String, need: UInt,
  carried: UInt, banked: UInt, items: List<CollectItemOut>)`, `QuestOut.collect: CollectOut?` (last field); a forager's
  `QuestOut.anchor` is its first unpicked item; `shape == "collect"`; `Engine.bankAtHome(tMs: Long): List<EventOut>`.

- [ ] **Step 1: Write the failing test** (new `#[cfg(test)] mod tests` at the end of `core/ffi/src/engine.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use apgo_core::verify::Collected;

    #[test]
    fn a_forager_reports_its_items_and_counts_and_other_quests_report_none() {
        let (a, b) = (Point::new(40.0, -111.0), Point::new(40.01, -111.0));
        let t = Target::Collect { pts: vec![a, b], need: 1, r: 25.0, theme: "shells".into() };
        let c = Collected { picked: std::collections::BTreeSet::from([0]), carried: 1, banked: 0 };
        let out = collect_out(&t, Some(&c)).expect("a forager has items");
        assert_eq!((out.theme.as_str(), out.need, out.carried, out.banked), ("shells", 1, 1, 0));
        assert_eq!(out.items.iter().map(|i| i.picked).collect::<Vec<_>>(), [true, false]);
        assert!((out.items[1].at.lat - 40.01).abs() < 1e-12);
        assert_eq!(collect_out(&t, None).unwrap().items.iter().filter(|i| i.picked).count(), 0, "nothing picked yet");
        assert!(collect_out(&Target::Point { p: a, r: 40.0 }, None).is_none());
        assert_eq!(first_open(&out).map(|p| p.lat), Some(40.01), "the pin to open is the first item still out there");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd core && cargo test -p apgo-ffi`
Expected: compile error, `collect_out` / `first_open` not found.

- [ ] **Step 3: Implement** (`core/ffi/src/engine.rs`)

Add `use apgo_core::verify::Collected;` (next to the other `apgo_core` imports). After `QuestOut`:

```rust
/// One item of a forager quest.
#[derive(Debug, uniffi::Record)]
pub struct CollectItemOut {
    /// Where the item lies.
    pub at: GeoPoint,
    /// Whether the player has picked it up.
    pub picked: bool,
}

/// A forager quest's items and how far it has got.
#[derive(Debug, uniffi::Record)]
pub struct CollectOut {
    /// What the items are ("pinecones").
    pub theme: String,
    /// How many must be brought home.
    pub need: u32,
    /// Picked up and not yet brought home.
    pub carried: u32,
    /// Brought home so far.
    pub banked: u32,
    /// Every item, in the quest's order.
    pub items: Vec<CollectItemOut>,
}

fn collect_out(t: &Target, c: Option<&Collected>) -> Option<CollectOut> {
    let Target::Collect { pts, need, theme, .. } = t else { return None };
    let c = c.cloned().unwrap_or_default();
    let items = pts.iter().enumerate().map(|(i, p)| CollectItemOut { at: gp(*p), picked: u16::try_from(i).is_ok_and(|i| c.picked.contains(&i)) }).collect();
    Some(CollectOut { theme: theme.clone(), need: *need, carried: c.carried, banked: c.banked, items })
}

/// The first item still out there, where the quest's pin and popup sit.
fn first_open(c: &CollectOut) -> Option<Point> {
    c.items.iter().find(|i| !i.picked).map(|i| pt(&i.at))
}
```

In `QuestOut`, after `chain_id`:

```rust
    /// A forager quest's items and counts; `None` for other quests.
    pub collect: Option<CollectOut>,
```

In `Engine::quests`, inside the `map`:

```rust
                    let (shape, anchor, anchor_b, radius_m, path, detail) = describe(&q.target);
                    let collect = collect_out(&q.target, q.collected.as_ref());
                    let anchor = collect.as_ref().and_then(first_open).or(anchor);
```

and add `collect,` at the end of the `QuestOut { .. }` literal.

In the exported `impl Engine`, after `on_steps` (same shape; it saves at once because banking is progress even when nothing completes;
its logic is unit-tested in core, Task 7):

```rust
    /// The phone joined home Wi-Fi: every forager quest banks what it carries (see `Game::bank_at_home`). Safe to call again.
    pub fn bank_at_home(&self, t_ms: i64) -> Vec<EventOut> {
        let Some((game_id, ev, entries)) = self.with_game(|g| {
            let ev = g.bank_at_home(t_ms);
            self.save_if_due(g, t_ms, true);
            let entries = g.journal_events(&ev, t_ms, None);
            (g.id.clone(), ev, entries)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&game_id, e)));
        ev.into_iter().map(ev_out).collect()
    }
```

Kotlin fixtures: in `PlayLayoutTest.kt` and `MapGeoJsonTest.kt`, the `QuestOut(...)` calls get `collect = null,` after `chainId = ..`.

- [ ] **Step 4: Run to verify it passes**

Run: `cd core && cargo test -p apgo-ffi && cd .. && just check-rust && just check-android`
Expected: PASS (bindings regenerate in `check-android`).

- [ ] **Step 5: Commit**

```bash
git add core/ffi/src/engine.rs android/app/src/test/java/dev/apgo2/PlayLayoutTest.kt android/app/src/test/java/dev/apgo2/MapGeoJsonTest.kt
git commit -m "feat: expose forager items over ffi" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Forager row text, item list and help

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/ui/CollectFormat.kt`
- Create: `android/app/src/test/java/dev/apgo2/ui/CollectFormatTest.kt`
- Modify: `android/app/src/main/java/dev/apgo2/PlayDetails.kt`
- Modify: `android/app/src/main/java/dev/apgo2/ui/HelpText.kt`
- Modify: `android/app/src/main/java/dev/apgo2/ui/ApgoIcons.kt`

**Interfaces:**

- Consumes: `CollectOut`, `CollectItemOut` (Task 8).
- Produces: `CollectFormat.row(c: CollectOut): String`, `CollectFormat.banked(c): String`, `CollectFormat.item(c, i: Int): String`;
  `ApgoIcons.collectible(theme: String): ImageVector`.

- [ ] **Step 1: Write the failing test**

```kotlin
package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.apgo_ffi.CollectItemOut
import uniffi.apgo_ffi.CollectOut
import uniffi.apgo_ffi.GeoPoint

class CollectFormatTest {
    private fun collect(
        carried: Int,
        banked: Int,
        need: Int,
        vararg picked: Boolean,
    ) = CollectOut(
        theme = "pinecones",
        need = need.toUInt(),
        carried = carried.toUInt(),
        banked = banked.toUInt(),
        items = picked.map { CollectItemOut(at = GeoPoint(lat = 0.0, lon = 0.0), picked = it) },
    )

    @Test fun theRowSaysWhatIsCarriedAndBanked() {
        assertEquals("carrying 2 · banked 3 / 5", CollectFormat.row(collect(2, 3, 5)))
        assertEquals("carrying 0 · banked 0 / 3", CollectFormat.row(collect(0, 0, 3)))
        assertEquals("banking past the need is shown as it is", "carrying 0 · banked 6 / 5", CollectFormat.row(collect(0, 6, 5)))
    }

    @Test fun theDetailsNameTheThemeAndEveryItem() {
        val c = collect(1, 3, 5, true, false)
        assertEquals("Banked 3 of 5 pinecones", CollectFormat.banked(c))
        assertEquals("Item 1: picked up", CollectFormat.item(c, 0))
        assertEquals("Item 2: still out there", CollectFormat.item(c, 1))
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.ui.CollectFormatTest' --console=plain -q`
Expected: compile error, `CollectFormat` unresolved.

- [ ] **Step 3: Implement**

`ui/CollectFormat.kt`:

```kotlin
package dev.apgo2.ui

import uniffi.apgo_ffi.CollectOut

/** Text for a forager quest: its progress row and its item list. Pure, so it is unit-tested. */
internal object CollectFormat {
    fun row(c: CollectOut): String = "carrying ${c.carried} · banked ${c.banked} / ${c.need}"

    fun banked(c: CollectOut): String = "Banked ${c.banked} of ${c.need} ${c.theme}"

    fun item(
        c: CollectOut,
        i: Int,
    ): String = "Item ${i + 1}: ${if (c.items[i].picked) "picked up" else "still out there"}"
}
```

`ui/ApgoIcons.kt`: import `com.composables.icons.lucide.{Clover, Feather, Gem, Leaf, Nut, Shell, Sprout, TreePine}` (each its own
import line, kept sorted), add `"forager" to Lucide.Sprout,` to `kinds` after `"there_and_back"`, and after `kinds`:

```kotlin
    // What a forager quest's items are (core `FORAGE_THEMES`); an unknown theme gets the courier icon.
    private val collectibles: Map<String, ImageVector> =
        mapOf(
            "pinecones" to Lucide.TreePine,
            "shells" to Lucide.Shell,
            "acorns" to Lucide.Nut,
            "leaves" to Lucide.Leaf,
            "feathers" to Lucide.Feather,
            "clovers" to Lucide.Clover,
            "gems" to Lucide.Gem,
        )

    /** The icon of a forager item. */
    fun collectible(theme: String): ImageVector = collectibles[theme] ?: families.getValue("courier")
```

`ui/HelpText.kt`, the `"courier"` family entry becomes:

```kotlin
            "courier" to
                HelpTopic(
                    "Courier",
                    "Pick something up at one spot and deliver it to another, go out and come back, " +
                        "or gather things scattered around your area and bring enough of them home.",
                ),
```

`PlayDetails.kt`:

- imports: `androidx.compose.foundation.layout.ExperimentalLayoutApi`, `androidx.compose.foundation.layout.FlowRow`,
  `dev.apgo2.ui.CollectFormat`, `uniffi.apgo_ffi.CollectOut`.
- `ProgressRow`: the detail `Text(q.detail, ..)` becomes `Text(q.collect?.let(CollectFormat::row) ?: q.detail, ..)`.
- `QuestDetails`: after `Text(q.detail, ..)` add `q.collect?.let { CollectItems(it) }`.
- New composable at the end of the file:

```kotlin
// A forager quest's items: a check for each one picked up, its theme icon for each one still out there, and the banked total.
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CollectItems(c: CollectOut) {
    Text(CollectFormat.banked(c), fontSize = 12.sp)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        c.items.forEachIndexed { i, item ->
            Icon(
                if (item.picked) ApgoIcons.Check else ApgoIcons.collectible(c.theme),
                contentDescription = CollectFormat.item(c, i),
                tint = if (item.picked) ApgoPalette.questDone else ApgoPalette.family("courier"),
                modifier = Modifier.size(16.dp),
            )
        }
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `just check-android`
Expected: PASS (`HelpTextTest` still finds a whole sentence for courier).

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/ui/CollectFormat.kt android/app/src/test/java/dev/apgo2/ui/CollectFormatTest.kt \
  android/app/src/main/java/dev/apgo2/PlayDetails.kt android/app/src/main/java/dev/apgo2/ui/HelpText.kt \
  android/app/src/main/java/dev/apgo2/ui/ApgoIcons.kt
git commit -m "feat: show forager progress and items" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Forager item pins on the Play map

**Files:**

- Modify: `android/app/src/main/java/dev/apgo2/ui/MapMarkers.kt`
- Modify: `android/app/src/main/java/dev/apgo2/MapGeoJson.kt`
- Modify: `android/app/src/main/java/dev/apgo2/QuestMap.kt`
- Modify: `android/app/src/main/java/dev/apgo2/PlayScreen.kt`
- Modify: `android/app/src/main/java/dev/apgo2/DevSimulator.kt`
- Test: `android/app/src/test/java/dev/apgo2/ui/MapMarkersTest.kt`, `android/app/src/test/java/dev/apgo2/MapGeoJsonTest.kt`

**Interfaces:**

- Consumes: `QuestOut.collect` (Task 8), `ApgoIcons.collectible` (Task 9).
- Produces: `MarkerSpec.Item(theme: String, state: String)` with key `"item|$theme|courier|$state"`;
  `MapFeatures.questImages(quests: List<QuestOut>): Set<String>`; `QuestOut.tapPoints: List<GeoPoint>`.

- [ ] **Step 1: Write the failing tests**

`MapMarkersTest.everyMarkerSurvivesAKeyRoundTrip`: add `MarkerSpec.Item("pinecones", "open"),` and `MarkerSpec.Item("shells", "locked"),`
to `specs`.

`MapGeoJsonTest.kt` (class `MapFeaturesTest` or whichever class holds the `quest(..)` helper; imports `uniffi.apgo_ffi.CollectItemOut`,
`uniffi.apgo_ffi.CollectOut`, `dev.apgo2.ui.MarkerSpec`):

```kotlin
    @Test fun aForagerShowsOnePinPerItemStillOutThereAndIsTappedAtThem() {
        val items = listOf(CollectItemOut(geo(1.0, 1.0), false), CollectItemOut(geo(2.0, 2.0), true), CollectItemOut(geo(3.0, 3.0), false))
        val q = quest(7, shape = "collect").copy(family = "courier", kindId = "forager", collect = CollectOut("acorns", 3u, 1u, 0u, items))
        val pins = MapFeatures.quests(listOf(q), selected = 7)
        assertEquals("picked items disappear", 2, pins.size)
        val item = MarkerSpec.Item("acorns", "open").key
        assertTrue(pins.all { it.getJSONObject("properties").getString(MapProp.IMAGE) == item })
        assertEquals(1.0, pins[0].getJSONObject("geometry").getJSONArray("coordinates").getDouble(1), 0.0)
        assertEquals(setOf(q.mapImageKey, item), MapFeatures.questImages(listOf(q)))
        assertEquals(listOf(geo(1.0, 1.0), geo(3.0, 3.0)), q.tapPoints)
    }

    @Test fun otherQuestsAreTappedAtTheirAnchor() {
        assertEquals(listOf(geo(1.0, 2.0)), quest(1).tapPoints)
        assertTrue(quest(2, anchor = null).tapPoints.isEmpty())
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.*' --console=plain -q`
Expected: compile errors (`MarkerSpec.Item`, `questImages`, `tapPoints` unresolved).

- [ ] **Step 3: Implement**

`ui/MapMarkers.kt`: in `MarkerSpec`, add

```kotlin
    /** A forager item on the Play map: its theme's icon in the courier colour, grey while its zone is locked. */
    data class Item(
        val theme: String,
        val state: String,
    ) : MarkerSpec {
        override val key get() = "item|$theme|courier|$state"
    }
```

in `parse`, add `"item" -> MarkerSpec.Item(kindId, extra)`; in `render`, add

```kotlin
            is MarkerSpec.Item -> {
                renderQuestPin(
                    ApgoIcons.collectible(spec.theme),
                    QUEST_PIN_PX,
                    fill = if (spec.state == STATE_LOCKED) ApgoPalette.muted else ApgoPalette.family("courier"),
                    badge = Badge.None,
                )
            }
```

`MapGeoJson.kt`:

```kotlin
/** The name of the pin image of this forager quest's items (null for other quests). */
internal val QuestOut.itemImageKey: String? get() = collect?.let { MarkerSpec.Item(it.theme, state).key }

/** Where a tap selects this quest: each item still out there for a forager, otherwise its pin. */
internal val QuestOut.tapPoints: List<GeoPoint> get() = collect?.items?.filterNot { it.picked }?.map { it.at } ?: listOfNotNull(anchor)
```

(import `uniffi.apgo_ffi.GeoPoint`). In `MapFeatures.quests`:

```kotlin
        val visible = quests.filter { it.state != HIDDEN }
        val anchored = visible.filter { it.shape != "line" && it.collect == null }.mapNotNull { q -> q.anchor?.let { q to it } }
        val dropOffs = visible.filter { it.shape == "courier" }.mapNotNull { q -> q.anchorB?.let { q to it } }
        val pins = (anchored + dropOffs).map { (q, p) -> GeoJson.pointFeature(p.lat, p.lon, questProps(q, q.locationId == selected)) }
        val items =
            visible.flatMap { q ->
                q.collect?.items.orEmpty().filterNot { it.picked }.map {
                    GeoJson.pointFeature(it.at.lat, it.at.lon, questProps(q, q.locationId == selected, q.itemImageKey ?: q.mapImageKey))
                }
            }
        return pins + items
```

add

```kotlin
    /** Every pin image the quest layer needs: each quest's, and each forager's item pin. */
    fun questImages(quests: List<QuestOut>): Set<String> = quests.flatMap { listOfNotNull(it.mapImageKey, it.itemImageKey) }.toSet()
```

and give `questProps` an `image: String = q.mapImageKey` parameter used for `MapProp.IMAGE`.

`QuestMap.kt` `SyncContent`: `style?.let { st -> quests.forEach { holder.ensureImage(st, it.mapImageKey) } }` becomes
`style?.let { st -> MapFeatures.questImages(quests).forEach { holder.ensureImage(st, it) } }`.

`PlayScreen.kt` `nearestQuest`:

```kotlin
private fun nearestQuest(
    quests: List<QuestOut>,
    tap: LatLng,
): QuestOut? =
    quests
        .filter { it.state != HIDDEN }
        .flatMap { q -> q.tapPoints.map { q to it } }
        .minByOrNull { (_, a) ->
            val d = floatArrayOf(0f)
            android.location.Location.distanceBetween(tap.latitude, tap.longitude, a.lat, a.lon, d)
            d[0]
        }?.first
```

`DevSimulator.kt` `fixesFor`: add `"collect" -> collectFixes(q, home)` and

```kotlin
    // A forager: walk to as many items as are still needed, then home to bank them.
    private fun collectFixes(
        q: QuestOut,
        home: GeoPoint,
    ): List<EventOut> {
        val c = q.collect ?: return emptyList()
        val still = (c.need.toInt() - c.banked.toInt() - c.carried.toInt()).coerceAtLeast(0)
        return c.items.filterNot { it.picked }.take(still).flatMap { fix(it.at, GAP_MS) } + fix(home, GAP_MS)
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `just check-android`
Expected: PASS (Spotless, detekt, Lint, unit tests, Kover floor 7).

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/ui/MapMarkers.kt android/app/src/main/java/dev/apgo2/MapGeoJson.kt \
  android/app/src/main/java/dev/apgo2/QuestMap.kt android/app/src/main/java/dev/apgo2/PlayScreen.kt \
  android/app/src/main/java/dev/apgo2/DevSimulator.kt android/app/src/test/java/dev/apgo2/ui/MapMarkersTest.kt \
  android/app/src/test/java/dev/apgo2/MapGeoJsonTest.kt
git commit -m "feat: draw forager item pins" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Bank when presence arrives home (Android)

**Files:**

- Modify: `android/app/src/main/java/dev/apgo2/presence/PresencePolicy.kt`
- Modify: `android/app/src/main/java/dev/apgo2/PresenceController.kt`
- Modify: `android/app/src/main/java/dev/apgo2/AppModel.kt`
- Test: `android/app/src/test/java/dev/apgo2/presence/PresencePolicyTest.kt`

**Interfaces:**

- Consumes: `Engine.bankAtHome(tMs: Long): List<EventOut>` (Task 8), `AppModel.handle`, `AppModel.refreshPlay`, `AppModel.now`.
- Produces: `PresencePolicy.arrivedHome(before: PresenceState, after: PresenceState): Boolean`; `AppModel.bankAtHome()`.

- [ ] **Step 1: Write the failing test** (`PresencePolicyTest`, add `import org.junit.Assert.assertFalse` and
  `import org.junit.Assert.assertTrue`)

```kotlin
    @Test fun arrivingHomeIsOnlyTheChangeIntoAtHome() {
        assertTrue(PresencePolicy.arrivedHome(PresenceState.InZone, PresenceState.AtHome))
        assertTrue(PresencePolicy.arrivedHome(PresenceState.OutsideZones, PresenceState.AtHome))
        assertTrue("from the car to home", PresencePolicy.arrivedHome(PresenceState.InCar, PresenceState.AtHome))
        assertTrue("opening a game at home", PresencePolicy.arrivedHome(PresenceState.Stopped, PresenceState.AtHome))
        assertFalse("still home", PresencePolicy.arrivedHome(PresenceState.AtHome, PresenceState.AtHome))
        assertFalse("leaving home", PresencePolicy.arrivedHome(PresenceState.AtHome, PresenceState.InZone))
        assertFalse(PresencePolicy.arrivedHome(PresenceState.InZone, PresenceState.InCar))
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.presence.PresencePolicyTest' --console=plain -q`
Expected: compile error, `arrivedHome` unresolved.

- [ ] **Step 3: Implement**

`PresencePolicy` (after `decide`):

```kotlin
    /** Whether presence just arrived home (home Wi-Fi joined): the moment forager quests bank what they carry. */
    fun arrivedHome(
        before: PresenceState,
        after: PresenceState,
    ): Boolean = after == PresenceState.AtHome && before != PresenceState.AtHome
```

`AppModel` (next to `onSteps`):

```kotlin
    /** Presence arrived home (home Wi-Fi): forager quests bank what they carry, even with no GPS fix. */
    fun bankAtHome() {
        if (!engine.hasGame()) return
        handle(engine.bankAtHome(now()))
        refreshPlay(withTrace = false)
    }
```

`PresenceController.evaluate`, after `if (d == decision) return`:

```kotlin
        val changedState = d.state != decision.state
        val arrived = PresencePolicy.arrivedHome(decision.state, d.state)
        decision = d
        if (changedState) {
            Diag.info(TAG, d.state.name, "counting" to d.counting, "gps" to d.gps.toString())
            if (model.hud != null) model.engine.logPresence(presenceText(d.state), t)
        }
        if (arrived && model.hud != null) model.bankAtHome()
        applyLocation()
```

(Only the `arrived` lines are new; keep the rest as it is on `main`.)

- [ ] **Step 4: Run to verify it passes**

Run: `just check-android`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/presence/PresencePolicy.kt android/app/src/main/java/dev/apgo2/PresenceController.kt \
  android/app/src/main/java/dev/apgo2/AppModel.kt android/app/src/test/java/dev/apgo2/presence/PresencePolicyTest.kt
git commit -m "feat: bank forager items on arriving home" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Docs, full gates and device check

**Files:**

- Modify: `docs/context/v1-architecture-and-status.md`
- Modify: `docs/context/ui-design-system.md`

- [ ] **Step 1: Update the docs**

`v1-architecture-and-status.md`: "a 76-kind catalog" becomes "a 77-kind catalog"; under the near-a-path bullet add:

```markdown
- **Forager (#5)**: courier-family kind `forager` (walk, run, bike), verify `Collect`. `2 * need` items (need 3/5/7/10 by tier, 10 above
  tier 4) are street-pool points at least 60 m apart and at least `max(min_distance_m, 125 m)` from home, spread out to the effort
  distance (`dist_for(want / 2)`), nearest first (the fog anchor). Pickup 25 m; banking on any accepted fix within `verify::HOME_RADIUS_M`,
  and on joining home Wi-Fi (`PresenceController` calls `Engine::bank_at_home` on the change to `AtHome`).
  Progress is saved in `Game::collected` (trackers are not saved). A Shuffle trap moves only unpicked items (`assign::replace_unpicked`).
  Verified by unit tests and on the emulator; the device walk is not done yet.
```

`ui-design-system.md` Rules, the `MapMarkers.render` bullet gets: "`Item` (Play map: a forager item, its theme icon from
`ApgoIcons.collectible` in the courier colour, no badge; one pin per item still out there)".

- [ ] **Step 2: Run every gate**

Run: `just check-rust && just check-android && just check-hygiene`
Expected: PASS. If line coverage rose, raise the floor to the measured value rounded down (rust `--fail-under-lines` in the `justfile`,
kotlin `minBound` in `android/app/build.gradle.kts`); never lower it.

- [ ] **Step 3: Commit**

```bash
git add docs/context/v1-architecture-and-status.md docs/context/ui-design-system.md
git commit -m "docs: record the forager quest" -m "Refs #5" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 4: Emulator check, then the owner's device walk**

Emulator: `just emu-start && just emu-run`, start a solo game with only the courier quest type until a forager appears (seeds vary),
screenshot the map (themed item pins, nearest one opens the popup with the item row), and run the dev simulator to completion.
Owner on the Pixel 8 Pro (spec): a short walk that picks up two items, banks at home, goes out again and completes the quest; then pull
diagnostics (`docs/context/outdoor-test-plan.md`) and update the "device walk" sentence in `v1-architecture-and-status.md`. Say plainly in the
PR which of these were done.
