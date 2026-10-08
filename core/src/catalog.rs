//! The quest catalog: which kinds of quests the map data can support (data-driven, see `data/quest_catalog.json`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How the player travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// On foot.
    Walk,
    /// Running.
    Run,
    /// By bike.
    Bike,
    /// By car.
    Drive,
}

impl Mode {
    /// Every mode.
    pub const ALL: [Self; 4] = [Self::Walk, Self::Run, Self::Bike, Self::Drive];

    /// The modes a game can use for now. Car is left out until it is supported.
    pub const PLAY: [Self; 3] = [Self::Walk, Self::Run, Self::Bike];

    /// The lowercase name of the mode, as used in settings and files.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Walk => "walk",
            Self::Run => "run",
            Self::Bike => "bike",
            Self::Drive => "drive",
        }
    }

    /// The mode called `s` (`car` also means drive), if any.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s || (s == "car" && *m == Self::Drive))
    }

    /// Typical active speed in meters per minute.
    #[must_use]
    pub fn m_per_min(self) -> f64 {
        match self {
            Self::Walk => 75.0,
            Self::Run => 150.0,
            Self::Bike => 250.0,
            Self::Drive => 583.0,
        }
    }
}

/// A condition on one map tag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cond {
    /// The OSM tag key to look at.
    pub key: String,
    /// `"*"` matches any value; OSM `a;b` multi-values match if any part matches.
    pub values: Vec<String>,
}

impl Cond {
    /// Whether the tag set satisfies the condition.
    #[must_use]
    pub fn holds(&self, tags: &BTreeMap<String, String>) -> bool {
        let Some(v) = tags.get(&self.key) else { return false };
        self.values.iter().any(|want| want == "*" || v.split(';').any(|part| part.trim() == want))
    }
}

/// The shape of the map feature a kind uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Geom {
    /// A single point.
    Point,
    /// An area.
    Area,
    /// A line or path.
    Line,
    /// No map feature (anywhere works).
    None,
}

/// How a quest kind is proven complete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Verify {
    /// Get within `radius_m` of a place.
    Reach {
        /// How close counts as reached, in metres.
        radius_m: f64,
    },
    /// Stay near a place for some time.
    Dwell {
        /// How long to stay, in minutes.
        minutes: f64,
        /// How close counts as there, in metres.
        radius_m: f64,
    },
    /// Spend time inside an area.
    DwellInArea {
        /// How long to stay, in minutes.
        minutes: f64,
    },
    /// Follow a path.
    FollowLine {
        /// How far off the path still counts, in metres.
        corridor_m: f64,
        /// Share of the path to cover, 0 to 1.
        coverage: f64,
        /// Shortest path allowed, in metres.
        min_len_m: f64,
        /// Longest path allowed, in metres.
        max_len_m: f64,
    },
    /// Carry something between places.
    Courier {
        /// Number of legs.
        legs: u32,
    },
    /// Go to a far point and come back home.
    RoundTrip,
    /// Visit new map cells.
    CoverCells {
        /// Number of cells.
        cells: u32,
        /// Edge length of a cell, in metres.
        cell_m: f64,
    },
    /// Take a number of steps.
    Steps {
        /// Number of steps.
        steps: u32,
    },
    /// Spend time far from home.
    Away {
        /// Minimum distance from home, in metres.
        min_distance_m: f64,
        /// How long to stay away, in minutes.
        minutes: f64,
    },
    /// The biggest quest of the realm.
    Boss,
}

fn metres(m: f64) -> String {
    if m < 1000.0 {
        format!("{} m", m.round())
    } else {
        format!("{} km", (m / 100.0).round() / 10.0)
    }
}

impl Verify {
    /// What the player has to do, in one plain sentence.
    #[must_use]
    pub fn how(&self) -> String {
        match self {
            Self::Reach { radius_m } => format!("Get within {}.", metres(*radius_m)),
            Self::Dwell { minutes, radius_m } => format!("Stay within {} for {} min.", metres(*radius_m), minutes.round()),
            Self::DwellInArea { minutes } => format!("Spend {} min inside it.", minutes.round()),
            Self::FollowLine { coverage, .. } => format!("Walk {}% of its length.", (coverage * 100.0).round()),
            Self::Courier { .. } => "Pick something up at one spot and deliver it to another.".to_string(),
            Self::RoundTrip => "Go out to a spot, then come back home (no time limit).".to_string(),
            Self::CoverCells { cells, .. } => format!("Visit {cells} new map cells."),
            Self::Steps { steps } => format!("Take {steps} steps."),
            Self::Away { min_distance_m, minutes } => format!("Get {} from home and stay {} min.", metres(*min_distance_m), minutes.round()),
            Self::Boss => "The biggest quest of the realm.".to_string(),
        }
    }
}

/// A kind of quest: which places it matches, how it is proven and in which modes it can be played.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // mirrors the catalog JSON schema, where each flag is an independent switch
pub struct Kind {
    /// Stable id of the kind.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Family the kind belongs to.
    pub family: String,
    /// Short description shown to the player.
    pub blurb: String,
    /// What sort of map feature the kind uses.
    pub geom: Geom,
    /// Groups of conditions; a place matches when every condition of any one group holds.
    #[serde(default)]
    pub any_of: Vec<Vec<Cond>>,
    /// Conditions that rule a place out.
    #[serde(default)]
    pub none_of: Vec<Cond>,
    /// Whether a place needs a name to match.
    #[serde(default)]
    pub require_name: bool,
    /// How the quest is proven complete.
    pub verify: Verify,
    /// Modes the kind can be played in.
    pub modes: Vec<Mode>,
    /// How many matching places a realm needs before the kind is offered.
    #[serde(default)]
    pub min_features: u32,
    /// Whether a reach quest picks its point in one compass direction from home (a 90-degree sector).
    #[serde(default)]
    pub sector: bool,
    /// Whether the area is shown as an outline on the map.
    #[serde(default)]
    pub outline: bool,
    /// Whether a line must form a closed loop.
    #[serde(default)]
    pub closed: bool,
}

impl Kind {
    /// The `key=value` map tags this kind looked at on a place, so the player can see why it matched.
    #[must_use]
    pub fn evidence(&self, tags: &BTreeMap<String, String>) -> Vec<String> {
        let mut keys: Vec<&str> = self.any_of.iter().flatten().map(|c| c.key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        keys.into_iter().filter_map(|k| tags.get(k).map(|v| format!("{k}={v}"))).collect()
    }

    /// Does an OSM feature with these tags satisfy this kind's filters?
    #[must_use]
    pub fn matches(&self, tags: &BTreeMap<String, String>) -> bool {
        if self.any_of.is_empty() || (self.require_name && !tags.contains_key("name")) {
            return false;
        }
        if self.none_of.iter().any(|c| c.holds(tags)) {
            return false;
        }
        self.any_of.iter().any(|group| group.iter().all(|c| c.holds(tags)))
    }

    /// Whether the kind can be played in `mode`.
    #[must_use]
    pub fn allows(&self, mode: Mode) -> bool {
        self.modes.contains(&mode)
    }
}

/// The quest catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    /// Catalog format version.
    pub version: u32,
    /// Names of the quest families.
    pub families: Vec<String>,
    /// Every quest kind.
    pub kinds: Vec<Kind>,
}

impl Catalog {
    /// The quest catalog embedded in the binary.
    ///
    /// # Panics
    /// Panics if the embedded `quest_catalog.json` is malformed; a unit test guards against shipping that.
    #[must_use]
    #[allow(clippy::expect_used)] // the embedded JSON is validated by tests
    pub fn builtin() -> Self {
        serde_json::from_str(include_str!("../data/quest_catalog.json")).expect("quest_catalog.json is valid")
    }

    /// The kind with this id, if any.
    #[must_use]
    pub fn kind(&self, id: &str) -> Option<&Kind> {
        self.kinds.iter().find(|k| k.id == id)
    }
}

#[cfg(test)]
#[allow(clippy::assert_is_empty)] // test code: `is_empty()` reads better in assertions than comparing with a typed empty array
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn verify_explains_how_to_complete_a_quest_in_plain_words() {
        assert_eq!(Verify::Reach { radius_m: 40.0 }.how(), "Get within 40 m.");
        assert_eq!(Verify::Dwell { minutes: 5.0, radius_m: 30.0 }.how(), "Stay within 30 m for 5 min.");
        assert_eq!(Verify::FollowLine { corridor_m: 25.0, coverage: 0.6, min_len_m: 300.0, max_len_m: 5000.0 }.how(), "Walk 60% of its length.");
        assert_eq!(Verify::CoverCells { cells: 12, cell_m: 150.0 }.how(), "Visit 12 new map cells.");
        assert_eq!(Verify::Away { min_distance_m: 1500.0, minutes: 30.0 }.how(), "Get 1.5 km from home and stay 30 min.");
    }

    #[test]
    fn evidence_lists_only_the_map_tags_a_kind_looked_at() {
        let c = Catalog::builtin();
        let pitch = tags(&[("leisure", "pitch"), ("sport", "tennis"), ("surface", "asphalt")]);
        assert_eq!(c.kind("court_jester").unwrap().evidence(&pitch), ["leisure=pitch"]);
        assert_eq!(c.kind("love_all").unwrap().evidence(&pitch), ["leisure=pitch", "sport=tennis"]);
        assert!(c.kind("love_all").unwrap().evidence(&tags(&[("amenity", "bench")])).is_empty());
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
