//! `slot_data` v2: what the apworld (or the solo generator) says about a game. Mirrors `apworld/docs/slot_data.schema.json`.

use serde::{Deserialize, Serialize};

use crate::catalog::Mode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ZoneSlot {
    pub id: u32,
    pub mode: Mode,
    pub zone_keys_needed: u32,
    pub tool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestSlot {
    pub location_id: i64,
    pub zone: u32,
    pub mode: Mode,
    pub difficulty: String,
    pub effort_tier: u8,
    #[serde(rename = "type")]
    pub family: String,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// One win condition and its parameter (0 means the goal's own default).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoalSpec {
    pub id: String,
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
    if mode == GoalMode::AtLeast && !(1..=goals.len() as u32).contains(&need) {
        return Err(format!("the goals need to be at least {need} of {}: pick a number from 1 to {}", goals.len(), goals.len()));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SlotData {
    pub schema_version: u32,
    /// Schema 2 has one goal. Schema 3 has `goals` (and these two still hold the first one).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub goal: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub goal_target: u32,
    #[serde(default)]
    pub goals: Vec<GoalSpec>,
    #[serde(default, rename = "goal_requirement")]
    pub goal_mode: GoalMode,
    #[serde(default)]
    pub goal_need: u32,
    pub minutes_per_tier: u32,
    pub reduction_percent: u32,
    pub min_distance_m: u32,
    pub fog_of_war: bool,
    pub return_home: bool,
    pub death_link: bool,
    pub enabled_traps: Vec<String>,
    pub zones: Vec<ZoneSlot>,
    pub trips: Vec<QuestSlot>,
    pub boss: Option<QuestSlot>,
}

/// The `slot_data` version this app writes, and the ones it can read.
pub const CURRENT_SCHEMA: u32 = 3;
pub const SUPPORTED_SCHEMAS: [u32; 2] = [2, 3];

impl SlotData {
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

    pub fn check_goals(&self) -> Result<(), String> {
        check_goal_specs(&self.goal_list(), self.goal_mode, self.goal_need)
    }

    /// All quests including the boss (if any), in a stable order.
    #[must_use]
    pub fn all_quests(&self) -> Vec<&QuestSlot> {
        self.trips.iter().chain(self.boss.iter()).collect()
    }

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
