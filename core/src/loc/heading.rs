//! Compass readings: the "held flat and steady" rule for the map arrow, and the carry offset between where the phone points
//! and where the player walks.

use std::collections::VecDeque;

use crate::loc::{HeadingIn, LocParams};
use crate::num::{count_f64, i64_to_f64};

/// `d` folded into (-180, 180] degrees.
#[must_use]
pub fn wrap_deg(d: f64) -> f64 {
    let w = (d + 180.0).rem_euclid(360.0) - 180.0;
    if w <= -180.0 {
        180.0
    } else {
        w
    }
}

/// The mean direction of `angles` (degrees, in [0, 360)); `None` when they cancel out.
#[must_use]
pub fn circular_mean_deg(angles: &[f64]) -> Option<f64> {
    let (s, c) = angles.iter().fold((0.0, 0.0), |(s, c), a| (s + a.to_radians().sin(), c + a.to_radians().cos()));
    // `rem_euclid` of a tiny negative angle rounds to 360.0: fold it to 0.
    let deg = s.atan2(c).to_degrees().rem_euclid(360.0);
    (s.hypot(c) > 1e-9 * count_f64(angles.len().max(1))).then_some(if deg >= 360.0 { 0.0 } else { deg })
}

/// How far the phone's screen is tilted from flat, degrees (0 = lying flat, 90 = upright).
#[must_use]
pub fn tilt_deg(pitch_deg: f64, roll_deg: f64) -> f64 {
    (pitch_deg.to_radians().cos() * roll_deg.to_radians().cos()).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The sigma of the last GPS course `gap_s` seconds into a gap: `carry_gap_sigma0_deg` growing `carry_gap_sigma_per_s`, at most
/// `carry_gap_sigma_max_deg`.
#[must_use]
pub fn gap_course_sigma_deg(gap_s: f64, p: &LocParams) -> f64 {
    (p.carry_gap_sigma0_deg + p.carry_gap_sigma_per_s * gap_s).min(p.carry_gap_sigma_max_deg)
}

/// The last few seconds of compass readings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Compass {
    recent: VecDeque<HeadingIn>,
}

impl Compass {
    /// Add a reading (older than the newest is ignored); keeps 20 s.
    pub fn push(&mut self, h: HeadingIn) {
        if self.recent.back().is_some_and(|b| h.t_ms <= b.t_ms) {
            return;
        }
        self.recent.push_back(h);
        while self.recent.front().is_some_and(|f| h.t_ms.saturating_sub(f.t_ms) > 20_000) {
            self.recent.pop_front();
        }
    }

    /// Time of the newest reading, ms.
    #[must_use]
    pub fn newest_ms(&self) -> Option<i64> {
        self.recent.back().map(|h| h.t_ms)
    }

    /// The newest reading, if it is at most `fresh_ms` old at `now_ms`.
    #[must_use]
    pub fn latest(&self, now_ms: i64, fresh_ms: i64) -> Option<HeadingIn> {
        self.recent.back().filter(|h| now_ms.saturating_sub(h.t_ms) <= fresh_ms).copied()
    }

    fn window(&self, now_ms: i64, window_ms: i64) -> impl Iterator<Item = &HeadingIn> {
        self.recent.iter().filter(move |h| h.t_ms <= now_ms && now_ms.saturating_sub(h.t_ms) <= window_ms)
    }

    fn window_azimuths(&self, now_ms: i64, window_ms: i64) -> Vec<f64> {
        self.window(now_ms, window_ms).map(|h| h.azimuth_deg).collect()
    }

    /// The largest angle between a reading of the last `window_ms` and their mean, degrees.
    #[must_use]
    pub fn spread_deg(&self, now_ms: i64, window_ms: i64) -> Option<f64> {
        let az = self.window_azimuths(now_ms, window_ms);
        let m = circular_mean_deg(&az)?;
        Some(az.iter().map(|a| wrap_deg(a - m).abs()).fold(0.0, f64::max))
    }

    /// The reading nearest in time to `t_ms` (before or after it: batched fixes arrive after newer readings), if at most `fresh_ms` away.
    #[must_use]
    pub fn nearest(&self, t_ms: i64, fresh_ms: i64) -> Option<HeadingIn> {
        self.recent
            .iter()
            .filter(|h| h.t_ms.saturating_sub(t_ms).saturating_abs() <= fresh_ms)
            .min_by_key(|h| h.t_ms.saturating_sub(t_ms).saturating_abs())
            .copied()
    }

    /// How far the azimuth turned over the last `window_ms` (first to last reading), degrees, and over how long, seconds.
    fn turn(&self, now_ms: i64, window_ms: i64) -> Option<(f64, f64)> {
        let mut w = self.window(now_ms, window_ms);
        let first = w.next()?;
        let last = w.last()?;
        let dt = i64_to_f64(last.t_ms.saturating_sub(first.t_ms)) / 1000.0;
        (dt > 0.0).then(|| (wrap_deg(last.azimuth_deg - first.azimuth_deg).abs(), dt))
    }

    /// How fast the azimuth turned over the last `window_ms`, degrees per second (first to last reading).
    #[must_use]
    pub fn rate_deg_s(&self, now_ms: i64, window_ms: i64) -> Option<f64> {
        self.turn(now_ms, window_ms).map(|(deg, dt)| deg / dt)
    }

    /// How far the azimuth turned over the last `window_ms` (first to last reading), degrees.
    #[must_use]
    pub fn turn_deg(&self, now_ms: i64, window_ms: i64) -> Option<f64> {
        self.turn(now_ms, window_ms).map(|(deg, _)| deg)
    }

    /// The angle between the readings of the last `window_ms` farthest apart on either side of their mean, degrees (twice the spread
    /// for two readings; it also sees a swing there and back).
    #[must_use]
    pub fn range_deg(&self, now_ms: i64, window_ms: i64) -> Option<f64> {
        let az = self.window_azimuths(now_ms, window_ms);
        let m = circular_mean_deg(&az)?;
        let (lo, hi) = az.iter().map(|a| wrap_deg(a - m)).fold((0.0, 0.0), |(lo, hi): (f64, f64), d| (lo.min(d), hi.max(d)));
        Some(hi - lo)
    }

    /// The mean azimuth of the readings of the last `window_ms`, degrees.
    #[must_use]
    pub fn mean_deg(&self, now_ms: i64, window_ms: i64) -> Option<f64> {
        circular_mean_deg(&self.window_azimuths(now_ms, window_ms))
    }

    /// Swinging over the last `compass_window_ms`: the readings span more than `carry_jump_deg` or turn at `carry_max_rate_deg_s` or faster
    /// (Task 22 note 5: one reading in a swing is no heading).
    #[must_use]
    pub fn swinging(&self, now_ms: i64, p: &LocParams) -> bool {
        self.range_deg(now_ms, p.compass_window_ms).is_some_and(|r| r > p.carry_jump_deg)
            || self.rate_deg_s(now_ms, p.compass_window_ms).is_some_and(|r| r >= p.carry_max_rate_deg_s)
    }

    /// The compass azimuth to show while standing: only with a fresh reading, accuracy medium or better ([`HeadingIn::accurate`]), the
    /// phone within 60 degrees of flat and the azimuth steady (spread under 10 degrees over 2 s). Otherwise no arrow: an unreliable compass is dropped, not shown.
    #[must_use]
    pub fn held_flat_and_steady(&self, now_ms: i64, p: &LocParams) -> Option<f64> {
        let h = self.latest(now_ms, p.compass_fresh_ms)?;
        let accurate = h.accurate();
        let flat = tilt_deg(h.pitch_deg, h.roll_deg) <= p.compass_max_tilt_deg;
        let steady = self.spread_deg(now_ms, p.compass_window_ms).is_some_and(|s| s < p.compass_max_spread_deg);
        if accurate && flat && steady {
            circular_mean_deg(&self.window_azimuths(now_ms, p.compass_window_ms))
        } else {
            None
        }
    }
}

/// Fewer residuals than this have no steadiness.
const MIN_STEADY_RESIDUALS: usize = 5;

/// A carry change is a run of at least this many one-sided innovations (ruling T20-change).
const MIN_CHANGE_RUN: usize = 3;

/// A pending carry change is forgotten after a pause in learning this much longer than `carry_change_ms` (review M1).
const CHANGE_EXPIRY_MS: i64 = 5_000;

/// The offset between where the phone points and where the player walks, `delta = course - azimuth`, learned online against good GPS
/// courses (a 1-D circular Kalman filter) with a confidence. It changes whenever the phone moves (hand, pocket, bag).
#[derive(Debug, Clone, PartialEq)]
pub struct CarryOffset {
    delta_deg: f64,
    var_deg2: f64,
    last_ms: Option<i64>,
    resid: VecDeque<(i64, f64)>,
    /// Since when innovations have been beyond `carry_change_sigmas`.
    over_since: Option<i64>,
    /// The current run of innovations of at least `carry_change_min_deg`, all on one side (ruling T20-change).
    run: VecDeque<(i64, f64)>,
}

impl Default for CarryOffset {
    fn default() -> Self {
        Self {
            delta_deg: 0.0,
            var_deg2: LocParams::default().carry_reset_var_deg2,
            last_ms: None,
            resid: VecDeque::new(),
            over_since: None,
            run: VecDeque::new(),
        }
    }
}

impl CarryOffset {
    /// The learned offset, degrees.
    #[must_use]
    pub fn delta_deg(&self) -> f64 {
        self.delta_deg
    }

    /// When it last learned (the fix's time), if ever.
    #[must_use]
    pub fn learned_ms(&self) -> Option<i64> {
        self.last_ms
    }

    /// Its sigma, degrees.
    #[must_use]
    pub fn sigma_deg(&self) -> f64 {
        self.var_deg2.max(0.0).sqrt()
    }

    /// The residuals of the `carry_steady_window_ms` that end at the last learning (ruling T20-decay2: a gap does not age them; the
    /// grown variance does the decaying).
    fn recent(&self, p: &LocParams) -> impl Iterator<Item = f64> + '_ {
        let (window, last) = (p.carry_steady_window_ms, self.last_ms.unwrap_or(i64::MIN));
        self.resid.iter().filter(move |(t, _)| last - t <= window).map(|(_, r)| *r)
    }

    /// Mean resultant length of the residuals of the `carry_steady_window_ms` that end at the last learning (1 = perfectly steady); 0
    /// with too few.
    #[must_use]
    pub fn steadiness(&self, p: &LocParams) -> f64 {
        let (n, s, c) = self.recent(p).fold((0, 0.0, 0.0), |(n, s, c), r| (n + 1, s + r.to_radians().sin(), c + r.to_radians().cos()));
        if n < MIN_STEADY_RESIDUALS {
            return 0.0;
        }
        s.hypot(c) / count_f64(n)
    }

    /// `clamp(1 - sigma / 45 deg, 0, 1)` at `now_ms`, the sigma grown by the time since the last learning (review I3: about two
    /// minutes above 0.5 at the defaults), and 0 while the offset is not steady or has too few residuals.
    #[must_use]
    pub fn confidence(&self, p: &LocParams, now_ms: i64) -> f64 {
        if self.recent(p).count() < p.carry_min_residuals || self.steadiness(p) < p.carry_min_steadiness {
            return 0.0;
        }
        (1.0 - self.grown_var(now_ms, p).sqrt() / 45.0).clamp(0.0, 1.0)
    }

    /// The phone moved: start learning again (from `carry_reset_var_deg2`).
    pub fn reset(&mut self, p: &LocParams) {
        *self = Self { last_ms: self.last_ms, var_deg2: p.carry_reset_var_deg2, ..Self::default() };
    }

    /// Learn from one good GPS course at `now_ms` (the fix's time) while the compass reading nearest that time is reliable and the
    /// compass steady around it (turning slower than `carry_max_rate_deg_s` over the compass window). No tilt rule: in a pocket the
    /// phone is upright. True when it learned. The carry changed, and the estimator restarts at the new offset, after
    /// `carry_change_ms` of innovations beyond `carry_change_sigmas`, or of at least three innovations of `carry_change_min_deg` or
    /// more all on one side (ruling T20-change: a hand-to-pocket change can stay inside 3 sigma). A pause in learning longer than
    /// `carry_change_ms` + 5 s forgets a pending change (review M1).
    pub fn learn(&mut self, course_deg: f64, course_sigma_deg: f64, compass: &Compass, now_ms: i64, p: &LocParams) -> bool {
        let Some(h) = compass.nearest(now_ms, p.carry_compass_fresh_ms) else { return false };
        let Some(cs) = h.sigma_deg() else { return false };
        // The window ends at the reading, so a 1 Hz batched compass still has two readings in it.
        if compass.rate_deg_s(h.t_ms, p.compass_window_ms).is_none_or(|r| r >= p.carry_max_rate_deg_s) {
            return false;
        }
        if self.last_ms.is_some_and(|t| now_ms.saturating_sub(t) > p.carry_change_ms + CHANGE_EXPIRY_MS) {
            self.over_since = None;
            self.run.clear();
        }
        self.var_deg2 = self.grown_var(now_ms, p);
        self.last_ms = Some(now_ms);
        let r = course_sigma_deg * course_sigma_deg + cs * cs;
        let raw = wrap_deg(course_deg - h.azimuth_deg);
        let innov = wrap_deg(raw - self.delta_deg);
        let outlier = innov.abs() > p.carry_change_sigmas * (self.var_deg2 + r).sqrt();
        let over_long = outlier && now_ms.saturating_sub(*self.over_since.get_or_insert(now_ms)) >= p.carry_change_ms;
        if !outlier {
            self.over_since = None;
        }
        if over_long || self.extend_run(now_ms, innov, p) {
            self.reset(p);
            self.delta_deg = raw;
            return true;
        }
        if outlier {
            return true;
        }
        let k = self.var_deg2 / (self.var_deg2 + r);
        self.delta_deg = wrap_deg(self.delta_deg + k * innov);
        self.var_deg2 *= 1.0 - k;
        self.resid.push_back((now_ms, innov));
        while self.resid.front().is_some_and(|(t, _)| now_ms.saturating_sub(*t) > p.carry_steady_window_ms) {
            self.resid.pop_front();
        }
        true
    }

    /// Add `innov` to the one-sided run (or start a new one); true when the run is a carry change: at least three innovations spanning
    /// `carry_change_ms`.
    fn extend_run(&mut self, now_ms: i64, innov: f64, p: &LocParams) -> bool {
        if innov.abs() < p.carry_change_min_deg {
            self.run.clear();
            return false;
        }
        if self.run.back().is_some_and(|(_, r)| r.signum() != innov.signum()) {
            self.run.clear();
        }
        self.run.push_back((now_ms, innov));
        let span = self.run.back().zip(self.run.front()).map_or(0, |((last, _), (first, _))| last - first);
        self.run.len() >= MIN_CHANGE_RUN && span >= p.carry_change_ms
    }

    /// During a gap: the azimuth spreading over more than `carry_jump_deg` within the compass window ([`Compass::range_deg`], review
    /// M3) means the phone moved in the pocket, unless something else `explained` it (the cadence changed, or the walker may be turning
    /// at a crossing, Task 22). True when it reset the offset.
    pub fn watch_gap(&mut self, compass: &Compass, now_ms: i64, explained: bool, p: &LocParams) -> bool {
        let jumped = compass.range_deg(now_ms, p.compass_window_ms).is_some_and(|d| d > p.carry_jump_deg);
        if jumped && !explained {
            self.reset(p);
        }
        jumped && !explained
    }

    /// The bearing (in [0, 360)) and sigma, degrees, to bridge a gap with: compass plus offset when the offset is confident at `now_ms`
    /// (its sigma grown by the time since it last learned), or the raw compass when `carry_enabled` is false; else the last GPS course with a sigma
    /// growing `carry_gap_sigma_per_s` from `carry_gap_sigma0_deg` (at most `carry_gap_sigma_max_deg`); `None` with neither.
    #[must_use]
    pub fn heading_for_gap(&self, compass: &Compass, now_ms: i64, last_course_deg: Option<f64>, gap_s: f64, p: &LocParams) -> Option<(f64, f64)> {
        let usable = !p.carry_enabled || self.confidence(p, now_ms) >= p.carry_min_confidence;
        let reading = compass.nearest(now_ms, p.carry_compass_fresh_ms).filter(|_| usable);
        if let Some((h, cs)) = reading.and_then(|h| h.sigma_deg().map(|cs| (h, cs))) {
            let (delta, var) = if p.carry_enabled { (self.delta_deg, self.grown_var(now_ms, p)) } else { (0.0, 0.0) };
            return Some(((h.azimuth_deg + delta).rem_euclid(360.0), (var + cs * cs).sqrt()));
        }
        last_course_deg.map(|c| (c, gap_course_sigma_deg(gap_s, p)))
    }

    /// The variance grown by the process noise since the last learning, capped at the reset variance.
    fn grown_var(&self, now_ms: i64, p: &LocParams) -> f64 {
        let dt_s = self.last_ms.map_or(0.0, |t| (i64_to_f64(now_ms.saturating_sub(t)) / 1000.0).max(0.0));
        (self.var_deg2 + p.carry_q_deg2_per_s * dt_s).min(p.carry_reset_var_deg2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loc::CompassAccuracy;

    fn h(t_ms: i64, az: f64, acc: CompassAccuracy, pitch: f64) -> HeadingIn {
        HeadingIn { t_ms, azimuth_deg: az, accuracy: acc, pitch_deg: pitch, roll_deg: 0.0, error_deg: None }
    }

    #[test]
    fn angles_wrap_and_average_across_north() {
        assert!((wrap_deg(190.0) + 170.0).abs() < 1e-9 && (wrap_deg(-190.0) - 170.0).abs() < 1e-9);
        let m = circular_mean_deg(&[350.0, 10.0]).unwrap();
        assert!(m < 1e-9 || (m - 360.0).abs() < 1e-9, "{m}");
        assert!(circular_mean_deg(&[0.0, 180.0]).is_none(), "no mean direction");
        assert!((tilt_deg(0.0, 0.0)).abs() < 1e-9 && (tilt_deg(60.0, 0.0) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn the_compass_counts_only_when_held_flat_steady_and_accurate() {
        let p = LocParams::default();
        let mut c = Compass::default();
        for i in 0..5 {
            c.push(h(1000 + i * 500, 90.0 + f64::from(u8::try_from(i).unwrap()), CompassAccuracy::High, 10.0));
        }
        assert!((c.held_flat_and_steady(3000, &p).unwrap() - 92.0).abs() < 1.0);
        assert!(c.held_flat_and_steady(5000, &p).is_none(), "stale reading");
        let mut tilted = Compass::default();
        (0..5).for_each(|i| tilted.push(h(1000 + i * 500, 90.0, CompassAccuracy::High, 80.0)));
        assert!(tilted.held_flat_and_steady(3000, &p).is_none(), "phone upright in a pocket");
        let mut swinging = Compass::default();
        (0..5).for_each(|i| swinging.push(h(1000 + i * 500, if i % 2 == 0 { 60.0 } else { 120.0 }, CompassAccuracy::High, 10.0)));
        assert!(swinging.held_flat_and_steady(3000, &p).is_none(), "spread over 10 degrees");
        let mut low = Compass::default();
        (0..5).for_each(|i| low.push(h(1000 + i * 500, 90.0, CompassAccuracy::Low, 10.0)));
        assert!(low.held_flat_and_steady(3000, &p).is_none(), "accuracy below medium");
    }

    #[test]
    fn the_azimuth_rate_is_degrees_per_second() {
        let mut c = Compass::default();
        (0..5).for_each(|i| c.push(h(i * 500, f64::from(u8::try_from(i).unwrap()) * 20.0, CompassAccuracy::High, 0.0)));
        assert!((c.rate_deg_s(2000, 2000).unwrap() - 40.0).abs() < 1e-6);
    }

    #[test]
    fn the_mean_direction_is_always_in_zero_to_360() {
        for angles in [[359.0, 1.0], [1.0, 359.0], [350.0, 10.0], [0.0, 0.0]] {
            let m = circular_mean_deg(&angles).unwrap();
            assert!((0.0..360.0).contains(&m) && wrap_deg(m).abs() < 1e-9, "{angles:?}: {m}");
        }
    }

    #[test]
    fn a_compass_steady_across_north_counts() {
        let p = LocParams::default();
        let mut c = Compass::default();
        for (i, az) in [355.0, 5.0, 358.0, 2.0].into_iter().enumerate() {
            c.push(h(1000 + i64::try_from(i).unwrap() * 500, az, CompassAccuracy::High, 10.0));
        }
        let spread = c.spread_deg(2500, p.compass_window_ms).unwrap();
        assert!((spread - 5.0).abs() < 0.1, "spread across north: {spread}");
        let m = c.held_flat_and_steady(2500, &p).unwrap();
        assert!((0.0..360.0).contains(&m) && wrap_deg(m).abs() < 0.1, "{m}");
    }

    #[test]
    fn old_and_repeated_readings_are_dropped_and_twenty_seconds_are_kept() {
        let mut c = Compass::default();
        c.push(h(5_000, 10.0, CompassAccuracy::High, 0.0));
        c.push(h(5_000, 99.0, CompassAccuracy::High, 0.0));
        c.push(h(4_000, 99.0, CompassAccuracy::High, 0.0));
        assert_eq!(c.latest(5_000, 1_000).map(|x| x.azimuth_deg), Some(10.0));
        c.push(h(30_000, 20.0, CompassAccuracy::High, 0.0));
        assert_eq!(c.recent.len(), 1, "readings over 20 s older than the newest are forgotten");
        assert!(c.latest(31_001, 1_000).is_none() && c.spread_deg(60_000, 2_000).is_none() && c.rate_deg_s(30_000, 2_000).is_none());
        assert!((wrap_deg(180.0) - 180.0).abs() < 1e-9 && (wrap_deg(-180.0) - 180.0).abs() < 1e-9, "(-180, 180]");
    }

    fn steady(az: f64, t_ms: i64) -> Compass {
        let mut c = Compass::default();
        (0..5).for_each(|i| c.push(h(t_ms - 2000 + i * 500, az, CompassAccuracy::High, 80.0)));
        c
    }

    /// [`steady`] with the phone's own heading error on every reading (the band stays high).
    fn steady_with_error(az: f64, t_ms: i64, error_deg: f64) -> Compass {
        let mut c = Compass::default();
        (0..5).for_each(|i| c.push(HeadingIn { error_deg: Some(error_deg), ..h(t_ms - 2000 + i * 500, az, CompassAccuracy::High, 80.0) }));
        c
    }

    #[test]
    fn the_phones_heading_error_decides_the_arrow_when_given() {
        let p = LocParams::default();
        let flat = |acc, error_deg| {
            let mut c = Compass::default();
            (0..5).for_each(|i| c.push(HeadingIn { error_deg, ..h(1000 + i * 500, 90.0, acc, 10.0) }));
            c.held_flat_and_steady(3000, &p)
        };
        assert!(flat(CompassAccuracy::Low, Some(40.0)).is_some(), "error 40 (sigma 20) is accurate on a low band");
        assert!(flat(CompassAccuracy::High, Some(60.0)).is_some(), "sigma 30, as good as medium");
        assert!(flat(CompassAccuracy::High, Some(70.0)).is_none(), "sigma 35, worse than medium");
        assert!(flat(CompassAccuracy::High, Some(180.0)).is_none(), "no idea");
        assert!(flat(CompassAccuracy::Medium, None).is_some() && flat(CompassAccuracy::Low, None).is_none(), "no error given: the band");
    }

    #[test]
    fn the_carry_takes_the_compass_sigma_from_the_phones_heading_error() {
        let off = LocParams { carry_enabled: false, ..LocParams::default() };
        let k = CarryOffset::default();
        assert_eq!(k.heading_for_gap(&steady_with_error(10.0, 80_000, 20.0), 80_000, Some(0.0), 5.0, &off), Some((10.0, 10.0)));
        assert_eq!(k.heading_for_gap(&steady_with_error(10.0, 80_000, 180.0), 80_000, Some(0.0), 5.0, &off), Some((0.0, 20.0)), "no idea: the course");
        let p = LocParams::default();
        let learned = |error_deg| {
            let mut k = CarryOffset::default();
            let ok = k.learn(90.0, 5.0, &steady_with_error(0.0, 10_000, error_deg), 10_000, &p);
            (ok, k.sigma_deg())
        };
        assert!(!learned(180.0).0, "a compass with no idea is not learned");
        let ((sharp_ok, sharp), (vague_ok, vague)) = (learned(10.0), learned(100.0));
        assert!(sharp_ok && vague_ok && sharp < vague, "a smaller error learns more: {sharp} vs {vague}");
    }

    #[test]
    fn a_pocket_offset_is_learned_from_good_courses() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            let t = 10_000 + s * 1000;
            assert!(k.learn(90.0, 5.0, &steady(0.0, t), t, &p), "steady compass, good course");
        }
        assert!(wrap_deg(k.delta_deg() - 90.0).abs() < 5.0, "{}", k.delta_deg());
        assert!(k.confidence(&p, 69_000) >= 0.5, "{}", k.confidence(&p, 69_000));
        let (theta, sigma) = k.heading_for_gap(&steady(10.0, 80_000), 80_000, Some(0.0), 5.0, &p).unwrap();
        assert!(wrap_deg(theta - 100.0).abs() < 5.0 && sigma < 45.0, "compass + offset: {theta} {sigma}");
    }

    #[test]
    fn a_swinging_or_unreliable_compass_is_not_learned() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        let mut swing = Compass::default();
        (0..5).for_each(|i| swing.push(h(8_000 + i * 500, f64::from(u8::try_from(i).unwrap()) * 40.0, CompassAccuracy::High, 0.0)));
        assert!(!k.learn(90.0, 5.0, &swing, 10_000, &p), "80 deg/s");
        let mut bad = Compass::default();
        (0..5).for_each(|i| bad.push(h(8_000 + i * 500, 0.0, CompassAccuracy::Unreliable, 0.0)));
        assert!(!k.learn(90.0, 5.0, &bad, 10_000, &p));
        assert_eq!(k.confidence(&p, 10_000), 0.0);
    }

    #[test]
    fn a_carry_change_while_learning_resets_the_confidence() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        for s in 60..64 {
            k.learn(90.0, 5.0, &steady(90.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
            // the phone moved: offset now 0
        }
        assert!(k.confidence(&p, 73_000) < 0.5, "{}", k.confidence(&p, 73_000));
    }

    #[test]
    fn a_compass_jump_in_a_gap_resets_unless_the_cadence_changed_too() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        let mut jump = Compass::default();
        // Review I3: the gap starts within the steadiness window of the last learning (69 s), or the offset is no longer confident.
        jump.push(h(80_000, 0.0, CompassAccuracy::High, 80.0));
        jump.push(h(81_500, 70.0, CompassAccuracy::High, 80.0));
        let mut kept = k.clone();
        kept.watch_gap(&jump, 81_500, true, &p);
        assert!(kept.confidence(&p, 81_500) >= 0.5, "a cadence change explains it (turning a corner)");
        k.watch_gap(&jump, 81_500, false, &p);
        assert_eq!(k.confidence(&p, 81_500), 0.0);
    }

    #[test]
    fn without_confidence_the_gap_heading_is_the_last_course_with_a_growing_sigma() {
        let p = LocParams::default();
        let k = CarryOffset::default();
        assert_eq!(k.heading_for_gap(&Compass::default(), 0, Some(45.0), 10.0, &p), Some((45.0, 30.0)));
        assert_eq!(k.heading_for_gap(&Compass::default(), 0, Some(45.0), 100.0, &p), Some((45.0, 90.0)));
        assert_eq!(k.heading_for_gap(&Compass::default(), 0, None, 1.0, &p), None);
    }

    /// A reading every whole second from `from_ms` to `to_ms` (the screen-off rate, ruling T15-compass-rate).
    fn one_hz(c: &mut Compass, from_ms: i64, to_ms: i64, az: f64) {
        (from_ms / 1000..=to_ms / 1000).for_each(|s| c.push(h(s * 1000, az, CompassAccuracy::High, 80.0)));
    }

    #[test]
    fn a_pocket_offset_is_learned_with_the_screen_off() {
        // Controller note 1: in a pocket the compass runs at 1 Hz, batched up to a second, and GPS fixes come every ~5 s.
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        let mut c = Compass::default();
        let mut learned = 0;
        for n in 0..40 {
            let t = 10_000 + n * 5_100;
            one_hz(&mut c, t - 6_000, t - 1_200, 270.0); // the newest reading is 1.2 to 2.2 s old when the fix arrives
            learned += i32::from(k.learn(0.0, 8.0, &c, t, &p));
        }
        assert!(learned >= 38, "a 1 Hz batched compass still learns: {learned}");
        assert!(wrap_deg(k.delta_deg() - 90.0).abs() < 5.0, "{}", k.delta_deg());
        let last = 10_000 + 39 * 5_100;
        assert!(k.confidence(&p, last) >= 0.5, "fixes every 5 s still give enough residuals: {}", k.confidence(&p, last));
        let t = 10_000 + 40 * 5_100;
        one_hz(&mut c, t - 3_000, t - 1_500, 270.0);
        let (theta, _) = k.heading_for_gap(&c, t, Some(180.0), 3.0, &p).unwrap();
        assert!(wrap_deg(theta).abs() < 5.0, "the 1 Hz compass bridges the gap: {theta}");
    }

    #[test]
    fn a_batched_fix_learns_from_the_reading_nearest_its_time() {
        // Review Focus 1: screen-off fixes arrive in batches, after newer compass readings.
        let p = LocParams::default();
        let mut c = Compass::default();
        one_hz(&mut c, 0, 10_000, 0.0);
        one_hz(&mut c, 11_000, 20_000, 45.0);
        let mut k = CarryOffset::default();
        assert!(k.learn(90.0, 5.0, &c, 5_000, &p));
        assert!(wrap_deg(k.delta_deg() - 90.0).abs() < 5.0, "the reading at the fix's time, not the newest: {}", k.delta_deg());
    }

    #[test]
    fn a_one_second_turn_under_the_jump_limit_keeps_the_offset() {
        // At 1 Hz a 30 degree turn between two readings is a turn, not a 60 degree "jump" over the 2 s window.
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        let mut c = Compass::default();
        c.push(h(80_000, 0.0, CompassAccuracy::High, 80.0));
        c.push(h(81_000, 30.0, CompassAccuracy::High, 80.0));
        k.watch_gap(&c, 81_000, false, &p);
        assert!(k.confidence(&p, 81_000) >= 0.5, "{}", k.confidence(&p, 81_000));
    }

    #[test]
    fn the_gap_sigma_grows_with_the_time_since_learning() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        let soon = k.heading_for_gap(&steady(0.0, 70_000), 70_000, None, 1.0, &p).unwrap().1;
        let later = k.heading_for_gap(&steady(0.0, 90_000), 90_000, None, 1.0, &p).unwrap().1;
        assert!(soon < 20.0 && later > soon + 1.0, "{soon} {later}");
        k.reset(&p);
        assert_eq!((k.confidence(&p, 70_000), k.sigma_deg()), (0.0, 90.0), "a reset forgets the offset");
        let q = LocParams { carry_reset_var_deg2: 3600.0, ..p.clone() };
        k.reset(&q);
        assert!((k.sigma_deg() - 60.0).abs() < 1e-9, "review R3: the reset variance is the param's");
    }

    #[test]
    fn a_learned_offset_decays_to_the_last_course() {
        // Rulings I3 and T20-decay2: the confidence decays with the grown variance alone (the residuals are judged at the last
        // learning): still usable 90 s on, the last course ten minutes on.
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        assert!(k.confidence(&p, 69_000) >= 0.5);
        assert_eq!(k.confidence(&p, 669_000), 0.0);
        assert_eq!(k.heading_for_gap(&steady(0.0, 669_000), 669_000, Some(45.0), 600.0, &p), Some((45.0, 90.0)));
        assert!(k.confidence(&p, 159_000) >= 0.5, "90 s on: {}", k.confidence(&p, 159_000));
        let (theta, _) = k.heading_for_gap(&steady(0.0, 159_000), 159_000, Some(45.0), 90.0, &p).unwrap();
        assert!(wrap_deg(theta - 90.0).abs() < 5.0, "compass plus offset 90 s on: {theta}");
        assert!(k.confidence(&p, 69_000 + 150_000) < 0.5, "about two minutes at most: {}", k.confidence(&p, 219_000));
    }

    #[test]
    fn with_the_carry_off_the_gap_heading_is_the_raw_compass() {
        // Controller note 5: `carry_enabled = false` trusts the raw compass (the bench comparison).
        let p = LocParams { carry_enabled: false, ..LocParams::default() };
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        assert_eq!(k.heading_for_gap(&steady(10.0, 80_000), 80_000, Some(0.0), 5.0, &p), Some((10.0, 15.0)));
        assert_eq!(k.heading_for_gap(&Compass::default(), 80_000, Some(0.0), 5.0, &p), Some((0.0, 20.0)), "no compass: the last course");
    }

    #[test]
    fn a_run_of_same_sided_residuals_is_a_carry_change() {
        // Ruling T20-change: a 60 degree change stays inside 3 sigma, but three or more residuals over 30 degrees on one side for
        // `carry_change_ms` restart the offset.
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        for s in 60..65 {
            k.learn(90.0, 25.0, &steady(60.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
            // a walking course sigma: 60 is inside 3 sigma
        }
        assert!(wrap_deg(k.delta_deg() - 30.0).abs() < 5.0, "re-learned at the new offset: {}", k.delta_deg());
        assert!(k.confidence(&p, 74_000) < 0.5, "{}", k.confidence(&p, 74_000));
    }

    #[test]
    fn residuals_alternating_sides_are_no_carry_change() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        for s in 60..70 {
            let az = if s % 2 == 0 { 35.0 } else { -35.0 };
            k.learn(90.0, 5.0, &steady(az, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        assert!(wrap_deg(k.delta_deg() - 90.0).abs() < 10.0 && k.sigma_deg() < 20.0, "{} {}", k.delta_deg(), k.sigma_deg());
    }

    #[test]
    fn an_old_outlier_is_forgotten_across_a_pause() {
        // Review M1: two outliers ten seconds apart (no learning between) are no carry change.
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        k.learn(90.0, 5.0, &steady(240.0, 70_000), 70_000, &p);
        k.learn(90.0, 5.0, &steady(240.0, 80_000), 80_000, &p);
        assert!(wrap_deg(k.delta_deg() - 90.0).abs() < 5.0, "{}", k.delta_deg());
        assert!(k.sigma_deg() < 20.0, "not reset: {}", k.sigma_deg());
    }

    #[test]
    fn a_swing_there_and_back_in_a_gap_is_a_jump() {
        // Review M3: the phone flipped in the pocket and back within the window; first-to-last sees no turn, the spread does.
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        let mut c = Compass::default();
        for (t, az) in [(80_000, 0.0), (80_500, 70.0), (81_000, 0.0)] {
            c.push(h(t, az, CompassAccuracy::High, 80.0));
        }
        assert!((c.range_deg(81_000, 2_000).unwrap() - 70.0).abs() < 1e-9);
        k.watch_gap(&c, 81_000, false, &p);
        assert_eq!(k.confidence(&p, 81_000), 0.0);
    }
}
