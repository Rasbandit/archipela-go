//! The carry offset: where the player walks relative to where the phone points, learned from good courses.

use crate::loc::heading::CarryOffset;
use crate::loc::{Estimate, Source};

use super::Locator;

impl Locator {
    /// The carry offset learned so far: where the player walks relative to where the phone points.
    #[must_use]
    pub fn carry(&self) -> &CarryOffset {
        &self.carry
    }

    /// Learn the carry offset from an accepted, moving GPS estimate with a sure course (controller notes 2-4: no tilt rule, no street
    /// graph needed; never from a simulated fix), at most once per `carry_learn_min_gap_ms` (review R2). A maneuver's line fit that
    /// turned away from the filter's course has no sigma, so a corner teaches nothing (review I1); nor does an "unseen" one while the
    /// compass turned more than `carry_turn_skip_deg` over the fit's span (ruling T20-corner2: the walker turned the phone too).
    pub(super) fn learn_carry(&mut self, e: &Estimate) {
        let p = &self.params;
        let due = self.carry.learned_ms().is_none_or(|t| e.t_ms.saturating_sub(t) >= p.carry_learn_min_gap_ms);
        let good = due
            && e.accepted
            && !self.last_sim
            && e.source == Source::Gps
            && e.uncertainty_m <= p.carry_learn_max_unc_m
            && e.speed_mps >= p.carry_learn_min_speed_mps
            && self
                .last_course_fit_from_ms
                .is_none_or(|t0| self.compass.turn_deg(e.t_ms, e.t_ms.saturating_sub(t0)).is_none_or(|d| d <= p.carry_turn_skip_deg));
        if let (true, Some(c), Some(cs)) = (good, e.course_deg, self.last_course_sigma_deg) {
            if cs <= p.carry_learn_max_course_sigma_deg {
                self.carry.learn(c, cs, &self.compass, e.t_ms, p);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::geo::destination;

    use crate::loc::locator::maneuver::wrap_deg;
    use crate::loc::locator::test_util::*;
    use crate::loc::locator::Locator;
    use crate::loc::params::LocParams;
    use crate::loc::HeadingIn;
    use crate::num::i64_to_f64;

    /// Walk east at 1.4 m/s for `secs` with a fix every `fix_s` and the phone upright in a pocket, its compass pointing `az` and read every
    /// `compass_ms`.
    fn walk_with_compass(l: &mut Locator, secs: i64, fix_s: i64, acc: f64, az: f64, compass_ms: i64) {
        let head = |t_ms| HeadingIn { t_ms, azimuth_deg: az, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 80.0, roll_deg: 0.0, error_deg: None };
        for t in 0..secs {
            (0..1000 / compass_ms).for_each(|i| l.on_heading(&head(t * 1000 + i * compass_ms)));
            if t % fix_s == 0 {
                l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, acc));
            }
        }
    }

    #[test]
    fn walking_with_the_phone_in_a_pocket_learns_the_carry_offset() {
        // Controller notes 2-4: upright (no flat rule), from the filter's course, with no street graph.
        let mut l = Locator::default();
        walk_with_compass(&mut l, 90, 1, 4.0, 0.0, 200);
        assert!(wrap_deg(l.carry().delta_deg() - 90.0).abs() < 5.0, "{}", l.carry().delta_deg());
        assert!(conf(&l) >= 0.5, "{}", conf(&l));
    }

    #[test]
    fn walking_with_the_screen_off_learns_the_carry_offset() {
        // Controller note 1: fixes every 5 s and the compass at 1 Hz while the screen is off; the course is then the line fit's.
        let mut l = Locator::default();
        walk_with_compass(&mut l, 240, 5, 4.0, 270.0, 1000);
        assert!(wrap_deg(l.carry().delta_deg() + 180.0).abs() < 5.0, "{}", l.carry().delta_deg());
        assert!(conf(&l) >= 0.5, "{}", conf(&l));
    }

    #[test]
    fn turning_a_corner_keeps_the_carry_offset() {
        // The phone turns with the player; the filter's course lags the corner for a few fixes, which must not read as a carry change
        // (a reset would read as no confidence for several fixes).
        let mut l = Locator::default();
        let head = |t_ms, az| HeadingIn { t_ms, azimuth_deg: az, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 80.0, roll_deg: 0.0, error_deg: None };
        let corner = destination(o(), 90.0, 1.4 * 90.0);
        for t in 0..100 {
            let (at, az) =
                if t < 90 { (destination(o(), 90.0, 1.4 * i64_to_f64(t)), 0.0) } else { (destination(corner, 0.0, 1.4 * i64_to_f64(t - 90)), 270.0) };
            (0..5).for_each(|i| l.on_heading(&head(t * 1000 + i * 200, az)));
            l.on_fix(&fix(at, t, 4.0));
            assert!(t < 60 || conf(&l) >= 0.5, "t {t}: {}", conf(&l));
            assert!(t < 60 || wrap_deg(l.carry().delta_deg() - 90.0).abs() < 5.0, "review I1, t {t}: {}", l.carry().delta_deg());
        }
    }

    #[test]
    fn turning_a_corner_with_the_screen_off_keeps_the_carry_offset() {
        // Review I1, ruling T20-corner2: with 5 s fixes the filter has no course, so the line fit is "unseen" even across the corner;
        // the compass turning with the walker over the fit's span keeps the straddling fit from teaching.
        let mut l = Locator::default();
        let head = |t_ms, az| HeadingIn { t_ms, azimuth_deg: az, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 80.0, roll_deg: 0.0, error_deg: None };
        let corner = destination(o(), 90.0, 1.4 * 240.0);
        for t in 0..300 {
            let (at, az) =
                if t < 240 { (destination(o(), 90.0, 1.4 * i64_to_f64(t)), 0.0) } else { (destination(corner, 0.0, 1.4 * i64_to_f64(t - 240)), 270.0) };
            l.on_heading(&head(t * 1000, az));
            if t % 5 == 0 {
                l.on_fix(&fix(at, t, 4.0));
                assert!(t < 200 || wrap_deg(l.carry().delta_deg() - 90.0).abs() < 5.0, "t {t}: {}", l.carry().delta_deg());
            }
        }
    }

    #[test]
    fn a_noisy_compass_on_a_straight_screen_off_walk_still_learns() {
        // Ruling T20-corner2: the compass-turn skip must not block straight walks with pocket noise (up to 10 degrees either way).
        let mut l = Locator::default();
        let head = |t_ms, az| HeadingIn { t_ms, azimuth_deg: az, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 80.0, roll_deg: 0.0, error_deg: None };
        for t in 0..240 {
            l.on_heading(&head(t * 1000, 270.0 + 10.0 * (i64_to_f64(t) * 1.7).sin()));
            if t % 5 == 0 {
                l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, 4.0));
            }
        }
        assert!(wrap_deg(l.carry().delta_deg() + 180.0).abs() < 8.0, "{}", l.carry().delta_deg());
        assert!(conf(&l) >= 0.5, "{}", conf(&l));
    }

    #[test]
    fn the_carry_offset_learns_at_most_every_three_seconds() {
        // Review R2: one-second courses are strongly correlated; learning from every one would overstate what is known.
        let mut l = Locator::default();
        let mut times = Vec::new();
        let head = |t_ms| HeadingIn { t_ms, azimuth_deg: 0.0, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 80.0, roll_deg: 0.0, error_deg: None };
        for t in 0..90 {
            (0..5).for_each(|i| l.on_heading(&head(t * 1000 + i * 200)));
            l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, 4.0));
            times.extend(l.carry().learned_ms().filter(|m| times.last() != Some(m)));
        }
        assert!(times.len() >= 10, "{times:?}");
        assert!(times.windows(2).all(|w| w[1] - w[0] >= l.params().carry_learn_min_gap_ms), "{times:?}");
    }

    #[test]
    fn moving_the_phone_from_hand_to_pocket_relearns_the_offset() {
        // Ruling T20-change: a 60 degree carry change (inside 3 sigma) is found by its run of one-sided residuals.
        let mut l = Locator::default();
        let head = |t_ms, az| HeadingIn { t_ms, azimuth_deg: az, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 80.0, roll_deg: 0.0, error_deg: None };
        let mut doubted = false;
        for t in 0..130 {
            let az = if t < 90 { 0.0 } else { 300.0 }; // offset 90, then 150
            (0..5).for_each(|i| l.on_heading(&head(t * 1000 + i * 200, az)));
            l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, 4.0));
            doubted |= t >= 90 && conf(&l) < 0.5;
            if t == 100 {
                assert!(wrap_deg(l.carry().delta_deg() - 150.0).abs() < 10.0, "re-learned within 10 s: {}", l.carry().delta_deg());
            }
        }
        assert!(doubted, "the confidence drops while re-learning");
        assert!(wrap_deg(l.carry().delta_deg() - 150.0).abs() < 5.0 && conf(&l) >= 0.5, "{} {}", l.carry().delta_deg(), conf(&l));
    }

    #[test]
    fn with_the_carry_off_learning_still_runs_for_the_bench() {
        // Review M5/M6: `carry_enabled = false` only changes the gap heading; `carry()` still shows what was learned.
        let mut l = Locator::new(LocParams { carry_enabled: false, ..LocParams::default() });
        walk_with_compass(&mut l, 90, 1, 4.0, 0.0, 200);
        assert!(wrap_deg(l.carry().delta_deg() - 90.0).abs() < 5.0 && conf(&l) >= 0.5, "{} {}", l.carry().delta_deg(), conf(&l));
    }

    #[test]
    fn standing_or_blurry_fixes_teach_no_carry_offset() {
        let mut still = Locator::default();
        for t in 0..120 {
            still.on_heading(&HeadingIn {
                t_ms: t * 1000,
                azimuth_deg: 0.0,
                accuracy: crate::loc::CompassAccuracy::High,
                pitch_deg: 80.0,
                roll_deg: 0.0,
                error_deg: None,
            });
            stand(&mut still, t, 1);
        }
        assert_eq!(conf(&still), 0.0, "no course while standing");
        let mut blurry = Locator::default();
        walk_with_compass(&mut blurry, 90, 1, 30.0, 0.0, 200);
        assert_eq!(conf(&blurry), 0.0, "uncertainty over 10 m");
    }
}
