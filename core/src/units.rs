//! The one place distances, speeds and areas become text, in the player's units.
//!
//! Numbers lean to clean values: short distances snap to 5 or 10 (m) or 10 (ft), mid-range ones keep at most one decimal,
//! long ones are whole; areas keep one decimal under 10. A limit rounds down and an amount to go rounds up ([`Round`]).
//! Always a decimal point and Western digits, whatever the phone's locale.

/// Metres in a mile.
const M_PER_MILE: f64 = 1609.344;
/// Metres in a foot.
const M_PER_FOOT: f64 = 0.3048;
/// Below this many feet (a tenth of a mile) distances read in feet.
const FEET_UNTIL: f64 = 528.0;
/// What a value that cannot be shown (NaN, infinite) reads as.
const NO_VALUE: &str = "–";

/// The units distances are shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnitSystem {
    /// Metres and kilometres.
    #[default]
    Metric,
    /// Feet and miles.
    Imperial,
}

/// Which way a shown number rounds. A limit the player must stay within rounds `Down`, an amount still to go (or a reading
/// that missed a limit) rounds `Up`, so the text never promises more room than the check allows, nor says done too soon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Round {
    /// To the nearest step.
    Nearest,
    /// Up to the next step.
    Up,
    /// Down to the step below.
    Down,
}

/// Slack for float noise when rounding up or down: 0.3 / 0.1 is 2.9999999999999996, which must still count as 3 steps.
const STEP_SLACK: f64 = 1e-9;

/// `v` rounded to a multiple of `step`, the `round` way.
fn snap(v: f64, step: f64, round: Round) -> f64 {
    let q = v / step;
    let n = match round {
        Round::Nearest => q.round(),
        Round::Up => (q - STEP_SLACK).ceil(),
        Round::Down => (q + STEP_SLACK).floor(),
    };
    // + 0.0 turns the -0.0 that rounding 0 up gives (ceil of -1e-9) into 0.0, which prints "0", not "-0".
    n * step + 0.0
}

/// A big-unit value: one decimal (a trailing ".0" dropped) below 10, whole from 10; never below `min`, so a value that
/// only just crossed into the big unit cannot round back under it.
fn big(v: f64, min: f64, unit: &str, round: Round) -> String {
    if v >= 10.0 {
        return format!("{} {unit}", snap(v, 1.0, round));
    }
    let r = snap(v, 0.1, round).max(min);
    if r >= 10.0 || r.fract().abs() < STEP_SLACK {
        format!("{} {unit}", r.round())
    } else {
        format!("{r:.1} {unit}")
    }
}

/// A distance in metres as text, e.g. "50 m", "1.4 km", "60 ft", "0.3 mi".
#[must_use]
pub fn distance(m: f64, units: UnitSystem) -> String {
    distance_rounded(m, units, Round::Nearest)
}

/// A distance in metres as text, rounded the `round` way (see [`Round`]).
#[must_use]
pub fn distance_rounded(m: f64, units: UnitSystem, round: Round) -> String {
    if !m.is_finite() {
        return NO_VALUE.into();
    }
    let m = m.max(0.0);
    match units {
        UnitSystem::Metric => {
            // Switch on the rounded value, or 996 m would read "1000 m".
            let short = snap(m, if m < 100.0 { 5.0 } else { 10.0 }, round);
            if short < 1000.0 {
                format!("{short} m")
            } else {
                big(m / 1000.0, 1.0, "km", round)
            }
        }
        UnitSystem::Imperial => {
            let feet = snap(m / M_PER_FOOT, 10.0, round);
            if feet < FEET_UNTIL {
                format!("{feet} ft")
            } else {
                big(m / M_PER_MILE, 0.1, "mi", round)
            }
        }
    }
}

/// A speed in km/h as text: whole km/h or mph.
#[must_use]
pub fn speed_kmh(kmh: f64, units: UnitSystem) -> String {
    if !kmh.is_finite() {
        return NO_VALUE.into();
    }
    match units {
        UnitSystem::Metric => format!("{} km/h", kmh.round()),
        UnitSystem::Imperial => format!("{} mph", (kmh * 1000.0 / M_PER_MILE).round()),
    }
}

/// An area in square metres as text in km² or mi²: one decimal under 10, whole from 10, "<0.1 km²" for a tiny one.
#[must_use]
pub fn area(m2: f64, units: UnitSystem) -> String {
    if !m2.is_finite() {
        return NO_VALUE.into();
    }
    let (v, unit) = match units {
        UnitSystem::Metric => (m2 / 1_000_000.0, "km²"),
        UnitSystem::Imperial => (m2 / (M_PER_MILE * M_PER_MILE), "mi²"),
    };
    if v > 0.0 && snap(v, 0.1, Round::Nearest) == 0.0 {
        format!("<0.1 {unit}")
    } else {
        big(v.max(0.0), 0.0, unit, Round::Nearest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use UnitSystem::{Imperial, Metric};

    fn km(m: f64) -> String {
        distance(m, Metric)
    }

    fn mi(m: f64) -> String {
        distance(m, Imperial)
    }

    #[test]
    fn short_metric_distances_snap_to_five_then_ten_metres() {
        assert_eq!(km(0.0), "0 m");
        assert_eq!(km(2.0), "0 m");
        assert_eq!(km(3.0), "5 m");
        assert_eq!(km(48.0), "50 m");
        assert_eq!(km(97.0), "95 m");
        assert_eq!(km(99.0), "100 m");
        assert_eq!(km(234.0), "230 m");
        assert_eq!(km(994.0), "990 m");
    }

    #[test]
    fn metres_switch_to_kilometres_once_rounding_reaches_one() {
        assert_eq!(km(995.0), "1 km");
        assert_eq!(km(1000.0), "1 km");
    }

    #[test]
    fn mid_kilometres_keep_one_decimal_and_drop_a_trailing_zero() {
        assert_eq!(km(1_440.0), "1.4 km");
        assert_eq!(km(1_250.0), "1.3 km");
        assert_eq!(km(2_000.0), "2 km");
        assert_eq!(km(9_940.0), "9.9 km");
        assert_eq!(km(9_960.0), "10 km");
    }

    #[test]
    fn long_kilometres_are_whole() {
        assert_eq!(km(12_400.0), "12 km");
        assert_eq!(km(100_000.0), "100 km");
        assert_eq!(km(10_000_000.0), "10000 km");
    }

    #[test]
    fn short_imperial_distances_snap_to_ten_feet() {
        assert_eq!(mi(0.0), "0 ft");
        assert_eq!(mi(17.07), "60 ft"); // 56 ft
        assert_eq!(mi(15.0), "50 ft"); // 49 ft
        assert_eq!(mi(152.0), "500 ft");
    }

    #[test]
    fn feet_switch_to_miles_at_a_tenth_of_a_mile() {
        assert_eq!(mi(158.0), "520 ft");
        assert_eq!(mi(0.1 * M_PER_MILE), "0.1 mi");
        assert_eq!(mi(0.34 * M_PER_MILE), "0.3 mi");
    }

    #[test]
    fn miles_keep_one_decimal_then_go_whole() {
        assert_eq!(mi(1.5 * M_PER_MILE), "1.5 mi");
        assert_eq!(mi(M_PER_MILE), "1 mi");
        assert_eq!(mi(26.2 * M_PER_MILE), "26 mi");
        assert_eq!(mi(10_000.0 * M_PER_MILE), "10000 mi");
    }

    #[test]
    fn bad_or_negative_distances_do_not_crash() {
        assert_eq!(km(f64::NAN), "–");
        assert_eq!(mi(f64::INFINITY), "–");
        assert_eq!(km(-1e-9), "0 m");
    }

    #[test]
    fn speeds_are_whole() {
        assert_eq!(speed_kmh(5.6, Metric), "6 km/h");
        assert_eq!(speed_kmh(16.09344, Imperial), "10 mph");
        assert_eq!(speed_kmh(f64::NAN, Metric), "–");
    }

    #[test]
    fn areas_keep_one_decimal_under_ten_then_go_whole() {
        assert_eq!(area(2_500_000.0, Metric), "2.5 km²");
        assert_eq!(area(600_000.0, Metric), "0.6 km²");
        assert_eq!(area(3_000_000.0, Metric), "3 km²");
        assert_eq!(area(12_400_000.0, Metric), "12 km²");
        assert_eq!(area(30_000.0, Metric), "<0.1 km²");
        assert_eq!(area(0.0, Metric), "0 km²");
        assert_eq!(area(M_PER_MILE * M_PER_MILE * 25.0, Imperial), "25 mi²");
        assert_eq!(area(500_000.0, Imperial), "0.2 mi²");
        assert_eq!(area(f64::NAN, Imperial), "–");
    }

    #[test]
    fn a_limit_rounds_down_so_the_text_never_promises_more_room() {
        assert_eq!(distance_rounded(17.07, Imperial, Round::Down), "50 ft"); // 56 ft
        assert_eq!(distance_rounded(30.0, Imperial, Round::Down), "90 ft"); // 98 ft
        assert_eq!(distance_rounded(38.0, Metric, Round::Down), "35 m");
        assert_eq!(distance_rounded(40.0, Metric, Round::Down), "40 m", "an exact step stays put");
        assert_eq!(distance_rounded(995.0, Metric, Round::Down), "990 m");
        assert_eq!(distance_rounded(1_490.0, Metric, Round::Down), "1.4 km");
        assert_eq!(distance_rounded(800.0, Imperial, Round::Down), "0.4 mi");
        assert_eq!(distance_rounded(19_900.0, Metric, Round::Down), "19 km");
    }

    #[test]
    fn an_amount_to_go_rounds_up_so_it_never_reads_as_done_too_soon() {
        assert_eq!(distance_rounded(2.0, Metric, Round::Up), "5 m");
        assert_eq!(distance_rounded(0.5, Imperial, Round::Up), "10 ft");
        assert_eq!(distance_rounded(37.0, Metric, Round::Up), "40 m");
        assert_eq!(distance_rounded(60.0, Metric, Round::Up), "60 m", "an exact step stays put");
        assert_eq!(distance_rounded(996.0, Metric, Round::Up), "1 km");
        assert_eq!(distance_rounded(400.0, Imperial, Round::Up), "0.3 mi");
        assert_eq!(distance_rounded(0.0, Metric, Round::Up), "0 m");
        assert_eq!(distance_rounded(10_100.0, Metric, Round::Up), "11 km");
    }

    #[test]
    fn a_tenth_that_float_maths_puts_just_off_a_step_still_snaps_to_it() {
        // 0.3 / 0.1 is 2.9999999999999996 in f64: rounding down must still give 0.3, not 0.2.
        assert_eq!(distance_rounded(300.0, Metric, Round::Down), "300 m");
        assert_eq!(distance_rounded(1_300.0, Metric, Round::Down), "1.3 km");
        assert_eq!(distance_rounded(1_700.0, Metric, Round::Up), "1.7 km");
    }
}
