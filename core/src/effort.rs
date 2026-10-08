//! Effort model: difficulty = estimated active minutes; a tier is `minutes_per_tier` wide.

use crate::catalog::Mode;
use crate::num::round_u32;

/// Straight-line distance understates real routes.
pub const DETOUR: f64 = 1.3;

/// Minutes needed to cover `dist_m` metres in `mode`.
#[must_use]
pub fn travel_min(dist_m: f64, mode: Mode) -> f64 {
    dist_m * DETOUR / mode.m_per_min()
}

/// The effort tier (1-based) that `effort_min` minutes falls into, given `minutes_per_tier`.
#[must_use]
pub fn tier_for(effort_min: f64, minutes_per_tier: f64) -> u8 {
    u8::try_from(round_u32((effort_min / minutes_per_tier).ceil().max(1.0)).min(10)).unwrap_or(10)
}

/// Inclusive-lower / inclusive-upper effort minutes of a tier.
#[must_use]
pub fn band(tier: u8, minutes_per_tier: f64) -> (f64, f64) {
    ((f64::from(tier) - 1.0) * minutes_per_tier, f64::from(tier) * minutes_per_tier)
}

/// The middle of a tier in minutes: the inverse of [`tier_for`].
#[must_use]
pub fn mid(tier: u8, minutes_per_tier: f64) -> f64 {
    (f64::from(tier) - 0.5) * minutes_per_tier
}

/// Distance that takes about `effort_min` to reach one way.
#[must_use]
pub fn dist_for(effort_min: f64, mode: Mode) -> f64 {
    effort_min * mode.m_per_min() / DETOUR
}

/// Typical steps per minute when walking or running in `mode`.
#[must_use]
pub fn cadence_steps_per_min(mode: Mode) -> f64 {
    match mode {
        Mode::Run => 150.0,
        _ => 100.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_bands_and_distances_are_consistent() {
        assert_eq!(tier_for(0.5, 10.0), 1);
        assert_eq!(tier_for(10.0, 10.0), 1);
        assert_eq!(tier_for(10.1, 10.0), 2);
        assert_eq!(tier_for(500.0, 10.0), 10);
        assert_eq!(band(3, 10.0), (20.0, 30.0));
        assert_eq!(mid(3, 10.0), 25.0);
        let d = dist_for(30.0, Mode::Walk);
        assert!((travel_min(d, Mode::Walk) - 30.0).abs() < 1e-9);
        assert!(dist_for(30.0, Mode::Drive) > 5.0 * dist_for(30.0, Mode::Walk));
    }
}
