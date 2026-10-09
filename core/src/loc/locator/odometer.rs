//! The odometer: distance travelled, from accepted estimates.

use crate::catalog::Mode;
use crate::geo::distance_m;
use crate::loc::imm::{self};
use crate::loc::{Estimate, Motion, Verdict};
use crate::num::i64_to_f64;

/// Distance travelled, from accepted estimates. One rule for the game and the bench.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Odometer {
    last: Option<Estimate>,
    speed_cap_mps: f64,
}

impl Default for Odometer {
    /// An odometer for a Walk zone.
    fn default() -> Self {
        Self::new(imm::mode_cap_mps(Mode::Walk))
    }
}

impl Odometer {
    /// A gap longer than this between accepted estimates is not travelled distance, ms.
    pub const MAX_GAP_MS: i64 = 300_000;

    /// An odometer for a zone whose mode moves at most `speed_cap_mps` (a filter restart faster than that is not travelled distance).
    #[must_use]
    pub fn new(speed_cap_mps: f64) -> Self {
        Self { last: None, speed_cap_mps }
    }

    /// The zone's mode changed.
    pub fn set_speed_cap(&mut self, speed_cap_mps: f64) {
        self.speed_cap_mps = speed_cap_mps;
    }

    /// The speed cap in use, m/s.
    #[must_use]
    pub fn speed_cap_mps(&self) -> f64 {
        self.speed_cap_mps
    }

    /// Forget the previous point (counting paused, game reopened).
    pub fn clear(&mut self) {
        self.last = None;
    }

    /// Metres to add for `e`: the step from the previous accepted estimate, unless `e` is not accepted, relocates the filter, comes after a
    /// gap over 5 min, or the player is standing. A restart that is not accepted forgets the previous estimate. A restart (`Reset`, after a gap) adds the chord from the previous estimate when the
    /// implied speed is within the zone's cap, and nothing when it is not (ruling T8-R13).
    pub fn step(&mut self, e: &Estimate) -> f64 {
        self.step_with(e, None)
    }

    /// [`Self::step`] for an estimate whose fix ended a stationary hold as a relocation, when `hold_relocation_speed` is the agreeing
    /// fixes' own speed ([`Locator::hold_relocation_speed`](super::Locator::hold_relocation_speed)): the `Relocated` then adds the chord when that speed is within the zone's cap
    /// (rulings T8-R18, R20).
    pub fn step_with(&mut self, e: &Estimate, hold_relocation_speed: Option<f64>) -> f64 {
        if !e.accepted {
            if matches!(e.verdict, Verdict::Reset | Verdict::Relocated) {
                self.last = None; // a restart too uncertain to count: no chord from before it (ruling FR-I1)
            }
            return 0.0;
        }
        let moved = match self.last {
            Some(l) if e.t_ms.saturating_sub(l.t_ms) <= Self::MAX_GAP_MS => {
                let d = distance_m(l.point(), e.point());
                let dt_s = i64_to_f64(e.t_ms.saturating_sub(l.t_ms)) / 1000.0;
                let plausible = dt_s > 0.0 && d / dt_s <= self.speed_cap_mps;
                match e.verdict {
                    Verdict::Reset if plausible => d,
                    Verdict::Relocated if hold_relocation_speed.is_some_and(|v| v <= self.speed_cap_mps) => d,
                    Verdict::Reset | Verdict::Relocated => 0.0,
                    _ if e.motion == Motion::Stationary => 0.0,
                    _ => d,
                }
            }
            _ => 0.0,
        };
        self.last = Some(*e);
        moved
    }
}

#[cfg(test)]
mod tests {

    use crate::catalog::Mode;
    use crate::geo::destination;

    use crate::loc::imm::{self};

    use crate::loc::{Estimate, Motion, Verdict};

    use crate::loc::locator::test_util::*;
    use crate::loc::locator::Odometer;

    #[test]
    fn the_odometer_counts_walking_and_not_standing_or_gaps() {
        let mut odo = Odometer::new(imm::mode_cap_mps(Mode::Walk));
        let at =
            |m: f64, t_s: i64, motion: Motion| Estimate { motion, ..Estimate::exact(destination(o(), 90.0, m).lat, destination(o(), 90.0, m).lon, t_s * 1000) };
        assert_eq!(odo.step(&at(0.0, 0, Motion::Walking)), 0.0);
        assert!((odo.step(&at(10.0, 5, Motion::Walking)) - 10.0).abs() < 0.01);
        assert_eq!(odo.step(&at(12.0, 10, Motion::Stationary)), 0.0);
        assert_eq!(odo.step(&at(500.0, 400, Motion::Walking)), 0.0, "over 5 min");
        assert_eq!(odo.step(&Estimate { accepted: false, ..at(600.0, 401, Motion::Walking) }), 0.0);
        assert_eq!(odo.step(&Estimate { verdict: Verdict::Relocated, ..at(5000.0, 402, Motion::Walking) }), 0.0);
        assert_eq!(odo.step(&Estimate { verdict: Verdict::Reset, ..at(9000.0, 403, Motion::Walking) }), 0.0, "ruling T8-R13: 4 km in 1 s is no walk");
        assert!((odo.step(&at(9010.0, 404, Motion::Walking)) - 10.0).abs() < 0.01, "counting goes on from the reset point");
        // Ruling T8-R13: a reset after a gap adds the chord when the implied speed is plausible for the zone (80 m in 60 s walking) ...
        let reset = |m: f64, t_s: i64| Estimate { verdict: Verdict::Reset, motion: Motion::Stationary, ..at(m, t_s, Motion::Walking) };
        assert!((odo.step(&reset(9090.0, 464)) - 80.0).abs() < 0.01, "a plausible gap chord counts");
        // ... and nothing when it is not (2 km in 60 s), or for a relocation.
        assert_eq!(odo.step(&reset(11_090.0, 524)), 0.0, "33 m/s is no walk");
        assert_eq!(odo.step(&Estimate { verdict: Verdict::Relocated, ..at(11_170.0, 584, Motion::Walking) }), 0.0);
        // Ruling T8-R18: a relocation that ended a stationary hold adds the chord under the same cap.
        let relocated = |m: f64, t_s: i64| Estimate { verdict: Verdict::Relocated, ..at(m, t_s, Motion::Walking) };
        // Ruling T8-R20: judged by the agreeing fixes' own speed, not the time since the previous (frozen) estimate.
        assert!((odo.step_with(&relocated(11_200.0, 585), Some(2.0)) - 30.0).abs() < 0.01, "agreeing at 2 m/s from a hold counts");
        assert_eq!(odo.step_with(&relocated(11_390.0, 604), Some(20.0)), 0.0, "20 m/s from a hold is no walk");
        assert_eq!(odo.step_with(&relocated(11_400.0, 609), None), 0.0, "a relocation that ended no hold adds nothing");
    }
}
