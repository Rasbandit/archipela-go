//! The near-a-path rule: every point a player must reach lies within [`NEAR_PATH_M`] of a walkable road or path from the zone's scan,
//! so nobody has to go off-road or into a backyard.
//!
//! [`PathIndex`] buckets the scan's street points, and the pieces of street between consecutive samples of a way, into a grid of
//! [`NEAR_PATH_M`] cells, so "is this near a path" looks at a handful of cells instead of the whole zone. Distances are to the street
//! itself, not only to its samples (60 m apart, or more when a big scan is thinned).

use std::collections::HashMap;

use crate::geo::{densify, distance_m, distance_to_segment_m, point_in_polygon, Point};
use crate::num::{ceil_usize, count_f64, round_i64};

/// How far a quest point may be from the nearest scanned street or path, in metres.
pub const NEAR_PATH_M: f64 = 30.0;

const M_PER_DEG_LAT: f64 = 111_195.0;
/// Spacing of the samples taken along a line when looking for its first point near a path, in metres.
const LINE_STEP_M: f64 = 5.0;
/// Spacing of the spots tried along a piece of street when snapping an area onto it, in metres.
const SNAP_STEP_M: f64 = 10.0;

type Cell = (i64, i64);
type Seg = (Point, Point);

/// A grid index over the walkable streets and paths of a zone: their sample points and the pieces of street between them.
#[derive(Debug, Default)]
pub struct PathIndex {
    cells: HashMap<Cell, Vec<Point>>,
    segs: HashMap<Cell, Vec<Seg>>,
    dlat: f64,
    dlon: f64,
}

impl PathIndex {
    /// Index `points` (a zone's street and path points) on their own, with no street known between them.
    #[must_use]
    pub fn new(points: &[Point]) -> Self {
        Self::with_segments(points, &[])
    }

    /// Index `points` and the pieces of street `segments` between them (see [`crate::scan::Atlas::street_links`]).
    #[must_use]
    pub fn with_segments(points: &[Point], segments: &[Seg]) -> Self {
        let lat = points.first().or_else(|| segments.first().map(|s| &s.0)).map_or(0.0, |p| p.lat);
        let dlat = NEAR_PATH_M / M_PER_DEG_LAT;
        let dlon = dlat / lat.to_radians().cos().max(0.01);
        let mut idx = Self { cells: HashMap::new(), segs: HashMap::new(), dlat, dlon };
        for p in points {
            idx.cells.entry(idx.cell(*p)).or_default().push(*p);
        }
        for s in segments {
            let ((la, lo), (lb, hb)) = (idx.cell(sw_of(s.0, s.1)), idx.cell(ne_of(s.0, s.1)));
            for i in la..=lb {
                for j in lo..=hb {
                    idx.segs.entry((i, j)).or_default().push(*s);
                }
            }
        }
        idx
    }

    /// Whether the index holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty() && self.segs.is_empty()
    }

    fn cell(&self, p: Point) -> Cell {
        (round_i64((p.lat / self.dlat).floor()), round_i64((p.lon / self.dlon).floor()))
    }

    /// The cells covering the box `sw`..`ne` grown by `margin_m`, in a fixed order.
    fn keys(&self, sw: Point, ne: Point, margin_m: f64) -> impl Iterator<Item = Cell> {
        // One spare cell each way absorbs the change of a degree of longitude's length across a city-sized zone.
        let pad = i64::try_from(ceil_usize(margin_m / NEAR_PATH_M)).unwrap_or(i64::MAX / 4) + 1;
        let ((la, lo), (lb, hb)) = (self.cell(sw), self.cell(ne));
        let empty = self.is_empty();
        (la - pad..=lb + pad).filter(move |_| !empty).flat_map(move |i| (lo - pad..=hb + pad).map(move |j| (i, j)))
    }

    /// Sample points in the cells around the box.
    fn around(&self, sw: Point, ne: Point, margin_m: f64) -> impl Iterator<Item = Point> + '_ {
        self.keys(sw, ne, margin_m).filter_map(|k| self.cells.get(&k)).flatten().copied()
    }

    /// Pieces of street in the cells around the box (one may come up more than once).
    fn segs_around(&self, sw: Point, ne: Point, margin_m: f64) -> impl Iterator<Item = Seg> + '_ {
        self.keys(sw, ne, margin_m).filter_map(|k| self.segs.get(&k)).flatten().copied()
    }

    /// The sample point nearest to `p` within `max_m`, with its distance.
    #[must_use]
    pub fn nearest(&self, p: Point, max_m: f64) -> Option<(Point, f64)> {
        self.around(p, p, max_m).map(|q| (q, distance_m(p, q))).filter(|(_, d)| *d <= max_m).min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// The spot on a street or path nearest to `p` within `max_m` (a sample or a point between two), with its distance.
    #[must_use]
    pub fn nearest_on_path(&self, p: Point, max_m: f64) -> Option<(Point, f64)> {
        let on_segs = self.segs_around(p, p, max_m).map(|(a, b)| {
            let q = closest_on_segment(p, a, b);
            (q, distance_m(p, q))
        });
        self.nearest(p, max_m).into_iter().chain(on_segs).filter(|(_, d)| *d <= max_m).min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Whether `p` is within `max_m` of a street or path.
    #[must_use]
    pub fn within(&self, p: Point, max_m: f64) -> bool {
        self.around(p, p, max_m).any(|q| distance_m(p, q) <= max_m) || self.segs_around(p, p, max_m).any(|(a, b)| distance_to_segment_m(p, a, b) <= max_m)
    }

    /// Whether `p` is within [`NEAR_PATH_M`] of a street or path.
    #[must_use]
    pub fn near_path(&self, p: Point) -> bool {
        self.within(p, NEAR_PATH_M)
    }

    /// The share (0 to 1) of the samples taken every `step_m` along `pts` that lie within `max_m` of a street or path.
    #[must_use]
    pub fn share_near(&self, pts: &[Point], step_m: f64, max_m: f64) -> f64 {
        let dense = densify(pts, step_m);
        if dense.is_empty() {
            return 0.0;
        }
        count_f64(dense.iter().filter(|q| self.within(**q, max_m)).count()) / count_f64(dense.len())
    }

    /// A spot on a path inside the area `poly`, or failing that within [`NEAR_PATH_M`] of its edge, nearest to `prefer`.
    #[must_use]
    pub fn snap_into_area(&self, poly: &[Point], prefer: Point) -> Option<Point> {
        self.snap_into_area_where(poly, prefer, &|_| true)
    }

    /// Like [`Self::snap_into_area`], but only spots `ok` accepts (for example far enough from home) are taken.
    #[must_use]
    pub fn snap_into_area_where(&self, poly: &[Point], prefer: Point, ok: &dyn Fn(Point) -> bool) -> Option<Point> {
        if poly.len() < 3 {
            return None;
        }
        let sw = poly.iter().fold(poly[0], |a, p| sw_of(a, *p));
        let ne = poly.iter().fold(poly[0], |a, p| ne_of(a, *p));
        // Each edge with its box grown by NEAR_PATH_M: only spots inside the box can be close enough to that edge.
        let edges: Vec<(Seg, Point, Point)> = poly
            .windows(2)
            .map(|w| (w[0], w[1]))
            .chain(std::iter::once((poly[poly.len() - 1], poly[0])))
            .map(|(a, b)| {
                let (s, n) = (sw_of(a, b), ne_of(a, b));
                ((a, b), Point::new(s.lat - self.dlat, s.lon - self.dlon), Point::new(n.lat + self.dlat, n.lon + self.dlon))
            })
            .collect();
        let near_edge = |q: Point| {
            edges
                .iter()
                .any(|((a, b), s, n)| (s.lat..=n.lat).contains(&q.lat) && (s.lon..=n.lon).contains(&q.lon) && distance_to_segment_m(q, *a, *b) <= NEAR_PATH_M)
        };
        let on_segs = self.segs_around(sw, ne, NEAR_PATH_M).flat_map(|(a, b)| {
            let n = ceil_usize(distance_m(a, b) / SNAP_STEP_M).max(1);
            (0..=n).map(move |i| lerp(a, b, count_f64(i) / count_f64(n))).chain(std::iter::once(closest_on_segment(prefer, a, b)))
        });
        // Inside beats next-to-the-edge; then the nearest to `prefer` wins.
        self.around(sw, ne, NEAR_PATH_M)
            .chain(on_segs)
            .filter(|q| ok(*q))
            .filter_map(|q| if point_in_polygon(q, poly) { Some((0u8, q)) } else { near_edge(q).then_some((1u8, q)) })
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
            (0..n).map(|s| lerp(w[0], w[1], count_f64(s) / count_f64(n))).find(|q| self.near_path(*q)).map(|q| (i, q))
        })
    }
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.lat + (b.lat - a.lat) * t, a.lon + (b.lon - a.lon) * t)
}

fn sw_of(a: Point, b: Point) -> Point {
    Point::new(a.lat.min(b.lat), a.lon.min(b.lon))
}

fn ne_of(a: Point, b: Point) -> Point {
    Point::new(a.lat.max(b.lat), a.lon.max(b.lon))
}

/// The point of segment `a`-`b` closest to `p` (flat approximation around `p`, fine at city scale).
fn closest_on_segment(p: Point, a: Point, b: Point) -> Point {
    let cos_lat = p.lat.to_radians().cos();
    let (ax, ay, bx, by) = ((a.lon - p.lon) * cos_lat, a.lat - p.lat, (b.lon - p.lon) * cos_lat, b.lat - p.lat);
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0) };
    lerp(a, b, t)
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

    #[test]
    fn a_point_beside_the_street_between_two_samples_is_near_it() {
        // Samples 60 m apart (stride 1) and 120 m apart (stride 2): the middle of the street is far from both samples.
        for gap in [60.0, 120.0] {
            let (a, b) = (at(0.0, 0.0), at(0.0, gap));
            let beside = at(20.0, gap / 2.0);
            assert!(!PathIndex::new(&[a, b]).near_path(beside), "points alone miss it (gap {gap})");
            let idx = PathIndex::with_segments(&[a, b], &[(a, b)]);
            assert!(idx.near_path(beside), "20 m off the middle of a {gap} m piece of street");
            assert!(!idx.near_path(at(40.0, gap / 2.0)), "40 m off is too far");
            assert!(idx.within(at(25.0, gap / 2.0), 26.0) && !idx.within(at(25.0, gap / 2.0), 24.0));
            let (q, d) = idx.nearest_on_path(beside, 50.0).expect("the street is 20 m away");
            assert!((d - 20.0).abs() < 0.5 && distance_m(q, at(0.0, gap / 2.0)) < 0.5, "{d} m, at {q:?}");
            assert!(idx.nearest_on_path(at(200.0, 0.0), 50.0).is_none());
        }
    }

    #[test]
    fn an_area_snaps_onto_a_street_running_through_it_and_honours_a_filter() {
        // A street crosses the square with samples only outside it: the snap lands on the street inside the square.
        let square = vec![at(0.0, 0.0), at(0.0, 200.0), at(200.0, 200.0), at(200.0, 0.0), at(0.0, 0.0)];
        let (a, b) = (at(100.0, -100.0), at(100.0, 300.0));
        let idx = PathIndex::with_segments(&[a, b], &[(a, b)]);
        let q = idx.snap_into_area(&square, at(100.0, 100.0)).expect("the street runs through the park");
        assert!(point_in_polygon(q, &square) && distance_m(q, at(100.0, 100.0)) < 1.0);
        let far_east = |p: Point| distance_m(at(100.0, 0.0), p) >= 150.0;
        let q = idx.snap_into_area_where(&square, at(100.0, 100.0), &far_east).expect("another spot on the street qualifies");
        assert!(far_east(q) && (point_in_polygon(q, &square) || distance_m(q, at(100.0, 200.0)) <= NEAR_PATH_M));
        assert!(idx.snap_into_area_where(&square, at(100.0, 100.0), &|_| false).is_none());
    }

    #[test]
    fn the_share_of_a_line_near_a_path_counts_its_samples() {
        let street = (at(0.0, 0.0), at(0.0, 500.0));
        let idx = PathIndex::with_segments(&[street.0, street.1], &[street]);
        let line = vec![at(10.0, 0.0), at(10.0, 500.0), at(300.0, 500.0), at(300.0, 1000.0)]; // half beside the street, half far away
        let share = idx.share_near(&line, 20.0, NEAR_PATH_M);
        assert!((0.3..0.6).contains(&share), "share {share}");
        assert!(idx.share_near(&line, 20.0, 5.0) < 0.05, "a tighter reach finds none");
        assert!(idx.share_near(&[], 20.0, NEAR_PATH_M).abs() < f64::EPSILON);
    }
}
