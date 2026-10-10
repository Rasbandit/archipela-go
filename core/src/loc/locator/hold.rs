//! The stationary hold: entering it, judging far fixes, leaving it.

use crate::loc::imm::{self, S};
use crate::loc::mat::{add, max_eig2, Mat};

use super::Locator;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Hold {
    pub(super) at: [f64; 2],
    pub(super) far: u32,
}

impl Locator {
    /// Whether the newest fix ended a stationary hold.
    #[must_use]
    pub fn ended_hold(&self) -> bool {
        self.ended_hold
    }

    /// When the newest fix ended a stationary hold as a relocation: the agreeing fixes' own speed, m/s (for [`Odometer::step_with`](super::Odometer::step_with)).
    #[must_use]
    pub fn hold_relocation_speed(&self) -> Option<f64> {
        self.reloc_speed.filter(|_| self.ended_hold)
    }

    /// Whether the stationary hold is freezing the position.
    #[must_use]
    pub fn holding(&self) -> bool {
        self.hold.is_some()
    }

    /// Enter or leave the stationary hold after an accepted fix at `z` with position noise `r_pos` (inflated for a Soft fix). "Far" uses
    /// the sigma of the fix-to-held-point innovation (filter covariance plus fix noise, ruling E1), so ordinary fix noise does not end a
    /// hold. With a quiet step counter (ruling T8-R2) far is `max(hold_quiet_max_m, hold_quiet_sigmas sigma)`: drift at a table never ends the hold, but
    /// a player moving without steps (a wheelchair, a stroller) is hidden at most that far (ruling FR-C1).
    pub(super) fn update_hold(&mut self, z: [f64; 2], r_pos: Mat<2, 2>, t_ms: i64) {
        let Some(imm) = &self.imm else { return };
        let p = &self.params;
        let out = imm.output();
        let moving = self.moving_steps(t_ms);
        if let Some(mut h) = self.hold {
            let sigma = max_eig2(&add(&imm::pos_block(&out.p), &r_pos)).max(0.0).sqrt();
            let limit = if self.quiet_counter(t_ms) { p.hold_quiet_max_m.max(p.hold_quiet_sigmas * sigma) } else { p.hold_exit_min_m.max(2.0 * sigma) };
            let far = (z[0] - h.at[0]).hypot(z[1] - h.at[1]) > limit;
            h.far = if far { h.far + 1 } else { 0 };
            self.hold = (h.far < p.hold_exit_fixes && !moving).then_some(h);
        } else if imm.mu[S] > p.hold_mu_s && out.x[2].hypot(out.x[3]) < p.hold_speed_mps && self.quiet_steps(t_ms, p.hold_quiet_steps_ms) {
            self.hold = Some(Hold { at: [out.x[0], out.x[1]], far: 0 });
            self.last_course = None; // a walker who stood may leave any way (review I4)
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::catalog::Mode;
    use crate::geo::{destination, distance_m};

    use crate::loc::imm::{self, Gate};
    use crate::loc::mat::{add, max_eig2, scale, Mat};

    use crate::loc::{Motion, RawFix, Verdict};
    use crate::num::i64_to_f64;

    use crate::loc::locator::test_util::*;
    use crate::loc::locator::{Locator, Odometer};

    #[test]
    fn standing_still_holds_the_position_until_two_far_fixes() {
        let mut l = Locator::default();
        let mut t = stand(&mut l, 0, 40);
        assert!(l.holding());
        let held = l.last().unwrap().point();
        assert_eq!(l.on_fix(&fix(destination(o(), 90.0, 4.0), t, 6.0)).point(), held, "one far fix does not end the hold");
        t += 1;
        let mut moved = false;
        for k in 1..=5 {
            moved |= l.on_fix(&fix(destination(o(), 90.0, 10.0 + 1.4 * i64_to_f64(k)), t, 6.0)).point() != held;
            t += 1;
        }
        assert!(moved && !l.holding());
    }

    #[test]
    fn steps_coming_in_end_the_hold_at_once() {
        let mut l = Locator::default();
        l.on_steps(100, 0, None);
        l.on_steps(100, 30_000, None);
        stand(&mut l, 1, 40);
        assert!(l.holding());
        l.on_steps(104, 41_500, None);
        assert!(!l.holding(), "4 steps in the last 5 s");
    }

    #[test]
    fn without_a_step_counter_hold_and_walking_still_work() {
        // Review Focus 2: no on_steps call at all.
        let mut l = Locator::default();
        let t = stand(&mut l, 0, 40);
        assert!(l.holding());
        let mut last = l.last().unwrap();
        for k in 1..=40 {
            last = l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(k)), t + k, 5.0));
        }
        assert!(!l.holding() && last.motion != Motion::Stationary, "{last:?}");
        assert!(distance_m(last.point(), destination(o(), 90.0, 56.0)) < 6.0);
    }

    #[test]
    fn walking_without_a_step_counter_never_holds() {
        // T5-carry / Review Focus 2: position-only fixes with 3 m noise read slow; the hold must still never engage while walking.
        use crate::loc::bench::gauss;
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut l = Locator::default();
            for k in 0..300 {
                let truth = destination(o(), 90.0, 1.4 * i64_to_f64(k));
                let noisy = destination(destination(truth, 0.0, 3.0 * gauss(&mut rng)), 90.0, 3.0 * gauss(&mut rng));
                l.on_fix(&fix(noisy, k, 5.0));
                assert!(!l.holding(), "seed {seed}: held at {k} s while walking");
            }
        }
    }

    #[test]
    fn fix_noise_alone_does_not_end_a_hold() {
        // Ruling E1: "far" is measured against the fix-to-held-point innovation sigma (filter plus fix noise). 6 m fixes at 15 m
        // accuracy are within that noise; against the filter's own sigma (well under 1 m after 40 fixes) they would end the hold.
        let mut l = Locator::default();
        let t = stand(&mut l, 0, 40);
        let held = l.last().unwrap().point();
        for k in 0..10 {
            let e = l.on_fix(&fix(destination(o(), if k % 2 == 0 { 0.0 } else { 180.0 }, 6.0), t + k, 15.0));
            assert!(e.accepted && e.point() == held && l.holding(), "{k}: {e:?}");
        }
    }

    #[test]
    #[allow(clippy::many_single_char_names)] // l, t, z, m, p, h, d, f, e: locator, time, fix ENU, measurement, P, hold, distance, fix, estimate
    fn a_soft_fix_is_judged_far_with_the_inflated_noise_the_filter_used() {
        // Ruling T7-soft: the hold exit uses R x r_scale for a Soft fix. A fix whose velocity disagrees (soft on 4 dof) and whose position
        // lies between 2 sigma of the plain innovation and 2 sigma of the inflated one is near, so two of them keep the hold.
        let mut l = Locator::default();
        let t = stand(&mut l, 0, 40);
        assert!(l.holding());
        let at = |d: f64, v: f64, t_s: i64| RawFix {
            speed_mps: Some(v),
            speed_acc_mps: Some(0.5),
            bearing_deg: Some(90.0),
            bearing_acc_deg: Some(10.0),
            ..fix(destination(o(), 90.0, d), t_s, 6.0)
        };
        // The fix's distance from the held point lies between the two thresholds, judged on the covariance after the update (as the
        // hold rule sees it).
        let window = |l: &Locator, f: &RawFix| {
            let z = l.frame.unwrap().to_enu(f.point());
            let (preds, _) = l.imm.as_ref().unwrap().predict_to(f.t_ms, l.mode, &l.params);
            let m = l.measurement(f, z);
            let Gate::Accept { r_scale, soft: true } = imm::gate(imm::min_d2(&preds, &m), true, &l.params) else { return false };
            let mut after = l.clone();
            after.on_fix(f);
            let p = imm::pos_block(&after.imm.as_ref().unwrap().output().p);
            let two_sigma = |r: &Mat<2, 2>| 2.0 * max_eig2(&add(&p, r)).sqrt();
            let h = l.hold.unwrap().at;
            let d = (z[0] - h[0]).hypot(z[1] - h[1]);
            d > two_sigma(&m.r_pos).max(3.0) && d < two_sigma(&scale(&m.r_pos, r_scale))
        };
        let candidates = || (20..250).flat_map(|d| (2..60).map(move |v| (0.1 * f64::from(d), 0.25 * f64::from(v))));
        let held = l.last().unwrap().point();
        for k in 0..2 {
            let f = candidates().map(|(d, v)| at(d, v, t + k)).find(|f| window(&l, f)).expect("a soft fix inside the window");
            let e = l.on_fix(&f);
            assert_eq!(e.verdict, Verdict::Soft, "{k}");
            assert_eq!(e.point(), held, "{k}: a soft fix within the inflated noise is near");
        }
        assert!(l.holding());
    }

    /// A locator holding still after 40 s, with a step counter that has reported no new steps for the last 30 s.
    fn held_with_a_quiet_step_counter() -> (Locator, i64) {
        let mut l = Locator::default();
        l.on_steps(100, 0, None);
        let t = stand(&mut l, 0, 40);
        l.on_steps(100, t * 1000 - 1000, None);
        assert!(l.holding());
        (l, t)
    }

    #[test]
    fn far_fixes_do_not_end_a_hold_while_the_step_counter_is_quiet() {
        // Ruling T8-R2: the step counter is the strongest evidence of standing; drift a few metres out does not end the hold.
        let (mut l, t) = held_with_a_quiet_step_counter();
        let held = l.last().unwrap().point();
        for k in 0..10 {
            let e = l.on_fix(&fix(destination(o(), 90.0, 12.0), t + k, 6.0));
            assert!(e.accepted && e.point() == held && l.holding(), "{k}: {e:?}");
        }
    }

    #[test]
    fn a_step_counter_silent_for_ten_minutes_stays_present() {
        // Ruling FR-N1: Android's counter reports only when the count changes, so a player standing for ten minutes gets no events. The
        // counter stays present (no time window) and 12 m fixes still do not end the hold; the 15 m quiet bound covers a dead sensor.
        let (mut l, t) = held_with_a_quiet_step_counter();
        let t = stand(&mut l, t, 600);
        assert!(l.holding() && l.steps().present(t * 1000, l.params().steps_present_ms));
        let held = l.last().unwrap().point();
        for k in 0..10 {
            let e = l.on_fix(&fix(destination(o(), 90.0, 12.0), t + k, 6.0));
            assert!(e.accepted && e.point() == held && l.holding(), "{k}: {e:?}");
        }
    }

    #[test]
    fn a_mover_without_steps_ends_a_quiet_hold_past_the_quiet_limit() {
        // Ruling FR-C1: with the counter quiet, fixes past `hold_quiet_max_m` end the hold (a wheelchair at 1 m/s), and the shown
        // position never stays more than about that far behind.
        let (mut l, t) = held_with_a_quiet_step_counter();
        let mut ended = None;
        for k in 1..=60 {
            l.on_steps(100, (t + k) * 1000, None);
            let truth = destination(o(), 90.0, i64_to_f64(k));
            let e = l.on_fix(&fix(truth, t + k, 5.0));
            assert!(distance_m(e.point(), truth) < 25.0, "{k}: {:.1} m behind", distance_m(e.point(), truth));
            if ended.is_none() && !l.holding() {
                ended = Some(k);
            }
        }
        assert!(ended.is_some_and(|k| k <= 30), "{ended:?}");
    }

    #[test]
    fn a_vehicle_leaving_ends_a_quiet_hold_through_the_relocator() {
        // Ruling T8-R2: with the counter quiet only consistent displaced fixes (a bus or car pulling away) end the hold, as a relocation.
        let (mut l, t) = held_with_a_quiet_step_counter();
        let v: Vec<Verdict> = (1..=8).map(|k| l.on_fix(&fix(destination(o(), 90.0, 10.0 * i64_to_f64(k)), t + k, 6.0)).verdict).collect();
        let k = v.iter().position(|v| *v == Verdict::Relocated).unwrap_or_else(|| panic!("{v:?}"));
        assert!(k < 6 && !l.holding(), "{v:?}");
    }

    #[test]
    fn the_locator_says_which_fix_ended_a_hold() {
        // Ruling T8-R18: the odometer counts the chord of a relocation that ended a hold, so the locator says which fix ended it: here a
        // car pulling away from a quiet hold (a relocation) and, without a step counter, a walk away (far fixes).
        let (mut l, t) = held_with_a_quiet_step_counter();
        let ended: Vec<(i64, Verdict)> = (1..=8)
            .filter_map(|k| {
                let e = l.on_fix(&fix(destination(o(), 90.0, 10.0 * i64_to_f64(k)), t + k, 6.0));
                l.ended_hold().then_some((k, e.verdict))
            })
            .collect();
        assert!(matches!(ended[..], [(_, Verdict::Relocated)]), "{ended:?}");
        let mut l = Locator::default();
        let t = stand(&mut l, 0, 40);
        assert!(l.holding());
        let ended: Vec<i64> =
            (1..=10).filter(|&k| l.on_fix(&fix(destination(o(), 90.0, 8.0 + 1.4 * i64_to_f64(k)), t + k, 6.0)).accepted && l.ended_hold()).collect();
        assert_eq!(ended.len(), 1, "{ended:?}");
    }

    #[test]
    fn a_run_zone_departure_from_a_quiet_hold_counts_its_distance() {
        // Ruling T8-R20: a runner leaves a quiet hold at 5 m/s with the counter seeing no steps (phone in a bag). The hold ends as a
        // relocation; its chord is judged by the agreeing fixes' own speed (5 m/s, within the Run cap), not by the time since the frozen
        // point, so the distance counts.
        let (mut l, t) = held_with_a_quiet_step_counter();
        l.set_mode(Mode::Run);
        let mut odo = Odometer::new(imm::mode_cap_mps(Mode::Run));
        odo.step(&l.last().unwrap());
        let mut total = 0.0;
        for k in 1..=30 {
            let e = l.on_fix(&fix(destination(o(), 90.0, 5.0 * i64_to_f64(k)), t + k, 6.0));
            total += odo.step_with(&e, l.hold_relocation_speed());
        }
        assert!((total - 150.0).abs() <= 15.0, "odometer {total:.1} m of 150 m");
    }
}
