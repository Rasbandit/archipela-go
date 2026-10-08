//! Realms: saved, user-drawn geofences with a mode tag. Persisted as JSON in the app's files dir.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::geo::{centroid, Point};
use crate::marks::Marks;
use crate::scan::Atlas;
use crate::zone::Zone;

/// Metres east/north of the first vertex, on a flat map: enough for shapes a few kilometres across.
fn flatten(vertices: &[Point]) -> Vec<(f64, f64)> {
    let o = vertices[0];
    let k = o.lat.to_radians().cos();
    vertices.iter().map(|v| ((v.lon - o.lon) * 111_195.0 * k, (v.lat - o.lat) * 111_195.0)).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    Circle { center: Point, radius_m: f64 },
    Polygon { vertices: Vec<Point> },
}

/// Where a point is relative to a zone area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Proximity {
    Inside,
    /// Within `NEAR_ZONE_M` of the area: precise GPS starts here so arrival is not missed.
    Near,
    Far,
}

pub const NEAR_ZONE_M: f64 = 300.0;

/// The closest classification of `p` across `shapes` (Inside beats Near beats Far); `None` with no shapes.
pub fn closest_proximity(shapes: &[Shape], p: Point) -> Option<Proximity> {
    shapes.iter().map(|s| s.proximity(p)).min()
}

impl Shape {
    pub fn to_zone(&self) -> Zone {
        match self {
            Shape::Circle { center, radius_m } => Zone::Circle { center: *center, radius_m: *radius_m },
            Shape::Polygon { vertices } => Zone::Polygon(vertices.clone()),
        }
    }

    pub fn center(&self) -> Point {
        match self {
            Shape::Circle { center, .. } => *center,
            Shape::Polygon { vertices } => centroid(vertices),
        }
    }

    /// Area in square metres. A polygon is measured on a flat map centred on itself (accurate for realm-sized shapes).
    pub fn area_m2(&self) -> f64 {
        match self {
            Shape::Circle { radius_m, .. } => std::f64::consts::PI * radius_m * radius_m,
            Shape::Polygon { vertices } if vertices.len() >= 3 => {
                let flat = flatten(vertices);
                (0..flat.len()).map(|i| flat[i].0 * flat[(i + 1) % flat.len()].1 - flat[(i + 1) % flat.len()].0 * flat[i].1).sum::<f64>().abs() / 2.0
            }
            Shape::Polygon { .. } => 0.0,
        }
    }

    /// Length of the outline in metres.
    pub fn perimeter_m(&self) -> f64 {
        match self {
            Shape::Circle { radius_m, .. } => 2.0 * std::f64::consts::PI * radius_m,
            Shape::Polygon { vertices } if vertices.len() >= 3 => {
                (0..vertices.len()).map(|i| crate::geo::distance_m(vertices[i], vertices[(i + 1) % vertices.len()])).sum()
            }
            Shape::Polygon { .. } => 0.0,
        }
    }

    /// The farthest any part of the shape is from `from`, in a straight line.
    pub fn farthest_m(&self, from: Point) -> f64 {
        match self {
            Shape::Circle { center, radius_m } => crate::geo::distance_m(from, *center) + radius_m,
            Shape::Polygon { vertices } => vertices.iter().map(|v| crate::geo::distance_m(from, *v)).fold(0.0, f64::max),
        }
    }

    /// How far outside the shape `p` is, in metres; 0 when it is inside.
    pub fn distance_m(&self, p: Point) -> f64 {
        match self {
            Shape::Circle { center, radius_m } => (crate::geo::distance_m(p, *center) - radius_m).max(0.0),
            Shape::Polygon { vertices } => {
                if crate::geo::point_in_polygon(p, vertices) || vertices.len() < 2 {
                    return 0.0;
                }
                (0..vertices.len()).map(|i| crate::geo::distance_to_segment_m(p, vertices[i], vertices[(i + 1) % vertices.len()])).fold(f64::INFINITY, f64::min)
            }
        }
    }

    /// Where `p` is relative to this area: inside, within `NEAR_ZONE_M` of it, or far.
    pub fn proximity(&self, p: Point) -> Proximity {
        let d = self.distance_m(p);
        if d == 0.0 {
            Proximity::Inside
        } else if d <= NEAR_ZONE_M {
            Proximity::Near
        } else {
            Proximity::Far
        }
    }

    pub fn is_valid(&self) -> bool {
        match self {
            Shape::Circle { radius_m, .. } => *radius_m >= 50.0,
            Shape::Polygon { vertices } => vertices.len() >= 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Realm {
    pub id: String,
    pub name: String,
    /// The icon picked for the realm (a name from the app's icon set); `None` until one is chosen. (Older files may still carry a `mode`; it is ignored,
    /// since how you travel is chosen per zone when a game is made.)
    #[serde(default)]
    pub icon: Option<String>,
    /// The active shape: the one scanned and played in.
    pub shape: Shape,
    /// The other kind of shape (circle or polygon), kept so switching back and forth in the editor loses no work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spare: Option<Shape>,
    #[serde(default)]
    pub scanned_at_ms: Option<u64>,
}

impl Realm {
    fn each_shape(&self) -> impl Iterator<Item = &Shape> {
        std::iter::once(&self.shape).chain(self.spare.iter())
    }

    /// The circle (center, radius in metres), whether active or kept in reserve.
    pub fn circle(&self) -> Option<(Point, f64)> {
        self.each_shape().find_map(|s| match s {
            Shape::Circle { center, radius_m } => Some((*center, *radius_m)),
            Shape::Polygon { .. } => None,
        })
    }

    /// The polygon corners, whether active or kept in reserve.
    pub fn polygon(&self) -> Option<&[Point]> {
        self.each_shape().find_map(|s| match s {
            Shape::Polygon { vertices } => Some(vertices.as_slice()),
            Shape::Circle { .. } => None,
        })
    }

    pub fn polygon_active(&self) -> bool {
        matches!(self.shape, Shape::Polygon { .. })
    }
}

/// Files under one directory: `realms.json`, `home.json`, `atlas/<realm id>.json`.
pub struct RealmStore {
    dir: PathBuf,
}

fn io(e: std::io::Error) -> String {
    e.to_string()
}

impl RealmStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn atlas_path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        self.dir.join("atlas").join(format!("{safe}.json"))
    }

    fn marks_path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        self.dir.join("marks").join(format!("{safe}.json"))
    }

    pub fn marks(&self, id: &str) -> Marks {
        std::fs::read_to_string(self.marks_path(id)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_marks(&self, id: &str, marks: &Marks) -> Result<(), String> {
        let path = self.marks_path(id);
        std::fs::create_dir_all(path.parent().unwrap_or(&self.dir)).map_err(io)?;
        std::fs::write(path, serde_json::to_string(marks).map_err(|e| e.to_string())?).map_err(io)
    }

    pub fn list(&self) -> Vec<Realm> {
        std::fs::read_to_string(self.dir.join("realms.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    fn write_list(&self, realms: &[Realm]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(io)?;
        let body = serde_json::to_string_pretty(realms).map_err(|e| e.to_string())?;
        std::fs::write(self.dir.join("realms.json"), body).map_err(io)
    }

    /// Insert or replace by id.
    pub fn save(&self, realm: &Realm) -> Result<(), String> {
        if !realm.shape.is_valid() {
            return Err("a realm needs a polygon of 3+ points or a circle of at least 50 m".into());
        }
        let mut all = self.list();
        match all.iter_mut().find(|r| r.id == realm.id) {
            Some(existing) => *existing = realm.clone(),
            None => all.push(realm.clone()),
        }
        self.write_list(&all)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let all: Vec<Realm> = self.list().into_iter().filter(|r| r.id != id).collect();
        self.write_list(&all)?;
        let _ = std::fs::remove_file(self.atlas_path(id));
        let _ = std::fs::remove_file(self.marks_path(id));
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<Realm> {
        self.list().into_iter().find(|r| r.id == id)
    }

    pub fn save_atlas(&self, atlas: &Atlas) -> Result<(), String> {
        let path = self.atlas_path(&atlas.realm_id);
        std::fs::create_dir_all(path.parent().unwrap_or(&self.dir)).map_err(io)?;
        std::fs::write(path, serde_json::to_string(atlas).map_err(|e| e.to_string())?).map_err(io)
    }

    pub fn load_atlas(&self, id: &str) -> Option<Atlas> {
        std::fs::read_to_string(self.atlas_path(id)).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn home(&self) -> Option<Point> {
        std::fs::read_to_string(self.dir.join("home.json")).ok().and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn set_home(&self, home: Point) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(io)?;
        std::fs::write(self.dir.join("home.json"), serde_json::to_string(&home).map_err(|e| e.to_string())?).map_err(io)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("apgo-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn realm(id: &str) -> Realm {
        Realm {
            id: id.into(),
            name: "Home Turf".into(),
            icon: None,
            shape: Shape::Circle { center: Point::new(40.0, -111.0), radius_m: 1500.0 },
            spare: None,
            scanned_at_ms: None,
        }
    }

    #[test]
    fn a_realm_keeps_both_shapes_but_only_one_is_active() {
        let tri = vec![Point::new(40.0, -111.0), Point::new(40.0, -110.99), Point::new(40.01, -111.0)];
        let mut r = realm("a"); // active circle
        assert!(r.circle().is_some() && r.polygon().is_none() && !r.polygon_active());
        r.spare = Some(Shape::Polygon { vertices: tri.clone() });
        assert_eq!(r.polygon().unwrap(), tri.as_slice());
        assert_eq!(r.circle().unwrap().1, 1500.0);
        assert!(!r.polygon_active());
        // The scan zone is the active shape only.
        assert!(matches!(r.shape.to_zone(), Zone::Circle { .. }));

        let store = RealmStore::new(tmp("spare"));
        store.save(&r).unwrap();
        let back = store.get("a").unwrap();
        assert_eq!(back.polygon().unwrap().len(), 3, "the inactive shape survives a save and reload");
    }

    #[test]
    fn realms_saved_before_spare_shapes_still_load() {
        let old = r#"[{"id":"a","name":"Old","mode":"walk","shape":{"kind":"circle","center":{"lat":40.0,"lon":-111.0},"radius_m":900.0},"scanned_at_ms":5}]"#;
        let dir = tmp("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("realms.json"), old).unwrap();
        let r = RealmStore::new(&dir).get("a").unwrap();
        assert!(r.spare.is_none() && r.circle().is_some() && r.polygon().is_none());
    }

    #[test]
    fn realms_saved_with_a_mode_or_modes_still_load_and_the_tags_are_ignored() {
        let old = r#"[{"id":"a","name":"Old","mode":"bike","shape":{"kind":"circle","center":{"lat":40.0,"lon":-111.0},"radius_m":900.0}},
                      {"id":"b","name":"Newer","modes":["walk","run"],"shape":{"kind":"circle","center":{"lat":40.0,"lon":-111.0},"radius_m":900.0}}]"#;
        let dir = tmp("legacy-mode");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("realms.json"), old).unwrap();
        let store = RealmStore::new(&dir);
        assert_eq!((store.get("a").unwrap().name, store.get("b").unwrap().name), ("Old".to_string(), "Newer".to_string()));
        assert_eq!(store.get("a").unwrap().icon, None);
    }

    #[test]
    fn a_circle_has_the_area_perimeter_and_reach_of_a_circle() {
        let center = Point::new(40.0, -111.0);
        let c = Shape::Circle { center, radius_m: 1000.0 };
        assert!((c.area_m2() - std::f64::consts::PI * 1_000_000.0).abs() < 1.0);
        assert!((c.perimeter_m() - 2.0 * std::f64::consts::PI * 1000.0).abs() < 1.0);
        let home = crate::geo::destination(center, 90.0, 400.0);
        assert!((c.farthest_m(home) - 1400.0).abs() < 2.0, "the far side of the circle is distance + radius from home");
        assert!((c.farthest_m(center) - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn a_polygon_has_its_own_area_perimeter_and_farthest_corner() {
        let o = Point::new(40.0, -111.0);
        let (e, ne, n) = (crate::geo::destination(o, 90.0, 1000.0), Point::new(0.0, 0.0), crate::geo::destination(o, 0.0, 1000.0));
        let _ = ne;
        let square = Shape::Polygon { vertices: vec![o, e, Point::new(n.lat, e.lon), n] };
        let area = square.area_m2();
        assert!((area - 1_000_000.0).abs() < 15_000.0, "about 1 km2, got {area}");
        assert!((square.perimeter_m() - 4000.0).abs() < 40.0);
        let far = square.farthest_m(o);
        assert!((far - 1414.0).abs() < 25.0, "the opposite corner is about 1.41 km away, got {far}");
    }

    #[test]
    fn nothing_is_measured_for_a_shape_that_is_not_one_yet() {
        let two = Shape::Polygon { vertices: vec![Point::new(0.0, 0.0), Point::new(0.0, 0.01)] };
        assert_eq!((two.area_m2(), two.perimeter_m()), (0.0, 0.0));
    }

    #[test]
    fn a_realm_remembers_the_icon_picked_for_it() {
        let store = RealmStore::new(tmp("icon"));
        let mut r = realm("a");
        assert_eq!(r.icon, None, "no icon until one is picked");
        r.icon = Some("trees".into());
        store.save(&r).unwrap();
        assert_eq!(store.get("a").unwrap().icon.as_deref(), Some("trees"));
    }

    #[test]
    fn marks_persist_per_realm_survive_a_new_atlas_and_go_with_the_realm() {
        use crate::marks::Mark;
        let store = RealmStore::new(tmp("marks"));
        store.save(&realm("a")).unwrap();
        assert_eq!(store.marks("a"), crate::marks::Marks::default());
        let mut m = store.marks("a");
        m.set("n1", Mark::Favorite);
        m.set("n2", Mark::Banned);
        store.save_marks("a", &m).unwrap();
        let back = store.marks("a");
        assert_eq!((back.get("n1"), back.get("n2"), back.get("n3")), (Mark::Favorite, Mark::Banned, Mark::None));
        assert_eq!(store.marks("b"), crate::marks::Marks::default(), "another realm is untouched");
        store.delete("a").unwrap();
        assert_eq!(store.marks("a"), crate::marks::Marks::default(), "deleting a realm deletes its marks");
    }

    #[test]
    fn save_list_replace_delete_round_trip() {
        let store = RealmStore::new(tmp("rt"));
        assert!(store.list().is_empty());
        store.save(&realm("a")).unwrap();
        store.save(&realm("b")).unwrap();
        let mut a2 = realm("a");
        a2.name = "Renamed".into();
        store.save(&a2).unwrap();
        let all = store.list();
        assert_eq!(all.len(), 2);
        assert_eq!(store.get("a").unwrap().name, "Renamed");
        store.delete("a").unwrap();
        assert_eq!(store.list().len(), 1);
    }

    #[test]
    fn invalid_shapes_are_rejected() {
        let store = RealmStore::new(tmp("bad"));
        let mut r = realm("x");
        r.shape = Shape::Polygon { vertices: vec![Point::new(0.0, 0.0), Point::new(1.0, 1.0)] };
        assert!(store.save(&r).is_err());
        r.shape = Shape::Circle { center: Point::new(0.0, 0.0), radius_m: 10.0 };
        assert!(store.save(&r).is_err());
    }

    #[test]
    fn home_and_corrupt_files_are_handled() {
        let store = RealmStore::new(tmp("home"));
        assert!(store.home().is_none());
        store.set_home(Point::new(1.5, 2.5)).unwrap();
        assert_eq!(store.home(), Some(Point::new(1.5, 2.5)));
        std::fs::write(store.dir().join("realms.json"), "not json").unwrap();
        assert!(store.list().is_empty(), "corrupt file must not crash");
    }

    #[test]
    fn proximity_is_inside_near_or_far_with_a_300_m_buffer() {
        let c = Point::new(40.0, -111.0);
        let s = Shape::Circle { center: c, radius_m: 500.0 };
        assert_eq!(s.proximity(crate::geo::destination(c, 0.0, 100.0)), Proximity::Inside);
        assert_eq!(s.proximity(crate::geo::destination(c, 0.0, 700.0)), Proximity::Near);
        assert_eq!(s.proximity(crate::geo::destination(c, 0.0, 900.0)), Proximity::Far);
    }

    #[test]
    fn closest_proximity_takes_the_best_across_shapes() {
        let c = Point::new(40.0, -111.0);
        let at = |m| crate::geo::destination(c, 0.0, m);
        let circle = |center| Shape::Circle { center, radius_m: 500.0 };
        let near_c = crate::geo::destination(c, 0.0, 1500.0);
        // From `at(700)`: `circle(c)` is Near, `circle(near_c)` is Far.
        assert_eq!(closest_proximity(&[circle(near_c), circle(c)], at(700.0)), Some(Proximity::Near));
        // From `at(100)`: `circle(c)` is Inside, the other is Far, in either order.
        assert_eq!(closest_proximity(&[circle(near_c), circle(c)], at(100.0)), Some(Proximity::Inside));
        assert_eq!(closest_proximity(&[circle(c), circle(near_c)], at(100.0)), Some(Proximity::Inside));
        assert_eq!(closest_proximity(&[], at(100.0)), None);
    }

    #[test]
    fn the_near_buffer_ends_at_300_m_from_the_edge() {
        let c = Point::new(40.0, -111.0);
        let s = Shape::Circle { center: c, radius_m: 500.0 };
        assert_eq!(s.proximity(crate::geo::destination(c, 0.0, 799.0)), Proximity::Near);
        assert_eq!(s.proximity(crate::geo::destination(c, 0.0, 801.0)), Proximity::Far);
    }

    #[test]
    fn distance_to_a_circle_is_zero_inside_and_the_gap_outside() {
        let c = Point::new(40.0, -111.0);
        let s = Shape::Circle { center: c, radius_m: 500.0 };
        assert_eq!(s.distance_m(crate::geo::destination(c, 90.0, 300.0)), 0.0);
        let d = s.distance_m(crate::geo::destination(c, 90.0, 800.0));
        assert!((d - 300.0).abs() < 2.0, "got {d}");
    }

    #[test]
    fn distance_to_a_polygon_is_zero_inside_and_measured_to_the_nearest_edge_outside() {
        let a = Point::new(40.0, -111.0);
        let b = crate::geo::destination(a, 90.0, 1000.0);
        let c = crate::geo::destination(b, 0.0, 1000.0);
        let d = crate::geo::destination(a, 0.0, 1000.0);
        let s = Shape::Polygon { vertices: vec![a, b, c, d] };
        let inside = crate::geo::destination(crate::geo::destination(a, 90.0, 500.0), 0.0, 500.0);
        assert_eq!(s.distance_m(inside), 0.0);
        let east = crate::geo::destination(crate::geo::destination(a, 90.0, 1300.0), 0.0, 500.0);
        assert!((s.distance_m(east) - 300.0).abs() < 3.0);
        let corner = crate::geo::destination(crate::geo::destination(b, 90.0, 300.0), 0.0, 400.0);
        assert!((s.distance_m(corner) - 300.0).abs() < 3.0, "nearest edge is the east side");
        let beyond = crate::geo::destination(crate::geo::destination(b, 90.0, 300.0), 180.0, 400.0);
        assert!((s.distance_m(beyond) - 500.0).abs() < 3.0, "nearest feature is the corner vertex");
    }
}
