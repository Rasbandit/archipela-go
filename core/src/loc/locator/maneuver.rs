//! Maneuvers and course: the line fit through the last used fixes, and the course from the moving models.

use crate::loc::imm::{self, Imm, F, W};
use crate::loc::params::LocParams;
use crate::num::i64_to_f64;

use super::Locator;

/// Fixes in the maneuver line fit (ruling T8-R8).
pub(super) const LINE_FIT_FIXES: usize = 4;

/// A fix the filter used, in the frame, with its position sigma (for the maneuver line fit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct UsedFix {
    pub(super) en: [f64; 2],
    pub(super) t_ms: i64,
    pub(super) sigma_m: f64,
}

/// Course (degrees) of the least-squares line through the last [`LINE_FIT_FIXES`] fixes, when there are that many and their fitted speed
/// is significant (ruling T8-R17): at least `min_speed_mps` and more than `min_sigmas` sigmas of the fitted slope, `sigma / sqrt(sum (t -
/// t_mean)^2)` with `sigma` the fixes' rms position sigma. With the course's sigma, `atan(slope sigma / speed)`, degrees.
pub(super) fn line_fit_course(fixes: &[UsedFix], min_speed_mps: f64, min_sigmas: f64) -> Option<(f64, f64)> {
    let pts = fixes.get(fixes.len().checked_sub(LINE_FIT_FIXES)?..)?;
    let n = crate::num::count_f64(pts.len());
    let t = |u: &UsedFix| i64_to_f64(u.t_ms) / 1000.0;
    let tm = pts.iter().map(t).sum::<f64>() / n;
    let den: f64 = pts.iter().map(|u| (t(u) - tm).powi(2)).sum();
    if den <= 0.0 {
        return None;
    }
    let slope = |axis: usize| {
        let mean = pts.iter().map(|u| u.en[axis]).sum::<f64>() / n;
        pts.iter().map(|u| (t(u) - tm) * (u.en[axis] - mean)).sum::<f64>() / den
    };
    let (ve, vn) = (slope(0), slope(1));
    let slope_sigma = (pts.iter().map(|u| u.sigma_m * u.sigma_m).sum::<f64>() / n).sqrt() / den.sqrt();
    let speed = ve.hypot(vn);
    let sigma_deg = (slope_sigma / speed).atan().to_degrees();
    (speed >= min_speed_mps && speed > min_sigmas * slope_sigma).then(|| (ve.atan2(vn).to_degrees().rem_euclid(360.0), sigma_deg))
}

pub(super) fn wrap_deg(d: f64) -> f64 {
    (d + 540.0).rem_euclid(360.0) - 180.0
}

impl Locator {
    /// Judge the maneuver on the used fixes plus `fix` (when given), after `before` disagreeing fixes: sets the disagreement count and
    /// the reported maneuver course, and returns whether the line fit turned away from `filter_course`. The line moving while the filter
    /// has no course is the largest disagreement (ruling T8-R14), when the moving models are likely (`moving_mu`, ruling T8-R17).
    pub(super) fn judge_maneuver(&mut self, fix: Option<UsedFix>, filter_course: Option<f64>, moving_mu: f64, before: u32) -> bool {
        let p = &self.params;
        let pts: Vec<UsedFix> = self.used.iter().copied().chain(fix).collect();
        let line = line_fit_course(&pts, p.course_min_speed_mps, p.maneuver_min_sigmas);
        let turned = matches!((line, filter_course), (Some((a, _)), Some(b)) if wrap_deg(a - b).abs() > p.maneuver_course_deg);
        let unseen = line.is_some() && filter_course.is_none() && moving_mu >= p.course_min_moving_mu;
        let disagrees = self.hold.is_none() && (turned || unseen);
        self.disagreeing = if disagrees { before + 1 } else { 0 };
        let from_ms = pts.len().checked_sub(LINE_FIT_FIXES).and_then(|i| pts.get(i)).map_or(0, |u| u.t_ms);
        self.maneuver_course = line.filter(|_| self.disagreeing >= p.maneuver_fixes).map(|(c, s)| (c, (!turned).then_some((s, from_ms))));
        turned
    }

    /// Direction of travel from the moving models (W and F) alone, so the stationary model's zero velocity does not drag it: reported
    /// when they are likely enough, fast enough and sure enough of the direction (ruling T8-R5). With its sigma, degrees.
    pub(super) fn course_of(imm: &Imm, p: &LocParams) -> Option<(f64, f64)> {
        let moving_mu = imm.mu[W] + imm.mu[F];
        if moving_mu < p.course_min_moving_mu {
            return None;
        }
        let m = imm::combine(&imm.models, &[0.0, imm.mu[W] / moving_mu, imm.mu[F] / moving_mu]);
        let (ve, vn) = (m.x[2], m.x[3]);
        let speed = ve.hypot(vn);
        if speed < p.course_min_speed_mps {
            return None;
        }
        let vb = imm::vel_block(&m.p);
        let w = [-vn / speed, ve / speed]; // across the direction of travel
        let cross = w[0] * w[0] * vb[0][0] + 2.0 * w[0] * w[1] * vb[0][1] + w[1] * w[1] * vb[1][1];
        let sigma_deg = (cross.max(0.0).sqrt() / speed).atan().to_degrees();
        (sigma_deg < p.course_max_sigma_deg).then(|| (ve.atan2(vn).to_degrees().rem_euclid(360.0), sigma_deg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::geo::destination;

    use crate::loc::locator::maneuver::wrap_deg;
    use crate::loc::locator::test_util::*;
    use crate::loc::locator::Locator;
    use crate::loc::Verdict;
    use crate::num::i64_to_f64;

    #[test]
    fn the_course_comes_from_the_moving_models_while_walking_and_after_a_turn() {
        // Ruling T8-R5: course and its sigma come from the W+F conditioned estimate (the stationary model's zero velocity would drag it),
        // reported when mu_W + mu_F >= 0.5 and that speed >= 0.5 m/s, with a 35 deg sigma gate. (The 4 s turn threshold is the corner
        // scenario's, on 20 seeds.)
        use crate::loc::bench::gauss;
        use rand::rngs::StdRng;
        use rand::SeedableRng;
        let mut rng = StdRng::seed_from_u64(7);
        let mut l = Locator::default();
        let corner = destination(o(), 90.0, 42.0);
        let mut seen = None;
        for k in 0..60 {
            let truth = if k <= 30 { destination(o(), 90.0, 1.4 * i64_to_f64(k)) } else { destination(corner, 0.0, 1.4 * i64_to_f64(k - 30)) };
            let noisy = destination(destination(truth, 0.0, 1.5 * gauss(&mut rng)), 90.0, 1.5 * gauss(&mut rng));
            let e = l.on_fix(&fix(noisy, k, 5.0));
            if k == 29 {
                assert!(e.course_deg.is_some_and(|c| (c - 90.0).abs() < 20.0), "walking east: {e:?}");
            }
            if k > 30 && seen.is_none() && e.course_deg.is_some_and(|c| ((c + 540.0) % 360.0 - 180.0).abs() < 20.0) {
                seen = Some(k - 30);
            }
        }
        assert!(seen.is_some_and(|s| s <= 8), "turned after {seen:?} s");
    }

    #[test]
    fn a_corner_at_running_speed_is_seen_by_the_line_fit() {
        // Rulings T8-R8, R17 (kept by the R19 fallback): at 3.5 m/s and 3 m accuracy the line through the last 4 used fixes is
        // significant, so after a 90 deg corner its course is reported and the shown course turns within 4 s.
        let mut l = Locator::default();
        let corner = destination(o(), 90.0, 105.0);
        let (mut seen, mut fired) = (None, false);
        for k in 0..50 {
            let p = if k <= 30 { destination(o(), 90.0, 3.5 * i64_to_f64(k)) } else { destination(corner, 0.0, 3.5 * i64_to_f64(k - 30)) };
            let e = l.on_fix(&fix(p, k, 3.0));
            fired |= k > 30 && l.maneuver_course.is_some();
            if k > 30 && seen.is_none() && e.course_deg.is_some_and(|c| wrap_deg(c).abs() < 20.0) {
                seen = Some(k - 30);
            }
        }
        assert!(fired, "the line fit reported the turn");
        assert!(seen.is_some_and(|s| s <= 4), "turned after {seen:?} s");
    }

    #[test]
    fn a_gated_fix_is_no_part_of_the_line_fit() {
        // Finding M3: the spike is rejected, so the course stays the walk's (east), not the line through the spike.
        let mut l = Locator::default();
        let t = walk(&mut l, 1, 20);
        let e = l.on_fix(&fix(destination(o(), 0.0, 150.0), t, 5.0));
        assert_eq!(e.verdict, Verdict::Gated);
        assert!(e.course_deg.is_some_and(|c| (c - 90.0).abs() < 20.0), "{e:?}");
    }

    #[test]
    fn a_straight_walk_is_no_maneuver() {
        // Ruling T8-R17: the line fit's course counts only when its speed is significant (over 2 sigma of the fitted slope, from the
        // fixes' accuracies), so on a straight walk the maneuver fires on at most 5 % of the fixes.
        use crate::loc::bench::{Leg, Scenario};
        let s = Scenario::walk(o(), vec![Leg::Move { bearing_deg: 90.0, dist_m: 420.0, speed_mps: 1.4 }], 5.0);
        let (mut fired, mut n) = (0_usize, 0_usize);
        for seed in 0..20 {
            let mut l = Locator::default();
            for f in &s.generate(seed).fixes {
                l.on_fix(f);
                fired += usize::from(l.maneuver_course.is_some());
                n += 1;
            }
        }
        let share = crate::num::count_f64(fired) / crate::num::count_f64(n);
        println!("maneuver on {fired} of {n} fixes ({share:.3})");
        assert!(share <= 0.05, "maneuver on {fired} of {n} fixes ({share:.3})");
    }

    #[test]
    fn a_line_fit_needs_four_used_fixes_and_a_walking_speed() {
        let at = |t_s: i64, e: f64, n: f64| UsedFix { en: [e, n], t_ms: t_s * 1000, sigma_m: 0.1 };
        let line = |v: &[UsedFix]| line_fit_course(v, 0.5, 2.0).map(|(c, _)| c);
        assert_eq!(line(&[at(0, 0.0, 0.0), at(1, 0.0, 1.4), at(2, 0.0, 2.8)]), None, "three fixes");
        assert!(line(&[at(0, 0.0, 0.0), at(1, 1.4, 0.0), at(2, 2.8, 0.0), at(3, 4.2, 0.0)]).is_some_and(|c| (c - 90.0).abs() < 1e-9));
        assert_eq!(line(&[at(0, 0.0, 0.0), at(1, 0.1, 0.0), at(2, 0.2, 0.0), at(3, 0.3, 0.0)]), None, "0.1 m/s is standing");
        // Ruling T8-R17: 1.4 m/s is under 2 sigma of the slope (3.3 m / sqrt(5) = 1.48 m/s) with 5 m fixes; 3.5 m/s is over it.
        let noisy = |v: f64| (0..4).map(|k| UsedFix { en: [v * f64::from(k), 0.0], t_ms: i64::from(k) * 1000, sigma_m: 3.3 }).collect::<Vec<_>>();
        assert_eq!(line(&noisy(1.4)), None, "within the noise");
        assert!(line(&noisy(3.5)).is_some_and(|c| (c - 90.0).abs() < 1e-9));
    }

    #[test]
    fn a_walk_start_in_noisy_fixes_shows_no_line_course() {
        // Ruling T8-R17 (replaces the R14 walk-start test): 1.4 m/s over 4 fixes of 5 m accuracy is within the noise, so the line fit
        // reports nothing; the course comes from the filter once it is sure.
        let mut l = Locator::default();
        for k in 0..8 {
            l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(k)), k, 5.0));
            assert!(l.maneuver_course.is_none(), "fix {k}");
        }
    }
}
