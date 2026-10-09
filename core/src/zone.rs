//! Play zones: where the game may place trips.

use crate::geo::{centroid, distance_m, point_in_polygon, unwrap_lon, Point};

/// The area in which trips may be placed.
#[derive(Debug, Clone)]
pub enum Zone {
    /// Everything within `radius_m` of `center`.
    Circle {
        /// Middle of the circle.
        center: Point,
        /// Radius in metres.
        radius_m: f64,
    },
    /// The ring between `min_m` and `max_m` of `center`.
    Annulus {
        /// Middle of the ring.
        center: Point,
        /// Inner radius in metres.
        min_m: f64,
        /// Outer radius in metres.
        max_m: f64,
    },
    /// A free-form area given by its corner points.
    Polygon(Vec<Point>),
}

impl Zone {
    /// Reference point for distance tiers: the center, or the polygon's vertex average.
    #[must_use]
    pub fn home(&self) -> Point {
        match self {
            Self::Circle { center, .. } | Self::Annulus { center, .. } => *center,
            Self::Polygon(v) => centroid(v),
        }
    }

    /// Farthest distance from `home` inside the zone (drives the distance tier step).
    pub fn max_extent_m(&self) -> f64 {
        match self {
            Self::Circle { radius_m, .. } => *radius_m,
            Self::Annulus { max_m, .. } => *max_m,
            Self::Polygon(v) => {
                let home = self.home();
                v.iter().map(|p| distance_m(home, *p)).fold(0.0, f64::max)
            }
        }
    }

    /// Whether `p` lies inside the zone.
    #[must_use]
    pub fn contains(&self, p: Point) -> bool {
        match self {
            Self::Circle { center, radius_m } => distance_m(*center, p) <= *radius_m,
            Self::Annulus { center, min_m, max_m } => {
                let d = distance_m(*center, p);
                d >= *min_m && d <= *max_m
            }
            Self::Polygon(v) => point_in_polygon(p, v),
        }
    }

    /// Axis-aligned bounds as (south-west, north-east). Longitudes run the short way round, so across the antimeridian the west
    /// or east edge lies past ±180 (`east - west` is always the real width).
    pub fn bbox(&self) -> (Point, Point) {
        let pts: Vec<Point> = match self {
            Self::Polygon(v) => {
                let around = v.first().map_or(0.0, |p| p.lon);
                v.iter().map(|p| Point::new(p.lat, unwrap_lon(p.lon, around))).collect()
            }
            Self::Circle { center, radius_m } | Self::Annulus { center, max_m: radius_m, .. } => {
                let dlat = radius_m / 111_195.0;
                let dlon = dlat / center.lat.to_radians().cos().max(0.01);
                vec![Point::new(center.lat - dlat, center.lon - dlon), Point::new(center.lat + dlat, center.lon + dlon)]
            }
        };
        let f = |g: fn(&Point) -> f64, pick: fn(f64, f64) -> f64, init: f64| pts.iter().map(g).fold(init, pick);
        (
            Point::new(f(|p| p.lat, f64::min, f64::MAX), f(|p| p.lon, f64::min, f64::MAX)),
            Point::new(f(|p| p.lat, f64::max, f64::MIN), f(|p| p.lon, f64::max, f64::MIN)),
        )
    }

    /// Overpass spatial filter for this zone (annulus queries its outer circle).
    #[must_use]
    pub fn overpass_filter(&self) -> String {
        match self {
            Self::Circle { center, radius_m } | Self::Annulus { center, max_m: radius_m, .. } => {
                format!("around:{},{},{}", radius_m.round(), center.lat, center.lon)
            }
            Self::Polygon(v) => {
                let pts: Vec<String> = v.iter().map(|p| format!("{} {}", p.lat, p.lon)).collect();
                format!("poly:\"{}\"", pts.join(" "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fiji_square() -> Zone {
        Zone::Polygon(vec![Point::new(-17.1, 179.9), Point::new(-17.1, -179.9), Point::new(-16.9, -179.9), Point::new(-16.9, 179.9)])
    }

    #[test]
    fn a_polygon_across_lon_180_has_a_narrow_bbox() {
        let (sw, ne) = fiji_square().bbox();
        assert!((ne.lon - sw.lon - 0.2).abs() < 1e-9, "{sw:?} {ne:?}");
        assert!((sw.lat + 17.1).abs() < 1e-9 && (ne.lat + 16.9).abs() < 1e-9);
    }

    #[test]
    fn a_circle_across_lon_180_has_a_narrow_bbox() {
        let (sw, ne) = Zone::Circle { center: Point::new(-17.0, 179.99), radius_m: 5000.0 }.bbox();
        assert!(sw.lon < 180.0 && ne.lon > 180.0 && ne.lon - sw.lon < 0.2, "{sw:?} {ne:?}");
    }

    #[test]
    fn a_polygon_across_lon_180_has_its_home_on_the_line() {
        let h = fiji_square().home();
        assert!(h.lon.abs() > 179.99 && h.lon.abs() <= 180.0 && (h.lat + 17.0).abs() < 1e-9, "{h:?}");
        assert!(fiji_square().max_extent_m() < 20_000.0);
    }
}
