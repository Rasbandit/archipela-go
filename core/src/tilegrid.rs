//! A fixed global grid of map tiles. Scans fetch one set of queries per tile, and a tile's query depends only on its cell, so the query cache is
//! shared by every realm: realms that overlap share tiles, and moving or resizing a realm fetches only the cells it newly touches.

use std::collections::BTreeSet;

use crate::geo::{densify, Point};
use crate::num::floor_i32;
use crate::zone::Zone;

/// Size of a cell in degrees (both ways): about 2.2 km north-south, 1.5 km east-west at 45 degrees.
pub const CELL_DEG: f64 = 0.02;

/// Columns in one turn of the globe (360 / `CELL_DEG`).
const COLS: i32 = 18_000;

/// A cell of the grid, by index (row = latitude, col = longitude). Cell 0,0 starts at 0N 0E.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tile {
    /// Row index, counted by latitude.
    pub row: i32,
    /// Column index, counted by longitude.
    pub col: i32,
}

impl Tile {
    /// The tile containing `p` (longitudes past ±180 wrap round, so lon 180 and -180 share a tile).
    #[must_use]
    pub fn containing(p: Point) -> Self {
        Self::at(floor_i32(p.lat / CELL_DEG), floor_i32(p.lon / CELL_DEG))
    }

    /// The tile at `row`, `col`, with the column wrapped into one turn of the globe.
    fn at(row: i32, col: i32) -> Self {
        Self { row, col: (col + COLS / 2).rem_euclid(COLS) - COLS / 2 }
    }

    /// The centre point of the tile.
    #[must_use]
    pub fn center(self) -> Point {
        Point::new((f64::from(self.row) + 0.5) * CELL_DEG, (f64::from(self.col) + 0.5) * CELL_DEG)
    }

    /// Overpass bbox filter `south,west,north,east`, written the same way every time so equal tiles give equal query text.
    #[must_use]
    pub fn filter(self) -> String {
        let (s, w) = (f64::from(self.row) * CELL_DEG, f64::from(self.col) * CELL_DEG);
        format!("{s:.4},{w:.4},{:.4},{:.4}", s + CELL_DEG, w + CELL_DEG)
    }
}

/// The cells a zone touches (not its whole bounding box), in a stable order.
pub fn tiles_for(zone: &Zone) -> Vec<Tile> {
    let (sw, ne) = zone.bbox();
    // Unwrapped bounds: across lon 180 the east column runs past the wrap, so the loop below walks straight over it.
    let (row_lo, row_hi) = (floor_i32(sw.lat / CELL_DEG), floor_i32(ne.lat / CELL_DEG));
    let (col_lo, col_hi) = (floor_i32(sw.lon / CELL_DEG), floor_i32(ne.lon / CELL_DEG));
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
    for row in row_lo..=row_hi {
        for col in col_lo..=col_hi {
            let t = Tile::at(row, col);
            if zone.contains(t.center()) {
                out.insert(t);
            }
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
#[allow(clippy::cast_sign_loss)] // test code: test fixtures use small, known-positive numbers
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
    fn a_zone_across_lon_180_gets_cells_on_both_sides_and_no_more() {
        let c = Point::new(-17.0, 179.99);
        let tiles = tiles_for(&circle(c, 3000.0));
        assert!(tiles.len() < 200, "{} cells", tiles.len());
        assert!(tiles.iter().all(|t| (-9000..9000).contains(&t.col)), "{tiles:?}");
        for p in [c, destination(c, 90.0, 2990.0), destination(c, 270.0, 2990.0)] {
            assert!(tiles.contains(&Tile::containing(p)), "{p:?}");
        }
        assert_eq!(Tile::containing(Point::new(0.0, 180.0)), Tile::containing(Point::new(0.0, -180.0)));
    }

    #[test]
    fn a_tiny_zone_still_gets_a_cell() {
        assert_eq!(tiles_for(&circle(Point::new(10.005, 10.005), 60.0)).len(), 1);
    }
}
