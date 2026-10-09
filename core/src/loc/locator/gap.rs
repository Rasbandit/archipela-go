//! GPS gaps: bridging from steps and heading with the particle cloud, compass jumps in the gap, re-anchoring.

use crate::catalog::Mode;
use crate::geo::Point;
use crate::loc::bridge::{Bridge, Steer};
use crate::loc::calib::step_length_m;
use crate::loc::graph::mode_mask;
use crate::loc::heading::gap_course_sigma_deg;
use crate::loc::imm::{self, Imm, W};
use crate::loc::{Estimate, HeadingIn, Motion, RawFix, Source, Verdict};
use crate::num::i64_to_f64;

use super::Locator;

/// Model probabilities when a walked gap re-anchors the filter: walking is by far the likeliest.
pub(super) const REANCHOR_MU_WALKING: [f64; 3] = [0.1, 0.85, 0.05];

/// Fixes taken after a bridge re-anchor whose estimates do not count yet (adversarial review C1): at 1 Hz about 3 s of quest delay.
pub(super) const REANCHOR_WARMUP_FIXES: u32 = 3;

/// A compass jump at a crossing, taken for a turn: if `bridge_turn_m + bridge_turn_check_m` later the compass is still rotated from
/// `az_deg` but the cloud's streets have not turned from `course_deg`, and no street near `at` leads the new way, the phone moved after
/// all (review M8). Known limit (review M9): a turn into a path the graph does not have, on a long street, reads the same way, as the
/// phone moving.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TurnCheck {
    /// The compass before the jump, degrees.
    az_deg: f64,
    /// The cloud's street course at the jump, degrees.
    course_deg: f64,
    /// The bridged position at the jump.
    at: Point,
    /// Bridged since the jump, metres.
    walked_m: f64,
}

/// The compass re-reference of a gap after the phone moved in it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) enum Reref {
    /// None needed.
    #[default]
    None,
    /// The carry offset reset in the gap: take one once the compass settles.
    Pending,
    /// `course - azimuth`, degrees.
    Set(f64),
}

impl Locator {
    /// A step counter reading. Steps coming in end a stationary hold at once. In a Walk or Run zone, once the last accepted fix is
    /// `bridge_after_ms` old and steps keep coming, the gap is bridged (Task 22): each step batch moves the particle cloud by `steps x
    /// L(cadence) x k` and returns a bridged estimate (display, fog and Cartographer squares only, `accepted = false`). The bridge ends at
    /// the next accepted fix, or `bridge_max_ms` after the last one (then the filter starts afresh and the last bridged pin stays, aging
    /// by the display rules). After `bridge_no_steps_ms` without steps it pauses, and the next steps carry the same cloud on.
    pub fn on_steps(&mut self, total: i64, t_ms: i64, cadence: Option<f64>) -> Option<Estimate> {
        let before = self.steps.latest();
        self.steps.push(total, t_ms);
        if self.moving_steps(t_ms) {
            self.hold = None;
        }
        // Only a reading the history took counts (not an old one, nor a counter that restarted).
        let new = match before {
            Some((_, b)) if self.steps.latest() == Some((t_ms, total)) => (total - b).max(0),
            _ => 0,
        };
        if self.bridge.is_none() {
            if new > 0 && self.may_bridge(t_ms) {
                self.start_bridge(t_ms);
                let speed = self.bridged_speed(t_ms);
                return self.emit_bridged(t_ms, speed);
            }
            return None;
        }
        let p = self.params.clone();
        let last_step = self.bridge.as_ref()?.last_step_ms;
        if self.last_accepted_ms.is_none_or(|a| t_ms.saturating_sub(a) > p.bridge_max_ms) {
            // As long without a fix as a reset gap (review M12): the next fix starts fresh; the last bridged pin stays and ages (I5).
            let pin = self.bridged;
            self.reset();
            self.bridged = pin;
            return None;
        }
        if new == 0 {
            if t_ms.saturating_sub(last_step) > p.bridge_no_steps_ms {
                self.bridge_paused = true; // the pin ages into "predicted" and "stale" until steps come again
            }
            return None;
        }
        self.bridge_paused = false;
        let resumed = t_ms.saturating_sub(last_step) > p.bridge_still_ms;
        // The batch's own time and cadence (review round 3, minor 3): after a stop, the 10 s cadence and the time since the last batch
        // count the standing too, so take the time since the reading before (or, with none in the stop, the steps at a walking cadence).
        let prev = before.map_or(last_step, |(t, _)| t);
        let batch_s = if resumed && prev <= last_step { None } else { Some(i64_to_f64(t_ms.saturating_sub(prev)) / 1000.0).filter(|s| *s > 0.0) };
        let f = match (cadence, resumed) {
            (Some(f), _) => f,
            (None, true) => batch_s.map_or(1.8, |s| i64_to_f64(new) / s),
            (None, false) => self.steps.cadence(t_ms).unwrap_or(1.8),
        };
        self.settle_reref(t_ms);
        self.check_turn(t_ms);
        let steer = self.gap_steer(t_ms)?;
        let az = self.compass.mean_deg(t_ms, p.compass_window_ms);
        // Walking on facing another way after standing: the walker turned round (review round 3), so the streets are walked anew.
        if let (true, Some(now), Some(then)) = (resumed, az, self.bridge_step_az) {
            if crate::loc::heading::wrap_deg(now - then).abs() > p.bridge_split_deg {
                self.bridge.as_mut()?.redirect(Some(steer.theta_deg), &p);
            }
        }
        self.bridge_step_az = az.or(self.bridge_step_az);
        let dist = i64_to_f64(new) * step_length_m(f) * self.calib.k();
        if let Some(c) = &mut self.turn_check {
            c.walked_m += dist;
        }
        let b = self.bridge.as_mut()?;
        b.step(dist, steer, &p);
        b.last_step_ms = t_ms;
        let dt_s = batch_s.unwrap_or(i64_to_f64(new) / f.max(0.5));
        self.emit_bridged(t_ms, dist / dt_s)
    }

    /// Standing in a gap: the bridge is paused, or no step came for `bridge_still_ms`.
    pub(super) fn standing_in_gap(&self, t_ms: i64) -> bool {
        self.bridge_paused || self.steps.quiet(t_ms, self.params.bridge_still_ms)
    }

    /// Whether a GPS gap is being bridged (and the walker is not stopped in it).
    #[must_use]
    pub fn bridging(&self) -> bool {
        self.bridge.is_some() && !self.bridge_paused
    }

    /// Walk or Run zone, a filter running, a step counter, and no accepted fix for `bridge_after_ms` (but not yet `bridge_max_ms`), nor
    /// any fix the filter took (a gated one too, review I2) for `bridge_after_ms`.
    pub(super) fn may_bridge(&self, t_ms: i64) -> bool {
        let p = &self.params;
        matches!(self.mode, Mode::Walk | Mode::Run)
            && self.imm.as_ref().is_some_and(|imm| t_ms.saturating_sub(imm.t_ms) > p.bridge_after_ms)
            && self.steps.present(t_ms, self.params.steps_present_ms)
            && self.last_accepted_ms.is_some_and(|a| t_ms.saturating_sub(a) > p.bridge_after_ms && t_ms.saturating_sub(a) <= p.bridge_max_ms)
    }

    /// Start the cloud from the filter's walking model at its last fix (on the matcher's segment when it is confident), then walk it by
    /// the steps since: the gap is noticed `bridge_after_ms` late, and the steps say how far the walker went better than the filter's
    /// velocity does.
    pub(super) fn start_bridge(&mut self, t_ms: i64) {
        let (Some(imm), Some(frame)) = (&self.imm, self.frame) else { return };
        let p = &self.params;
        let walking = imm.models[W]; // a Walk or Run zone with steps coming in: neither standing nor fast
        let mean = frame.to_geo([walking.x[0], walking.x[1]]);
        let prefer = self.matcher.best().filter(|m| m.confidence >= p.match_show_confidence).and_then(|m| m.seg).map(|s| (s, 1.0));
        self.reref = Reref::None;
        self.turn_check = None;
        let steer = self.gap_steer(t_ms);
        // The way to walk the streets (review I4): the steer's, unless the course is unknown since the last hold; then both ways.
        let course = steer.filter(|_| self.last_course.is_some()).map(|s| s.theta_deg);
        let mask = mode_mask(self.mode);
        let mut b = Bridge::start(self.graph.clone(), frame, mask, mean, imm::pos_block(&walking.p), self.calib.sigma_k(), prefer, course, t_ms, p);
        let since = self.steps.gained(imm.t_ms, t_ms);
        let f = self.steps.cadence(t_ms).unwrap_or(1.8);
        let dist = i64_to_f64(since) * step_length_m(f) * self.calib.k();
        if let (Some(steer), true) = (steer, dist > 0.0) {
            b.step(dist, steer, &self.params);
        }
        self.bridge = Some(b);
    }

    /// The walking speed from the steps of the last 10 s, m/s.
    pub(super) fn bridged_speed(&self, t_ms: i64) -> f64 {
        self.steps.cadence(t_ms).map_or(0.0, |f| f * step_length_m(f) * self.calib.k())
    }

    /// The cloud's estimate as a bridged [`Estimate`] at `t_ms`, moving at `speed_mps`; the bearing of its street becomes the last
    /// course.
    pub(super) fn emit_bridged(&mut self, t_ms: i64, speed_mps: f64) -> Option<Estimate> {
        let (at, uncertainty_m, course_deg) = self.bridge.as_ref()?.estimate();
        let est = Estimate {
            t_ms,
            lat: at.lat,
            lon: at.lon,
            uncertainty_m,
            speed_mps,
            course_deg,
            motion: Motion::Walking,
            mode_probs: [0.0, 1.0, 0.0],
            source: Source::Bridged,
            verdict: Verdict::Used,
            accepted: false,
            ..Estimate::default()
        };
        self.bridged = Some(est);
        // A street's bearing is news; an off-network cloud's course is only the heading it was steered by (feeding that back would
        // let the bias of the cloud turn it in circles).
        if let Some(c) = self.bridge.as_ref().and_then(Bridge::street_course) {
            self.last_course = Some(c);
        }
        Some(est)
    }

    /// The bearing to bridge with at `t_ms` (Task 22 notes 5, 6): the last course while the compass swings; else compass plus the
    /// carry offset when it is confident (or the raw compass with `carry_enabled` off); else, after the phone moved in this gap, the
    /// compass against its re-reference to the street course; else the last course, its sigma growing with the gap.
    pub(super) fn gap_steer(&self, t_ms: i64) -> Option<Steer> {
        let p = &self.params;
        let gap_s = (i64_to_f64(t_ms.saturating_sub(self.last_accepted_ms.unwrap_or(t_ms))) / 1000.0).max(0.0);
        let course = || self.last_course.map(|c| (c, gap_course_sigma_deg(gap_s, p)));
        let (theta_deg, sigma_deg) = if self.compass.swinging(t_ms, p) {
            course()?
        } else {
            let reref = || match self.reref {
                Reref::Set(d) => {
                    let h = self.compass.nearest(t_ms, p.carry_compass_fresh_ms)?;
                    let cs = h.sigma_deg()?;
                    Some(((h.azimuth_deg + d).rem_euclid(360.0), p.bridge_reref_sigma_deg.hypot(cs)))
                }
                _ => None,
            };
            self.carry.heading_for_gap(&self.compass, t_ms, None, gap_s, p).or_else(reref).or_else(course)?
        };
        Some(Steer { theta_deg, sigma_deg })
    }

    /// A pending re-reference is taken once the compass has settled: the cloud's course minus the mean azimuth of the window.
    pub(super) fn settle_reref(&mut self, t_ms: i64) {
        if self.reref != Reref::Pending || self.compass.swinging(t_ms, &self.params) {
            return;
        }
        let course = self.bridge.as_ref().and_then(|b| b.estimate().2);
        if let (Some(c), Some(az)) = (course, self.compass.mean_deg(t_ms, self.params.compass_window_ms)) {
            self.reref = Reref::Set(crate::loc::heading::wrap_deg(c - az));
        }
    }

    /// The cadence 2 s ago and now differ by more than `calib_cadence_jump`.
    pub(super) fn cadence_changed(&self, t_ms: i64) -> bool {
        match (self.steps.cadence(t_ms), self.steps.cadence(t_ms.saturating_sub(2_000))) {
            (Some(f), Some(f0)) => (f - f0).abs() > self.params.calib_cadence_jump * f0,
            _ => false,
        }
    }

    /// A compass reading (true north), on the same clock as the fixes. While bridging every reading is watched for the phone moving
    /// (Task 22 note 5): a jump the cadence or a crossing near the cloud does not explain resets the carry offset, and the gap is then
    /// steered by the compass re-referenced to the street course.
    pub fn on_heading(&mut self, h: &HeadingIn) {
        self.compass.push(*h);
        let Some(b) = &self.bridge else { return };
        let p = &self.params;
        let cadence = self.cadence_changed(h.t_ms);
        // Standing, a turn is the walker turning round (review round 3): no carry reset, no turn check.
        let standing = self.standing_in_gap(h.t_ms);
        let at_turn = b.near_turn_share(p.bridge_turn_m, p.carry_jump_deg) >= p.bridge_turn_share;
        let jumped = self.compass.range_deg(h.t_ms, p.compass_window_ms).is_some_and(|d| d > p.carry_jump_deg);
        if jumped && at_turn && !cadence && !standing && self.turn_check.is_none() {
            let before = self.compass.mean_deg(h.t_ms.saturating_sub(p.compass_window_ms), p.compass_window_ms);
            if let (Some(az_deg), Some(course_deg)) = (before, b.street_course()) {
                self.turn_check = Some(TurnCheck { az_deg, course_deg, at: b.estimate().0, walked_m: 0.0 });
            }
        }
        if self.carry.watch_gap(&self.compass, h.t_ms, cadence || at_turn || standing, p) {
            self.reref = Reref::Pending;
        }
    }

    /// Once the cloud is `bridge_turn_check_m` past a jump read as a turn ([`TurnCheck`]): a compass still rotated by more than
    /// `carry_jump_deg` while the cloud's streets did not turn, and no street at the jump leads the new way, is the phone moving. The carry resets and the
    /// compass is re-referenced to the street course of the jump; off-network particles within `bridge_snap_m` of a street go back onto it.
    pub(super) fn check_turn(&mut self, t_ms: i64) {
        let p = &self.params;
        let Some(c) = self.turn_check.filter(|c| c.walked_m >= p.bridge_turn_m + p.bridge_turn_check_m) else { return };
        let Some(az) = self.compass.mean_deg(t_ms, p.compass_window_ms).filter(|_| !self.compass.swinging(t_ms, p)) else { return };
        self.turn_check = None;
        let rotated = crate::loc::heading::wrap_deg(az - c.az_deg).abs() > p.carry_jump_deg;
        // The streets the cloud's street particles walk, wherever its weight went. A cloud with none left was steered off every street
        // there was: no turn the map knows (review M9).
        let turned =
            self.bridge.as_ref().and_then(Bridge::network_course).is_some_and(|sc| crate::loc::heading::wrap_deg(sc - c.course_deg).abs() > p.carry_jump_deg);
        // A street the walker could have taken the new way: the cloud may only have lost the turn.
        let heading = az + if p.carry_enabled { self.carry.delta_deg() } else { 0.0 };
        let mask = mode_mask(self.mode);
        let walkable = self.graph.as_ref().is_some_and(|g| {
            let at = g.frame().to_enu(c.at);
            let away = |node: usize| (g.node_en(node)[0] - at[0]).hypot(g.node_en(node)[1] - at[1]);
            g.candidates(c.at, 2.0 * p.bridge_turn_m, 16, mask).iter().any(|k| {
                let sg = g.seg(k.seg);
                // Each way along the segment that leads away from the jump, and its bearing.
                [(1.0, sg.a, sg.b), (-1.0, sg.b, sg.a)].into_iter().any(|(dir, from, to)| {
                    away(to) > away(from) && crate::loc::heading::wrap_deg(g.bearing_deg(k.seg, dir) - heading).abs() <= p.carry_jump_deg
                })
            })
        });
        if rotated && !turned && !walkable {
            self.carry.reset(p);
            self.reref = Reref::Set(crate::loc::heading::wrap_deg(c.course_deg - az));
            let (within, course) = (p.bridge_snap_m, c.course_deg);
            let p = self.params.clone();
            if let Some(b) = self.bridge.as_mut() {
                b.snap_to_streets(within, course, &p); // ruling T22-R4: the cloud comes back to the street
            }
        }
    }

    /// A fix in a bridged gap: the cloud, moment-matched, becomes the filter's prior (moving along the last course at the bridged
    /// speed), and the fixes before the gap leave the line fit (review M14); the fix is then gated against it as usual, and a gated one
    /// goes to the relocation rule. The cloud runs on until a fix is taken ([`Self::end_bridge`]). The prior keeps the pin continuous but
    /// is no evidence (adversarial review C1): the fix is gated against the cloud itself (re-review N2), a taken one updates a prior
    /// whose position variance is at least the fix's accuracy squared per axis, and the next [`REANCHOR_WARMUP_FIXES`] taken fixes do
    /// not count.
    pub(super) fn reanchor_bridge(&mut self, f: &RawFix) {
        let t_ms = f.t_ms;
        let (Some(b), Some(frame)) = (self.bridge.as_ref(), self.frame) else { return };
        if self.reanchored_step_ms == Some(b.last_step_ms) {
            return; // no step since: the filter goes on from the fixes it took (review round 3, minor 4)
        }
        self.reanchored_step_ms = Some(b.last_step_ms);
        self.used.clear();
        let (mean, cov) = b.moments();
        self.reanchor_floor_m2 = Some(if f.accuracy_m.is_finite() { f.accuracy_m * f.accuracy_m } else { 0.0 });
        self.reanchor_warmup = REANCHOR_WARMUP_FIXES;
        // Standing in the gap (review round 3, minor 2): no velocity, and standing the likeliest model; else walking on.
        let (vel, mu) = if self.standing_in_gap(t_ms) {
            ([0.0, 0.0], self.params.mu0_slow)
        } else {
            let speed = self.bridged.map_or(0.0, |e| e.speed_mps);
            (self.last_course.map_or([0.0, 0.0], |c| [speed * c.to_radians().sin(), speed * c.to_radians().cos()]), REANCHOR_MU_WALKING)
        };
        self.imm = Some(Imm::with_prior(frame.to_enu(mean), cov, vel, mu, b.last_step_ms, self.mode, &self.params));
    }

    /// An accepted fix ends the bridge; the bridged pin is kept but [`Self::last`] is newer.
    pub(super) fn end_bridge(&mut self) {
        self.bridge = None;
        self.bridge_paused = false;
        self.reref = Reref::None;
        self.turn_check = None;
        self.bridge_step_az = None;
        self.reanchored_step_ms = None;
    }
}

#[cfg(test)]
mod tests {

    use std::sync::Arc;

    use crate::catalog::Mode;
    use crate::geo::{destination, distance_m, Point};
    use crate::loc::bridge::{Bridge, Steer};

    use crate::loc::frame::Frame;
    use crate::loc::graph::{mode_mask, StreetGraph};

    use crate::loc::imm::{S, W};

    use crate::loc::locator::maneuver::wrap_deg;
    use crate::loc::locator::test_util::*;
    use crate::loc::locator::Locator;
    use crate::loc::params::LocParams;
    use crate::loc::{DisplaySource, Estimate, HeadingIn, RawFix, Source, Verdict};
    use crate::num::i64_to_f64;

    #[test]
    fn a_coarse_start_is_no_accepted_fix_to_bridge_from() {
        // Task 7 minor (same root as FR-I1): a restart that is not accepted does not set the time of the last accepted fix, so steps
        // after a coarse start bridge nothing.
        let mut l = Locator::default();
        l.on_steps(10_000, 0, None);
        l.on_fix(&fix(o(), 1, 50.0));
        let bridged: Vec<Estimate> = (1..20).filter_map(|k| l.on_steps(10_000 + 4 * k, (1 + 2 * k) * 1000, None)).collect();
        assert!(bridged.is_empty() && !l.bridging(), "{bridged:?}");
    }

    #[test]
    fn on_a_bendy_street_the_bridged_pin_moves_smoothly() {
        // Review I3: a cluster by segment id hops between 3 to 5 m segments; a spatial cluster moves with the walker.
        let street = bendy_street();
        let way = crate::scan::WayGeom { id: 1, class: crate::scan::way_class::FOOT, pts: street.clone() };
        let g = Arc::new(StreetGraph::from_ways(&[way]).unwrap());
        let p = LocParams::default();
        let mut b =
            Bridge::start(Some(g), Frame::new(o()), mode_mask(Mode::Walk), at_along(&street, 20.0), [[9.0, 0.0], [0.0, 9.0]], 0.05, None, Some(110.0), 0, &p);
        let mut last = b.estimate().0;
        for i in 0..200 {
            b.step(1.4, Steer { theta_deg: 110.0, sigma_deg: 20.0 }, &p); // the compass sees the street's mean bearing, not each bend
            let at = b.estimate().0;
            let step = distance_m(last, at);
            assert!(step <= 3.0, "batch {i}: a {step:.1} m step");
            last = at;
        }
    }

    /// A walk east along the y = 100 m street from x = 0 at 1.4 m/s, with steps every 2 s and the compass in hand (2 Hz) reading
    /// `az(t_s)`; fixes until `gap_from_s`, then only steps and compass until `until_s`. Returns the estimates the steps produced.
    fn walk_into_a_gap(l: &mut Locator, gap_from_s: i64, until_s: i64, az: impl Fn(i64) -> f64) -> Vec<Estimate> {
        let street = destination(o(), 0.0, 100.0);
        let mut out = Vec::new();
        for t in 0..until_s {
            if t % 2 == 0 {
                out.extend(l.on_steps(10_000 + t * 193 / 100, t * 1000, None));
            }
            for half in 0..2 {
                l.on_heading(&HeadingIn {
                    t_ms: t * 1000 + half * 500,
                    azimuth_deg: az(t),
                    accuracy: crate::loc::CompassAccuracy::High,
                    pitch_deg: 10.0,
                    roll_deg: 0.0,
                    error_deg: None,
                });
            }
            if t < gap_from_s {
                l.on_fix(&fix(destination(street, 90.0, 1.4 * i64_to_f64(t)), t, 4.0));
            }
        }
        out
    }

    fn on_street(x_m: f64) -> Point {
        destination(destination(o(), 0.0, 100.0), 90.0, x_m)
    }

    #[test]
    fn a_gps_gap_while_walking_is_bridged_from_steps_and_a_fix_ends_it() {
        let mut l = Locator::default();
        l.set_graph(grid());
        let bridged = walk_into_a_gap(&mut l, 40, 70, |_| 90.0);
        assert!(l.bridging());
        assert!(bridged.iter().all(|e| e.t_ms > 49_000), "bridging starts 10 s after the last fix: {:?}", bridged.first().map(|e| e.t_ms));
        assert!(bridged.iter().all(|e| e.source == Source::Bridged && !e.accepted && e.uncertainty_m.is_finite()));
        let last = bridged.last().unwrap();
        assert!(distance_m(last.point(), on_street(1.4 * 68.0)) < 8.0, "{} m off", distance_m(last.point(), on_street(1.4 * 68.0)));
        let d = l.display(last.t_ms).unwrap();
        assert_eq!(d.source, DisplaySource::Bridged);
        let e = l.on_fix(&fix(on_street(1.4 * 70.0), 70, 4.0));
        // Adversarial review C1: the re-anchored fix ends the bridge but counts only after three taken fixes.
        assert!(!l.bridging() && e.source == Source::Gps && e.verdict == Verdict::Used && !e.accepted, "{e:?}");
        assert!(distance_m(e.point(), on_street(1.4 * 70.0)) < 6.0, "re-anchored from the cloud: {} m", distance_m(e.point(), on_street(98.0)));
        assert_eq!(l.display(70_000).unwrap().source, DisplaySource::Gps);
        assert_eq!(l.last().map(|x| x.source), Some(Source::Gps), "`last` is the filter's");
        let accepted: Vec<bool> = (71..74).map(|t| l.on_fix(&fix(on_street(1.4 * i64_to_f64(t)), t, 4.0)).accepted).collect();
        assert_eq!(accepted, [false, false, true], "the fourth fix after the re-anchor counts");
    }

    #[test]
    fn without_a_street_graph_a_gap_is_bridged_along_the_heading() {
        let mut l = Locator::default();
        let bridged = walk_into_a_gap(&mut l, 40, 70, |_| 90.0);
        let last = bridged.last().unwrap();
        assert!(distance_m(last.point(), on_street(1.4 * 68.0)) < 10.0, "{} m off", distance_m(last.point(), on_street(1.4 * 68.0)));
    }

    #[test]
    fn bike_zones_and_phones_without_steps_never_bridge() {
        let mut bike = Locator::default();
        bike.set_mode(Mode::Bike);
        assert!(walk_into_a_gap(&mut bike, 40, 70, |_| 90.0).is_empty() && !bike.bridging());
        let mut fresh = Locator::default();
        assert!(fresh.on_steps(100, 0, None).is_none() && fresh.on_steps(200, 20_000, None).is_none(), "no filter, no bridge");
        let mut stepless = Locator::default();
        for t in 0..40 {
            stepless.on_fix(&fix(on_street(1.4 * i64_to_f64(t)), t, 4.0));
        }
        assert!(stepless.display(70_000).is_some_and(|d| d.source == DisplaySource::Stale) && !stepless.bridging());
    }

    #[test]
    fn a_bridged_pin_ages_once_steps_stop_and_the_bridge_ends() {
        // Task 22 note 2: no forever-confident hollow pin.
        let mut l = Locator::default();
        l.set_graph(grid());
        let last = *walk_into_a_gap(&mut l, 40, 60, |_| 90.0).last().unwrap();
        let at = |dt_ms| l.display(last.t_ms + dt_ms).unwrap();
        assert_eq!((at(2_000).source, at(2_000).uncertainty_m), (DisplaySource::Bridged, last.uncertainty_m), "fresh: its own uncertainty");
        assert_eq!(at(10_000).source, DisplaySource::Predicted, "no steps for a while: predicted");
        assert!(at(10_000).uncertainty_m > last.uncertainty_m + 1.0, "and growing: {}", at(10_000).uncertainty_m);
        assert_eq!(at(31_000).source, DisplaySource::Stale);
        let total = 10_000 + 58 * 193 / 100;
        assert!(l.on_steps(total, last.t_ms + 20_000, None).is_none() && l.bridging(), "no new steps: nothing moves");
        assert!(l.on_steps(total, last.t_ms + 31_000, None).is_none() && !l.bridging(), "30 s without steps ends the bridge");
        assert_eq!(l.display(last.t_ms + 31_000).unwrap().source, DisplaySource::Stale, "the last bridged pin stays, stale");
    }

    #[test]
    fn five_minutes_of_bridging_start_the_filter_afresh() {
        let mut l = Locator::default();
        let bridged = walk_into_a_gap(&mut l, 40, 360, |_| 90.0);
        assert!(bridged.iter().all(|e| e.t_ms - 50_000 <= 300_000) && !l.bridging(), "{:?}", bridged.last().map(|e| e.t_ms));
        assert_eq!(l.on_fix(&fix(on_street(1.4 * 360.0), 360, 4.0)).verdict, Verdict::Reset);
    }

    /// Steps every 2 s (`total(t_s)`) and the compass in hand (2 Hz, `az(t_s)`) from `from_s` to `until_s`, no fixes; the estimates.
    fn gap_steps(l: &mut Locator, from_s: i64, until_s: i64, total: impl Fn(i64) -> i64, az: impl Fn(i64) -> f64) -> Vec<Estimate> {
        let mut out = Vec::new();
        for t in from_s..until_s {
            if t % 2 == 0 {
                out.extend(l.on_steps(total(t), t * 1000, None));
            }
            for half in 0..2 {
                l.on_heading(&HeadingIn {
                    t_ms: t * 1000 + half * 500,
                    azimuth_deg: az(t),
                    accuracy: crate::loc::CompassAccuracy::High,
                    pitch_deg: 10.0,
                    ..HeadingIn::default()
                });
            }
        }
        out
    }

    #[test]
    fn a_stop_mid_gap_pauses_the_cloud_and_walking_on_resumes_it() {
        // Review I1: 35 s standing in a gap pauses the cloud; the next steps carry it on from the last bridged point, not from the
        // fix before the gap.
        let mut l = Locator::default();
        l.set_graph(grid());
        let walked = |t: i64| 10_000 + t.min(80) * 193 / 100 + (t - 115).max(0) * 193 / 100; // stands from 80 s to 115 s
        let az = |t: i64| if t < 71 { 90.0 } else { 0.0 }; // north at the x = 100 m crossing
        walk_into_a_gap(&mut l, 40, 70, az);
        let started = l.bridge.as_ref().unwrap().started_ms;
        let before = *gap_steps(&mut l, 70, 81, walked, az).last().unwrap();
        assert!(gap_steps(&mut l, 81, 116, walked, az).is_empty() && !l.bridging(), "standing 35 s pauses the bridge");
        let after = gap_steps(&mut l, 116, 118, walked, az);
        assert!(l.bridging() && l.bridge.as_ref().unwrap().started_ms == started, "the same cloud goes on");
        let back = distance_m(before.point(), after[0].point()) - 1.4 * 3.0;
        assert!(back <= 5.0, "a {back:.1} m jump from {before:?} to {:?}", after[0]);
    }

    #[test]
    fn a_bridged_pin_says_so_while_it_ages() {
        // Ruling T22-R3: the zone must never come from a bridged pin, also once it reads "predicted" or "stale".
        let mut l = Locator::default();
        l.set_graph(grid());
        let last = *walk_into_a_gap(&mut l, 40, 60, |_| 90.0).last().unwrap();
        for (dt, source) in [(1_000, DisplaySource::Bridged), (10_000, DisplaySource::Predicted), (31_000, DisplaySource::Stale)] {
            let d = l.display(last.t_ms + dt).unwrap();
            assert_eq!((d.source, d.bridged_origin), (source, true), "+{dt} ms");
        }
        l.on_fix(&fix(on_street(1.4 * 62.0), 62, 4.0));
        assert!(!l.display(62_000).unwrap().bridged_origin, "a GPS estimate is not");
    }

    #[test]
    fn turning_round_while_stopped_in_a_gap_walks_the_pin_back() {
        // Review round 3 (Important): standing in a gap, a compass turn is the walker turning round, not the phone moving. The carry
        // stays and the cloud walks back the way the walker now goes.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 25, 45, |_| 90.0); // the last fix at x = 34 m; walking on to x = 63 m, mid-block
        let delta = l.carry().delta_deg();
        let total = |t: i64| 10_000 + t.min(45) * 193 / 100 + (t - 80).max(0) * 193 / 100; // stands 45..80 s
        let az = |t: i64| if t < 60 { 90.0 } else { 270.0 }; // turns round at 60 s
        gap_steps(&mut l, 45, 80, total, az);
        let last = *gap_steps(&mut l, 80, 101, total, az).last().unwrap();
        assert!(conf(&l) >= 0.5 && (l.carry().delta_deg() - delta).abs() < 1e-9, "the carry is kept: {} {}", conf(&l), l.carry().delta_deg());
        let want = on_street(63.0 - 1.4 * 20.0);
        assert!(
            distance_m(last.point(), want) < 8.0,
            "{:.1} m off; {:.1} m from where they stood",
            distance_m(last.point(), want),
            distance_m(last.point(), on_street(63.0))
        );
    }

    #[test]
    fn walking_on_after_a_stop_reports_the_walking_speed() {
        // Review round 3 (minor 3): the first resumed batch's speed is its steps over the batch, not over the stop.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 25, 45, |_| 90.0);
        let total = |t: i64| 10_000 + t.min(45) * 193 / 100 + (t - 80).max(0) * 193 / 100;
        gap_steps(&mut l, 45, 80, total, |_| 90.0);
        let first = gap_steps(&mut l, 80, 83, total, |_| 90.0)[0];
        assert!((0.8..2.5).contains(&first.speed_mps), "{first:?}");
    }

    #[test]
    fn a_fix_while_stopped_in_a_gap_starts_the_filter_standing() {
        // Review round 3 (minor 2): re-anchoring a paused cloud gives it no velocity, and standing is the likeliest model.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 25, 45, |_| 90.0);
        let total = |t: i64| 10_000 + t.min(45) * 193 / 100;
        gap_steps(&mut l, 45, 80, total, |_| 90.0);
        l.reanchor_bridge(&RawFix::at(o().lat, o().lon, 80_000, 5.0));
        let imm = l.imm.as_ref().unwrap();
        assert!(imm.mu[S] > 0.5 && imm.models[W].x[2].hypot(imm.models[W].x[3]) < 1e-9, "{:?} {:?}", imm.mu, imm.models[W].x);
    }

    #[test]
    fn fixes_without_steps_between_build_on_one_another() {
        // Review round 3 (minor 4): the cloud re-anchors the filter only once it has stepped since the last re-anchor.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 40, 56, |_| 90.0);
        l.on_fix(&multipath(56));
        l.imm.as_mut().unwrap().t_ms = 56_500; // a mark the next re-anchor would undo
        l.reanchor_bridge(&RawFix::at(o().lat, o().lon, 56_600, 5.0));
        assert_eq!(l.imm.as_ref().unwrap().t_ms, 56_500, "not re-anchored without steps");
        gap_steps(&mut l, 56, 58, |t| 10_000 + t * 193 / 100, |_| 90.0);
        l.reanchor_bridge(&RawFix::at(o().lat, o().lon, 58_000, 5.0));
        assert_eq!(l.imm.as_ref().unwrap().t_ms, l.bridge.as_ref().unwrap().last_step_ms, "re-anchored after a step batch");
    }

    #[test]
    fn the_bridge_never_outlasts_the_reset_gap() {
        // Review M12: bridge_max_ms counts from the last accepted fix (39 s), like reset_gap_ms.
        let mut l = Locator::default();
        let bridged = walk_into_a_gap(&mut l, 40, 400, |_| 90.0);
        let last = bridged.last().unwrap().t_ms;
        assert!(last - 39_000 <= l.params().reset_gap_ms, "bridged until {last}");
    }

    #[test]
    fn at_the_bridge_maximum_the_pin_stays_and_turns_stale() {
        // Review I5: clearing the filter at bridge_max_ms keeps the last bridged pin, which then ages by the display rules.
        let mut l = Locator::default();
        let last = *walk_into_a_gap(&mut l, 40, 400, |_| 90.0).last().unwrap();
        assert!(!l.bridging());
        let d = l.display(last.t_ms + 1_000).expect("the pin stays");
        assert_eq!((d.est_lat, d.est_lon, d.source), (last.lat, last.lon, DisplaySource::Bridged));
        assert_eq!(l.display(last.t_ms + 31_000).map(|d| d.source), Some(DisplaySource::Stale));
    }

    /// Multipath in a gap: a 30 m fix 100 m off the street at `t_s`, north and south by turns, so no three in a row agree (and too
    /// coarse for the quick relocation). The gate judges it against the cloud itself, not the prior widened for the update
    /// (adversarial re-review N2).
    fn multipath(t_s: i64) -> RawFix {
        fix(destination(on_street(1.4 * i64_to_f64(t_s)), if t_s % 2 == 0 { 0.0 } else { 180.0 }, 100.0), t_s, 30.0)
    }

    /// Steps and compass readings at `t_s` (GPS back in the gap, the walk goes on), then the fix `f`.
    fn walking_fix(l: &mut Locator, t_s: i64, f: &RawFix) -> Estimate {
        gap_steps(l, t_s, t_s + 1, |t| 10_000 + t * 193 / 100, |_| 90.0);
        l.on_fix(f)
    }

    #[test]
    fn alternating_multipath_after_a_bridged_gap_never_counts_far_from_the_truth() {
        // Adversarial re-review N2: the prior widened to the fix's accuracy also gated, so a 30 m fix 100 m off passed; the filter
        // locked onto one ghost and, after the warm-up, counted estimates about 100 m off.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 40, 70, |_| 90.0);
        let far: Vec<(i64, f64)> = (70..82)
            .filter_map(|t| {
                let e = walking_fix(&mut l, t, &multipath(t));
                let off = distance_m(e.point(), on_street(1.4 * i64_to_f64(t)));
                (e.accepted && off > 25.0).then_some((t, off))
            })
            .collect();
        assert!(far.is_empty(), "accepted far from the street: {far:?}");
    }

    #[test]
    fn one_multipath_fix_after_a_bridged_gap_does_not_drag_the_pin() {
        // Adversarial re-review N2: a single 30 m fix 100 m off pulled the pin about 70 m, and the good fixes after it were gated.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 40, 70, |_| 90.0);
        let e = walking_fix(&mut l, 70, &multipath(70));
        let pin = l.display(70_000).expect("a pin");
        let dragged = distance_m(Point::new(pin.est_lat, pin.est_lon), on_street(98.0));
        assert!(dragged <= 10.0, "pin {dragged:.1} m off the street after {e:?}");
        let good: Vec<Estimate> = (71..77).map(|t| walking_fix(&mut l, t, &fix(on_street(1.4 * i64_to_f64(t)), t, 4.0))).collect();
        assert!(good.iter().all(|e| matches!(e.verdict, Verdict::Used | Verdict::Soft)), "the good fixes are taken: {good:?}");
    }

    #[test]
    fn fixes_the_filter_does_not_accept_leave_the_cloud_running() {
        // Review I2: only an accepted fix ends the bridge; 30 s of gated fixes neither tear it down nor flicker the pin.
        let mut l = Locator::default();
        l.set_graph(grid());
        let walked = |t: i64| 10_000 + t * 193 / 100;
        walk_into_a_gap(&mut l, 40, 56, |_| 90.0);
        let started = l.bridge.as_ref().unwrap().started_ms;
        for t in 56..86 {
            gap_steps(&mut l, t, t + 1, walked, |_| 90.0);
            let e = l.on_fix(&multipath(t));
            assert!(!e.accepted, "t {t}: {e:?}");
            assert!(l.bridging() && l.bridge.as_ref().unwrap().started_ms == started, "t {t}: one bridge");
            assert_eq!(l.display(t * 1000).map(|d| d.source), Some(DisplaySource::Bridged), "t {t}: no flicker");
        }
        assert!(l.used.len() <= 1, "the fixes before the gap are no part of the line fit: {}", l.used.len());
        let e = l.on_fix(&fix(on_street(1.4 * 86.0), 86, 4.0));
        assert!(e.verdict == Verdict::Used && !l.bridging(), "a good fix ends it: {e:?}");
        assert!(distance_m(e.point(), on_street(1.4 * 86.0)) < 8.0, "{} m", distance_m(e.point(), on_street(1.4 * 86.0)));
        let counted = (87..90).map(|t| l.on_fix(&fix(on_street(1.4 * i64_to_f64(t)), t, 4.0))).last().unwrap();
        assert!(counted.accepted, "the fourth fix after the re-anchor counts (C1): {counted:?}");
    }

    #[test]
    fn a_gap_of_gated_fixes_is_not_bridged() {
        // Review I2: the filter still sees fixes (gated, its clock goes on), so this is no gap without fixes.
        let mut l = Locator::default();
        l.set_graph(grid());
        let walked = |t: i64| 10_000 + t * 193 / 100;
        walk_into_a_gap(&mut l, 40, 40, |_| 90.0);
        for t in 40..70 {
            let out = gap_steps(&mut l, t, t + 1, walked, |_| 90.0);
            l.on_fix(&multipath(t));
            assert!(out.is_empty() && !l.bridging(), "t {t}");
        }
    }

    #[test]
    fn the_line_fit_forgets_the_fixes_before_a_bridged_gap() {
        // Review M14: the first fix after the gap is no line with the fixes before it.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 40, 70, |_| 90.0);
        assert_eq!(l.on_fix(&fix(on_street(1.4 * 70.0), 70, 4.0)).verdict, Verdict::Used);
        assert_eq!(l.used.len(), 1);
    }

    #[test]
    fn a_swinging_compass_steers_by_the_last_course() {
        // Task 22 note 5: an instantaneous reading in a swing is no heading.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 40, 56, |_| 90.0);
        let steady = l.gap_steer(56_000).unwrap();
        assert!((steady.theta_deg - 90.0).abs() < 10.0 && steady.sigma_deg < 25.0, "{steady:?}");
        for i in 0..4 {
            l.on_heading(&HeadingIn { t_ms: 56_000 + i * 500, azimuth_deg: if i % 2 == 0 { 300.0 } else { 60.0 }, ..HeadingIn::default() });
        }
        let swung = l.gap_steer(57_500).unwrap();
        assert!((swung.theta_deg - 90.0).abs() < 1.0, "the last course, not the swing's mean (0): {swung:?}");
        assert!(swung.sigma_deg > steady.sigma_deg, "with its growing sigma: {swung:?}");
    }

    #[test]
    fn a_compass_jump_mid_street_is_the_phone_moving_and_the_bridge_keeps_the_street_course() {
        // Task 22 note 5: watch_gap runs on every compass reading in a gap, not only with steps. Mid-block the walker cannot turn, so a
        // jump with the same cadence is the phone moving: the carry resets, and the gap steers by the compass against the street course.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_into_a_gap(&mut l, 25, 37, |_| 90.0); // the last fix at x = 34 m; bridging from x = 50 m, midway between crossings
        assert!(conf(&l) >= 0.5 && l.bridging(), "{}", conf(&l));
        let jump = |t_ms| HeadingIn { t_ms, azimuth_deg: 180.0, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 80.0, ..HeadingIn::default() };
        l.on_heading(&jump(37_000));
        l.on_heading(&jump(37_500));
        assert!(l.carry().confidence(l.params(), 37_500) < 0.5, "reset by the readings alone");
        for t in (38_000..=40_000).step_by(500) {
            if t % 2_000 == 0 {
                let _ = l.on_steps(10_000 + t / 1000 * 193 / 100, t, None);
            }
            l.on_heading(&jump(t));
        }
        let s = l.gap_steer(40_000).unwrap();
        assert!(wrap_deg(s.theta_deg - 90.0).abs() < 15.0, "still east along the street: {s:?}");
    }

    #[test]
    fn a_compass_turn_at_a_crossing_is_a_turn_not_a_phone_move() {
        let mut l = Locator::default();
        l.set_graph(grid());
        // The gap starts at x = 56 m; bridging from x = 70 m, the crossing at x = 100 m comes at 71 s.
        walk_into_a_gap(&mut l, 40, 72, |t| if t < 71 { 90.0 } else { 0.0 });
        assert!(l.bridging() && conf(&l) >= 0.5, "the carry is kept: {}", conf(&l));
        let last = *gap_steps(&mut l, 72, 84, |t| 10_000 + t * 193 / 100, |_| 0.0).last().unwrap();
        let s = l.gap_steer(84_000).unwrap();
        assert!(wrap_deg(s.theta_deg).abs() < 15.0, "steered north by the compass and the kept carry: {s:?}");
        let crossing = on_street(100.0);
        let (east, north) = (crate::geo::bearing_deg(crossing, last.point()), distance_m(crossing, last.point()));
        let (x, y) = (north * east.to_radians().sin(), north * east.to_radians().cos());
        assert!(x.abs() < 5.0 && y > 5.0, "on the north street: {x:.1} m east, {y:.1} m north of the crossing");
    }

    #[test]
    fn a_compass_jump_at_a_crossing_the_cloud_walks_past_is_the_phone_moving_after_all() {
        // Review M8: on the y = 0 street the phone turns south at the x = 100 m T-junction while the walker goes on east. Near the
        // crossing the jump reads as a turn; 20 m on, the street did not turn but the compass stays rotated: the phone moved.
        let mut l = Locator::default();
        l.set_graph(grid());
        let along = |x: f64| destination(o(), 90.0, x);
        let az = |t: i64| if t < 69 { 90.0 } else { 180.0 };
        for t in 0..40 {
            if t % 2 == 0 {
                let _ = l.on_steps(10_000 + t * 193 / 100, t * 1000, None);
            }
            for half in 0..2 {
                l.on_heading(&HeadingIn {
                    t_ms: t * 1000 + half * 500,
                    azimuth_deg: az(t),
                    accuracy: crate::loc::CompassAccuracy::High,
                    pitch_deg: 10.0,
                    ..HeadingIn::default()
                });
            }
            l.on_fix(&fix(along(1.4 * i64_to_f64(t)), t, 4.0));
        }
        gap_steps(&mut l, 40, 72, |t| 10_000 + t * 193 / 100, az);
        assert!(conf(&l) >= 0.5, "near the crossing the jump is a turn: {}", conf(&l));
        let last = *gap_steps(&mut l, 72, 100, |t| 10_000 + t * 193 / 100, az).last().unwrap();
        let s = l.gap_steer(100_000).unwrap();
        assert!(wrap_deg(s.theta_deg - 90.0).abs() < 20.0, "steered east again: {s:?}");
        // Ruling T22-R4: the cloud the wrong compass drew off the street comes back to it.
        let off = distance_m(last.point(), Point::new(o().lat, last.lon));
        assert!(off < 8.0, "{off:.1} m off the y = 0 street: {last:?}");
    }

    #[test]
    fn a_split_or_unknown_direction_starts_the_cloud_both_ways() {
        // Review I4: a street within 30 degrees of perpendicular to the course, or no course, says nothing of the direction.
        let p = LocParams::default();
        let at = destination(destination(o(), 0.0, 100.0), 90.0, 50.0);
        let share_east = |course: Option<f64>| {
            let b = Bridge::start(grid(), Frame::new(o()), mode_mask(Mode::Walk), at, [[4.0, 0.0], [0.0, 4.0]], 0.05, None, course, 0, &p);
            b.street_dirs().iter().filter(|(bearing, _)| (bearing - 90.0).abs() < 1.0).map(|(_, w)| w).sum::<f64>()
                / b.street_dirs().iter().map(|(_, w)| w).sum::<f64>()
        };
        assert!(share_east(Some(80.0)) > 0.99, "{}", share_east(Some(80.0)));
        assert!(share_east(Some(260.0)) < 0.01, "{}", share_east(Some(260.0)));
        for c in [None, Some(10.0), Some(170.0)] {
            assert!((share_east(c) - 0.5).abs() < 0.1, "{c:?}: {}", share_east(c));
        }
    }

    #[test]
    fn after_standing_the_gap_goes_the_way_the_compass_says() {
        // Review I4: walk east, stand (the hold forgets the course), then walk west into a gap with the compass west.
        let mut l = Locator::default();
        l.set_graph(grid());
        let total = |t: i64| 10_000 + t.min(40) * 193 / 100 + (t - 70).max(0) * 193 / 100;
        walk_into_a_gap(&mut l, 40, 40, |_| 90.0);
        for t in 40..70 {
            gap_steps(&mut l, t, t + 1, total, |_| 90.0);
            l.on_fix(&fix(on_street(56.0), t, 4.0));
        }
        assert!(l.holding() && l.last_course.is_none(), "a hold forgets the course: {:?}", l.last_course);
        let last = *gap_steps(&mut l, 70, 100, total, |_| 270.0).last().unwrap();
        let want = on_street(56.0 - 1.4 * 30.0);
        assert!(
            distance_m(last.point(), want) < 10.0,
            "{:.1} m off, {:.1} m from where they stood",
            distance_m(last.point(), want),
            distance_m(last.point(), on_street(56.0))
        );
    }

    #[test]
    fn fixes_after_a_bridged_gap_pull_to_the_gps_and_count_only_after_three() {
        // Adversarial review C1: the bridge re-anchor keeps the pin continuous, but its prior is no evidence. The player stood `off`
        // metres north of where the steps say; the fixes after the gap pull the estimate there, and none of the first three counts.
        for (off, acc) in [(15.0, 20.0), (22.0, 25.0), (30.0, 35.0)] {
            let mut l = Locator::default();
            l.set_graph(grid());
            walk_into_a_gap(&mut l, 40, 70, |_| 90.0);
            let pin = l.bridged.expect("bridged through the gap");
            let gps = destination(pin.point(), 0.0, off);
            let mut taken = Vec::new();
            for t in 70..76 {
                if t % 2 == 0 {
                    l.on_steps(10_000 + t * 193 / 100, t * 1000, None); // the phone shaken on: steps go on after GPS is back
                }
                taken.push(l.on_fix(&fix(gps, t, acc)));
            }
            assert!(taken[..3].iter().all(|e| !e.accepted && matches!(e.verdict, Verdict::Used | Verdict::Soft)), "off {off}: {taken:?}");
            assert!(distance_m(taken[0].point(), gps) <= 0.5 * off, "off {off}: first {:.1} m from the fix", distance_m(taken[0].point(), gps));
            assert!(distance_m(taken[2].point(), gps) <= 0.25 * off, "off {off}: third {:.1} m from the fix", distance_m(taken[2].point(), gps));
            assert!(taken[0].uncertainty_m >= 0.5 * acc, "off {off}: the prior is no tighter than the fix: {:.1}", taken[0].uncertainty_m);
            assert!(taken[3].accepted && distance_m(taken[3].point(), gps) <= 0.25 * off, "off {off}: {:?}", taken[3]);
            assert!(!l.bridging(), "a taken fix ends the bridge");
        }
    }
}
