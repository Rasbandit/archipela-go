//! `slot_data` v2: what the apworld (or the solo generator) says about a game. Mirrors `apworld/docs/slot_data.schema.json`.

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

#[allow(clippy::trivially_copy_pass_by_ref)] // serde `skip_serializing_if` passes a reference
fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// One win condition and its parameter (0 means the goal's own default).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoalSpec {
    /// Id of the win condition.
    pub id: String,
    /// Parameter of the goal.
    #[serde(default)]
    pub target: u32,
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
pub fn check_goal_specs(goals: &[GoalSpec], mode: GoalMode, need: u32) -> Result<(), String> {
    if goals.is_empty() {
        return Err("a game needs at least one goal".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    for g in goals {
        if !crate::solo::GOALS.contains(&g.id.as_str()) {
            return Err(format!("unknown goal {}", g.id));
        }
        if !seen.insert(g.id.as_str()) {
            return Err(format!("the goal {} is listed twice", g.id));
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
    /// Schema 2 has one goal. Schema 3 has `goals` (and these two still hold the first one).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub goal: String,
    /// Parameter of the single goal (schema 2).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub goal_target: u32,
    /// The win conditions (schema 3).
    #[serde(default)]
    pub goals: Vec<GoalSpec>,
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

/// The `slot_data` version this app writes, and the ones it can read.
pub const CURRENT_SCHEMA: u32 = 3;
/// The `slot_data` versions this app can read.
pub const SUPPORTED_SCHEMAS: [u32; 2] = [2, 3];

impl SlotData {
    /// Parse and validate `slot_data` JSON.
    ///
    /// # Errors
    /// Returns a message if the JSON is malformed, the schema version is unsupported, there are no zones, or the goals cannot be played.
    pub fn from_json(s: &str) -> Result<Self, String> {
        let d: Self = serde_json::from_str(s).map_err(|e| format!("bad slot_data: {e}"))?;
        if !SUPPORTED_SCHEMAS.contains(&d.schema_version) {
            return Err(format!("unsupported slot_data schema {} (this app understands {SUPPORTED_SCHEMAS:?})", d.schema_version));
        }
        if d.zones.is_empty() {
            return Err("slot_data has no zones".into());
        }
        d.check_goals()?;
        Ok(d)
    }

    /// The win conditions: the `goals` list, or for older (schema 2) data the single goal.
    #[must_use]
    pub fn goal_list(&self) -> Vec<GoalSpec> {
        if self.goals.is_empty() && !self.goal.is_empty() {
            vec![GoalSpec { id: self.goal.clone(), target: self.goal_target }]
        } else {
            self.goals.clone()
        }
    }

    /// Check that the game's goals can be played.
    ///
    /// # Errors
    /// Returns a message under the same rules as [`check_goal_specs`].
    pub fn check_goals(&self) -> Result<(), String> {
        check_goal_specs(&self.goal_list(), self.goal_mode, self.goal_need)
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
    fn real_apworld_sample_parses() {
        let d = SlotData::from_json(SAMPLE).unwrap();
        assert_eq!(d.schema_version, 3);
        assert_eq!(d.goal_list(), vec![GoalSpec { id: "boss".into(), target: 0 }]);
        assert_eq!((d.goal_mode, d.goal_need), (GoalMode::Any, 1));
        assert_eq!(d.zones.len(), 3);
        assert_eq!(d.zones[1].tool.as_deref(), Some("Bike"));
        assert_eq!(d.trips.len(), 60);
        assert!(d.boss.is_some());
        assert_eq!(d.all_quests().len(), 61);
        assert!(d.trips.iter().all(|t| (1..=10).contains(&t.effort_tier) && d.zone(t.zone).is_some()));
    }

    #[test]
    fn schema_2_single_goal_data_is_still_understood() {
        let v2 = SAMPLE
            .replace("\"schema_version\": 3", "\"schema_version\": 2")
            .replace("\"goals\": [", "\"goal\": \"boss\", \"goal_target\": 0, \"was_goals\": [");
        let d = SlotData::from_json(&v2).unwrap();
        assert_eq!(d.goal_list(), vec![GoalSpec { id: "boss".into(), target: 0 }]);
    }

    #[test]
    fn unknown_schema_and_garbage_are_refused() {
        assert!(SlotData::from_json("{}").is_err());
        let bumped = SAMPLE.replace("\"schema_version\": 3", "\"schema_version\": 99");
        assert!(SlotData::from_json(&bumped).unwrap_err().contains("unsupported"));
    }
}
