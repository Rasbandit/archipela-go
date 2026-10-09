//! Play zones: where the game may place trips.

use crate::geo::{centroid, distance_m, distance_to_segment_m, point_in_polygon, unwrap_lon, Point};

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

    /// Whether any part of the segment `a`-`b` lies inside the zone: an end inside, or the segment crossing it with both ends outside.
    #[must_use]
    pub fn touches_segment(&self, a: Point, b: Point) -> bool {
        match self {
            Self::Circle { center, radius_m } => distance_to_segment_m(*center, a, b) <= *radius_m,
            // the nearest point of the segment is within the outer circle and its farthest point (an end) outside the hole
            Self::Annulus { center, min_m, max_m } => {
                distance_to_segment_m(*center, a, b) <= *max_m && distance_m(*center, a).max(distance_m(*center, b)) >= *min_m
            }
            Self::Polygon(v) => {
                // Planar crossing on longitudes taken the short way from `a`, so a zone across lon ±180 is one piece.
                let near = |q: Point| Point::new(q.lat, unwrap_lon(q.lon, a.lon));
                let (pa, pb) = (near(a), near(b));
                point_in_polygon(a, v) || point_in_polygon(b, v) || (0..v.len()).any(|i| segments_cross(pa, pb, near(v[i]), near(v[(i + 1) % v.len()])))
            }
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

/// Whether segments `a`-`b` and `c`-`d` cross or touch (planar on lat/lon; fine at city scale).
#[allow(clippy::many_single_char_names)] // standard planar-geometry notation (a, b, c, d, p, q, r)
fn segments_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    // which side of p-q the point r is on: 1 left, -1 right, 0 on the line
    let orient = |p: Point, q: Point, r: Point| {
        let x = (q.lon - p.lon) * (r.lat - p.lat) - (q.lat - p.lat) * (r.lon - p.lon);
        i8::from(x > 0.0) - i8::from(x < 0.0)
    };
    let on = |p: Point, q: Point, r: Point| r.lat >= p.lat.min(q.lat) && r.lat <= p.lat.max(q.lat) && r.lon >= p.lon.min(q.lon) && r.lon <= p.lon.max(q.lon);
    let (o1, o2, o3, o4) = (orient(a, b, c), orient(a, b, d), orient(c, d, a), orient(c, d, b));
    (o1 != o2 && o3 != o4) || (o1 == 0 && on(a, b, c)) || (o2 == 0 && on(a, b, d)) || (o3 == 0 && on(c, d, a)) || (o4 == 0 && on(c, d, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

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

    #[test]
    fn a_segment_across_lon_180_touches_a_polygon_there_the_short_way() {
        // A way through Fiji's square with both ends outside it, one on each side of the antimeridian.
        let (w, e) = (Point::new(-17.0, 179.8), Point::new(-17.0, -179.8));
        assert!(fiji_square().touches_segment(w, e));
        let (n_w, n_e) = (Point::new(-16.5, 179.8), Point::new(-16.5, -179.8));
        assert!(!fiji_square().touches_segment(n_w, n_e), "north of the square");
    }

    #[test]
    fn a_segment_touches_a_zone_when_it_crosses_it_even_with_both_ends_outside() {
        let o = Point::new(40.0, -111.0);
        let (w, e) = (destination(o, 270.0, 600.0), destination(o, 90.0, 600.0));
        let (nw, ne) = (destination(w, 0.0, 700.0), destination(e, 0.0, 700.0));
        let circle = Zone::Circle { center: o, radius_m: 500.0 };
        assert!(circle.touches_segment(w, e));
        assert!(circle.touches_segment(o, nw), "one end inside");
        assert!(!circle.touches_segment(nw, ne));
        let ring = Zone::Annulus { center: o, min_m: 100.0, max_m: 500.0 };
        assert!(ring.touches_segment(w, e), "crosses the ring");
        assert!(!ring.touches_segment(o, destination(o, 0.0, 50.0)), "inside the hole");
        assert!(!ring.touches_segment(nw, ne));
        let square = Zone::Polygon(vec![
            destination(destination(o, 0.0, 500.0), 270.0, 500.0),
            destination(destination(o, 0.0, 500.0), 90.0, 500.0),
            destination(destination(o, 180.0, 500.0), 90.0, 500.0),
            destination(destination(o, 180.0, 500.0), 270.0, 500.0),
        ]);
        assert!(square.touches_segment(w, e), "crosses two edges");
        assert!(square.touches_segment(o, nw), "one end inside");
        assert!(!square.touches_segment(nw, ne));
    }
}
