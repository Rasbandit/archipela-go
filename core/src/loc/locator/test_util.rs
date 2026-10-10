//! Helpers shared by the locator's tests.

use std::sync::Arc;

use crate::geo::{destination, distance_m, Point};
use crate::loc::graph::StreetGraph;
use crate::loc::RawFix;
use crate::num::i64_to_f64;

use super::Locator;

pub(super) fn o() -> Point {
    Point::new(40.0, -111.0)
}

pub(super) fn fix(p: Point, t_s: i64, acc: f64) -> RawFix {
    RawFix::at(p.lat, p.lon, t_s * 1000, acc)
}

pub(super) fn stand(l: &mut Locator, from_s: i64, n: i64) -> i64 {
    for i in 0..n {
        l.on_fix(&fix(destination(o(), f64::from(u16::try_from(i * 97 % 360).unwrap()), 2.0), from_s + i, 6.0));
    }
    from_s + n
}

pub(super) fn walk(l: &mut Locator, from_s: i64, n: i64) -> i64 {
    for k in 0..n {
        l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(k)), from_s + k, 5.0));
    }
    from_s + n
}

pub(super) fn grid() -> Option<Arc<StreetGraph>> {
    StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 5, 100.0)).map(Arc::new)
}

/// Walk east along the y = 100 m street from `t0` for `n` seconds; returns the next second.
pub(super) fn walk_the_street(l: &mut Locator, t0: i64, n: i64) -> i64 {
    let street = destination(o(), 0.0, 100.0);
    for t in t0..t0 + n {
        l.on_fix(&fix(destination(street, 90.0, 1.4 * i64_to_f64(t)), t, 4.0));
    }
    t0 + n
}

pub(super) fn points(runs: &[Vec<Point>]) -> usize {
    runs.iter().map(Vec::len).sum()
}

/// Trace points the matcher had not yet decided: they go into the trace when it restarts.
pub(super) fn undecided(l: &Locator) -> usize {
    let mut m = l.matcher.clone();
    let before = m.trace().len();
    m.restart();
    m.trace().len() - before
}

/// A bendy street of 3 to 5 m segments, east from `o()` and turning 20 degrees at every node (left twice, right twice, ...).
pub(super) fn bendy_street() -> Vec<Point> {
    let mut pts = vec![o()];
    let mut heading = 90.0;
    for k in 0..120_i32 {
        heading += if k % 4 < 2 { 20.0 } else { -20.0 };
        pts.push(destination(pts[pts.len() - 1], heading, [3.0, 4.0, 5.0][usize::try_from(k % 3).unwrap()]));
    }
    pts
}

/// The point `s_m` metres along `pts`.
pub(super) fn at_along(pts: &[Point], s_m: f64) -> Point {
    let mut left = s_m;
    for w in pts.windows(2) {
        let d = distance_m(w[0], w[1]);
        if left <= d {
            return destination(w[0], crate::geo::bearing_deg(w[0], w[1]), left);
        }
        left -= d;
    }
    pts[pts.len() - 1]
}

pub(super) fn way_class_foot() -> u8 {
    crate::scan::way_class::FOOT
}

/// The carry confidence at the newest estimate's time.
pub(super) fn conf(l: &Locator) -> f64 {
    l.carry().confidence(l.params(), l.last().map_or(0, |e| e.t_ms))
}
