# Cartographer by street coverage — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cartographer quests ask for "% of the zone's streets walked" (counted per game, from the location-quality street graph)
instead of 150 m grid squares, and selecting one highlights the walked streets.

**Architecture:**

- A pure core module `core/src/coverage.rs`: street pieces cut from `loc::StreetGraph`, location keys, crediting, zone totals,
  walked lines.
- `Game` owns the walked set (saved in the game JSON) and feeds it from `reveal`.
- Cartographer becomes a `Target::Streets { distance_m }` chain whose counter (walked metres) is read live. The UI shows metres
  as %.
- Android adds a walked-streets map layer for the selected Cartographer chain.

**Tech Stack:** Rust (apgo-core, apgo-ffi via UniFFI), Kotlin/Compose + MapLibre (android/).

**Spec:** `docs/superpowers/specs/2026-10-09-cartographer-street-coverage-design.md`

## Global Constraints

- **Start only after `feat/location-quality` is merged into main**, including its `way_class::SIDE = 8` bit (agreed with that
  session). Then branch from main.
  - Before each task, re-check the signatures quoted here: `StreetGraph`, `Estimate`, `Game::reveal`, `Game::chains`,
    `normalize_counters`, `ChainOut`, `rule_text(away_m)`.
  - The code below was read from `feat/location-quality` at `228b405`. If a name moved, follow the code, not the plan.
- `PIECE_M = 20.0`, `CREDIT_RADIUS_M = 15.0`, `CREDIT_MAX_UNCERTAINTY_M = 20.0`, `MAX_SHARE = 0.5`,
  `MIN_ZONE_STREET_M = 2000.0`, key scale `1e5` (about 1 m).
- Pieces with the `SIDE` class bit are excluded from totals and crediting.
- Walked state is per game (`Game.walked`, `#[serde(default)]`); never a lifetime store.
- Events over polling: crediting only on estimates that already arrive; % computed when asked; the highlight fetched on
  selection or progress change.
- Every colour is an `ApgoPalette` token (`scripts/check_color_tokens.sh`); line widths come from `core/src/line_width.rs` via
  `scaledWidth`.
- The Explorer win goal and fog keep their squares (`Fog.cells`); only Cartographer changes.
- TDD: failing test first, never edit a test to fit the code. Conventional commits (allowed types: fix, feat, chore, docs, style,
  refactor, perf, test), subject < 50 chars, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Checks: `cd core && cargo test -q && cargo clippy -q --all-targets && cargo fmt --check`;
  `just check-android` (needs `android/local.properties` with `sdk.dir=/home/rasbandit/Android/Sdk` in a fresh worktree).

## Review Focus

1. **An old save with square-based Cartographer progress** must load, keep its done marks done, and not read old square counts
   as metres. Task 5 test `old_cells_save_converts`.
2. **A zone with sidewalks mapped beside every street:** walking the street must earn the street's share, not a third. Task 1 test
   `sidewalks_are_ignored`.
3. **Standing still with GPS jitter** must add nothing after the first piece. Task 1 test `same_piece_twice_credits_once`; Task 2
   test `jitter_in_place_adds_nothing`.
4. **The graph rebuilt by a rescan with different segment ids:** walked metres must stay the same. Task 1 test
   `walked_survives_rebuild`.
5. **A chain shown before the street graph is ready** (it is built off the main thread on open): no crash, no completion, marks
   shown as distances. Task 4 test `no_graph_never_completes`.

---

### Task 1: Core coverage module

**Files:**

- Create: `core/src/coverage.rs`
- Modify: `core/src/lib.rs` (add `pub mod coverage;` in alphabetical order)

**Interfaces:**

- Consumes:
  - `loc::graph::StreetGraph` with `segment_count()`, `seg(i) -> Segment { a, b, len_m, class }`, `geo_at(seg, off_m) -> Point`,
    `candidates(p, radius_m, max, mask) -> Vec<Cand { seg, off_m, d_m }>`, `mode_mask(Mode) -> u8`;
  - `scan::way_class::{FOOT, BIKE, CAR, SIDE}`; `zone::Zone::contains`; `geo::Point`.
- Produces:
  - `pub type Key = (i32, i32);`
  - `pub struct Walked(BTreeMap<Key, (f32, u8)>)`: `Default`, `Serialize`/`Deserialize` as a flat `Vec<(i32, i32, f32, u8)>`.
  - `pub fn credit(walked: &mut Walked, g: &StreetGraph, p: Point, uncertainty_m: f64) -> bool` (true if a new piece was
    added).
  - `pub fn zone_total_m(g: &StreetGraph, zone: &Zone, mask: u8) -> f64`
  - `pub fn walked_m(walked: &Walked, zone: &Zone, mask: u8) -> f64`
  - `pub fn walked_lines(walked: &Walked, g: &StreetGraph, zone: &Zone, mask: u8) -> Vec<Vec<Point>>`

- [ ] **Step 1: Write the failing tests** at the bottom of `core/src/coverage.rs`. Build the graph with
  `StreetGraph::from_ways(&[WayGeom { id, class, pts }])` and place points with `geo::destination(origin, bearing_deg, m)`. Check
  the `WayGeom` field names in `scan.rs`.

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use crate::geo::{destination, Point};
      use crate::loc::graph::StreetGraph;
      use crate::scan::{way_class, WayGeom};
      use crate::zone::Zone;

      const FOOT: u8 = way_class::FOOT;

      fn origin() -> Point {
          Point::new(45.0, -122.0)
      }

      /// One straight east-west street of `len_m`, plus optional extra ways.
      fn graph(len_m: f64, extra: Vec<WayGeom>) -> StreetGraph {
          let mut ways = vec![WayGeom { id: 1, class: FOOT, pts: vec![origin(), destination(origin(), 90.0, len_m)] }];
          ways.extend(extra);
          StreetGraph::from_ways(&ways).expect("graph")
      }

      fn big_zone() -> Zone {
          Zone::Circle { center: origin(), radius_m: 5_000.0 }
      }

      #[test]
      fn total_counts_every_street_metre_in_the_zone() {
          let g = graph(200.0, vec![]);
          assert!((zone_total_m(&g, &big_zone(), FOOT) - 200.0).abs() < 1.0);
      }

      #[test]
      fn walking_the_street_credits_its_length() {
          let g = graph(200.0, vec![]);
          let mut w = Walked::default();
          for m in (0..=200).step_by(5) {
              credit(&mut w, &g, destination(destination(origin(), 90.0, f64::from(m)), 0.0, 5.0), 8.0);
          }
          assert!((walked_m(&w, &big_zone(), FOOT) - 200.0).abs() < 1.0);
      }

      #[test]
      fn same_piece_twice_credits_once() {
          let g = graph(200.0, vec![]);
          let mut w = Walked::default();
          let p = destination(origin(), 90.0, 50.0);
          assert!(credit(&mut w, &g, p, 5.0));
          assert!(!credit(&mut w, &g, p, 5.0));
          assert!((walked_m(&w, &big_zone(), FOOT) - PIECE_M).abs() < 0.5);
      }

      #[test]
      fn blurry_or_far_positions_credit_nothing() {
          let g = graph(200.0, vec![]);
          let mut w = Walked::default();
          assert!(!credit(&mut w, &g, destination(origin(), 90.0, 50.0), 25.0), "uncertainty over 20 m");
          assert!(!credit(&mut w, &g, destination(destination(origin(), 90.0, 50.0), 0.0, 16.0), 5.0), "16 m from the street");
          assert_eq!(walked_m(&w, &big_zone(), FOOT), 0.0);
      }

      #[test]
      fn sidewalks_are_ignored() {
          let side = |id, off| WayGeom {
              id,
              class: FOOT | way_class::SIDE,
              pts: vec![destination(origin(), 0.0, off), destination(destination(origin(), 0.0, off), 90.0, 200.0)],
          };
          let g = graph(200.0, vec![side(2, 8.0), side(3, -8.0)]);
          assert!((zone_total_m(&g, &big_zone(), FOOT) - 200.0).abs() < 1.0, "sidewalks left out of the total");
          let mut w = Walked::default();
          for m in (0..=200).step_by(5) {
              credit(&mut w, &g, destination(destination(origin(), 90.0, f64::from(m)), 0.0, 7.0), 8.0); // on the sidewalk
          }
          assert!((walked_m(&w, &big_zone(), FOOT) - 200.0).abs() < 1.0, "the street is credited, not the sidewalk");
      }

      #[test]
      fn only_pieces_inside_the_zone_count() {
          let g = graph(400.0, vec![]);
          let half = Zone::Circle { center: origin(), radius_m: 200.0 };
          assert!((zone_total_m(&g, &half, FOOT) - 200.0).abs() < PIECE_M);
          let mut w = Walked::default();
          credit(&mut w, &g, destination(origin(), 90.0, 350.0), 5.0); // outside the zone
          assert_eq!(walked_m(&w, &half, FOOT), 0.0);
      }

      #[test]
      fn mode_mask_filters_classes() {
          let car_only = WayGeom { id: 2, class: way_class::CAR, pts: vec![destination(origin(), 0.0, 300.0), destination(origin(), 45.0, 600.0)] };
          let g = graph(200.0, vec![car_only]);
          assert!((zone_total_m(&g, &big_zone(), FOOT) - 200.0).abs() < 1.0);
      }

      #[test]
      fn walked_survives_rebuild() {
          let g1 = graph(200.0, vec![]);
          let mut w = Walked::default();
          for m in (0..=200).step_by(5) {
              credit(&mut w, &g1, destination(origin(), 90.0, f64::from(m)), 5.0);
          }
          // The same street drawn in the opposite direction with a different id: segment ids and directions change on rescan.
          let g2 = StreetGraph::from_ways(&[WayGeom { id: 99, class: FOOT, pts: vec![destination(origin(), 90.0, 200.0), origin()] }]).unwrap();
          assert!((walked_m(&w, &big_zone(), FOOT) - zone_total_m(&g2, &big_zone(), FOOT)).abs() < PIECE_M);
      }

      #[test]
      fn walked_lines_join_neighbouring_pieces() {
          let g = graph(200.0, vec![]);
          let mut w = Walked::default();
          assert!(walked_lines(&w, &g, &big_zone(), FOOT).is_empty());
          for m in (0..=100).step_by(5) {
              credit(&mut w, &g, destination(origin(), 90.0, f64::from(m)), 5.0);
          }
          let lines = walked_lines(&w, &g, &big_zone(), FOOT);
          assert_eq!(lines.len(), 1, "adjacent pieces form one line");
          assert!(lines[0].len() >= 2);
      }

      #[test]
      fn walked_round_trips_through_json() {
          let g = graph(200.0, vec![]);
          let mut w = Walked::default();
          credit(&mut w, &g, destination(origin(), 90.0, 50.0), 5.0);
          let back: Walked = serde_json::from_str(&serde_json::to_string(&w).unwrap()).unwrap();
          assert_eq!(back, w);
      }
  }
  ```

`walked_survives_rebuild` relies on keys coming from the **piece midpoint**. That is the same point whichever end the segment
starts from, as long as pieces are cut symmetrically. So cut each segment into `n = max(1, round(len / PIECE_M))` equal pieces,
not 20 m pieces with a short remainder: equal pieces are the same from both ends.

- [ ] **Step 2: Run the tests and confirm they fail.**

  Run: `cd core && cargo test -q --lib coverage`. Expected: compile errors (the items are not defined).

- [ ] **Step 3: Implement** `core/src/coverage.rs` above the tests:

  ```rust
  //! Street coverage: which pieces of the game's streets the player has walked, for Cartographer.
  //!
  //! Each street-graph segment is cut into about `PIECE_M` long equal pieces. A piece is known by its midpoint rounded to about 1 m
  //! (a [`Key`]), so walked pieces survive graph rebuilds (segment ids and directions change on every rescan). Sidewalks and
  //! crossings (`way_class::SIDE`) run beside streets that already count, so they are left out of totals and crediting.

  use std::collections::BTreeMap;

  use serde::{Deserialize, Deserializer, Serialize, Serializer};

  use crate::geo::Point;
  use crate::loc::graph::StreetGraph;
  use crate::num::round_i64;
  use crate::scan::way_class;
  use crate::zone::Zone;

  /// Target length of a street piece in metres.
  pub const PIECE_M: f64 = 20.0;
  /// A position this close to a street (metres) walks it.
  pub const CREDIT_RADIUS_M: f64 = 15.0;
  /// Positions less certain than this (metres) credit nothing.
  pub const CREDIT_MAX_UNCERTAINTY_M: f64 = 20.0;
  const KEY_SCALE: f64 = 1e5; // ~1 m
  const ANY_MODE: u8 = way_class::FOOT | way_class::BIKE | way_class::CAR;

  /// A piece's midpoint, rounded to about 1 m.
  pub type Key = (i32, i32);

  /// The pieces walked in one game: key → (length in metres, way class bits).
  #[derive(Debug, Clone, Default, PartialEq)]
  pub struct Walked(BTreeMap<Key, (f32, u8)>);

  fn key(p: Point) -> Key {
      #[allow(clippy::cast_possible_truncation)] // lat/lon × 1e5 fits i32 with room to spare
      (round_i64(p.lat * KEY_SCALE) as i32, round_i64(p.lon * KEY_SCALE) as i32)
  }

  fn key_point(k: Key) -> Point {
      Point::new(f64::from(k.0) / KEY_SCALE, f64::from(k.1) / KEY_SCALE)
  }

  /// How many equal pieces segment `len_m` is cut into.
  fn pieces_of(len_m: f64) -> usize {
      #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a segment is a few km at most
      ((len_m / PIECE_M).round() as usize).max(1)
  }

  /// Midpoint and length of piece `i` of segment `seg`.
  fn piece(g: &StreetGraph, seg: usize, i: usize) -> (Point, f64) {
      let s = g.seg(seg);
      let n = pieces_of(s.len_m);
      let step = s.len_m / crate::num::count_f64(n);
      (g.geo_at(seg, step * (crate::num::count_f64(i) + 0.5)), step)
  }

  fn counts(class: u8, mask: u8) -> bool {
      class & mask != 0 && class & way_class::SIDE == 0
  }

  /// Mark the piece under `p` walked. Returns true when it was not walked before.
  pub fn credit(walked: &mut Walked, g: &StreetGraph, p: Point, uncertainty_m: f64) -> bool {
      if uncertainty_m > CREDIT_MAX_UNCERTAINTY_M {
          return false;
      }
      let Some(c) = g.candidates(p, CREDIT_RADIUS_M, 4, ANY_MODE).into_iter().find(|c| g.seg(c.seg).class & way_class::SIDE == 0) else {
          return false;
      };
      let s = g.seg(c.seg);
      let n = pieces_of(s.len_m);
      #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // off_m is within the segment
      let i = ((c.off_m / (s.len_m / crate::num::count_f64(n))) as usize).min(n - 1);
      let (mid, len) = piece(g, c.seg, i);
      #[allow(clippy::cast_possible_truncation)] // piece lengths are metres, f32 is plenty
      let fresh = walked.0.insert(key(mid), (len as f32, s.class)).is_none();
      fresh
  }

  /// Length of the zone's streets on `mask` (sidewalks left out), in metres.
  #[must_use]
  pub fn zone_total_m(g: &StreetGraph, zone: &Zone, mask: u8) -> f64 {
      (0..g.segment_count())
          .filter(|&i| counts(g.seg(i).class, mask))
          .flat_map(|i| (0..pieces_of(g.seg(i).len_m)).map(move |k| (i, k)))
          .map(|(i, k)| piece(g, i, k))
          .filter(|(mid, _)| zone.contains(*mid))
          .map(|(_, len)| len)
          .sum()
  }

  /// Walked length inside the zone on `mask`, in metres.
  #[must_use]
  pub fn walked_m(walked: &Walked, zone: &Zone, mask: u8) -> f64 {
      walked.0.iter().filter(|(k, (_, class))| counts(*class, mask) && zone.contains(key_point(**k))).map(|(_, (len, _))| f64::from(*len)).sum()
  }

  /// The walked pieces of the zone as lines (consecutive walked pieces of a segment joined), for the map.
  #[must_use]
  pub fn walked_lines(walked: &Walked, g: &StreetGraph, zone: &Zone, mask: u8) -> Vec<Vec<Point>> {
      let mut out = Vec::new();
      for seg in (0..g.segment_count()).filter(|&i| counts(g.seg(i).class, mask)) {
          let s = g.seg(seg);
          let n = pieces_of(s.len_m);
          let step = s.len_m / crate::num::count_f64(n);
          let mut run: Vec<Point> = Vec::new();
          for i in 0..n {
              let (mid, _) = piece(g, seg, i);
              if walked.0.contains_key(&key(mid)) && zone.contains(mid) {
                  if run.is_empty() {
                      run.push(g.geo_at(seg, step * crate::num::count_f64(i)));
                  }
                  run.push(g.geo_at(seg, step * crate::num::count_f64(i + 1)));
              } else if !run.is_empty() {
                  out.push(std::mem::take(&mut run));
              }
          }
          if !run.is_empty() {
              out.push(run);
          }
      }
      out
  }

  impl Serialize for Walked {
      fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
          let flat: Vec<(i32, i32, f32, u8)> = self.0.iter().map(|(k, (len, c))| (k.0, k.1, *len, *c)).collect();
          flat.serialize(s)
      }
  }

  impl<'de> Deserialize<'de> for Walked {
      fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
          let flat = Vec::<(i32, i32, f32, u8)>::deserialize(d)?;
          Ok(Self(flat.into_iter().map(|(a, b, len, c)| ((a, b), (len, c))).collect()))
      }
  }
  ```

  - `count_f64` and `round_i64` are in `crate::num`. If either is `pub(crate)`, that is fine inside the crate.
  - If clippy flags `let fresh = …; fresh`, return the expression directly.

- [ ] **Step 4: Run the tests and confirm they pass.**

  Run: `cd core && cargo test -q --lib coverage && cargo clippy -q --all-targets && cargo fmt --check`. Expected: 10 passed and
  no warnings.

- [ ] **Step 5: Commit.**

  ```bash
  git add core/src/coverage.rs core/src/lib.rs
  git commit -m "feat: street coverage pieces and walked set

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
  ```

---

### Task 2: Game owns the walked set and credits it in reveal

**Files:**

- Modify: `core/src/game.rs`:
  - `Game` struct (~line 293);
  - `reveal` (~line 1024) and its two callers (~973 and ~982);
  - `create` (~495) and `attach_streets` (~1070).
- Modify: `core/ffi/src/engine.rs`: every call of `attach_streets` (~872 and ~884) also passes the zone shapes.
- Test: `core/src/game.rs` tests module.

**Interfaces:**

- Consumes: everything Task 1 produces.
- Produces:
  - `Game.walked: coverage::Walked` (`#[serde(default)]`);
  - `Game::set_zone_shapes(&mut self, shapes: Vec<(u32, Zone)>)`;
  - `pub fn zone_street_m(&self, zone: u32) -> Option<f64>` (`None` when there is no graph or no shape);
  - `pub fn zone_walked_m(&self, zone: u32) -> f64`;
  - `pub fn walked_lines(&self, zone: u32) -> Vec<Vec<Point>>`.

- [ ] **Step 1: Write the failing tests** in game.rs's tests module. Use the module's existing helpers for a test game. Find the
  helper that builds a `Game` with a walk zone around `home()`, and the one feeding fixes (`on_fix` with a `RawFix`). Attach a
  graph with one 300 m street through home:

  ```rust
  #[test]
  fn walking_a_street_raises_the_zones_walked_metres() {
      let mut g = test_game(); // the existing helper; zone 1 is a walk zone around home()
      let street = vec![home(), destination(home(), 90.0, 300.0)];
      g.locator.set_graph(StreetGraph::from_ways(&[WayGeom { id: 1, class: way_class::FOOT, pts: street }]).map(Arc::new));
      g.set_zone_shapes(vec![(1, Zone::Circle { center: home(), radius_m: 2_000.0 })]);
      assert!((g.zone_street_m(1).unwrap() - 300.0).abs() < 1.0);
      walk_east(&mut g, 0.0, 300.0); // the existing fix-feeding helper: accepted fixes every 5 m along the street
      assert!(g.zone_walked_m(1) > 250.0);
  }

  #[test]
  fn jitter_in_place_adds_nothing() {
      let mut g = test_game();
      g.locator.set_graph(StreetGraph::from_ways(&[WayGeom { id: 1, class: way_class::FOOT, pts: vec![home(), destination(home(), 90.0, 300.0)] }]).map(Arc::new));
      g.set_zone_shapes(vec![(1, Zone::Circle { center: home(), radius_m: 2_000.0 })]);
      stand_still(&mut g, 60); // existing helper or 60 fixes at home with 3 m noise
      assert!(g.zone_walked_m(1) <= coverage::PIECE_M * 2.0, "at most the piece or two under you");
  }

  #[test]
  fn no_graph_means_no_total() {
      let g = test_game();
      assert_eq!(g.zone_street_m(1), None);
      assert_eq!(g.zone_walked_m(1), 0.0);
  }

  #[test]
  fn walked_set_is_saved() {
      let mut g = test_game();
      g.locator.set_graph(StreetGraph::from_ways(&[WayGeom { id: 1, class: way_class::FOOT, pts: vec![home(), destination(home(), 90.0, 300.0)] }]).map(Arc::new));
      g.set_zone_shapes(vec![(1, Zone::Circle { center: home(), radius_m: 2_000.0 })]);
      walk_east(&mut g, 0.0, 100.0);
      let back: Game = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
      assert_eq!(back.walked, g.walked);
  }
  ```

  If `walk_east` or `stand_still` do not exist, add them to the tests module and build them from the existing `RawFix` helper.
  Fixes need `accuracy_m` ≤ 8 so the locator accepts them.

- [ ] **Step 2: Run the tests and confirm they fail.**

  Run: `cd core && cargo test -q --lib game::tests::walking_a_street`. Expected: FAIL to compile (no `set_zone_shapes` /
  `zone_street_m`).

- [ ] **Step 3: Implement.**
  - In `Game`, after `fog`:

  ```rust
      /// Street pieces walked this game (Cartographer coverage).
      #[serde(default)]
      pub walked: coverage::Walked,
      /// Each zone's area, from its realm; attached with the streets, never saved.
      #[serde(skip)]
      zone_shapes: BTreeMap<u32, Zone>,
      /// Zone street totals for the graph they were computed from (rebuilt graphs throw them away).
      #[serde(skip)]
      street_totals: std::cell::RefCell<(usize, BTreeMap<u32, f64>)>,
  ```

  - Methods:

  ```rust
      /// The zones' areas, so coverage knows which streets belong to which zone.
      pub fn set_zone_shapes(&mut self, shapes: Vec<(u32, Zone)>) {
          self.zone_shapes = shapes.into_iter().collect();
          self.street_totals.borrow_mut().1.clear();
      }

      fn zone_mask(&self, zone: u32) -> u8 {
          self.slot.zones.iter().find(|z| z.id == zone).map_or(way_class::FOOT, |z| graph::mode_mask(z.mode))
      }

      /// Length of the zone's streets (sidewalks left out), once the street graph is ready.
      #[must_use]
      pub fn zone_street_m(&self, zone: u32) -> Option<f64> {
          let g = self.street_graph()?;
          let shape = self.zone_shapes.get(&zone)?;
          let id = Arc::as_ptr(g) as usize;
          let mut cache = self.street_totals.borrow_mut();
          if cache.0 != id {
              *cache = (id, BTreeMap::new());
          }
          Some(*cache.1.entry(zone).or_insert_with(|| coverage::zone_total_m(g, shape, self.zone_mask(zone))))
      }

      /// Street length walked this game inside the zone.
      #[must_use]
      pub fn zone_walked_m(&self, zone: u32) -> f64 {
          self.zone_shapes.get(&zone).map_or(0.0, |s| coverage::walked_m(&self.walked, s, self.zone_mask(zone)))
      }

      /// The zone's walked streets as lines, for the map.
      #[must_use]
      pub fn walked_lines(&self, zone: u32) -> Vec<Vec<Point>> {
          match (self.street_graph(), self.zone_shapes.get(&zone)) {
              (Some(g), Some(s)) => coverage::walked_lines(&self.walked, g, s, self.zone_mask(zone)),
              _ => Vec::new(),
          }
      }
  ```

  - `reveal` gains `uncertainty_m: f64` and credits coverage. Both callers pass `est.uncertainty_m`:

  ```rust
      fn reveal(&mut self, pos: Point, uncertainty_m: f64, ev: &mut Vec<Event>) {
          let scout = self.count("Progressive Scouting Distance");
          for id in self.fog.update(pos, &self.assignments, reveal_radius(scout)) {
              if self.fog_on() {
                  ev.push(Event::Discovered { location_id: id });
              }
          }
          if let Some(g) = self.locator.graph().cloned() {
              coverage::credit(&mut self.walked, &g, pos, uncertainty_m);
          }
      }
  ```

    Keep the `credit_cells` call for now (with its `cells_before` bookkeeping). Task 4 removes it when the chain unit changes.
  - **`Game::create`:** set `zone_shapes` from the `ZoneCtx`s: `zones.iter().map(|z| (z.zone, z.realm.shape.to_zone())).collect()`.
  - **`engine.rs`:** next to each `attach_streets`, call `set_zone_shapes` with `(zone id, realm.shape.to_zone())` for each
    `slot.zones[i]` and `zone_realms[i]`. `Engine::zones()` (~engine.rs:1232) already pairs them like this.
  - **`RefCell` in `Game`:** if `Game` must be `Sync` (the FFI keeps it in a `Mutex`, so `Send` is enough), a `RefCell` field is
    fine. If clippy or the compiler objects, use `std::sync::Mutex` instead.

- [ ] **Step 4: Run the tests and confirm they pass.**

  Run: `cd core && cargo test -q && cargo clippy -q --all-targets && cargo fmt --check`. Expected: all pass. Existing fog and
  Cartographer tests still pass, because `credit_cells` is untouched.

- [ ] **Step 5: Commit.**

  ```bash
  git add core/src/game.rs core/ffi/src/engine.rs
  git commit -m "feat: credit walked streets per game

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
  ```

---

### Task 3: Streets target, catalog and sizing

**Files:**

- Modify:
  - `core/src/catalog.rs` (`Verify`, `how()`, `is_progressive`, tests ~303 and ~331);
  - `core/data/quest_catalog.json` (the cartographer entry);
  - `core/src/assign.rs` (`Target`, goal text ~123, the `CoverCells` arm ~473, `Target::Cells` match arms elsewhere);
  - `docs/context/quest-catalog.md` (regenerated).

**Interfaces:**

- Produces:
  - `Verify::StreetShare` (JSON `{"type":"street_share"}`);
  - `Target::Streets { distance_m: f64 }`;
  - `pub const MIN_ZONE_STREET_M: f64 = 2000.0` in assign.rs.
- `Target::Cells` and `Verify::CoverCells` stay (old saves; catalog tests may still build them). Mark `Target::Cells` with
  `/// Old saves only: converted to Streets on load.`

- [ ] **Step 1: Write the failing tests.**
  - In catalog.rs tests:

  ```rust
  #[test]
  fn cartographer_counts_street_share() {
      let c = Catalog::builtin();
      let k = c.kinds.iter().find(|k| k.id == "cartographer").unwrap();
      assert_eq!(k.verify, Verify::StreetShare);
      assert!(k.is_progressive());
      assert_eq!(Verify::StreetShare.how(), "Walk a share of the zone's streets.");
  }
  ```

  - In assign.rs tests, following the existing `free_candidate` / `assign` test helpers there (search for a test that builds a
    `ZoneCtx` with an `Atlas`):

  ```rust
  #[test]
  fn cartographer_is_sized_as_a_street_distance() {
      // A zone whose atlas has 40 km of walkable streets; a 20-minute walk slot.
      let (t, effort, _) = cartographer_candidate(40_000.0, 20.0).expect("offered");
      let Target::Streets { distance_m } = t else { panic!("{t:?}") };
      assert!((distance_m - 20.0 * 75.0 * 0.7).abs() < 1.0);
      assert!((effort - 20.0).abs() < 0.5);
  }

  #[test]
  fn cartographer_skips_street_poor_zones() {
      assert!(cartographer_candidate(1_500.0, 20.0).is_none());
  }
  ```

    `cartographer_candidate(walkable_m, want)` is a test helper you write. It builds an `Atlas` with
    `walkable_len_m = walkable_m` (the fields are in scan.rs `Atlas`) and calls `free_candidate` with the cartographer `Kind`.

- [ ] **Step 2: Run the tests and confirm they fail.**

  Run: `cd core && cargo test -q --lib cartographer`. Expected: compile errors (no `StreetShare` / `Streets`).

- [ ] **Step 3: Implement.**
  - `Verify` gets `/// Walk a share of the zone's streets (Cartographer).` and `StreetShare,` placed after `CoverCells`. In
    `how()`: `Self::StreetShare => "Walk a share of the zone's streets.".into(),`. Add `| Verify::StreetShare` to
    `is_progressive`.
  - `quest_catalog.json`, the cartographer entry: `"blurb": "Walk more and more of the zone's streets."` and
    `"verify": { "type": "street_share" }`.
  - `Target` gets `/// Walk this much more of the zone's streets (a Cartographer chain member).` and
    `Streets { distance_m: f64 },`. Goal text: `Self::Streets { distance_m } => format!("Walk {} of new streets", crate::chain::distance_text(*distance_m)),`.
    If `distance_text` is private, make it `pub(crate)`.
  - In `free_candidate`, next to the `CoverCells` arm:

  ```rust
          Verify::StreetShare => {
              if z.atlas.walkable_m() < MIN_ZONE_STREET_M {
                  return None;
              }
              let distance_m = want * mode.m_per_min() * 0.7;
              Some((Target::Streets { distance_m }, want, "Streets of the zone".into()))
          }
  ```

  - **Exhaustive matches:** fix every one that now misses `Target::Streets`. `cargo build` lists them. Use the `Target::Cells`
    arm next to it as the guide:
    - assign.rs ~356 (`Target::Cells { .. } | …` gets `| Target::Streets { .. }`);
    - verify.rs `Tracker::new` (`Target::Streets { .. } => State::None`, since chain members never get trackers);
    - verify.rs `update` (no arm needed if it has `_ =>`);
    - game.rs `is_chain_target` and the effort scaling (~740, `other => other.clone()` covers it);
    - engine.rs `describe` (`Target::Streets { .. } => ("streets", None, None, 0.0, vec![], text)`).
  - Regenerate the doc: `python3 scripts/catalog_doc.py`.

- [ ] **Step 4: Run the tests and confirm they pass.**

  Run: `cd core && cargo test -q && cargo clippy -q --all-targets && cargo fmt --check`. Expected: all pass. Update the
  `is_progressive` list test only if the expected ids change (they do not: still `cartographer`, `step_up`, `wanderlust`).

- [ ] **Step 5: Commit.**

  ```bash
  git add core/src/catalog.rs core/src/assign.rs core/src/verify.rs core/src/game.rs core/ffi/src/engine.rs core/data/quest_catalog.json docs/context/quest-catalog.md
  git commit -m "feat: Cartographer targets a street distance

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
  ```

---

### Task 4: Streets chain unit: live counter, capped marks, text

**Files:**

- Modify:
  - `core/src/chain.rs`: `ChainUnit`, `amount_of`, `rule_text`, `amount_text`, and a new `share_text`;
  - `core/src/game.rs`: `chains()`, `counter_of`, `credit_cells` filter.

**Interfaces:**

- Produces:
  - `ChainUnit::Streets` (replaces `Cells`; counter and marks in metres);
  - `pub fn cap_marks(c: &mut Chain, total_street_m: f64)` in chain.rs;
  - `pub fn share_text(walked_m: f64, total_m: f64) -> String` (for example `"3.1%"`, `"12%"`);
  - `Chain::rule_text(&self, away_m: f64, street_total_m: Option<f64>) -> String`.

- [ ] **Step 1: Write the failing tests.**
  - In chain.rs tests:

  ```rust
  #[test]
  fn streets_chains_count_metres() {
      assert_eq!(amount_of(&Target::Streets { distance_m: 1050.0 }), Some((ChainUnit::Streets, 1050.0)));
      assert_eq!(amount_of(&Target::Cells { n: 8, cell_m: 150.0 }), Some((ChainUnit::Streets, 1200.0)), "old saves read as metres");
  }

  #[test]
  fn marks_cap_at_half_the_zone() {
      let mut c = derive(&[member(1, 1, "cartographer", Target::Streets { distance_m: 1000.0 }), member(2, 1, "cartographer", Target::Streets { distance_m: 2000.0 })]).remove(0);
      cap_marks(&mut c, 4000.0); // last mark would be 3000 m = 75 %
      assert!((c.marks[1].at - 2000.0).abs() < 1e-6, "last mark is 50 % of 4 km");
      assert!((c.marks[0].at - 2000.0 / 3.0).abs() < 1e-6, "earlier marks scale the same");
      let mut small = derive(&[member(1, 1, "cartographer", Target::Streets { distance_m: 500.0 })]).remove(0);
      cap_marks(&mut small, 40_000.0);
      assert!((small.marks[0].at - 500.0).abs() < 1e-6, "under the cap: unchanged");
  }

  #[test]
  fn share_text_rounds_by_size() {
      assert_eq!(share_text(1240.0, 40_000.0), "3.1%");
      assert_eq!(share_text(4800.0, 40_000.0), "12%");
      assert_eq!(share_text(0.0, 0.0), "0%");
  }

  #[test]
  fn streets_rule_text_uses_share_when_known() {
      let c = derive(&[member(1, 1, "cartographer", Target::Streets { distance_m: 2000.0 })]).remove(0);
      assert_eq!(c.rule_text(0.0, Some(40_000.0)), "Walk 5.0% of the zone's streets");
      assert_eq!(c.rule_text(0.0, None), "Walk 2 km of the zone's streets");
  }
  ```

    `5.0%` uses one decimal below 10 %; 2000 / 40000 is exactly 5, so expect `"5.0%"`. If `distance_text(2000.0)` returns
    `"2.0 km"`, match that.
  - In game.rs tests:

  ```rust
  #[test]
  fn cartographer_mark_completes_on_walked_share() {
      let mut g = test_game_with_cartographer(1_000.0); // helper: one Streets member of 1 km in zone 1, zone unlocked
      attach_street(&mut g, 4_000.0);                   // helper: a straight 4 km FOOT street through home + zone shape
      walk_east(&mut g, 0.0, 900.0);
      assert!(g.done.is_empty(), "900 m < 1 km");
      walk_east(&mut g, 900.0, 1_100.0);
      assert_eq!(g.done.len(), 1);
  }

  #[test]
  fn no_graph_never_completes() {
      let mut g = test_game_with_cartographer(1_000.0); // no attach_street
      walk_east(&mut g, 0.0, 2_000.0);
      assert!(g.done.is_empty());
      assert_eq!(g.chains()[0].unit, ChainUnit::Streets);
  }
  ```

- [ ] **Step 2: Run the tests and confirm they fail.**

  Run: `cd core && cargo test -q --lib chain && cargo test -q --lib game::tests::cartographer`. Expected: compile errors.

- [ ] **Step 3: Implement.**
  - chain.rs: rename the variant `Cells` → `Streets` (doc: `/// Street metres walked in the zone (shown as a share).`).
    `amount_of`:

  ```rust
          Target::Streets { distance_m } => Some((ChainUnit::Streets, *distance_m)),
          Target::Cells { n, cell_m } => Some((ChainUnit::Streets, f64::from(*n) * cell_m)), // old saves
  ```

  - New functions:

  ```rust
  /// The last mark is at most this share of the zone's streets: some streets are private, dead ends or not worth walking.
  pub const MAX_SHARE: f64 = 0.5;

  /// Scale a streets chain's marks down so the last is at most `MAX_SHARE` of the zone's streets.
  pub fn cap_marks(c: &mut Chain, total_street_m: f64) {
      let cap = total_street_m * MAX_SHARE;
      let last = c.total();
      if c.unit == ChainUnit::Streets && last > cap && last > 0.0 {
          let k = cap / last;
          c.marks.iter_mut().for_each(|m| m.at *= k);
      }
  }

  /// "3.1%" below 10 %, "12%" from there; "0%" with no streets.
  #[must_use]
  pub fn share_text(walked_m: f64, total_m: f64) -> String {
      let pct = if total_m > 0.0 { 100.0 * walked_m / total_m } else { 0.0 };
      if pct < 10.0 && pct > 0.0 { format!("{pct:.1}%") } else { format!("{}%", round_u64(pct)) }
  }
  ```

    `share_text(0.0, 0.0)` must give `"0%"`, hence the `pct > 0.0` guard.
  - `rule_text(&self, away_m: f64, street_total_m: Option<f64>)`, Streets arm:

  ```rust
              ChainUnit::Streets => match street_total_m.filter(|t| *t > 0.0) {
                  Some(total) => format!("Walk {} of the zone's streets", share_text(t, total)),
                  None => format!("Walk {} of the zone's streets", distance_text(t)),
              },
  ```

    `amount_text` Streets arm: `distance_text(at)`. The phone shows the % from `total_street_m` (Task 6). Update every
    `rule_text` caller (engine.rs `chains`) to pass `game.zone_street_m(c.zone)`.
  - game.rs:
    - `chains()` caps per zone:

  ```rust
      pub fn chains(&self) -> Vec<Chain> {
          let mut chains = chain::derive(&self.assignments);
          for c in &mut chains {
              if let Some(total) = self.zone_street_m(c.zone) {
                  chain::cap_marks(c, total);
              }
          }
          chains
      }
  ```

  - `counter_of`:

  ```rust
      fn counter_of(&self, c: &Chain) -> f64 {
          match c.unit {
              ChainUnit::Streets if self.zone_street_m(c.zone).is_some() => self.zone_walked_m(c.zone),
              ChainUnit::Streets => 0.0, // no graph yet: cannot complete
              _ => self.counters.progress.get(&c.id).copied().unwrap_or(0.0),
          }
      }
  ```

  - Delete `credit_cells` and its call in `reveal` (fog still updates `Fog.cells` for the Explorer goal).
  - Remove the Cells branch of `normalize_counters`' `migrate_cells`; Task 5 replaces it.
  - **Completion on walk:** `check` already calls `complete_reached` after `reveal`, so a walked share completes on the same
    fix. Bridged estimates go through `reveal` but return before `complete_reached`. Add
    `ev.extend(self.complete_reached(est.t_ms, None));` after the bridged `reveal` call, so a dead-reckoned walk can complete
    Cartographer. This follows the owner decision that bridged positions credit Cartographer.
  - engine.rs `chains`: `ChainUnit::Cells => "cells"` becomes `ChainUnit::Streets => "streets"`.

- [ ] **Step 4: Run the tests and confirm they pass.**

  Run: `cd core && cargo test -q && cargo clippy -q --all-targets && cargo fmt --check`. Expected: all pass. Old tests that
  assert `ChainUnit::Cells`, "squares" text or `credit_cells` behaviour (e.g. chain.rs ~208 and ~282, game.rs Cartographer
  tests) test removed behaviour: replace each with its Streets equivalent above. Do not delete tests of behaviour that still
  exists (fog cells, the Explorer goal).

- [ ] **Step 5: Commit.**

  ```bash
  git add core/src/chain.rs core/src/game.rs core/ffi/src/engine.rs
  git commit -m "feat: Cartographer chains count walked streets

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
  ```

---

### Task 5: Old saves convert

**Files:**

- Modify: `core/src/game.rs` (`normalize_counters`, ~line 1379)
- Test: `core/src/game.rs` tests

- [ ] **Step 1: Write the failing test.** Build the old-save JSON from a fresh test game, and simulate the old format by patching
  the JSON. Use the module's existing save/load test pattern (search `fn an_old_save` or `Game::load` in tests).

  ```rust
  #[test]
  fn old_cells_save_converts() {
      let mut g = test_game_with_cartographer(1_200.0);
      // Make it an old save: Cells target, 8 squares of progress, the first mark done.
      for a in &mut g.assignments {
          if matches!(a.target, Target::Streets { .. }) {
              a.target = Target::Cells { n: 8, cell_m: 150.0 };
          }
      }
      let chain_id = g.chains()[0].id.clone();
      g.counters.progress.insert(chain_id.clone(), 8.0);
      let done_id = g.chains()[0].marks[0].location_id;
      g.done.insert(done_id);
      let json = serde_json::to_string(&g).unwrap();
      let back = load_from_str(&json); // the module's helper around Game::load, or write the JSON to a temp dir and Game::load it

      assert!(back.assignments.iter().any(|a| a.target == Target::Streets { distance_m: 1200.0 }));
      assert!(!back.counters.progress.contains_key(&chain_id), "square counts are not read as metres");
      assert!(back.done.contains(&done_id), "done marks stay done");
  }
  ```

- [ ] **Step 2: Run the test and confirm it fails.**

  Run: `cd core && cargo test -q --lib old_cells_save_converts`. Expected: FAIL. The target is still `Cells`, and the progress
  entry is kept, or floored to the mark.

- [ ] **Step 3: Implement.** At the top of `normalize_counters`:

  ```rust
          // Cartographer used to count 150 m squares; it now counts street metres, read live. Convert old targets to the distance they
          // were sized from and drop their square counts (and the floor below would turn them into metres).
          let mut converted = BTreeSet::new();
          for a in &mut self.assignments {
              if let Target::Cells { n, cell_m } = a.target {
                  a.target = Target::Streets { distance_m: f64::from(n) * cell_m };
                  converted.insert(format!("{}:{}", a.zone, a.kind_id));
              }
          }
          for id in &converted {
              self.counters.progress.remove(id);
          }
  ```

  - In the existing floor loop, skip Streets chains: their counter is live, so it has no stored floor. Change the loop body to
    start with `if c.unit == ChainUnit::Streets { continue; }`.
  - Remove the now-unused `migrate_cells` / `cells_counted` logic. Keep the `cells_counted` field with `#[serde(default)]` so
    saves that have it still load. Its doc becomes `/// Unused; kept so older saves load.`

- [ ] **Step 4: Run the tests and confirm they pass.**

  Run: `cd core && cargo test -q && cargo clippy -q --all-targets && cargo fmt --check`. Expected: all pass.

- [ ] **Step 5: Commit.**

  ```bash
  git add core/src/game.rs
  git commit -m "fix: convert old Cartographer saves to streets

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
  ```

---

### Task 6: FFI: chain share, walked lines, walked-street width

**Files:**

- Modify:
  - `core/src/line_width.rs` (`LineKind::WalkedStreet`, its row, the test `ALL` and `street_width`);
  - `core/ffi/src/lib.rs` (mirror enum and `From`);
  - `core/ffi/src/engine.rs` (`ChainOut.total_street_m`, `Engine::walked_lines`).

**Interfaces:**

- Produces (Kotlin):
  - `ChainOut.totalStreetM: Double` (0 when unknown);
  - `ChainOut.unit == "streets"`;
  - `Engine.walkedLines(zone: UInt): List<List<GeoPoint>>`;
  - `LineKind.WALKED_STREET`.

- [ ] **Step 1: Write the failing test** in line_width.rs tests:
  - add `LineKind::WalkedStreet` to `ALL` (now 8 entries);
  - add `LineKind::WalkedStreet => 4.0,` to `street_width`.
  
  The existing tests then fail (no variant).

- [ ] **Step 2: Run the test and confirm it fails.**

  Run: `cd core && cargo test -q --lib line_width`. Expected: compile error.

- [ ] **Step 3: Implement.**
  - line_width.rs:
    - the variant `/// Streets walked this game, shown for a selected Cartographer chain.` `WalkedStreet,`;
    - in `street_and_floor`, `Self::WalkedStreet => (4.0, 1.5),`. If clippy's `match_same_arms` fires, merge it with
      `Self::Trail`.
  - ffi lib.rs: add `WalkedStreet` to the mirror enum and `LineKind::WalkedStreet => Self::WalkedStreet` to the `From` impl.
  - engine.rs `ChainOut`: add `/// Length of the zone's streets in metres (0 until the street graph is ready); streets chains show
    counter / this as a share.` and `pub total_street_m: f64,`. Fill it in `chains()` with
    `game.zone_street_m(c.zone).unwrap_or(0.0)`.
  - engine.rs `Engine`, following the locking pattern of `quests()`:

  ```rust
      /// Streets walked this game in `zone`, as lines for the map (empty with no game or no street graph).
      pub fn walked_lines(&self, zone: u32) -> Vec<Vec<GeoPoint>> {
          self.with_game(|g| g.walked_lines(zone)).unwrap_or_default().into_iter()
              .map(|l| l.into_iter().map(|p| GeoPoint { lat: p.lat, lon: p.lon }).collect()).collect()
      }
  ```

    Use whatever accessor `quests()` uses for the current game in place of `with_game`.

- [ ] **Step 4: Run the checks.**

  Run: `cd core && cargo test -q && cargo clippy -q --all-targets && cargo fmt --check`. Expected: pass.

- [ ] **Step 5: Commit.**

  ```bash
  git add core/src/line_width.rs core/ffi/src/lib.rs core/ffi/src/engine.rs
  git commit -m "feat: expose walked streets and chain share

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
  ```

---

### Task 7: Android: share text and the walked-streets layer

**Files:**

- Modify:
  - `android/app/src/main/java/dev/apgo2/ui/ChainFormat.kt` (unit words ~42-51, `next` ~68);
  - `android/app/src/test/.../ChainFormatTest.kt` (the `"squares"` cases ~34 and ~91);
  - `android/app/src/main/java/dev/apgo2/PlayLayout.kt:8` (`OFF_MAP`: `"cells"` → `"streets"`);
  - `android/app/src/main/java/dev/apgo2/DevSimulator.kt:156` (`"cells"` → `"streets"`);
  - `android/app/src/main/java/dev/apgo2/MapStyle.kt` (`MapSource.WALKED`, a layer);
  - `android/app/src/main/java/dev/apgo2/QuestMap.kt` (selected chain → walked lines);
  - `android/app/src/main/java/dev/apgo2/PlayScreen.kt` (~182: pass `m.selectedChain` to `QuestMap`).

**Interfaces:**

- Consumes:
  - `ChainOut.unit == "streets"`;
  - `ChainOut.totalStreetM`, `ChainOut.counter` and `ChainOut.marks[i].at` (metres);
  - `Engine.walkedLines(zone)`;
  - `LineKind.WALKED_STREET`;
  - `Units.distance(m)` and `ApgoPalette.quest(state)`.

- [ ] **Step 1: Write the failing tests** in `ChainFormatTest.kt`, replacing the `"squares"` expectations with streets ones:

  ```kotlin
  @Test
  fun streetsAmountIsAShareWhenTheTotalIsKnown() {
      assertEquals("3.1%", ChainFormat.amount("streets", 1240.0, totalStreetM = 40_000.0))
      assertEquals("12%", ChainFormat.amount("streets", 4800.0, totalStreetM = 40_000.0))
  }

  @Test
  fun streetsAmountIsADistanceWithoutATotal() {
      assertEquals(Units.distance(1200.0), ChainFormat.amount("streets", 1200.0, totalStreetM = 0.0))
  }

  @Test
  fun streetsNextSaysHowFarIsLeft() {
      // counter 1240 m of a 2000 m mark: about 760 m more
      assertEquals("next: 5.0% (about ${Units.distance(760.0)} more)", ChainFormat.next(streetsChain(counter = 1240.0, mark = 2000.0, total = 40_000.0)))
  }
  ```

  `ChainFormat.amount` gains an optional `totalStreetM: Double = 0.0` parameter; update its callers (`PlayDetails.kt:160` and
  `ChainFormat.next`) to pass `c.totalStreetM`. Write a `streetsChain(...)` test helper that builds a `ChainOut` with
  `unit = "streets"`, following the file's existing `ChainOut` builder.

- [ ] **Step 2: Run the tests and confirm they fail.**

  Run: `cd android && ./gradlew :app:testDebugUnitTest --console=plain -q --tests '*ChainFormatTest*'`. Expected: FAIL.

- [ ] **Step 3: Implement.**
  - **ChainFormat:** for `"streets"`, the amount is `share(at, total)` when `total > 0`, else `Units.distance(at)`.
    - `share` mirrors core `share_text`: one decimal below 10 %, a whole number from 10 %, `"0%"` at 0.
    - `next` for streets is `"next: ${amount(mark)} (about ${Units.distance(mark - counter)} more)"`.
    - Other units are unchanged.
  - **`OFF_MAP` and `DevSimulator`:** replace `"cells"` with `"streets"`.
  - **MapStyle:**
    - add `const val WALKED = "walked"` to `MapSource` and include it in `ALL`;
    - add the layer in `traceLayers()`, right after `trace-layer`, so it sits above the trace and below quest routes:

  ```kotlin
              // Streets walked this game, shown while a Cartographer chain is selected (colour = the chain's state, set per update).
              LineLayer("walked-streets", MapSource.WALKED).withProperties(
                  lineColor(Expression.get(MapProp.COLOR)),
                  scaledWidth(LineKind.WALKED_STREET),
                  lineCap(ROUND),
                  lineJoin(ROUND),
              ),
  ```

  Add `const val COLOR = "c"` to `MapProp` if it has no colour property yet.

  **QuestMap:** take `selectedChain: ChainOut?` and `engine` (or a `walkedLines: (UInt) -> List<List<GeoPoint>>` lambda,
  matching how QuestMap already gets data), and add:

  ```kotlin
      LaunchedEffect(style, selectedChain?.id, selectedChain?.counter) {
          val c = selectedChain?.takeIf { it.unit == "streets" }
          val lines = c?.let { walkedLines(it.zone) }.orEmpty()
          val color = ApgoPalette.quest(if (c != null && c.marks.all { it.reached }) "done" else "progress").hex()
          holder.show(MapSource.WALKED, lines.map { GeoJson.line(it, mapOf(MapProp.COLOR to color)) })
      }
  ```

  The effect is keyed on the chain id and counter, so it refetches when progress or the selection changes, and an empty list
  clears it. Use the existing `GeoJson` line-feature builder (see how `MapFeatures.lines` builds a LineString) instead of
  writing a new one.

  **PlayScreen:** pass `m.chains.firstOrNull { it.id == m.selectedChain }` to `QuestMap`.

- [ ] **Step 4: Run the checks.**

  Run: `just check-android`. Expected: pass, including the colour-token check (no colour literals: the colour comes from
  `ApgoPalette.quest`).

- [ ] **Step 5: Commit.**

  ```bash
  git add android/
  git commit -m "feat: show walked streets for Cartographer

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
  ```

---

### Task 8: Docs, device check, PR

**Files:**

- Modify:
  - `docs/context/ui-design-system.md` (map rules: the walked-streets layer);
  - `docs/context/v1-architecture-and-status.md` (Cartographer now counts street coverage; where the code is);
  - `docs/context/progression-zones-and-tools.md`, only if it describes Cartographer cells (`grep -n -i cartographer docs/context/*.md`).

- [ ] **Step 1: Update the docs.** One or two lines each, matching the files' style. State that the Explorer goal still uses
  squares (issue #102).
- [ ] **Step 2: Run everything.** `just check` and `markdownlint-cli2 "docs/**/*.md"`. Expected: green.
- [ ] **Step 3: Device check.** Run `just android-run` (set `ANDROID_SERIAL` to the phone if the emulator is attached too).
  - Start a new game with a Cartographer chain.
  - Walk a few blocks.
  - The chain shows a % rising and "about X km more".
  - Selecting it highlights the walked streets.
  - Standing still adds nothing.
- [ ] **Step 4: Commit the docs** (`docs: Cartographer by street coverage`), push with `-u`, and open the PR to main using
  `.github/pull_request_template.md`.
