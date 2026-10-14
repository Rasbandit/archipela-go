//! `slot_data` v4: what the apworld (or the solo generator) says about a game. Mirrors `apworld/docs/slot_data.schema.json`.

use serde::{Deserialize, Serialize};

use crate::catalog::Mode;

/// A zone of the game: its travel mode and what unlocks it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZoneSlot {
    /// Zone number, starting at 1.
    pub id: u32,
    /// How the player travels in this zone.
    pub mode: Mode,
    /// Zone keys needed before the zone opens.
    pub zone_keys_needed: u32,
    /// Name of the tool item that must be held, if the mode needs one.
    pub tool: Option<String>,
}

/// A quest location in the game.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestSlot {
    /// Archipelago location id of the check.
    pub location_id: i64,
    /// Zone number the quest is in.
    pub zone: u32,
    /// How the player travels there.
    pub mode: Mode,
    /// Difficulty band: easy, medium or hard.
    pub difficulty: String,
    /// Effort tier, starting at 1.
    pub effort_tier: u8,
    /// Quest family wanted for the slot.
    #[serde(rename = "type")]
    pub family: String,
}

/// How several goals combine into winning.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoalMode {
    /// The first goal finished wins.
    Any,
    /// Every goal must be finished.
    #[default]
    All,
    /// `goal_need` of the goals must be finished.
    AtLeast,
}

/// Whether a set of goals can be played: known ids, no repeats, and a sensible count for `AtLeast`.
///
/// # Errors
/// Returns a message if there are no goals, a goal id is unknown or repeated, or `need` is out of range for [`GoalMode::AtLeast`].
pub fn check_goal_ids(goals: &[String], mode: GoalMode, need: u32) -> Result<(), String> {
    if goals.is_empty() {
        return Err("a game needs at least one goal".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for g in goals {
        if !crate::solo::GOALS.contains(&g.as_str()) {
            return Err(format!("unknown goal {g}"));
        }
        if !seen.insert(g.as_str()) {
            return Err(format!("the goal {g} is listed twice"));
        }
    }
    if mode == GoalMode::AtLeast && !(1..=crate::num::count_u32(goals.len())).contains(&need) {
        return Err(format!("the goals need to be at least {need} of {}: pick a number from 1 to {}", goals.len(), goals.len()));
    }
    Ok(())
}

/// The `slot_data` the game was generated with.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SlotData {
    /// Version of the `slot_data` format.
    pub schema_version: u32,
    /// The win conditions, as goal ids.
    pub goals: Vec<String>,
    /// How the goals combine into winning.
    #[serde(default, rename = "goal_requirement")]
    pub goal_mode: GoalMode,
    /// For [`GoalMode::AtLeast`], how many goals must be finished.
    #[serde(default)]
    pub goal_need: u32,
    /// Minutes of effort one tier covers.
    pub minutes_per_tier: u32,
    /// Percent of effort each reduction item removes.
    pub reduction_percent: u32,
    /// Quests must be at least this far from home, in metres.
    pub min_distance_m: u32,
    /// Whether quests stay hidden until the player is near.
    pub fog_of_war: bool,
    /// Whether the player must return home to finish a quest.
    pub return_home: bool,
    /// Whether death link is on.
    pub death_link: bool,
    /// Keys of the traps in the item pool.
    pub enabled_traps: Vec<String>,
    /// The zones of the game.
    pub zones: Vec<ZoneSlot>,
    /// The regular quest locations.
    pub trips: Vec<QuestSlot>,
    /// The boss quest location, if the game has one.
    pub boss: Option<QuestSlot>,
}

/// The `slot_data` version this app writes, and the only one it reads.
pub const CURRENT_SCHEMA: u32 = 4;
/// The `slot_data` versions this app can read.
pub const SUPPORTED_SCHEMAS: [u32; 1] = [4];

/// Check that this app reads `slot_data` schema `version`.
///
/// # Errors
/// Returns a message naming the version and the ones this app understands.
pub fn check_schema(version: u32) -> Result<(), String> {
    if SUPPORTED_SCHEMAS.contains(&version) {
        Ok(())
    } else {
        Err(format!("unsupported slot_data schema {version} (this app understands {SUPPORTED_SCHEMAS:?})"))
    }
}

impl SlotData {
    /// Parse and validate `slot_data` JSON.
    ///
    /// # Errors
    /// Returns a message if the JSON is malformed, the schema version is unsupported, there are no zones, or the goals cannot be played.
    pub fn from_json(s: &str) -> Result<Self, String> {
        /// Only the version, read first: an older seed's other fields may not parse as this schema.
        #[derive(Deserialize)]
        struct Head {
            schema_version: u32,
        }
        let head: Head = serde_json::from_str(s).map_err(|e| format!("bad slot_data: {e}"))?;
        check_schema(head.schema_version)?;
        let d: Self = serde_json::from_str(s).map_err(|e| format!("bad slot_data: {e}"))?;
        if d.zones.is_empty() {
            return Err("slot_data has no zones".into());
        }
        d.check_goals()?;
        Ok(d)
    }

    /// Check that the game's goals can be played.
    ///
    /// # Errors
    /// Returns a message under the same rules as [`check_goal_ids`].
    pub fn check_goals(&self) -> Result<(), String> {
        check_goal_ids(&self.goals, self.goal_mode, self.goal_need)
    }

    /// All quests including the boss (if any), in a stable order.
    #[must_use]
    pub fn all_quests(&self) -> Vec<&QuestSlot> {
        self.trips.iter().chain(self.boss.iter()).collect()
    }

    /// The zone with this id, if any.
    #[must_use]
    pub fn zone(&self, id: u32) -> Option<&ZoneSlot> {
        self.zones.iter().find(|z| z.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../apworld/docs/slot_data.sample.json");

    #[test]
    fn removed_goals_are_refused() {
        for id in ["macguffin_short", "macguffin_long", "boss", "zone_conqueror", "well_rounded", "quest_dex", "marathon", "explorer", "streak", "boss_rush"] {
            let goals = vec![id.to_string()];
            let err = check_goal_ids(&goals, GoalMode::Any, 1).unwrap_err();
            assert!(err.contains("unknown goal"), "{id}: {err}");
        }
        let both = vec!["treasure_hunt".to_string(), "all_trips".to_string()];
        assert!(check_goal_ids(&both, GoalMode::AtLeast, 2).is_ok());
        assert!(check_goal_ids(&both, GoalMode::AtLeast, 3).is_err());
        let twice = vec!["all_trips".to_string(), "all_trips".to_string()];
        assert!(check_goal_ids(&twice, GoalMode::Any, 1).unwrap_err().contains("twice"));
    }

    #[test]
    fn real_apworld_sample_parses() {
        let d = SlotData::from_json(SAMPLE).unwrap();
        assert_eq!(d.schema_version, 4);
        assert_eq!(d.goals, vec!["treasure_hunt".to_string()]);
        assert_eq!((d.goal_mode, d.goal_need), (GoalMode::Any, 1));
        assert_eq!(d.zones.len(), 3);
        assert_eq!(d.zones[1].tool.as_deref(), Some("Running Shoes"));
        assert_eq!(d.trips.len(), 60);
        assert!(d.boss.is_some());
        assert_eq!(d.all_quests().len(), 61);
        assert!(d.trips.iter().all(|t| (1..=10).contains(&t.effort_tier) && d.zone(t.zone).is_some()));
    }

    /// The sample with `key` set to `value`.
    fn sample_with(key: &str, value: serde_json::Value) -> String {
        let mut v: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        v[key] = value;
        v.to_string()
    }

    #[test]
    fn schema_3_is_refused_and_4_is_read() {
        let err = SlotData::from_json(&sample_with("schema_version", 3.into())).unwrap_err();
        assert!(err.contains("unsupported slot_data schema 3"), "{err}");
        assert_eq!(SUPPORTED_SCHEMAS, [4]);
        assert_eq!(SlotData::from_json(SAMPLE).unwrap().schema_version, 4);
    }

    #[test]
    fn a_real_schema_3_seed_is_refused_by_its_version_not_its_shape() {
        let mut v3: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        v3["schema_version"] = 3.into();
        v3["goals"] = serde_json::json!([{ "id": "treasure_hunt", "target": 0 }]);
        v3["zones"][1]["mode"] = "bike".into();
        v3["zones"][1]["tool"] = "Bike".into();
        let err = SlotData::from_json(&v3.to_string()).unwrap_err();
        assert_eq!(err, "unsupported slot_data schema 3 (this app understands [4])");
        assert!(SlotData::from_json("{\"zones\": []}").unwrap_err().starts_with("bad slot_data"), "no version at all is malformed");
    }

    #[test]
    fn goals_are_a_list_of_ids() {
        let objects = sample_with("goals", serde_json::json!([{ "id": "treasure_hunt", "target": 0 }]));
        assert!(SlotData::from_json(&objects).is_err(), "schema 4 goals are plain ids");
        let mut missing: serde_json::Value = serde_json::from_str(SAMPLE).unwrap();
        missing.as_object_mut().unwrap().remove("goals");
        assert!(SlotData::from_json(&missing.to_string()).is_err(), "schema 4 always has goals");
    }

    #[test]
    fn unknown_schema_and_garbage_are_refused() {
        assert!(SlotData::from_json("{}").is_err());
        let bumped = sample_with("schema_version", 99.into());
        assert!(SlotData::from_json(&bumped).unwrap_err().contains("unsupported"));
    }
}
