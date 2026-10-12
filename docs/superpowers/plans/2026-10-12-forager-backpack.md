# Forager Backpack Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Forager items become abundant and respawn, everything carried shares one soft-limit backpack (20) whose
overload slows step counting, and the quest card shows the backpack.

**Architecture:** A new pure module `core/src/forage.rs` owns live items, picking, top-ups, the step factor and the
bag's load text. `Game` keeps `forage` (saved), `bag_size` (saved, default 20) and `forage_pools` (not saved, filled by
the streets build that already loads the scans), picks items on fixes, tops up on fixes/steps/ticks when due, and
credits steps through the factor. The FFI exposes live items and `Engine::bag()`; Kotlin draws the backpack row.

**Tech Stack:** Rust core (`apgo-core`), UniFFI (`apgo-ffi`), Kotlin/Compose, Roborazzi ATF tests.

**Spec:** `docs/superpowers/specs/2026-10-12-forager-backpack-design.md`

## Global Constraints

- Bag size default **20**; live items per open forager quest **3 × need**; respawn **3 h** after the first pick.
- Penalty factor `max(0.5, 1 − 0.05 × over)`, on step credit only (step quests and step chains).
- Events over polling: no ticking timer; top-ups run on fix/steps/tick events, and `next_due_ms` includes the earliest
  `next_spawn_ms`.
- Logic in the core; Kotlin only draws. Colours are palette tokens. TDD. Clippy pedantic: casts via `crate::num`.
- Old saves load: new fields are `#[serde(default)]`.

## Review Focus

1. A save from before this change with a half-collected forager: picked items stay picked, carried/banked unchanged,
   the map shows only the unpicked ones. → Task 2 test `old_saves_seed_live_items_from_the_unpicked_points`.
2. Picking two items in one fix (both within 25 m): both count, the bag can go over 20, nothing is refused. → Task 1
   test `one_fix_can_pick_several_items_and_never_refuses`.
3. A top-up due while the streets build has not run (pools empty): nothing placed, still due, placed after the build.
   → Task 2 test `a_due_top_up_waits_for_the_pools`.
4. Steps credited across banking: once the bag empties at Home, the factor returns to 1 for the next steps. → Task 3
   test `banking_lifts_the_step_penalty`.
5. A forager completed (banked ≥ need): its items leave the map and never respawn, even with a pending
   `next_spawn_ms`. → Task 2 test `a_done_forager_clears_its_items_and_never_respawns`.

---

### Task 1: `forage` module (pure)

**Files:** Create `core/src/forage.rs`; modify `core/src/lib.rs` (`pub mod forage;`).

**Interfaces — Produces:**

```rust
pub const BAG_SIZE: u32 = 20;
pub const RESPAWN_MS: i64 = 3 * 60 * 60_000;
pub const LIVE_PER_NEED: u32 = 3;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] pub struct Item { pub id: u32, pub p: Point }
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Forage { pub items: Vec<Item>, pub next_id: u32, pub next_spawn_ms: Option<i64> }
impl Forage {
    pub fn seed(pts: &[Point]) -> Self;                                   // ids 0.., next_id = len
    pub fn pick(&mut self, at: Point, r: f64, t_ms: i64) -> u32;          // removes items within r; sets next_spawn_ms once
    pub fn due(&self, now_ms: i64) -> bool;                               // next_spawn_ms <= now
    pub fn top_up(&mut self, want: usize, pool: &[Point], spacing_m: f64, rng: &mut StdRng) -> usize; // adds up to want-len, spaced; clears next_spawn_ms when it reached want
    pub fn clear(&mut self);                                              // items empty, next_spawn_ms None
}
pub fn step_factor(over: u32) -> f64;                                     // max(0.5, 1 - 0.05*over)
pub fn load_text(carried: u32, bag: u32) -> String;                       // "Empty" | "Light" | "Full" | "Heavy · steps −N%"
```

- [ ] **Step 1: Failing tests** in `forage.rs` `mod tests`:

```rust
#[test] fn seeding_numbers_the_items() { let f = Forage::seed(&[home(), east(50.0)]); assert_eq!((f.items.len(), f.next_id, f.items[1].id), (2, 2, 1)); }
#[test] fn one_fix_can_pick_several_items_and_never_refuses() {
    let mut f = Forage::seed(&[home(), east(10.0), east(500.0)]);
    assert_eq!(f.pick(home(), 25.0, 1_000), 2);
    assert_eq!(f.items.iter().map(|i| i.id).collect::<Vec<_>>(), [2]);
    assert_eq!(f.next_spawn_ms, Some(1_000 + RESPAWN_MS));
    f.pick(east(500.0), 25.0, 9_000);
    assert_eq!(f.next_spawn_ms, Some(1_000 + RESPAWN_MS), "the first pick sets the clock; later picks do not push it");
}
#[test] fn top_up_refills_to_want_spaced_and_clears_the_clock() {
    let mut f = Forage::seed(&[home()]); f.next_spawn_ms = Some(0);
    let pool: Vec<Point> = (1..40).map(|i| east(f64::from(i) * 30.0)).collect();
    let added = f.top_up(6, &pool, 60.0, &mut StdRng::seed_from_u64(1));
    assert_eq!((added, f.items.len(), f.next_spawn_ms), (5, 6, None));
    assert!(f.items.iter().all(|a| f.items.iter().all(|b| a.id == b.id || distance_m(a.p, b.p) >= 60.0)));
    assert_eq!(f.items.iter().map(|i| i.id).max(), Some(5)); assert_eq!(f.next_id, 6);
}
#[test] fn a_short_pool_tops_up_what_it_can_and_stays_due() {
    let mut f = Forage::default(); f.next_spawn_ms = Some(0);
    assert_eq!(f.top_up(6, &[home(), east(100.0)], 60.0, &mut StdRng::seed_from_u64(1)), 2);
    assert_eq!(f.next_spawn_ms, Some(0), "not full: try again next time");
}
#[test] fn the_penalty_is_five_percent_per_item_over_with_a_floor() {
    assert!((step_factor(0) - 1.0).abs() < 1e-12); assert!((step_factor(2) - 0.9).abs() < 1e-12); assert!((step_factor(20) - 0.5).abs() < 1e-12);
}
#[test] fn the_bag_reads_empty_light_full_or_heavy() {
    assert_eq!([load_text(0, 20), load_text(3, 20), load_text(20, 20), load_text(22, 20)], ["Empty", "Light", "Full", "Heavy · steps −10%"]);
}
```

- [ ] **Step 2:** `cargo test -p apgo-core --lib forage::` → FAIL (module missing).
- [ ] **Step 3: Implement.** `pick` uses `retain`, counting removed; `top_up` shuffles `pool`, adds points at least
  `spacing_m` from every live item and from each other, assigns `next_id++`, clears `next_spawn_ms` only when
  `items.len() >= want`. `load_text` uses `crate::num::round_u64((1.0 - step_factor(over)) * 100.0)`.
- [ ] **Step 4:** run → PASS; `cargo clippy -p apgo-core --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `feat: forage items, bag load and step factor`.

---

### Task 2: Game keeps live items, picks, tops up

**Files:** Modify `core/src/game.rs` (fields, load, `check`, `tick`, `next_due_ms`, re-roll path, `quest_views`),
`core/src/assign.rs` (`pub fn forage_pool(z: &ZoneCtx, p: &AssignParams, tier: u8, r: f64) -> Vec<Point>` reusing
`item_band` + `quest_pool`), `core/src/verify.rs` (tracker no longer picks: it only banks and reports).

**Interfaces:**

- Consumes: Task 1 `Forage`, `LIVE_PER_NEED`.
- Produces:
  - `Game.forage: BTreeMap<i64, Forage>` (`#[serde(default)]`)
  - `Game.bag_size: u32` (`#[serde(default = "default_bag")]`, 20)
  - `Game.forage_pools: BTreeMap<i64, Vec<Point>>` (`#[serde(skip)]`)
  - `Game::set_forage_pools(&mut self, atlases: &[&Atlas])`
  - `Game::carried_total(&self) -> u32`
  - `QuestView.live_items: Vec<Point>` (forager live items; empty for others)

- [ ] **Step 1: Failing tests** in `game.rs` tests (use the existing forager helpers around `game.rs:3034`):
  - `picking_on_a_fix_removes_live_items_and_carries_them`: a fix on an item → `forage[id].items` one fewer,
    `collected[id].carried == 1`.
  - `old_saves_seed_live_items_from_the_unpicked_points`: JSON without `forage`, with `collected.picked = {0}` →
    after `Game::load`, `forage[id].items` = target points minus index 0; carried/banked unchanged.
  - `a_due_top_up_waits_for_the_pools`: set `next_spawn_ms = Some(0)`, `forage_pools` empty → `tick(1)` adds nothing;
    after `set_forage_pools` with a pool, the next `tick` fills to `3 × need`.
  - `next_due_ms_includes_the_respawn`: one pick at t → `next_due_ms(t)` = `t + RESPAWN_MS` when nothing earlier.
  - `a_done_forager_clears_its_items_and_never_respawns`: bank to need → `forage[id].items` empty and
    `next_due_ms` ignores it.
  - `the_view_shows_live_items`: `quest_views` `live_items` equals the forage items' points.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3: Implement.**
  - `Tracker` collect branch drops the pick loop: it only banks at home and sets progress from `Collected`.
  - In `Game::check`, before trackers update: for each open, unlocked forager, `n = forage.pick(p, r, t)` and
    `collected.carried += n`; drop the tracker so it rebuilds from `collected`.
  - In `Game::load`, seed missing `forage` entries from `Target::Collect.pts` minus `Collected.picked`.
  - `top_up_due(&mut self, now)`: called from `check`, `on_steps` and `tick`. It tops up each due, not-done forager
    from `forage_pools[id]` to `LIVE_PER_NEED × need` with spacing 60 m and a seeded rng (`seed ^ id ^ now`).
  - `complete` clears a forager's items.
  - `next_due_ms` takes the min of today's value and every not-done `forage.next_spawn_ms`.
  - The re-roll path (`replace_unpicked`) reseeds `forage[id]` from the new target's unpicked points.
- [ ] **Step 4:** `cargo test -p apgo-core --lib` → PASS (the whole suite: existing forager tests must stay green;
  where an existing test asserts tracker-side picking, it now goes through `Game`, so change only its setup, never
  its expected numbers).
- [ ] **Step 5: Commit** `feat: live forager items that respawn`.

---

### Task 3: Shared bag and the step penalty

**Files:** Modify `core/src/game.rs` (`Counters.steps_effective: f64` serde default; `credit_steps`; trackers get
the effective total).

**Interfaces — Produces:** `Game::step_factor(&self) -> f64` = `forage::step_factor(carried_total − bag_size)`;
`Counters.steps_effective`.

- [ ] **Step 1: Failing tests:**
  - `overloaded_steps_count_less_for_chains_and_step_quests`: carry 22 (bag 20) → `credit_steps` +1000 raw → chain
    progress +900, a `Target::Steps` quest advances 900.
  - `banking_lifts_the_step_penalty`: carry 22, bank at home, +1000 raw → +1000.
  - `distance_and_time_are_not_penalised`: carry 40, a line quest's coverage and `Away` minutes advance as without
    a load.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3: Implement.**
  - `credit_steps` computes `gained × step_factor()`, adds it to chains and to `steps_effective`.
  - `check` passes `Some(round_i64(steps_effective))` to step trackers instead of the raw total, when a raw total
    came with the estimate.
- [ ] **Step 4:** run → PASS.
- [ ] **Step 5: Commit** `feat: shared backpack slows steps when overloaded`.

---

### Task 4: FFI

**Files:** Modify `core/ffi/src/engine.rs`.

**Interfaces — Produces:**
`BagOut { size: u32, items: Vec<String>, over: u32, step_factor: f32, load: String }`;
`Engine::bag() -> Option<BagOut>` (`None` with no game); `CollectOut.items` = live items (all `picked = false`);
`swap_streets` calls `g.set_forage_pools(atlases)`.

- [ ] **Step 1: Failing engine tests:** `bag()` lists each carried item's theme across two foragers (acorns ×2,
  diamonds ×1 → `["acorns","acorns","diamonds"]`), `load == "Light"`; `quests()` forager `collect.items` equals the
  live items; after `refresh_streets`, a due top-up fills on the next `tick`.
- [ ] **Step 2:** run → FAIL. **Step 3:** implement. **Step 4:** `just check-rust` → PASS.
- [ ] **Step 5: Commit** `feat: backpack over the FFI`.

---

### Task 5: Android backpack row

**Files:** Modify `android/app/src/main/java/dev/apgo2/QuestCard.kt` (backpack row above the Banked block for
`q.collect != null`), `PlayScreen.kt` (pass `m.engine.bag()` read off the main thread with the quests; a "Heavy" chip
in the game panel when `over > 0`), `ui/Palette.kt` (an `overload` amber token reusing `progressLight/Dark` if
contrast passes), `test/.../QuestCardA11yTest.kt`, `test/.../ui/ContrastTest.kt`, fixtures that build `CollectOut`.

- [ ] **Step 1: Failing tests:** `QuestCardA11yTest` forager card with a bag of 22/20 in light and dark (ATF passes;
  the row's semantics read "Backpack, 22 items, Heavy · steps −10%"); `ContrastTest` for the overload slot colour.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3: Implement:** header row `Backpack · N` + load word (amber when over); a `FlowRow` of 22dp slots, one
  per bag space, item icon via `ApgoIcons.collectible(theme)`, free slots dashed, over-limit slots amber; no divider,
  no "+".
- [ ] **Step 4:** `just check-android` → PASS; phone check (`just android-run`) light and dark.
- [ ] **Step 5: Commit** `feat: backpack row on the forager card`.

---

### Task 6: Docs and PR

- [ ] `docs/context/economy-eggs-buddies.md`: backpack built (size 20, penalty, respawn), what remains (upgrades, bank
  places). `v1-architecture-and-status.md` Play line. `ui-design-system.md` backpack row rules.
- [ ] `just check-rust check-android check-hygiene`; PR "feat: forager backpack", closes #142.
