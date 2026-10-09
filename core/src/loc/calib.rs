//! Step length: the cadence model `L = clamp(0.25 + 0.25 f, 0.5, 1.1)` m times a per-phone, per-source scale `k`, learned against good GPS
//! (1-D Kalman) and saved by the app, never in the game save.

use serde::{Deserialize, Serialize};

use crate::geo::{distance_m, Point};
use crate::loc::{Estimate, LocParams, Motion, Source, StepHistory, Verdict};
use crate::num::i64_to_f64;

/// The step source of the phone's own step counter (Android `TYPE_STEP_COUNTER`; iOS `CMPedometer` will be `phone.pedometer`).
pub const PHONE_STEP_COUNTER: &str = "phone.step_counter";

/// A window spans at most this long, ms: [`StepHistory`] keeps two minutes of readings, and a window must start inside them.
const MAX_WINDOW_MS: i64 = 110_000;

/// Step length of the cadence model at `cadence_hz` steps per second, metres (before the scale).
#[must_use]
pub fn step_length_m(cadence_hz: f64) -> f64 {
    (0.25 + 0.25 * cadence_hz).clamp(0.5, 1.1)
}

/// A saved step calibration of one step source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepCal {
    /// Step source id.
    pub source: String,
    /// Scale on the cadence model.
    pub k: f64,
    /// Variance of `k`.
    pub var_k: f64,
    /// Windows learned from.
    pub samples: u32,
    /// Last update, Unix ms.
    pub updated_ms: i64,
}

impl StepCal {
    /// The default for `source`: `k = 1`, `var_k = 0.15^2`.
    #[must_use]
    pub fn default_for(source: &str) -> Self {
        Self { source: source.to_string(), k: 1.0, var_k: 0.15 * 0.15, samples: 0, updated_ms: 0 }
    }
}

/// Learns `k` of the phone's step counter from windows of walking with good GPS.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibrator {
    cal: StepCal,
    window: Vec<(i64, Point, f64)>,
    walked_ms: i64,
    misses: u32,
    last_window: Option<(f64, f64)>,
    /// The CUSUM's upper and lower sums.
    cusum: (f64, f64),
    /// How often the CUSUM declared a carry change.
    cusum_fires: u32,
}

impl Default for Calibrator {
    fn default() -> Self {
        Self { cal: StepCal::default_for(PHONE_STEP_COUNTER), window: vec![], walked_ms: 0, misses: 0, last_window: None, cusum: (0.0, 0.0), cusum_fires: 0 }
    }
}

impl Calibrator {
    /// The calibration now.
    #[must_use]
    pub fn cal(&self) -> StepCal {
        self.cal.clone()
    }

    /// Load a saved calibration, its scale clamped to `calib_k_min..=calib_k_max`; false (ignored) when it is for another step source or
    /// its scale or variance is not a finite number (the variance also positive).
    pub fn set(&mut self, cal: StepCal, p: &LocParams) -> bool {
        if cal.source != PHONE_STEP_COUNTER || !cal.k.is_finite() || !cal.var_k.is_finite() || cal.var_k <= 0.0 {
            return false;
        }
        self.cal = StepCal { k: cal.k.clamp(p.calib_k_min, p.calib_k_max), ..cal };
        true
    }

    /// Forget the window being built: the next estimate starts a new one (a hold, a filter restart).
    pub fn break_window(&mut self) {
        self.window.clear();
    }

    /// The scale.
    #[must_use]
    pub fn k(&self) -> f64 {
        self.cal.k
    }

    /// Its sigma.
    #[must_use]
    pub fn sigma_k(&self) -> f64 {
        self.cal.var_k.max(0.0).sqrt()
    }

    /// How often the CUSUM declared a carry change.
    #[must_use]
    pub fn cusum_fires(&self) -> u32 {
        self.cusum_fires
    }

    /// Feed an estimate: accepted walking GPS estimates of at most `calib_max_unc_m` build a window; a window of `calib_window_ms` and
    /// `calib_min_dist_m` gives one observation `k_obs = d / (steps x L(cadence))` with sigma `calib_obs_scale x mean uncertainty / (steps
    /// x L(cadence) x k)`.
    pub fn on_estimate(&mut self, e: &Estimate, steps: &StepHistory, p: &LocParams) {
        let walking_steps = steps.present(e.t_ms, p.steps_present_ms) && !steps.quiet(e.t_ms, p.steps_quiet_window_ms);
        let good = e.accepted && e.source == Source::Gps && e.motion == Motion::Walking && e.uncertainty_m <= p.calib_max_unc_m && walking_steps;
        if !good {
            self.break_window();
            return;
        }
        // Nothing joins a window across a filter restart, a gap between fixes, or more time than the step history keeps (ruling
        // T21-I1): the positions could not see how the player got there, the steps could.
        let joins = self.window.first().zip(self.window.last()).is_some_and(|(first, last)| {
            !matches!(e.verdict, Verdict::Reset | Verdict::Relocated) && e.t_ms - last.0 <= p.calib_max_gap_ms && e.t_ms - first.0 <= MAX_WINDOW_MS
        });
        if !joins {
            self.window.clear();
        }
        self.window.push((e.t_ms, e.point(), e.uncertainty_m));
        let (Some(first), Some(last)) = (self.window.first().copied(), self.window.last().copied()) else { return };
        let span_ms = last.0 - first.0;
        let d = path_m(&self.window, p.calib_path_step_m);
        if span_ms < p.calib_window_ms || d < p.calib_min_dist_m {
            return;
        }
        let n = steps.gained(first.0, last.0);
        self.window = vec![last];
        if n <= 0 {
            return;
        }
        let secs = i64_to_f64(span_ms) / 1000.0;
        let cadence = i64_to_f64(n) / secs;
        let mean_unc = f64::midpoint(first.2, last.2);
        let k_obs = d / (i64_to_f64(n) * step_length_m(cadence));
        // The sigma uses the distance the steps predict, not the measured one: weighting by the measured d^2 favours windows whose
        // distance reads long (ruling T21-R5).
        let predicted_m = i64_to_f64(n) * step_length_m(cadence) * self.cal.k;
        let var_obs = (p.calib_obs_scale * mean_unc / predicted_m).powi(2);
        self.update(k_obs, var_obs, cadence, d / secs, span_ms, last.0, p);
    }

    #[allow(clippy::too_many_arguments)] // one observation and its context
    fn update(&mut self, k_obs: f64, var_obs: f64, cadence: f64, speed: f64, span_ms: i64, t_ms: i64, p: &LocParams) {
        let c = &mut self.cal;
        c.var_k += p.calib_q_per_min * i64_to_f64(span_ms) / 60_000.0;
        if self.walked_ms < p.calib_session_ms {
            c.var_k = c.var_k.max(p.calib_session_var); // re-validate a stored value in every session
        }
        self.walked_ms += span_ms;
        // Two-sided CUSUM on the normalized residual (ruling T21-R2): a small shift that never misses by 3 sigma still adds up.
        let z = (k_obs - c.k) / var_obs.sqrt();
        let (up, down) = self.cusum;
        self.cusum = ((up + z - p.calib_cusum_drift).max(0.0), (down - z - p.calib_cusum_drift).max(0.0));
        let shifted = self.cusum.0 > p.calib_cusum_h || self.cusum.1 > p.calib_cusum_h;
        if shifted {
            self.cusum_fires += 1;
        }
        let far = (k_obs - c.k).abs() > p.calib_adapt_sigmas * (c.var_k + var_obs).sqrt();
        self.misses = if far { self.misses + 1 } else { 0 };
        let cadence_jump =
            self.last_window.is_some_and(|(f0, v0)| (speed - v0).abs() <= p.calib_same_speed * v0 && (cadence - f0).abs() > p.calib_cadence_jump * f0);
        if self.misses >= 2 || cadence_jump || shifted {
            c.var_k = c.var_k.max(p.calib_adapt_var); // the carry changed: let the next windows dominate
            self.misses = 0; // every rule starts over after an adapt (ruling T21-M2)
            self.cusum = (0.0, 0.0);
        }
        let gain = c.var_k / (c.var_k + var_obs);
        c.k = (c.k + gain * (k_obs - c.k)).clamp(p.calib_k_min, p.calib_k_max);
        c.var_k *= 1.0 - gain;
        c.samples += 1;
        c.updated_ms = t_ms;
        self.last_window = Some((cadence, speed));
    }
}

/// The length of the path through `window`'s points, keeping a point only when it is at least `step_m` from the last one kept; the
/// last point replaces the last kept one when it is closer than that, so no short, jitter-sized segment ends the path. Metres.
fn path_m(window: &[(i64, Point, f64)], step_m: f64) -> f64 {
    let mut kept: Vec<Point> = Vec::with_capacity(window.len());
    for &(_, q, _) in window {
        if kept.last().is_none_or(|&a| distance_m(a, q) >= step_m) {
            kept.push(q);
        }
    }
    if let Some(&(_, end, _)) = window.last() {
        match kept.len() {
            n if n > 1 && kept[n - 1] != end => kept[n - 1] = end,
            1 if kept[0] != end => kept.push(end),
            _ => {}
        }
    }
    kept.windows(2).map(|w| distance_m(w[0], w[1])).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    #[test]
    fn the_cadence_model_is_the_spec_formula_clamped() {
        assert!((step_length_m(1.8) - 0.70).abs() < 1e-9 && (step_length_m(2.8) - 0.95).abs() < 1e-9);
        assert!((step_length_m(0.5) - 0.5).abs() < 1e-9 && (step_length_m(5.0) - 1.1).abs() < 1e-9);
    }

    fn walk(c: &mut Calibrator, scale: f64, from_s: i64, secs: i64, steps0: i64) -> i64 {
        walk_jittered(c, scale, from_s, secs, steps0, 0.0)
    }

    /// [`walk`] with each estimate `lateral_m` to the left or right of the line, alternating (filter jitter).
    #[allow(clippy::many_single_char_names)] // a test walk: params, history, origin, cadence, calibrator
    fn walk_jittered(c: &mut Calibrator, scale: f64, from_s: i64, secs: i64, steps0: i64, lateral_m: f64) -> i64 {
        let p = LocParams::default();
        let mut h = StepHistory::default();
        let o = Point::new(40.0, -111.0);
        let f = crate::loc::bench::cadence_for(1.4, scale);
        let mut total = steps0;
        for s in 0..=secs {
            let t = (from_s + s) * 1000;
            if s % 2 == 0 {
                h.push(total, t);
            }
            total = steps0 + crate::num::round_i64(f * i64_to_f64(s));
            let side = if (from_s + s) % 2 == 0 { 0.0 } else { 180.0 };
            let at = destination(destination(o, 90.0, 1.4 * i64_to_f64(from_s + s)), side, lateral_m);
            c.on_estimate(&Estimate { uncertainty_m: 4.0, speed_mps: 1.4, ..Estimate::exact(at.lat, at.lon, t) }, &h, &p);
        }
        total
    }

    #[test]
    fn walking_with_good_gps_learns_the_scale_and_clamps_it() {
        let mut c = Calibrator::default();
        walk(&mut c, 0.85, 0, 300, 1000);
        assert!((c.k() - 0.85).abs() < 0.05, "{}", c.k());
        assert!(c.cal().samples >= 4, "one per 60 s / 80 m window (ruling T21-R5): {}", c.cal().samples);
        let mut tiny = Calibrator::default();
        walk(&mut tiny, 0.4, 0, 300, 1000);
        assert!((tiny.k() - 0.6).abs() < 1e-9, "clamped to 0.6: {}", tiny.k());
    }

    #[test]
    fn a_stored_value_is_rechecked_in_a_new_session_and_a_carry_change_adapts_fast() {
        let mut c = Calibrator::default();
        assert!(c.set(StepCal { var_k: 0.0001, k: 1.0, ..StepCal::default_for(PHONE_STEP_COUNTER) }, &LocParams::default()));
        walk(&mut c, 0.85, 0, 60, 1000); // the first window (60 s, ruling T21-R5)
        assert!(c.sigma_k() >= 0.05, "the first minutes of a session keep var_k >= 0.1^2 or adapt: {}", c.sigma_k());
        walk(&mut c, 0.85, 61, 270, 2000);
        assert!((c.k() - 0.85).abs() < 0.05, "{}", c.k());
    }

    #[test]
    fn jitter_between_estimates_is_not_walked_distance() {
        let mut c = Calibrator::default();
        walk_jittered(&mut c, 1.0, 0, 300, 1000, 1.0);
        assert!((c.k() - 1.0).abs() < 0.05, "the path between points calib_path_step_m apart (ruling T21-R1): {}", c.k());
    }

    #[test]
    fn the_cusum_stays_quiet_on_a_steady_walk_and_fires_on_a_sustained_shift() {
        let mut c = Calibrator::default();
        let n = walk(&mut c, 1.0, 0, 600, 1000);
        assert_eq!(c.cusum_fires(), 0, "steady");
        walk(&mut c, 0.85, 601, 180, n);
        assert!(c.cusum_fires() >= 1, "a 15 % shift trips it (ruling T21-R2)");
        assert!((c.k() - 0.85).abs() < 0.05, "{}", c.k());
    }

    /// A walk at `speed` m/s with steps of scale 1 every 2 s for `secs` seconds; `at(s)` is the estimate's point and verdict at second
    /// `s`, `None` for no fix (the steps go on).
    #[allow(clippy::many_single_char_names)] // a test walk: params, history, cadence, calibrator, point
    fn feed(c: &mut Calibrator, secs: i64, speed: f64, at: impl Fn(i64) -> Option<(Point, Verdict)>) {
        let p = LocParams::default();
        let mut h = StepHistory::default();
        let f = crate::loc::bench::cadence_for(speed, 1.0);
        for s in 0..=secs {
            let t = s * 1000;
            if s % 2 == 0 {
                h.push(1000 + crate::num::round_i64(f * i64_to_f64(s)), t);
            }
            if let Some((q, verdict)) = at(s) {
                c.on_estimate(&Estimate { uncertainty_m: 4.0, speed_mps: speed, verdict, ..Estimate::exact(q.lat, q.lon, t) }, &h, &p);
            }
        }
    }

    fn east_of_origin(m: f64) -> Point {
        destination(Point::new(40.0, -111.0), 90.0, m)
    }

    #[test]
    fn a_fix_gap_breaks_the_window() {
        // 30 s without fixes, spent walking 21 m north and back: the steps count it, the positions cannot.
        let mut c = Calibrator::default();
        feed(&mut c, 260, 1.4, |s| match s {
            100..130 => None,
            0..100 => Some((east_of_origin(1.4 * i64_to_f64(s)), Verdict::Used)),
            _ => Some((east_of_origin(1.4 * i64_to_f64(s - 30)), Verdict::Used)),
        });
        assert!(c.cal().samples >= 3 && (c.k() - 1.0).abs() < 0.05, "ruling T21-I1: {} after {}", c.k(), c.cal().samples);
    }

    #[test]
    fn a_relocation_breaks_the_window() {
        // Ends right after the window the relocation fell in: the next one would pull k back.
        let mut c = Calibrator::default();
        feed(&mut c, 200, 1.4, |s| {
            let q = east_of_origin(1.4 * i64_to_f64(s));
            Some(match s {
                0..150 => (q, Verdict::Used),
                150 => (destination(q, 0.0, 300.0), Verdict::Relocated),
                _ => (destination(q, 0.0, 300.0), Verdict::Used),
            })
        });
        assert!(c.cal().samples >= 2 && (c.k() - 1.0).abs() < 0.05, "ruling T21-I1: {} after {}", c.k(), c.cal().samples);
    }

    #[test]
    fn a_window_never_outlasts_the_step_history() {
        // At 0.6 m/s, 80 m takes 133 s: more than the two minutes of step readings kept, so the window restarts instead.
        let mut c = Calibrator::default();
        feed(&mut c, 600, 0.6, |s| Some((east_of_origin(0.6 * i64_to_f64(s)), Verdict::Used)));
        assert_eq!(c.cal().samples, 0, "k {}", c.k());
    }

    #[test]
    fn an_adapt_resets_the_cusum_and_the_misses() {
        let p = LocParams::default();
        let mut c = Calibrator { walked_ms: p.calib_session_ms, misses: 1, cusum: (2.0, 0.0), ..Calibrator::default() };
        c.cal.var_k = 0.0001;
        c.update(0.5, 0.0001, 1.9, 1.4, 60_000, 60_000, &p); // a second miss: adapt
        assert_eq!((c.misses, c.cusum), (0, (0.0, 0.0)), "ruling T21-M2");
    }

    #[test]
    fn a_stored_value_must_be_finite_and_is_clamped() {
        let p = LocParams::default();
        let mut c = Calibrator::default();
        let cal = |k, var_k| StepCal { k, var_k, samples: 3, ..StepCal::default_for(PHONE_STEP_COUNTER) };
        for (k, var_k) in [(f64::NAN, 0.01), (1.0, f64::INFINITY), (1.0, 0.0), (1.0, -0.01), (f64::INFINITY, 0.01)] {
            assert!(!c.set(cal(k, var_k), &p), "{k} {var_k}");
        }
        assert!((c.k() - 1.0).abs() < 1e-12 && c.cal().samples == 0, "untouched");
        assert!(c.set(cal(3.0, 0.01), &p));
        assert!((c.k() - p.calib_k_max).abs() < 1e-12, "clamped: {}", c.k());
    }

    #[test]
    fn calibrations_of_other_sources_never_touch_the_phone_one() {
        let mut c = Calibrator::default();
        assert!(!c.set(StepCal { k: 1.3, ..StepCal::default_for("watch.health_connect") }, &LocParams::default()));
        assert_eq!(c.cal().source, PHONE_STEP_COUNTER);
        assert!((c.k() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn blurry_or_standing_estimates_never_calibrate() {
        let p = LocParams::default();
        let mut c = Calibrator::default();
        let h = StepHistory::default();
        for t in 0..100 {
            c.on_estimate(&Estimate { uncertainty_m: 20.0, ..Estimate::exact(40.0, -111.0 + 1e-5 * f64::from(t), i64::from(t) * 1000) }, &h, &p);
        }
        assert_eq!(c.cal().samples, 0);
    }
}
