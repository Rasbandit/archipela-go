//! Player settings over FFI: the unit choice, the units it resolves to, and the one distance formatter.

use apgo_core::settings::{self as core, Settings};
use apgo_core::units;

use crate::engine::Engine;
use crate::CoreError;

/// The units the player picked for distances; `Auto` follows the phone's region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum UnitChoice {
    /// Miles in the US, UK and a few others, kilometres elsewhere.
    Auto,
    /// Metres and kilometres.
    Metric,
    /// Feet and miles.
    Imperial,
}

/// The units distances are shown in once `Auto` is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum UnitSystem {
    /// Metres and kilometres.
    Metric,
    /// Feet and miles.
    Imperial,
}

impl From<core::UnitChoice> for UnitChoice {
    fn from(c: core::UnitChoice) -> Self {
        match c {
            core::UnitChoice::Auto => Self::Auto,
            core::UnitChoice::Metric => Self::Metric,
            core::UnitChoice::Imperial => Self::Imperial,
        }
    }
}

impl From<UnitChoice> for core::UnitChoice {
    fn from(c: UnitChoice) -> Self {
        match c {
            UnitChoice::Auto => Self::Auto,
            UnitChoice::Metric => Self::Metric,
            UnitChoice::Imperial => Self::Imperial,
        }
    }
}

impl From<units::UnitSystem> for UnitSystem {
    fn from(u: units::UnitSystem) -> Self {
        match u {
            units::UnitSystem::Metric => Self::Metric,
            units::UnitSystem::Imperial => Self::Imperial,
        }
    }
}

impl From<UnitSystem> for units::UnitSystem {
    fn from(u: UnitSystem) -> Self {
        match u {
            UnitSystem::Metric => Self::Metric,
            UnitSystem::Imperial => Self::Imperial,
        }
    }
}

/// A distance in metres as text in `units` ("50 m", "1.4 km", "60 ft", "0.3 mi"): the app's one distance formatter.
#[uniffi::export]
#[must_use]
pub fn format_distance(m: f64, units: UnitSystem) -> String {
    units::distance(m, units.into())
}

/// An area in square metres as text in `units`: whole km² or mi², "<1 km²" for a small one.
#[uniffi::export]
#[must_use]
pub fn format_area(m2: f64, units: UnitSystem) -> String {
    units::area(m2, units.into())
}

#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // uniffi requires owned args
impl Engine {
    /// The saved unit choice (`Auto` until the player picks one).
    pub fn unit_choice(&self) -> UnitChoice {
        Settings::load(self.dir()).units.into()
    }

    /// Save the unit choice, keeping every other setting; the open game's text follows it from now on.
    ///
    /// # Errors
    /// Returns an error if the settings file cannot be written.
    pub fn set_unit_choice(&self, choice: UnitChoice) -> Result<(), CoreError> {
        let mut settings = Settings::load(self.dir());
        settings.units = choice.into();
        settings.save(self.dir()).map_err(|detail| CoreError::Failed { detail })?;
        self.refresh_units(None);
        Ok(())
    }

    /// Tell the core the phone's region (an ISO 3166 code such as "US"), which `Auto` units follow.
    pub fn set_region(&self, country: String) {
        self.refresh_units(Some(country));
    }

    /// The units everything is shown in now: the saved choice, with `Auto` resolved by the region.
    pub fn units(&self) -> UnitSystem {
        self.unit_system().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// A fresh engine directory, removed when the test ends.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let d = std::env::temp_dir().join(format!("apgo-ffi-settings-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            Self(d)
        }

        fn engine(&self) -> Arc<Engine> {
            Engine::new(self.0.to_string_lossy().into_owned())
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn choice_defaults_to_auto_and_follows_the_region() {
        let d = Dir::new("default");
        let e = d.engine();
        assert_eq!(e.unit_choice(), UnitChoice::Auto);
        assert_eq!(e.units(), UnitSystem::Metric, "no region yet");
        e.set_region("US".into());
        assert_eq!(e.units(), UnitSystem::Imperial);
        e.set_region("DE".into());
        assert_eq!(e.units(), UnitSystem::Metric);
    }

    #[test]
    fn a_saved_choice_survives_a_restart_and_overrides_the_region() {
        let d = Dir::new("saved");
        d.engine().set_unit_choice(UnitChoice::Metric).unwrap();
        let e = d.engine();
        e.set_region("US".into());
        assert_eq!(e.unit_choice(), UnitChoice::Metric);
        assert_eq!(e.units(), UnitSystem::Metric);
        e.set_unit_choice(UnitChoice::Imperial).unwrap();
        assert_eq!(e.units(), UnitSystem::Imperial);
    }

    #[test]
    fn a_failed_save_is_reported_and_changes_nothing() {
        let d = Dir::new("blocked");
        let e = d.engine();
        std::fs::create_dir_all(d.0.join("settings.json")).unwrap();
        assert!(e.set_unit_choice(UnitChoice::Imperial).is_err());
        assert_eq!(e.unit_choice(), UnitChoice::Auto);
        assert_eq!(e.units(), UnitSystem::Metric);
    }

    #[test]
    fn the_formatters_are_the_cores() {
        assert_eq!(format_distance(17.07, UnitSystem::Imperial), "60 ft");
        assert_eq!(format_distance(1_440.0, UnitSystem::Metric), "1.4 km");
        assert_eq!(format_area(2_500_000.0, UnitSystem::Metric), "3 km²");
    }
}
