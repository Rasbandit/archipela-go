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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_degree_of_latitude_is_about_111_km() {
        let d = distance_m(Point::new(40.0, -111.0), Point::new(41.0, -111.0));
        assert!((d - 111_195.0).abs() < 200.0, "got {d}");
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
