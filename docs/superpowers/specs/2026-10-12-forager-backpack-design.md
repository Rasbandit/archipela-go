# Forager backpack and respawning items (#142)

Owner decisions 2026-10-11 (quest card review) and 2026-10-12: one shared backpack, bag size 20.

## Goal

Forager quests stop being a fixed handful of items. The map always holds plenty, picked items come back after a while,
and what you carry goes in one backpack with a soft limit: carrying more than fits slows your step count but is never
refused. The quest card shows the backpack, not the map total.

## Rules

1. **Abundant items.** Each open forager quest keeps `3 × need` live items on the map, placed as today (street points
   around home, spaced from other quests, `pick_r_m` pick-up radius).
2. **Picking.** A fix within the pick-up radius of a live item removes it from the map and adds one to that quest's
   `carried`. Always allowed, whatever the bag holds.
3. **Respawn.** The first pick while no top-up is pending sets `next_spawn_ms = t + 3 h`. At or after that time, the
   quest tops back up to `3 × need` and clears `next_spawn_ms`. Event-driven: the top-up runs on the first fix or steps
   event after it is due, and `Game::next_due_ms` includes the earliest `next_spawn_ms`, so the app's one scheduled
   wake-up also refreshes an idle map. No ticking timer. A top-up needs the zone's street points; until the streets
   build has run, it waits (still due).
4. **One backpack.** The game has `bag_size` (default 20; upgrades later, with the shop). Carried total = sum of
   `carried` over forager quests. `over = max(0, total − bag_size)`.
5. **Overload penalty.** Steps credited while over the limit are multiplied by `max(0.5, 1 − 0.05 × over)`. It applies
   only where steps are credited (step quests and step chains), never to distance, time or presence.
6. **Banking.** Unchanged: reaching Home banks everything carried; a quest's check goes out when `banked ≥ need`
   (the economy rule "a check goes out when it is revealed").
7. **Done quests.** A done forager quest's items leave the map and do not respawn.

## State and saves

- `Game.forage: BTreeMap<location_id, Forage>` with `Forage { items: Vec<Item { id: u32, p: Point }>, next_id: u32,
  next_spawn_ms: Option<i64> }`; `Game.bag_size: u32` (serde default 20).
- `Collected { carried, banked }` stays per quest; `picked` (indexes into the target's points) is no longer written.
- Old saves: a forager quest with no `forage` entry is seeded on load from its `Target::Collect` points minus the
  `picked` indexes, then topped up to `3 × need` on the next top-up. `Target::Collect.pts` stays as the original
  placement (the quest's anchor and pin), not the live set.

## Surfaces

- **FFI.** `QuestOut.collect.items` = the live items (map pins), not the original points. New `Engine::bag() -> BagOut
  { size, items: Vec<String> (each carried item's theme, for its icon), over: u32, step_factor: f32, load: String }`
  with `load` one of Empty / Light / Full / "Heavy · steps −N%" (text from the core).
- **Card** (spec `2026-10-11-quest-card-design.md`, forager section): above the Banked bar, the backpack row:
  `Backpack · N` and the load word; one slot per bag space with the carried item's icon, free slots dashed, items past
  the bag size amber. No divider, no "+" slot. The game panel shows a "Heavy · steps −N%" chip while over.

## Tests (core first)

- Seeding to `3 × need`; picking removes the item and adds to `carried`; a pick sets `next_spawn_ms` once.
- Top-up at or after the due time on a fix, on steps and via `next_due_ms`; not before; not for done quests; waits
  without streets.
- Penalty: 0 over → 1.0; 2 over → 0.9; 20 over → 0.5 floor; applied to step credit only.
- Banking still sends the check at `banked ≥ need`; an over-full bag still banks everything.
- Old save migration from `Target::Collect` points and `picked`.
- `bag()` load words and per-item themes across two forager quests (shared bag).
- Android: the backpack row in `QuestCardA11yTest` (light and dark), slots wrap at 20.

## Out of scope

Buying bag upgrades, bank places other than Home, money, eggs; changing how forager quests are first placed.
