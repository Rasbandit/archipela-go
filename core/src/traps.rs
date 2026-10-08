//! Trap effects (the item is the server's/solo table's job; the effect is ours). Every trap has a way out.

use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::geo::{destination, distance_m, Point};

const MIN: i64 = 60_000;
const THAW_RADIUS_M: f64 = 40.0;

/// A negative effect applied to the player, each with a way out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Trap {
    /// No checks count until you reach `thaw` (or the timer runs out).
    Freeze {
        /// Where to go to thaw.
        thaw: Point,
        /// Expiry time in Unix milliseconds.
        until_ms: i64,
    },
    /// The map hides all quests.
    Fog {
        /// Expiry time in Unix milliseconds.
        until_ms: i64,
    },
    /// Notifications muted.
    Silence {
        /// Expiry time in Unix milliseconds.
        until_ms: i64,
    },
    /// Checks only count within `radius_m` of `center`.
    Leash {
        /// Middle of the allowed area.
        center: Point,
        /// Radius of the allowed area in metres.
        radius_m: f64,
        /// Expiry time in Unix milliseconds.
        until_ms: i64,
    },
    /// Visit `waypoint` before any check counts.
    Detour {
        /// The place that must be visited.
        waypoint: Point,
        /// Whether it has been visited yet.
        visited: bool,
        /// Expiry time in Unix milliseconds.
        until_ms: i64,
    },
    /// Cover `need_m` meters before any check counts.
    Toll {
        /// Distance that must be covered, in metres.
        need_m: f64,
        /// Distance covered so far, in metres.
        moved_m: f64,
        /// Expiry time in Unix milliseconds.
        until_ms: i64,
    },
    /// Dwell quests take twice as long.
    Slow {
        /// Expiry time in Unix milliseconds.
        until_ms: i64,
    },
}

/// The traps currently affecting the player.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Traps {
    /// Traps that have not yet expired or been cleared.
    pub active: Vec<Trap>,
}

fn pool_point(pool: &[Point], from: Point, min: f64, max: f64, rng: &mut StdRng) -> Point {
    let near: Vec<&Point> = pool.iter().filter(|p| (min..=max).contains(&distance_m(from, **p))).collect();
    let want = f64::midpoint(min, max);
    // No street at the usual distance: the street point closest to it (#51: a point to reach is never made up off the streets).
    let closest = || pool.iter().min_by(|a, b| (distance_m(from, **a) - want).abs().total_cmp(&(distance_m(from, **b) - want).abs())).copied();
    match near.choose(rng) {
        Some(p) => **p,
        None => closest().unwrap_or_else(|| destination(from, rng.random_range(0.0..360.0), want)),
    }
}

impl Traps {
    /// Start the effect of a trap item. Returns a player-facing message (also for honor traps).
    pub fn trigger(&mut self, item: &str, now_ms: i64, pos: Option<Point>, home: Point, pool: &[Point], rng: &mut StdRng) -> Option<String> {
        let at = pos.unwrap_or(home);
        let (trap, msg) = match item {
            "Freeze Trap" => {
                let thaw = pool_point(pool, at, 300.0, 800.0, rng);
                (Trap::Freeze { thaw, until_ms: now_ms + 30 * MIN }, "Frozen! Reach the glowing thaw point to move again.".to_string())
            }
            "Fog Of War Trap" => (Trap::Fog { until_ms: now_ms + 15 * MIN }, "Fog rolls in: the map is hidden for 15 minutes.".into()),
            "Silence Trap" => (Trap::Silence { until_ms: now_ms + 15 * MIN }, "Silence: notifications muted for 15 minutes.".into()),
            "Leash Trap" => (
                Trap::Leash { center: home, radius_m: 800.0, until_ms: now_ms + 30 * MIN },
                "Leashed! Checks only count within 800 m of home for 30 minutes.".into(),
            ),
            "Detour Trap" => {
                let waypoint = pool_point(pool, at, 300.0, 700.0, rng);
                (Trap::Detour { waypoint, visited: false, until_ms: now_ms + 20 * MIN }, "Detour! Visit the marked waypoint before any check counts.".into())
            }
            "Toll Trap" => (Trap::Toll { need_m: 400.0, moved_m: 0.0, until_ms: now_ms + 20 * MIN }, "Toll! Cover 400 m before any check counts.".into()),
            "Slow Trap" => (Trap::Slow { until_ms: now_ms + 30 * MIN }, "Slow! Dwell quests take twice as long for 30 minutes.".into()),
            "Shuffle Trap" => return Some("Shuffle! Unfinished quests are rerolled.".into()),
            honor if honor.ends_with("Trap") => return Some(format!("{honor}: do it on your honor!")),
            _ => return None,
        };
        // A newer trap of the same kind replaces the old one.
        self.active.retain(|t| std::mem::discriminant(t) != std::mem::discriminant(&trap));
        self.active.push(trap);
        Some(msg)
    }

    /// Advance timers and clear satisfied traps. Returns messages for traps that ended.
    pub fn tick(&mut self, now_ms: i64, pos: Point, moved_m: f64) -> Vec<String> {
        let mut msgs = Vec::new();
        for t in &mut self.active {
            match t {
                Trap::Toll { moved_m: m, .. } => *m += moved_m,
                Trap::Detour { waypoint, visited, .. } if distance_m(pos, *waypoint) <= 50.0 => *visited = true,
                _ => {}
            }
        }
        self.active.retain(|t| {
            let (ended, why) = match t {
                Trap::Freeze { thaw, until_ms } => (distance_m(pos, *thaw) <= THAW_RADIUS_M || now_ms >= *until_ms, "You thawed out."),
                Trap::Fog { until_ms } | Trap::Silence { until_ms } | Trap::Slow { until_ms } | Trap::Leash { until_ms, .. } => {
                    (now_ms >= *until_ms, "A trap wore off.")
                }
                Trap::Detour { visited, until_ms, .. } => (*visited || now_ms >= *until_ms, "Detour done."),
                Trap::Toll { need_m, moved_m, until_ms } => (*moved_m >= *need_m || now_ms >= *until_ms, "Toll paid."),
            };
            if ended {
                msgs.push(why.to_string());
            }
            !ended
        });
        msgs
    }

    /// Why checks cannot count right now (None = free to check).
    #[must_use]
    pub fn blocks_checks(&self, pos: Point) -> Option<String> {
        for t in &self.active {
            match t {
                Trap::Freeze { .. } => return Some("Frozen: reach the thaw point first".into()),
                Trap::Leash { center, radius_m, .. } if distance_m(pos, *center) > *radius_m => return Some("Leashed: stay near home".into()),
                Trap::Detour { visited: false, .. } => return Some("Detour: visit the waypoint first".into()),
                Trap::Toll { need_m, moved_m, .. } if moved_m < need_m => return Some(format!("Toll: {:.0} m to go", need_m - moved_m)),
                _ => {}
            }
        }
        None
    }

    /// With no known position, whether any trap that could block checks is active (a leash cannot be judged without one).
    #[must_use]
    pub fn may_block_without_position(&self) -> bool {
        self.active.iter().any(|t| match t {
            Trap::Freeze { .. } | Trap::Leash { .. } => true,
            Trap::Detour { visited, .. } => !visited,
            Trap::Toll { need_m, moved_m, .. } => moved_m < need_m,
            _ => false,
        })
    }

    /// Whether a fog trap is active.
    #[must_use]
    pub fn fog_active(&self) -> bool {
        self.active.iter().any(|t| matches!(t, Trap::Fog { .. }))
    }

    /// Whether a silence trap is active.
    #[must_use]
    pub fn silenced(&self) -> bool {
        self.active.iter().any(|t| matches!(t, Trap::Silence { .. }))
    }

    /// How much longer dwell quests take: 2 under a slow trap, otherwise 1.
    #[must_use]
    pub fn dwell_multiplier(&self) -> f64 {
        if self.active.iter().any(|t| matches!(t, Trap::Slow { .. })) {
            2.0
        } else {
            1.0
        }
    }

    /// Where to go to thaw an active freeze trap.
    #[must_use]
    pub fn thaw_point(&self) -> Option<Point> {
        self.active.iter().find_map(|t| if let Trap::Freeze { thaw, .. } = t { Some(*thaw) } else { None })
    }

    /// The detour waypoint still to be visited, if any.
    #[must_use]
    pub fn waypoint(&self) -> Option<Point> {
        self.active.iter().find_map(|t| if let Trap::Detour { waypoint, visited: false, .. } = t { Some(*waypoint) } else { None })
    }
}

#[cfg(test)]
#[allow(clippy::assert_is_empty, clippy::float_cmp)] // test code: `is_empty()` reads better in assertions than comparing with a typed empty array; comparing against exact constants the code returns verbatim
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }

    fn rng() -> StdRng {
        StdRng::seed_from_u64(1)
    }

    fn pool() -> Vec<Point> {
        (1..40).map(|i| destination(home(), f64::from(i) * 9.0, 100.0 * f64::from(i))).collect()
    }

    #[test]
    fn freeze_blocks_until_thawed_or_timed_out() {
        let mut t = Traps::default();
        t.trigger("Freeze Trap", 0, Some(home()), home(), &pool(), &mut rng()).unwrap();
        let thaw = t.thaw_point().unwrap();
        assert!((300.0..=800.0).contains(&distance_m(home(), thaw)));
        assert!(t.blocks_checks(home()).is_some());
        assert!(t.tick(1000, home(), 0.0).is_empty() && t.blocks_checks(home()).is_some());
        assert_eq!(t.tick(2000, thaw, 0.0).len(), 1);
        assert!(t.blocks_checks(home()).is_none());
        t.trigger("Freeze Trap", 0, Some(home()), home(), &pool(), &mut rng());
        assert_eq!(t.tick(31 * 60_000, home(), 0.0).len(), 1, "the 30 minute safety valve frees you");
    }

    #[test]
    fn a_thaw_point_stays_on_a_street_even_when_no_street_is_at_the_usual_distance() {
        // #51: a point the player must reach is a street point, never a made-up spot in a backyard.
        let far: Vec<Point> = (0..5).map(|i| destination(home(), 72.0 * f64::from(i), 2000.0 + 10.0 * f64::from(i))).collect();
        let mut t = Traps::default();
        t.trigger("Freeze Trap", 0, Some(home()), home(), &far, &mut rng());
        let thaw = t.thaw_point().unwrap();
        assert!(far.iter().any(|p| distance_m(*p, thaw) < 1e-6), "thaw point {thaw:?} is not a street point");
    }

    #[test]
    fn leash_detour_toll_slow_fog_silence() {
        let mut t = Traps::default();
        t.trigger("Leash Trap", 0, None, home(), &pool(), &mut rng());
        assert!(t.blocks_checks(destination(home(), 0.0, 1500.0)).is_some());
        assert!(t.blocks_checks(destination(home(), 0.0, 300.0)).is_none());
        t.trigger("Detour Trap", 0, Some(home()), home(), &pool(), &mut rng());
        let wp = t.waypoint().unwrap();
        assert!(t.blocks_checks(home()).is_some());
        t.tick(10, wp, 0.0);
        assert!(t.waypoint().is_none());
        t.trigger("Toll Trap", 0, Some(home()), home(), &pool(), &mut rng());
        assert!(t.blocks_checks(home()).unwrap().contains("Toll"));
        t.tick(10, home(), 250.0);
        t.tick(20, home(), 200.0);
        assert!(t.blocks_checks(home()).is_none());
        t.trigger("Slow Trap", 0, None, home(), &pool(), &mut rng());
        t.trigger("Fog Of War Trap", 0, None, home(), &pool(), &mut rng());
        t.trigger("Silence Trap", 0, None, home(), &pool(), &mut rng());
        assert_eq!(t.dwell_multiplier(), 2.0);
        assert!(t.fog_active() && t.silenced());
        t.tick(40 * 60_000, home(), 0.0);
        assert!(t.active.is_empty() && !t.fog_active() && t.dwell_multiplier() == 1.0);
    }

    #[test]
    fn honor_and_shuffle_traps_only_message_and_unknown_items_are_ignored() {
        let mut t = Traps::default();
        assert!(t.trigger("Push Up Trap", 0, None, home(), &[], &mut rng()).unwrap().contains("honor"));
        assert!(t.trigger("Shuffle Trap", 0, None, home(), &[], &mut rng()).is_some());
        assert!(t.trigger("Bike", 0, None, home(), &[], &mut rng()).is_none());
        assert!(t.active.is_empty());
        // an empty pool still produces a reachable thaw point
        t.trigger("Freeze Trap", 0, None, home(), &[], &mut rng());
        assert!(t.thaw_point().is_some());
    }
}
