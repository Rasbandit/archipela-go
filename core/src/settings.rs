//! Player settings that are not tied to one game, saved as `settings.json` in the engine directory.

use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE: &str = "settings.json";

/// Countries that measure road distances in miles.
const IMPERIAL_COUNTRIES: [&str; 4] = ["US", "GB", "LR", "MM"];

/// The units the player picked for distances; `Auto` follows the phone's region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitChoice {
    /// Miles in the US, UK and a few others, kilometres elsewhere.
    #[default]
    Auto,
    /// Metres and kilometres.
    Metric,
    /// Feet and miles.
    Imperial,
}

/// The units distances are shown in once `Auto` is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitSystem {
    /// Metres and kilometres.
    Metric,
    /// Feet and miles.
    Imperial,
}

/// Every setting; a missing field reads as its default, so older files keep loading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Distance units.
    pub units: UnitChoice,
}

impl Settings {
    /// The settings saved in `dir`, or the defaults when there is no file or it cannot be read.
    #[must_use]
    pub fn load(dir: &Path) -> Self {
        std::fs::read_to_string(dir.join(FILE)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    /// Save to `dir`, creating it if needed.
    ///
    /// # Errors
    /// Returns an error if the file cannot be written.
    pub fn save(&self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let body = serde_json::to_string(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(FILE), body).map_err(|e| e.to_string())
    }
}

/// The units to show for `choice` on a phone set to `country` (an ISO 3166 code such as "US").
#[must_use]
pub fn resolve_units(choice: UnitChoice, country: &str) -> UnitSystem {
    match choice {
        UnitChoice::Imperial => UnitSystem::Imperial,
        UnitChoice::Auto if IMPERIAL_COUNTRIES.iter().any(|c| c.eq_ignore_ascii_case(country)) => UnitSystem::Imperial,
        UnitChoice::Metric | UnitChoice::Auto => UnitSystem::Metric,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("apgo-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn missing_file_loads_defaults() {
        assert_eq!(Settings::load(&tmp("missing")), Settings { units: UnitChoice::Auto });
    }

    #[test]
    fn save_then_load_round_trips_and_creates_the_dir() {
        let dir = tmp("round-trip").join("nested");
        let s = Settings { units: UnitChoice::Imperial };
        s.save(&dir).unwrap();
        assert_eq!(Settings::load(&dir), s);
    }

    #[test]
    fn corrupt_or_unknown_file_loads_defaults() {
        let dir = tmp("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), "not json").unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
        std::fs::write(dir.join(FILE), r#"{"units":"nautical"}"#).unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
    }

    #[test]
    fn missing_field_reads_as_default() {
        let dir = tmp("old-file");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), "{}").unwrap();
        assert_eq!(Settings::load(&dir), Settings::default());
    }

    #[test]
    fn save_fails_when_the_dir_is_a_file() {
        let dir = tmp("blocked");
        std::fs::write(&dir, "a file").unwrap();
        assert!(Settings { units: UnitChoice::Metric }.save(&dir).is_err());
        let _ = std::fs::remove_file(&dir);
    }

    #[test]
    fn auto_follows_the_country() {
        for c in ["US", "GB", "LR", "MM", "us"] {
            assert_eq!(resolve_units(UnitChoice::Auto, c), UnitSystem::Imperial, "{c}");
        }
        for c in ["DE", "CA", "AU", ""] {
            assert_eq!(resolve_units(UnitChoice::Auto, c), UnitSystem::Metric, "{c}");
        }
    }

    #[test]
    fn a_forced_choice_ignores_the_country() {
        assert_eq!(resolve_units(UnitChoice::Metric, "US"), UnitSystem::Metric);
        assert_eq!(resolve_units(UnitChoice::Imperial, "DE"), UnitSystem::Imperial);
    }
}
