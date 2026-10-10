//! The reference track of a real walk (no ground truth exists): an RTS-smoothed track of the good fixes (the replay's default), or the
//! good fixes themselves.

use crate::geo::{bearing_deg, distance_m};
use crate::loc::bench::TruthPoint;
use crate::loc::frame::Frame;
use crate::loc::imm::cv_noise;
use crate::loc::mat::{add, identity, inverse, kalman_update, mul, mul_vec, scale, sub, transpose, Mat};
use crate::loc::{RawFix, ACC_TO_SIGMA};
use crate::num::i64_to_f64;

/// The fixes with accuracy at most `max_acc_m`, with speed and course from the previous good fix.
#[must_use]
pub fn good_fix_reference(fixes: &[RawFix], max_acc_m: f64) -> Vec<TruthPoint> {
    let good: Vec<&RawFix> = fixes.iter().filter(|f| f.accuracy_m <= max_acc_m).collect();
    let mut out: Vec<TruthPoint> = Vec::with_capacity(good.len());
    for (i, f) in good.iter().enumerate() {
        let (speed_mps, course_deg) = match i.checked_sub(1).map(|j| good[j]) {
            Some(p) if f.t_ms > p.t_ms => (distance_m(p.point(), f.point()) / (i64_to_f64(f.t_ms - p.t_ms) / 1000.0), bearing_deg(p.point(), f.point())),
            _ => (0.0, 0.0),
        };
        out.push(TruthPoint { t_ms: f.t_ms, p: f.point(), speed_mps, course_deg });
    }
    out
}

/// The reference track of a real walk: an RTS (forward-backward) smoother of the constant-velocity model (`sigma_a`, m/s^2) over the
/// fixes with accuracy at most `max_acc_m`, forward and backward over the whole recording. One point per good fix.
#[must_use]
#[allow(clippy::many_single_char_names)] // h, p, x, z, c: the Kalman symbols
pub fn rts_reference(fixes: &[RawFix], max_acc_m: f64, sigma_a: f64) -> Vec<TruthPoint> {
    let good: Vec<&RawFix> = fixes.iter().filter(|f| f.accuracy_m <= max_acc_m && f.accuracy_m.is_finite()).collect();
    let Some(first) = good.first() else { return vec![] };
    let frame = Frame::new(first.point());
    let h: Mat<2, 4> = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]];
    let sigma_of = |f: &RawFix| (f.accuracy_m / ACC_TO_SIGMA).max(2.0);
    let (s0, z0) = (sigma_of(first), frame.to_enu(first.point()));
    let mut x = [z0[0], z0[1], 0.0, 0.0];
    let mut p: Mat<4, 4> = [[s0 * s0, 0.0, 0.0, 0.0], [0.0, s0 * s0, 0.0, 0.0], [0.0, 0.0, 4.0, 0.0], [0.0, 0.0, 0.0, 4.0]];
    // Filtered (xs, ps) and predicted (xp, pp) states, and the transition into each step (fs).
    let (mut xs, mut ps, mut xp, mut pp, mut fs) = (vec![x], vec![p], vec![x], vec![p], vec![identity::<4>()]);
    for w in good.windows(2) {
        let (prev, f) = (w[0], w[1]);
        let (sigma, z) = (sigma_of(f), frame.to_enu(f.point()));
        let dt = i64_to_f64(f.t_ms - prev.t_ms) / 1000.0;
        let fm: Mat<4, 4> = [[1.0, 0.0, dt, 0.0], [0.0, 1.0, 0.0, dt], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        x = mul_vec(&fm, &x);
        p = add(&mul(&mul(&fm, &p), &transpose(&fm)), &cv_noise(sigma_a, dt));
        xp.push(x);
        pp.push(p);
        fs.push(fm);
        if let Some(u) = kalman_update(&x, &p, &z, &h, &scale(&identity::<2>(), sigma * sigma)) {
            x = u.x;
            p = u.p;
        }
        xs.push(x);
        ps.push(p);
    }
    for k in (0..xs.len().saturating_sub(1)).rev() {
        let Some((pp_inv, _)) = inverse(&pp[k + 1]) else { continue };
        let c = mul(&mul(&ps[k], &transpose(&fs[k + 1])), &pp_inv);
        let dx: [f64; 4] = std::array::from_fn(|i| xs[k + 1][i] - xp[k + 1][i]);
        let corr = mul_vec(&c, &dx);
        xs[k] = std::array::from_fn(|i| xs[k][i] + corr[i]);
        ps[k] = add(&ps[k], &mul(&mul(&c, &sub(&ps[k + 1], &pp[k + 1])), &transpose(&c)));
    }
    good.iter()
        .zip(&xs)
        .map(|(f, x)| TruthPoint {
            t_ms: f.t_ms,
            p: frame.to_geo([x[0], x[1]]),
            speed_mps: x[2].hypot(x[3]),
            course_deg: x[2].atan2(x[3]).to_degrees().rem_euclid(360.0),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_provisional_reference_keeps_only_good_fixes_and_gives_them_a_speed() {
        let f = |t: i64, lat: f64, acc: f64| RawFix::at(lat, -111.0, t, acc);
        let r = good_fix_reference(&[f(0, 40.0, 5.0), f(1000, 40.000_01, 30.0), f(2000, 40.000_02, 5.0)], 15.0);
        assert_eq!(r.len(), 2);
        assert!((r[1].speed_mps - 1.11).abs() < 0.05, "2.2 m in 2 s: {}", r[1].speed_mps);
    }

    #[test]
    fn the_rts_reference_is_smoother_than_the_fixes_and_close_to_the_truth() {
        // Ruling T8-R1: no smoother removes the generator's common-mode drift, so the 0.7 ratio is checked on white noise (tau 1 s) and,
        // on the default (drifting) generator, the smoother must do no worse than the forward filter.
        use crate::geo::Point;
        use crate::loc::bench::{interp, position_error, run_locator, Leg, ReplayOpts, Scenario, Shown};
        use crate::loc::Verdict;
        let s = Scenario::walk(Point::new(40.0, -111.0), vec![Leg::Move { bearing_deg: 90.0, dist_m: 420.0, speed_mps: 1.4 }], 8.0);
        let white = Scenario { ar_tau_s: 1.0, ..s.clone() }.generate(3);
        let refr = rts_reference(&white.fixes, 15.0, 0.5);
        let err = |p: Point, t: i64| distance_m(p, interp(&white.truth, t));
        // Finding M8: mean errors over each vector's own length.
        let raw: f64 = white.fixes.iter().map(|f| err(f.point(), f.t_ms)).sum::<f64>() / crate::num::count_f64(white.fixes.len());
        let smooth: f64 = refr.iter().map(|t| err(t.p, t.t_ms)).sum::<f64>() / crate::num::count_f64(refr.len());
        assert!(smooth < raw * 0.7, "smoothed {smooth} vs raw {raw}");
        // Rulings T8-R12 and T8-R16: one point's course is a coin flip; the median course error at index 150 over seeds 1 to 20 is
        // checked (15 deg; the brief's smoother measures 12.9).
        assert!((refr[150].speed_mps - 1.4).abs() < 0.3, "{:?}", refr[150]);
        let course_err: Vec<f64> = (1..=20)
            .map(|seed| {
                let w = Scenario { ar_tau_s: 1.0, ..s.clone() }.generate(seed);
                (rts_reference(&w.fixes, 15.0, 0.5)[150].course_deg - 90.0).abs()
            })
            .collect();
        let median = crate::loc::bench::percentile(&course_err, 0.5);
        assert!(median <= 15.0, "median course error {median:.1} deg: {course_err:?}");
        let r = s.generate(3);
        let as_shown =
            |t: &TruthPoint| Shown { t_ms: t.t_ms, p: t.p, uncertainty_m: 1.0, accepted: true, verdict: Verdict::Used, course_deg: None, odometer_m: 0.0 };
        let smoothed: Vec<Shown> = rts_reference(&r.fixes, 15.0, 0.5).iter().map(as_shown).collect();
        let forward = run_locator(&r.fixes, &[], &[], &ReplayOpts::default());
        let (rts_rms, fwd_rms) = (position_error(&r.truth, &smoothed).0, position_error(&r.truth, &forward.shown).0);
        assert!(rts_rms <= fwd_rms, "smoother {rts_rms} vs forward filter {fwd_rms}");
    }
}
