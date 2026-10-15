//! The walk recap ("Welcome home"): what one outing away from home came to, and whether it is worth showing.
//! Built by `Game::arrive_at_camp` from the outing's snapshots; the engine adds the route from the journal.

use serde::{Deserialize, Serialize};

use crate::chain::ChainUnit;
use crate::geo::{polyline_len_m, simplify, Point};
use crate::units::{distance, UnitSystem};

/// An outing shorter than this is never shown (a trip to the mailbox, Wi-Fi flicker at the door).
pub const MIN_AWAY_MS: i64 = 5 * 60_000;
/// Steps that make an outing worth showing on their own.
pub const MIN_STEPS: i64 = 500;
/// Route thinning tolerance in metres.
const THIN_M: f64 = 5.0;

/// One quest finished on the walk, as the recap lists it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)] // a field added later must not make old walks unopenable
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

/// One progressive chain that grew on the walk: its bar before and after, and where its checks fall.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)] // a field added later must not make old walks unopenable
pub struct RecapChain {
    /// `"{zone}:{kind_id}"`.
    pub id: String,
    /// Display name of the chain.
    pub name: String,
    /// What the counter counts.
    pub unit: ChainUnit,
    /// Zone number the chain belongs to.
    pub zone: u32,
    /// The counter on leaving home (0 when the chain did not exist yet).
    pub before: f64,
    /// The counter on arriving home.
    pub after: f64,
    /// The counter value of each check on the chain, in unlock order.
    pub marks: Vec<f64>,
}

/// One forager (collect quest) that moved on the walk: its banked count before and after.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)] // a field added later must not make old walks unopenable
pub struct RecapForager {
    /// Display name of the quest.
    pub name: String,
    /// What is collected ("acorns").
    pub theme: String,
    /// Items needed to finish.
    pub need: u32,
    /// Items banked on leaving home.
    pub banked_before: u32,
    /// Items banked on arriving home.
    pub banked_after: u32,
    /// Items picked up on the walk.
    pub picked: u32,
    /// Items still carried (not banked) on arriving.
    pub carried: u32,
}

/// What one outing came to.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)] // a field added later must not make old walks unopenable
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
    pub quests: Vec<RecapQuest>,
    /// The chains that grew on the walk, by unit (steps, minutes, cells) then zone.
    pub chains: Vec<RecapChain>,
    /// Steps that counted toward chains (slowed by the backpack), rounded.
    pub counted_steps: i64,
    /// The foragers that banked or picked something on the walk, in quest order.
    pub foragers: Vec<RecapForager>,
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

/// The one-line summary of a walk, e.g. "3.1 km · 42 min · 2 quests" (quests omitted when none).
#[must_use]
pub fn walk_summary(r: &Recap, units: UnitSystem) -> String {
    let minutes = ((r.end_ms - r.start_ms).max(0) + 30_000) / 60_000;
    let time = if minutes >= 60 && minutes % 60 == 0 {
        format!("{} h", minutes / 60)
    } else if minutes >= 60 {
        format!("{} h {} min", minutes / 60, minutes % 60)
    } else {
        format!("{minutes} min")
    };
    let mut parts = vec![distance(r.distance_m, units), time];
    match r.quests_done.len() {
        0 => {}
        1 => parts.push("1 quest".into()),
        n => parts.push(format!("{n} quests")),
    }
    parts.join(" · ")
}

impl Recap {
    /// Long enough, and something happened.
    #[must_use]
    pub fn worth_showing(&self) -> bool {
        let happened = !self.quests_done.is_empty()
            || !self.items.is_empty()
            || !self.zones_unlocked.is_empty()
            || self.banked > 0
            || self.chains.iter().any(|c| c.marks.iter().any(|&m| c.before < m && m <= c.after))
            || self.steps >= MIN_STEPS;
        self.end_ms - self.start_ms >= MIN_AWAY_MS && happened
    }

    /// Set the route and distance from the recorded trace (one line per stretch; the journal splits at gaps).
    pub fn set_route(&mut self, segments: &[Vec<Point>]) {
        self.distance_m = segments.iter().map(|s| polyline_len_m(s)).sum();
        self.route = segments.iter().filter(|s| s.len() >= 2).map(|s| simplify(s, THIN_M)).collect();
    }
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

    fn bar(before: f64, after: f64) -> Recap {
        Recap {
            start_ms: 0,
            end_ms: 10 * 60_000,
            chains: vec![RecapChain { id: "1:x".into(), name: "x".into(), unit: ChainUnit::Cells, zone: 1, before, after, marks: vec![5.0] }],
            ..Recap::default()
        }
    }

    #[test]
    fn a_bar_that_crossed_a_mark_is_worth_showing() {
        assert!(bar(4.0, 6.0).worth_showing());
        assert!(bar(4.0, 5.0).worth_showing(), "landing exactly on a mark counts");
    }

    #[test]
    fn a_bar_that_grew_without_crossing_a_mark_is_not_worth_showing() {
        assert!(!bar(1.0, 2.0).worth_showing());
        assert!(!bar(5.0, 7.0).worth_showing(), "the mark was already behind it");
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

    #[test]
    fn the_summary_reads_distance_time_and_quests() {
        let mut r = Recap { start_ms: 0, end_ms: 42 * 60_000, distance_m: 3100.0, ..Recap::default() };
        assert_eq!(walk_summary(&r, UnitSystem::Metric), "3.1 km · 42 min");
        r.quests_done = vec!["a".into()];
        assert_eq!(walk_summary(&r, UnitSystem::Metric), "3.1 km · 42 min · 1 quest");
        r.quests_done.push("b".into());
        assert_eq!(walk_summary(&r, UnitSystem::Metric), "3.1 km · 42 min · 2 quests");
        r.end_ms = 135 * 60_000;
        assert_eq!(walk_summary(&r, UnitSystem::Metric), "3.1 km · 2 h 15 min · 2 quests");
        r.end_ms = 60 * 60_000;
        assert_eq!(walk_summary(&r, UnitSystem::Metric), "3.1 km · 1 h · 2 quests");
        assert!(walk_summary(&r, UnitSystem::Imperial).starts_with("1.9 mi"));
    }
}
