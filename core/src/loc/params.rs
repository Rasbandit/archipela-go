//! Every number of the location program in one place, so the bench can tune them (`replay --params p.json`). Defaults are the spec's.

use serde::{Deserialize, Serialize};

/// 2000-01-01 00:00 UTC, ms: a fix dated before it comes from a broken clock (adversarial review M1).
pub const MIN_FIX_T_MS: i64 = 946_684_800_000;
/// 2100-01-01 00:00 UTC, ms: a fix dated after it comes from a broken clock (adversarial re-review N3).
pub const MAX_FIX_T_MS: i64 = 4_102_444_800_000;

/// Parameters of the location filters. Grouped by layer: filter, display, matching, carry offset, step calibration, gap bridging.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_field_names)] // names mirror the spec's symbols
pub struct LocParams {
    /// Measurement sigma floor, metres.
    pub sigma_floor_m: f64,
    /// Fixes coarser than this (68 % radius, metres) are unusable.
    pub unusable_acc_m: f64,
    /// A network fix (cold start, no GNSS yet) has its R multiplied by this.
    pub network_r_factor: f64,
    /// Below this speed a fix's velocity is not measured, m/s.
    pub min_speed_for_velocity_mps: f64,
    /// Stationary model: position random walk, m^2 per second.
    pub q_stationary_m2_per_s: f64,
    /// Stationary model: velocity sigma, m/s.
    pub stationary_vel_sigma_mps: f64,
    /// White-noise acceleration of the walking model in a Walk zone (and Bike/Drive zones), m/s^2.
    pub sigma_a_walk: f64,
    /// White-noise acceleration of the walking model in a Run zone, m/s^2.
    pub sigma_a_run: f64,
    /// White-noise acceleration of the fast model (Walk, Run and Bike zones), m/s^2.
    pub sigma_a_bike: f64,
    /// White-noise acceleration of the fast model in a Drive zone, m/s^2.
    pub sigma_a_drive: f64,
    /// Initial velocity sigma in Walk/Run zones, m/s.
    pub v0_slow_mps: f64,
    /// Initial velocity sigma in Bike/Drive zones, m/s.
    pub v0_fast_mps: f64,
    /// Initial model probabilities in Walk/Run zones (S, W, F).
    pub mu0_slow: [f64; 3],
    /// Initial model probabilities in Bike/Drive zones.
    pub mu0_fast: [f64; 3],
    /// Per-second switching rates in Walk/Run zones (the diagonal is ignored: it is the remainder).
    pub pi_slow: [[f64; 3]; 3],
    /// Per-second switching rates in Bike/Drive zones.
    pub pi_fast: [[f64; 3]; 3],
    /// Longest single prediction step, seconds (longer gaps are predicted in steps).
    pub max_predict_s: f64,
    /// Most of a `Pi` row that may switch away in one step.
    pub max_offdiag_share: f64,
    /// Model probability floor.
    pub mu_floor: f64,
    /// Soft gate, 2 dof (99 %).
    pub gate_soft_2: f64,
    /// Hard gate, 2 dof (99.9 %).
    pub gate_hard_2: f64,
    /// Soft gate, 4 dof (99 %).
    pub gate_soft_4: f64,
    /// Hard gate, 4 dof (99.9 %).
    pub gate_hard_4: f64,
    /// Consecutive agreeing gated fixes that mean a real relocation.
    pub reloc_count: usize,
    /// ... spanning at least this, ms.
    pub reloc_span_ms: i64,
    /// After this much continuous gating, ms, two good agreeing fixes are enough.
    pub reloc_quick_after_ms: i64,
    /// "Good" for the quick rule, metres.
    pub reloc_quick_acc_m: f64,
    /// Fastest plausible movement between agreeing gated fixes, m/s: a global physical cap, whatever the zone's mode (ruling T8-R4).
    pub reloc_vmax_mps: f64,
    /// A relocation moving faster than this, m/s, restarts with `mu_reloc_fast`; slower but moving (>= `min_speed_for_velocity_mps`), with
    /// `mu_reloc_walk`; standing, with the zone's initial probabilities.
    pub reloc_fast_mps: f64,
    /// Model probabilities (S, W, F) of a restart moving at walking speed.
    pub mu_reloc_walk: [f64; 3],
    /// Model probabilities (S, W, F) of a restart moving faster.
    pub mu_reloc_fast: [f64; 3],
    /// Reset after this long without an accepted fix, ms. A fix older than the last one by more than this is a clock jump: it restarts
    /// the filter (adversarial review I1).
    pub reset_gap_ms: i64,
    /// A fix dated before this is unusable, ms: [`MIN_FIX_T_MS`]. The core's own unit tests run on a clock from 0 and use `i64::MIN`.
    pub min_fix_t_ms: i64,
    /// A fix dated after this is unusable, ms: [`MAX_FIX_T_MS`]. A fix dated wrong inside the range restarts the filter, and the next
    /// real fix, older by more than `reset_gap_ms`, restarts it again (adversarial re-review N1, N3).
    pub max_fix_t_ms: i64,
    /// Reset when the position covariance trace exceeds this, m^2.
    pub reset_trace_m2: f64,
    /// Hold: stationary probability above this ...
    pub hold_mu_s: f64,
    /// ... speed below this, m/s ...
    pub hold_speed_mps: f64,
    /// ... and no new steps for this long, ms.
    pub hold_quiet_steps_ms: i64,
    /// Hold ends after this many consecutive far fixes.
    pub hold_exit_fixes: u32,
    /// "Far" is at least this, metres (or 2 sigma of the estimate, if more).
    pub hold_exit_min_m: f64,
    /// With a quiet step counter (Walk/Run), "far" is at least this, metres (or `hold_quiet_sigmas` sigma, if more): how far a quiet
    /// counter may hide movement without steps (a wheelchair, a stroller; ruling FR-C1).
    pub hold_quiet_max_m: f64,
    /// ... in sigmas of the fix-to-held-point innovation. FR-C1 said 3; at 3 (and up to 3.3) GNSS drift at 8 m accuracy ends a
    /// standing hold in the CI standing scenario, so 3.5.
    pub hold_quiet_sigmas: f64,
    /// Stationary likelihood factor when steps are coming in.
    pub steps_moving_factor: f64,
    /// Walking likelihood factor when no steps came for a while (Walk/Run).
    pub steps_quiet_factor: f64,
    /// "Steps are coming in": at least `steps_moving_min` in this window, ms.
    pub steps_moving_window_ms: i64,
    /// Minimum steps in the moving window.
    pub steps_moving_min: i64,
    /// "No steps for a while" window, ms.
    pub steps_quiet_window_ms: i64,
    /// A host that sends step totals only with fixes (no step events) has a step counter while a total came within this, ms. A counter
    /// that sent a step event stays present (ruling FR-N1).
    pub steps_present_ms: i64,
    /// The step total sent with a fix is used only when no step event came for this long, ms (ruling FR-I3).
    pub steps_fallback_ms: i64,
    /// Course (from the moving models, ruling T8-R5) is reported when their probability is at least this ...
    pub course_min_moving_mu: f64,
    /// ... their speed at least this, m/s ...
    pub course_min_speed_mps: f64,
    /// ... when its sigma is below this, degrees.
    pub course_max_sigma_deg: f64,
    /// Maneuver (rulings T8-R8, R14): a line fit over the last 4 used fixes that disagrees with the filter's course by more than this,
    /// degrees (or moves while the filter has no course) ...
    pub maneuver_course_deg: f64,
    /// ... for this many fixes in a row is reported as the course, ...
    pub maneuver_fixes: u32,
    /// The line fit counts only when its speed is more than this many sigmas of the fitted slope (ruling T8-R17).
    pub maneuver_min_sigmas: f64,
    /// ... and the step is predicted with every model's `sigma_a` times this.
    pub maneuver_sigma_a_scale: f64,
    /// Accepted estimates are at most this uncertain, metres.
    pub max_uncertainty_m: f64,
    /// Simulated fixes become exact estimates of this uncertainty, metres.
    pub sim_uncertainty_m: f64,
    /// Whether mock-location fixes are used (debug bench only).
    pub allow_mock: bool,
    /// The map shows the estimate predicted along its course at most this far ahead while moving, ms.
    pub display_predict_max_ms: i64,
    /// A shown position counts as predicted (not GPS) this long after its fix, ms (more than the 5 s screen-off fix interval).
    pub predicted_after_ms: i64,
    /// ... and as stale (pin greyed, no course arrow) after this, ms.
    pub stale_after_ms: i64,
    /// A compass reading counts for the map arrow while it is at most this old, ms.
    pub compass_fresh_ms: i64,
    /// The compass is "steady" over this window, ms ...
    pub compass_window_ms: i64,
    /// ... when every reading in it is within this of their mean, degrees.
    pub compass_max_spread_deg: f64,
    /// The compass counts only with the phone at most this far from flat, degrees.
    pub compass_max_tilt_deg: f64,
    /// Matcher input: an estimate counts when it moved at least this far from the last input, metres ...
    pub match_min_move_m: f64,
    /// ... or came this long after it, ms.
    pub match_min_gap_ms: i64,
    /// Floor of the matcher's `sigma_z`, metres (Newson and Krumm's 4.07 m).
    pub match_sigma_floor_m: f64,
    /// Candidates lie within `min(this, 3 sigma_z + 10 m)`, metres ...
    pub match_max_radius_m: f64,
    /// ... at most this many, nearest first.
    pub match_max_candidates: usize,
    /// The off-network emission is the street emission at this distance, metres.
    pub match_off_road_m: f64,
    /// Transition `beta`: the scale of `|d_gc - d_route|`, metres.
    pub match_beta_m: f64,
    /// Reversing on the same segment multiplies the transition by this (unless slower than 0.5 m/s).
    pub match_uturn_factor: f64,
    /// Street to off-network, probability per second (times `dt`, capped at 0.5).
    pub match_to_off_per_s: f64,
    /// Off-network to street, probability per second (times `dt`, capped at 0.5).
    pub match_to_on_per_s: f64,
    /// The trace is decided this many inputs behind the newest.
    pub match_lag: usize,
    /// Street routes are searched this far from a node, metres (on top of the per-pair bound `2 d_gc + 50 m`).
    pub match_route_limit_m: f64,
    /// States within `clamp(1.5 sigma_z, 1.5 m, this)` of each other are one place for the confidence, metres (ruling T18-conf2: the
    /// cap keeps streets 10 m apart distinct however bad the GPS).
    pub match_place_max_m: f64,
    /// On a degraded graph the confidence is at most this.
    pub match_degraded_cap: f64,
    /// The matched point is shown (pin and trace) at this confidence or more ...
    pub match_show_confidence: f64,
    /// ... and, for the pin, within `max(this, 2 sigma)` of the estimate, metres.
    pub match_show_min_m: f64,
    /// A shown pin stays on the street while the confidence is above this on the same or a connected segment (rulings T19-hyst,
    /// hyst2: at 1 Hz a crossing is ambiguous for an input or two) ...
    pub match_stay_confidence: f64,
    /// ... and it is within this many times the showing distance of the estimate.
    pub match_stay_gate_factor: f64,
    /// Carry offset (Task 20): learn only from estimates at most this uncertain, metres ...
    pub carry_learn_max_unc_m: f64,
    /// ... at least this fast, m/s ...
    pub carry_learn_min_speed_mps: f64,
    /// ... with a course sigma at most this, degrees ...
    pub carry_learn_max_course_sigma_deg: f64,
    /// ... at most once per this long, ms (one-second courses are strongly correlated, review R2) ...
    pub carry_learn_min_gap_ms: i64,
    /// ... and not from a line-fit course while the compass turned more than this over the fit's span, degrees (ruling T20-corner2) ...
    pub carry_turn_skip_deg: f64,
    /// ... while the compass turns slower than this, degrees per second.
    pub carry_max_rate_deg_s: f64,
    /// A compass reading counts for the carry offset while it is at most this far in time from the fix, ms (the screen-off compass runs
    /// at 1 Hz and is batched up to 1 s).
    pub carry_compass_fresh_ms: i64,
    /// The offset's process noise, degrees^2 per second.
    pub carry_q_deg2_per_s: f64,
    /// The offset's variance after a reset (and its cap), degrees^2.
    pub carry_reset_var_deg2: f64,
    /// An innovation beyond this many sigmas ...
    pub carry_change_sigmas: f64,
    /// ... for this long means the carry changed, ms; so do at least three innovations of `carry_change_min_deg` or more, all on one
    /// side, spanning this long (ruling T20-change).
    pub carry_change_ms: i64,
    /// The smallest innovation that counts toward a one-sided carry-change run, degrees.
    pub carry_change_min_deg: f64,
    /// The steadiness is measured over this window of residuals, ms (it holds `carry_min_residuals` at 5 s screen-off fixes).
    pub carry_steady_window_ms: i64,
    /// The offset is confident only with a steadiness of at least this ...
    pub carry_min_steadiness: f64,
    /// ... over at least this many residuals.
    pub carry_min_residuals: usize,
    /// A gap is bridged with compass plus offset from this confidence.
    pub carry_min_confidence: f64,
    /// In a gap, the azimuth spreading over more than this within the compass window, cadence unchanged, means the phone moved,
    /// degrees.
    pub carry_jump_deg: f64,
    /// Without a confident offset the gap heading is the last course with this sigma, degrees ...
    pub carry_gap_sigma0_deg: f64,
    /// ... growing this much per second of gap ...
    pub carry_gap_sigma_per_s: f64,
    /// ... up to this, degrees.
    pub carry_gap_sigma_max_deg: f64,
    /// False trusts the raw compass for gap headings (the bench comparison, review M5/M6): the reading nearest the time within
    /// `carry_compass_fresh_ms`, with no offset and the compass sigma, whatever the confidence; else the last course as with it on.
    /// Learning still runs (`Locator::carry()` shows it), so the bench can compare both from one walk.
    pub carry_enabled: bool,
    /// Step calibration (Task 21): a window of good walking estimates closes once it spans at least this long, ms ...
    pub calib_window_ms: i64,
    /// ... and covers at least this far along the estimates' positions, metres.
    pub calib_min_dist_m: f64,
    /// Only accepted walking GPS estimates at most this uncertain build a window, metres.
    pub calib_max_unc_m: f64,
    /// A window breaks where consecutive estimates are more than this apart, ms (ruling T21-I1).
    pub calib_max_gap_ms: i64,
    /// The scale's process noise, per minute of windows.
    pub calib_q_per_min: f64,
    /// The scale is clamped to at least this ...
    pub calib_k_min: f64,
    /// ... and at most this.
    pub calib_k_max: f64,
    /// For the first this much walking of a session, ms ...
    pub calib_session_ms: i64,
    /// ... the scale's variance is at least this, so a stored value is re-checked every session.
    pub calib_session_var: f64,
    /// A carry change (two windows beyond `calib_adapt_sigmas`, or a cadence jump at the same speed) raises the variance to this.
    pub calib_adapt_var: f64,
    /// An observation this many sigmas from the scale is a miss.
    pub calib_adapt_sigmas: f64,
    /// A cadence change of more than this share ...
    pub calib_cadence_jump: f64,
    /// ... at a speed within this share of the last window's is a carry change.
    pub calib_same_speed: f64,
    /// An observation's sigma is this times the mean uncertainty over the window's step-predicted distance (spec gap 21, rulings E3,
    /// T21-R5).
    pub calib_obs_scale: f64,
    /// A window's distance is the path between its estimates at least this far apart, metres: summing one-second steps counts the
    /// filter's jitter as walking (+18 % on a synthetic walk, ruling T21-R1).
    pub calib_path_step_m: f64,
    /// The two-sided CUSUM on normalized window residuals (ruling T21-R2) subtracts this drift per window ...
    pub calib_cusum_drift: f64,
    /// ... and a sum above this means the carry changed.
    pub calib_cusum_h: f64,
    /// Gap bridging (Task 22): a Walk/Run gap is bridged once the last accepted fix is this old and steps come in, ms ... (Bike and
    /// Drive never bridge: their pin coasts on the filter's prediction, `predicted_after_ms` then `stale_after_ms`; the brief's
    /// `coast_max_ms` is not added, ruling T22-R2.)
    pub bridge_after_ms: i64,
    /// ... for at most this long (then the next fix starts the filter afresh), ms ...
    pub bridge_max_ms: i64,
    /// ... and ends after this long without new steps (the pin then ages into "predicted" and "stale"), ms.
    pub bridge_no_steps_ms: i64,
    /// Particles in the cloud.
    pub bridge_particles: usize,
    /// Share of the particles started off the street network.
    pub bridge_off_share: f64,
    /// Sigma of each particle's own heading bias, degrees.
    pub bridge_bias_sigma_deg: f64,
    /// Each particle's step scale is `N(1, max(sigma_k, this))`.
    pub bridge_sigma_k_min: f64,
    /// Resampled street particles are moved along their street by this sigma, metres.
    pub bridge_roughen_m: f64,
    /// An off-network particle's weight is multiplied by this per step batch.
    pub bridge_off_weight: f64,
    /// Seed of the cloud's random numbers (mixed with the gap's start time), so a replay is repeatable.
    pub bridge_seed: u64,
    /// In a gap, a compass jump is a turn, not the phone moving, while at least `bridge_turn_share` of the cloud is within this of a
    /// crossing, metres.
    pub bridge_turn_m: f64,
    /// See `bridge_turn_m`.
    pub bridge_turn_share: f64,
    /// A jump taken for a turn is checked this far past the crossing: a compass still rotated on an unturned street is the phone
    /// moving after all, metres (review M8).
    pub bridge_turn_check_m: f64,
    /// When that check finds the phone moved, off-network particles within this of a street go back onto it, metres (ruling T22-R4).
    pub bridge_snap_m: f64,
    /// No steps for this long in a gap is standing: a compass turn is the walker turning round, not the phone moving, ms (review
    /// round 3).
    pub bridge_still_ms: i64,
    /// The bridged position is the centre of the cloud's heaviest cluster, found with this radius (or the cloud's sigma when wider),
    /// metres (review I3).
    pub bridge_cluster_m: f64,
    /// A street within this of perpendicular to the course at the start of a gap is walked either way, degrees (review I4).
    pub bridge_split_deg: f64,
    /// After the phone moved in a gap, the compass is re-referenced to the cloud's street course with this sigma, degrees (gap only:
    /// the carry offset itself learns from GPS alone).
    pub bridge_reref_sigma_deg: f64,
}

impl Default for LocParams {
    #[allow(clippy::too_many_lines)] // one line per tunable; splitting would only scatter the defaults
    fn default() -> Self {
        Self {
            sigma_floor_m: 2.0,
            unusable_acc_m: 100.0,
            network_r_factor: 4.0,
            min_speed_for_velocity_mps: 0.5,
            q_stationary_m2_per_s: 0.05 * 0.05,
            stationary_vel_sigma_mps: 0.1,
            sigma_a_walk: 0.3,
            sigma_a_run: 1.0,
            sigma_a_bike: 1.5,
            sigma_a_drive: 3.0,
            v0_slow_mps: 2.0,
            v0_fast_mps: 10.0,
            mu0_slow: [0.6, 0.35, 0.05],
            mu0_fast: [0.4, 0.1, 0.5],
            pi_slow: [[0.95, 0.048, 0.002], [0.05, 0.94, 0.01], [0.02, 0.08, 0.90]],
            pi_fast: [[0.93, 0.02, 0.05], [0.05, 0.80, 0.15], [0.04, 0.01, 0.95]],
            max_predict_s: 10.0,
            max_offdiag_share: 0.9,
            mu_floor: 1e-4,
            gate_soft_2: 9.21,
            gate_hard_2: 13.8,
            gate_soft_4: 13.28,
            gate_hard_4: 18.5,
            reloc_count: 3,
            reloc_span_ms: 2_000,
            reloc_quick_after_ms: 20_000,
            reloc_quick_acc_m: 20.0,
            reloc_vmax_mps: 1.5 * 150.0 / 3.6,
            reloc_fast_mps: 12.0 / 3.6,
            mu_reloc_walk: [0.05, 0.9, 0.05],
            mu_reloc_fast: [0.05, 0.05, 0.9],
            reset_gap_ms: 5 * 60_000,
            min_fix_t_ms: if cfg!(test) { i64::MIN } else { MIN_FIX_T_MS },
            max_fix_t_ms: MAX_FIX_T_MS,
            reset_trace_m2: 200.0 * 200.0,
            hold_mu_s: 0.8,
            hold_speed_mps: 0.3,
            hold_quiet_steps_ms: 10_000,
            hold_exit_fixes: 2,
            hold_exit_min_m: 3.0,
            hold_quiet_max_m: 15.0,
            hold_quiet_sigmas: 3.5,
            steps_moving_factor: 0.1,
            steps_quiet_factor: 0.3,
            steps_moving_window_ms: 5_000,
            steps_moving_min: 2,
            steps_quiet_window_ms: 10_000,
            steps_present_ms: 600_000,
            steps_fallback_ms: 10_000,
            course_min_moving_mu: 0.5,
            course_min_speed_mps: 0.5,
            course_max_sigma_deg: 35.0,
            maneuver_course_deg: 45.0,
            maneuver_fixes: 1,
            maneuver_min_sigmas: 2.0,
            maneuver_sigma_a_scale: 3.0,
            max_uncertainty_m: crate::loc::MAX_UNCERTAINTY_M,
            sim_uncertainty_m: 3.0,
            allow_mock: false,
            display_predict_max_ms: 3000,
            predicted_after_ms: 6000,
            stale_after_ms: 30_000,
            compass_fresh_ms: 1000,
            compass_window_ms: 2000,
            compass_max_spread_deg: 10.0,
            compass_max_tilt_deg: 60.0,
            match_min_move_m: 2.0,
            match_min_gap_ms: 1000,
            match_sigma_floor_m: 4.07,
            match_max_radius_m: 50.0,
            match_max_candidates: 8,
            match_off_road_m: 20.0,
            match_beta_m: 5.0,
            match_uturn_factor: 0.2,
            match_to_off_per_s: 0.02,
            match_to_on_per_s: 0.05,
            match_lag: 3,
            match_route_limit_m: 300.0,
            match_place_max_m: 6.5,
            match_degraded_cap: 0.5,
            match_show_confidence: 0.7,
            match_show_min_m: 10.0,
            match_stay_confidence: 0.4,
            match_stay_gate_factor: 1.5,
            carry_learn_max_unc_m: 10.0,
            carry_learn_min_speed_mps: 0.8,
            carry_learn_max_course_sigma_deg: 35.0,
            carry_learn_min_gap_ms: 3000,
            carry_turn_skip_deg: 30.0,
            carry_max_rate_deg_s: 30.0,
            carry_compass_fresh_ms: 2500,
            carry_q_deg2_per_s: 4.0,
            carry_reset_var_deg2: 8100.0,
            carry_change_sigmas: 3.0,
            carry_change_ms: 3000,
            carry_change_min_deg: 30.0,
            carry_steady_window_ms: 30_000,
            carry_min_steadiness: 0.6,
            carry_min_residuals: 5,
            carry_min_confidence: 0.5,
            carry_jump_deg: 45.0,
            carry_gap_sigma0_deg: 10.0,
            carry_gap_sigma_per_s: 2.0,
            carry_gap_sigma_max_deg: 90.0,
            carry_enabled: true,
            calib_window_ms: 60_000,
            calib_min_dist_m: 80.0,
            calib_max_unc_m: 10.0,
            calib_max_gap_ms: 10_000,
            calib_q_per_min: 0.0001,
            calib_k_min: 0.6,
            calib_k_max: 1.4,
            calib_session_ms: 180_000,
            calib_session_var: 0.01,
            calib_adapt_var: 0.04,
            calib_adapt_sigmas: 3.0,
            calib_cadence_jump: 0.2,
            calib_same_speed: 0.1,
            calib_obs_scale: 1.5,
            calib_path_step_m: 15.0,
            calib_cusum_drift: 0.25,
            calib_cusum_h: 3.0,
            bridge_after_ms: 10_000,
            bridge_max_ms: 300_000,
            bridge_no_steps_ms: 30_000,
            bridge_particles: 300,
            bridge_off_share: 0.1,
            bridge_bias_sigma_deg: 10.0,
            bridge_sigma_k_min: 0.05,
            bridge_roughen_m: 1.0,
            bridge_off_weight: 0.5,
            bridge_seed: 0x5eed,
            bridge_turn_m: 10.0,
            bridge_turn_share: 0.3,
            bridge_turn_check_m: 20.0,
            bridge_snap_m: 20.0,
            bridge_still_ms: 4_000,
            bridge_reref_sigma_deg: 15.0,
            bridge_cluster_m: 15.0,
            bridge_split_deg: 30.0,
        }
    }
}
