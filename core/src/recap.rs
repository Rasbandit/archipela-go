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

/// One quest finished on the walk, as the recap lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecapQuest {
    /// Display name of the quest.
    pub name: String,
    /// Name of the place the quest used.
    pub place: String,
    /// What the quest asked for, in the player's units.
    pub goal: String,
    /// When it was finished, Unix ms.
    pub t_ms: i64,
    /// Where the player was when it was finished, if a fix was known.
    pub at: Option<Point>,
    /// The reward, when known: solo games only (Archipelago rewards belong to other players).
    pub reward: Option<String>,
}

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
    /// The quests completed that stand alone (chain milestones are not listed), in the order they were finished.
    #[serde(default)]
    pub quests: Vec<RecapQuest>,
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
        self.route = segments.iter().filter(|s| s.len() >= 2).map(|s| thin(s)).collect();
    }
}

/// Douglas-Peucker via `geo`, keeping both ends.
fn thin(line: &[Point]) -> Vec<Point> {
    use ::geo::{Coord, LineString, Simplify};
    let ls: LineString<f64> = line.iter().map(|p| Coord { x: p.lon, y: p.lat }).collect();
    ls.simplify(THIN_DEG).coords().map(|c| Point::new(c.y, c.x)).collect()
}

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
        assert_eq!((r.distance_m, r.route.len()), (0.0, 0), "a lone point has no length and draws no line");
        r.set_route(&[vec![Point::new(40.0, -111.0)], vec![Point::new(40.0, -111.0), Point::new(40.001, -111.0)]]);
        assert_eq!(r.route.len(), 1, "only the stretch with a line is kept");
    }
}
