//! Layer 1: an interacting multiple model (IMM) Kalman filter with three models sharing the state `[e, n, ve, vn]` (metres, m/s, ENU):
//! S stationary, W walking (constant velocity, gentle), F fast (constant velocity, agile).

use crate::catalog::Mode;
use crate::loc::mat::{add, block_diag, identity, kalman_update, mahalanobis2, mul, mul_vec, outer, scale, symmetrize, transpose, zeros, Mat};
use crate::loc::params::LocParams;
use crate::loc::{Provider, RawFix};
use crate::num::i64_to_f64;

/// Index of the stationary model.
pub const S: usize = 0;
/// Index of the walking model.
pub const W: usize = 1;
/// Index of the fast model.
pub const F: usize = 2;

const H_POS: Mat<2, 4> = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]];

/// A state and its covariance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gaussian {
    /// `[e, n, ve, vn]`.
    pub x: [f64; 4],
    /// Covariance.
    pub p: Mat<4, 4>,
}

/// One fix as a measurement in the filter frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meas {
    /// Position, metres east and north.
    pub pos: [f64; 2],
    /// Its covariance.
    pub r_pos: Mat<2, 2>,
    /// Velocity and its covariance, when the fix has a usable one.
    pub vel: Option<([f64; 2], Mat<2, 2>)>,
}

/// The position block of a covariance.
#[must_use]
pub fn pos_block(p: &Mat<4, 4>) -> Mat<2, 2> {
    [[p[0][0], p[0][1]], [p[1][0], p[1][1]]]
}

/// The velocity block of a covariance.
#[must_use]
pub fn vel_block(p: &Mat<4, 4>) -> Mat<2, 2> {
    [[p[2][2], p[2][3]], [p[3][2], p[3][3]]]
}

fn slow(mode: Mode) -> bool {
    matches!(mode, Mode::Walk | Mode::Run)
}

/// The model transition matrix for a step of `dt_s`: per-second rates times `min(dt, max_predict_s)` off the diagonal (scaled down if a row
/// would switch away more than `max_offdiag_share`), the remainder on it.
#[must_use]
pub fn transition(p: &LocParams, mode: Mode, dt_s: f64) -> [[f64; 3]; 3] {
    let rates = if slow(mode) { p.pi_slow } else { p.pi_fast };
    let t = dt_s.clamp(0.0, p.max_predict_s);
    std::array::from_fn(|i| {
        let mut off: [f64; 3] = std::array::from_fn(|j| if i == j { 0.0 } else { rates[i][j] * t });
        let sum: f64 = off.iter().sum();
        if sum > p.max_offdiag_share {
            off = off.map(|v| v * p.max_offdiag_share / sum);
        }
        let sum: f64 = off.iter().sum();
        std::array::from_fn(|j| if i == j { 1.0 - sum } else { off[j] })
    })
}

/// White-noise acceleration of `model` in a zone of `mode` (S has none).
#[must_use]
pub fn sigma_a(p: &LocParams, mode: Mode, model: usize) -> f64 {
    match model {
        W if mode == Mode::Run => p.sigma_a_run,
        W => p.sigma_a_walk,
        F if mode == Mode::Drive => p.sigma_a_drive,
        F => p.sigma_a_bike,
        _ => 0.0,
    }
}

/// Constant-velocity process noise over `dt`: per axis `sigma_a^2 [[dt^4/4, dt^3/2], [dt^3/2, dt^2]]`.
#[must_use]
pub fn cv_noise(sigma_a: f64, dt: f64) -> Mat<4, 4> {
    let s2 = sigma_a * sigma_a;
    let (a, b, c) = (s2 * dt.powi(4) / 4.0, s2 * dt.powi(3) / 2.0, s2 * dt * dt);
    [[a, 0.0, b, 0.0], [0.0, a, 0.0, b], [b, 0.0, c, 0.0], [0.0, b, 0.0, c]]
}

/// Predict `g` under `model` for `dt` seconds.
#[must_use]
pub fn predict(g: &Gaussian, model: usize, dt: f64, sigma_a: f64, p: &LocParams) -> Gaussian {
    if model == S {
        let pb = pos_block(&g.p);
        let q = p.q_stationary_m2_per_s * dt;
        let vs = p.stationary_vel_sigma_mps * p.stationary_vel_sigma_mps;
        let cov = [[pb[0][0] + q, pb[0][1], 0.0, 0.0], [pb[1][0], pb[1][1] + q, 0.0, 0.0], [0.0, 0.0, vs, 0.0], [0.0, 0.0, 0.0, vs]];
        return Gaussian { x: [g.x[0], g.x[1], 0.0, 0.0], p: cov };
    }
    let f: Mat<4, 4> = [[1.0, 0.0, dt, 0.0], [0.0, 1.0, 0.0, dt], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    Gaussian { x: mul_vec(&f, &g.x), p: symmetrize(&add(&mul(&mul(&f, &g.p), &transpose(&f)), &cv_noise(sigma_a, dt))) }
}

/// IMM mixing: each model's starting state is the blend of all models weighted by how likely each switched into it. Returns the mixed
/// states and the predicted model probabilities `c_j = sum_i pi_ij mu_i`; a model nothing switches into (`c_j <= 0`) keeps its own state.
#[must_use]
pub fn mix(models: &[Gaussian; 3], mu: &[f64; 3], pi: &[[f64; 3]; 3]) -> ([Gaussian; 3], [f64; 3]) {
    let c: [f64; 3] = std::array::from_fn(|j| (0..3).map(|i| pi[i][j] * mu[i]).sum());
    let mixed = std::array::from_fn(|j| if c[j] > 0.0 { combine(models, &std::array::from_fn(|i| pi[i][j] * mu[i] / c[j])) } else { models[j] });
    (mixed, c)
}

/// The moment-matched blend of the models: `x = sum w x_j`, `P = sum w (P_j + (x_j - x)(x_j - x)^T)`.
#[must_use]
pub fn combine(models: &[Gaussian; 3], w: &[f64; 3]) -> Gaussian {
    let x: [f64; 4] = std::array::from_fn(|k| (0..3).map(|j| w[j] * models[j].x[k]).sum());
    let mut p = zeros::<4, 4>();
    for (g, wj) in models.iter().zip(w) {
        let d: [f64; 4] = std::array::from_fn(|k| g.x[k] - x[k]);
        p = add(&p, &scale(&add(&g.p, &outer(&d, &d)), *wj));
    }
    Gaussian { x, p: symmetrize(&p) }
}

/// The filter: three models, their probabilities, and the time of the state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Imm {
    /// Per-model states.
    pub models: [Gaussian; 3],
    /// Model probabilities (S, W, F).
    pub mu: [f64; 3],
    /// Time of the state, Unix ms.
    pub t_ms: i64,
}

fn normalized(v: [f64; 3], floor: f64) -> [f64; 3] {
    let s: f64 = v.iter().sum();
    let v = if s > 0.0 && s.is_finite() { v.map(|x| x / s) } else { [1.0 / 3.0; 3] };
    let v = v.map(|x| x.max(floor));
    let s: f64 = v.iter().sum();
    v.map(|x| x / s)
}

impl Imm {
    /// A fresh filter at `z` (metres, ENU) with position sigma `sigma`: velocity 0 with the mode's initial sigma, the mode's model priors.
    #[must_use]
    pub fn new(z: [f64; 2], sigma: f64, t_ms: i64, mode: Mode, p: &LocParams) -> Self {
        let v0 = if slow(mode) { p.v0_slow_mps } else { p.v0_fast_mps };
        let mut cov = zeros::<4, 4>();
        cov[0][0] = sigma * sigma;
        cov[1][1] = sigma * sigma;
        cov[2][2] = v0 * v0;
        cov[3][3] = v0 * v0;
        let g = Gaussian { x: [z[0], z[1], 0.0, 0.0], p: cov };
        Self { models: [g; 3], mu: normalized(if slow(mode) { p.mu0_slow } else { p.mu0_fast }, p.mu_floor), t_ms }
    }

    /// A fresh filter at `z` already moving at `v` (m/s, ENU, per-axis sigma `v_sigma`), with model probabilities `mu`: a relocation
    /// whose agreeing fixes showed the speed (ruling T8-R4).
    #[must_use]
    pub fn moving(z: [f64; 2], sigma: f64, v: [f64; 2], v_sigma: f64, mu: [f64; 3], t_ms: i64, p: &LocParams) -> Self {
        let cov = |vs: f64| {
            let mut c = zeros::<4, 4>();
            (c[0][0], c[1][1], c[2][2], c[3][3]) = (sigma * sigma, sigma * sigma, vs * vs, vs * vs);
            c
        };
        let moving = Gaussian { x: [z[0], z[1], v[0], v[1]], p: cov(v_sigma) };
        let still = Gaussian { x: [z[0], z[1], 0.0, 0.0], p: cov(p.stationary_vel_sigma_mps) };
        Self { models: [still, moving, moving], mu: normalized(mu, p.mu_floor), t_ms }
    }

    /// A filter whose prior is a known position Gaussian (the bridge's cloud, `cov` in m^2) moving at `vel` (m/s; the stationary model
    /// stays still) with model probabilities `mu`, at `t_ms`.
    #[must_use]
    pub fn with_prior(pos: [f64; 2], cov: Mat<2, 2>, vel: [f64; 2], mu: [f64; 3], t_ms: i64, mode: Mode, p: &LocParams) -> Self {
        let mut imm = Self::new(pos, 1.0, t_ms, mode, p);
        for (j, g) in imm.models.iter_mut().enumerate() {
            g.p[0][0] = cov[0][0].max(1.0);
            g.p[0][1] = cov[0][1];
            g.p[1][0] = cov[1][0];
            g.p[1][1] = cov[1][1].max(1.0);
            if j == S {
                // Standing still is standing still (review M10): zero velocity with the stationary model's own sigma.
                let v2 = p.stationary_vel_sigma_mps * p.stationary_vel_sigma_mps;
                (g.p[2][2], g.p[3][3]) = (v2, v2);
            } else {
                g.x = [pos[0], pos[1], vel[0], vel[1]];
            }
        }
        imm.mu = normalized(mu, p.mu_floor);
        imm
    }

    /// Mixed and predicted model states at `t_ms` (gaps over `max_predict_s` are predicted in steps), and the predicted probabilities.
    #[must_use]
    pub fn predict_to(&self, t_ms: i64, mode: Mode, p: &LocParams) -> ([Gaussian; 3], [f64; 3]) {
        let dt = (i64_to_f64(t_ms.saturating_sub(self.t_ms)) / 1000.0).max(0.0);
        let (mut preds, c) = mix(&self.models, &self.mu, &transition(p, mode, dt));
        for (j, g) in preds.iter_mut().enumerate() {
            let mut left = dt;
            while left > 0.0 {
                let step = left.min(p.max_predict_s);
                *g = predict(g, j, step, sigma_a(p, mode, j), p);
                left -= step;
            }
        }
        (preds, c)
    }

    /// Update the predicted models with `m` (its whole R, position and velocity, scaled by `r_scale` for a soft-gated fix), weigh each model's likelihood by `weights_by_model` (step
    /// evidence) and the predicted probabilities `c`. False (and nothing changed) if no model could take the measurement.
    #[allow(clippy::too_many_arguments)] // one IMM step: everything it needs, nothing it keeps
    pub fn update(&mut self, preds: [Gaussian; 3], c: [f64; 3], m: &Meas, r_scale: f64, weights_by_model: [f64; 3], t_ms: i64, mu_floor: f64) -> bool {
        let mut post = preds;
        let mut logs = [f64::NEG_INFINITY; 3];
        for (j, g) in preds.iter().enumerate() {
            let u = match m.vel {
                Some((v, rv)) => kalman_update(&g.x, &g.p, &[m.pos[0], m.pos[1], v[0], v[1]], &identity::<4>(), &scale(&block_diag(&m.r_pos, &rv), r_scale)),
                None => kalman_update(&g.x, &g.p, &m.pos, &H_POS, &scale(&m.r_pos, r_scale)),
            };
            if let Some(u) = u {
                post[j] = Gaussian { x: u.x, p: u.p };
                logs[j] = u.log_likelihood + weights_by_model[j].max(1e-300).ln() + c[j].max(1e-300).ln();
            }
        }
        let best = logs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !best.is_finite() {
            return false;
        }
        self.models = post;
        self.mu = normalized(logs.map(|l| (l - best).exp()), mu_floor);
        self.t_ms = t_ms;
        true
    }

    /// No usable measurement (a gated fix): keep the predictions and the predicted probabilities, move the clock.
    pub fn coast(&mut self, preds: [Gaussian; 3], c: [f64; 3], t_ms: i64, mu_floor: f64) {
        self.models = preds;
        self.mu = normalized(c, mu_floor);
        self.t_ms = t_ms;
    }

    /// The combined estimate.
    #[must_use]
    pub fn output(&self) -> Gaussian {
        combine(&self.models, &self.mu)
    }

    /// Widen every model's position variance to at least `floor_m2` per axis.
    pub fn floor_pos_var(&mut self, floor_m2: f64) {
        for g in &mut self.models {
            (g.p[0][0], g.p[1][1]) = (g.p[0][0].max(floor_m2), g.p[1][1].max(floor_m2));
        }
    }

    /// Move every model's position through `f` (re-anchoring the frame: old ENU -> geo -> new ENU, exact, so nothing jumps).
    pub fn map_positions(&mut self, f: &dyn Fn([f64; 2]) -> [f64; 2]) {
        for g in &mut self.models {
            let [e, n] = f([g.x[0], g.x[1]]);
            g.x[0] = e;
            g.x[1] = n;
        }
    }
}

/// What the gate decided for one fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gate {
    /// Use it; a soft-gated fix has its R inflated by `r_scale` (robust, Huber-like).
    Accept {
        /// R multiplier (1 unless soft).
        r_scale: f64,
        /// Whether it was soft-gated.
        soft: bool,
    },
    /// A GPS jump: ignore it.
    Reject,
}

/// Gate by the squared innovation distance: hard gate 13.8 (2 dof, 99.9 %) or 18.5 (4 dof); between the 99 % and 99.9 % bounds the fix is
/// used with R scaled by `d2 / soft`.
#[must_use]
pub fn gate(d2: f64, with_velocity: bool, p: &LocParams) -> Gate {
    let (soft, hard) = if with_velocity { (p.gate_soft_4, p.gate_hard_4) } else { (p.gate_soft_2, p.gate_hard_2) };
    if !d2.is_finite() || d2 > hard {
        Gate::Reject
    } else if d2 > soft {
        Gate::Accept { r_scale: d2 / soft, soft: true }
    } else {
        Gate::Accept { r_scale: 1.0, soft: false }
    }
}

/// The squared innovation distance of `m` against one predicted model (`None` when it cannot take it).
#[must_use]
pub fn d2(g: &Gaussian, m: &Meas) -> Option<f64> {
    match m.vel {
        Some((v, rv)) => mahalanobis2(&g.x, &g.p, &[m.pos[0], m.pos[1], v[0], v[1]], &identity::<4>(), &block_diag(&m.r_pos, &rv)),
        None => mahalanobis2(&g.x, &g.p, &m.pos, &H_POS, &m.r_pos),
    }
}

/// The smallest squared innovation distance of `m` over the three predicted models (infinite when none can take it).
#[must_use]
pub fn min_d2(preds: &[Gaussian; 3], m: &Meas) -> f64 {
    preds.iter().filter_map(|g| d2(g, m)).fold(f64::INFINITY, f64::min)
}

/// A gated fix kept to judge a relocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatedFix {
    /// Position, metres ENU.
    pub en: [f64; 2],
    /// Reported accuracy, metres.
    pub acc_m: f64,
    /// Time, Unix ms.
    pub t_ms: i64,
}

/// Most gated fixes the relocator keeps.
const RELOC_KEEP: usize = 16;

/// Consecutive gated fixes, to tell a real relocation (they agree with each other) from a burst of bad fixes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Relocator {
    pending: Vec<GatedFix>,
    since_ms: Option<i64>,
    agreed: Option<(GatedFix, GatedFix)>,
}

fn agree(a: &GatedFix, b: &GatedFix, vmax: f64) -> bool {
    let d = (a.en[0] - b.en[0]).hypot(a.en[1] - b.en[1]);
    d <= a.acc_m + b.acc_m + vmax * (i64_to_f64(a.t_ms.saturating_sub(b.t_ms).saturating_abs()) / 1000.0)
}

impl Relocator {
    /// Forget the gated fixes (a fix was accepted, or the filter restarted).
    pub fn clear(&mut self) {
        self.pending.clear();
        self.since_ms = None;
        self.agreed = None;
    }

    /// The velocity (m/s, ENU) and its per-axis sigma shown by the oldest and newest of the fixes that agreed on the last relocation.
    #[must_use]
    pub fn agreed_velocity(&self) -> Option<([f64; 2], f64)> {
        let (a, b) = self.agreed?;
        let dt = i64_to_f64(b.t_ms.saturating_sub(a.t_ms)) / 1000.0;
        (dt > 0.0).then(|| {
            let sigma = (a.acc_m.hypot(b.acc_m) / crate::loc::ACC_TO_SIGMA) / dt;
            ([(b.en[0] - a.en[0]) / dt, (b.en[1] - a.en[1]) / dt], sigma)
        })
    }

    /// Move the kept fixes through `f` (re-anchoring).
    pub fn map_positions(&mut self, f: &dyn Fn([f64; 2]) -> [f64; 2]) {
        for g in &mut self.pending {
            g.en = f(g.en);
        }
    }

    /// Add a gated fix; true when the newest one should be believed: the last `reloc_count` agree pairwise and span `reloc_span_ms`, or,
    /// after `reloc_quick_after_ms` of continuous gating, two fixes of at most `reloc_quick_acc_m` agree.
    pub fn push(&mut self, f: GatedFix, vmax_mps: f64, p: &LocParams) -> bool {
        let since = *self.since_ms.get_or_insert(f.t_ms);
        self.pending.push(f);
        if self.pending.len() > RELOC_KEEP {
            self.pending.remove(0);
        }
        let n = p.reloc_count.clamp(2, RELOC_KEEP);
        if self.pending.len() >= n {
            let last = &self.pending[self.pending.len() - n..];
            let all_agree = last.iter().enumerate().all(|(i, a)| last[i + 1..].iter().all(|b| agree(a, b, vmax_mps)));
            if all_agree && last[n - 1].t_ms.saturating_sub(last[0].t_ms) >= p.reloc_span_ms {
                self.agreed = Some((last[0], last[n - 1]));
                return true;
            }
        }
        let quick = (f.t_ms.saturating_sub(since) >= p.reloc_quick_after_ms && f.acc_m <= p.reloc_quick_acc_m)
            .then(|| self.pending[..self.pending.len() - 1].iter().find(|g| g.acc_m <= p.reloc_quick_acc_m && agree(g, &f, vmax_mps)).copied())
            .flatten();
        self.agreed = quick.map(|g| (g, f));
        self.agreed.is_some()
    }
}

/// Whether a fix is dropped before the filter: invalid (NaN, out of range, negative accuracy), coarser than `unusable_acc_m`, dated
/// outside `min_fix_t_ms..=max_fix_t_ms`, not newer than the last fix (stale or duplicate), a mock (unless the bench allows it), or a
/// network fix once GNSS fixes have arrived. No sensor is a clock here: steps and compass readings stop while the player sits still.
#[must_use]
pub fn unusable(f: &RawFix, last_t_ms: Option<i64>, gnss_seen: bool, p: &LocParams) -> bool {
    // `contains` is false for NaN, so these also drop NaN and infinite fields.
    let valid = (-90.0..=90.0).contains(&f.lat) && (-180.0..=180.0).contains(&f.lon) && (0.0..=p.unusable_acc_m).contains(&f.accuracy_m);
    let sane = (p.min_fix_t_ms..=p.max_fix_t_ms).contains(&f.t_ms);
    !valid || !sane || last_t_ms.is_some_and(|t| f.t_ms <= t) || (f.mock && !p.allow_mock) || (f.provider == Provider::Network && gnss_seen)
}

/// Whether the filter restarts at this fix: none yet, more than `reset_gap_ms` since the last accepted fix, or lost (`P` trace too big).
#[must_use]
pub fn needs_reset(last_accepted_ms: Option<i64>, t_ms: i64, pos_trace_m2: f64, p: &LocParams) -> bool {
    last_accepted_ms.is_none_or(|l| t_ms.saturating_sub(l) > p.reset_gap_ms) || pos_trace_m2 > p.reset_trace_m2
}

/// The speed above which quests of a mode stop counting (`Game::speed_ok`), m/s; Drive has none there, 150 km/h here.
#[must_use]
pub fn mode_cap_mps(mode: Mode) -> f64 {
    let kmh = match mode {
        Mode::Walk => 12.0,
        Mode::Run => 25.0,
        Mode::Bike => 50.0,
        Mode::Drive => 150.0,
    };
    kmh / 3.6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prior_keeps_the_stationary_model_still() {
        // Review M10: the cloud's velocity is for the moving models; the stationary one keeps zero and its own sigma.
        let p = LocParams::default();
        let imm = Imm::with_prior([5.0, 6.0], [[16.0, 2.0], [2.0, 9.0]], [1.2, 0.3], [0.1, 0.85, 0.05], 1_000, Mode::Walk, &p);
        let still = imm.models[S];
        assert_eq!((still.x, still.p[2][2], still.p[3][3]), ([5.0, 6.0, 0.0, 0.0], p.stationary_vel_sigma_mps.powi(2), p.stationary_vel_sigma_mps.powi(2)));
        for g in [imm.models[W], imm.models[F]] {
            assert_eq!((g.x, g.p[0][0], g.p[0][1], g.p[1][1]), ([5.0, 6.0, 1.2, 0.3], 16.0, 2.0, 9.0));
        }
    }

    fn params() -> LocParams {
        LocParams::default()
    }

    #[test]
    fn pi_rows_are_probabilities_for_any_gap_and_both_mode_groups() {
        for mode in Mode::ALL {
            for dt in [0.0, 0.5, 1.0, 5.0, 10.0, 60.0] {
                for row in transition(&params(), mode, dt) {
                    assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-12, "{mode:?} {dt}: {row:?}");
                    assert!(row.iter().all(|v| (0.0..=1.0).contains(v)), "{mode:?} {dt}: {row:?}");
                }
            }
        }
    }

    #[test]
    fn one_second_of_pi_is_the_spec_table() {
        let p = transition(&params(), Mode::Walk, 1.0);
        assert!((p[S][S] - 0.95).abs() < 1e-12 && (p[W][F] - 0.01).abs() < 1e-12 && (p[F][W] - 0.08).abs() < 1e-12);
        let b = transition(&params(), Mode::Bike, 1.0);
        assert!((b[F][F] - 0.95).abs() < 1e-12 && (b[W][F] - 0.15).abs() < 1e-12);
    }

    #[test]
    fn the_stationary_model_keeps_the_position_and_drops_the_velocity() {
        let g = Gaussian { x: [3.0, 4.0, 1.0, 1.0], p: scale(&identity::<4>(), 2.0) };
        let out = predict(&g, S, 10.0, 0.0, &params());
        assert_eq!(&out.x, &[3.0, 4.0, 0.0, 0.0]);
        assert!((out.p[0][0] - (2.0 + 0.0025 * 10.0)).abs() < 1e-12);
        assert!((out.p[2][2] - 0.01).abs() < 1e-12);
    }

    #[test]
    fn constant_velocity_moves_by_v_dt_and_adds_white_acceleration_noise() {
        let g = Gaussian { x: [0.0, 0.0, 1.5, -0.5], p: zeros() };
        let out = predict(&g, W, 2.0, 0.5, &params());
        assert!((out.x[0] - 3.0).abs() < 1e-12 && (out.x[1] + 1.0).abs() < 1e-12);
        let q = cv_noise(0.5, 2.0);
        assert!((q[0][0] - 0.25 * 16.0 / 4.0).abs() < 1e-12 && (q[0][2] - 0.25 * 8.0 / 2.0).abs() < 1e-12 && (q[2][2] - 0.25 * 4.0).abs() < 1e-12);
        assert_eq!(out.p, q);
    }

    #[test]
    fn sigma_a_follows_the_zone_mode() {
        let p = params();
        // Ruling T8-R9: the Walk default is 0.3 (inside the spec's 0.3 to 0.8), tuned on the layer 1 scenarios; the order stays.
        assert_eq!((sigma_a(&p, Mode::Walk, W), sigma_a(&p, Mode::Run, W)), (0.3, 1.0));
        assert!(sigma_a(&p, Mode::Walk, W) < sigma_a(&p, Mode::Run, W) && sigma_a(&p, Mode::Bike, F) < sigma_a(&p, Mode::Drive, F));
        assert_eq!((sigma_a(&p, Mode::Bike, F), sigma_a(&p, Mode::Drive, F)), (1.5, 3.0));
    }

    #[test]
    fn mixing_without_switching_changes_nothing_and_combining_adds_the_spread() {
        let a = Gaussian { x: [0.0, 0.0, 0.0, 0.0], p: identity::<4>() };
        let b = Gaussian { x: [2.0, 0.0, 0.0, 0.0], p: identity::<4>() };
        let (mixed, c) = mix(&[a, b, a], &[0.5, 0.5, 0.0], &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert_eq!(mixed[0].x, a.x);
        assert_eq!(mixed[1].x, b.x);
        assert_eq!(c, [0.5, 0.5, 0.0]);
        let out = combine(&[a, b, a], &[0.5, 0.5, 0.0]);
        assert!((out.x[0] - 1.0).abs() < 1e-12 && (out.p[0][0] - 2.0).abs() < 1e-12, "1 + spread 1");
    }

    fn feed(imm: &mut Imm, mode: Mode, steps: [f64; 3], pts: impl Iterator<Item = (i64, [f64; 2])>) {
        let p = params();
        for (t, z) in pts {
            let (preds, c) = imm.predict_to(t, mode, &p);
            let m = Meas { pos: z, r_pos: scale(&identity::<2>(), 9.0), vel: None };
            assert!(imm.update(preds, c, &m, 1.0, steps, t, p.mu_floor));
        }
    }

    #[test]
    fn walking_fixes_raise_the_walking_model_and_standing_ones_the_stationary() {
        let p = params();
        let mut walk = Imm::new([0.0, 0.0], 3.0, 0, Mode::Walk, &p);
        // Step evidence (S x 0.1, steps coming in) is fed: position-only walking biases the combined speed low (mu_S ~0.2),
        // tracked for Tasks 7 and 23 in the ledger.
        feed(&mut walk, Mode::Walk, [0.1, 1.0, 1.0], (1..=60).map(|i| (i * 1000, [1.4 * i64_to_f64(i), 0.0])));
        assert!(walk.mu[W] > 0.6, "{:?}", walk.mu);
        assert!((walk.output().x[2] - 1.4).abs() < 0.3, "velocity learned: {:?}", walk.output().x);
        let mut stand = Imm::new([0.0, 0.0], 3.0, 0, Mode::Walk, &p);
        feed(&mut stand, Mode::Walk, [1.0; 3], (1..=60).map(|i| (i * 1000, [0.0, 0.0])));
        assert!(stand.mu[S] > 0.8, "{:?}", stand.mu);
        assert!((stand.mu.iter().sum::<f64>() - 1.0).abs() < 1e-9 && stand.mu.iter().all(|m| *m >= p.mu_floor * 0.99));
    }

    #[test]
    fn coasting_keeps_the_prediction_and_advances_the_time() {
        let p = params();
        let mut imm = Imm::new([0.0, 0.0], 3.0, 0, Mode::Walk, &p);
        let (preds, c) = imm.predict_to(5_000, Mode::Walk, &p);
        imm.coast(preds, c, 5_000, p.mu_floor);
        assert_eq!(imm.t_ms, 5_000);
        assert!(imm.output().p[0][0] > 9.0, "uncertainty grew");
    }

    #[test]
    fn mixing_keeps_a_model_nothing_switches_into() {
        // T5-m2: c_j <= 0 (bench params with zero rates and priors) keeps models[j] instead of an origin-at-zero blend.
        let a = Gaussian { x: [1.0, 2.0, 0.0, 0.0], p: identity::<4>() };
        let b = Gaussian { x: [5.0, 6.0, 1.0, 0.0], p: scale(&identity::<4>(), 3.0) };
        let (mixed, c) = mix(&[a, b, b], &[1.0, 0.0, 0.0], &identity::<3>());
        assert_eq!(c, [1.0, 0.0, 0.0]);
        assert_eq!(mixed[0], a);
        assert_eq!(mixed[1], b);
        assert_eq!(mixed[2], b);
    }

    #[test]
    fn new_normalizes_the_model_priors() {
        // T5-m2: zero or unnormalized priors from --params still give probabilities.
        let zero = LocParams { mu0_slow: [0.0; 3], ..params() };
        let imm = Imm::new([0.0, 0.0], 3.0, 0, Mode::Walk, &zero);
        assert!(imm.mu.iter().all(|m| (m - 1.0 / 3.0).abs() < 1e-12), "{:?}", imm.mu);
        let twice = LocParams { mu0_fast: [2.0, 1.0, 1.0], ..params() };
        let imm = Imm::new([0.0, 0.0], 3.0, 0, Mode::Bike, &twice);
        assert!((imm.mu[S] - 0.5).abs() < 1e-12 && (imm.mu[W] - 0.25).abs() < 1e-12, "{:?}", imm.mu);
    }

    fn four_dof() -> (Gaussian, Meas) {
        let g = Gaussian { x: [0.0; 4], p: scale(&identity::<4>(), 4.0) };
        let m = Meas { pos: [2.0, -2.0], r_pos: scale(&identity::<2>(), 4.0), vel: Some(([1.0, 0.0], scale(&identity::<2>(), 4.0))) };
        (g, m)
    }

    fn close4(a: &[f64; 4], b: &[f64; 4]) -> bool {
        a.iter().zip(b).all(|(u, v)| (u - v).abs() < 1e-12)
    }

    #[test]
    fn a_fix_with_velocity_updates_all_four_components() {
        // T5-m3: P = 4 I, R = 4 I, H = I -> K = I / 2: x = z / 2, P = 2 I.
        let (g, m) = four_dof();
        let mut imm = Imm { models: [g; 3], mu: [1.0 / 3.0; 3], t_ms: 0 };
        assert!(imm.update([g; 3], [1.0 / 3.0; 3], &m, 1.0, [1.0; 3], 1_000, 1e-4));
        for post in imm.models {
            assert!(close4(&post.x, &[1.0, -1.0, 0.5, 0.0]), "{:?}", post.x);
            assert!((0..4).all(|i| (0..4).all(|j| (post.p[i][j] - if i == j { 2.0 } else { 0.0 }).abs() < 1e-12)), "{:?}", post.p);
        }
        assert_eq!(imm.t_ms, 1_000);
    }

    #[test]
    fn a_soft_gate_inflates_the_velocity_noise_too() {
        // T5-m1: r_scale 3 -> R = 12 I on all four components -> K = I / 4: the velocity moves a quarter of the way, not half.
        let (g, m) = four_dof();
        let mut imm = Imm { models: [g; 3], mu: [1.0 / 3.0; 3], t_ms: 0 };
        assert!(imm.update([g; 3], [1.0 / 3.0; 3], &m, 3.0, [1.0; 3], 1_000, 1e-4));
        assert!(close4(&imm.models[W].x, &[0.5, -0.5, 0.25, 0.0]), "{:?}", imm.models[W].x);
        assert!((imm.models[W].p[2][2] - 3.0).abs() < 1e-12, "{:?}", imm.models[W].p);
    }

    #[test]
    fn a_nan_fix_changes_nothing() {
        // T5-m3: no model can take a NaN measurement: update returns false and the state stays.
        let p = params();
        let mut imm = Imm::new([0.0, 0.0], 3.0, 0, Mode::Walk, &p);
        let before = imm;
        let (preds, c) = imm.predict_to(1_000, Mode::Walk, &p);
        let m = Meas { pos: [f64::NAN, 0.0], r_pos: identity::<2>(), vel: None };
        assert!(!imm.update(preds, c, &m, 1.0, [1.0; 3], 1_000, p.mu_floor));
        assert_eq!(imm, before);
    }

    #[test]
    fn the_gate_follows_the_chi_square_boundaries() {
        let p = params();
        assert_eq!(gate(9.2, false, &p), Gate::Accept { r_scale: 1.0, soft: false });
        assert_eq!(gate(13.79, false, &p), Gate::Accept { r_scale: 13.79 / 9.21, soft: true });
        assert_eq!(gate(13.81, false, &p), Gate::Reject);
        assert_eq!(gate(18.4, true, &p), Gate::Accept { r_scale: 18.4 / 13.28, soft: true });
        assert_eq!(gate(18.6, true, &p), Gate::Reject);
        assert_eq!(gate(f64::NAN, false, &p), Gate::Reject);
    }

    #[test]
    fn the_gate_distance_is_the_smallest_over_the_models() {
        let near = Gaussian { x: [0.0; 4], p: identity::<4>() };
        let far = Gaussian { x: [100.0, 0.0, 0.0, 0.0], p: identity::<4>() };
        let m = Meas { pos: [1.0, 0.0], r_pos: identity::<2>(), vel: None };
        assert!((min_d2(&[far, near, far], &m) - 0.5).abs() < 1e-12);
        let mv = Meas { vel: Some(([2.0, 0.0], identity::<2>())), ..m };
        assert!((min_d2(&[far, near, far], &mv) - 2.5).abs() < 1e-12, "position 0.5 + velocity 2");
        let none = Meas { pos: [f64::NAN, 0.0], ..m };
        assert!(min_d2(&[near; 3], &none).is_infinite(), "a NaN fix fits no model");
    }

    fn gf(e: f64, t_s: i64, acc: f64) -> GatedFix {
        GatedFix { en: [e, 0.0], acc_m: acc, t_ms: t_s * 1000 }
    }

    #[test]
    fn three_agreeing_gated_fixes_over_two_seconds_mean_a_relocation() {
        let p = params();
        let mut r = Relocator::default();
        assert!(!r.push(gf(2000.0, 1, 5.0), 5.0, &p));
        assert!(!r.push(gf(2003.0, 2, 5.0), 5.0, &p));
        assert!(r.push(gf(2001.0, 3, 5.0), 5.0, &p), "third agreeing fix, 2 s span");
    }

    #[test]
    fn scattered_or_too_quick_gated_fixes_are_not_a_relocation() {
        let p = params();
        let mut r = Relocator::default();
        for (e, t) in [(2000.0, 1), (2200.0, 2), (1800.0, 3)] {
            assert!(!r.push(gf(e, t, 5.0), 5.0, &p), "scattered");
        }
        let mut q = Relocator::default();
        for t_ms in [1000, 1500, 1900] {
            assert!(!q.push(GatedFix { en: [2000.0, 0.0], acc_m: 5.0, t_ms }, 5.0, &p), "under 2 s");
        }
    }

    #[test]
    fn after_twenty_seconds_of_gating_two_good_agreeing_fixes_are_enough() {
        let p = params();
        let mut r = Relocator::default();
        assert!(!r.push(gf(2000.0, 0, 40.0), 5.0, &p));
        assert!(!r.push(gf(2500.0, 10, 40.0), 5.0, &p));
        assert!(!r.push(gf(2000.0, 21, 15.0), 5.0, &p));
        assert!(r.push(gf(2004.0, 22, 15.0), 5.0, &p), "two fixes <= 20 m that agree, after 20 s of gating");
    }

    #[test]
    fn clearing_or_moving_the_kept_fixes() {
        let p = params();
        let mut r = Relocator::default();
        r.push(gf(2000.0, 1, 5.0), 5.0, &p);
        r.push(gf(2003.0, 2, 5.0), 5.0, &p);
        r.clear();
        assert!(!r.push(gf(2001.0, 3, 5.0), 5.0, &p), "cleared: one fix only");
        assert_eq!(r, Relocator { pending: vec![gf(2001.0, 3, 5.0)], since_ms: Some(3_000), agreed: None });
        r.map_positions(&|[e, n]| [e - 2000.0, n + 1.0]);
        assert_eq!(r.pending[0].en, [1.0, 1.0]);
    }

    #[test]
    fn the_relocator_keeps_a_bounded_history() {
        let p = params();
        let mut r = Relocator::default();
        for i in 0..40 {
            r.push(gf(f64::from(i) * 1000.0, i64::from(i), 5.0), 5.0, &p);
        }
        assert!(r.pending.len() <= 16, "{}", r.pending.len());
    }

    #[test]
    fn unusable_rules() {
        // Review Focus 1: stale, duplicate and invalid fixes never reach the filter.
        let p = params();
        let ok = RawFix::at(40.0, -111.0, 10_000, 5.0);
        assert!(!unusable(&ok, Some(9_000), false, &p));
        assert!(unusable(&RawFix { accuracy_m: 101.0, ..ok }, None, false, &p), "coarser than 100 m");
        assert!(unusable(&RawFix { accuracy_m: -1.0, ..ok }, None, false, &p), "negative accuracy");
        assert!(unusable(&ok, Some(10_000), false, &p), "duplicate time");
        assert!(unusable(&ok, Some(11_000), false, &p), "older than the last fix");
        assert!(unusable(&RawFix { lat: f64::NAN, ..ok }, None, false, &p));
        assert!(unusable(&RawFix { lat: 91.0, ..ok }, None, false, &p));
        assert!(unusable(&RawFix { mock: true, ..ok }, None, false, &p));
        assert!(!unusable(&RawFix { mock: true, ..ok }, None, false, &LocParams { allow_mock: true, ..params() }));
        let net = RawFix { provider: Provider::Network, ..ok };
        assert!(!unusable(&net, None, false, &p), "a cold-start network fix is fine");
        assert!(unusable(&net, None, true, &p), "never mixed in once GNSS fixes arrive");
    }

    #[test]
    fn fixes_dated_outside_a_sane_range_are_unusable() {
        // Adversarial review M1, re-review N3: before 2000-01-01 (the production floor) or after 2100-01-01, whatever was seen before.
        use crate::loc::params::{MAX_FIX_T_MS, MIN_FIX_T_MS};
        let p = LocParams { min_fix_t_ms: MIN_FIX_T_MS, ..params() };
        let at = |t_ms: i64| RawFix::at(40.0, -111.0, t_ms, 5.0);
        assert!(unusable(&at(MIN_FIX_T_MS - 1), None, false, &p), "1999");
        assert!(!unusable(&at(MIN_FIX_T_MS), None, false, &p), "2000-01-01");
        assert!(unusable(&at(i64::MIN), None, false, &p));
        assert!(!unusable(&at(MAX_FIX_T_MS), Some(1_800_000_000_000), false, &p), "2100-01-01, however far after the last fix");
        assert!(unusable(&at(MAX_FIX_T_MS + 1), None, false, &p), "after 2100");
        assert!(unusable(&at(i64::MAX), None, false, &p));
    }

    #[test]
    fn absurd_fields_are_unusable() {
        let p = params();
        let ok = RawFix::at(40.0, -111.0, 10_000, 5.0);
        assert!(unusable(&RawFix { lon: 180.5, ..ok }, None, false, &p));
        assert!(unusable(&RawFix { lon: f64::INFINITY, ..ok }, None, false, &p));
        assert!(unusable(&RawFix { accuracy_m: f64::NAN, ..ok }, None, false, &p));
        assert!(unusable(&RawFix { lat: -90.1, ..ok }, None, false, &p));
        assert!(!unusable(&RawFix { lat: -90.0, lon: 180.0, accuracy_m: 100.0, ..ok }, None, false, &p), "the edges are valid");
    }

    #[test]
    fn a_long_gap_or_a_lost_filter_resets() {
        let p = params();
        assert!(needs_reset(None, 0, 1.0, &p));
        assert!(!needs_reset(Some(0), 300_000, 1.0, &p));
        assert!(needs_reset(Some(0), 300_001, 1.0, &p));
        assert!(needs_reset(Some(0), 1_000, 40_001.0, &p));
    }

    #[test]
    fn mode_caps_match_speed_ok_and_drive_gets_150_kmh() {
        let caps = Mode::ALL.map(|m| mode_cap_mps(m) * 3.6);
        assert!(caps.iter().zip([12.0, 25.0, 50.0, 150.0]).all(|(a, b)| (a - b).abs() < 1e-9), "{caps:?}");
    }
}
