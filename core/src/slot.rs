//! slot_data v2: what the apworld (or the solo generator) says about a game. Mirrors apworld/docs/slot_data.schema.json.

use serde::{Deserialize, Serialize};

use crate::catalog::Mode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ZoneSlot {
    pub id: u32,
    pub mode: Mode,
    pub zone_keys_needed: u32,
    pub tool: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuestSlot {
    pub location_id: i64,
    pub zone: u32,
    pub mode: Mode,
    pub difficulty: String,
    pub effort_tier: u8,
    #[serde(rename = "type")]
    pub family: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SlotData {
    pub schema_version: u32,
    pub goal: String,
    pub goal_target: u32,
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

pub const SUPPORTED_SCHEMA: u32 = 2;

impl SlotData {
    pub fn from_json(s: &str) -> Result<SlotData, String> {
        let d: SlotData = serde_json::from_str(s).map_err(|e| format!("bad slot_data: {e}"))?;
        if d.schema_version != SUPPORTED_SCHEMA {
            return Err(format!("unsupported slot_data schema {} (this app understands {SUPPORTED_SCHEMA})", d.schema_version));
        }
        if d.zones.is_empty() {
            return Err("slot_data has no zones".into());
        }
        Ok(d)
    }

    /// All quests including the boss (if any), in a stable order.
    pub fn all_quests(&self) -> Vec<&QuestSlot> {
        self.trips.iter().chain(self.boss.iter()).collect()
    }

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
        assert_eq!(d.schema_version, 2);
        assert_eq!(d.zones.len(), 3);
        assert_eq!(d.zones[1].tool.as_deref(), Some("Bike"));
        assert_eq!(d.trips.len(), 60);
        assert!(d.boss.is_some());
        assert_eq!(d.all_quests().len(), 61);
        assert!(d.trips.iter().all(|t| (1..=10).contains(&t.effort_tier) && d.zone(t.zone).is_some()));
    }

    #[test]
    fn unknown_schema_and_garbage_are_refused() {
        assert!(SlotData::from_json("{}").is_err());
        let bumped = SAMPLE.replace("\"schema_version\": 2", "\"schema_version\": 3");
        assert!(SlotData::from_json(&bumped).unwrap_err().contains("unsupported"));
    }
}
