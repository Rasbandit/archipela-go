//! Great-circle math on WGS84 degrees.

use serde::{Deserialize, Serialize};

const EARTH_RADIUS_M: f64 = 6_371_000.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub lat: f64,
    pub lon: f64,
}

impl Point {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

/// Haversine distance in meters.
pub fn distance_m(a: Point, b: Point) -> f64 {
    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = lat2 - lat1;
    let dlon = (b.lon - a.lon).to_radians();
    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Initial bearing from `a` to `b` in degrees (0 = north, clockwise).
pub fn bearing_deg(a: Point, b: Point) -> f64 {
    let (p1, p2) = (a.lat.to_radians(), b.lat.to_radians());
    let dl = (b.lon - a.lon).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Total length of a polyline in meters.
pub fn polyline_len_m(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| distance_m(w[0], w[1])).sum()
}

/// Points along a polyline roughly every `step_m` meters (always includes both ends).
pub fn densify(pts: &[Point], step_m: f64) -> Vec<Point> {
    let mut out = Vec::new();
    for w in pts.windows(2) {
        let seg = distance_m(w[0], w[1]);
        let n = (seg / step_m).ceil().max(1.0) as usize;
        for i in 0..n {
            let t = i as f64 / n as f64;
            out.push(Point::new(w[0].lat + (w[1].lat - w[0].lat) * t, w[0].lon + (w[1].lon - w[0].lon) * t));
        }
    }
    if let Some(last) = pts.last() {
        out.push(*last);
    }
    out
}

/// Average of the points (good enough as a polygon "center" at city scale).
pub fn centroid(pts: &[Point]) -> Point {
    let n = pts.len().max(1) as f64;
    Point::new(pts.iter().map(|p| p.lat).sum::<f64>() / n, pts.iter().map(|p| p.lon).sum::<f64>() / n)
}

/// Ray-casting point-in-polygon on lat/lon (planar; fine at city scale).
pub fn point_in_polygon(p: Point, v: &[Point]) -> bool {
    let mut inside = false;
    let mut j = v.len().wrapping_sub(1);
    for i in 0..v.len() {
        let (a, b) = (v[i], v[j]);
        if (a.lat > p.lat) != (b.lat > p.lat) && p.lon < (b.lon - a.lon) * (p.lat - a.lat) / (b.lat - a.lat) + a.lon {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Point reached by moving `dist_m` from `from` along `bearing_deg`.
pub fn destination(from: Point, bearing_deg: f64, dist_m: f64) -> Point {
    let d = dist_m / EARTH_RADIUS_M;
    let (b, lat1, lon1) = (bearing_deg.to_radians(), from.lat.to_radians(), from.lon.to_radians());
    let lat2 = (lat1.sin() * d.cos() + lat1.cos() * d.sin() * b.cos()).asin();
    let lon2 = lon1 + (b.sin() * d.sin() * lat1.cos()).atan2(d.cos() - lat1.sin() * lat2.sin());
    Point::new(lat2.to_degrees(), lon2.to_degrees())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
