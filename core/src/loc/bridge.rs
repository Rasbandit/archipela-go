//! Layer 3: a particle filter that keeps the position going through a GPS gap from steps and heading, on the street graph (Walk and Run).
//! Its positions feed the map, fog and Cartographer squares only.

use std::sync::Arc;

use rand::rngs::Xoshiro256PlusPlus;
use rand::{RngExt, SeedableRng};

use crate::geo::Point;
use crate::loc::bench::gauss;
use crate::loc::frame::Frame;
use crate::loc::graph::StreetGraph;
use crate::loc::heading::wrap_deg;
use crate::loc::mat::Mat;
use crate::loc::{LocParams, ACC_TO_SIGMA};
use crate::num::{count_f64, floor_i64, floor_usize};

/// The bearing the bridge steers by, and its sigma, degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Steer {
    /// Bearing, degrees from north.
    pub theta_deg: f64,
    /// Sigma, degrees.
    pub sigma_deg: f64,
}

/// One hypothesis of where the walker is: on a street segment (`seg`, `off` metres from its node `a`, walking `dir` +1 towards `b` or -1
/// back) or off the network (`seg` `None`, position `en` only), with its own step scale `s` and heading bias `b_deg`.
#[derive(Debug, Clone, Copy)]
struct Particle {
    seg: Option<usize>,
    off: f64,
    dir: f64,
    en: [f64; 2],
    s: f64,
    b_deg: f64,
    course_deg: f64,
    w: f64,
}

/// The particle cloud of one GPS gap.
#[derive(Debug, Clone)]
pub struct Bridge {
    graph: Option<Arc<StreetGraph>>,
    frame: Frame,
    mask: u8,
    ps: Vec<Particle>,
    /// The cluster radius of [`Self::estimate`], metres (`bridge_cluster_m`).
    cluster_m: f64,
    /// Seeded per gap from `bridge_seed` and [`Self::started_ms`]: a replay draws the same cloud. Xoshiro, since `StdRng` is not `Clone`.
    rng: Xoshiro256PlusPlus,
    /// When the gap started being bridged, Unix ms.
    pub started_ms: i64,
    /// The last step batch, Unix ms.
    pub last_step_ms: i64,
}

/// At most this many mean-shift steps find the cluster's centre (it settles to 0.1 m in a few).
const MEAN_SHIFT_STEPS: usize = 8;

/// Nodes one step batch may cross: far more than any walked distance needs (1 km over 1 m segments).
const MAX_HOPS: usize = 1000;

fn heading_weight(d_deg: f64, sigma_deg: f64) -> f64 {
    (-0.5 * (d_deg / sigma_deg.max(1.0)).powi(2)).exp()
}

/// The weighted mean course of the heaviest direction among `ps`, degrees: particles within 45 degrees of the heaviest 30 degree bin, so a
/// cluster that splits at a crossing reports one street's bearing, not the average of both. `None` without weight.
fn dominant_course<'a>(ps: impl Iterator<Item = &'a Particle>) -> Option<f64> {
    let ps: Vec<&Particle> = ps.collect();
    let mut bins = [0.0; 12];
    for x in &ps {
        bins[floor_usize(x.course_deg.rem_euclid(360.0) / 30.0).min(11)] += x.w;
    }
    let (best, w) = bins.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    if *w <= 0.0 {
        return None;
    }
    let center = count_f64(best) * 30.0 + 15.0;
    let (mut se, mut sn) = (0.0, 0.0);
    for x in ps.iter().filter(|x| wrap_deg(x.course_deg - center).abs() <= 45.0) {
        let (sin, cos) = x.course_deg.to_radians().sin_cos();
        (se, sn) = (se + x.w * sin, sn + x.w * cos);
    }
    (se.hypot(sn) > 1e-12).then(|| se.atan2(sn).to_degrees().rem_euclid(360.0))
}

/// Whether a street particle walks its segment forwards (bearing `fwd_deg`): the way of `course_deg`, or either way at random when there is
/// none or the street is within `bridge_split_deg` of perpendicular to it (review I4).
fn forward_on(fwd_deg: f64, course_deg: Option<f64>, p: &LocParams, rng: &mut Xoshiro256PlusPlus) -> bool {
    match course_deg.map(|course| wrap_deg(fwd_deg - course).abs()) {
        Some(d) if (d - 90.0).abs() >= p.bridge_split_deg => d <= 90.0,
        _ => rng.random::<bool>(),
    }
}

impl Bridge {
    /// Start a cloud from the filter's `N(mean, cov)` (`cov` in m^2, east/north): 90 % projected onto streets within 3 sigma (preferring the
    /// matcher's confident segment `prefer`), 10 % off-network; each particle draws a step scale `N(1, sigma_k)` and a heading bias `N(0, 10)`.
    /// A street particle walks the way of `course_deg`, or either way at random when there is none or the street is within
    /// `bridge_split_deg` of perpendicular to it (review I4). Without a graph every particle is off-network. `t_ms` is when bridging starts: [`Self::started_ms`], [`Self::last_step_ms`] and the
    /// seed.
    #[allow(clippy::too_many_arguments)] // the gap's starting facts
    #[must_use]
    pub fn start(
        graph: Option<Arc<StreetGraph>>,
        frame: Frame,
        mask: u8,
        mean: Point,
        cov: Mat<2, 2>,
        sigma_k: f64,
        prefer: Option<(usize, f64)>,
        course_deg: Option<f64>,
        t_ms: i64,
        p: &LocParams,
    ) -> Self {
        let frame = graph.as_ref().map_or(frame, |g| *g.frame());
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(p.bridge_seed ^ t_ms.unsigned_abs());
        let m = frame.to_enu(mean);
        // Cholesky factor of the position covariance.
        let l00 = cov[0][0].max(1e-6).sqrt();
        let l10 = cov[1][0] / l00;
        let l11 = (cov[1][1] - l10 * l10).max(1e-6).sqrt();
        let sigma = f64::midpoint(cov[0][0], cov[1][1]).max(1.0).sqrt();
        let n = p.bridge_particles.max(10);
        let n_off = floor_usize(count_f64(n) * p.bridge_off_share);
        let mut ps = Vec::with_capacity(n);
        for i in 0..n {
            let (z0, z1) = (gauss(&mut rng), gauss(&mut rng));
            let en = [m[0] + l00 * z0, m[1] + l10 * z0 + l11 * z1];
            let mut part = Particle {
                seg: None,
                off: 0.0,
                dir: 1.0,
                en,
                s: (1.0 + sigma_k.max(p.bridge_sigma_k_min) * gauss(&mut rng)).max(0.3),
                b_deg: p.bridge_bias_sigma_deg * gauss(&mut rng),
                course_deg: course_deg.unwrap_or(0.0),
                w: 1.0,
            };
            if let (Some(g), true) = (graph.as_ref(), i >= n_off) {
                let cands = g.candidates(frame.to_geo(en), 3.0 * sigma, 8, mask);
                let pick = prefer.filter(|_| i % 2 == 0).and_then(|(seg, _)| cands.iter().find(|c| c.seg == seg)).or_else(|| cands.first());
                if let Some(c) = pick {
                    let forward = forward_on(g.bearing_deg(c.seg, 1.0), course_deg, p, &mut rng);
                    part.seg = Some(c.seg);
                    part.off = c.off_m;
                    part.dir = if forward { 1.0 } else { -1.0 };
                    part.en = g.en_at(c.seg, c.off_m);
                    part.course_deg = g.bearing_deg(c.seg, part.dir);
                }
            }
            ps.push(part);
        }
        let mut b = Self { graph, frame, mask, ps, cluster_m: p.bridge_cluster_m.max(1.0), rng, started_ms: t_ms, last_step_ms: t_ms };
        b.normalize();
        b
    }

    /// Move every particle `dist_m` (times its own scale): along its street, choosing at each node by heading; off-network along the heading.
    /// Weights follow the heading likelihood (off-network ones at `bridge_off_weight`); resample when `N_eff < N/2`.
    pub fn step(&mut self, dist_m: f64, steer: Steer, p: &LocParams) {
        let graph = self.graph.clone();
        for i in 0..self.ps.len() {
            let mut part = self.ps[i];
            let d = dist_m * part.s;
            if let (Some(_), Some(g)) = (part.seg, graph.as_ref()) {
                self.walk_graph(g, &mut part, d, steer);
                part.w *= heading_weight(wrap_deg(part.course_deg - steer.theta_deg), steer.sigma_deg);
            } else {
                let th = steer.theta_deg + part.b_deg + 5.0 * gauss(&mut self.rng);
                let (sin, cos) = th.to_radians().sin_cos();
                part.en = [part.en[0] + d * sin, part.en[1] + d * cos];
                part.course_deg = th.rem_euclid(360.0);
                part.w *= p.bridge_off_weight;
            }
            self.ps[i] = part;
        }
        self.normalize();
        let n_eff = 1.0 / self.ps.iter().map(|x| x.w * x.w).sum::<f64>();
        if n_eff < count_f64(self.ps.len()) / 2.0 {
            self.resample(p);
        }
    }

    /// Walk `part` `d` metres along its street, choosing at each node by `steer` (review M7: the whole distance, however many short
    /// segments it crosses; [`MAX_HOPS`] only guards against a graph that would loop without moving).
    fn walk_graph(&mut self, g: &StreetGraph, part: &mut Particle, d: f64, steer: Steer) {
        let mut left = d;
        for hop in 0.. {
            if hop == MAX_HOPS {
                debug_assert!(hop < MAX_HOPS, "{d} m crossed {MAX_HOPS} nodes");
                break;
            }
            let Some(seg) = part.seg else { return };
            let len = g.seg(seg).len_m;
            let room = if part.dir > 0.0 { len - part.off } else { part.off };
            if left <= room {
                part.off += part.dir * left;
                break;
            }
            left -= room;
            let node = if part.dir > 0.0 { g.seg(seg).b } else { g.seg(seg).a };
            let options: Vec<(usize, f64)> = g.leaving(node, self.mask).into_iter().filter(|(s, _)| *s != seg).collect();
            let options = if options.is_empty() { vec![(seg, -part.dir)] } else { options }; // dead end: turn back
            let target = steer.theta_deg + part.b_deg;
            let weights: Vec<f64> = options.iter().map(|(s, dir)| heading_weight(wrap_deg(g.bearing_deg(*s, *dir) - target), steer.sigma_deg) + 1e-9).collect();
            let total: f64 = weights.iter().sum();
            let mut u = self.rng.random::<f64>() * total;
            let mut pick = options[0];
            for (o, w) in options.iter().zip(&weights) {
                u -= w;
                if u <= 0.0 {
                    pick = *o;
                    break;
                }
            }
            part.seg = Some(pick.0);
            part.dir = pick.1;
            part.off = if pick.1 > 0.0 { 0.0 } else { g.seg(pick.0).len_m };
        }
        if let Some(seg) = part.seg {
            part.en = g.en_at(seg, part.off);
            part.course_deg = g.bearing_deg(seg, part.dir);
        }
    }

    fn normalize(&mut self) {
        let total: f64 = self.ps.iter().map(|x| x.w).sum();
        let n = count_f64(self.ps.len());
        for x in &mut self.ps {
            x.w = if total > 0.0 && total.is_finite() { x.w / total } else { 1.0 / n };
        }
    }

    /// Systematic resampling, roughening street particles `bridge_roughen_m` along their street.
    fn resample(&mut self, p: &LocParams) {
        let n = self.ps.len();
        let step = 1.0 / count_f64(n);
        let mut u = self.rng.random::<f64>() * step;
        let (mut cum, mut at) = (self.ps[0].w, 0);
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            while u > cum && at + 1 < n {
                at += 1;
                cum += self.ps[at].w;
            }
            let mut q = self.ps[at];
            q.w = step;
            if let (Some(seg), Some(g)) = (q.seg, self.graph.as_ref()) {
                q.off = (q.off + p.bridge_roughen_m * gauss(&mut self.rng)).clamp(0.0, g.seg(seg).len_m);
                q.en = g.en_at(seg, q.off);
            }
            out.push(q);
            u += step;
        }
        // Keep `bridge_off_share` of the cloud off the network (review M6): off-network particles weigh less every batch, and without a
        // floor a walk off the mapped streets could never be followed. The newest copies leave their streets where they stand.
        let floor = if self.graph.is_some() { floor_usize(count_f64(n) * p.bridge_off_share) } else { 0 };
        let mut need = floor.saturating_sub(out.iter().filter(|x| x.seg.is_none()).count());
        if need > 0 {
            let stride = (n / floor.max(1)).max(1);
            for q in out.iter_mut().step_by(stride).filter(|x| x.seg.is_some()) {
                if need == 0 {
                    break;
                }
                q.seg = None;
                need -= 1;
            }
        }
        self.ps = out;
    }

    /// The heaviest cluster's centre (review I3: found in space, wherever the street's segments start and end), its 68 % radius over the
    /// whole cloud, and the cluster's course.
    #[must_use]
    pub fn estimate(&self) -> (Point, f64, Option<f64>) {
        let (mean, cluster) = self.cluster();
        let course = dominant_course(cluster.iter().map(|&i| &self.ps[i]));
        (self.frame.to_geo(mean), ACC_TO_SIGMA * (self.spread_m2(mean) / 2.0).max(0.0).sqrt(), course)
    }

    /// The heaviest cluster: its centre (ENU) and the particles within the radius `r` of it, `r` being `cluster_m` or the cloud's own
    /// sigma when that is wider (a cloud long drawn out along a street is one cluster, not a row of small ones to hop between). The
    /// cloud is binned into `r` squares; the square whose 3 x 3 neighbourhood weighs most seeds a Gaussian mean shift of bandwidth `r`.
    fn cluster(&self) -> ([f64; 2], Vec<usize>) {
        let all: Vec<usize> = (0..self.ps.len()).collect();
        let r = self.cluster_m.max((self.spread_m2(self.weighted_mean(&all)) / 2.0).max(0.0).sqrt());
        let cell = |en: [f64; 2]| (floor_i64(en[0] / r), floor_i64(en[1] / r));
        let mut bins: std::collections::HashMap<(i64, i64), f64> = std::collections::HashMap::new();
        for x in &self.ps {
            *bins.entry(cell(x.en)).or_insert(0.0) += x.w;
        }
        let around = |(ce, cn): (i64, i64)| (-1..=1).flat_map(move |de| (-1..=1).map(move |dn| (ce + de, cn + dn)));
        let score = |c: (i64, i64)| around(c).filter_map(|k| bins.get(&k)).sum::<f64>();
        // Ties go to the smaller cell, so a replay picks the same one whatever the map's order.
        let best = bins.keys().copied().max_by(|a, b| score(*a).total_cmp(&score(*b)).then(b.cmp(a))).unwrap_or((0, 0));
        let near: std::collections::HashSet<(i64, i64)> = around(best).collect();
        let mut mean = self.weighted_mean(&all.iter().copied().filter(|&i| near.contains(&cell(self.ps[i].en))).collect::<Vec<_>>());
        let d2 = |x: &Particle, m: [f64; 2]| (x.en[0] - m[0]).powi(2) + (x.en[1] - m[1]).powi(2);
        for _ in 0..MEAN_SHIFT_STEPS {
            let k: Vec<f64> = self.ps.iter().map(|x| x.w * (-0.5 * d2(x, mean) / (r * r)).exp()).collect();
            let ks: f64 = k.iter().sum();
            if ks <= f64::MIN_POSITIVE {
                break;
            }
            let along = |c: usize| self.ps.iter().zip(&k).map(|(x, w)| w * x.en[c]).sum::<f64>() / ks;
            let next = [along(0), along(1)];
            let moved = (next[0] - mean[0]).hypot(next[1] - mean[1]);
            mean = next;
            if moved < 0.1 {
                break;
            }
        }
        (mean, all.into_iter().filter(|&i| d2(&self.ps[i], mean) <= r * r).collect())
    }

    /// The weighted mean of the particles `idx` (ENU).
    fn weighted_mean(&self, idx: &[usize]) -> [f64; 2] {
        let wsum: f64 = idx.iter().map(|&i| self.ps[i].w).sum::<f64>().max(1e-12);
        let sum = |k: usize| idx.iter().map(|&i| self.ps[i].w * self.ps[i].en[k]).sum::<f64>() / wsum;
        [sum(0), sum(1)]
    }

    /// The cloud's weighted mean squared distance from `m`, m^2.
    fn spread_m2(&self, m: [f64; 2]) -> f64 {
        self.ps.iter().map(|x| x.w * ((x.en[0] - m[0]).powi(2) + (x.en[1] - m[1]).powi(2))).sum()
    }

    /// Memory the cloud holds, bytes (the spec's 5 MB budget covers graph, lattice and particles).
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        size_of::<Self>() + self.ps.capacity() * size_of::<Particle>()
    }

    /// The bearing of the street the heaviest cluster walks, degrees; `None` when the cluster is mostly off the network (its course is
    /// then only the heading it was steered by).
    #[must_use]
    pub fn street_course(&self) -> Option<f64> {
        let (_, cluster) = self.cluster();
        let on: Vec<&Particle> = cluster.iter().map(|&i| &self.ps[i]).filter(|x| x.seg.is_some()).collect();
        let (w_on, w_all) = (on.iter().map(|x| x.w).sum::<f64>(), cluster.iter().map(|&i| self.ps[i].w).sum::<f64>());
        if w_on < 0.5 * w_all {
            return None;
        }
        dominant_course(on.into_iter())
    }

    /// The bearing of the streets the cloud's street particles walk, all of them (not only the heaviest cluster's), degrees; `None`
    /// with none on a street.
    #[must_use]
    pub fn network_course(&self) -> Option<f64> {
        dominant_course(self.ps.iter().filter(|x| x.seg.is_some()))
    }

    /// Put the off-network particles within `within_m` of a street back onto the nearest one, walking the way nearest `course_deg`
    /// (ruling T22-R4: after a wrong compass drew them off), keeping the `bridge_off_share` floor off the network (the farthest stay).
    pub fn snap_to_streets(&mut self, within_m: f64, course_deg: f64, p: &LocParams) {
        let Some(g) = self.graph.clone() else { return };
        let floor = floor_usize(count_f64(self.ps.len()) * p.bridge_off_share);
        let mut near: Vec<(usize, crate::loc::graph::Cand)> = (0..self.ps.len())
            .filter(|&i| self.ps[i].seg.is_none())
            .filter_map(|i| g.candidates(self.frame.to_geo(self.ps[i].en), within_m, 1, self.mask).first().map(|c| (i, *c)))
            .collect();
        let off = self.ps.iter().filter(|x| x.seg.is_none()).count();
        near.sort_by(|a, b| a.1.d_m.total_cmp(&b.1.d_m));
        near.truncate(off.saturating_sub(floor));
        for (i, c) in near {
            let dir = if wrap_deg(g.bearing_deg(c.seg, 1.0) - course_deg).abs() <= 90.0 { 1.0 } else { -1.0 };
            let x = &mut self.ps[i];
            (x.seg, x.off, x.dir, x.en, x.course_deg) = (Some(c.seg), c.off_m, dir, g.en_at(c.seg, c.off_m), g.bearing_deg(c.seg, dir));
        }
    }

    /// Decide again which way each street particle walks, as at [`Self::start`] (review round 3: the walker turned round while standing).
    pub fn redirect(&mut self, course_deg: Option<f64>, p: &LocParams) {
        let Some(g) = self.graph.clone() else { return };
        for x in &mut self.ps {
            if let Some(seg) = x.seg {
                x.dir = if forward_on(g.bearing_deg(seg, 1.0), course_deg, p, &mut self.rng) { 1.0 } else { -1.0 };
                x.course_deg = g.bearing_deg(seg, x.dir);
            }
        }
    }

    /// The whole cloud moment-matched to a Gaussian (mean, covariance m^2): the filter's prior when GPS returns.
    #[must_use]
    pub fn moments(&self) -> (Point, Mat<2, 2>) {
        let mean = [self.ps.iter().map(|x| x.w * x.en[0]).sum::<f64>(), self.ps.iter().map(|x| x.w * x.en[1]).sum::<f64>()];
        let mut cov = [[0.0; 2]; 2];
        for x in &self.ps {
            let d = [x.en[0] - mean[0], x.en[1] - mean[1]];
            for (r, dr) in cov.iter_mut().zip(d) {
                r[0] += x.w * dr * d[0];
                r[1] += x.w * dr * d[1];
            }
        }
        (self.frame.to_geo(mean), cov)
    }

    /// The weight share of the cloud on streets within `within_m` of a node where it could turn by more than `turn_deg` (a crossing or a
    /// bend), ahead or just behind: there a compass jump may be the walker turning. Off-network particles do not count.
    #[must_use]
    pub fn near_turn_share(&self, within_m: f64, turn_deg: f64) -> f64 {
        let Some(g) = self.graph.as_ref() else { return 0.0 };
        let can_turn_at = |node: usize, seg: usize, course: f64| {
            g.leaving(node, self.mask).into_iter().any(|(s, dir)| s != seg && wrap_deg(g.bearing_deg(s, dir) - course).abs() > turn_deg)
        };
        self.ps
            .iter()
            .filter(|x| {
                x.seg.is_some_and(|seg| {
                    let sg = g.seg(seg);
                    (x.off <= within_m && can_turn_at(sg.a, seg, x.course_deg)) || (sg.len_m - x.off <= within_m && can_turn_at(sg.b, seg, x.course_deg))
                })
            })
            .map(|x| x.w)
            .sum()
    }
}

#[cfg(test)]
impl Bridge {
    /// The street particles' courses and weights.
    pub(crate) fn street_dirs(&self) -> Vec<(f64, f64)> {
        self.ps.iter().filter(|x| x.seg.is_some()).map(|x| (x.course_deg, x.w)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Mode;
    use crate::geo::{destination, distance_m};
    use crate::loc::bench::grid_ways;
    use crate::loc::graph::mode_mask;

    fn o() -> Point {
        Point::new(40.0, -111.0)
    }

    fn grid() -> Arc<StreetGraph> {
        Arc::new(StreetGraph::from_ways(&grid_ways(o(), 5, 100.0)).unwrap())
    }

    fn start_east(p: &LocParams) -> Bridge {
        let at = destination(destination(o(), 0.0, 100.0), 90.0, 20.0);
        Bridge::start(Some(grid()), Frame::new(o()), mode_mask(Mode::Walk), at, [[9.0, 0.0], [0.0, 9.0]], 0.05, None, Some(90.0), 0, p)
    }

    #[test]
    fn steps_along_a_street_move_the_cloud_along_it() {
        let p = LocParams::default();
        let mut b = start_east(&p);
        for _ in 0..20 {
            b.step(2.0, Steer { theta_deg: 90.0, sigma_deg: 15.0 }, &p);
        }
        let (pos, unc, course) = b.estimate();
        let want = destination(destination(o(), 0.0, 100.0), 90.0, 60.0);
        assert!(distance_m(pos, want) < 8.0, "{} m off", distance_m(pos, want));
        assert!(unc.is_finite() && unc > 0.0 && course.is_some_and(|c| (c - 90.0).abs() < 15.0));
    }

    #[test]
    fn at_a_crossing_the_heading_picks_the_street() {
        let p = LocParams::default();
        let mut b = start_east(&p);
        for _ in 0..40 {
            b.step(2.0, Steer { theta_deg: 90.0, sigma_deg: 15.0 }, &p); // to the crossing at 100 m east
        }
        for _ in 0..20 {
            b.step(2.0, Steer { theta_deg: 0.0, sigma_deg: 15.0 }, &p); // turn north
        }
        let (pos, _, _) = b.estimate();
        let want = destination(destination(destination(o(), 0.0, 100.0), 90.0, 100.0), 0.0, 40.0);
        assert!(distance_m(pos, want) < 12.0, "{} m off", distance_m(pos, want));
    }

    #[test]
    fn the_same_seed_replays_the_same_cloud() {
        let p = LocParams::default();
        let (mut a, mut b) = (start_east(&p), start_east(&p));
        for _ in 0..10 {
            a.step(1.5, Steer { theta_deg: 90.0, sigma_deg: 20.0 }, &p);
            b.step(1.5, Steer { theta_deg: 90.0, sigma_deg: 20.0 }, &p);
        }
        assert_eq!(a.estimate().0, b.estimate().0);
    }

    #[test]
    fn without_a_graph_the_cloud_follows_the_heading() {
        let p = LocParams::default();
        let mut b = Bridge::start(None, Frame::new(o()), mode_mask(Mode::Walk), o(), [[4.0, 0.0], [0.0, 4.0]], 0.05, None, Some(0.0), 0, &p);
        for _ in 0..10 {
            b.step(2.0, Steer { theta_deg: 0.0, sigma_deg: 10.0 }, &p);
        }
        assert!(distance_m(b.estimate().0, destination(o(), 0.0, 20.0)) < 4.0);
        let (mean, cov) = b.moments();
        assert!(distance_m(mean, b.estimate().0) < 2.0 && cov[0][0] > 0.0 && cov[1][1] > 0.0);
    }

    #[test]
    fn snapping_puts_near_off_network_particles_back_on_the_street_and_keeps_the_floor() {
        // Ruling T22-R4.
        let p = LocParams::default();
        let mut b = start_east(&p);
        for (i, x) in b.ps.iter_mut().enumerate() {
            x.seg = None;
            x.en[1] -= if i % 10 == 0 { 40.0 } else { 8.0 }; // a tenth 40 m south of the street, the rest 8 m
        }
        b.snap_to_streets(20.0, 90.0, &p);
        let floor = floor_usize(count_f64(b.ps.len()) * p.bridge_off_share);
        assert_eq!(b.ps.iter().filter(|x| x.seg.is_none()).count(), floor);
        assert!(b.ps.iter().filter(|x| x.seg.is_none()).all(|x| x.en[1] < 70.0), "the far ones stay off (the street is y = 100 m)");
        assert!(b.street_dirs().iter().all(|(c, _)| (c - 90.0).abs() < 1.0), "walking east");
    }

    #[test]
    fn resampling_keeps_an_off_network_floor() {
        // Review M6: off-network particles weigh less every batch; without a floor they die out and a walk off the streets is lost.
        let p = LocParams::default();
        let mut b = start_east(&p);
        let floor = floor_usize(count_f64(b.ps.len()) * p.bridge_off_share);
        for i in 0..60 {
            let theta_deg = if i < 40 { 90.0 } else { 0.0 }; // the turn at the crossing resamples
            b.step(2.0, Steer { theta_deg, sigma_deg: 15.0 }, &p);
            let off = b.ps.iter().filter(|x| x.seg.is_none()).count();
            assert!(off >= floor, "batch {i}: {off} off-network, floor {floor}");
        }
    }

    #[test]
    fn a_long_step_batch_walks_its_whole_distance_over_short_segments() {
        // Review M7: a street of 1 m segments and a 60 m batch (screen-off steps): no silent drop after 20 segments.
        let pts: Vec<Point> = (0..=200).map(|i| destination(o(), 90.0, f64::from(i))).collect();
        let way = crate::scan::WayGeom { id: 1, class: crate::scan::way_class::FOOT, pts };
        let g = Arc::new(StreetGraph::from_ways(&[way]).unwrap());
        let p = LocParams { bridge_bias_sigma_deg: 0.0, ..LocParams::default() };
        let start = destination(o(), 90.0, 20.0);
        let mut b = Bridge::start(Some(g), Frame::new(o()), mode_mask(Mode::Walk), start, [[1.0, 0.0], [0.0, 1.0]], 0.0, None, Some(90.0), 0, &p);
        b.step(60.0, Steer { theta_deg: 90.0, sigma_deg: 15.0 }, &p);
        let d = distance_m(b.estimate().0, destination(o(), 90.0, 80.0));
        assert!(d < 6.0, "{d} m short of 80 m east");
    }
}
