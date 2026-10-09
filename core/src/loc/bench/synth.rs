//! Synthetic walks: a seeded truth trajectory, GNSS error as AR(1) drift plus white noise, spikes, gaps, steps and compass readings.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::geo::{destination, Point};
use crate::loc::{CompassAccuracy, HeadingIn, Provider, RawFix, ACC_TO_SIGMA};
use crate::num::{i64_to_f64, round_i64};

/// One piece of the route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Leg {
    /// Move `dist_m` on a straight line at `bearing_deg`, at `speed_mps`.
    Move {
        /// Direction, degrees from north.
        bearing_deg: f64,
        /// Length, metres.
        dist_m: f64,
        /// Speed, m/s.
        speed_mps: f64,
    },
    /// Stand still.
    Stop {
        /// How long, seconds.
        secs: u32,
    },
}

/// Fixes from `at_s` on (`len` of them) are pushed `offset_m` away at `bearing_deg` (multipath, a network fix).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spike {
    /// Seconds after the start.
    pub at_s: u32,
    /// How far off, metres.
    pub offset_m: f64,
    /// Which way, degrees from north.
    pub bearing_deg: f64,
    /// How many fixes in a row.
    pub len: u32,
}

/// How the phone's compass relates to the walking direction.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadingSim {
    /// From these seconds on, the offset `course - azimuth`, degrees (a phone in a pocket points anywhere).
    pub offset: Vec<(u32, f64)>,
    /// Compass noise, one sigma, degrees.
    pub noise_deg: f64,
    /// Reported accuracy.
    pub accuracy: CompassAccuracy,
    /// Reported pitch and roll (0, 0 = flat in the hand).
    pub tilt_deg: (f64, f64),
}

impl HeadingSim {
    /// A phone in a pocket: upright (pitch 80), medium accuracy, 5 degrees of noise.
    #[must_use]
    pub fn pocket(offset: Vec<(u32, f64)>) -> Self {
        Self { offset, noise_deg: 5.0, accuracy: CompassAccuracy::Medium, tilt_deg: (80.0, 0.0) }
    }

    /// A phone held flat in the hand pointing where the player walks.
    #[must_use]
    pub fn in_hand() -> Self {
        Self { offset: vec![(0, 0.0)], noise_deg: 3.0, accuracy: CompassAccuracy::High, tilt_deg: (10.0, 0.0) }
    }
}

/// A synthetic outing.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    /// Start point.
    pub start: Point,
    /// Start time, Unix ms.
    pub t0_ms: i64,
    /// The route.
    pub legs: Vec<Leg>,
    /// Seconds between fixes.
    pub fix_every_s: u32,
    /// True 68 % accuracy, metres; or a range sampled per fix (`acc_spread_m` > 0: acc +- spread).
    pub acc_m: f64,
    /// Fix-to-fix spread of the true accuracy, metres (the BALANCED stress uses 15).
    pub acc_spread_m: f64,
    /// Reported accuracy jitter around the true one (0.3 = +-30 %).
    pub acc_jitter: f64,
    /// AR(1) time constant of the drift, seconds.
    pub ar_tau_s: f64,
    /// Spikes and bursts.
    pub spikes: Vec<Spike>,
    /// Seconds `[from, to)` with no fixes.
    pub gaps: Vec<(u32, u32)>,
    /// Whether the phone has a step counter.
    pub steps: bool,
    /// From these seconds on, the true step-length scale against the cadence model.
    pub step_scale: Vec<(u32, f64)>,
    /// Compass readings, if any.
    pub heading: Option<HeadingSim>,
    /// Whether fixes carry speed and bearing (with accuracies).
    pub with_velocity: bool,
    /// Provider of every fix.
    pub provider: Provider,
}

/// The truth at one second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TruthPoint {
    /// Time, Unix ms.
    pub t_ms: i64,
    /// Position.
    pub p: Point,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// Direction of travel (the last one while standing), degrees.
    pub course_deg: f64,
}

/// What a scenario produced for one seed.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// One truth point per second.
    pub truth: Vec<TruthPoint>,
    /// The truth plus the generated drift (the AR(1) part of the error), one point per second: what any filter of the fixes can at best
    /// see (ruling T8-R1). Speed and course are the truth's.
    pub observed: Vec<TruthPoint>,
    /// The fixes, in time order.
    pub fixes: Vec<RawFix>,
    /// Step counter readings `(t_ms, cumulative total)`: every 2 s while the count changes, none while it does not (as Android's counter).
    pub steps: Vec<(i64, i64)>,
    /// Compass readings at 2 Hz.
    pub headings: Vec<HeadingIn>,
}

/// One standard normal sample (Box-Muller; `rand_distr` is not a dependency).
pub fn gauss<R: RngExt + ?Sized>(rng: &mut R) -> f64 {
    let u1: f64 = rng.random::<f64>().max(f64::MIN_POSITIVE);
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// The truth at `t_ms` (the last point at or before it; the first one before the start).
#[must_use]
pub fn truth_at(truth: &[TruthPoint], t_ms: i64) -> TruthPoint {
    let i = truth.partition_point(|t| t.t_ms <= t_ms);
    truth[i.saturating_sub(1).min(truth.len() - 1)]
}

/// Cadence (steps/s) at which the cadence model `L = 0.25 + 0.25 f` (times `scale`) covers `speed` m/s: solves `f * L(f) * scale = v`.
#[must_use]
pub fn cadence_for(speed_mps: f64, scale: f64) -> f64 {
    let a = 0.25 * scale;
    (-a + (a * a + 4.0 * a * speed_mps).sqrt()) / (2.0 * a)
}

fn value_at(table: &[(u32, f64)], s: u32, default: f64) -> f64 {
    table.iter().rev().find(|(from, _)| *from <= s).map_or(default, |(_, v)| *v)
}

impl Scenario {
    /// A walk at 1 Hz with fixes of accuracy `acc_m`, no steps, no compass, no velocity in the fixes.
    #[must_use]
    pub fn walk(start: Point, legs: Vec<Leg>, acc_m: f64) -> Self {
        Self {
            start,
            t0_ms: 1_800_000_000_000,
            legs,
            fix_every_s: 1,
            acc_m,
            acc_spread_m: 0.0,
            acc_jitter: 0.3,
            ar_tau_s: 30.0,
            spikes: vec![],
            gaps: vec![],
            steps: false,
            step_scale: vec![(0, 1.0)],
            heading: None,
            with_velocity: false,
            provider: Provider::Fused,
        }
    }

    fn truth(&self) -> Vec<TruthPoint> {
        let mut out = vec![TruthPoint { t_ms: self.t0_ms, p: self.start, speed_mps: 0.0, course_deg: 0.0 }];
        let (mut p, mut course) = (self.start, 0.0);
        for leg in &self.legs {
            match *leg {
                Leg::Move { bearing_deg, dist_m, speed_mps } => {
                    course = bearing_deg;
                    let n = round_i64(dist_m / speed_mps).max(1);
                    let step = dist_m / i64_to_f64(n);
                    for _ in 0..n {
                        p = destination(p, bearing_deg, step);
                        let t = out.last().map_or(self.t0_ms, |l| l.t_ms) + 1000;
                        out.push(TruthPoint { t_ms: t, p, speed_mps, course_deg: course });
                    }
                }
                Leg::Stop { secs } => {
                    for _ in 0..secs {
                        let t = out.last().map_or(self.t0_ms, |l| l.t_ms) + 1000;
                        out.push(TruthPoint { t_ms: t, p, speed_mps: 0.0, course_deg: course });
                    }
                }
            }
        }
        out
    }

    /// Everything the phone would have delivered on this route, for one `seed`.
    #[must_use]
    #[allow(clippy::many_single_char_names)] // a, e, n, p, s, f: the AR(1) coefficient, east/north error, point, second, fix
    pub fn generate(&self, seed: u64) -> Run {
        let mut rng = StdRng::seed_from_u64(seed);
        let truth = self.truth();
        let a = (-1.0 / self.ar_tau_s).exp();
        let (mut ar_e, mut ar_n) = (0.0, 0.0);
        let mut fixes = Vec::new();
        let mut observed = Vec::with_capacity(truth.len());
        for (i, tp) in truth.iter().enumerate() {
            let s = u32::try_from(i).unwrap_or(u32::MAX);
            let true_acc = (self.acc_m + self.acc_spread_m * (2.0 * rng.random::<f64>() - 1.0)).max(1.0);
            let sigma = true_acc / ACC_TO_SIGMA;
            // AR(1) drift (64 % of the variance) plus white noise (36 %): the total per-axis sigma is the scenario's.
            ar_e = a * ar_e + (1.0 - a * a).sqrt() * 0.8 * sigma * gauss(&mut rng);
            ar_n = a * ar_n + (1.0 - a * a).sqrt() * 0.8 * sigma * gauss(&mut rng);
            let (we, wn) = (0.6 * sigma * gauss(&mut rng), 0.6 * sigma * gauss(&mut rng));
            observed.push(TruthPoint { p: destination(destination(tp.p, 90.0, ar_e), 0.0, ar_n), ..*tp });
            if s % self.fix_every_s.max(1) != 0 || self.gaps.iter().any(|(from, to)| (*from..*to).contains(&s)) {
                continue;
            }
            let (e, n) = (ar_e + we, ar_n + wn);
            let mut p = destination(destination(tp.p, 90.0, e), 0.0, n);
            if let Some(sp) = self.spikes.iter().find(|sp| (sp.at_s..sp.at_s + sp.len.max(1)).contains(&s)) {
                p = destination(p, sp.bearing_deg, sp.offset_m);
            }
            let reported = true_acc * (1.0 + self.acc_jitter * (2.0 * rng.random::<f64>() - 1.0));
            let mut f = RawFix { provider: self.provider, ..RawFix::at(p.lat, p.lon, tp.t_ms, reported) };
            if self.with_velocity {
                f.speed_mps = Some((tp.speed_mps + 0.3 * gauss(&mut rng)).max(0.0));
                f.speed_acc_mps = Some(0.5);
                f.bearing_deg = Some((tp.course_deg + 5.0 * gauss(&mut rng)).rem_euclid(360.0));
                f.bearing_acc_deg = Some(if tp.speed_mps > 0.5 { 10.0 } else { 90.0 });
            }
            fixes.push(f);
        }
        let steps = if self.steps { self.steps_of(&truth) } else { vec![] };
        let headings = self.heading.as_ref().map(|h| Self::headings_of(h, &truth, &mut rng)).unwrap_or_default();
        Run { truth, observed, fixes, steps, headings }
    }

    fn steps_of(&self, truth: &[TruthPoint]) -> Vec<(i64, i64)> {
        let mut total = 10_000.0_f64;
        let mut out = vec![(self.t0_ms, 10_000)];
        for (i, tp) in truth.iter().enumerate().skip(1) {
            let s = u32::try_from(i).unwrap_or(u32::MAX);
            let scale = value_at(&self.step_scale, s, 1.0);
            if tp.speed_mps > 0.0 {
                total += cadence_for(tp.speed_mps, scale);
            }
            // Android's step counter reports only when the count changes: a player standing still gets no events (final review N1).
            let n = round_i64(total.floor());
            if i % 2 == 0 && out.last().is_none_or(|(_, last)| *last != n) {
                out.push((tp.t_ms, n));
            }
        }
        out
    }

    fn headings_of(h: &HeadingSim, truth: &[TruthPoint], rng: &mut StdRng) -> Vec<HeadingIn> {
        let mut out = Vec::new();
        for (i, tp) in truth.iter().enumerate() {
            let s = u32::try_from(i).unwrap_or(u32::MAX);
            let offset = value_at(&h.offset, s, 0.0);
            for half in 0..2_i64 {
                let azimuth = (tp.course_deg - offset + h.noise_deg * gauss(rng)).rem_euclid(360.0);
                out.push(HeadingIn {
                    t_ms: tp.t_ms + half * 500,
                    azimuth_deg: azimuth,
                    accuracy: h.accuracy,
                    pitch_deg: h.tilt_deg.0,
                    roll_deg: h.tilt_deg.1,
                    error_deg: None,
                });
            }
        }
        out
    }
}

/// An `n` x `n` grid of streets `spacing_m` apart, east and north of `origin`: every crossing is a shared node, every class may use it.
#[must_use]
pub fn grid_ways(origin: Point, n: usize, spacing_m: f64) -> Vec<crate::scan::WayGeom> {
    use crate::scan::way_class::{BIKE, CAR, FOOT};
    let at = |i: usize, j: usize| {
        let p = destination(destination(origin, 90.0, spacing_m * crate::num::count_f64(i)), 0.0, spacing_m * crate::num::count_f64(j));
        Point::new((p.lat * 1e7).round() / 1e7, (p.lon * 1e7).round() / 1e7)
    };
    let mut out = Vec::new();
    for k in 0..n {
        let id = i64::try_from(k).unwrap_or(0);
        out.push(crate::scan::WayGeom { id: 2 * id, class: FOOT | BIKE | CAR, pts: (0..n).map(|i| at(i, k)).collect() });
        out.push(crate::scan::WayGeom { id: 2 * id + 1, class: FOOT | BIKE | CAR, pts: (0..n).map(|j| at(k, j)).collect() });
    }
    out
}
