//! Play zones: where the game may place trips.

use crate::geo::{distance_m, Point};

#[derive(Debug, Clone)]
pub enum Zone {
    Circle { center: Point, radius_m: f64 },
    Annulus { center: Point, min_m: f64, max_m: f64 },
    Polygon(Vec<Point>),
}

impl Zone {
    /// Reference point for distance tiers: the center, or the polygon's vertex average.
    pub fn home(&self) -> Point {
        match self {
            Zone::Circle { center, .. } | Zone::Annulus { center, .. } => *center,
            Zone::Polygon(v) => {
                let n = v.len().max(1) as f64;
                Point::new(v.iter().map(|p| p.lat).sum::<f64>() / n, v.iter().map(|p| p.lon).sum::<f64>() / n)
            }
        }
    }

    /// Farthest distance from `home` inside the zone (drives the distance tier step).
    pub fn max_extent_m(&self) -> f64 {
        match self {
            Zone::Circle { radius_m, .. } => *radius_m,
            Zone::Annulus { max_m, .. } => *max_m,
            Zone::Polygon(v) => v.iter().map(|p| distance_m(self.home(), *p)).fold(0.0, f64::max),
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        match self {
            Zone::Circle { center, radius_m } => distance_m(*center, p) <= *radius_m,
            Zone::Annulus { center, min_m, max_m } => {
                let d = distance_m(*center, p);
                d >= *min_m && d <= *max_m
            }
            Zone::Polygon(v) => point_in_polygon(p, v),
        }
    }

    /// Axis-aligned bounds as (south-west, north-east).
    pub fn bbox(&self) -> (Point, Point) {
        let pts: Vec<Point> = match self {
            Zone::Polygon(v) => v.clone(),
            Zone::Circle { center, radius_m } | Zone::Annulus { center, max_m: radius_m, .. } => {
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
    pub fn overpass_filter(&self) -> String {
        match self {
            Zone::Circle { center, radius_m } | Zone::Annulus { center, max_m: radius_m, .. } => {
                format!("around:{},{},{}", radius_m.round(), center.lat, center.lon)
            }
            Zone::Polygon(v) => {
                let pts: Vec<String> = v.iter().map(|p| format!("{} {}", p.lat, p.lon)).collect();
                format!("poly:\"{}\"", pts.join(" "))
            }
        }
    }
}

/// Ray casting on lat/lon (planar; fine at city scale).
fn point_in_polygon(p: Point, v: &[Point]) -> bool {
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
