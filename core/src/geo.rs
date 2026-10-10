//! Great-circle math on WGS84 degrees.

use serde::{Deserialize, Serialize};

use crate::num::{ceil_usize, count_f64};

const EARTH_RADIUS_M: f64 = 6_371_000.0;
/// Metres per degree of latitude (and of longitude at the equator) on the [`EARTH_RADIUS_M`] sphere, rounded.
const M_PER_DEG: f64 = 111_195.0;

/// A WGS84 coordinate in degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    /// Latitude in degrees, north positive.
    pub lat: f64,
    /// Longitude in degrees, east positive.
    pub lon: f64,
}

impl Point {
    /// The smaller of two points by (lat, lon): a canonical choice that does not depend on a line's direction.
    #[must_use]
    pub fn min_by_coords(self, other: Self) -> Self {
        if (self.lat, self.lon) <= (other.lat, other.lon) {
            self
        } else {
            other
        }
    }

    /// A point at `lat`, `lon` degrees.
    #[must_use]
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

/// Haversine distance in meters.
#[must_use]
pub fn distance_m(a: Point, b: Point) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Initial bearing from `a` to `b` in degrees (0 = north, clockwise).
#[must_use]
pub fn bearing_deg(a: Point, b: Point) -> f64 {
    let (p1, p2) = (a.lat.to_radians(), b.lat.to_radians());
    let dl = (b.lon - a.lon).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Total length of a polyline in meters.
#[must_use]
pub fn polyline_len_m(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| distance_m(w[0], w[1])).sum()
}

/// Points along a polyline roughly every `step_m` meters (always includes both ends).
#[must_use]
pub fn densify(pts: &[Point], step_m: f64) -> Vec<Point> {
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let seg = distance_m(w[0], w[1]);
        let n = ceil_usize(seg / step_m).max(1);
        let dlon = unwrap_lon(w[1].lon, w[0].lon) - w[0].lon; // the short way, across lon ±180 if that is shorter
        for i in 0..n {
            let t = count_f64(i) / count_f64(n);
            out.push(Point::new(w[0].lat + (w[1].lat - w[0].lat) * t, normal_lon(w[0].lon + dlon * t)));
        }
    }
    if let Some(last) = pts.last() {
        out.push(*last);
    }
    out
}

/// `pts` thinned for drawing, ends kept: first points closer than `min_step_m` to the last kept one are dropped (standing still or
/// GPS scatter becomes one spot), then points within `tolerance_m` of the line through their neighbours ([`simplify`], Douglas-Peucker;
/// wobble goes, real corners stay).
#[must_use]
pub fn simplify_spaced(pts: &[Point], min_step_m: f64, tolerance_m: f64) -> Vec<Point> {
    let (Some(&first), Some(&last)) = (pts.first(), pts.last()) else { return Vec::new() };
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut spaced = vec![first];
    for &p in &pts[1..pts.len() - 1] {
        if spaced.last().is_some_and(|&q| distance_m(q, p) >= min_step_m) {
            spaced.push(p);
        }
    }
    spaced.push(last);
    simplify(&spaced, tolerance_m)
}

/// Average of the points (good enough as a polygon "center" at city scale), the short way across the antimeridian.
#[must_use]
pub fn centroid(pts: &[Point]) -> Point {
    let n = count_f64(pts.len().max(1));
    let around = pts.first().map_or(0.0, |p| p.lon);
    let lon = pts.iter().map(|p| unwrap_lon(p.lon, around)).sum::<f64>() / n;
    Point::new(pts.iter().map(|p| p.lat).sum::<f64>() / n, normal_lon(lon))
}

/// `lon` in [-180, 180].
pub(crate) fn normal_lon(lon: f64) -> f64 {
    if (-180.0..=180.0).contains(&lon) {
        lon
    } else {
        unwrap_lon(lon, 0.0)
    }
}

/// Shortest distance from `p` to the segment `a`-`b`, in metres (flat approximation around `p`, fine at city scale).
#[must_use]
#[allow(clippy::many_single_char_names)] // standard planar-geometry notation (p, a, b, k, t)
pub fn distance_to_segment_m(p: Point, a: Point, b: Point) -> f64 {
    let k = M_PER_DEG;
    let cos_lat = p.lat.to_radians().cos();
    let xy = |q: Point| ((unwrap_lon(q.lon, p.lon) - p.lon) * k * cos_lat, (q.lat - p.lat) * k);
    let ((ax, ay), (bx, by)) = (xy(a), xy(b));
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (-(ax * dx + ay * dy) / len2).clamp(0.0, 1.0) };
    let (cx, cy) = (ax + t * dx, ay + t * dy);
    (cx * cx + cy * cy).sqrt()
}

/// `lon` moved by whole turns to within 180° of `around`, so a shape across the antimeridian is one piece around a point.
pub(crate) fn unwrap_lon(lon: f64, around: f64) -> f64 {
    around + (lon - around + 180.0).rem_euclid(360.0) - 180.0
}

/// Ray-casting point-in-polygon on lat/lon (planar; fine at city scale, and across the antimeridian).
#[must_use]
#[allow(clippy::many_single_char_names)] // standard ray-casting notation (p, v, i, j, a, b)
pub fn point_in_polygon(p: Point, v: &[Point]) -> bool {
    // Corners and the point all within 180° of the first corner: the polygon is one piece even where it crosses lon ±180.
    let Some(first) = v.first() else { return false };
    let near = |q: Point| Point::new(q.lat, unwrap_lon(q.lon, first.lon));
    let p = near(p);
    let mut inside = false;
    let mut j = v.len().wrapping_sub(1);
    for i in 0..v.len() {
        let (a, b) = (near(v[i]), near(v[j]));
        if (a.lat > p.lat) != (b.lat > p.lat) && p.lon < (b.lon - a.lon) * (p.lat - a.lat) / (b.lat - a.lat) + a.lon {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The key of the smallest outline (by its bounding box) that holds `p`, so a park inside a bigger park wins; `None` when none
/// does. An outline needs three corners; one across the antimeridian is measured the short way.
pub fn smallest_containing<'a, K>(p: Point, shapes: impl IntoIterator<Item = (K, &'a [Point])>) -> Option<K> {
    let bbox = |v: &[Point]| {
        let around = v.first().map_or(0.0, |q| q.lon);
        let span = |vals: Vec<f64>| vals.iter().copied().fold(f64::NEG_INFINITY, f64::max) - vals.iter().copied().fold(f64::INFINITY, f64::min);
        span(v.iter().map(|q| q.lat).collect()) * span(v.iter().map(|q| unwrap_lon(q.lon, around)).collect())
    };
    shapes.into_iter().filter(|(_, v)| v.len() >= 3 && point_in_polygon(p, v)).min_by(|a, b| bbox(a.1).total_cmp(&bbox(b.1))).map(|(k, _)| k)
}

/// A point guaranteed to be inside `poly` (falls back to the first vertex nudged inward for degenerate shapes).
#[must_use]
#[allow(clippy::many_single_char_names)] // short loop and geometry names (c, n, i, j, a, b)
pub fn point_inside(poly: &[Point]) -> Point {
    let c = centroid(poly);
    if poly.len() < 3 || point_in_polygon(c, poly) {
        return c;
    }
    // Midpoints of vertex pairs and triples scan the interior of most concave polygons.
    let n = poly.len();
    for step in [n / 2, n / 3, n / 4, 1] {
        let step = step.max(1);
        for i in 0..n {
            let m = centroid(&[poly[i], poly[(i + step) % n]]);
            if point_in_polygon(m, poly) {
                return m;
            }
        }
    }
    for i in 0..n {
        let m = centroid(&[poly[i], poly[(i + 1) % n], poly[(i + 2) % n]]);
        if point_in_polygon(m, poly) {
            return m;
        }
    }
    poly[0]
}

/// Point reached by moving `dist_m` from `from` along `bearing_deg`.
#[must_use]
pub fn destination(from: Point, bearing_deg: f64, dist_m: f64) -> Point {
    let d = dist_m / EARTH_RADIUS_M;
    let (b, lat1, lon1) = (bearing_deg.to_radians(), from.lat.to_radians(), from.lon.to_radians());
    let lat2 = (lat1.sin() * d.cos() + lat1.cos() * d.sin() * b.cos()).asin();
    let lon2 = lon1 + (b.sin() * d.sin() * lat1.cos()).atan2(d.cos() - lat1.sin() * lat2.sin());
    Point::new(lat2.to_degrees(), lon2.to_degrees())
}

/// Douglas-Peucker (the `geo` crate's): the points of `pts` needed to keep the line within `tol_m` metres (both ends always kept). The
/// points are laid flat in metres around the first point's latitude (fine at city scale) so the tolerance means the same at any
/// latitude; a line spanning many kilometres north-south measures its far end slightly off. A tolerance of 0 or less keeps every point.
#[must_use]
pub fn simplify(pts: &[Point], tol_m: f64) -> Vec<Point> {
    use geo::SimplifyIdx;
    let Some(o) = pts.first() else { return Vec::new() };
    let cos_lat = o.lat.to_radians().cos();
    let flat: geo::LineString<f64> = pts.iter().map(|p| geo::Coord { x: (p.lon - o.lon) * M_PER_DEG * cos_lat, y: (p.lat - o.lat) * M_PER_DEG }).collect();
    flat.simplify_idx(tol_m).into_iter().map(|i| pts[i]).collect()
}

/// [`simplify`] that never drops a point marked in `pinned` (a junction shared with another way).
#[must_use]
pub fn simplify_pinned(pts: &[Point], pinned: &[bool], tol_m: f64) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    let mut start = 0;
    for i in 1..pts.len() {
        if i == pts.len() - 1 || pinned.get(i).copied().unwrap_or(false) {
            let piece = simplify(&pts[start..=i], tol_m);
            out.extend(piece.into_iter().skip(usize::from(!out.is_empty())));
            start = i;
        }
    }
    if out.is_empty() {
        out.extend(pts.iter().copied());
    }
    out
}

/// `pts` as an encoded polyline (Google's algorithm: zigzag deltas in 5-bit chunks, offset into printable ASCII), at `factor` units per
/// degree (1e5 is Google's default; 1e7 keeps 1e-7 degree rounding exact).
#[must_use]
pub fn encode_polyline(pts: &[Point], factor: f64) -> String {
    let mut out = Vec::new();
    let mut prev = (0_i64, 0_i64);
    for p in pts {
        let v = (crate::num::round_i64(p.lat * factor), crate::num::round_i64(p.lon * factor));
        for d in [v.0 - prev.0, v.1 - prev.1] {
            let mut u = ((d << 1) ^ (d >> 63)).cast_unsigned();
            while u >= 0x20 {
                out.push(polyline_char(0x20 | (u & 0x1f)));
                u >>= 5;
            }
            out.push(polyline_char(u));
        }
        prev = v;
    }
    out.into_iter().map(char::from).collect()
}

fn polyline_char(chunk: u64) -> u8 {
    // a chunk is at most 0x3f, so the character is at most 126
    u8::try_from(chunk + 63).unwrap_or(b'?')
}

/// The points of an encoded polyline made by [`encode_polyline`] with the same `factor`; `None` if it is not one (bad characters, a value
/// cut short, a sum that overflows, or a point off the globe).
#[must_use]
pub fn decode_polyline(s: &str, factor: f64) -> Option<Vec<Point>> {
    let mut values = Vec::new();
    let (mut acc, mut shift) = (0_u64, 0_u32);
    for b in s.bytes() {
        if !(63..=126).contains(&b) || shift > 60 {
            return None;
        }
        let chunk = u64::from(b - 63);
        acc |= (chunk & 0x1f) << shift;
        shift += 5;
        if chunk < 0x20 {
            values.push((acc >> 1).cast_signed() ^ -((acc & 1).cast_signed()));
            (acc, shift) = (0, 0);
        }
    }
    if shift != 0 || values.len() % 2 != 0 {
        return None;
    }
    let mut at = (0_i64, 0_i64);
    let mut out = Vec::with_capacity(values.len() / 2);
    for d in values.chunks(2) {
        at = (at.0.checked_add(d[0])?, at.1.checked_add(d[1])?);
        let p = Point::new(crate::num::i64_to_f64(at.0) / factor, crate::num::i64_to_f64(at.1) / factor);
        if p.lat.abs() > 90.0 || p.lon.abs() > 180.0 {
            return None;
        }
        out.push(p);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn east(from: Point, m: f64) -> Point {
        destination(from, 90.0, m)
    }

    fn square(lat: f64, lon: f64, side: f64) -> Vec<Point> {
        vec![Point::new(lat, lon), Point::new(lat, lon + side), Point::new(lat + side, lon + side), Point::new(lat + side, lon)]
    }

    #[test]
    fn a_point_picks_the_smallest_outline_around_it() {
        let big = square(0.0, 0.0, 1.0);
        let small = square(0.4, 0.4, 0.2);
        let elsewhere = square(5.0, 5.0, 1.0);
        let shapes = [(1, big.as_slice()), (2, small.as_slice()), (3, elsewhere.as_slice())];
        assert_eq!(smallest_containing(Point::new(0.5, 0.5), shapes), Some(2), "inside both: the inner one");
        assert_eq!(smallest_containing(Point::new(0.1, 0.1), shapes), Some(1));
        assert_eq!(smallest_containing(Point::new(3.0, 3.0), shapes), None, "inside none");
    }

    #[test]
    fn an_outline_across_the_antimeridian_is_measured_the_short_way() {
        // 1° wide across lon 180, inside a 2° square that does not cross it: the narrow one is the smaller.
        let across = vec![Point::new(0.0, 179.5), Point::new(0.0, -179.5), Point::new(1.0, -179.5), Point::new(1.0, 179.5)];
        let wide = square(-0.5, 178.0, 2.0);
        let p = Point::new(0.5, 179.8);
        assert_eq!(smallest_containing(p, [(1, wide.as_slice()), (2, across.as_slice())]), Some(2));
    }

    #[test]
    fn an_outline_with_too_few_corners_holds_nothing() {
        let line = [Point::new(0.0, 0.0), Point::new(1.0, 1.0)];
        assert_eq!(smallest_containing(Point::new(0.5, 0.5), [(1, line.as_slice())]), None);
    }

    #[test]
    fn a_wobbly_straight_walk_simplifies_to_its_ends() {
        let a = Point::new(45.5, -122.6);
        // 20 fixes along 400 m east, each up to 2 m off the line: GPS wobble, not a turn
        let pts: Vec<Point> = (0..20).map(|i| destination(east(a, f64::from(i) * 20.0), 0.0, if i % 2 == 0 { 2.0 } else { -2.0 })).collect();
        let s = simplify_spaced(&pts, 8.0, 4.0);
        assert_eq!(s.len(), 2, "{s:?}");
        assert_eq!((s[0], s[1]), (pts[0], pts[19]), "the ends are kept as they are");
    }

    #[test]
    fn a_real_corner_is_kept() {
        let a = Point::new(45.5, -122.6);
        let corner = east(a, 200.0);
        let pts: Vec<Point> = (0..=10).map(|i| east(a, f64::from(i) * 20.0)).chain((1..=10).map(|i| destination(corner, 0.0, f64::from(i) * 20.0))).collect();
        let s = simplify_spaced(&pts, 8.0, 4.0);
        assert_eq!(s.len(), 3);
        assert!(distance_m(s[1], corner) < 1.0);
    }

    #[test]
    fn standing_still_collapses_to_one_spot() {
        let a = Point::new(45.5, -122.6);
        // 50 fixes scattered within 5 m of one spot (standing still, or at home), then a walk away
        let mut pts: Vec<Point> = (0..50).map(|i| destination(a, f64::from(i * 37 % 360), f64::from(i % 5))).collect();
        pts.extend((1..=5).map(|i| east(a, f64::from(i) * 30.0)));
        let s = simplify_spaced(&pts, 8.0, 4.0);
        assert!(s.len() <= 3, "the scribble is gone: {} points", s.len());
    }

    #[test]
    fn short_lines_are_left_alone() {
        let a = Point::new(45.5, -122.6);
        assert_eq!(simplify_spaced(&[], 8.0, 4.0), Vec::<Point>::new());
        assert_eq!(simplify_spaced(&[a], 8.0, 4.0), vec![a]);
        assert_eq!(simplify_spaced(&[a, east(a, 1.0)], 8.0, 4.0), vec![a, east(a, 1.0)], "two points stay two, however close");
    }

    #[test]
    fn polylines_match_the_published_example() {
        let pts = [Point::new(38.5, -120.2), Point::new(40.7, -120.95), Point::new(43.252, -126.453)];
        assert_eq!(encode_polyline(&pts, 1e5), "_p~iF~ps|U_ulLnnqC_mqNvxq`@");
        assert_eq!(decode_polyline("_p~iF~ps|U_ulLnnqC_mqNvxq`@", 1e5), Some(pts.to_vec()));
    }

    #[test]
    fn polylines_round_trip_exactly_at_seven_decimals() {
        let r7 = |x: f64| crate::num::i64_to_f64(crate::num::round_i64(x * 1e7)) / 1e7;
        let pts: Vec<Point> =
            [(40.123_456_7, -111.765_432_1), (-33.868_819_9, 151.209_295_5), (0.0, 0.0), (-89.999_999_9, 179.999_999_9), (-89.999_999_9, -180.0)]
                .iter()
                .map(|&(lat, lon)| Point::new(r7(lat), r7(lon)))
                .collect();
        assert_eq!(decode_polyline(&encode_polyline(&pts, 1e7), 1e7), Some(pts));
        assert_eq!(encode_polyline(&[], 1e7), "");
        assert_eq!(decode_polyline("", 1e7), Some(vec![]));
    }

    /// Raw polyline values (deltas, in units) as the encoded string, to craft hostile input.
    fn raw_polyline(values: &[i64]) -> String {
        let mut out = String::new();
        for &d in values {
            let mut u = ((d << 1) ^ (d >> 63)).cast_unsigned();
            while u >= 0x20 {
                out.push(char::from(u8::try_from((0x20 | (u & 0x1f)) + 63).unwrap()));
                u >>= 5;
            }
            out.push(char::from(u8::try_from(u + 63).unwrap()));
        }
        out
    }

    #[test]
    fn a_crafted_polyline_that_overflows_or_leaves_the_globe_does_not_decode() {
        let big = i64::MAX - 1;
        assert_eq!(decode_polyline(&raw_polyline(&[big, 0, big, 0]), 1e7), None, "the latitude sum overflows");
        assert_eq!(decode_polyline(&raw_polyline(&[0, -big, 0, -big]), 1e7), None, "the longitude sum overflows");
        assert_eq!(decode_polyline(&raw_polyline(&[910_000_000, 0]), 1e7), None, "latitude 91");
        assert_eq!(decode_polyline(&raw_polyline(&[0, -1_800_000_001]), 1e7), None, "longitude below -180");
        assert_eq!(decode_polyline(&raw_polyline(&[900_000_000, 1_800_000_000]), 1e7), Some(vec![Point::new(90.0, 180.0)]), "the edges are fine");
    }

    #[test]
    fn a_broken_polyline_does_not_decode() {
        assert_eq!(decode_polyline("_p~iF~ps|U_", 1e5), None, "a dangling latitude");
        assert_eq!(decode_polyline("_p~iF~ps|", 1e5), None, "a value cut short");
        assert_eq!(decode_polyline("_p~iF ps|U", 1e5), None, "a character outside the alphabet");
    }

    #[test]
    fn one_degree_of_latitude_is_about_111_km() {
        let d = distance_m(Point::new(40.0, -111.0), Point::new(41.0, -111.0));
        assert!((d - 111_195.0).abs() < 200.0, "got {d}");
    }

    #[test]
    fn bearing_and_destination_round_trip() {
        let a = Point::new(40.0, -111.0);
        let b = destination(a, 90.0, 1000.0);
        assert!((distance_m(a, b) - 1000.0).abs() < 1.0);
        assert!((bearing_deg(a, b) - 90.0).abs() < 0.5);
        assert!(bearing_deg(a, destination(a, 0.0, 500.0)) < 0.5 || bearing_deg(a, destination(a, 0.0, 500.0)) > 359.5);
    }

    #[test]
    fn polygon_contains_and_polyline_densify() {
        let sq = [Point::new(0.0, 0.0), Point::new(0.0, 1.0), Point::new(1.0, 1.0), Point::new(1.0, 0.0)];
        assert!(point_in_polygon(Point::new(0.5, 0.5), &sq));
        assert!(!point_in_polygon(Point::new(1.5, 0.5), &sq));
        let line = [Point::new(40.0, -111.0), destination(Point::new(40.0, -111.0), 0.0, 1000.0)];
        let d = densify(&line, 20.0);
        assert!(d.len() >= 50 && (polyline_len_m(&line) - 1000.0).abs() < 1.0);
    }

    #[test]
    fn a_polygon_across_the_antimeridian_contains_points_on_both_sides() {
        // A 0.2 x 0.2 degree square straddling lon 180 (Fiji).
        let sq = [Point::new(-17.1, 179.9), Point::new(-17.1, -179.9), Point::new(-16.9, -179.9), Point::new(-16.9, 179.9)];
        assert!(point_in_polygon(Point::new(-17.0, 179.95), &sq));
        assert!(point_in_polygon(Point::new(-17.0, -179.95), &sq));
        assert!(!point_in_polygon(Point::new(-17.0, 179.0), &sq)); // west of it
        assert!(!point_in_polygon(Point::new(-17.0, -179.0), &sq)); // east of it
        assert!(!point_in_polygon(Point::new(-17.0, 0.0), &sq)); // the long way round
    }

    #[test]
    fn a_line_across_the_antimeridian_is_sampled_the_short_way() {
        let line = [Point::new(0.0, 179.99), Point::new(0.0, -179.99)]; // ~2.2 km
        let d = densify(&line, 100.0);
        assert!(d.len() < 40, "about 23 samples, not one per 100 m round the world: {}", d.len());
        assert!(d.iter().all(|p| p.lon.abs() > 179.98 && p.lon.abs() <= 180.0), "{d:?}");
    }

    #[test]
    fn the_centre_of_a_shape_across_the_antimeridian_is_on_it() {
        let sq = [Point::new(-17.1, 179.9), Point::new(-17.1, -179.9), Point::new(-16.9, -179.9), Point::new(-16.9, 179.9)];
        let c = centroid(&sq);
        assert!((c.lat + 17.0).abs() < 1e-9 && c.lon.abs() > 179.99, "{c:?}");
        assert!(point_in_polygon(point_inside(&sq), &sq));
        // A concave L across the line: its centroid is outside, so the midpoint search runs.
        let l = [
            Point::new(0.0, 179.0),
            Point::new(0.0, -170.0),
            Point::new(1.0, -170.0),
            Point::new(1.0, 180.0),
            Point::new(10.0, 180.0),
            Point::new(10.0, 179.0),
        ];
        assert!(!point_in_polygon(centroid(&l), &l), "test shape must have a centroid outside");
        assert!(point_in_polygon(point_inside(&l), &l));
    }

    #[test]
    fn distance_to_a_segment_across_the_antimeridian_is_short() {
        let (a, b) = (Point::new(0.0, 179.99), Point::new(0.0, -179.99));
        let p = Point::new(0.01, 180.0); // ~1.1 km north of the segment's middle
        assert!((distance_to_segment_m(p, a, b) - 1112.0).abs() < 5.0, "{}", distance_to_segment_m(p, a, b));
    }

    #[test]
    fn point_inside_handles_concave_shapes() {
        // an L shape whose vertex-average centroid lies outside the polygon
        let l = [Point::new(0.0, 0.0), Point::new(0.0, 10.0), Point::new(1.0, 10.0), Point::new(1.0, 1.0), Point::new(10.0, 1.0), Point::new(10.0, 0.0)];
        assert!(!point_in_polygon(centroid(&l), &l), "test shape must have a centroid outside");
        assert!(point_in_polygon(point_inside(&l), &l));
        let tri = [Point::new(0.0, 0.0), Point::new(0.0, 1.0), Point::new(1.0, 0.0)];
        assert!(point_in_polygon(point_inside(&tri), &tri));
    }

    #[test]
    fn same_point_is_zero() {
        let p = Point::new(45.5, -122.6);
        assert_eq!(distance_m(p, p), 0.0);
    }

    #[test]
    fn distance_is_symmetric() {
        let a = Point::new(45.5152, -122.6784);
        let b = Point::new(45.5231, -122.6765);
        assert!((distance_m(a, b) - distance_m(b, a)).abs() < 1e-6);
    }

    #[test]
    fn simplify_drops_points_on_a_straight_line_and_keeps_corners() {
        let o = Point::new(40.0, -111.0);
        let line: Vec<Point> = (0..=10).map(|i| destination(o, 90.0, 10.0 * f64::from(i))).collect();
        assert_eq!(simplify(&line, 2.0), vec![line[0], line[10]]);
        let corner = vec![o, destination(o, 90.0, 50.0), destination(destination(o, 90.0, 50.0), 0.0, 50.0)];
        assert_eq!(simplify(&corner, 2.0).len(), 3);
    }

    #[test]
    fn no_tolerance_keeps_every_point() {
        let o = Point::new(40.0, -111.0);
        let line: Vec<Point> = (0..=10).map(|i| destination(o, 90.0, 10.0 * f64::from(i))).collect();
        assert_eq!(simplify(&line, 0.0), line);
        assert_eq!(simplify(&line, -1.0), line);
    }

    #[test]
    fn a_pinned_point_survives_simplification() {
        let o = Point::new(40.0, -111.0);
        let line: Vec<Point> = (0..=10).map(|i| destination(o, 90.0, 10.0 * f64::from(i))).collect();
        let mut pinned = vec![false; 11];
        pinned[4] = true;
        assert_eq!(simplify_pinned(&line, &pinned, 2.0), vec![line[0], line[4], line[10]]);
    }

    #[test]
    fn simplify_keeps_short_lines_as_they_are() {
        let (a, b) = (Point::new(40.0, -111.0), Point::new(40.001, -111.0));
        for pts in [vec![], vec![a], vec![a, b]] {
            assert_eq!(simplify(&pts, 2.0), pts);
            assert_eq!(simplify_pinned(&pts, &vec![true; pts.len()], 2.0), pts);
            assert_eq!(simplify_pinned(&pts, &[], 2.0), pts, "no pins given");
        }
    }

    #[test]
    fn a_closed_loop_keeps_its_corners() {
        let o = Point::new(40.0, -111.0);
        let (e, n) = (destination(o, 90.0, 100.0), destination(o, 0.0, 100.0));
        let ne = destination(e, 0.0, 100.0);
        let loop_ = vec![o, destination(o, 90.0, 50.0), e, ne, n, o];
        assert_eq!(simplify(&loop_, 2.0), vec![o, e, ne, n, o], "the mid-edge point goes, the corners and the closing point stay");
        assert_eq!(simplify_pinned(&loop_, &[true, false, false, false, false, true], 2.0), vec![o, e, ne, n, o]);
    }

    #[test]
    fn the_tolerance_is_metres_at_any_latitude() {
        for lat in [0.0, 60.0, -75.0] {
            let o = Point::new(lat, 10.0);
            let (mid, end) = (destination(o, 90.0, 50.0), destination(o, 90.0, 100.0));
            let (bump, dent) = (destination(mid, 0.0, 3.0), destination(mid, 180.0, 1.0));
            assert_eq!(simplify(&[o, bump, end], 2.0), vec![o, bump, end], "a 3 m bump stays at {lat}");
            assert_eq!(simplify(&[o, dent, end], 2.0), vec![o, end], "a 1 m dent goes at {lat}");
            let (side, nudge) = (destination(o, 0.0, 100.0), destination(destination(o, 0.0, 50.0), 90.0, 1.5));
            assert_eq!(simplify(&[o, nudge, side], 2.0), vec![o, side], "1.5 m off a north-south line goes at {lat}");
        }
    }

    #[test]
    fn pins_on_the_ends_change_nothing() {
        let o = Point::new(40.0, -111.0);
        let line: Vec<Point> = (0..=10).map(|i| destination(o, 90.0, 10.0 * f64::from(i))).collect();
        let mut pinned = vec![false; 11];
        (pinned[0], pinned[10]) = (true, true);
        assert_eq!(simplify_pinned(&line, &pinned, 2.0), vec![line[0], line[10]]);
    }
}
