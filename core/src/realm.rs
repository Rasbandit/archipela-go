//! Realms: saved, user-drawn geofences with a mode tag. Persisted as JSON in the app's files dir.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::catalog::Mode;
use crate::geo::{centroid, Point};
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
    pub mode: Mode,
    pub shape: Shape,
    #[serde(default)]
    pub scanned_at_ms: Option<u64>,
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
            mode: Mode::Walk,
            shape: Shape::Circle { center: Point::new(40.0, -111.0), radius_m: 1500.0 },
            scanned_at_ms: None,
        }
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
