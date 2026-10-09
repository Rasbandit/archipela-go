//! The `Locator`: the one thing the game talks to. Raw fixes, steps and compass readings in; estimates out. Not saved: a restart starts
//! fresh (spec "Filter state").

use std::collections::VecDeque;
use std::sync::Arc;

use crate::catalog::Mode;
use crate::geo::distance_m;
use crate::loc::bridge::Bridge;
use crate::loc::calib::Calibrator;
use crate::loc::frame::Frame;
use crate::loc::graph::{mode_mask, StreetGraph};
use crate::loc::heading::{CarryOffset, Compass};
use crate::loc::imm::{self, Gate, GatedFix, Imm, Meas, Relocator, F, S, W};
use crate::loc::mat::{identity, max_eig2, scale, Mat};
use crate::loc::matcher::Matcher;
use crate::loc::params::LocParams;
use crate::loc::{Estimate, Motion, Provider, RawFix, Source, Verdict, ACC_TO_SIGMA};
use crate::num::to_f32;

mod carry;
mod display;
mod gap;
mod hold;
mod maneuver;
mod odometer;
mod steps;
#[cfg(test)]
mod test_util;

pub use odometer::Odometer;
pub use steps::StepHistory;

use display::show_match;
use gap::{Reref, TurnCheck};
use hold::Hold;
use maneuver::{UsedFix, LINE_FIT_FIXES};

/// A simulated fix farther than this from the position before it is a teleport and breaks the trace, metres.
const SIM_TELEPORT_M: f64 = 50.0;

/// The location filter of one game session.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)] // independent facts about the newest fix and session (sim, GNSS seen, hold ended, restart, reset asked), not a state machine
pub struct Locator {
    params: LocParams,
    mode: Mode,
    frame: Option<Frame>,
    imm: Option<Imm>,
    last_t_ms: Option<i64>,
    /// Time of the newest fix the filter took (a restart, or an update, however uncertain): the reset-gap clock.
    last_taken_ms: Option<i64>,
    /// Time of the newest accepted estimate: the gap bridge's clocks (ruling FR-I1, Task 7 minor).
    last_accepted_ms: Option<i64>,
    started_ms: Option<i64>,
    gnss_seen: bool,
    last_sim: bool,
    reloc: Relocator,
    hold: Option<Hold>,
    steps: StepHistory,
    last: Option<Estimate>,
    used: VecDeque<UsedFix>,
    disagreeing: u32,
    /// The maneuver's line-fit course, degrees, while it is reported (rulings T8-R8, R14, R17), with its sigma only when the filter has
    /// no course ("unseen"), together with the time of the fit's first fix: a line fit that turned away from the filter's course
    /// straddles a corner and teaches the carry nothing.
    maneuver_course: Option<(f64, Option<(f64, i64)>)>,
    ended_hold: bool,
    /// The agreeing fixes' speed of a relocation by the newest fix, m/s.
    reloc_speed: Option<f64>,
    compass: Compass,
    /// Where the player walks relative to where the phone points (Task 20), learned from good courses; kept across filter restarts.
    carry: CarryOffset,
    /// The sigma of the newest estimate's course (moving models or an "unseen" maneuver line fit), degrees; `None` without one.
    last_course_sigma_deg: Option<f64>,
    /// The time of the first fix of the line fit the newest course came from; `None` for a course from the moving models.
    last_course_fit_from_ms: Option<i64>,
    /// Whether the newest fix restarted the filter (reset, relocation or a simulated fix): the pin jumps instead of gliding.
    restarted: bool,
    /// The street graph of the game's zones; `None` without streets.
    graph: Option<Arc<StreetGraph>>,
    /// Map matching of the accepted estimates (display and trace only).
    matcher: Matcher,
    /// [`Self::reset`] was asked for: the restart it causes keeps the trace's line.
    reset_requested: bool,
    /// Time of the newest accepted estimate, for the trace's gap rule (kept through [`Self::reset`]).
    trace_last_ms: Option<i64>,
    /// The matched segment the map shows, decided per fix with hysteresis (`show_match`); `None` shows the estimate.
    shown_seg: Option<usize>,
    /// The step length scale of the phone's step counter (Task 21), learned from good walking GPS; kept across filter restarts.
    calib: Calibrator,
    /// The particle cloud while a GPS gap is bridged (Task 22).
    bridge: Option<Bridge>,
    /// The walker stopped in the gap (no steps for `bridge_no_steps_ms`): the cloud waits for the next steps (review I1).
    bridge_paused: bool,
    /// The newest bridged estimate: the map shows it while it is newer than [`Self::last`], and it ages like any other.
    bridged: Option<Estimate>,
    /// The newest course: an accepted estimate's, or the bearing of the bridge's street during a gap, degrees.
    last_course: Option<f64>,
    /// In a gap after the phone moved: the compass re-referenced to the cloud's street course, `course - azimuth`, degrees (gap only).
    reref: Reref,
    /// A compass jump in a gap read as a turn at a crossing, to be checked once the cloud is past it (review M8).
    turn_check: Option<TurnCheck>,
    /// The compass at the newest step batch of the gap, degrees: a walker who stood and walks on facing another way turned round.
    bridge_step_az: Option<f64>,
    /// The cloud's newest step batch when it last re-anchored the filter: without a step since, the filter goes on from its own state
    /// (review round 3, minor 4).
    reanchored_step_ms: Option<i64>,
    /// Fixes still to be taken (Used or Soft) after a bridge re-anchor before an estimate may count: the cloud's prior is no evidence
    /// (adversarial review C1).
    reanchor_warmup: u32,
    /// After a bridge re-anchor, until a fix is taken: the position variance (m^2 per axis) the prior is widened to for that fix's
    /// update. The gate judges the fix against the cloud itself (adversarial re-review N2).
    reanchor_floor_m2: Option<f64>,
    /// A bridged gap ended on a taken fix without a restart: the trackers owe a pause at the next accepted estimate.
    bridge_pause_owed: bool,
    /// The newest estimate is the first accepted one after a bridged gap ([`Self::resumed_after_bridge`]).
    resumed_after_bridge: bool,
    /// The fix being filtered is a network (Wi-Fi or cell) position: display only, never accepted (adversarial review I2).
    network_fix: bool,
}

impl Default for Locator {
    fn default() -> Self {
        Self::new(LocParams::default())
    }
}

impl Locator {
    /// A fresh locator with `params`, in a Walk zone until told otherwise.
    #[must_use]
    pub fn new(params: LocParams) -> Self {
        Self {
            params,
            mode: Mode::Walk,
            frame: None,
            imm: None,
            last_t_ms: None,
            last_taken_ms: None,
            last_accepted_ms: None,
            started_ms: None,
            gnss_seen: false,
            last_sim: false,
            reloc: Relocator::default(),
            hold: None,
            steps: StepHistory::default(),
            last: None,
            used: VecDeque::new(),
            disagreeing: 0,
            maneuver_course: None,
            ended_hold: false,
            reloc_speed: None,
            compass: Compass::default(),
            carry: CarryOffset::default(),
            last_course_sigma_deg: None,
            last_course_fit_from_ms: None,
            restarted: false,
            graph: None,
            matcher: Matcher::new(None, mode_mask(Mode::Walk)),
            reset_requested: false,
            trace_last_ms: None,
            shown_seg: None,
            calib: Calibrator::default(),
            bridge: None,
            bridge_paused: false,
            bridged: None,
            last_course: None,
            reref: Reref::None,
            turn_check: None,
            bridge_step_az: None,
            reanchored_step_ms: None,
            reanchor_warmup: 0,
            reanchor_floor_m2: None,
            bridge_pause_owed: false,
            resumed_after_bridge: false,
            network_fix: false,
        }
    }

    /// The street graph of the game's zones (map matching and gap bridging use it); `None` without streets.
    pub fn set_graph(&mut self, graph: Option<Arc<StreetGraph>>) {
        self.matcher.set_graph(graph.clone());
        self.shown_seg = None; // the old graph's segment ids mean nothing in the new one
        self.graph = graph;
    }

    /// The street graph, if any.
    #[must_use]
    pub fn graph(&self) -> Option<&Arc<StreetGraph>> {
        self.graph.as_ref()
    }

    /// The parameters in use.
    #[must_use]
    pub fn params(&self) -> &LocParams {
        &self.params
    }

    /// The travel mode at the player's position (see [`crate::loc::mode_at`]); applies from the next fix. A new mode restarts the
    /// matcher on that mode's streets.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        if self.matcher.set_mask(mode_mask(mode)) {
            self.shown_seg = None; // a restarted match re-enters at the show confidence
        }
    }

    /// The travel mode in use.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Start fresh at the next fix (counting toggled, game opened). The step history, "GNSS seen" and the session trace are kept; the
    /// fix clock goes, so a fix dated wrong never outlives a reset (adversarial review I1); the matcher restarts, and the trace's line
    /// goes on (the player is where they were, review M7).
    pub fn reset(&mut self) {
        self.restart_matcher();
        self.reset_requested = true;
        self.clear_filter();
        self.last_t_ms = None;
    }

    /// Restart the matcher's lattice and forget the shown match, so the pin re-enters a street only at the show confidence.
    fn restart_matcher(&mut self) {
        self.matcher.restart();
        self.shown_seg = None;
    }

    /// Forget the filter (not the matcher, the fix clock, the steps or "GNSS seen").
    fn clear_filter(&mut self) {
        self.frame = None;
        self.imm = None;
        self.last_taken_ms = None;
        self.last_accepted_ms = None;
        self.started_ms = None;
        self.reloc.clear();
        self.hold = None;
        self.last = None;
        self.bridge = None;
        self.bridge_paused = false;
        self.bridged = None;
        self.last_course = None;
        self.reref = Reref::None;
        self.turn_check = None;
        self.bridge_step_az = None;
        self.reanchored_step_ms = None;
        self.reanchor_warmup = 0;
        self.reanchor_floor_m2 = None;
        self.bridge_pause_owed = false; // a restart pauses the trackers itself
    }

    /// The newest estimate made from a fix that reached the filter.
    #[must_use]
    pub fn last(&self) -> Option<Estimate> {
        self.last
    }

    /// Feed one raw fix; the estimate says what became of it.
    pub fn on_fix(&mut self, f: &RawFix) -> Estimate {
        let was_holding = self.hold.is_some();
        // A simulated fix far from the position before it is a dev teleport: no line to it (fix round 3).
        let teleport = f.provider == Provider::Sim && self.last.is_some_and(|l| distance_m(l.point(), f.point()) > SIM_TELEPORT_M);
        self.reloc_speed = None;
        self.restarted = false;
        let e = self.filter_fix(f);
        if e.accepted || matches!(e.verdict, Verdict::Used | Verdict::Soft) {
            // GPS is back (review I2: a gated or blurry fix leaves the cloud running). A fix taken after a re-anchor ends it too, though
            // it does not count yet: re-anchoring again on the next steps would throw the fix away (adversarial review C1).
            self.bridge_pause_owed |= self.bridge.is_some() && !matches!(e.verdict, Verdict::Reset | Verdict::Relocated);
            self.end_bridge();
        }
        // The time in a bridged gap was not seen by any fix: dwell and away timers pause at the first accepted estimate after it, as
        // they do at a restart (adversarial re-review, quest integrity).
        self.resumed_after_bridge = e.accepted && std::mem::take(&mut self.bridge_pause_owed);
        self.ended_hold = was_holding && self.hold.is_none();
        self.restarted |= matches!(e.verdict, Verdict::Reset | Verdict::Relocated);
        // Nothing is known of the way across a gap without accepted estimates (review I2, measured from the last accepted estimate so
        // standing, which feeds the matcher nothing, keeps the line): a new lattice and, as the journal's segments do, a new run.
        if e.accepted && self.trace_last_ms.replace(e.t_ms).is_some_and(|t| e.t_ms.saturating_sub(t) > crate::journal::DEFAULT_MAX_GAP_MS) {
            self.restart_matcher();
            self.matcher.break_trace();
        }
        if matches!(e.verdict, Verdict::Reset | Verdict::Relocated) {
            // The off-network state links every step, so the lattice restarts only here (and on a reset or a new mode).
            self.restart_matcher();
            if !std::mem::take(&mut self.reset_requested) {
                self.matcher.break_trace(); // no line between where the player was and where they reappeared (ruling T19-R3)
            }
        }
        if teleport && e.verdict != Verdict::Unusable {
            self.restart_matcher();
            self.matcher.break_trace();
        }
        // A simulated fix restarts the filter, so its estimate never looks moving; the dev walk is fed as walking (review M6).
        let fed = if f.provider == Provider::Sim { Estimate { motion: Motion::Walking, ..e } } else { e };
        // A held position is frozen, like standing: it feeds nothing whatever model is most likely (ruling T19-input; a guard: in the
        // tests a hold always reads stationary, which the matcher skips anyway).
        if self.holding() {
            self.calib.break_window(); // a held position is frozen: no window across it (ruling T21-I1)
        } else {
            self.matcher.push(&fed, &self.params); // ignores what is not accepted or stationary
            self.learn_carry(&e);
            if self.ended_hold {
                self.calib.break_window(); // the estimate catches up from the held position: not walked in the window
            }
            if !self.last_sim {
                self.calib.on_estimate(&e, &self.steps, &self.params); // positions, not the filter's speed (Task 21 notes 3)
            }
        }
        self.shown_seg = show_match(self.shown_seg, self.matcher.best(), &e, self.graph.as_deref(), &self.params);
        if e.accepted && e.course_deg.is_some() && self.hold.is_none() {
            self.last_course = e.course_deg;
        }
        e
    }

    /// Whether the newest estimate is the first accepted one after a bridged gap that ended without a restart: dwell and away timers
    /// pause there, as at a restart, so the gap is not dwelt.
    #[must_use]
    pub fn resumed_after_bridge(&self) -> bool {
        self.resumed_after_bridge
    }

    /// Whether the newest fix restarted the filter: a reset, a relocation or a simulated fix (which resets to an exact estimate).
    #[must_use]
    pub fn restarted(&self) -> bool {
        self.restarted
    }

    fn filter_fix(&mut self, f: &RawFix) -> Estimate {
        self.network_fix = f.provider == Provider::Network;
        if f.provider == Provider::Sim {
            return self.simulated(f);
        }
        if std::mem::take(&mut self.last_sim) {
            self.clear_filter(); // the Reset this fix becomes restarts the matcher and the line: real GPS may be far from the dev walk
            self.last_t_ms = None;
            // Readings on the simulator's clock (ahead of real time) would drop the real ones that follow (final review M1).
            self.steps = StepHistory::default();
            self.compass = Compass::default();
        }
        // Older than the last fix by more than the reset gap: the clock jumped (or the last fix was dated wrong), so the filter
        // restarts from this fix instead of dropping every fix until the clock catches up (adversarial review I1).
        let jumped = self.last_t_ms.is_some_and(|t| f.t_ms < t.saturating_sub(self.params.reset_gap_ms));
        let clock = if jumped { None } else { self.last_t_ms };
        if imm::unusable(f, clock, self.gnss_seen, &self.params) {
            return self.unusable(f);
        }
        if jumped {
            self.clock_jump(f.t_ms);
        }
        self.last_t_ms = Some(f.t_ms);
        self.gnss_seen |= f.provider.is_gnss();
        self.reanchor_bridge(f);
        let restart = imm::needs_reset(self.last_taken_ms, f.t_ms, 0.0, &self.params);
        // The filter is taken out for this fix and put back unless the fix restarts it (`start` makes a new one).
        let (Some(frame), Some(mut imm)) = (self.frame, self.imm.take().filter(|_| !restart)) else {
            return self.reset_at(f);
        };
        let z = frame.to_enu(f.point());
        let meas = self.measurement(f, z);
        let weights_by_model = self.step_factors(f.t_ms);
        let quiet_hold = self.hold.is_some() && self.quiet_counter(f.t_ms);
        let (mode, vmax) = (self.mode, self.params.reloc_vmax_mps);
        // Maneuver (rulings T8-R8, R14): the line through the last used fixes and this one turned away from the filter's course for
        // `maneuver_fixes` fixes: report the line's course and predict this step with inflated process noise.
        let (filter_course, moving_mu) = (Self::course_of(&imm, &self.params).map(|(c, _)| c), imm.mu[W] + imm.mu[F]);
        let fix_used = UsedFix { en: z, t_ms: f.t_ms, sigma_m: self.sigma_of(f) };
        let before = self.disagreeing;
        let turned = self.judge_maneuver(Some(fix_used), filter_course, moving_mu, before);
        // Only a turn away from the filter's own course inflates Q: inflating when the filter has no course would widen its velocity
        // and keep the course hidden, so the maneuver would never end.
        let inflated = self.maneuver_course.is_some() && turned;
        let (preds, c) = self.predict(&imm, f.t_ms, inflated);
        // Lost: the prediction itself is too uncertain to gate against (ruling T8-R3: checked after the prediction, so a long gap counts).
        let predicted = imm::combine(&preds, &c);
        if imm::needs_reset(self.last_taken_ms, f.t_ms, predicted.p[0][0] + predicted.p[1][1], &self.params) {
            return self.reset_at(f);
        }
        // Held with a quiet step counter, the player stands: a fix only the moving models explain is a jump or a vehicle leaving, for
        // the relocator to judge (ruling T8-R2).
        let d2 = if quiet_hold { imm::d2(&preds[S], &meas).unwrap_or(f64::INFINITY) } else { imm::min_d2(&preds, &meas) };
        match imm::gate(d2, meas.vel.is_some(), &self.params) {
            Gate::Reject => {
                // A gated fix in the warm-up restarts it: ghosts that alternate with gated ones must not run it down (re-review N2').
                if self.reanchor_warmup > 0 {
                    self.reanchor_warmup = gap::REANCHOR_WARMUP_FIXES;
                }
                // A rejected fix is no part of the line fit (finding M3): coast on the ordinary prediction and judge the maneuver again
                // on the used fixes alone.
                let (preds, c) = if inflated { imm.predict_to(f.t_ms, mode, &self.params) } else { (preds, c) };
                imm.coast(preds, c, f.t_ms, self.params.mu_floor);
                self.judge_maneuver(None, filter_course, moving_mu, before);
                if self.reloc.push(GatedFix { en: z, acc_m: f.accuracy_m, t_ms: f.t_ms }, vmax, &self.params) {
                    let moving = self.reloc.agreed_velocity();
                    self.reloc_speed = moving.map(|(v, _)| v[0].hypot(v[1]));
                    self.start(f, self.sigma_of(f));
                    if let Some((v, v_sigma)) = moving {
                        self.start_moving(f, v, v_sigma);
                    }
                    return self.emit(f.t_ms, Verdict::Relocated);
                }
                self.imm = Some(imm);
                self.emit(f.t_ms, Verdict::Gated)
            }
            Gate::Accept { r_scale, soft } => {
                // The first fix taken after a re-anchor passed the gate against the cloud; it updates a prior no tighter than itself, so
                // the cloud decides nothing (adversarial review C1, re-review N2).
                let (preds, c) = match self.reanchor_floor_m2.take() {
                    Some(floor) => {
                        imm.floor_pos_var(floor);
                        self.predict(&imm, f.t_ms, inflated)
                    }
                    None => (preds, c),
                };
                if !imm.update(preds, c, &meas, r_scale, weights_by_model, f.t_ms, self.params.mu_floor) {
                    return self.reset_at(f);
                }
                self.imm = Some(imm);
                self.reloc.clear();
                self.last_taken_ms = Some(f.t_ms);
                self.used.push_back(fix_used);
                if self.used.len() > LINE_FIT_FIXES {
                    self.used.pop_front();
                }
                self.update_hold(z, scale(&meas.r_pos, r_scale), f.t_ms); // the noise the filter used (ruling T7-soft)
                let e = self.emit(f.t_ms, if soft { Verdict::Soft } else { Verdict::Used });
                if matches!(e.verdict, Verdict::Used | Verdict::Soft) {
                    self.reanchor_warmup = self.reanchor_warmup.saturating_sub(1);
                }
                e
            }
        }
    }

    /// A clock jump back to `t_ms`: forget the filter and the fix clock (the Reset this fix becomes breaks the trace), and the step
    /// and compass readings dated after the jump, which would drop the real ones that follow.
    fn clock_jump(&mut self, t_ms: i64) {
        self.clear_filter();
        self.last_t_ms = None;
        let ahead = |s: Option<i64>| s.is_some_and(|s| s > t_ms.saturating_add(self.params.reset_gap_ms));
        if ahead(self.steps.last_event_ms().or(self.steps.latest().map(|(t, _)| t))) {
            self.steps = StepHistory::default();
        }
        if ahead(self.compass.newest_ms()) {
            self.compass = Compass::default();
        }
    }

    /// The filter's predictions at `t_ms`, with the maneuver's inflated process noise when `inflated`.
    fn predict(&self, imm: &Imm, t_ms: i64, inflated: bool) -> ([imm::Gaussian; 3], [f64; 3]) {
        if !inflated {
            return imm.predict_to(t_ms, self.mode, &self.params);
        }
        let mut q = self.params.clone();
        let k = q.maneuver_sigma_a_scale;
        (q.sigma_a_walk, q.sigma_a_run, q.sigma_a_bike, q.sigma_a_drive) = (q.sigma_a_walk * k, q.sigma_a_run * k, q.sigma_a_bike * k, q.sigma_a_drive * k);
        imm.predict_to(t_ms, self.mode, &q)
    }

    fn sigma_of(&self, f: &RawFix) -> f64 {
        let s = (f.accuracy_m / ACC_TO_SIGMA).max(self.params.sigma_floor_m);
        if f.provider == Provider::Network {
            s * self.params.network_r_factor.sqrt()
        } else {
            s
        }
    }

    #[allow(clippy::many_single_char_names)] // f, z, v, b, r, u, w: the fix, its position, speed, bearing, R and the along/across axes
    pub(crate) fn measurement(&self, f: &RawFix, z: [f64; 2]) -> Meas {
        let sigma = self.sigma_of(f);
        let vel = match (f.speed_mps, f.speed_acc_mps, f.bearing_deg, f.bearing_acc_deg) {
            (Some(v), Some(va), Some(b), Some(bacc))
                if v.is_finite() && b.is_finite() && v >= self.params.min_speed_for_velocity_mps && va > 0.0 && bacc > 0.0 && bacc < 180.0 =>
            {
                let (sb, cb) = b.to_radians().sin_cos();
                let (along, cross) = (va * va, (v * bacc.to_radians().sin()).powi(2).max(0.01));
                let (u, w) = ([sb, cb], [cb, -sb]);
                let r: Mat<2, 2> = std::array::from_fn(|i| std::array::from_fn(|j| along * u[i] * u[j] + cross * w[i] * w[j]));
                Some(([v * sb, v * cb], r))
            }
            _ => None,
        };
        Meas { pos: z, r_pos: scale(&identity::<2>(), sigma * sigma), vel }
    }

    /// Restart the filter at `f`: a `Reset`.
    fn reset_at(&mut self, f: &RawFix) -> Estimate {
        self.start(f, self.sigma_of(f));
        self.emit(f.t_ms, Verdict::Reset)
    }

    fn start(&mut self, f: &RawFix, sigma: f64) {
        self.frame = Some(Frame::new(f.point()));
        self.imm = Some(Imm::new([0.0, 0.0], sigma, f.t_ms, self.mode, &self.params));
        self.last_taken_ms = Some(f.t_ms);
        self.started_ms = Some(f.t_ms);
        self.reloc.clear();
        self.hold = None;
        self.used.clear();
        self.disagreeing = 0;
        self.maneuver_course = None;
        self.reanchor_warmup = 0; // a restart from a fix owes nothing to the cloud
        self.reanchor_floor_m2 = None;
    }

    /// Restart (after [`Self::start`]) moving at `v`, the agreeing fixes' velocity, with model probabilities from its speed (ruling T8-R4).
    fn start_moving(&mut self, f: &RawFix, v: [f64; 2], v_sigma: f64) {
        let p = &self.params;
        let speed = v[0].hypot(v[1]);
        let mu = if speed > p.reloc_fast_mps {
            p.mu_reloc_fast
        } else if speed >= p.min_speed_for_velocity_mps {
            p.mu_reloc_walk
        } else {
            return;
        };
        let sigma = self.sigma_of(f);
        self.imm = Some(Imm::moving([0.0, 0.0], sigma, v, v_sigma.max(p.min_speed_for_velocity_mps), mu, f.t_ms, p));
    }

    fn simulated(&mut self, f: &RawFix) -> Estimate {
        // Believed whatever its accuracy, mock flag or time, but never at an invalid point (adversarial review I3).
        let valid = (-90.0..=90.0).contains(&f.lat) && (-180.0..=180.0).contains(&f.lon) && f.accuracy_m.is_finite();
        if !valid {
            return self.unusable(f);
        }
        self.clear_filter(); // the matcher goes on: a dev walk is one line (review M6)
        self.restarted = true;
        self.last_sim = true;
        self.last_t_ms = Some(f.t_ms);
        self.start(f, self.params.sim_uncertainty_m / ACC_TO_SIGMA);
        self.emit(f.t_ms, Verdict::Used)
    }

    fn unusable(&self, f: &RawFix) -> Estimate {
        let finite = |x: f64| if x.is_finite() { x } else { 0.0 };
        let fallback = Estimate {
            t_ms: f.t_ms,
            lat: finite(f.lat),
            lon: finite(f.lon),
            uncertainty_m: if f.accuracy_m.is_finite() { f.accuracy_m.clamp(1.0, 1e6) } else { 1e6 },
            ..Estimate::default()
        };
        Estimate { verdict: Verdict::Unusable, accepted: false, ..self.last.unwrap_or(fallback) }
    }

    /// Move the anchor to the estimate once it is 5 km out; every position is mapped exactly (old ENU -> geo -> new ENU).
    fn reanchor_if_far(&mut self) {
        let (Some(imm), Some(frame)) = (self.imm.as_mut(), self.frame) else { return };
        let out = imm.output();
        let pos = self.hold.map_or([out.x[0], out.x[1]], |h| h.at);
        if !frame.needs_reanchor(pos) {
            return;
        }
        let new = Frame::new(frame.to_geo(pos));
        let map = |en: [f64; 2]| new.to_enu(frame.to_geo(en));
        imm.map_positions(&map);
        self.reloc.map_positions(&map);
        for u in &mut self.used {
            u.en = map(u.en);
        }
        if let Some(h) = &mut self.hold {
            h.at = map(h.at);
        }
        self.frame = Some(new);
    }

    fn emit(&mut self, t_ms: i64, verdict: Verdict) -> Estimate {
        self.reanchor_if_far();
        let (Some(imm), Some(frame)) = (self.imm.as_ref(), self.frame.as_ref()) else {
            return Estimate { t_ms, verdict: Verdict::Unusable, uncertainty_m: 1e6, ..Estimate::default() };
        };
        let p = &self.params;
        let out = imm.output();
        let pos = self.hold.map_or([out.x[0], out.x[1]], |h| h.at);
        let uncertainty_m = ACC_TO_SIGMA * max_eig2(&imm::pos_block(&out.p)).max(0.0).sqrt();
        let (speed, vb) = (out.x[2].hypot(out.x[3]), imm::vel_block(&out.p));
        let (course_deg, course_sigma, fit_from) = match self.maneuver_course {
            Some((c, teach)) => (Some(c), teach.map(|(s, _)| s), teach.map(|(_, t)| t)),
            None => Self::course_of(imm, p).map_or((None, None, None), |(c, s)| (Some(c), Some(s), None)),
        };
        let best = (0..3).max_by(|&a, &b| imm.mu[a].total_cmp(&imm.mu[b])).unwrap_or(S);
        let at = frame.to_geo(pos);
        // An update too uncertain to count is Blurry; a restart keeps its verdict (the pin snaps, the trace breaks, trackers pause) and is
        // only not accepted (ruling FR-I1).
        let sure = uncertainty_m <= p.max_uncertainty_m;
        let verdict = if matches!(verdict, Verdict::Used | Verdict::Soft) && !sure { Verdict::Blurry } else { verdict };
        let est = Estimate {
            t_ms,
            lat: at.lat,
            lon: at.lon,
            uncertainty_m,
            speed_mps: speed,
            speed_sigma_mps: f64::midpoint(vb[0][0], vb[1][1]).max(0.0).sqrt(),
            course_deg,
            motion: [Motion::Stationary, Motion::Walking, Motion::Fast][best],
            mode_probs: imm.mu.map(to_f32),
            source: Source::Gps,
            verdict,
            // After a bridge re-anchor, the first fixes only pull the estimate off the cloud's prior (adversarial review C1).
            // A network position can be spoofed without the mock flag: display only (adversarial review I2).
            accepted: verdict.may_count() && sure && self.reanchor_warmup == 0 && !self.network_fix,
        };
        if est.accepted {
            self.last_accepted_ms = Some(t_ms);
        }
        self.last = Some(est);
        self.last_course_sigma_deg = course_sigma;
        self.last_course_fit_from_ms = fit_from;
        est
    }
}

#[cfg(test)]
mod tests {

    use crate::catalog::Mode;
    use crate::geo::{destination, distance_m, Point};

    use crate::loc::imm::{self};

    use crate::loc::{Estimate, HeadingIn, Motion, Provider, RawFix, Verdict, ACC_TO_SIGMA};
    use crate::num::i64_to_f64;

    use crate::loc::locator::test_util::*;
    use crate::loc::locator::{Locator, Odometer};

    #[test]
    fn the_first_fix_starts_the_filter_at_the_fix() {
        let mut l = Locator::default();
        let e = l.on_fix(&fix(o(), 1, 5.0));
        assert_eq!(e.verdict, Verdict::Reset);
        assert!(e.accepted && distance_m(e.point(), o()) < 1e-6);
        assert!((e.uncertainty_m - 5.0).abs() < 0.01, "{}", e.uncertainty_m);
    }

    #[test]
    fn a_coarse_first_fix_is_a_reset_that_does_not_count() {
        // Ruling FR-I1: a restart keeps its verdict (the pin snaps, trackers pause); over 35 m it is only not accepted.
        let mut l = Locator::default();
        let e = l.on_fix(&fix(o(), 1, 50.0));
        assert_eq!(e.verdict, Verdict::Reset);
        assert!(!e.accepted && l.restarted());
    }

    /// Walk east 60 s at 1.4 m/s on 5 m fixes with Doppler speed, then reappear 500 m north with three agreeing fixes at
    /// `acc_far` and walk on at 5 m: the estimates of the second part, the odometer after the walk and at the end, and whether the
    /// third far fix restarted the filter.
    fn walk_then_relocate(acc_far: f64) -> (Vec<Estimate>, f64, f64, bool, usize) {
        let mut l = Locator::default();
        let mut odo = Odometer::default();
        let mut total = 0.0;
        let moving = |p: Point, t: i64, acc: f64| RawFix {
            speed_mps: Some(1.4),
            speed_acc_mps: Some(0.3),
            bearing_deg: Some(90.0),
            bearing_acc_deg: Some(10.0),
            ..fix(p, t, acc)
        };
        for t in 0..60 {
            let e = l.on_fix(&moving(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, 5.0));
            total += odo.step_with(&e, l.hold_relocation_speed());
        }
        let walked = total;
        let base = destination(destination(o(), 90.0, 84.0), 0.0, 500.0);
        let mut out = Vec::new();
        let mut restarted = false;
        for i in 0..12 {
            let e = l.on_fix(&moving(destination(base, 90.0, 1.4 * i64_to_f64(i)), 60 + i, if i < 3 { acc_far } else { 5.0 }));
            total += odo.step_with(&e, l.hold_relocation_speed());
            restarted |= i == 2 && l.restarted();
            out.push(e);
        }
        (out, walked, total, restarted, l.trace_matched().1.len())
    }

    #[test]
    fn a_coarse_relocation_still_restarts_and_adds_no_distance() {
        // Final review I1: three agreeing fixes 500 m away at 45 m accuracy relocate the filter. The estimate is too uncertain to count,
        // but it is still a restart: the pin snaps, the trace breaks, and the odometer adds nothing for the jump (it added 500 m).
        let (est, walked, total, restarted, runs) = walk_then_relocate(45.0);
        assert_eq!(est[2].verdict, Verdict::Relocated, "{:?}", est[2]);
        assert!(!est[2].accepted && restarted);
        assert!(est[3..].iter().any(|e| e.accepted));
        assert!(total - walked < 20.0, "odometer {walked:.1} -> {total:.1}");
        assert_eq!(runs, 2, "the trace breaks at the relocation");
        // The same burst at 10 m counts at once, and adds nothing either.
        let (est, walked, total, restarted, runs) = walk_then_relocate(10.0);
        assert!(est[2].verdict == Verdict::Relocated && est[2].accepted && restarted && runs == 2);
        assert!(total - walked < 20.0, "odometer {walked:.1} -> {total:.1}");
    }

    #[test]
    fn a_spike_is_gated_and_does_not_move_the_estimate() {
        let mut l = Locator::default();
        for t in 1..=20 {
            l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, 5.0));
        }
        let before = l.last().unwrap();
        let e = l.on_fix(&fix(destination(o(), 0.0, 150.0), 21, 5.0));
        assert_eq!(e.verdict, Verdict::Gated);
        assert!(!e.accepted && distance_m(e.point(), before.point()) < 3.0);
    }

    #[test]
    fn three_far_fixes_in_a_row_relocate() {
        let mut l = Locator::default();
        let t = stand(&mut l, 1, 10);
        let far = destination(o(), 90.0, 5000.0);
        let v: Vec<Verdict> = (0..3).map(|i| l.on_fix(&fix(far, t + i, 5.0)).verdict).collect();
        assert_eq!(v, [Verdict::Gated, Verdict::Gated, Verdict::Relocated]);
        assert!(distance_m(l.last().unwrap().point(), far) < 1e-6);
    }

    #[test]
    fn a_gap_over_five_minutes_resets() {
        let mut l = Locator::default();
        l.on_fix(&fix(o(), 0, 5.0));
        assert_eq!(l.on_fix(&fix(destination(o(), 0.0, 20_000.0), 301, 5.0)).verdict, Verdict::Reset);
    }

    #[test]
    fn simulated_fixes_bypass_the_filter() {
        let mut l = Locator::default();
        let sim = |p: Point, t: i64| RawFix { provider: Provider::Sim, ..fix(p, t, 5.0) };
        let a = l.on_fix(&sim(o(), 1));
        let b = l.on_fix(&sim(destination(o(), 0.0, 5000.0), 2));
        assert!(a.accepted && b.accepted && a.verdict == Verdict::Used && b.verdict == Verdict::Used);
        assert!((b.uncertainty_m - 3.0).abs() < 1e-9 && distance_m(b.point(), destination(o(), 0.0, 5000.0)) < 1e-6);
    }

    #[test]
    fn a_real_fix_after_simulated_ones_starts_fresh() {
        // Review Focus 3: the simulator's clock runs ahead and it teleports; the first real fix must not be gated or dropped as stale.
        let mut l = Locator::default();
        l.on_fix(&RawFix { provider: Provider::Sim, ..fix(destination(o(), 0.0, 3000.0), 1_000_000, 5.0) });
        let e = l.on_fix(&fix(o(), 10, 5.0));
        assert_eq!(e.verdict, Verdict::Reset);
        assert!(e.accepted && distance_m(e.point(), o()) < 1e-6);
    }

    #[test]
    fn real_steps_and_compass_after_simulated_fixes_are_kept() {
        // Final review M1: the simulator's clock runs ahead, so steps and compass readings stamped on its clock would drop the real ones
        // that follow (not newer than the last). The first real fix clears both.
        let mut l = Locator::default();
        let heading = |az: f64, t_ms: i64| HeadingIn {
            t_ms,
            azimuth_deg: az,
            accuracy: crate::loc::CompassAccuracy::High,
            pitch_deg: 0.0,
            roll_deg: 0.0,
            error_deg: None,
        };
        let sim_t = 1_000_000_000;
        l.on_steps(50_400, sim_t, None);
        l.on_heading(&HeadingIn { t_ms: sim_t, ..heading(90.0, 0) });
        l.on_fix(&RawFix { provider: Provider::Sim, ..fix(o(), sim_t / 1000, 5.0) });
        l.on_fix(&fix(o(), 10, 5.0));
        l.on_steps(1_000, 10_500, None);
        l.on_heading(&heading(180.0, 10_600));
        assert_eq!(l.steps().latest(), Some((10_500, 1_000)));
        assert_eq!(l.compass.latest(10_600, 1_000).map(|h| h.azimuth_deg), Some(180.0));
    }

    #[test]
    fn crossing_the_reanchor_distance_moves_nothing() {
        // Review Focus 4: a 7.2 km ride at 8 m/s crosses the 5 km re-anchor; the estimate and the odometer stay smooth.
        let mut l = Locator::default();
        l.set_mode(Mode::Bike);
        let mut odo = Odometer::default();
        let (mut total, mut prev, mut reanchors): (f64, Option<Estimate>, usize) = (0.0, None, 0);
        for t in 0..=900 {
            let origin = l.frame.map(|f| f.origin());
            let e = l.on_fix(&fix(destination(o(), 90.0, 8.0 * i64_to_f64(t)), t, 4.0));
            total += odo.step(&e);
            // The first seconds are the filter warming up in a Bike zone; this test is about the re-anchor.
            if let Some(p) = prev.filter(|_| t >= 3) {
                let step = distance_m(p.point(), e.point());
                assert!(step < 9.5, "jump at {t}: {step}");
                if origin.is_some_and(|a| Some(a) != l.frame.map(|f| f.origin())) {
                    reanchors += 1;
                    assert!((step - 8.0).abs() <= 0.1, "the re-anchor step at {t}: {step}");
                }
            }
            prev = Some(e);
        }
        assert_eq!(reanchors, 1, "7.2 km from the start crosses the 5 km re-anchor once");
        assert!((total - 7200.0).abs() < 72.0, "odometer {total}");
    }

    #[test]
    fn a_second_fix_with_the_same_time_is_unusable_and_moves_nothing() {
        // Ruling T7-tests (Review Focus 1, end to end): a duplicate t_ms (the phone redelivered a fix) never updates the filter twice.
        let mut l = Locator::default();
        let t = walk(&mut l, 1, 20);
        let before = l.last().unwrap();
        let e = l.on_fix(&fix(destination(o(), 0.0, 4.0), t - 1, 5.0));
        assert_eq!(e.verdict, Verdict::Unusable);
        assert!(!e.accepted && e.point() == before.point() && l.last() == Some(before));
        assert!(l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t - 1)), t, 5.0)).accepted, "the next fix is used as usual");
    }

    #[test]
    fn odd_fix_fields_never_break_the_estimate() {
        // Review Focus 5: accuracy 0, negative or NaN speed, missing or negative (iOS "unknown") accuracies.
        let mut l = Locator::default();
        let base = fix(o(), 1, 0.0);
        let odd = [
            base,
            RawFix { t_ms: 2000, speed_mps: Some(-1.0), speed_acc_mps: Some(0.0), bearing_deg: Some(90.0), bearing_acc_deg: None, ..base },
            RawFix { t_ms: 3000, speed_mps: Some(1.0), speed_acc_mps: Some(0.5), bearing_deg: Some(90.0), bearing_acc_deg: Some(-1.0), ..base },
            RawFix { t_ms: 4000, speed_mps: Some(f64::NAN), speed_acc_mps: Some(0.5), bearing_deg: Some(f64::NAN), bearing_acc_deg: Some(10.0), ..base },
        ];
        let first = l.on_fix(&odd[0]);
        assert!((first.uncertainty_m - 2.0 * ACC_TO_SIGMA).abs() < 1e-6, "accuracy 0 still gets the 2 m sigma floor: {first:?}");
        for f in &odd[1..] {
            let e = l.on_fix(f);
            assert!(e.accepted, "{e:?}");
            assert!(e.uncertainty_m.is_finite() && e.uncertainty_m > 0.0 && e.lat.is_finite() && e.speed_mps.is_finite(), "{e:?}");
        }
        assert!(l.measurement(&odd[1], [0.0, 0.0]).vel.is_none() && l.measurement(&odd[2], [0.0, 0.0]).vel.is_none());
        assert!(l.measurement(&odd[3], [0.0, 0.0]).vel.is_none());
    }

    #[test]
    fn a_network_fix_counts_double_sigma_before_gnss_and_is_dropped_after() {
        let mut l = Locator::default();
        let net = RawFix { provider: Provider::Network, ..fix(o(), 1, 30.0) };
        let e = l.on_fix(&net);
        assert!((e.uncertainty_m - 60.0).abs() < 0.1, "R x 4 = sigma x 2: {}", e.uncertainty_m);
        assert!(!e.accepted, "display only (adversarial review I2)");
        l.on_fix(&RawFix { provider: Provider::Gps, ..fix(o(), 2, 5.0) });
        assert_eq!(l.on_fix(&RawFix { t_ms: 3000, ..net }).verdict, Verdict::Unusable);
    }

    #[test]
    fn network_fixes_never_count_however_many_agree() {
        // Adversarial review I2: smoothing several Wi-Fi fixes shrank the uncertainty under 35 m, so they counted for quests. A network
        // position is display only (it can be spoofed without the mock flag).
        for acc in [5.0, 12.0, 17.0, 25.0, 30.0] {
            let mut l = Locator::default();
            for k in 0..30 {
                let p = destination(o(), f64::from(u16::try_from(k * 71 % 360).unwrap()), 3.0);
                let e = l.on_fix(&RawFix { provider: Provider::Network, ..fix(p, 1 + k * 5, acc) });
                assert!(!e.accepted && e.verdict != Verdict::Unusable, "acc {acc}, fix {k}: {e:?}");
            }
            assert!(l.on_fix(&RawFix { provider: Provider::Gps, ..fix(o(), 200, 5.0) }).accepted, "a GNSS fix counts at once");
        }
    }

    #[test]
    #[allow(clippy::many_single_char_names)] // l, t, p, f, e: the locator, time, point, fix and estimate
    fn random_fixes_never_panic_and_stay_finite() {
        // Property test (spec "proptest-style"): seeded random walks, jumps, NaNs, time jitter, any provider, mock fixes and valid or
        // invalid velocity fields (ruling T7-tests).
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};
        const PROVIDERS: [Provider; 6] = [Provider::Fused, Provider::Gps, Provider::Network, Provider::Ios, Provider::Sim, Provider::Other];
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let odd = |rng: &mut StdRng, lo: f64, hi: f64| match rng.random_range(0..6) {
                0 => None,
                1 => Some(f64::NAN),
                2 => Some(-rng.random_range(0.0..hi)),
                _ => Some(rng.random_range(lo..hi)),
            };
            let mut l = Locator::default();
            let (mut t, mut last_accepted) = (0_i64, None::<i64>);
            for _ in 0..500 {
                t += rng.random_range(-500..3_000);
                let p = destination(o(), rng.random_range(0.0..360.0), rng.random_range(0.0..3_000.0));
                let acc = if rng.random_range(0..50) == 0 { f64::NAN } else { rng.random_range(0.0..150.0) };
                let f = RawFix {
                    speed_mps: odd(&mut rng, 0.0, 40.0),
                    speed_acc_mps: odd(&mut rng, 0.0, 5.0),
                    bearing_deg: odd(&mut rng, 0.0, 360.0),
                    bearing_acc_deg: odd(&mut rng, 0.0, 200.0),
                    provider: PROVIDERS[rng.random_range(0..PROVIDERS.len())],
                    mock: rng.random_range(0..10) == 0,
                    ..RawFix::at(p.lat, p.lon, t, acc)
                };
                let e = l.on_fix(&f);
                assert!(e.uncertainty_m.is_finite() && e.uncertainty_m > 0.0, "{e:?}");
                assert!(e.lat.is_finite() && e.lon.is_finite() && e.speed_mps.is_finite() && e.speed_sigma_mps.is_finite(), "{e:?}");
                let sum: f32 = e.mode_probs.iter().sum();
                assert!(e.verdict == Verdict::Unusable || (sum - 1.0).abs() < 1e-4, "{e:?}");
                if f.provider == Provider::Sim {
                    // A simulated fix is always believed and the next real fix starts fresh on its own clock.
                    last_accepted = None;
                } else if e.accepted {
                    assert!(last_accepted.is_none_or(|a| e.t_ms > a), "accepted {} after {last_accepted:?}", e.t_ms);
                    last_accepted = Some(e.t_ms);
                }
            }
        }
    }

    #[test]
    fn a_stale_fix_after_a_gated_one_is_unusable_and_moves_nothing() {
        // T6-wire: the fix clock is the last fix seen, so an older fix after a gated one never steps the estimate back.
        let mut l = Locator::default();
        let t = walk(&mut l, 1, 20);
        assert_eq!(l.on_fix(&fix(destination(o(), 0.0, 150.0), t, 5.0)).verdict, Verdict::Gated);
        let before = l.last().unwrap();
        let stale = RawFix { t_ms: t * 1000 - 500, ..fix(destination(o(), 90.0, 26.0), t, 5.0) };
        let e = l.on_fix(&stale);
        assert_eq!(e.verdict, Verdict::Unusable);
        assert!(!e.accepted && e.point() == before.point() && l.last() == Some(before));
    }

    #[test]
    fn relocation_speed_is_the_global_cap_not_the_zone_mode() {
        // Ruling T8-R4: far fixes moving 30 m/s agree whatever the zone: a walker who boards a train is relocated, not gated for ever.
        let far = |k: i64| destination(o(), 90.0, 5000.0 + 30.0 * i64_to_f64(k));
        for mode in [Mode::Walk, Mode::Drive] {
            let mut l = Locator::default();
            l.set_mode(mode);
            let t = stand(&mut l, 1, 10);
            let v: Vec<Verdict> = (0..3).map(|k| l.on_fix(&fix(far(k), t + k, 5.0)).verdict).collect();
            assert_eq!(v, [Verdict::Gated, Verdict::Gated, Verdict::Relocated], "{mode:?}");
        }
    }

    #[test]
    fn a_relocation_restarts_moving_at_the_speed_of_the_agreeing_fixes() {
        // Ruling T8-R4: the restart takes its velocity from the agreeing fixes and its model probabilities from that speed, so a driver
        // in a Walk zone is tracked on from the relocation instead of being gated again.
        let mut l = Locator::default();
        let t = stand(&mut l, 1, 10);
        let at = |k: i64| fix(destination(o(), 90.0, 3000.0 + 14.0 * i64_to_f64(k)), t + k, 5.0);
        let k = (0..5).find(|&k| l.on_fix(&at(k)).verdict == Verdict::Relocated).expect("relocated");
        let e = l.last().unwrap();
        assert!((e.speed_mps - 14.0).abs() < 2.0 && e.motion == Motion::Fast, "{e:?}");
        assert!(e.course_deg.is_some_and(|c| (c - 90.0).abs() < 10.0), "{e:?}");
        for j in k + 1..k + 20 {
            assert!(l.on_fix(&at(j)).accepted, "tracked on at {j}");
        }
    }

    #[test]
    fn a_fix_with_velocity_is_gated_on_four_degrees_of_freedom() {
        // T6-wire: the gate uses 4 dof (18.5) when the fix has a velocity: an offset gated on position alone (> 13.8) is soft with it.
        let mut l = Locator::default();
        let t = stand(&mut l, 1, 40);
        let d2 = |l: &Locator, f: &RawFix| {
            let z = l.frame.unwrap().to_enu(f.point());
            let (preds, _) = l.imm.as_ref().unwrap().predict_to(f.t_ms, l.mode, &l.params);
            imm::min_d2(&preds, &l.measurement(f, z))
        };
        let at = |m: f64| fix(destination(o(), 90.0, m), t, 6.0);
        let offset = (0..500).map(|i| 0.1 * f64::from(i)).find(|&m| d2(&l, &at(m)) > 14.5).unwrap();
        let pos_only = at(offset);
        let with_vel = RawFix { speed_mps: Some(0.5), speed_acc_mps: Some(2.0), bearing_deg: Some(90.0), bearing_acc_deg: Some(90.0), ..pos_only };
        assert!(l.measurement(&with_vel, [0.0, 0.0]).vel.is_some());
        let (d_pos, d_vel) = (d2(&l, &pos_only), d2(&l, &with_vel));
        assert!(d_pos > 13.8 && d_vel > 13.28 && d_vel <= 18.5, "{d_pos} {d_vel}");
        assert_eq!(l.clone().on_fix(&pos_only).verdict, Verdict::Gated);
        assert_eq!(l.on_fix(&with_vel).verdict, Verdict::Soft);
    }

    #[test]
    fn an_accepted_fix_or_a_reset_forgets_the_gated_fixes() {
        // T6-wire: Relocator::clear on accept and on reset.
        let far = destination(o(), 90.0, 5000.0);
        let mut l = Locator::default();
        let t = stand(&mut l, 1, 10);
        assert_eq!(l.on_fix(&fix(far, t, 5.0)).verdict, Verdict::Gated);
        assert_eq!(l.on_fix(&fix(far, t + 1, 5.0)).verdict, Verdict::Gated);
        assert!(l.on_fix(&fix(o(), t + 2, 5.0)).accepted);
        let v: Vec<Verdict> = (3..5).map(|k| l.on_fix(&fix(far, t + k, 5.0)).verdict).collect();
        assert_eq!(v, [Verdict::Gated, Verdict::Gated], "the accepted fix started the count over");
        l.reset();
        assert!(l.last().is_none() && !l.holding());
        assert_eq!(l.on_fix(&fix(o(), t + 5, 5.0)).verdict, Verdict::Reset);
        assert_eq!(l.on_fix(&fix(far, t + 6, 5.0)).verdict, Verdict::Gated, "the reset forgot the earlier gated fixes");
    }

    #[test]
    fn a_reset_starts_fresh_and_forgets_the_fix_clock() {
        // T6-wire: counting toggled or game opened. Adversarial review I1: the fix clock goes too, so a reset always recovers.
        let mut l = Locator::default();
        let t = stand(&mut l, 1, 40);
        assert!(l.holding());
        l.reset();
        assert!(!l.holding() && l.last().is_none());
        let e = l.on_fix(&fix(destination(o(), 0.0, 30.0), t - 5, 5.0));
        assert_eq!(e.verdict, Verdict::Reset, "an older fix after a reset starts the filter");
        assert!(distance_m(e.point(), destination(o(), 0.0, 30.0)) < 1e-6);
    }

    #[test]
    fn a_future_dated_fix_does_not_lock_out_the_session() {
        // Adversarial review I1: one fix a day ahead (a wrong clock) made every real fix after it Unusable until the game was reopened.
        let mut l = Locator::default();
        let t = walk(&mut l, 1, 20);
        // A day after the last fix: still in the sane range (M1), so the filter takes it.
        assert_eq!(l.on_fix(&fix(destination(o(), 90.0, 30.0), t - 1 + 86_400, 5.0)).verdict, Verdict::Reset);
        let next = l.on_fix(&fix(destination(o(), 90.0, 28.0), t, 5.0));
        assert_eq!(next.verdict, Verdict::Reset, "a clock jump back restarts the filter: {next:?}");
        assert!(next.accepted && distance_m(next.point(), destination(o(), 90.0, 28.0)) < 1e-6);
        let on: Vec<bool> = (1..10).map(|k| l.on_fix(&fix(destination(o(), 90.0, 28.0 + 1.4 * i64_to_f64(k)), t + k, 5.0)).accepted).collect();
        assert!(on.iter().all(|a| *a), "{on:?}");
        // However far ahead (inside the sane range, re-review N3), the next real fix restarts the filter at once.
        let t = t + 10;
        assert_eq!(l.on_fix(&fix(o(), t + 365 * 86_400, 5.0)).verdict, Verdict::Reset);
        let next = l.on_fix(&fix(destination(o(), 90.0, 28.0 + 1.4 * 10.0), t, 5.0));
        assert!(next.verdict == Verdict::Reset && next.accepted, "{next:?}");
    }

    #[test]
    fn a_small_step_back_in_time_is_unusable_and_a_big_one_restarts() {
        // Adversarial review I1: up to the reset gap older than the last fix is stale (Unusable); more is a clock jump (Reset).
        let gap_s = Locator::default().params().reset_gap_ms / 1000;
        let mut l = Locator::default();
        let t = walk(&mut l, gap_s + 10, 20);
        assert_eq!(l.on_fix(&fix(o(), t - 1 - gap_s, 5.0)).verdict, Verdict::Unusable, "exactly the reset gap back");
        assert_eq!(l.on_fix(&fix(o(), t - 2 - gap_s, 5.0)).verdict, Verdict::Reset, "a second more");
    }

    /// A real epoch, ms: the sensor-clock tests run on the phone's clock, not one from 0.
    const EPOCH_MS: i64 = 1_800_000_000_000;

    fn fix_ms(p: Point, t_ms: i64) -> RawFix {
        RawFix::at(p.lat, p.lon, t_ms, 8.0)
    }

    /// Walk east 60 s from `EPOCH_MS` with 1 Hz fixes and a step event every 2 s.
    fn walk_with_steps(l: &mut Locator) {
        for t in 0..60 {
            if t % 2 == 0 {
                l.on_steps(10_000 + t * 2, EPOCH_MS + t * 1000, None);
            }
            l.on_fix(&fix_ms(destination(o(), 90.0, 1.4 * i64_to_f64(t)), EPOCH_MS + t * 1000));
        }
    }

    #[test]
    fn a_bus_ride_outside_zones_after_the_last_step_keeps_its_fixes() {
        // Adversarial re-review N1: outside zones (a fix every 90 s, no compass) the player boards a bus 20 s after a fix. A fix dated
        // a minute after the newest step was dropped, so every fix of the ride was, until the next step.
        let mut l = Locator::default();
        let (mut steps, mut pos) = (1_000, o());
        for t in 0..=560 {
            if t % 10 == 0 {
                steps += 18;
                l.on_steps(steps, EPOCH_MS + t * 1000, None);
            }
            if t % 90 == 0 && t <= 540 {
                l.on_fix(&fix_ms(pos, EPOCH_MS + t * 1000));
            }
            pos = destination(pos, 90.0, 1.4);
        }
        let dropped: Vec<i64> = (1..=14)
            .filter(|k| {
                pos = destination(pos, 90.0, 900.0);
                l.on_fix(&fix_ms(pos, EPOCH_MS + (540 + 90 * k) * 1000)).verdict == Verdict::Unusable
            })
            .collect();
        assert!(dropped.is_empty(), "bus fixes dropped: {dropped:?}");
    }

    #[test]
    fn a_presence_resume_without_a_new_step_keeps_its_fixes() {
        // Adversarial re-review N1: home for 20 h (counting off and on again: two resets), then fixes with no new step event.
        let mut l = Locator::default();
        walk_with_steps(&mut l);
        l.reset();
        let back = EPOCH_MS + 60_000 + 20 * 3_600_000;
        l.reset();
        let dropped =
            (0..600).filter(|&k| l.on_fix(&fix_ms(destination(o(), 90.0, 1.4 * i64_to_f64(k)), back + k * 1000)).verdict == Verdict::Unusable).count();
        assert_eq!(dropped, 0);
    }

    #[test]
    fn a_gps_gap_over_the_end_of_a_walk_without_a_compass_keeps_its_fixes() {
        // Adversarial re-review N1: GPS lost at 60 s (an underground garage), steps until 100 s (the walk to the car), no compass, then
        // the drive out with GPS back at 200 s.
        let mut l = Locator::default();
        walk_with_steps(&mut l);
        for t in (60..100).step_by(2) {
            l.on_steps(10_000 + t * 2, EPOCH_MS + t * 1000, None);
        }
        let street = |t: i64| destination(o(), 0.0, 10.0 * i64_to_f64(t));
        let dropped = (200..500).filter(|&t| l.on_fix(&fix_ms(street(t), EPOCH_MS + t * 1000)).verdict == Verdict::Unusable).count();
        assert_eq!(dropped, 0);
    }

    #[test]
    fn a_phone_left_a_day_in_a_drawer_takes_its_first_fix() {
        // Adversarial re-review N3: counting on, no fix, step or compass reading for 25 h, then fixes without steps. A fix more than a
        // day after the newest time seen was dropped until a sensor reading came.
        let mut l = Locator::default();
        walk_with_steps(&mut l);
        let e = l.on_fix(&fix_ms(o(), EPOCH_MS + 60_000 + 25 * 3_600_000));
        assert_eq!(e.verdict, Verdict::Reset, "{e:?}");
    }

    #[test]
    fn standing_with_a_quiet_step_counter_keeps_its_fixes() {
        // Adversarial review I1, edge: a step counter reports only on change, so its newest time ages while the player stands. A sensor
        // reading older than the last fix is no clock: the fixes go on being used.
        let mut l = Locator::default();
        l.on_steps(1_000, 500, None);
        let t = stand(&mut l, 1, 120);
        assert!(l.on_fix(&fix(o(), t, 6.0)).accepted, "two minutes after the last step");
    }

    #[test]
    fn simulated_fixes_skip_the_unusable_and_mock_rules() {
        // T6-wire: a simulated fix is always believed, even if mocked, coarse or older than the last fix.
        let mut l = Locator::default();
        l.on_fix(&fix(o(), 100, 5.0));
        let p = destination(o(), 0.0, 800.0);
        let e = l.on_fix(&RawFix { provider: Provider::Sim, mock: true, ..fix(p, 50, 500.0) });
        assert!(e.accepted && e.verdict == Verdict::Used && distance_m(e.point(), p) < 1e-6);
        assert!((e.uncertainty_m - 3.0).abs() < 1e-9);
    }

    #[test]
    fn walking_across_the_antimeridian_is_one_walk() {
        // Adversarial review M2: east across 180 degrees (Fiji) was two GPS jumps and a relocation.
        let mut l = Locator::default();
        let start = Point::new(-17.0, 179.9995);
        for t in 0..120 {
            let mut p = destination(start, 90.0, 1.4 * i64_to_f64(t));
            if p.lon >= 180.0 {
                p = Point::new(p.lat, p.lon - 360.0);
            }
            let e = l.on_fix(&fix(p, t, 5.0));
            assert!(!matches!(e.verdict, Verdict::Gated | Verdict::Relocated), "t {t}: {e:?}");
            assert!((-180.0..=180.0).contains(&e.lon) && distance_m(e.point(), p) < 10.0, "t {t}: {e:?} vs {p:?}");
        }
    }

    #[test]
    fn extreme_times_never_panic_and_stay_finite() {
        // Adversarial review M1: time arithmetic overflowed in debug builds (a panic UniFFI turns into an uncaught Kotlin exception).
        let heading =
            |t_ms: i64| HeadingIn { t_ms, azimuth_deg: 90.0, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 0.0, roll_deg: 0.0, error_deg: None };
        let mut l = Locator::default();
        l.on_fix(&fix(o(), 1, 5.0));
        let e = l.on_fix(&RawFix::at(40.0, -111.0, i64::MAX, 5.0));
        assert_eq!(e.verdict, Verdict::Unusable, "after 2100: {e:?}");
        assert!(l.display(i64::MAX).is_some_and(|d| d.lat.is_finite()));
        l.on_steps(1_000, i64::MIN, None);
        l.on_steps(1_010, i64::MAX, None);
        l.on_heading(&heading(i64::MIN));
        l.on_heading(&heading(i64::MAX));
        let mut l = Locator::default();
        l.on_fix(&RawFix::at(40.0, -111.0, i64::MIN + 1, 5.0));
        for t in [1000, i64::MAX, 2000] {
            let e = l.on_fix(&RawFix::at(40.0, -111.0, t, 5.0));
            assert!(e.lat.is_finite() && e.uncertainty_m.is_finite(), "{e:?}");
        }
    }

    #[test]
    fn a_simulated_fix_with_invalid_fields_is_unusable() {
        // Adversarial review I3: the simulator is believed, but never at a NaN or out-of-range point, nor with a non-finite accuracy.
        let mut l = Locator::default();
        l.on_fix(&fix(o(), 1, 5.0));
        let before = l.last().unwrap();
        let sim = |lat: f64, lon: f64, acc: f64| RawFix { provider: Provider::Sim, ..RawFix::at(lat, lon, 3000, acc) };
        for f in [sim(f64::NAN, -111.0, 5.0), sim(40.0, 1000.0, 5.0), sim(91.0, -111.0, 5.0), sim(40.0, -111.0, f64::NAN), sim(40.0, f64::INFINITY, 5.0)] {
            let e = l.on_fix(&f);
            assert!(e.verdict == Verdict::Unusable && !e.accepted && e.point() == before.point(), "{f:?}: {e:?}");
        }
        assert_eq!(l.on_fix(&sim(40.0, -111.0, f64::INFINITY)).verdict, Verdict::Unusable);
    }

    #[test]
    fn a_far_fix_after_a_minute_without_fixes_restarts_the_filter() {
        // Ruling T8-R3: the reset check runs on the predicted covariance. After 60 s without fixes the prediction is so wide that a fix
        // 2 km away would pass the gate as an ordinary update; it restarts the filter instead, and the odometer never counts the jump.
        let mut l = Locator::default();
        let t = walk(&mut l, 1, 60);
        let far = destination(o(), 0.0, 2000.0);
        let e = l.on_fix(&fix(far, t + 60, 5.0));
        assert_eq!(e.verdict, Verdict::Reset, "{e:?}");
        assert!(distance_m(e.point(), far) < 1e-6);
    }
}
