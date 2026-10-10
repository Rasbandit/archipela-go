//! Local east-north frame (a tangent plane around an anchor): the filters work in metres, not degrees.

use crate::geo::Point;

const M_PER_DEG: f64 = 111_195.0;
/// The estimate moves to a new anchor once it is this far from the old one, so the flat-earth error stays small.
pub const REANCHOR_M: f64 = 5_000.0;

/// `deg` wrapped into [-180, 180).
fn wrap_lon(deg: f64) -> f64 {
    (deg + 180.0).rem_euclid(360.0) - 180.0
}

/// An anchor point and the length of a degree of longitude there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    origin: Point,
    m_per_deg_lon: f64,
}

impl Frame {
    /// A frame anchored at `origin`.
    #[must_use]
    pub fn new(origin: Point) -> Self {
        Self { origin, m_per_deg_lon: M_PER_DEG * origin.lat.to_radians().cos().max(1e-6) }
    }

    /// The anchor.
    #[must_use]
    pub fn origin(&self) -> Point {
        self.origin
    }

    /// `p` as metres east and north of the anchor. The longitude difference is wrapped into [-180, 180): the antimeridian is no edge
    /// (adversarial review M2).
    #[must_use]
    pub fn to_enu(&self, p: Point) -> [f64; 2] {
        [wrap_lon(p.lon - self.origin.lon) * self.m_per_deg_lon, (p.lat - self.origin.lat) * M_PER_DEG]
    }

    /// Metres east and north of the anchor as a point, its longitude in [-180, 180).
    #[must_use]
    pub fn to_geo(&self, en: [f64; 2]) -> Point {
        Point::new(self.origin.lat + en[1] / M_PER_DEG, wrap_lon(self.origin.lon + en[0] / self.m_per_deg_lon))
    }

    /// Whether a position this far out should move the anchor.
    #[must_use]
    pub fn needs_reanchor(&self, en: [f64; 2]) -> bool {
        en[0].hypot(en[1]) >= REANCHOR_M
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{destination, distance_m};

    #[test]
    fn enu_round_trip_is_exact_and_distances_match_great_circle_within_half_a_percent() {
        let o = Point::new(40.76, -111.89);
        let f = Frame::new(o);
        for (b, d) in [(0.0, 1000.0), (90.0, 2500.0), (225.0, 4000.0)] {
            let p = destination(o, b, d);
            let en = f.to_enu(p);
            let back = f.to_geo(en);
            assert!((back.lat - p.lat).abs() < 1e-12 && (back.lon - p.lon).abs() < 1e-12);
            assert!((en[0].hypot(en[1]) - distance_m(o, p)).abs() < d * 0.005, "{b} deg {d} m: {en:?}");
        }
        assert_eq!(f.to_enu(o), [0.0, 0.0]);
    }

    #[test]
    fn east_is_x_and_north_is_y() {
        let o = Point::new(10.0, 20.0);
        let f = Frame::new(o);
        let e = f.to_enu(destination(o, 90.0, 100.0));
        let n = f.to_enu(destination(o, 0.0, 100.0));
        assert!(e[0] > 99.0 && e[1].abs() < 0.1, "{e:?}");
        assert!(n[1] > 99.0 && n[0].abs() < 0.1, "{n:?}");
    }

    #[test]
    fn the_antimeridian_is_no_edge() {
        // Adversarial review M2: 179.9999 and -179.9999 are 22 m apart, not 40,000 km; positions come back in [-180, 180).
        let f = Frame::new(Point::new(-17.0, 179.9999));
        let en = f.to_enu(Point::new(-17.0, -179.9999));
        assert!((en[0] - 0.0002 * f.m_per_deg_lon).abs() < 1e-6 && en[1].abs() < 1e-9, "{en:?}");
        let back = f.to_geo(en);
        assert!((back.lon + 179.9999).abs() < 1e-9 && (back.lat + 17.0).abs() < 1e-12, "{back:?}");
        assert!((-180.0..180.0).contains(&f.to_geo([1.0e9, 0.0]).lon));
        let w = Frame::new(Point::new(-17.0, -179.9999));
        assert!((w.to_enu(Point::new(-17.0, 179.9999))[0] + 0.0002 * w.m_per_deg_lon).abs() < 1e-6);
        assert!((w.to_geo([-0.0003 * w.m_per_deg_lon, 0.0]).lon - 179.9998).abs() < 1e-9);
    }

    #[test]
    fn reanchor_is_due_from_five_km() {
        let f = Frame::new(Point::new(0.0, 0.0));
        assert!(!f.needs_reanchor([3000.0, 3999.0]));
        assert!(f.needs_reanchor([3000.0, 4001.0]));
    }
}
