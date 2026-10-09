//! Step evidence: the step counter's readings ([`StepHistory`]) and what the filter reads from them.

use std::collections::VecDeque;

use crate::catalog::Mode;
use crate::loc::calib::{Calibrator, StepCal};
use crate::loc::imm::{S, W};
use crate::num::i64_to_f64;

use super::Locator;

/// Recent readings of the phone's cumulative step counter (the last two minutes).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepHistory {
    pts: VecDeque<(i64, i64)>,
    /// Time of the newest step event (a reading from the sensor itself), ms.
    last_event_ms: Option<i64>,
}

impl StepHistory {
    /// Add a step event (a reading from the sensor at its own time); one not newer than the last event is ignored, a counter that went
    /// down (the phone restarted it) starts over. Fallback readings at or after its time give way to it (ruling FR-I3).
    pub fn push(&mut self, total: i64, t_ms: i64) {
        if self.last_event_ms.is_some_and(|e| t_ms <= e) {
            return;
        }
        while self.pts.back().is_some_and(|(t, _)| *t >= t_ms && self.last_event_ms.is_none_or(|e| *t > e)) {
            self.pts.pop_back();
        }
        self.last_event_ms = Some(t_ms);
        self.add(total, t_ms);
    }

    /// Add a fallback reading: the total the host last saw, stamped at a fix's time because no step event came for a while. It is not a
    /// step event (a counter that only has these is present while they come; one that had an event stays present).
    pub fn push_fallback(&mut self, total: i64, t_ms: i64) {
        self.add(total, t_ms);
    }

    /// Time of the newest step event, ms.
    #[must_use]
    pub fn last_event_ms(&self) -> Option<i64> {
        self.last_event_ms
    }

    fn add(&mut self, total: i64, t_ms: i64) {
        if let Some(&(t, last)) = self.pts.back() {
            if t_ms <= t {
                return;
            }
            if total < last {
                self.pts.clear();
            }
        }
        self.pts.push_back((t_ms, total));
        while self.pts.front().is_some_and(|(t, _)| t_ms.saturating_sub(*t) > 120_000) {
            self.pts.pop_front();
        }
    }

    /// Whether the phone has a step counter: a step event arrived since the history last started (ruling FR-N1: Android's counter reports
    /// only when the count changes, so a player standing still gets no events; a dead sensor is covered by the quiet bound, FR-C1). A host
    /// that sends totals only with fixes: a reading arrived within `within_ms` before `t_ms`.
    #[must_use]
    pub fn present(&self, t_ms: i64, within_ms: i64) -> bool {
        self.last_event_ms.is_some() || self.pts.back().is_some_and(|(t, _)| t_ms.saturating_sub(*t) <= within_ms)
    }

    /// The newest reading: time (ms) and cumulative total.
    #[must_use]
    pub fn latest(&self) -> Option<(i64, i64)> {
        self.pts.back().copied()
    }

    /// The cumulative total at `t_ms`: the newest reading at or before it, else the oldest reading; `None` without readings.
    #[must_use]
    pub fn total_at(&self, t_ms: i64) -> Option<i64> {
        self.pts.iter().rev().find(|(t, _)| *t <= t_ms).or(self.pts.front()).map(|(_, n)| *n)
    }

    /// Steps taken between `from_ms` and `to_ms` (0 without readings).
    #[must_use]
    pub fn gained(&self, from_ms: i64, to_ms: i64) -> i64 {
        match (self.total_at(from_ms), self.total_at(to_ms)) {
            (Some(a), Some(b)) => (b - a).max(0),
            _ => 0,
        }
    }

    /// Whether the counter stood still over the `window_ms` before `t_ms`: readings reach back that far and none gained a step. False
    /// while the readings are younger than the window (no evidence yet).
    #[must_use]
    pub fn quiet(&self, t_ms: i64, window_ms: i64) -> bool {
        self.pts.front().is_some_and(|(t, _)| *t <= t_ms.saturating_sub(window_ms)) && self.gained(t_ms.saturating_sub(window_ms), t_ms) == 0
    }

    /// Steps per second over the 10 s before `t_ms`, when the readings span at least 2 s.
    #[must_use]
    pub fn cadence(&self, t_ms: i64) -> Option<f64> {
        let first = self.pts.iter().find(|(t, _)| *t >= t_ms.saturating_sub(10_000))?;
        let last = self.pts.iter().rev().find(|(t, _)| *t <= t_ms)?;
        let dt = i64_to_f64(last.0 - first.0) / 1000.0;
        (dt >= 2.0).then(|| i64_to_f64(last.1 - first.1) / dt)
    }
}

impl Locator {
    /// The step counter readings of this session.
    #[must_use]
    pub fn steps(&self) -> &StepHistory {
        &self.steps
    }

    /// The step total the host sent with a fix (ruling FR-I3): steps reach the filter as step events ([`Self::on_steps`]) at their
    /// own time; this total is kept, at the fix's time, only when no step event came for `steps_fallback_ms`. Steps coming in end a
    /// stationary hold.
    pub fn on_fix_steps(&mut self, total: i64, t_ms: i64) {
        if self.steps.last_event_ms().is_some_and(|e| t_ms.saturating_sub(e) <= self.params.steps_fallback_ms) {
            return;
        }
        self.steps.push_fallback(total, t_ms);
        if self.moving_steps(t_ms) {
            self.hold = None;
        }
    }

    /// Load the phone's saved step calibration (ignored for another source). The filter itself still starts fresh.
    pub fn set_step_calibration(&mut self, c: StepCal) {
        let _ = self.calib.set(c, &self.params);
    }

    /// The step calibrator (diagnostics and the bench).
    #[must_use]
    pub fn step_calibrator(&self) -> &Calibrator {
        &self.calib
    }

    /// The step calibration to save.
    #[must_use]
    pub fn step_calibration(&self) -> StepCal {
        self.calib.cal()
    }

    /// In a Walk or Run zone, a step counter is present and reported no new steps for `hold_quiet_steps_ms`: the player likely stands,
    /// so only fixes `hold_quiet_max_m` out end a hold (rulings T8-R2, FR-C1). Not in Bike or Drive zones, where a quiet counter says
    /// nothing (ruling T8-R18).
    pub(super) fn quiet_counter(&self, t_ms: i64) -> bool {
        matches!(self.mode, Mode::Walk | Mode::Run)
            && self.steps.present(t_ms, self.params.steps_present_ms)
            && self.quiet_steps(t_ms, self.params.hold_quiet_steps_ms)
    }

    pub(super) fn moving_steps(&self, t_ms: i64) -> bool {
        self.steps.present(t_ms, self.params.steps_present_ms)
            && self.steps.gained(t_ms.saturating_sub(self.params.steps_moving_window_ms), t_ms) >= self.params.steps_moving_min
    }

    /// No new steps for `window_ms`. Without a step counter there is no step evidence, so the filter must have watched the player for
    /// that long instead: a walk that starts in noisy fixes is not frozen in its first seconds (Review Focus 2).
    pub(super) fn quiet_steps(&self, t_ms: i64, window_ms: i64) -> bool {
        if self.steps.present(t_ms, self.params.steps_present_ms) {
            self.steps.gained(t_ms.saturating_sub(window_ms), t_ms) == 0
        } else {
            self.started_ms.is_some_and(|s| t_ms.saturating_sub(s) >= window_ms)
        }
    }

    /// Step evidence as model likelihood factors (S, W, F): steps coming in make standing unlikely; none for 10 s makes walking unlikely
    /// in a Walk/Run zone. Neutral without a step counter.
    pub(super) fn step_factors(&self, t_ms: i64) -> [f64; 3] {
        let mut l = [1.0; 3];
        if self.steps.present(t_ms, self.params.steps_present_ms) {
            if self.moving_steps(t_ms) {
                l[S] *= self.params.steps_moving_factor;
            }
            if matches!(self.mode, Mode::Walk | Mode::Run) && self.quiet_steps(t_ms, self.params.steps_quiet_window_ms) {
                l[W] *= self.params.steps_quiet_factor;
            }
        }
        l
    }
}

#[cfg(test)]
mod tests {

    use crate::loc::locator::StepHistory;

    #[test]
    fn step_history_gains_cadence_and_counter_restarts() {
        let mut h = StepHistory::default();
        for (t, n) in [(0, 100), (2_000, 104), (4_000, 108), (6_000, 112)] {
            h.push(n, t);
        }
        assert_eq!(h.gained(2_000, 6_000), 8);
        assert!((h.cadence(6_000).unwrap() - 2.0).abs() < 1e-9);
        h.push(5, 8_000);
        assert_eq!(h.gained(7_000, 8_000), 0, "a restarted counter is a new baseline");
        h.push(3, 7_000);
        assert_eq!(h.gained(0, 9_000), 0, "an older reading is ignored");
    }

    #[test]
    fn a_counter_with_an_event_stays_present_and_fallbacks_alone_only_while_they_come() {
        // Ruling FR-N1: no time window once a step event came (a standing player's counter sends nothing).
        let mut h = StepHistory::default();
        assert!(!h.present(0, 600_000), "no readings");
        h.push(100, 0);
        assert!(h.present(3_600_000, 600_000), "an hour of standing after one event");
        let mut f = StepHistory::default();
        f.push_fallback(100, 0);
        assert!(f.present(600_000, 600_000) && !f.present(600_001, 600_000), "fallback readings only: present while they come");
    }
}
