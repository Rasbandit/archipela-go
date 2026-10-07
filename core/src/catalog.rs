//! The quest catalog: which kinds of quests the map data can support (data-driven, see data/quest_catalog.json).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Walk,
    Run,
    Bike,
    Drive,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::Walk, Mode::Run, Mode::Bike, Mode::Drive];

    pub fn name(self) -> &'static str {
        match self {
            Mode::Walk => "walk",
            Mode::Run => "run",
            Mode::Bike => "bike",
            Mode::Drive => "drive",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.name() == s || (s == "car" && *m == Mode::Drive))
    }

    /// Typical active speed in meters per minute.
    pub fn m_per_min(self) -> f64 {
        match self {
            Mode::Walk => 75.0,
            Mode::Run => 150.0,
            Mode::Bike => 250.0,
            Mode::Drive => 583.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cond {
    pub key: String,
    /// `"*"` matches any value; OSM `a;b` multi-values match if any part matches.
    pub values: Vec<String>,
}

impl Cond {
    pub fn holds(&self, tags: &BTreeMap<String, String>) -> bool {
        let Some(v) = tags.get(&self.key) else { return false };
        self.values.iter().any(|want| want == "*" || v.split(';').any(|part| part.trim() == want))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Geom {
    Point,
    Area,
    Line,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Verify {
    Reach { radius_m: f64 },
    Dwell { minutes: f64, radius_m: f64 },
    DwellInArea { minutes: f64 },
    FollowLine { corridor_m: f64, coverage: f64, min_len_m: f64, max_len_m: f64 },
    Courier { legs: u32 },
    RoundTrip,
    CoverCells { cells: u32, cell_m: f64 },
    Steps { steps: u32 },
    Away { min_distance_m: f64, minutes: f64 },
    Boss,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kind {
    pub id: String,
    pub name: String,
    pub family: String,
    pub blurb: String,
    pub geom: Geom,
    #[serde(default)]
    pub any_of: Vec<Vec<Cond>>,
    #[serde(default)]
    pub none_of: Vec<Cond>,
    #[serde(default)]
    pub require_name: bool,
    pub verify: Verify,
    pub modes: Vec<Mode>,
    #[serde(default)]
    pub min_features: u32,
    #[serde(default)]
    pub sector: bool,
    #[serde(default)]
    pub outline: bool,
    #[serde(default)]
    pub closed: bool,
}

impl Kind {
    /// Does an OSM feature with these tags satisfy this kind's filters?
    pub fn matches(&self, tags: &BTreeMap<String, String>) -> bool {
        if self.any_of.is_empty() || (self.require_name && !tags.contains_key("name")) {
            return false;
        }
        if self.none_of.iter().any(|c| c.holds(tags)) {
            return false;
        }
        self.any_of.iter().any(|group| group.iter().all(|c| c.holds(tags)))
    }

    pub fn allows(&self, mode: Mode) -> bool {
        self.modes.contains(&mode)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub version: u32,
    pub families: Vec<String>,
    pub kinds: Vec<Kind>,
}

impl Catalog {
    pub fn builtin() -> Catalog {
        serde_json::from_str(include_str!("../data/quest_catalog.json")).expect("quest_catalog.json is valid")
    }

    pub fn kind(&self, id: &str) -> Option<&Kind> {
        self.kinds.iter().find(|k| k.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn builtin_catalog_parses_with_unique_ids_and_known_families() {
        let c = Catalog::builtin();
        assert!(c.kinds.len() >= 70);
        let mut ids: Vec<_> = c.kinds.iter().map(|k| &k.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.kinds.len());
        for k in &c.kinds {
            assert!(c.families.contains(&k.family), "{} has unknown family {}", k.id, k.family);
            assert!(!k.modes.is_empty());
        }
    }

    #[test]
    fn matching_handles_and_or_none_of_and_names() {
        let c = Catalog::builtin();
        let mural = c.kind("mural_mural").unwrap();
        assert!(mural.matches(&tags(&[("tourism", "artwork"), ("artwork_type", "mural")])));
        assert!(!mural.matches(&tags(&[("tourism", "artwork")])));
        let art = c.kind("gallery_walls").unwrap();
        assert!(art.matches(&tags(&[("tourism", "artwork")])));
        let tree = c.kind("tree_hugger").unwrap();
        assert!(tree.matches(&tags(&[("natural", "tree"), ("denotation", "landmark")])));
        assert!(!tree.matches(&tags(&[("natural", "tree")])));
        let garden = c.kind("garden_party").unwrap();
        assert!(!garden.matches(&tags(&[("leisure", "garden"), ("name", "Back Yard"), ("access", "private")])));
        assert!(garden.matches(&tags(&[("leisure", "garden"), ("name", "Rose Garden")])));
        let peak = c.kind("summit_fever").unwrap();
        assert!(!peak.matches(&tags(&[("natural", "peak")])), "needs a name");
        let trail = c.kind("trail_boss").unwrap();
        assert!(!trail.matches(&tags(&[("highway", "footway"), ("footway", "sidewalk"), ("name", "Main St")])));
        assert!(trail.matches(&tags(&[("highway", "path"), ("name", "Ridge Trail")])));
        let religion = c.kind("steeple_chase").unwrap();
        assert!(religion.matches(&tags(&[("amenity", "place_of_worship"), ("name", "St. Mary")])));
    }

    #[test]
    fn multi_values_and_modes() {
        let c = Catalog::builtin();
        let memorial = c.kind("remember_when").unwrap();
        assert!(memorial.matches(&tags(&[("historic", "ruins;memorial")])));
        assert_eq!(Mode::parse("car"), Some(Mode::Drive));
        assert!(c.kind("trail_boss").unwrap().allows(Mode::Walk));
        assert!(!c.kind("trail_boss").unwrap().allows(Mode::Drive));
    }
}
