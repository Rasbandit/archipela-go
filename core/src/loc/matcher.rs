//! Layer 2: online HMM map matching (Newson and Krumm 2009) of the IMM estimates, with an off-network state. Display and the trace only:
//! quests never see a matched position.
//!
//! Route lengths and projections come from the graph's single equirectangular frame, so they stretch slightly far from its origin (about
//! 0.13 % at 10 km, 1.3 % at 100 km at latitude 40) while `d_gc` is a true distance: a few centimetres per step, small next to `beta`
//! and `sigma_z`.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::geo::{distance_m, Point};
use crate::loc::graph::{Cand, StreetGraph};
use crate::loc::{Estimate, LocParams, Motion, ACC_TO_SIGMA};
use crate::num::{count_f64, count_u32, i64_to_f64, to_f32};

/// States whose matched points lie this close together are one place for the confidence (ruling T18-conf): at a node, the end of one
/// segment, the start of the next and the cross street.
const SAME_PLACE_M: f64 = 1.5;
/// ... widened to this many `sigma_z` (ruling T18-conf2: near a node the projections spread by the input's offset, which the
/// measurement cannot tell apart), at most `match_place_max_m`.
const PLACE_SIGMAS: f64 = 1.5;

/// Decided trace points are simplified as they settle (final review F3): a point is dropped while the line from the last kept point to
/// the newest stays within this many metres of it (invisible on the map).
const TRACE_SIMPLIFY_M: f64 = 2.0;
/// ... over at most this many points not yet kept: O(window) work per input, so no fix ever stalls.
const TRACE_WINDOW: usize = 64;
/// Memory backstop only (16 B a point): a trace that reaches this many points although simplified (a zigzag) has its kept points
/// thinned to every other one, which rewrites what was given out and so forces the map to reload.
pub const TRACE_CAP: usize = 20_000;

/// The next [`TraceEpoch`]: unique in the process, so no other trace (another game, a copy) ever takes a cursor as its own. Starts at
/// 1, so the cursor 0 is never valid.
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

/// Which version of the trace's settled part a cursor belongs to: a new one whenever points already given out are rewritten.
#[derive(Debug, PartialEq, Eq)]
struct TraceEpoch(u64);

impl TraceEpoch {
    fn fresh() -> Self {
        Self(NEXT_EPOCH.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for TraceEpoch {
    fn default() -> Self {
        Self::fresh()
    }
}

impl Clone for TraceEpoch {
    /// A copy of a matcher is another trace: cursors of the original never apply to it.
    fn clone(&self) -> Self {
        Self::fresh()
    }
}

/// What changed in the session trace since a cursor (Task 19b): the map keeps the settled runs and appends, and redraws the tail.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceDelta {
    /// The cursor was unknown, stale or from another trace, or what it covered was rewritten (thinned at [`TRACE_CAP`]): `append` is
    /// the whole settled trace and replaces what the client holds.
    pub reset: bool,
    /// Pass this next time. It only grows.
    pub cursor: u64,
    /// Time of the first input of the session trace.
    pub from_ms: Option<i64>,
    /// When `append` is not empty: its first run carries on the client's last run (no break between them); every other one starts a
    /// new run. Meaningless when `append` is empty.
    pub joins: bool,
    /// Settled points added since the cursor, as runs, oldest first, none empty.
    pub append: Vec<Vec<Point>>,
    /// The provisional tail, whole (later inputs may revise it): the points the simplifier has not kept yet, then the undecided inputs;
    /// from the newest settled point when it carries on that run, so it draws joined. Empty when nothing is undecided.
    pub tail: Vec<Point>,
}

/// The route cache holds single-source maps for at most this many nodes; beyond it, it starts over.
const ROUTE_CACHE_NODES: usize = 256;

/// The matcher's answer for the newest input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchOut {
    /// The matched point (the estimate itself off-network).
    pub point: Point,
    /// Normalised forward probability of the chosen state and every state of the same kind (street or off-network) at the same place,
    /// within `clamp(1.5 sigma_z, 1.5 m, match_place_max_m)` (rulings T18-conf, conf2, conf3), 0..1; capped at `match_degraded_cap` on
    /// a degraded graph.
    pub confidence: f64,
    /// The street segment; `None` off-network.
    pub seg: Option<usize>,
}

/// One state of a lattice column: a street candidate or (with `cand` `None`) off-network.
#[derive(Debug, Clone, Copy)]
struct State {
    cand: Option<Cand>,
    point: Point,
    input: Point,
    /// Viterbi log score (the emission until linked).
    logv: f64,
    /// Normalised forward probability.
    fwd: f64,
    /// What is shown for this state: the forward probability of its place (ruling T18-conf), capped on a degraded graph.
    conf: f64,
    /// Best predecessor in the previous column.
    back: Option<usize>,
    /// Direction of travel along the segment that led here: +1 towards node `b`, -1 towards `a`, 0 unknown.
    dir: i8,
}

/// What one lattice step knows about the move between two inputs.
struct Step {
    d_gc: f64,
    p_off: f64,
    p_on: f64,
    bound: f64,
    moving: bool,
}

/// `ln` of the street-to-street transition: `(1 - p_off) exp(-|d_gc - d_route| / beta) / beta`.
#[must_use]
pub fn street_transition_log(d_gc: f64, d_route: f64, beta: f64, p_off: f64) -> f64 {
    (1.0 - p_off).ln() - (d_gc - d_route).abs() / beta - beta.ln()
}

/// Street-to-off-network and off-network-to-street probabilities for a step of `dt_s` seconds: `rate * dt`, capped at 0.5. A step with
/// no time (or, never expected, back in time) counts as 0 s, where neither switch can happen.
#[must_use]
fn on_off_probs(dt_s: f64, p: &LocParams) -> (f64, f64) {
    let dt = dt_s.max(0.0);
    ((p.match_to_off_per_s * dt).min(0.5), (p.match_to_on_per_s * dt).min(0.5))
}

/// Online Viterbi (pin: lag 0; trace: fixed lag) over street candidates plus an off-network state.
#[derive(Debug, Clone, Default)]
pub struct Matcher {
    graph: Option<Arc<StreetGraph>>,
    mask: u8,
    /// The newest columns, oldest first: the undecided ones (at most `match_lag`), and always the newest even when decided (the next
    /// step links to it).
    columns: VecDeque<Vec<State>>,
    /// How many of the newest columns are not yet in the trace.
    undecided: usize,
    last_input: Option<(Point, i64)>,
    best: Option<MatchOut>,
    /// Decided points: the kept ones (given out by [`Self::trace_since`]), then the ones the simplifier has not decided yet (the window,
    /// all after the last kept one, in its run).
    trace: Vec<Point>,
    /// How many leading trace points are kept.
    kept: usize,
    /// Where each run of the trace after the first starts (the filter reset or relocated there), ascending.
    breaks: Vec<usize>,
    trace_from_ms: Option<i64>,
    /// The version of the trace's settled part, for [`Self::trace_since`].
    epoch: TraceEpoch,
    /// Street distances from a node to the nodes within the route limit, sorted by node.
    cache: HashMap<usize, Vec<(u32, f32)>>,
    route_limit_m: f64,
    show_confidence: f64,
}

impl Matcher {
    /// A matcher on `graph` for the way classes in `mask`.
    #[must_use]
    pub fn new(graph: Option<Arc<StreetGraph>>, mask: u8) -> Self {
        Self { graph, mask, ..Self::default() }
    }

    /// Change the graph (it arrived after the game opened, or was rebuilt): the lattice restarts, so no segment id of the old graph
    /// survives; the trace keeps going.
    pub fn set_graph(&mut self, graph: Option<Arc<StreetGraph>>) {
        self.restart();
        self.graph = graph;
        self.cache.clear();
    }

    /// Change the way classes (the zone's mode changed); true when they changed, which restarts the lattice.
    pub fn set_mask(&mut self, mask: u8) -> bool {
        let changed = mask != self.mask;
        if changed {
            self.restart();
            self.mask = mask;
            self.cache.clear();
        }
        changed
    }

    /// Forget the lattice (the filter restarted); what it already holds is decided and added to the trace first.
    pub fn restart(&mut self) {
        self.flush();
        self.columns.clear();
        self.last_input = None;
        self.best = None;
    }

    /// The newest answer.
    #[must_use]
    pub fn best(&self) -> Option<MatchOut> {
        self.best
    }

    /// The session's line: matched points where confident, estimate points elsewhere, a few inputs behind, simplified at 2 m as it
    /// settles. All runs one after the other; [`Self::runs`] splits them.
    #[must_use]
    pub fn trace(&self) -> &[Point] {
        &self.trace
    }

    /// The trace as its runs, oldest first, none empty: it breaks where the filter reset or relocated (ruling T19-R3).
    #[must_use]
    pub fn runs(&self) -> Vec<&[Point]> {
        let mut out = Vec::with_capacity(self.breaks.len() + 1);
        let mut start = 0;
        for &b in self.breaks.iter().chain(std::iter::once(&self.trace.len())) {
            if b > start {
                out.push(&self.trace[start..b]);
            }
            start = b;
        }
        out
    }

    /// [`Self::runs`] with the provisional tail (ruling T19-R4): the undecided inputs on the current best path, so the line reaches the
    /// newest input. Later inputs may still revise the tail; it is never stored.
    #[must_use]
    pub fn runs_with_tail(&self) -> Vec<Vec<Point>> {
        let mut runs: Vec<Vec<Point>> = self.runs().into_iter().map(<[Point]>::to_vec).collect();
        let tail = self.tail();
        if tail.is_empty() {
            return runs;
        }
        match runs.last_mut() {
            Some(last) if self.run_goes_on() => last.extend(tail),
            _ => runs.push(tail),
        }
        runs
    }

    /// What changed since `cursor` (from an earlier call; 0 for a first or full load): the settled points added since, or everything
    /// with `reset` when the cursor is not this trace's current version, and always the whole provisional tail.
    #[must_use]
    pub fn trace_since(&self, cursor: u64) -> TraceDelta {
        let had = usize::try_from(cursor & u64::from(u32::MAX)).ok().filter(|&n| cursor >> 32 == self.epoch.0 && n <= self.kept);
        let reset = had.is_none();
        let from = had.unwrap_or(0);
        let mut append = Vec::new();
        let mut start = from;
        for &b in self.breaks.iter().filter(|&&b| b > from && b <= self.kept).chain(std::iter::once(&self.kept)) {
            if b > start {
                append.push(self.trace[start..b].to_vec());
            }
            start = b;
        }
        // The points the simplifier has not kept yet are provisional too: they join the tail.
        let mut tail: Vec<Point> = self.trace[self.kept..].to_vec();
        tail.extend(self.tail());
        if !tail.is_empty() && self.kept > 0 && !self.breaks.contains(&self.kept) {
            tail.insert(0, self.trace[self.kept - 1]);
        }
        TraceDelta {
            reset,
            cursor: (self.epoch.0 << 32) | u64::from(count_u32(self.kept)),
            from_ms: self.trace_from_ms,
            joins: !reset && from > 0 && !self.breaks.contains(&from),
            append,
            tail,
        }
    }

    /// The undecided inputs on the current best path (ruling T19-R4).
    fn tail(&self) -> Vec<Point> {
        let path = self.path();
        let skip = self.columns.len() - self.undecided;
        self.columns.iter().zip(&path).skip(skip).map(|(c, &i)| Self::shown(&c[i], self.show_confidence)).collect()
    }

    /// Whether the next point carries on the trace's last run (there is one, and no break ends it).
    fn run_goes_on(&self) -> bool {
        !self.trace.is_empty() && self.breaks.last() != Some(&self.trace.len())
    }

    /// End the trace's current run: the next point starts a new one. Call after [`Self::restart`] when the player may have reappeared
    /// elsewhere. The points the simplifier still held are kept as they are (ruling T19b-break: a break is an append for the map). A
    /// break at the trace's start or where it already breaks does nothing (the guard below), so callers may break twice.
    pub fn break_trace(&mut self) {
        self.kept = self.trace.len();
        let at = self.trace.len();
        if at != 0 && self.breaks.last() != Some(&at) {
            self.breaks.push(at);
        }
    }

    /// Time of the first input of the session trace.
    #[must_use]
    pub fn trace_from_ms(&self) -> Option<i64> {
        self.trace_from_ms
    }

    /// A decided state's trace point: the matched point when it is a street state shown with enough confidence, else the estimate.
    fn shown(s: &State, show_confidence: f64) -> Point {
        if s.cand.is_some() && s.conf >= show_confidence {
            s.point
        } else {
            s.input
        }
    }

    /// Confidence of state `i` of a column: the summed forward probability of every state of its kind (street or off-network) at its
    /// place (within `place_m`). The off-network state never adds to a street's: it sits at the estimate, a few metres from every
    /// street nearby, and would make either of two parallel streets look sure.
    fn confidence(col: &[State], i: usize, place_m: f64) -> f64 {
        let me = &col[i];
        col.iter().filter(|s| s.cand.is_some() == me.cand.is_some() && distance_m(s.point, me.point) <= place_m).map(|s| s.fwd).sum::<f64>().min(1.0)
    }

    fn best_in(col: &[State]) -> usize {
        (0..col.len()).max_by(|&a, &b| col[a].logv.total_cmp(&col[b].logv)).unwrap_or(0)
    }

    /// Backtrack from the best state of the newest column: the chosen state of every column, oldest first.
    fn path(&self) -> Vec<usize> {
        let Some(last) = self.columns.back() else { return vec![] };
        let mut idx = Self::best_in(last);
        let mut out = vec![idx];
        for k in (1..self.columns.len()).rev() {
            idx = self.columns[k][idx].back.unwrap_or(0);
            out.push(idx);
        }
        out.reverse();
        out
    }

    /// Decide every column not yet in the trace and add it.
    fn flush(&mut self) {
        let path = self.path();
        let skip = self.columns.len() - self.undecided;
        let shown: Vec<Point> = self.columns.iter().zip(&path).skip(skip).map(|(c, &i)| Self::shown(&c[i], self.show_confidence)).collect();
        for q in shown {
            self.settle(q);
        }
        self.undecided = 0;
    }

    /// Add a decided point, simplifying as it settles (final review F3). The first point of a run is kept at once; later ones wait in the
    /// window while the line from the last kept point to the newest stays within [`TRACE_SIMPLIFY_M`] of every one of them. A window
    /// point farther from that line (a corner) is kept, with the window before it simplified (Douglas-Peucker on at most
    /// [`TRACE_WINDOW`] points); a full window is kept up to its newest point the same way. Every dropped point stays within 2 m of the
    /// kept line, and kept points are never rewritten, except by the [`TRACE_CAP`] backstop.
    fn settle(&mut self, q: Point) {
        if self.trace.len() == self.breaks.last().copied().unwrap_or(0) {
            self.kept = self.trace.len() + 1; // a run's first point
        } else {
            loop {
                let anchor = self.trace[self.kept - 1];
                let far = self.trace[self.kept..]
                    .iter()
                    .enumerate()
                    .map(|(i, w)| (i, crate::geo::distance_to_segment_m(*w, anchor, q)))
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                let split = match far {
                    Some((k, d)) if d > TRACE_SIMPLIFY_M => self.kept + k,
                    Some(_) if self.trace.len() - self.kept >= TRACE_WINDOW => self.trace.len() - 1,
                    _ => break,
                };
                self.keep_through(split);
            }
        }
        self.trace.push(q);
        if self.trace.len() >= TRACE_CAP {
            self.thin();
        }
    }

    /// Keep the window up to the trace point `split`, simplified: the points after it stay in the window.
    fn keep_through(&mut self, split: usize) {
        let piece = crate::geo::simplify(&self.trace[self.kept - 1..=split], TRACE_SIMPLIFY_M);
        let rest = self.trace.split_off(split + 1);
        self.trace.truncate(self.kept - 1);
        self.trace.extend(piece);
        self.kept = self.trace.len();
        self.trace.extend(rest);
    }

    /// The memory backstop: keep every other kept point of each run (and each run's first and last), which rewrites what was given out
    /// (a fresh epoch: the map reloads). O(n), so even this never stalls a fix.
    fn thin(&mut self) {
        let ends: Vec<usize> = self.breaks.iter().copied().filter(|&b| b < self.kept).chain(std::iter::once(self.kept)).collect();
        let mut out = Vec::with_capacity(self.trace.len() / 2 + 2);
        let mut breaks = Vec::with_capacity(self.breaks.len());
        let mut start = 0;
        for end in ends {
            if start > 0 {
                breaks.push(out.len());
            }
            out.extend((start..end).filter(|i| (i - start) % 2 == 0 || *i == end - 1).map(|i| self.trace[i]));
            start = end;
        }
        let kept = out.len();
        out.extend_from_slice(&self.trace[self.kept..]); // the window, after the last kept point and in its run: no break in it
        if self.breaks.contains(&self.kept) {
            breaks.push(kept);
        }
        self.trace = out;
        self.breaks = breaks;
        self.kept = kept;
        self.epoch = TraceEpoch::fresh(); // points already given out changed
    }

    /// Street distance from node `from` to node `to`, if within the route limit.
    fn node_dist(&mut self, g: &StreetGraph, from: usize, to: usize) -> Option<f64> {
        if self.cache.len() >= ROUTE_CACHE_NODES && !self.cache.contains_key(&from) {
            self.cache.clear();
        }
        let (mask, limit) = (self.mask, self.route_limit_m);
        let map = self.cache.entry(from).or_insert_with(|| {
            let mut v: Vec<(u32, f32)> = g.dijkstra(from, limit, mask).into_iter().map(|(n, d)| (count_u32(n), to_f32(d))).collect();
            v.sort_unstable_by_key(|x| x.0);
            v
        });
        let to = count_u32(to);
        map.binary_search_by_key(&to, |x| x.0).ok().map(|i| f64::from(map[i].1))
    }

    /// Shortest street distance between two candidates, if at most `bound`.
    fn route(&mut self, g: &StreetGraph, a: &Cand, b: &Cand, bound: f64) -> Option<f64> {
        let best = if a.seg == b.seg {
            (b.off_m - a.off_m).abs()
        } else {
            let (sa, sb) = (g.seg(a.seg), g.seg(b.seg));
            let mut best = f64::INFINITY;
            for (n, c0) in [(sa.a, a.off_m), (sa.b, sa.len_m - a.off_m)] {
                for (m, c1) in [(sb.a, b.off_m), (sb.b, sb.len_m - b.off_m)] {
                    if let Some(d) = self.node_dist(g, n, m) {
                        best = best.min(c0 + d + c1);
                    }
                }
            }
            best
        };
        (best <= bound).then_some(best)
    }

    /// Feed an estimate. Only accepted, moving estimates that moved `match_min_move_m` (2 m) or came `match_min_gap_ms` (1 s) after the
    /// last input count (ruling T19-input: about 1 Hz while walking); standing freezes the match. `None` without
    /// a graph (the trace still grows from the estimates).
    pub fn push(&mut self, est: &Estimate, p: &LocParams) -> Option<MatchOut> {
        if !est.accepted || est.motion == Motion::Stationary {
            return self.best;
        }
        let here = est.point();
        if let Some((prev, t)) = self.last_input {
            if distance_m(prev, here) < p.match_min_move_m && est.t_ms - t < p.match_min_gap_ms {
                return self.best;
            }
        }
        self.trace_from_ms.get_or_insert(est.t_ms);
        if p.match_route_limit_m.to_bits() != self.route_limit_m.to_bits() {
            self.cache.clear(); // its maps were searched to the old limit
        }
        (self.route_limit_m, self.show_confidence) = (p.match_route_limit_m, p.match_show_confidence);
        let Some(graph) = self.graph.clone() else {
            self.settle(here);
            self.last_input = Some((here, est.t_ms));
            return None;
        };
        let sigma = (est.uncertainty_m / ACC_TO_SIGMA).max(p.match_sigma_floor_m);
        let radius = p.match_max_radius_m.min(3.0 * sigma + 10.0);
        let emis = |d: f64| -0.5 * (d / sigma).powi(2) - (std::f64::consts::TAU.sqrt() * sigma).ln();
        let mut states: Vec<State> = graph
            .candidates(here, radius, p.match_max_candidates, self.mask)
            .into_iter()
            .map(|c| State { cand: Some(c), point: graph.geo_at(c.seg, c.off_m), input: here, logv: emis(c.d_m), fwd: 0.0, conf: 0.0, back: None, dir: 0 })
            .collect();
        states.push(State { cand: None, point: here, input: here, logv: emis(p.match_off_road_m), fwd: 0.0, conf: 0.0, back: None, dir: 0 });
        let linked = match (self.columns.back().cloned(), self.last_input) {
            (Some(prev), Some((last_at, last_ms))) => {
                let dt = i64_to_f64(est.t_ms - last_ms) / 1000.0;
                let d_gc = distance_m(last_at, here);
                let (p_off, p_on) = on_off_probs(dt, p);
                let step = Step { d_gc, p_off, p_on, bound: 2.0 * d_gc + 50.0, moving: est.speed_mps >= 0.5 };
                self.link(&graph, &prev, &mut states, &step, p)
            }
            _ => false,
        };
        if !linked {
            // The first input since a restart. The off-network state keeps every later step linked (off to off is always possible), so
            // the lattice breaks only through `restart()`: the Locator calls it when the filter resets or relocates.
            self.restart();
            let top = states.iter().map(|s| s.logv).fold(f64::NEG_INFINITY, f64::max);
            for s in &mut states {
                s.fwd = (s.logv - top).exp();
            }
        }
        let total: f64 = states.iter().map(|s| s.fwd).sum();
        let top = states.iter().map(|s| s.logv).fold(f64::NEG_INFINITY, f64::max);
        for s in &mut states {
            s.fwd = if total > 0.0 { s.fwd / total } else { 0.0 };
            s.logv -= top; // keep the numbers small
        }
        let cap = if graph.is_degraded() { p.match_degraded_cap } else { 1.0 };
        let place_m = (PLACE_SIGMAS * sigma).min(p.match_place_max_m).max(SAME_PLACE_M);
        let confs: Vec<f64> = (0..states.len()).map(|i| Self::confidence(&states, i, place_m).min(cap)).collect();
        for (s, c) in states.iter_mut().zip(confs) {
            s.conf = c;
        }
        let i = Self::best_in(&states);
        self.best = Some(MatchOut { point: states[i].point, confidence: states[i].conf, seg: states[i].cand.map(|c| c.seg) });
        self.columns.push_back(states);
        self.undecided += 1;
        // Fixed lag: a column with `lag` newer inputs is final and goes into the trace; the newest column stays for the next step.
        if self.undecided > p.match_lag {
            let path = self.path();
            while self.undecided > p.match_lag {
                let k = self.columns.len() - self.undecided;
                self.settle(Self::shown(&self.columns[k][path[k]], p.match_show_confidence));
                self.undecided -= 1;
            }
        }
        while self.columns.len() > p.match_lag.max(1) {
            self.columns.pop_front();
        }
        self.last_input = Some((here, est.t_ms));
        self.best
    }

    /// One lattice step: Viterbi scores, back pointers and forward probabilities of `states` from `prev`. False when no state of `prev`
    /// can lead to any of `states`.
    fn link(&mut self, graph: &StreetGraph, prev: &[State], states: &mut [State], step: &Step, params: &LocParams) -> bool {
        let n_street = states.iter().filter(|s| s.cand.is_some()).count().max(1);
        let to_street = (step.p_on / count_f64(n_street)).ln();
        let mut any = false;
        for sj in states.iter_mut() {
            let emission = sj.logv;
            let (mut best, mut back, mut dir, mut fsum) = (f64::NEG_INFINITY, None, 0_i8, 0.0);
            for (i, si) in prev.iter().enumerate() {
                let (lt, d) = match (si.cand, sj.cand) {
                    (Some(a), Some(b)) => match self.route(graph, &a, &b, step.bound) {
                        Some(dr) => {
                            let d = if a.seg == b.seg { sign(b.off_m - a.off_m) } else { 0 };
                            // Not moving along the segment keeps the direction it had.
                            let kept = if a.seg == b.seg && d == 0 { si.dir } else { d };
                            let uturn = a.seg == b.seg && si.dir != 0 && d != 0 && d != si.dir && step.moving;
                            let penalty = if uturn { params.match_uturn_factor.ln() } else { 0.0 };
                            (street_transition_log(step.d_gc, dr, params.match_beta_m, step.p_off) + penalty, kept)
                        }
                        None => (f64::NEG_INFINITY, 0),
                    },
                    (Some(_), None) => (step.p_off.ln(), 0),
                    (None, Some(_)) => (to_street, 0),
                    (None, None) => ((1.0 - step.p_on).ln(), 0),
                };
                if !lt.is_finite() {
                    continue;
                }
                if si.logv + lt > best {
                    (best, back, dir) = (si.logv + lt, Some(i), d);
                }
                fsum += si.fwd * lt.exp();
            }
            sj.logv = best + emission;
            sj.back = back;
            sj.dir = dir;
            sj.fwd = fsum * emission.exp();
            any |= back.is_some();
        }
        any
    }
}

fn sign(x: f64) -> i8 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Mode;
    use crate::geo::destination;
    use crate::loc::bench::grid_ways;
    use crate::loc::graph::mode_mask;
    use crate::loc::Estimate;

    fn o() -> Point {
        Point::new(40.0, -111.0)
    }

    fn grid() -> Arc<StreetGraph> {
        Arc::new(StreetGraph::from_ways(&grid_ways(o(), 5, 100.0)).unwrap())
    }

    fn est(p: Point, t_s: i64) -> Estimate {
        Estimate { uncertainty_m: 5.0, speed_mps: 1.4, ..Estimate::exact(p.lat, p.lon, t_s * 1000) }
    }

    fn walk(m: &mut Matcher, from: Point, bearing: f64, side_m: f64, n: i64, t0: i64) -> Option<MatchOut> {
        let p = LocParams::default();
        let mut out = None;
        for i in 0..n {
            let on = destination(from, bearing, 7.0 * i64_to_f64(i));
            out = m.push(&est(destination(on, bearing + 90.0, side_m), t0 + i * 5), &p);
        }
        out
    }

    #[test]
    fn a_walk_beside_a_street_is_matched_onto_it_with_confidence() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let out = walk(&mut m, destination(o(), 0.0, 100.0), 90.0, 4.0, 10, 0).unwrap();
        assert!(out.seg.is_some() && out.confidence >= 0.7, "{out:?}");
        let on_street = (out.point.lat - destination(o(), 0.0, 100.0).lat).abs() * 111_195.0;
        assert!(on_street < 0.5, "the pin sits on the east-west street: {on_street} m off");
    }

    #[test]
    fn turning_at_a_crossing_switches_to_the_new_street() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        walk(&mut m, destination(o(), 0.0, 100.0), 90.0, 2.0, 15, 0); // east along y = 100 m to x = 100 m
        let corner = destination(destination(o(), 0.0, 100.0), 90.0, 100.0);
        let out = walk(&mut m, corner, 0.0, 2.0, 6, 75).unwrap(); // then north
        let off_line = (out.point.lon - corner.lon).abs() * 111_195.0 * corner.lat.to_radians().cos();
        assert!(off_line < 0.5, "matched onto the north street: {out:?}");
    }

    #[test]
    fn far_from_every_street_it_is_off_network() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let out = walk(&mut m, destination(destination(o(), 0.0, 150.0), 90.0, 150.0), 0.0, 0.0, 3, 0).unwrap(); // middle of a block, 50 m from streets
        assert!(out.seg.is_none(), "{out:?}");
    }

    #[test]
    fn a_degraded_graph_caps_the_confidence() {
        let links: Vec<(Point, Point)> =
            (0..4).map(|i| (destination(o(), 90.0, 60.0 * f64::from(i)), destination(o(), 90.0, 60.0 * f64::from(i + 1)))).collect();
        let mut m = Matcher::new(StreetGraph::degraded(&links).map(Arc::new), mode_mask(Mode::Walk));
        let out = walk(&mut m, o(), 90.0, 1.0, 8, 0).unwrap();
        assert!(out.confidence <= 0.5 + 1e-9, "{out:?}");
        // The trace too (review I1): every point is the estimate, 1 m beside the link, never snapped onto it.
        assert_eq!(m.trace().len(), 8 - 3);
        assert!(m.trace().iter().all(|q| (o().lat - q.lat) * 111_195.0 > 0.9), "{:?}", m.trace());
    }

    #[test]
    fn standing_and_tiny_moves_do_not_feed_the_lattice_and_the_trace_lags_three_inputs() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let p = LocParams::default();
        let still = Estimate { motion: Motion::Stationary, ..est(o(), 0) };
        assert!(m.push(&still, &p).is_none());
        walk(&mut m, destination(o(), 0.0, 100.0), 90.0, 2.0, 10, 1);
        assert_eq!(m.trace().len(), 10 - 3);
        let before = m.trace().len();
        // 1 m and 0.5 s after the last input: under both input thresholds (ruling T19-input: 2 m or 1 s).
        let beside = destination(destination(destination(o(), 0.0, 100.0), 90.0, 64.0), 180.0, 2.0);
        m.push(&Estimate { t_ms: 46_500, ..est(beside, 0) }, &p);
        assert_eq!(m.trace().len(), before);
    }

    #[test]
    fn with_no_lag_the_trace_follows_every_input_and_the_lattice_still_links() {
        // Review M2: lag 0 decides each input at once but keeps it as the previous column for the next step.
        let p = LocParams { match_lag: 0, ..LocParams::default() };
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let row = destination(o(), 0.0, 100.0);
        let mut out = None;
        for i in 0..6 {
            out = m.push(&est(destination(destination(row, 90.0, 7.0 * i64_to_f64(i)), 180.0, 2.0), i * 5), &p);
            assert_eq!(m.trace().len(), usize::try_from(i + 1).unwrap(), "input {i}");
            assert_eq!(m.columns.len(), 1);
        }
        let out = out.unwrap();
        assert!(out.seg.is_some() && out.confidence >= 0.7, "{out:?}");
        assert!(m.columns[0][0].back.is_some(), "the newest column was linked to the one before");
        m.restart();
        assert_eq!(m.trace().len(), 6, "a decided column is not added twice");
    }

    #[test]
    fn a_step_with_no_time_or_back_in_time_never_switches_on_or_off_the_streets() {
        // Review M1: dt is clamped to 0, where street <-> off-network is impossible (the spec's `rate * dt`).
        let p = LocParams::default();
        assert_eq!(on_off_probs(0.0, &p), (0.0, 0.0));
        assert_eq!(on_off_probs(-3.0, &p), (0.0, 0.0));
        assert_eq!(on_off_probs(2.0, &p), (0.04, 0.1));
        assert_eq!(on_off_probs(100.0, &p), (0.5, 0.5));
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let row = destination(o(), 0.0, 100.0);
        walk(&mut m, row, 90.0, 2.0, 4, 10);
        let out = m.push(&est(destination(row, 90.0, 40.0), 0), &p).unwrap(); // 19 m on, 25 s back in time
        assert!(out.seg.is_some() && (0.0..=1.0).contains(&out.confidence), "{out:?}");
    }

    #[test]
    fn a_new_route_limit_clears_the_route_cache() {
        // Review M5: maps cached under one limit are never read under another.
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let row = destination(o(), 0.0, 100.0);
        walk(&mut m, destination(row, 90.0, 80.0), 90.0, 2.0, 5, 0);
        assert!(m.cache.values().flatten().any(|x| x.1 > 100.0));
        let p = LocParams { match_route_limit_m: 60.0, ..LocParams::default() };
        m.push(&est(destination(row, 90.0, 120.0), 30), &p);
        assert!(!m.cache.is_empty() && m.cache.values().flatten().all(|x| x.1 <= 60.0), "{:?}", m.cache);
    }

    #[test]
    fn without_a_graph_the_trace_is_the_estimates() {
        let mut m = Matcher::default();
        assert!(walk(&mut m, o(), 90.0, 0.0, 5, 0).is_none());
        assert_eq!(m.trace().len(), 5);
    }

    #[test]
    fn a_long_session_trace_is_simplified_and_keeps_its_corners() {
        // Controller note 2: the trace grew 16 B per input; it is simplified at 2 m as it settles (final review F3).
        let mut m = Matcher::default();
        let p = LocParams::default();
        let n = i64::try_from(TRACE_CAP).unwrap() + 1_000;
        let corner = destination(o(), 0.0, i64_to_f64(n / 2));
        for i in 0..n {
            let at = if i < n / 2 { destination(o(), 0.0, i64_to_f64(i)) } else { destination(corner, 90.0, i64_to_f64(i - n / 2)) };
            m.push(&est(at, i * 5), &p);
            assert!(m.trace().len() <= TRACE_CAP, "input {i}: {}", m.trace().len());
        }
        assert!(m.trace().len() < 2_000, "{}", m.trace().len());
        assert!(m.trace().iter().any(|q| distance_m(*q, corner) < 1.0), "the corner is kept");
        assert!(m.trace().first().is_some_and(|q| distance_m(*q, o()) < 1e-6), "the first point is kept");
        let last = destination(corner, 90.0, i64_to_f64(n - 1 - n / 2));
        assert!(m.trace().last().is_some_and(|q| distance_m(*q, last) < 1e-6), "the newest point is kept");
        assert_eq!(m.trace_from_ms(), Some(0));
    }

    #[test]
    fn a_break_starts_a_new_run_and_simplifying_never_joins_two_runs() {
        // Ruling T19-R3: the trace breaks where the filter reset or relocated.
        let mut m = Matcher::default();
        let p = LocParams::default();
        m.break_trace(); // nothing to break yet
        for i in 0..5 {
            m.push(&est(destination(o(), 0.0, 7.0 * i64_to_f64(i)), i * 5), &p);
        }
        m.restart();
        m.break_trace();
        m.break_trace(); // twice is once
        let far = destination(o(), 90.0, 3000.0);
        for i in 0..5 {
            m.push(&est(destination(far, 0.0, 7.0 * i64_to_f64(i)), 100 + i * 5), &p);
        }
        let runs = m.runs();
        assert_eq!(runs.iter().map(|r| r.len()).collect::<Vec<_>>(), [5, 5], "a break simplifies nothing (ruling T19b-break): {runs:?}");
        assert!(distance_m(runs[1][0], far) < 1e-6, "the new run starts where the player reappeared");
        // Each run is simplified on its own as it settles (final review F3): the first run's end is never joined to the second, and
        // the first run, given out whole at the break, is never rewritten.
        let n = i64::try_from(TRACE_CAP).unwrap() + 10;
        for i in 5..n {
            m.push(&est(destination(far, 0.0, i64_to_f64(i) + 30.0), 100 + i * 5), &p);
        }
        let runs = m.runs();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].len(), 5, "the first run as it was at the break: {:?}", runs[0]);
        // A straight run keeps a point every TRACE_WINDOW inputs (the window's bound), plus the window not yet decided.
        let most = 2 + usize::try_from(n).unwrap() / TRACE_WINDOW + TRACE_WINDOW;
        assert!(distance_m(runs[1][0], far) < 1e-6 && runs[1].len() <= most, "{} of at most {most}", runs[1].len());
        assert_eq!(m.trace_from_ms(), Some(0));
    }

    #[test]
    fn the_runs_with_their_tail_reach_the_newest_input_without_storing_it() {
        // Ruling T19-R4: the undecided columns' best path is drawn as a provisional tail.
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let row = destination(o(), 0.0, 100.0);
        walk(&mut m, row, 90.0, 2.0, 10, 0);
        let decided = m.trace().len();
        let runs = m.runs_with_tail();
        assert_eq!((runs.len(), runs[0].len()), (1, decided + LocParams::default().match_lag));
        assert!(distance_m(*runs[0].last().unwrap(), m.best().unwrap().point) < 1e-6, "the tail ends at the newest answer");
        assert_eq!(m.trace().len(), decided, "the tail is not stored");
        // After a break the tail is the start of the new run.
        m.restart();
        m.break_trace();
        walk(&mut m, destination(row, 90.0, 300.0), 90.0, 2.0, 2, 100);
        let runs = m.runs_with_tail();
        assert_eq!(runs.iter().map(Vec::len).collect::<Vec<_>>(), [10, 2], "the first run kept whole (ruling T19b-break): {runs:?}");
    }

    /// What a client holds after applying `d` to `runs` (the settled runs it had); the tail is drawn apart.
    fn apply(runs: &mut Vec<Vec<Point>>, d: &TraceDelta) {
        if d.reset {
            runs.clear();
        }
        let mut append = d.append.iter();
        if d.joins {
            if let (Some(last), Some(first)) = (runs.last_mut(), append.next()) {
                last.extend(first);
            }
        }
        runs.extend(append.cloned());
    }

    /// The kept part of the trace as runs: what [`Matcher::trace_since`] gives out (the simplifier's window goes with the tail).
    fn settled(m: &Matcher) -> Vec<Vec<Point>> {
        let mut out = Vec::new();
        let mut start = 0;
        for &b in m.breaks.iter().filter(|&&b| b <= m.kept).chain(std::iter::once(&m.kept)) {
            if b > start {
                out.push(m.trace[start..b].to_vec());
            }
            start = b;
        }
        out
    }

    #[test]
    fn deltas_append_only_what_settled_and_always_carry_the_tail() {
        // Task 19b: the map gets what changed, not the whole session trace.
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let first = m.trace_since(0);
        assert!(first.reset && first.append.is_empty() && first.tail.is_empty() && first.from_ms.is_none(), "{first:?}");
        let (mut client, mut cursor) = (vec![], first.cursor);
        // A zigzag in the middle of a block (off the streets, so every input is drawn as it is), so the simplifier keeps points as it
        // goes: three inputs per step, 4 m on and 8 m either side of the block's middle.
        let middle = destination(destination(o(), 0.0, 150.0), 90.0, 114.0);
        let p = LocParams::default();
        for k in 0..6 {
            let before = m.kept;
            for j in 0..3 {
                let i = 3 * k + j;
                let at = destination(destination(middle, 90.0, 4.0 * i64_to_f64(i)), 0.0, if i % 2 == 0 { -8.0 } else { 8.0 });
                m.push(&est(at, 5 * i), &p);
            }
            let d = m.trace_since(cursor);
            assert!(!d.reset, "step {k}: {d:?}");
            assert!(d.cursor >= cursor && (d.append.is_empty() || d.cursor > cursor), "the cursor only grows");
            assert_eq!(d.append.iter().map(Vec::len).sum::<usize>(), m.kept - before, "step {k}: only the newly kept points");
            if !d.append.is_empty() {
                assert_eq!(d.joins, before > 0, "step {k}: the new points carry on the line");
            }
            assert_eq!(d.from_ms, Some(0));
            apply(&mut client, &d);
            assert_eq!(client, settled(&m), "step {k}");
            // The tail (the simplifier's window, then the undecided inputs) starts at the newest kept point, so it draws joined, and ends
            // at the newest answer.
            assert_eq!(d.tail.len(), (m.trace().len() - m.kept) + LocParams::default().match_lag + usize::from(m.kept > 0));
            assert!(distance_m(d.tail[0], *client.concat().last().unwrap_or(&d.tail[0])) < 1e-9);
            assert!(distance_m(*d.tail.last().unwrap(), m.best().unwrap().point) < 1e-6);
            cursor = d.cursor;
        }
        assert!(m.kept > 2, "the zigzag kept its corners: {}", m.kept);
        let again = m.trace_since(cursor);
        assert!(!again.reset && again.append.is_empty() && again.cursor == cursor, "nothing new: {again:?}");
        assert!(!again.tail.is_empty(), "the tail is always sent");
    }

    #[test]
    fn a_new_run_arrives_as_its_own_segment() {
        let mut m = Matcher::default();
        let p = LocParams::default();
        for i in 0..5 {
            m.push(&est(destination(o(), 0.0, 7.0 * i64_to_f64(i)), i * 5), &p);
        }
        let mut client = vec![];
        let d = m.trace_since(0);
        apply(&mut client, &d);
        m.restart();
        m.break_trace(); // ruling T19b-break: nothing sent is rewritten
        let far = destination(o(), 90.0, 3000.0);
        for i in 0..3 {
            m.push(&est(destination(far, 0.0, 7.0 * i64_to_f64(i)), 100 + i * 5), &p);
        }
        let d = m.trace_since(d.cursor);
        // The first run's points the simplifier still held are given out as they were at the break (joined to what the client has);
        // the new run arrives as its own segment, its first point kept and the rest (undecided by the simplifier) in the tail.
        assert!(!d.reset && d.joins, "{d:?}");
        assert_eq!(d.append.iter().map(Vec::len).collect::<Vec<_>>(), [4, 1]);
        assert_eq!(d.tail.len(), 3, "the new run's start and its window: {d:?}");
        apply(&mut client, &d);
        assert_eq!(client, settled(&m));
        assert_eq!(client.len(), 2);
    }

    #[test]
    fn rewriting_what_was_sent_forces_a_reset() {
        let mut m = Matcher::default();
        let p = LocParams::default();
        for i in 0..5 {
            m.push(&est(destination(o(), 0.0, 7.0 * i64_to_f64(i)), i * 5), &p);
        }
        let sent = m.trace_since(0).cursor;
        m.restart();
        m.break_trace(); // ruling T19b-break: a break rewrites nothing
        assert!(!m.trace_since(sent).reset);
        // Only the memory backstop rewrites what was sent (final review F3): a trace nothing simplifies (a zigzag) past the cap.
        let n = i64::try_from(TRACE_CAP).unwrap() + 10;
        for i in 0..n {
            m.push(&est(destination(zigzag(i), 90.0, 30.0), 100 + i * 5), &p);
        }
        let d = m.trace_since(sent);
        assert!(d.reset && d.cursor > sent, "thinned");
        let mut client = vec![];
        apply(&mut client, &d);
        assert_eq!(client, settled(&m));
    }

    #[test]
    fn a_cursor_from_another_trace_forces_a_reset() {
        // Another game's matcher, a copy, a made-up cursor: never applied to this trace.
        let p = LocParams::default();
        let feed = |m: &mut Matcher| {
            for i in 0..4 {
                m.push(&est(destination(o(), 0.0, 7.0 * i64_to_f64(i)), i * 5), &p);
            }
        };
        let (mut a, mut b) = (Matcher::default(), Matcher::default());
        feed(&mut a);
        feed(&mut b);
        let ca = a.trace_since(0).cursor;
        assert!(!a.trace_since(ca).reset);
        assert!(b.trace_since(ca).reset, "another game's cursor");
        let copy = a.clone();
        assert!(copy.trace_since(ca).reset, "a copy is another trace");
        assert!(a.trace_since(ca + 1).reset, "past the end");
        assert!(a.trace_since(u64::MAX).reset);
    }

    /// The settled part of the trace given out by [`Matcher::trace_since`], as runs.
    fn given_out(m: &Matcher) -> Vec<Vec<Point>> {
        let mut client = vec![];
        apply(&mut client, &m.trace_since(0));
        client
    }

    #[test]
    fn the_trace_is_simplified_as_it_settles_and_nothing_given_out_changes() {
        // Final review F3: a streaming simplifier at decide time. A long straight walk keeps a handful of points, its corner exactly,
        // and every delta only appends: no cursor is ever reset, so the map never reloads.
        let mut m = Matcher::default();
        let p = LocParams::default();
        let corner = destination(o(), 0.0, 3000.0);
        let (mut client, mut cursor) = (vec![], 0);
        for i in 0..6000 {
            let at = if i < 3000 { destination(o(), 0.0, i64_to_f64(i)) } else { destination(corner, 90.0, i64_to_f64(i - 3000)) };
            m.push(&est(at, i), &p);
            if i % 10 == 0 {
                let d = m.trace_since(cursor);
                assert!(!d.reset || cursor == 0, "input {i}: given-out points changed");
                apply(&mut client, &d);
                cursor = d.cursor;
            }
        }
        assert!(m.trace().len() < 300, "{}", m.trace().len());
        assert!(m.trace().iter().any(|q| distance_m(*q, corner) < 1e-6), "the corner is kept");
        assert_eq!(client, given_out(&m), "the client holds what was given out, and it is still the trace");
        let line: Vec<Point> = client.concat().into_iter().chain(m.trace_since(cursor).tail).collect();
        let off = (0..6000).map(|i| if i < 3000 { destination(o(), 0.0, f64::from(i)) } else { destination(corner, 90.0, f64::from(i - 3000)) });
        let worst = off.map(|q| line.windows(2).map(|w| crate::geo::distance_to_segment_m(q, w[0], w[1])).fold(f64::INFINITY, f64::min)).fold(0.0, f64::max);
        assert!(worst <= TRACE_SIMPLIFY_M + 0.01, "the drawn line is within 2 m of every input: {worst}");
    }

    /// A zigzag 5 m either side of a line north: nothing in it can be simplified at 2 m.
    fn zigzag(i: i64) -> Point {
        destination(destination(o(), 0.0, i64_to_f64(i)), 90.0, if i % 2 == 0 { 5.0 } else { -5.0 })
    }

    #[test]
    fn past_the_memory_cap_the_trace_is_thinned_once_and_the_map_reloads() {
        // Final review F3: the cap is only a memory backstop. A trace nothing simplifies (a zigzag) is thinned at the cap, which forces
        // one reset; the first point and the newest are kept.
        let mut m = Matcher::default();
        let p = LocParams::default();
        let n = i64::try_from(TRACE_CAP).unwrap() + 10;
        let mut cursor = 0;
        let mut resets = 0;
        for i in 0..n {
            m.push(&est(zigzag(i), i), &p);
            assert!(m.trace().len() <= TRACE_CAP, "input {i}: {}", m.trace().len());
            let d = m.trace_since(cursor);
            resets += usize::from(d.reset && cursor != 0);
            cursor = d.cursor;
        }
        assert_eq!(resets, 1);
        assert!(distance_m(m.trace()[0], zigzag(0)) < 1e-6 && distance_m(*m.trace().last().unwrap(), zigzag(n - 1)) < 1e-6);
        assert_eq!(m.trace_from_ms(), Some(0));
    }

    #[test]
    #[ignore = "timing harness: cargo test --release -p apgo-core --lib trace_work -- --ignored --nocapture"]
    fn trace_work_per_input_stays_under_a_millisecond() {
        // Final review F3: the old cap simplification stalled one fix for about 15 ms (host) on a noisy 20 000-point trace.
        let p = LocParams::default();
        let mut worst = 0.0_f64;
        for noisy in [false, true] {
            let mut m = Matcher::default();
            let mut rng = 7_u64;
            for i in 0..(i64::try_from(TRACE_CAP).unwrap() * 2) {
                rng = rng.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let jitter = if noisy { i64_to_f64(i64::try_from(rng >> 60).unwrap()) } else { 0.0 };
                let at = destination(zigzag(i), 0.0, jitter);
                let t = std::time::Instant::now();
                m.push(&est(at, i), &p);
                worst = worst.max(t.elapsed().as_secs_f64() * 1000.0);
            }
        }
        println!("worst trace work per input: {worst:.3} ms");
        assert!(worst < 1.0, "{worst} ms");
    }

    #[test]
    fn the_street_transition_prefers_a_route_as_long_as_the_straight_line() {
        let same = street_transition_log(50.0, 50.0, 5.0, 0.0);
        let detour = street_transition_log(50.0, 150.0, 5.0, 0.0);
        assert!(same > detour + 15.0, "{same} vs {detour}");
        assert!((same - (-(5.0_f64).ln())).abs() < 1e-9);
    }

    #[test]
    fn a_graph_arriving_mid_session_starts_matching_and_a_rebuild_leaves_no_stale_segments() {
        // Ruling T17-optimistic: a game starts without a graph and gets one (and later maybe a rebuilt one) while the player walks.
        let row = destination(o(), 0.0, 100.0);
        let mut m = Matcher::new(None, mode_mask(Mode::Walk));
        assert!(walk(&mut m, row, 90.0, 2.0, 4, 0).is_none());
        assert_eq!((m.trace().len(), m.trace_from_ms()), (4, Some(0)));
        m.set_graph(Some(grid()));
        let out = walk(&mut m, destination(row, 90.0, 28.0), 90.0, 2.0, 6, 20).unwrap();
        assert!(out.seg.is_some() && out.confidence >= 0.7, "matching starts on the new graph: {out:?}");
        // A rebuild with fewer segments: one street only, so the old grid's ids would be out of range.
        let line = Arc::new(
            StreetGraph::from_ways(&[crate::scan::WayGeom { id: 1, class: crate::scan::way_class::FOOT, pts: vec![row, destination(row, 90.0, 400.0)] }])
                .unwrap(),
        );
        m.set_graph(Some(Arc::clone(&line)));
        assert!(m.best().is_none(), "the old graph's answer is gone");
        let before = m.trace().len();
        assert!(before >= 4 + 6, "the old lattice's decided inputs went into the trace first: {before}");
        let out = walk(&mut m, destination(row, 90.0, 80.0), 90.0, 2.0, 5, 60).unwrap();
        assert!(out.seg.is_some_and(|s| s < line.segment_count()), "{out:?}");
        assert_eq!(m.trace_from_ms(), Some(0), "one session trace");
    }

    /// The lowest confidence of a straight walk east along the y = 100 m street, `side_m` south of it, past three crossings.
    fn lowest_confidence_along_a_street(side_m: f64) -> (f64, i64) {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let p = LocParams::default();
        let row = destination(o(), 0.0, 100.0);
        let mut low = (1.0, 0);
        for i in 0..57 {
            let on = destination(row, 90.0, 7.0 * i64_to_f64(i));
            let out = m.push(&est(destination(on, 180.0, side_m), i * 5), &p).unwrap();
            assert!(out.seg.is_some(), "input {i}: {out:?}");
            if i >= 2 && out.confidence < low.0 {
                low = (out.confidence, 7 * i);
            }
        }
        low
    }

    #[test]
    fn a_straight_walk_on_a_street_stays_confident_through_every_crossing() {
        // Rulings T18-conf, T18-conf2: at a node the end of one segment, the start of the next and the cross street are one place.
        let (low, at) = lowest_confidence_along_a_street(0.0);
        assert!(low >= 0.7, "{low} at {at} m");
    }

    #[test]
    fn a_straight_walk_beside_a_street_stays_confident_through_every_crossing() {
        let (low, at) = lowest_confidence_along_a_street(2.0);
        assert!(low >= 0.7, "{low} at {at} m");
    }

    #[test]
    fn halfway_between_two_parallel_streets_is_not_confident() {
        let (a, b) = (o(), destination(o(), 0.0, 10.0));
        let way = |id: i64, from: Point| crate::scan::WayGeom { id, class: crate::scan::way_class::FOOT, pts: vec![from, destination(from, 90.0, 300.0)] };
        let mut m = Matcher::new(StreetGraph::from_ways(&[way(1, a), way(2, b)]).map(Arc::new), mode_mask(Mode::Walk));
        let out = walk(&mut m, destination(o(), 0.0, 5.0), 90.0, 0.0, 10, 0).unwrap();
        assert!(out.seg.is_some() && out.confidence < 0.7, "{out:?}");
    }

    #[test]
    fn bad_gps_never_merges_streets_ten_metres_apart() {
        // Ruling T18-conf2: the place radius grows with sigma_z but is capped (6.5 m), so sigma_z = 20 m keeps the streets apart.
        let (a, b) = (o(), destination(o(), 0.0, 10.0));
        let way = |id: i64, from: Point| crate::scan::WayGeom { id, class: crate::scan::way_class::FOOT, pts: vec![from, destination(from, 90.0, 300.0)] };
        let mut m = Matcher::new(StreetGraph::from_ways(&[way(1, a), way(2, b)]).map(Arc::new), mode_mask(Mode::Walk));
        let p = LocParams::default();
        let mut out = None;
        for i in 0..10 {
            let at = destination(destination(o(), 0.0, 5.0), 90.0, 7.0 * i64_to_f64(i));
            out = m.push(&Estimate { uncertainty_m: 20.0 * ACC_TO_SIGMA, ..est(at, i * 5) }, &p);
        }
        assert!(out.is_some());
        let last = m.columns.back().unwrap();
        let streets: Vec<f64> = last.iter().filter(|s| s.cand.is_some()).map(|s| s.conf).collect();
        assert!(streets.len() == 2 && streets.iter().all(|c| *c < 0.7), "neither street is sure: {streets:?}");
    }

    #[test]
    fn a_new_mode_restarts_the_lattice() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        walk(&mut m, destination(o(), 0.0, 100.0), 90.0, 2.0, 5, 0);
        m.set_mask(mode_mask(Mode::Walk));
        assert!(m.best().is_some(), "the same mask changes nothing");
        m.set_mask(mode_mask(Mode::Drive));
        assert!(m.best().is_none());
        assert_eq!(m.trace().len(), 5, "what the lattice held went into the trace");
    }

    #[test]
    fn a_jump_to_another_street_lands_on_it() {
        // 300 m in 5 s: the off-network state carries the lattice over, and the new street wins at once.
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        walk(&mut m, destination(o(), 0.0, 100.0), 90.0, 2.0, 5, 0);
        let out = walk(&mut m, destination(destination(o(), 0.0, 400.0), 90.0, 50.0), 90.0, 2.0, 2, 25).unwrap();
        let north = (out.point.lat - destination(o(), 0.0, 400.0).lat).abs() * 111_195.0;
        assert!(out.seg.is_some() && north < 0.5, "{north} m off the top street: {out:?}");
    }

    #[test]
    fn turning_back_on_the_same_street_follows_the_player() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let row = destination(o(), 0.0, 100.0);
        walk(&mut m, destination(row, 90.0, 20.0), 90.0, 2.0, 8, 0); // east to x = 69 m
        let out = walk(&mut m, destination(row, 90.0, 62.0), 270.0, -2.0, 6, 40).unwrap(); // back west to x = 27 m
        let x = distance_m(Point::new(row.lat, out.point.lon), row);
        assert!(out.seg.is_some() && (x - 27.0).abs() < 3.0, "{x} m along: {out:?}");
    }

    /// Bytes the lattice and the route cache hold (the trace is reported apart: it grows with the session).
    fn lattice_bytes(m: &Matcher) -> usize {
        let cols: usize = m.columns.iter().map(|c| c.capacity() * size_of::<State>()).sum::<usize>() + m.columns.capacity() * size_of::<Vec<State>>();
        let cache: usize = m.cache.values().map(|v| v.capacity() * size_of::<(u32, f32)>() + size_of::<Vec<(u32, f32)>>()).sum::<usize>()
            + m.cache.capacity() * (size_of::<usize>() + size_of::<Vec<(u32, f32)>>() + 1);
        size_of::<Matcher>() + cols + cache
    }

    #[test]
    fn the_lattice_stays_small_on_a_long_walk() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let mut most = 0;
        for leg in 0..4_i32 {
            let start = destination(o(), 0.0, 100.0 * f64::from(leg));
            walk(&mut m, start, 90.0, 2.0, 57, i64::from(leg) * 300);
            most = most.max(lattice_bytes(&m));
        }
        assert!(most < 100_000, "{most} bytes");
    }

    #[test]
    #[ignore = "timing harness: cargo test --release -p apgo-core hmm_step_time -- --ignored --nocapture"]
    fn hmm_step_time() {
        // The 10 km-radius probe: a 20 km square of streets 100 m apart; a walk, a drive and a screen-off drive (a fix per 5 s) across it.
        let sw = destination(destination(o(), 180.0, 10_000.0), 270.0, 10_000.0);
        let graph = Arc::new(StreetGraph::from_ways(&grid_ways(sw, 201, 100.0)).unwrap());
        let params = LocParams::default();
        for (name, mode, step_m, dt_s) in [("walk", Mode::Walk, 5.0, 4), ("drive", Mode::Drive, 20.0, 1), ("screen-off drive", Mode::Drive, 75.0, 5)] {
            let mut m = Matcher::new(Some(Arc::clone(&graph)), mode_mask(mode));
            let (mut times, mut most, mut confident) = (vec![], 0, 0);
            // A staircase of blocks along the streets (300 m east, 200 m north, repeated) from the middle, with up to 3 m of noise. Points
            // are interpolated between the grid's own crossings (the grid's rows are great circles, not parallels).
            let node = |i: usize, j: usize| destination(destination(sw, 90.0, 100.0 * count_f64(i)), 0.0, 100.0 * count_f64(j));
            let mut corners = vec![(60_usize, 60_usize)];
            while corners.len() < 81 {
                let (i, j) = corners[corners.len() - 1];
                corners.push((i + 3, j));
                corners.push((i + 3, j + 2));
            }
            let along: Vec<Point> = corners
                .windows(2)
                .flat_map(|w| {
                    let (pa, pb) = (node(w[0].0, w[0].1), node(w[1].0, w[1].1));
                    let n = crate::num::round_i64(distance_m(pa, pb) / step_m);
                    (0..n).map(move |k| {
                        let f = i64_to_f64(k) / i64_to_f64(n);
                        Point::new(pa.lat + (pb.lat - pa.lat) * f, pa.lon + (pb.lon - pa.lon) * f)
                    })
                })
                .collect();
            let mut clock_s = 0_i64;
            for (step_no, at) in (0_i64..).zip(along.iter().cycle().take(4_000)) {
                let noisy = destination(*at, i64_to_f64((step_no * 97) % 360), i64_to_f64((step_no * 13) % 4));
                let fix = Estimate { uncertainty_m: 6.0, speed_mps: step_m / i64_to_f64(dt_s), ..Estimate::exact(noisy.lat, noisy.lon, clock_s * 1000) };
                clock_s += dt_s;
                let started = std::time::Instant::now();
                let out = m.push(&fix, &params);
                if m.last_input.is_some_and(|l| l.1 == fix.t_ms) {
                    times.push(started.elapsed().as_secs_f64() * 1e6); // a lattice step, not an input the 2 m / 1 s rule skipped
                }
                confident += usize::from(out.is_some_and(|o| o.seg.is_some() && o.confidence >= 0.7));
                most = most.max(lattice_bytes(&m));
            }
            times.sort_by(f64::total_cmp);
            let mean = times.iter().sum::<f64>() / count_f64(times.len());
            let p99 = times[times.len() * 99 / 100];
            println!(
                "{name}: {} lattice steps ({confident} of 4000 pushes confidently matched), mean {mean:.1} us, p99 {p99:.1} us, max {:.1} us; lattice + route cache at most {most} bytes; trace {} points ({} bytes)",
                times.len(),
                times[times.len() - 1],
                m.trace().len(),
                size_of_val(m.trace())
            );
        }
    }
}
