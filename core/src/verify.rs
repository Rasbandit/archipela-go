//! Proof of quests from the phone's signals (GPS track, step counter). One `Tracker` per active quest.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::assign::Target;
use crate::geo::{densify, distance_m, point_in_polygon, Point};
use crate::num::{count_f32, count_f64, count_u32, floor_i64, i64_to_f64, to_f32, trunc_i64};

/// One position reading from the phone.
#[derive(Debug, Clone, Copy)]
pub struct Fix {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
    /// When the reading was taken, in Unix milliseconds.
    pub t_ms: i64,
    /// Horizontal accuracy in metres.
    pub accuracy_m: f64,
}

impl Fix {
    /// The reading as a map point.
    #[must_use]
    pub fn point(&self) -> Point {
        Point::new(self.lat, self.lon)
    }
}

/// How far a quest has got.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    /// Not started.
    Idle,
    /// 0.0..1.0 progress toward completion.
    Active(f32),
    /// Completed.
    Done,
}

/// Fixes (now estimates) less sure than this are ignored by a tracker; the location filter applies the same limit first.
pub const MAX_ACCURACY_M: f64 = crate::loc::MAX_UNCERTAINTY_M;

/// Spacing of the samples a line quest is covered by, in metres.
pub const LINE_SAMPLE_M: f64 = 20.0;
/// How close to home counts as home: for round trips, and where a forager banks what it carries, in metres.
pub const HOME_RADIUS_M: f64 = 100.0;
const MAX_GAP_MS: i64 = 5 * 60_000;

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

impl Collected {
    /// The one banking rule: everything carried is brought home and counts as banked.
    pub fn bank(&mut self) {
        self.banked += std::mem::take(&mut self.carried);
    }
}

/// Progress of a collect quest: what is banked, plus half of what is carried, out of `need` (at most 1).
#[must_use]
pub fn collect_progress(c: &Collected, need: u32) -> f32 {
    to_f32(((f64::from(c.banked) + 0.5 * f64::from(c.carried)) / f64::from(need.max(1))).min(1.0))
}

// Saved with the game (#76), so an in-progress quest survives an app restart. A line's samples are not saved: they come
// from the target again (see `Tracker::reattach`).
#[derive(Debug, Clone, Serialize, Deserialize)]
enum State {
    None,
    Dwell {
        since: Option<i64>,
        best_ms: i64,
    },
    Line {
        #[serde(skip)]
        dense: Vec<Point>,
        covered: Vec<bool>,
    },
    Courier {
        picked_at: Option<i64>,
    },
    RoundTrip {
        left_home: bool,
        reached_far: bool,
    },
    Cells {
        seen: BTreeSet<(i64, i64)>,
    },
    Steps {
        baseline: Option<i64>,
        now: i64,
    },
    Away {
        accum_ms: i64,
        last_t: Option<i64>,
    },
    Collect(Collected),
}

/// Watches phone signals and decides whether one quest has been completed.
///
/// Saved without its target and home: a loaded tracker is detached until [`Tracker::reattach`] gives it the quest's current
/// target (a dwell's length depends on the traps active now, so it is never stored).
#[derive(Serialize, Deserialize)]
pub struct Tracker {
    #[serde(skip, default = "detached")]
    target: Target,
    #[serde(skip, default = "detached_home")]
    home: Point,
    state: State,
    done: bool,
    progress: f32,
}

// Placeholders of a loaded tracker until `reattach`; a point no fix can reach.
fn detached() -> Target {
    Target::Point { p: detached_home(), r: -1.0 }
}

fn detached_home() -> Point {
    Point::new(0.0, 0.0)
}

fn cell_id(p: Point, cell_m: f64) -> (i64, i64) {
    let lat_m = p.lat * 111_195.0;
    let lon_m = p.lon * 111_195.0 * p.lat.to_radians().cos();
    (floor_i64(lat_m / cell_m), floor_i64(lon_m / cell_m))
}

impl Tracker {
    /// A tracker for `target`, with distances measured from `home`.
    #[must_use]
    pub fn new(target: Target, home: Point) -> Self {
        let state = match &target {
            Target::Point { .. } => State::None,
            Target::Dwell { .. } | Target::DwellArea { .. } => State::Dwell { since: None, best_ms: 0 },
            Target::Line { pts, .. } => {
                let dense = densify(pts, LINE_SAMPLE_M);
                let covered = vec![false; dense.len()];
                State::Line { dense, covered }
            }
            Target::Courier { .. } => State::Courier { picked_at: None },
            Target::RoundTrip { .. } => State::RoundTrip { left_home: false, reached_far: false },
            Target::Cells { .. } => State::Cells { seen: BTreeSet::new() },
            Target::Steps { .. } => State::Steps { baseline: None, now: 0 },
            Target::Away { .. } => State::Away { accum_ms: 0, last_t: None },
            Target::Collect { .. } => State::Collect(Collected::default()),
        };
        Self { target, home, state, done: false, progress: 0.0 }
    }

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

    /// The quest's current status.
    #[must_use]
    pub fn status(&self) -> Status {
        if self.done {
            Status::Done
        } else if self.progress > 0.0 {
            Status::Active(self.progress)
        } else {
            Status::Idle
        }
    }

    fn finish(&mut self) -> Status {
        self.done = true;
        self.progress = 1.0;
        Status::Done
    }

    /// Feed a location fix (and the cumulative step counter, if available).
    #[allow(clippy::too_many_lines)] // one match arm per target kind; splitting would only scatter them
    pub fn update(&mut self, fix: &Fix, steps_total: Option<i64>) -> Status {
        if self.done {
            return Status::Done;
        }
        let steps_only = matches!(self.target, Target::Steps { .. });
        if !steps_only && fix.accuracy_m > MAX_ACCURACY_M {
            return self.status();
        }
        let p = fix.point();
        match (&self.target, &mut self.state) {
            (Target::Point { p: tp, r }, _) => {
                if distance_m(p, *tp) <= *r {
                    return self.finish();
                }
                self.progress = 0.0;
            }
            (Target::Dwell { p: tp, r, minutes }, State::Dwell { since, best_ms }) => {
                let inside = distance_m(p, *tp) <= *r;
                Self::dwell(inside, fix.t_ms, *minutes, since, best_ms, &mut self.progress, &mut self.done);
            }
            (Target::DwellArea { poly, center, r, minutes }, State::Dwell { since, best_ms }) => {
                let inside = if poly.len() >= 3 { point_in_polygon(p, poly) } else { distance_m(p, *center) <= *r };
                Self::dwell(inside, fix.t_ms, *minutes, since, best_ms, &mut self.progress, &mut self.done);
            }
            (Target::Line { corridor_m, coverage, .. }, State::Line { dense, covered }) => {
                for (i, d) in dense.iter().enumerate() {
                    if !covered[i] && distance_m(p, *d) <= *corridor_m {
                        covered[i] = true;
                    }
                }
                let frac = count_f64(covered.iter().filter(|c| **c).count()) / count_f64(covered.len().max(1));
                self.progress = to_f32((frac / coverage).min(1.0));
                self.done = frac >= *coverage;
            }
            (Target::Courier { a, b, r, time_limit_min }, State::Courier { picked_at }) => {
                match *picked_at {
                    None if distance_m(p, *a) <= *r => *picked_at = Some(fix.t_ms),
                    Some(t0) if fix.t_ms - t0 > trunc_i64(*time_limit_min * 60_000.0) => *picked_at = None,
                    Some(_) if distance_m(p, *b) <= *r => self.done = true,
                    _ => {}
                }
                self.progress = if self.done {
                    1.0
                } else if picked_at.is_some() {
                    0.5
                } else {
                    0.0
                };
            }
            (Target::RoundTrip { far, r }, State::RoundTrip { left_home, reached_far }) => {
                // No clock: reach the far point, then get home whenever you like. Coming home without the far point starts over.
                if distance_m(p, self.home) <= HOME_RADIUS_M {
                    if *reached_far {
                        self.done = true;
                    } else {
                        *left_home = false;
                    }
                } else {
                    *left_home = true;
                    if distance_m(p, *far) <= *r {
                        *reached_far = true;
                    }
                }
                self.progress = if self.done {
                    1.0
                } else if *reached_far {
                    0.5
                } else if *left_home {
                    0.1
                } else {
                    0.0
                };
            }
            (Target::Cells { n, cell_m }, State::Cells { seen }) => {
                seen.insert(cell_id(p, *cell_m));
                self.progress = (count_f32(seen.len()) / to_f32(f64::from(*n))).min(1.0);
                self.done = count_u32(seen.len()) >= *n;
            }
            (Target::Steps { n }, State::Steps { baseline, now }) => {
                if let Some(total) = steps_total {
                    let b = *baseline.get_or_insert(total - *now); // `now` > 0 only after `resume`: keep the steps already counted
                    *now = total - b;
                    self.progress = (to_f32(i64_to_f64(*now)) / to_f32(f64::from(*n))).clamp(0.0, 1.0);
                    self.done = *now >= i64::from(*n);
                }
            }
            (Target::Away { minutes }, State::Away { accum_ms, last_t }) => {
                if let Some(prev) = *last_t {
                    let dt = fix.t_ms - prev;
                    if distance_m(p, self.home) > HOME_RADIUS_M && (0..=MAX_GAP_MS).contains(&dt) {
                        *accum_ms += dt;
                    }
                }
                *last_t = Some(fix.t_ms);
                self.progress = to_f32((i64_to_f64(*accum_ms) / (*minutes * 60_000.0)).min(1.0));
                self.done = i64_to_f64(*accum_ms) >= *minutes * 60_000.0;
            }
            (Target::Collect { pts, need, r, .. }, State::Collect(c)) => {
                for (i, q) in pts.iter().enumerate() {
                    let Ok(i) = u16::try_from(i) else { break };
                    if distance_m(p, *q) <= *r && c.picked.insert(i) {
                        c.carried += 1;
                    }
                }
                if distance_m(p, self.home) <= HOME_RADIUS_M {
                    c.bank();
                }
                self.progress = collect_progress(c, *need);
                self.done = c.banked >= *need;
            }
            _ => {}
        }
        if self.done {
            self.progress = 1.0;
            return Status::Done;
        }
        self.status()
    }

    /// When a running dwell completes if nothing changes (inside since its stretch began, no fix said otherwise); `None` when
    /// none is running. The app schedules one wake-up then instead of needing fixes while the player stands still.
    #[must_use]
    pub fn due_ms(&self) -> Option<i64> {
        if self.done {
            return None;
        }
        match (&self.target, &self.state) {
            (Target::Dwell { minutes, .. } | Target::DwellArea { minutes, .. }, State::Dwell { since: Some(s), .. }) => Some(s + trunc_i64(minutes * 60_000.0)),
            _ => None,
        }
    }

    /// A scheduled wake-up: a running dwell is credited up to `now_ms` (the player is still inside: no fix said otherwise).
    pub fn tick(&mut self, now_ms: i64) -> Status {
        if self.done {
            return Status::Done;
        }
        if let (Target::Dwell { minutes, .. } | Target::DwellArea { minutes, .. }, State::Dwell { since, best_ms }) = (&self.target, &mut self.state) {
            if since.is_some() {
                Self::dwell(true, now_ms, *minutes, since, best_ms, &mut self.progress, &mut self.done);
            }
        }
        if self.done {
            return self.finish();
        }
        self.status()
    }

    /// Counting was switched off (car, home): forget only running timers, so what happened before the pause is not
    /// stitched to what happens after it. Progress (a courier pickup, a round trip's far point, coverage, cells) stays;
    /// a dwell keeps its best stretch (`best_ms` is progress) but its current stretch starts over.
    /// The step counter is re-based on its next reading, so steps taken while counting was off are not credited (the steps
    /// already counted are kept). Also used when a saved game is loaded: nothing from while the app was closed counts.
    pub fn pause(&mut self) {
        match &mut self.state {
            State::Dwell { since, .. } => *since = None,
            State::Away { last_t, .. } => *last_t = None,
            State::Steps { baseline, .. } => *baseline = None,
            _ => {}
        }
    }

    /// Give a loaded tracker its quest's current `target` and `home`. Returns false when the saved progress does not fit the
    /// target (another kind of quest, or a line with a different number of samples): the caller drops it and the quest starts over.
    #[must_use]
    pub fn reattach(&mut self, target: Target, home: Point) -> bool {
        let fresh = Self::new(target, home);
        if std::mem::discriminant(&fresh.state) != std::mem::discriminant(&self.state) {
            return false;
        }
        if let (State::Line { dense, covered }, State::Line { dense: fresh_dense, .. }) = (&mut self.state, fresh.state) {
            if covered.len() != fresh_dense.len() {
                return false;
            }
            *dense = fresh_dense;
        }
        self.target = fresh.target;
        self.home = fresh.home;
        true
    }

    fn dwell(inside: bool, t: i64, minutes: f64, since: &mut Option<i64>, best_ms: &mut i64, progress: &mut f32, done: &mut bool) {
        if inside {
            let s = *since.get_or_insert(t);
            *best_ms = (*best_ms).max(t - s);
        } else {
            *since = None;
        }
        let need = trunc_i64(minutes * 60_000.0);
        *progress = to_f32((i64_to_f64(*best_ms) / i64_to_f64(need)).min(1.0));
        *done = *best_ms >= need;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }

    fn fix(p: Point, t_s: i64) -> Fix {
        Fix { lat: p.lat, lon: p.lon, t_ms: t_s * 1000, accuracy_m: 10.0 }
    }

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

    #[test]
    fn banking_moves_everything_carried_into_banked_and_is_idempotent() {
        let mut c = Collected { picked: BTreeSet::from([0, 1, 2]), carried: 2, banked: 1 };
        c.bank();
        assert_eq!((c.carried, c.banked), (0, 3));
        assert_eq!(c.picked, BTreeSet::from([0, 1, 2]), "banking keeps the picked set");
        c.bank();
        assert_eq!((c.carried, c.banked), (0, 3), "banking twice adds nothing");
    }

    #[test]
    fn reach_completes_only_inside_radius_and_ignores_bad_accuracy() {
        let target = destination(home(), 0.0, 500.0);
        let mut t = Tracker::new(Target::Point { p: target, r: 40.0 }, home());
        assert_eq!(t.update(&fix(destination(home(), 0.0, 400.0), 0), None), Status::Idle);
        let mut bad = fix(target, 1);
        bad.accuracy_m = 200.0;
        assert_ne!(t.update(&bad, None), Status::Done, "a 200 m accuracy fix must not count");
        assert_eq!(t.update(&fix(target, 2), None), Status::Done);
    }

    #[test]
    fn dwell_needs_continuous_time_and_resets_when_leaving() {
        let spot = destination(home(), 90.0, 300.0);
        let mut t = Tracker::new(Target::Dwell { p: spot, r: 40.0, minutes: 3.0 }, home());
        t.update(&fix(spot, 0), None);
        assert!(matches!(t.update(&fix(spot, 120), None), Status::Active(p) if p > 0.5));
        t.update(&fix(destination(spot, 0.0, 300.0), 130), None);
        assert!(!matches!(t.update(&fix(spot, 140), None), Status::Done), "leaving resets the timer");
        assert_eq!(t.update(&fix(spot, 140 + 181), None), Status::Done);
    }

    #[test]
    fn area_dwell_uses_the_polygon_or_a_circle_fallback() {
        let a = destination(home(), 0.0, 600.0);
        let poly = vec![a, destination(a, 90.0, 300.0), destination(a, 135.0, 300.0)];
        let inside = Point::new((poly[0].lat + poly[1].lat + poly[2].lat) / 3.0, (poly[0].lon + poly[1].lon + poly[2].lon) / 3.0);
        let mut t = Tracker::new(Target::DwellArea { poly, center: inside, r: 80.0, minutes: 1.0 }, home());
        t.update(&fix(inside, 0), None);
        assert_eq!(t.update(&fix(inside, 61), None), Status::Done);
        let mut c = Tracker::new(Target::DwellArea { poly: vec![], center: inside, r: 80.0, minutes: 1.0 }, home());
        c.update(&fix(destination(inside, 0.0, 50.0), 0), None);
        assert_eq!(c.update(&fix(destination(inside, 0.0, 50.0), 61), None), Status::Done);
    }

    #[test]
    fn line_needs_coverage_of_the_whole_trail_not_just_its_ends() {
        let pts = vec![home(), destination(home(), 0.0, 1000.0)];
        let mut t = Tracker::new(Target::Line { pts: pts.clone(), corridor_m: 25.0, coverage: 0.9 }, home());
        t.update(&fix(pts[0], 0), None);
        assert!(!matches!(t.update(&fix(pts[1], 100), None), Status::Done), "teleporting to the end is not walking the trail");
        let mut done = false;
        for i in 0..=50 {
            done = t.update(&fix(destination(home(), 0.0, 20.0 * f64::from(i)), 200 + i64::from(i)), None) == Status::Done;
        }
        assert!(done);
    }

    /// A tracker saved and loaded back, attached to `target` again.
    fn reloaded(t: &Tracker, target: Target) -> Option<Tracker> {
        let mut back: Tracker = serde_json::from_str(&serde_json::to_string(t).unwrap()).unwrap();
        back.reattach(target, home()).then_some(back)
    }

    #[test]
    fn steps_while_paused_or_closed_are_not_credited_but_steps_taken_are_kept() {
        let mut t = Tracker::new(Target::Steps { n: 1000 }, home());
        t.update(&fix(home(), 0), Some(5000));
        t.update(&fix(home(), 60), Some(5400)); // 400 steps
        t.pause(); // home Wi-Fi
                   // 1000 steps around the house: the next reading is the new base, the 400 are kept.
        assert_eq!(t.update(&fix(home(), 600), Some(6400)), Status::Active(0.4));
        let mut back = reloaded(&t, Target::Steps { n: 1000 }).unwrap();
        back.pause(); // as `Game::load` does
                      // The counter moved on 2000 while the app was closed.
        assert_eq!(back.update(&fix(home(), 3600), Some(8400)), Status::Active(0.4));
        assert_eq!(back.update(&fix(home(), 3660), Some(9000)), Status::Done);
    }

    #[test]
    fn a_reloaded_tracker_takes_the_target_it_is_given() {
        let mut t = Tracker::new(Target::Dwell { p: home(), r: 50.0, minutes: 20.0 }, home()); // made longer by a trap
        t.update(&fix(home(), 0), None);
        t.update(&fix(home(), 300), None); // 5 minutes
        let mut back = reloaded(&t, Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }).unwrap(); // the trap is over
        back.pause(); // as `Game::load` does
        assert_eq!(back.update(&fix(home(), 400), None), Status::Active(0.5));
        back.update(&fix(home(), 500), None);
        assert_eq!(back.update(&fix(home(), 1100), None), Status::Done, "ten minutes, not twenty");
    }

    #[test]
    fn a_reloaded_line_rebuilds_its_samples_and_keeps_its_coverage() {
        let pts = vec![home(), destination(home(), 0.0, 1000.0)];
        let line = || Target::Line { pts: pts.clone(), corridor_m: 25.0, coverage: 0.9 };
        let mut t = Tracker::new(line(), home());
        for i in 0..=25 {
            t.update(&fix(destination(home(), 0.0, 20.0 * f64::from(i)), 100 + i64::from(i)), None);
        }
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("dense"), "samples are not saved: {json}");
        let mut back = reloaded(&t, line()).unwrap();
        let mut done = false;
        for i in 25..=50 {
            done = back.update(&fix(destination(home(), 0.0, 20.0 * f64::from(i)), 200 + i64::from(i)), None) == Status::Done;
        }
        assert!(done, "the first half stays covered");
        let other = Target::Line { pts: vec![home(), destination(home(), 0.0, 3000.0)], corridor_m: 25.0, coverage: 0.9 };
        assert!(reloaded(&t, other).is_none(), "a line of another length starts over");
    }

    #[test]
    fn a_reloaded_tracker_of_another_kind_is_dropped() {
        let t = Tracker::new(Target::Steps { n: 1000 }, home());
        assert!(reloaded(&t, Target::Away { minutes: 10.0 }).is_none());
    }

    #[test]
    fn pause_resets_only_timing_state() {
        let mut dwell = Tracker::new(Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }, home());
        dwell.update(&fix(home(), 0), None);
        dwell.update(&fix(home(), 300), None);
        dwell.pause();
        assert_ne!(dwell.update(&fix(home(), 700), None), Status::Done, "the stretch starts again after the pause");
        assert_eq!(dwell.update(&fix(home(), 1300), None), Status::Done);

        let far = destination(home(), 0.0, 1000.0);
        let mut quiet = Tracker::new(Target::Dwell { p: home(), r: 40.0, minutes: 10.0 }, home());
        quiet.update(&fix(home(), 0), None);
        assert_eq!(quiet.due_ms(), Some(600_000), "inside since 0: done at minute 10 if nothing changes");
        assert_ne!(quiet.tick(300_000), Status::Done);
        assert_eq!(quiet.tick(600_000), Status::Done, "standing still needs no more fixes to finish");
        let mut left = Tracker::new(Target::Dwell { p: home(), r: 40.0, minutes: 10.0 }, home());
        left.update(&fix(home(), 0), None);
        left.update(&fix(far, 60), None);
        assert_eq!(left.due_ms(), None, "outside: nothing will fall due");
        assert_ne!(left.tick(900_000), Status::Done);
        let mut away = Tracker::new(Target::Away { minutes: 10.0 }, home());
        away.update(&fix(far, 0), None);
        away.update(&fix(far, 60), None); // one minute counted
        away.pause();
        away.update(&fix(far, 100), None); // no interval spans the pause
        assert_eq!(away.update(&fix(far, 160), None), Status::Active(0.2), "only 1 + 1 minutes");

        let (a, b) = (destination(home(), 0.0, 400.0), destination(home(), 90.0, 800.0));
        let mut courier = Tracker::new(Target::Courier { a, b, r: 40.0, time_limit_min: 10.0 }, home());
        courier.update(&fix(a, 0), None);
        courier.pause();
        assert_eq!(courier.update(&fix(b, 100), None), Status::Done);

        let mut cells = Tracker::new(Target::Cells { n: 2, cell_m: 100.0 }, home());
        cells.update(&fix(home(), 0), None);
        cells.pause();
        assert_eq!(cells.update(&fix(destination(home(), 0.0, 400.0), 100), None), Status::Done, "cells seen before the pause still count");
    }

    #[test]
    fn courier_needs_a_then_b_within_the_time_limit() {
        let (a, b) = (destination(home(), 0.0, 400.0), destination(home(), 90.0, 800.0));
        let mut t = Tracker::new(Target::Courier { a, b, r: 40.0, time_limit_min: 10.0 }, home());
        assert_ne!(t.update(&fix(b, 0), None), Status::Done, "B before A does nothing");
        t.update(&fix(a, 10), None);
        assert_eq!(t.update(&fix(b, 300), None), Status::Done);
        let mut slow = Tracker::new(Target::Courier { a, b, r: 40.0, time_limit_min: 10.0 }, home());
        slow.update(&fix(a, 0), None);
        assert_ne!(slow.update(&fix(b, 700), None), Status::Done, "took longer than the limit");
    }

    #[test]
    fn round_trip_is_done_on_getting_home_after_the_far_point_however_long_it_takes() {
        let far = destination(home(), 0.0, 1500.0);
        let mut t = Tracker::new(Target::RoundTrip { far, r: 50.0 }, home());
        t.update(&fix(home(), 0), None);
        assert_ne!(t.update(&fix(home(), 60), None), Status::Done, "just being home is not a round trip");
        t.update(&fix(far, 600), None);
        // six hours later: no time limit, the player is never rushed home
        assert_eq!(t.update(&fix(destination(home(), 0.0, 40.0), 600 + 6 * 3600), None), Status::Done);
    }

    #[test]
    fn round_trip_returning_without_the_far_point_starts_over_and_progress_shows_the_stages() {
        let far = destination(home(), 0.0, 1500.0);
        let mut t = Tracker::new(Target::RoundTrip { far, r: 50.0 }, home());
        assert_eq!(t.update(&fix(destination(home(), 0.0, 300.0), 0), None), Status::Active(0.1));
        assert_ne!(t.update(&fix(home(), 100), None), Status::Done, "back home without the far point");
        assert_eq!(t.status(), Status::Idle);
        t.update(&fix(destination(home(), 0.0, 300.0), 200), None);
        assert_eq!(t.update(&fix(far, 400), None), Status::Active(0.5));
        assert_eq!(t.update(&fix(home(), 900), None), Status::Done);
    }

    #[test]
    fn cells_steps_and_away_accumulate() {
        let mut c = Tracker::new(Target::Cells { n: 4, cell_m: 150.0 }, home());
        let mut last = Status::Idle;
        for i in 0..6 {
            last = c.update(&fix(destination(home(), 0.0, 160.0 * f64::from(i)), i64::from(i)), None);
        }
        assert_eq!(last, Status::Done);

        let mut s = Tracker::new(Target::Steps { n: 1000 }, home());
        s.update(&fix(home(), 0), Some(50_000));
        assert!(matches!(s.update(&fix(home(), 60), Some(50_400)), Status::Active(p) if (p - 0.4).abs() < 0.01));
        assert_eq!(s.update(&fix(home(), 120), Some(51_100)), Status::Done);

        let away = destination(home(), 0.0, 2000.0);
        let mut a = Tracker::new(Target::Away { minutes: 10.0 }, home());
        for i in 0..=11 {
            a.update(&fix(away, i * 60), None);
        }
        assert_eq!(a.status(), Status::Done);
        let mut gap = Tracker::new(Target::Away { minutes: 10.0 }, home());
        gap.update(&fix(away, 0), None);
        gap.update(&fix(away, 3600), None);
        assert_ne!(gap.status(), Status::Done, "a one-hour gap in fixes must not count as time away");
    }
}
