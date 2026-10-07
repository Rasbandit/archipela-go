//! A fixed global grid of map tiles. Scans fetch one set of queries per tile, and a tile's query depends only on its cell, so the query cache is
//! shared by every realm: realms that overlap share tiles, and moving or resizing a realm fetches only the cells it newly touches.

use std::collections::BTreeSet;

use crate::geo::{densify, Point};
use crate::zone::Zone;

/// Size of a cell in degrees (both ways): about 2.2 km north-south, 1.5 km east-west at 45 degrees.
pub const CELL_DEG: f64 = 0.02;

/// A cell of the grid, by index (row = latitude, col = longitude). Cell 0,0 starts at 0N 0E.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    pub row: i32,
    pub col: i32,
}

impl Tile {
    pub fn containing(p: Point) -> Tile {
        Tile { row: (p.lat / CELL_DEG).floor() as i32, col: (p.lon / CELL_DEG).floor() as i32 }
    }

    pub fn center(self) -> Point {
        Point::new((f64::from(self.row) + 0.5) * CELL_DEG, (f64::from(self.col) + 0.5) * CELL_DEG)
    }

    /// Overpass bbox filter `south,west,north,east`, written the same way every time so equal tiles give equal query text.
    pub fn filter(self) -> String {
        let (s, w) = (f64::from(self.row) * CELL_DEG, f64::from(self.col) * CELL_DEG);
        format!("{s:.4},{w:.4},{:.4},{:.4}", s + CELL_DEG, w + CELL_DEG)
    }
}

/// The cells a zone touches (not its whole bounding box), in a stable order.
pub fn tiles_for(zone: &Zone) -> Vec<Tile> {
    let (sw, ne) = zone.bbox();
    let (lo, hi) = (Tile::containing(sw), Tile::containing(ne));
    // Cells the outline passes through: sample it finer than a cell is wide.
    let outline: Vec<Point> = match zone {
        Zone::Polygon(v) => {
            let mut closed = v.clone();
            closed.extend(v.first().copied());
            densify(&closed, 150.0)
        }
        Zone::Circle { center, radius_m } | Zone::Annulus { center, max_m: radius_m, .. } => {
            (0..720).map(|i| crate::geo::destination(*center, f64::from(i) * 0.5, *radius_m)).collect()
        }
    };
    let mut out: BTreeSet<Tile> = outline.into_iter().map(Tile::containing).collect();
    // Cells wholly inside have no outline in them: take every cell whose centre is inside.
    for row in lo.row..=hi.row {
        for col in lo.col..=hi.col {
            let t = Tile { row, col };
            if zone.contains(t.center()) {
                out.insert(t);
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn circle(center: Point, r: f64) -> Zone {
        Zone::Circle { center, radius_m: r }
    }

    #[test]
    fn a_tile_filter_is_the_same_text_every_time_and_covers_its_cell() {
        let t = Tile::containing(Point::new(45.5189, -122.6795));
        assert_eq!(t.filter(), t.filter());
        assert_eq!(t.filter(), "45.5000,-122.6800,45.5200,-122.6600");
        assert_eq!(Tile::containing(t.center()), t);
    }

    #[test]
    fn two_overlapping_realms_share_tiles_and_so_share_queries() {
        let a = tiles_for(&circle(Point::new(45.5189, -122.6795), 1500.0));
        let b = tiles_for(&circle(Point::new(45.5230, -122.6700), 1500.0)); // nudged a few hundred metres
        let shared = a.iter().filter(|t| b.contains(t)).count();
        assert!(shared >= a.len() / 2, "nearby realms must mostly share tiles ({shared} of {})", a.len());
        for t in a.iter().filter(|t| b.contains(t)) {
            assert_eq!(t.filter(), t.filter());
        }
    }

    #[test]
    fn a_circle_needs_only_the_cells_it_touches_not_its_bounding_box() {
        let z = circle(Point::new(45.5189, -122.6795), 8000.0);
        let (sw, ne) = z.bbox();
        let (lo, hi) = (Tile::containing(sw), Tile::containing(ne));
        let bbox_cells = ((hi.row - lo.row + 1) * (hi.col - lo.col + 1)) as usize;
        let needed = tiles_for(&z).len();
        assert!(needed < bbox_cells * 9 / 10, "{needed} cells vs {bbox_cells} in the bounding box");
        // every point of the circle's edge falls in a cell we fetch
        let tiles = tiles_for(&z);
        for d in (0..360).step_by(5) {
            assert!(tiles.contains(&Tile::containing(destination(z_center(&z), f64::from(d), 7990.0))));
        }
    }

    fn z_center(z: &Zone) -> Point {
        z.home()
    }

    #[test]
    fn a_polygon_gets_the_cells_along_its_edges_and_inside() {
        let o = Point::new(40.0, -111.0);
        let tri = Zone::Polygon(vec![o, destination(o, 90.0, 6000.0), destination(o, 0.0, 6000.0)]);
        let tiles = tiles_for(&tri);
        for p in [o, destination(o, 90.0, 5990.0), destination(o, 0.0, 5990.0), destination(o, 45.0, 1500.0)] {
            assert!(tiles.contains(&Tile::containing(p)), "{p:?}");
        }
        assert!(!tiles.contains(&Tile::containing(destination(o, 45.0, 8000.0))), "outside the triangle");
    }

    #[test]
    fn moving_a_realm_one_cell_changes_the_tile_set_only_by_the_difference() {
        let c = Point::new(40.0, -111.0);
        let before: BTreeSet<Tile> = tiles_for(&circle(c, 1500.0)).into_iter().collect();
        let after: BTreeSet<Tile> = tiles_for(&circle(destination(c, 90.0, 1500.0), 1500.0)).into_iter().collect();
        let new: Vec<&Tile> = after.difference(&before).collect();
        assert!(!new.is_empty() && new.len() < after.len(), "{} new of {}", new.len(), after.len());
    }

    #[test]
    fn a_tiny_zone_still_gets_a_cell() {
        assert_eq!(tiles_for(&circle(Point::new(10.005, 10.005), 60.0)).len(), 1);
    }
}
