//! Realms: saved, user-drawn geofences with a mode tag. Persisted as JSON in the app's files dir.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::geo::{centroid, Point};
use crate::marks::Marks;
use crate::scan::Atlas;
use crate::zone::Zone;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    Circle { center: Point, radius_m: f64 },
    Polygon { vertices: Vec<Point> },
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
}
