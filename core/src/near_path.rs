//! The near-a-path rule: every point a player must reach lies within [`NEAR_PATH_M`] of a walkable road or path from the zone's scan,
//! so nobody has to go off-road or into a backyard.
//!
//! [`PathIndex`] buckets the scan's street points into a grid of [`NEAR_PATH_M`] cells, so "is this near a path" and "nearest path point"
//! look at a handful of cells instead of every street point of the zone.

use std::collections::HashMap;

use crate::geo::{distance_m, distance_to_segment_m, point_in_polygon, Point};
use crate::num::{ceil_usize, round_i64};

/// How far a quest point may be from the nearest scanned street or path point, in metres.
pub const NEAR_PATH_M: f64 = 30.0;

const M_PER_DEG_LAT: f64 = 111_195.0;
/// Spacing of the samples taken along a line when looking for its first point near a path, in metres.
const LINE_STEP_M: f64 = 5.0;

/// A grid index over the walkable street and path points of a zone.
#[derive(Debug, Default)]
pub struct PathIndex {
    cells: HashMap<(i64, i64), Vec<Point>>,
    dlat: f64,
    dlon: f64,
}

impl PathIndex {
    /// Index `points` (a zone's street and path points).
    #[must_use]
    pub fn new(points: &[Point]) -> Self {
        let lat = points.first().map_or(0.0, |p| p.lat);
        let dlat = NEAR_PATH_M / M_PER_DEG_LAT;
        let dlon = dlat / lat.to_radians().cos().max(0.01);
        let mut idx = Self { cells: HashMap::new(), dlat, dlon };
        for p in points {
            idx.cells.entry(idx.cell(*p)).or_default().push(*p);
        }
        idx
    }

    /// Whether the index holds no points.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    fn cell(&self, p: Point) -> (i64, i64) {
        (round_i64((p.lat / self.dlat).floor()), round_i64((p.lon / self.dlon).floor()))
    }

    /// Points in the cells covering the box `sw`..`ne` grown by `margin_m`, in a fixed order.
    fn around(&self, sw: Point, ne: Point, margin_m: f64) -> impl Iterator<Item = Point> + '_ {
        // One spare cell each way absorbs the change of a degree of longitude's length across a city-sized zone.
        let pad = i64::try_from(ceil_usize(margin_m / NEAR_PATH_M)).unwrap_or(i64::MAX / 4) + 1;
        let ((la, lo), (lb, hb)) = (self.cell(sw), self.cell(ne));
        let empty = self.is_empty();
        (la - pad..=lb + pad)
            .filter(move |_| !empty)
            .flat_map(move |i| (lo - pad..=hb + pad).map(move |j| (i, j)))
            .filter_map(|k| self.cells.get(&k))
            .flatten()
            .copied()
    }

    /// The path point nearest to `p` within `max_m`, with its distance.
    #[must_use]
    pub fn nearest(&self, p: Point, max_m: f64) -> Option<(Point, f64)> {
        self.around(p, p, max_m).map(|q| (q, distance_m(p, q))).filter(|(_, d)| *d <= max_m).min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Whether `p` is within [`NEAR_PATH_M`] of a path point.
    #[must_use]
    pub fn near_path(&self, p: Point) -> bool {
        self.nearest(p, NEAR_PATH_M).is_some()
    }

    /// A path point inside the area `poly`, or failing that within [`NEAR_PATH_M`] of its edge, nearest to `prefer`.
    #[must_use]
    pub fn snap_into_area(&self, poly: &[Point], prefer: Point) -> Option<Point> {
        if poly.len() < 3 {
            return None;
        }
        let sw = poly.iter().fold(poly[0], |a, p| Point::new(a.lat.min(p.lat), a.lon.min(p.lon)));
        let ne = poly.iter().fold(poly[0], |a, p| Point::new(a.lat.max(p.lat), a.lon.max(p.lon)));
        let edge_m = |q: Point| {
            poly.windows(2).chain(std::iter::once(&[poly[poly.len() - 1], poly[0]][..])).map(|w| distance_to_segment_m(q, w[0], w[1])).fold(f64::MAX, f64::min)
        };
        // Inside beats next-to-the-edge; then the nearest to `prefer` wins.
        self.around(sw, ne, NEAR_PATH_M)
            .filter_map(|q| if point_in_polygon(q, poly) { Some((0u8, q)) } else { (edge_m(q) <= NEAR_PATH_M).then_some((1u8, q)) })
            .min_by(|a, b| a.0.cmp(&b.0).then(distance_m(prefer, a.1).total_cmp(&distance_m(prefer, b.1))))
            .map(|(_, q)| q)
    }

    /// `pts` changed so it starts at its first place within [`NEAR_PATH_M`] of a path: a closed loop is turned to start there, an open line
    /// is cut there (tried from either end; the longer result wins). `None` when no part of it is near a path.
    #[must_use]
    pub fn start_near_path(&self, pts: &[Point]) -> Option<Vec<Point>> {
        if pts.len() < 2 {
            return pts.first().filter(|p| self.near_path(**p)).map(|p| vec![*p]);
        }
        if pts[0] == pts[pts.len() - 1] && pts.len() >= 4 {
            let (i, q) = self.first_near(pts)?;
            let ring = &pts[..pts.len() - 1];
            let mut out = vec![q];
            out.extend_from_slice(&ring[i + 1..]);
            out.extend_from_slice(&ring[..=i]);
            out.push(q);
            return Some(out);
        }
        let cut = |line: &[Point]| {
            self.first_near(line).map(|(i, q)| {
                let mut out = vec![q];
                out.extend(line[i + 1..].iter().copied().filter(|p| *p != q));
                out
            })
        };
        let rev: Vec<Point> = pts.iter().rev().copied().collect();
        let (a, b) = (cut(pts), cut(&rev));
        match (a, b) {
            (Some(a), Some(b)) => Some(if crate::geo::polyline_len_m(&b) > crate::geo::polyline_len_m(&a) { b } else { a }),
            (a, b) => a.or(b),
        }
    }

    /// The first place along `pts` (segment index and point) within [`NEAR_PATH_M`] of a path.
    fn first_near(&self, pts: &[Point]) -> Option<(usize, Point)> {
        pts.windows(2).enumerate().find_map(|(i, w)| {
            let n = ceil_usize(distance_m(w[0], w[1]) / LINE_STEP_M).max(1);
            (0..n).map(|s| lerp(w[0], w[1], crate::num::count_f64(s) / crate::num::count_f64(n))).find(|q| self.near_path(*q)).map(|q| (i, q))
        })
    }
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.lat + (b.lat - a.lat) * t, a.lon + (b.lon - a.lon) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn at(north_m: f64, east_m: f64) -> Point {
        destination(destination(Point::new(40.0, -111.0), 0.0, north_m), 90.0, east_m)
    }

    #[test]
    fn nearest_matches_a_brute_force_search_across_cell_borders() {
        let pts: Vec<Point> = (0..400).map(|i| at(f64::from(i % 20) * 47.0, f64::from(i / 20) * 53.0)).collect();
        let idx = PathIndex::new(&pts);
        for i in 0..200 {
            let p = at(f64::from(i) * 4.7 - 30.0, f64::from(i * 7 % 1000) - 20.0);
            let brute = pts.iter().map(|q| distance_m(p, *q)).fold(f64::MAX, f64::min);
            match idx.nearest(p, 100.0) {
                Some((_, d)) => assert!((d - brute).abs() < 1e-9, "{d} vs {brute}"),
                None => assert!(brute > 100.0),
            }
            assert_eq!(idx.near_path(p), brute <= NEAR_PATH_M);
        }
    }

    #[test]
    fn an_empty_index_finds_nothing() {
        let idx = PathIndex::new(&[]);
        assert!(idx.is_empty() && !idx.near_path(at(0.0, 0.0)) && idx.nearest(at(0.0, 0.0), 1e6).is_none());
        assert!(idx.snap_into_area(&[at(0.0, 0.0), at(0.0, 100.0), at(100.0, 100.0)], at(50.0, 50.0)).is_none());
        assert!(idx.start_near_path(&[at(0.0, 0.0), at(0.0, 100.0)]).is_none());
    }

    #[test]
    fn an_area_snaps_inside_first_then_to_its_edge_and_is_rejected_when_nothing_is_near() {
        let square = vec![at(0.0, 0.0), at(0.0, 400.0), at(400.0, 400.0), at(400.0, 0.0), at(0.0, 0.0)];
        let mid = at(200.0, 200.0);
        let edge_only = PathIndex::new(&[at(-20.0, 200.0), at(-200.0, 200.0)]);
        assert_eq!(edge_only.snap_into_area(&square, mid), Some(at(-20.0, 200.0)));
        let with_inside = PathIndex::new(&[at(-5.0, 200.0), at(380.0, 380.0)]);
        assert_eq!(with_inside.snap_into_area(&square, mid), Some(at(380.0, 380.0)), "a point inside beats a nearer one outside");
        assert!(PathIndex::new(&[at(-60.0, 200.0)]).snap_into_area(&square, mid).is_none());
        assert!(edge_only.snap_into_area(&square[..2], mid).is_none(), "not an area");
    }

    #[test]
    fn a_loop_turns_to_start_near_a_path_and_an_open_line_is_cut_there() {
        let path = PathIndex::new(&[at(400.0, 200.0)]);
        let square = vec![at(0.0, 0.0), at(0.0, 400.0), at(400.0, 400.0), at(400.0, 0.0), at(0.0, 0.0)];
        let ring = path.start_near_path(&square).expect("the loop passes the path");
        assert!(distance_m(ring[0], at(400.0, 200.0)) <= NEAR_PATH_M && ring[0] == ring[ring.len() - 1]);
        assert!((crate::geo::polyline_len_m(&ring) - crate::geo::polyline_len_m(&square)).abs() < 1.0, "the whole loop is kept");

        let line = vec![at(0.0, 0.0), at(0.0, 500.0), at(0.0, 1000.0)];
        let near_end = PathIndex::new(&[at(0.0, 990.0)]);
        let cut = near_end.start_near_path(&line).expect("the end is near a path");
        assert!(near_end.near_path(cut[0]));
        assert!(crate::geo::polyline_len_m(&cut) > 900.0, "cut from the far end keeps most of it");
        let middle = PathIndex::new(&[at(0.0, 300.0)]);
        let cut = middle.start_near_path(&line).expect("the middle is near a path");
        assert!(middle.near_path(cut[0]) && crate::geo::polyline_len_m(&cut) > 650.0, "the longer side is kept");
        assert!(PathIndex::new(&[at(500.0, 0.0)]).start_near_path(&line).is_none());
        assert_eq!(near_end.start_near_path(&[at(0.0, 995.0)]), Some(vec![at(0.0, 995.0)]));
    }
}
