//! Fog of war: quests are hidden until you get near them. Also tracks visited map cells (explorer goal).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::assign::{Assignment, Target};
use crate::geo::{distance_m, Point};

pub const BASE_REVEAL_M: f64 = 150.0;
pub const PER_SCOUT_ITEM_M: f64 = 100.0;
pub const CELL_M: f64 = 150.0;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Fog {
    pub discovered: BTreeSet<i64>,
    pub cells: BTreeSet<(i64, i64)>,
}

pub fn reveal_radius(scout_items: u32) -> f64 {
    BASE_REVEAL_M + PER_SCOUT_ITEM_M * f64::from(scout_items)
}

pub fn anchor(t: &Target) -> Option<Point> {
    match t {
        Target::Point { p, .. } | Target::Dwell { p, .. } => Some(*p),
        Target::DwellArea { center, .. } => Some(*center),
        Target::Line { pts, .. } => pts.first().copied(),
        Target::Courier { a, .. } => Some(*a),
        Target::RoundTrip { far, .. } => Some(*far),
        _ => None,
    }
}

pub fn cell_of(p: Point) -> (i64, i64) {
    ((p.lat * 111_195.0 / CELL_M).floor() as i64, (p.lon * 111_195.0 * p.lat.to_radians().cos() / CELL_M).floor() as i64)
}

impl Fog {
    /// Reveal anything within `radius_m` of `pos`; geometry-free quests (steps, away, cells) are always visible.
    /// Returns the location ids newly discovered.
    pub fn update(&mut self, pos: Point, quests: &[Assignment], radius_m: f64) -> Vec<i64> {
        self.cells.insert(cell_of(pos));
        let mut fresh = Vec::new();
        for a in quests {
            if self.discovered.contains(&a.location_id) {
                continue;
            }
            let seen = match anchor(&a.target) {
                None => true,
                Some(p) => distance_m(pos, p) <= radius_m,
            };
            if seen {
                self.discovered.insert(a.location_id);
                fresh.push(a.location_id);
            }
        }
        fresh
    }

    pub fn is_visible(&self, fog_enabled: bool, location_id: i64) -> bool {
        !fog_enabled || self.discovered.contains(&location_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Mode;
    use crate::geo::destination;

    fn quest(id: i64, target: Target) -> Assignment {
        Assignment {
            location_id: id,
            zone: 1,
            mode: Mode::Walk,
            family: "reach".into(),
            kind_id: "k".into(),
            quest_name: "q".into(),
            blurb: "".into(),
            place: "".into(),
            tier: 1,
            effort_min: 5.0,
            target,
            fallback: false,
            boss: false,
        }
    }

    #[test]
    fn quests_appear_only_near_you_and_stay_discovered() {
        let home = Point::new(40.0, -111.0);
        let near = quest(1, Target::Point { p: destination(home, 0.0, 100.0), r: 40.0 });
        let far = quest(2, Target::Point { p: destination(home, 0.0, 2000.0), r: 40.0 });
        let steps = quest(3, Target::Steps { n: 100 });
        let mut fog = Fog::default();
        let found = fog.update(home, &[near.clone(), far.clone(), steps.clone()], reveal_radius(0));
        assert_eq!(found, vec![1, 3], "the far quest stays hidden; sensor quests are always visible");
        assert!(fog.is_visible(true, 1) && !fog.is_visible(true, 2) && fog.is_visible(false, 2));
        let found2 = fog.update(destination(home, 0.0, 1900.0), &[near, far], reveal_radius(0));
        assert_eq!(found2, vec![2]);
        assert!(fog.is_visible(true, 1), "discovered stays discovered");
        assert!(fog.cells.len() >= 2);
    }

    #[test]
    fn scouting_items_widen_the_radius() {
        assert_eq!(reveal_radius(0), 150.0);
        assert_eq!(reveal_radius(3), 450.0);
        let home = Point::new(40.0, -111.0);
        let q = quest(1, Target::Point { p: destination(home, 90.0, 400.0), r: 40.0 });
        assert!(Fog::default().update(home, std::slice::from_ref(&q), reveal_radius(0)).is_empty());
        assert_eq!(Fog::default().update(home, &[q], reveal_radius(3)), vec![1]);
    }
}
