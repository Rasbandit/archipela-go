//! Play zones: where the game may place trips.

use crate::geo::{distance_m, point_in_polygon, Point};
use crate::num::count_f64;

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
            Self::Polygon(v) => {
                let n = count_f64(v.len().max(1));
                Point::new(v.iter().map(|p| p.lat).sum::<f64>() / n, v.iter().map(|p| p.lon).sum::<f64>() / n)
            }
        }
    }

    /// Farthest distance from `home` inside the zone (drives the distance tier step).
    pub fn max_extent_m(&self) -> f64 {
        match self {
            Self::Circle { radius_m, .. } => *radius_m,
            Self::Annulus { max_m, .. } => *max_m,
            Self::Polygon(v) => v.iter().map(|p| distance_m(self.home(), *p)).fold(0.0, f64::max),
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

    /// Axis-aligned bounds as (south-west, north-east).
    pub fn bbox(&self) -> (Point, Point) {
        let pts: Vec<Point> = match self {
            Self::Polygon(v) => v.clone(),
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
