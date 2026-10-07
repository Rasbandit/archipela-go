//! Proof of quests from the phone's signals (GPS track, step counter). One `Tracker` per active quest.

use std::collections::BTreeSet;

use crate::assign::Target;
use crate::geo::{densify, distance_m, point_in_polygon, Point};

#[derive(Debug, Clone, Copy)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    pub t_ms: i64,
    pub accuracy_m: f64,
}

impl Fix {
    pub fn point(&self) -> Point {
        Point::new(self.lat, self.lon)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Idle,
    /// 0.0..1.0 progress toward completion.
    Active(f32),
    Done,
}

/// Fixes less accurate than this are ignored (urban canyons, indoor).
pub const MAX_ACCURACY_M: f64 = 75.0;
const HOME_RADIUS_M: f64 = 100.0;
const MAX_GAP_MS: i64 = 5 * 60_000;

#[derive(Debug, Clone)]
enum State {
    None,
    Dwell { since: Option<i64>, best_ms: i64 },
    Line { dense: Vec<Point>, covered: Vec<bool> },
    Courier { picked_at: Option<i64> },
    RoundTrip { start: Option<i64>, reached_far: bool },
    Cells { seen: BTreeSet<(i64, i64)> },
    Steps { baseline: Option<i64>, now: i64 },
    Away { accum_ms: i64, last_t: Option<i64> },
}

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
    ((lat_m / cell_m).floor() as i64, (lon_m / cell_m).floor() as i64)
}

impl Tracker {
    pub fn new(target: Target, home: Point) -> Tracker {
        let state = match &target {
            Target::Point { .. } => State::None,
            Target::Dwell { .. } | Target::DwellArea { .. } => State::Dwell { since: None, best_ms: 0 },
            Target::Line { pts, .. } => {
                let dense = densify(pts, 20.0);
                let covered = vec![false; dense.len()];
                State::Line { dense, covered }
            }
            Target::Courier { .. } => State::Courier { picked_at: None },
            Target::RoundTrip { .. } => State::RoundTrip { start: None, reached_far: false },
            Target::Cells { .. } => State::Cells { seen: BTreeSet::new() },
            Target::Steps { .. } => State::Steps { baseline: None, now: 0 },
            Target::Away { .. } => State::Away { accum_ms: 0, last_t: None },
        };
        Tracker { target, home, state, done: false, progress: 0.0 }
    }

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
                let frac = covered.iter().filter(|c| **c).count() as f64 / covered.len().max(1) as f64;
                self.progress = (frac / coverage).min(1.0) as f32;
                self.done = frac >= *coverage;
            }
            (Target::Courier { a, b, r, time_limit_min }, State::Courier { picked_at }) => {
                match *picked_at {
                    None if distance_m(p, *a) <= *r => *picked_at = Some(fix.t_ms),
                    Some(t0) if fix.t_ms - t0 > (*time_limit_min * 60_000.0) as i64 => *picked_at = None,
                    Some(_) if distance_m(p, *b) <= *r => self.done = true,
                    _ => {}
                }
                self.progress = if self.done { 1.0 } else if picked_at.is_some() { 0.5 } else { 0.0 };
            }
            (Target::RoundTrip { far, r, time_limit_min }, State::RoundTrip { start, reached_far }) => {
                // The clock starts when you leave home and stops when you are back (or the limit runs out and you start over).
                let limit = (*time_limit_min * 60_000.0) as i64;
                if distance_m(p, self.home) <= HOME_RADIUS_M {
                    if *reached_far && start.is_some_and(|t0| fix.t_ms - t0 <= limit) {
                        self.done = true;
                    } else {
                        *start = None;
                        *reached_far = false;
                    }
                } else {
                    if start.is_some_and(|t0| fix.t_ms - t0 > limit) {
                        *start = None;
                        *reached_far = false;
                    }
                    start.get_or_insert(fix.t_ms);
                    if distance_m(p, *far) <= *r {
                        *reached_far = true;
                    }
                }
                self.progress = if self.done { 1.0 } else if *reached_far { 0.5 } else if start.is_some() { 0.1 } else { 0.0 };
            }
            (Target::Cells { n, cell_m }, State::Cells { seen }) => {
                seen.insert(cell_id(p, *cell_m));
                self.progress = (seen.len() as f32 / *n as f32).min(1.0);
                self.done = seen.len() as u32 >= *n;
            }
            (Target::Steps { n }, State::Steps { baseline, now }) => {
                if let Some(total) = steps_total {
                    let b = *baseline.get_or_insert(total);
                    *now = total - b;
                    self.progress = (*now as f32 / *n as f32).clamp(0.0, 1.0);
                    self.done = *now >= i64::from(*n);
                }
            }
            (Target::Away { min_distance_m, minutes }, State::Away { accum_ms, last_t }) => {
                if let Some(prev) = *last_t {
                    let dt = fix.t_ms - prev;
                    if distance_m(p, self.home) >= *min_distance_m && (0..=MAX_GAP_MS).contains(&dt) {
                        *accum_ms += dt;
                    }
                }
                *last_t = Some(fix.t_ms);
                self.progress = (*accum_ms as f64 / (*minutes * 60_000.0)).min(1.0) as f32;
                self.done = *accum_ms as f64 >= *minutes * 60_000.0;
            }
            _ => {}
        }
        if self.done {
            self.progress = 1.0;
            return Status::Done;
        }
        self.status()
    }

    fn dwell(inside: bool, t: i64, minutes: f64, since: &mut Option<i64>, best_ms: &mut i64, progress: &mut f32, done: &mut bool) {
        if inside {
            let s = *since.get_or_insert(t);
            *best_ms = (*best_ms).max(t - s);
        } else {
            *since = None;
        }
        let need = (minutes * 60_000.0) as i64;
        *progress = (*best_ms as f64 / need as f64).min(1.0) as f32;
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
    fn round_trip_needs_far_point_then_home_in_time() {
        let far = destination(home(), 0.0, 1500.0);
        let mut t = Tracker::new(Target::RoundTrip { far, r: 50.0, time_limit_min: 30.0 }, home());
        t.update(&fix(home(), 0), None);
        assert_ne!(t.update(&fix(home(), 60), None), Status::Done, "just being home is not a round trip");
        t.update(&fix(far, 600), None);
        assert_eq!(t.update(&fix(destination(home(), 0.0, 40.0), 1200), None), Status::Done);
    }

    #[test]
    fn round_trip_recovers_after_a_timed_out_attempt_and_times_from_leaving_home() {
        let far = destination(home(), 0.0, 1500.0);
        let mut t = Tracker::new(Target::RoundTrip { far, r: 50.0, time_limit_min: 10.0 }, home());
        // slow first attempt: leaves, reaches the far point, but is far too late getting back
        t.update(&fix(destination(home(), 0.0, 300.0), 0), None);
        t.update(&fix(far, 120), None);
        assert_ne!(t.update(&fix(destination(home(), 0.0, 40.0), 120 + 700), None), Status::Done, "11+ minutes is over the 10 minute limit");
        // second attempt starts fresh and succeeds; the far fix after a long idle must still count
        t.update(&fix(far, 5000), None);
        assert_eq!(t.update(&fix(home(), 5000 + 300), None), Status::Done);
        // hanging around at home never starts the clock
        let mut idle = Tracker::new(Target::RoundTrip { far, r: 50.0, time_limit_min: 10.0 }, home());
        for i in 0..10 {
            idle.update(&fix(home(), i * 600), None);
        }
        assert_eq!(idle.status(), Status::Idle);
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
        let mut a = Tracker::new(Target::Away { min_distance_m: 1000.0, minutes: 10.0 }, home());
        for i in 0..=11 {
            a.update(&fix(away, i * 60), None);
        }
        assert_eq!(a.status(), Status::Done);
        let mut gap = Tracker::new(Target::Away { min_distance_m: 1000.0, minutes: 10.0 }, home());
        gap.update(&fix(away, 0), None);
        gap.update(&fix(away, 3600), None);
        assert_ne!(gap.status(), Status::Done, "a one-hour gap in fixes must not count as time away");
    }
}
