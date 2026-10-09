//! The one place distances, speeds and areas become text, in the player's units.
//!
//! Numbers lean to clean values: short distances snap to 5 or 10 (m) or 10 (ft), mid-range ones keep at most one decimal,
//! long ones and areas are whole. Always a decimal point and Western digits, whatever the phone's locale.

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

/// `v` rounded to the nearest multiple of `step`.
fn snap(v: f64, step: f64) -> f64 {
    (v / step).round() * step
}

/// A big-unit value: one decimal (a trailing ".0" dropped) below 10, whole from 10; never below `min`, so a value that
/// only just crossed into the big unit cannot round back under it.
fn big(v: f64, min: f64, unit: &str) -> String {
    if v >= 10.0 {
        return format!("{} {unit}", v.round());
    }
    let r = snap(v, 0.1).max(min);
    if r >= 10.0 || r.fract().abs() < 1e-9 {
        format!("{} {unit}", r.round())
    } else {
        format!("{r:.1} {unit}")
    }
}

/// A distance in metres as text, e.g. "50 m", "1.4 km", "60 ft", "0.3 mi".
#[must_use]
pub fn distance(m: f64, units: UnitSystem) -> String {
    if !m.is_finite() {
        return NO_VALUE.into();
    }
    let m = m.max(0.0);
    match units {
        UnitSystem::Metric => {
            // Switch on the rounded value, or 996 m would read "1000 m".
            let short = snap(m, if m < 100.0 { 5.0 } else { 10.0 });
            if short < 1000.0 {
                format!("{short} m")
            } else {
                big(m / 1000.0, 1.0, "km")
            }
        }
        UnitSystem::Imperial => {
            let feet = snap(m / M_PER_FOOT, 10.0);
            if feet < FEET_UNTIL {
                format!("{feet} ft")
            } else {
                big(m / M_PER_MILE, 0.1, "mi")
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

/// An area in square metres as text: whole km² or mi², "<1 km²" for a small one.
#[must_use]
pub fn area(m2: f64, units: UnitSystem) -> String {
    if !m2.is_finite() {
        return NO_VALUE.into();
    }
    let (v, unit) = match units {
        UnitSystem::Metric => (m2 / 1_000_000.0, "km²"),
        UnitSystem::Imperial => (m2 / (M_PER_MILE * M_PER_MILE), "mi²"),
    };
    if v > 0.0 && v.round() < 1.0 {
        format!("<1 {unit}")
    } else {
        format!("{} {unit}", v.max(0.0).round())
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
    fn areas_are_whole_and_small_ones_read_under_one() {
        assert_eq!(area(2_500_000.0, Metric), "3 km²");
        assert_eq!(area(600_000.0, Metric), "1 km²");
        assert_eq!(area(200_000.0, Metric), "<1 km²");
        assert_eq!(area(0.0, Metric), "0 km²");
        assert_eq!(area(M_PER_MILE * M_PER_MILE * 25.0, Imperial), "25 mi²");
        assert_eq!(area(200_000.0, Imperial), "<1 mi²");
        assert_eq!(area(f64::NAN, Imperial), "–");
    }
}
