//! Forager items on the map and the shared backpack (#142, spec `2026-10-12-forager-backpack-design.md`): items are picked up into
//! one bag with a soft limit, carrying more than fits slows step counting, and picked items come back a while later.

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::geo::{distance_m, Point};

/// How many items fit in the backpack before steps slow down, until bag upgrades exist.
pub const BAG_SIZE: u32 = 20;
/// How long after the first pick the map tops back up.
pub const RESPAWN_MS: i64 = 3 * 60 * 60_000;
/// Live items an open forager keeps on the map, per item it needs.
pub const LIVE_PER_NEED: u32 = 3;

/// One item on the map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    /// Stable within its quest, so a picked item never comes back under the same id.
    pub id: u32,
    /// Where it lies.
    pub p: Point,
}

/// A forager quest's live items and when it tops up next.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Forage {
    /// Items still on the map.
    pub items: Vec<Item>,
    /// The id the next new item gets.
    pub next_id: u32,
    /// When the map tops back up; set by the first pick, cleared once full again.
    pub next_spawn_ms: Option<i64>,
    /// Whether the quest was filled to its live size once (the first streets build after it was placed or loaded).
    #[serde(default)]
    pub filled: bool,
}

impl Forage {
    /// Items at `pts`, numbered from 0.
    #[must_use]
    pub fn seed(pts: &[Point]) -> Self {
        let items: Vec<Item> = (0u32..).zip(pts).map(|(id, p)| Item { id, p: *p }).collect();
        Self { next_id: crate::num::count_u32(items.len()), items, next_spawn_ms: None, filled: false }
    }

    /// Picks up every item within `r` of `at` at `t_ms`; returns how many. Never refused, whatever the bag holds.
    pub fn pick(&mut self, at: Point, r: f64, t_ms: i64) -> u32 {
        let before = self.items.len();
        self.items.retain(|i| distance_m(i.p, at) > r);
        let picked = crate::num::count_u32(before - self.items.len());
        if picked > 0 && self.next_spawn_ms.is_none() {
            self.next_spawn_ms = Some(t_ms + RESPAWN_MS);
        }
        picked
    }

    /// Whether a top-up is due at `now_ms`.
    #[must_use]
    pub fn due(&self, now_ms: i64) -> bool {
        self.next_spawn_ms.is_some_and(|t| t <= now_ms)
    }

    /// Adds items from `pool` (at least `spacing_m` from every live item and each other) until there are `want`; returns how many it
    /// added. Clears the due time once full, so a short pool tries again on the next event.
    pub fn top_up(&mut self, want: usize, pool: &[Point], spacing_m: f64, rng: &mut StdRng) -> usize {
        let mut order: Vec<Point> = pool.to_vec();
        order.shuffle(rng);
        let mut added = 0;
        for p in order {
            if self.items.len() >= want {
                break;
            }
            if self.items.iter().all(|i| distance_m(i.p, p) >= spacing_m) {
                self.items.push(Item { id: self.next_id, p });
                self.next_id += 1;
                added += 1;
            }
        }
        if self.items.len() >= want {
            self.next_spawn_ms = None;
        }
        added
    }

    /// No items and nothing due (a done quest).
    pub fn clear(&mut self) {
        self.items.clear();
        self.next_spawn_ms = None;
    }
}

/// How much a step counts while `over` items past the bag size are carried: 5% less per item, at least half.
#[must_use]
pub fn step_factor(over: u32) -> f64 {
    (1.0 - 0.05 * f64::from(over)).max(0.5)
}

/// The bag's load in words: Empty, Light, Full or "Heavy · steps −N%".
#[must_use]
pub fn load_text(carried: u32, bag: u32) -> String {
    match carried {
        0 => "Empty".into(),
        n if n < bag => "Light".into(),
        n if n == bag => "Full".into(),
        n => format!("Heavy · steps −{}%", crate::num::round_u64((1.0 - step_factor(n - bag)) * 100.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;
    use rand::SeedableRng;

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }
    fn east(m: f64) -> Point {
        destination(home(), 90.0, m)
    }

    #[test]
    fn seeding_numbers_the_items() {
        let f = Forage::seed(&[home(), east(50.0)]);
        assert_eq!((f.items.len(), f.next_id, f.items[1].id, f.next_spawn_ms), (2, 2, 1, None));
    }

    #[test]
    fn one_fix_can_pick_several_items_and_never_refuses() {
        let mut f = Forage::seed(&[home(), east(10.0), east(500.0)]);
        assert_eq!(f.pick(home(), 25.0, 1_000), 2);
        assert_eq!(f.items.iter().map(|i| i.id).collect::<Vec<_>>(), [2]);
        assert_eq!(f.next_spawn_ms, Some(1_000 + RESPAWN_MS));
        assert_eq!(f.pick(east(500.0), 25.0, 9_000), 1);
        assert_eq!(f.next_spawn_ms, Some(1_000 + RESPAWN_MS), "the first pick sets the clock; later picks do not push it");
        assert_eq!(f.pick(home(), 25.0, 9_500), 0, "nothing left there");
    }

    #[test]
    fn due_only_at_or_after_the_spawn_time() {
        let mut f = Forage::default();
        assert!(!f.due(i64::MAX), "nothing picked: nothing due");
        f.next_spawn_ms = Some(100);
        assert!(!f.due(99) && f.due(100) && f.due(5_000));
    }

    #[test]
    fn top_up_refills_to_want_spaced_and_clears_the_clock() {
        let mut f = Forage::seed(&[home()]);
        f.next_spawn_ms = Some(0);
        let pool: Vec<Point> = (1..40).map(|i| east(f64::from(i) * 30.0)).collect();
        let added = f.top_up(6, &pool, 60.0, &mut StdRng::seed_from_u64(1));
        assert_eq!((added, f.items.len(), f.next_spawn_ms), (5, 6, None));
        assert!(f.items.iter().all(|a| f.items.iter().all(|b| a.id == b.id || distance_m(a.p, b.p) >= 60.0)));
        assert_eq!((f.items.iter().map(|i| i.id).max(), f.next_id), (Some(5), 6));
    }

    #[test]
    fn a_short_pool_tops_up_what_it_can_and_stays_due() {
        let mut f = Forage { next_spawn_ms: Some(0), ..Forage::default() };
        assert_eq!(f.top_up(6, &[home(), east(100.0)], 60.0, &mut StdRng::seed_from_u64(1)), 2);
        assert_eq!(f.next_spawn_ms, Some(0), "not full: try again next time");
    }

    #[test]
    fn clearing_leaves_nothing_on_the_map_or_due() {
        let mut f = Forage::seed(&[home()]);
        f.next_spawn_ms = Some(5);
        f.clear();
        assert_eq!((f.items.len(), f.next_spawn_ms), (0, None));
    }

    #[test]
    fn the_penalty_is_five_percent_per_item_over_with_a_floor() {
        assert!((step_factor(0) - 1.0).abs() < 1e-12);
        assert!((step_factor(2) - 0.9).abs() < 1e-12);
        assert!((step_factor(20) - 0.5).abs() < 1e-12);
        assert!((step_factor(400) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn the_bag_reads_empty_light_full_or_heavy() {
        assert_eq!([load_text(0, 20), load_text(3, 20), load_text(20, 20), load_text(22, 20)], ["Empty", "Light", "Full", "Heavy · steps −10%"]);
    }
}
