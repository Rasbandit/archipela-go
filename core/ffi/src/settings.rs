//! Player settings over FFI: the unit choice and the units it resolves to.

use apgo_core::settings::{self as core, resolve_units, Settings};

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

impl From<core::UnitSystem> for UnitSystem {
    fn from(u: core::UnitSystem) -> Self {
        match u {
            core::UnitSystem::Metric => Self::Metric,
            core::UnitSystem::Imperial => Self::Imperial,
        }
    }
}

#[uniffi::export]
#[allow(clippy::needless_pass_by_value)] // uniffi requires owned args
impl Engine {
    /// The saved unit choice (`Auto` until the player picks one).
    pub fn unit_choice(&self) -> UnitChoice {
        Settings::load(self.dir()).units.into()
    }

    /// Save the unit choice, keeping every other setting.
    ///
    /// # Errors
    /// Returns an error if the settings file cannot be written.
    pub fn set_unit_choice(&self, choice: UnitChoice) -> Result<(), CoreError> {
        let mut settings = Settings::load(self.dir());
        settings.units = choice.into();
        settings.save(self.dir()).map_err(|detail| CoreError::Failed { detail })
    }

    /// The units to show on a phone set to `country` (an ISO 3166 code such as "US"), after the saved choice.
    pub fn units(&self, country: String) -> UnitSystem {
        resolve_units(Settings::load(self.dir()).units, &country).into()
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
    fn choice_defaults_to_auto_and_follows_the_country() {
        let d = Dir::new("default");
        let e = d.engine();
        assert_eq!(e.unit_choice(), UnitChoice::Auto);
        assert_eq!(e.units("US".into()), UnitSystem::Imperial);
        assert_eq!(e.units("DE".into()), UnitSystem::Metric);
    }

    #[test]
    fn a_saved_choice_survives_a_restart_and_overrides_the_country() {
        let d = Dir::new("saved");
        d.engine().set_unit_choice(UnitChoice::Metric).unwrap();
        let e = d.engine();
        assert_eq!(e.unit_choice(), UnitChoice::Metric);
        assert_eq!(e.units("US".into()), UnitSystem::Metric);
        e.set_unit_choice(UnitChoice::Imperial).unwrap();
        assert_eq!(e.units("DE".into()), UnitSystem::Imperial);
    }

    #[test]
    fn a_failed_save_is_reported() {
        let d = Dir::new("blocked");
        let e = d.engine();
        std::fs::create_dir_all(d.0.join("settings.json")).unwrap();
        assert!(e.set_unit_choice(UnitChoice::Imperial).is_err());
        assert_eq!(e.unit_choice(), UnitChoice::Auto);
    }
}
