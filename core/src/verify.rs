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

/// Fixes less accurate than this are ignored (urban canyons, indoor).
pub const MAX_ACCURACY_M: f64 = 35.0;
/// A jump implying more than this is a bad fix (a network or cell fix hundreds of metres off), not movement.
pub const MAX_PLAUSIBLE_KMH: f64 = 100.0;
/// After this many bad-looking fixes in a row the newest one is believed (you really did move, e.g. a long gap or a lift).
pub const MAX_OUTLIER_STREAK: u32 = 3;

/// Speed between two fixes in km/h, ignoring the part of the distance that both fixes' error radii could explain.
/// `None` when the gap is too short (< 1 s) or too long (> 2 min) to say anything.
#[must_use]
pub fn implied_speed_kmh(prev: &Fix, cur: &Fix) -> Option<f64> {
    let dt = i64_to_f64(cur.t_ms - prev.t_ms) / 1000.0;
    if !(1.0..=120.0).contains(&dt) {
        return None;
    }
    let effective = (distance_m(prev.point(), cur.point()) - prev.accuracy_m - cur.accuracy_m).max(0.0);
    Some(effective / dt * 3.6)
}

/// Spacing of the samples a line quest is covered by, in metres.
pub const LINE_SAMPLE_M: f64 = 20.0;
const HOME_RADIUS_M: f64 = 100.0;
const MAX_GAP_MS: i64 = 5 * 60_000;

// Saved with the game (#76), so an in-progress quest survives an app restart.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum State {
    None,
    Dwell { since: Option<i64>, best_ms: i64 },
    Line { dense: Vec<Point>, covered: Vec<bool> },
    Courier { picked_at: Option<i64> },
    RoundTrip { left_home: bool, reached_far: bool },
    Cells { seen: BTreeSet<(i64, i64)> },
    Steps { baseline: Option<i64>, now: i64 },
    Away { accum_ms: i64, last_t: Option<i64> },
}

/// Watches phone signals and decides whether one quest has been completed.
#[derive(Serialize, Deserialize)]
pub struct Tracker {
    target: Target,
    home: Point,
    state: State,
    done: bool,
    progress: f32,
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
        };
        Self { target, home, state, done: false, progress: 0.0 }
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
    pub fn pause(&mut self) {
        match &mut self.state {
            State::Dwell { since, .. } => *since = None,
            State::Away { last_t, .. } => *last_t = None,
            _ => {}
        }
    }

    /// The game was loaded after the app stopped: like [`Self::pause`], and the step counter is re-based on its next reading,
    /// so steps taken while the app was closed are not credited (the steps already counted are kept).
    pub fn resume(&mut self) {
        self.pause();
        if let State::Steps { baseline, .. } = &mut self.state {
            *baseline = None;
        }
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

    #[test]
    fn implied_speed_discounts_the_error_radii() {
        let a = Fix { accuracy_m: 5.0, ..fix(home(), 0) };
        let b = Fix { accuracy_m: 5.0, ..fix(destination(home(), 0.0, 100.0), 10) };
        let v = implied_speed_kmh(&a, &b).unwrap();
        assert!((v - 32.4).abs() < 0.5, "(100 - 10) m in 10 s is 32.4 km/h, got {v}");
        let jitter = Fix { accuracy_m: 5.0, ..fix(destination(home(), 0.0, 8.0), 5) };
        assert_eq!(implied_speed_kmh(&a, &jitter), Some(0.0), "movement inside the error radii is noise");
    }

    #[test]
    fn implied_speed_needs_a_sensible_gap() {
        let a = fix(home(), 0);
        let far = destination(home(), 0.0, 500.0);
        assert_eq!(implied_speed_kmh(&a, &fix(far, 0)), None, "same instant");
        assert_eq!(implied_speed_kmh(&a, &fix(far, 121)), None, "too long ago to compare");
        assert!(implied_speed_kmh(&a, &fix(far, 60)).is_some());
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

    #[test]
    fn resume_after_a_restart_keeps_steps_taken_but_not_steps_while_closed() {
        let mut t = Tracker::new(Target::Steps { n: 1000 }, home());
        t.update(&fix(home(), 0), Some(5000));
        t.update(&fix(home(), 60), Some(5400)); // 400 steps
        let mut back: Tracker = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        back.resume();
        // The counter moved on 2000 while the app was closed; the next reading is the new base, 400 are kept.
        assert_eq!(back.update(&fix(home(), 3600), Some(7400)), Status::Active(0.4));
        assert_eq!(back.update(&fix(home(), 3660), Some(8000)), Status::Done);
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
