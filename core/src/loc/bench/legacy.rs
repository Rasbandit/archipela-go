//! The rules the game used before the location program, kept only for `--compare baseline`: accuracy 35 m, an implied speed of 100 km/h
//! is a jump, the third outlier in a row is believed, and the odometer moves in steps of at least 6 m (or the accuracy).

use crate::geo::{distance_m, Point};
use crate::loc::bench::Shown;
use crate::loc::{RawFix, Verdict};
use crate::num::i64_to_f64;

const MAX_ACCURACY_M: f64 = 35.0;
const MAX_PLAUSIBLE_KMH: f64 = 100.0;
const MAX_OUTLIER_STREAK: u32 = 3;
const ODOMETER_MIN_STEP_M: f64 = 6.0;

/// Speed between two fixes in km/h, ignoring what both error radii could explain; `None` for gaps under 1 s or over 2 min.
#[must_use]
pub fn legacy_implied_speed_kmh(prev: &RawFix, cur: &RawFix) -> Option<f64> {
    let dt = i64_to_f64(cur.t_ms - prev.t_ms) / 1000.0;
    if !(1.0..=120.0).contains(&dt) {
        return None;
    }
    let effective = (distance_m(prev.point(), cur.point()) - prev.accuracy_m - cur.accuracy_m).max(0.0);
    Some(effective / dt * 3.6)
}

/// Today's rules as a filter. The map showed every raw fix, so the shown position is the raw one; `accepted` is what quests got.
#[derive(Debug, Clone, Default)]
pub struct LegacyRules {
    last: Option<RawFix>,
    streak: u32,
    anchor: Option<Point>,
    odometer_m: f64,
}

impl LegacyRules {
    /// A fresh rule set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one fix.
    pub fn feed(&mut self, f: &RawFix) -> Shown {
        let shown =
            |accepted, verdict, odometer_m| Shown { t_ms: f.t_ms, p: f.point(), uncertainty_m: f.accuracy_m, accepted, verdict, course_deg: None, odometer_m };
        if f.accuracy_m > MAX_ACCURACY_M {
            return shown(false, Verdict::Blurry, self.odometer_m);
        }
        let jump = self.last.as_ref().and_then(|l| legacy_implied_speed_kmh(l, f)).is_some_and(|k| k > MAX_PLAUSIBLE_KMH);
        if jump && self.streak < MAX_OUTLIER_STREAK - 1 {
            self.streak += 1;
            return shown(false, Verdict::Gated, self.odometer_m);
        }
        self.streak = 0;
        if self.last.is_some_and(|l| f.t_ms - l.t_ms > 300_000) {
            self.anchor = None;
        }
        let p = f.point();
        match self.anchor {
            Some(a) if distance_m(a, p) >= f.accuracy_m.max(ODOMETER_MIN_STEP_M) => {
                self.odometer_m += distance_m(a, p);
                self.anchor = Some(p);
            }
            Some(_) => {}
            None => self.anchor = Some(p),
        }
        self.last = Some(*f);
        shown(true, Verdict::Used, self.odometer_m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn at(p: Point, t_s: i64, acc: f64) -> RawFix {
        RawFix::at(p.lat, p.lon, t_s * 1000, acc)
    }

    #[test]
    fn the_legacy_rules_reproduce_the_old_game_behaviour() {
        let o = Point::new(40.0, -111.0);
        let mut l = LegacyRules::new();
        assert!(l.feed(&at(o, 1000, 5.0)).accepted);
        assert_eq!(l.feed(&at(o, 1001, 40.0)).verdict, Verdict::Blurry, "worse than 35 m");
        let far = destination(o, 90.0, 5000.0);
        let v: Vec<bool> = [1003, 1006, 1009].iter().map(|t| l.feed(&at(far, *t, 5.0)).accepted).collect();
        assert_eq!(v, [false, false, true], "the third far fix in a row is believed");
        assert!(legacy_implied_speed_kmh(&at(o, 0, 5.0), &at(o, 0, 5.0)).is_none(), "same instant");
    }

    #[test]
    fn the_legacy_odometer_ignores_wobble_under_six_metres() {
        let o = Point::new(40.0, -111.0);
        let mut l = LegacyRules::new();
        let mut last = l.feed(&at(o, 1000, 5.0)).odometer_m;
        for i in 0..60 {
            last = l.feed(&at(destination(o, f64::from(i * 97 % 360), 3.0 + f64::from(i % 3)), 1005 + i64::from(i) * 5, 5.0)).odometer_m;
        }
        assert!(last < 12.0, "{last}");
    }
}
