//! The street graph the map matcher and the gap bridge walk on: one graph of the scans' way geometry (`Atlas::ways`), plus, for atlases
//! scanned before that was recorded, degraded links from their street samples with run ends joined within 12 m.
//!
//! Stored compactly (`u32` ids, `f32` coordinates, adjacency and the cell index as flat CSR arrays), so a large suburban realm stays
//! within the spec's memory budget.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::catalog::Mode;
use crate::geo::Point;
use crate::loc::frame::Frame;
use crate::num::{count_u32, floor_i32, round_i64, round_u64, to_f32};
use crate::scan::{way_class, Atlas, WayGeom};

/// Grid cell size of the segment index, metres. Measured against 30 m (ruling T17-R1): 80 m keeps a 10 km suburb at about 4 MB instead
/// of 7 MB, and its candidate lookups are no slower.
pub const CELL_M: f64 = 80.0;
/// [`StreetGraph::candidates`] never searches farther than this, metres (a huge or infinite radius would walk millions of cells).
pub const MAX_RADIUS_M: f64 = 1000.0;
/// A degraded graph joins a run end to any node this close, metres.
pub const JOIN_M: f64 = 12.0;

/// The way classes a travel mode may use.
#[must_use]
pub fn mode_mask(mode: Mode) -> u8 {
    match mode {
        Mode::Walk | Mode::Run => way_class::FOOT,
        Mode::Bike => way_class::BIKE,
        Mode::Drive => way_class::CAR,
    }
}

/// [`StreetGraph::along`] goes on past a segment's end only onto a segment that turns at most this much, degrees.
pub const STRAIGHT_ON_DEG: f64 = 30.0;

/// [`StreetGraph::along`] carries a point on over at most this many segments past the first ...
pub const CARRY_SEGMENTS: usize = 8;
/// ... or this many metres of them.
pub const CARRY_M: f64 = 30.0;

/// A straight piece of street between two nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// First node.
    pub a: usize,
    /// Second node.
    pub b: usize,
    /// Length, metres.
    pub len_m: f64,
    /// [`way_class`] bits.
    pub class: u8,
}

/// A point projected onto a segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cand {
    /// The segment.
    pub seg: usize,
    /// Metres from the segment's node `a`.
    pub off_m: f64,
    /// Distance from the point to the projection, metres.
    pub d_m: f64,
}

/// A segment as stored.
#[derive(Debug, Clone, Copy)]
struct Packed {
    a: u32,
    b: u32,
    len_m: f32,
    class: u8,
}

/// Nodes, segments and a grid index, in a local metric frame.
#[derive(Debug, Clone)]
pub struct StreetGraph {
    frame: Frame,
    nodes: Vec<[f32; 2]>,
    segs: Vec<Packed>,
    /// Segments at node `n`: `adj[adj_off[n]..adj_off[n + 1]]`.
    adj_off: Vec<u32>,
    adj: Vec<u32>,
    /// Sorted packed cell keys; segments in cell `cell_keys[i]`: `cell_segs[cell_off[i]..cell_off[i + 1]]`.
    cell_keys: Vec<u64>,
    cell_off: Vec<u32>,
    cell_segs: Vec<u32>,
    degraded: bool,
}

fn cell(en: [f64; 2]) -> (i32, i32) {
    (floor_i32(en[0] / CELL_M), floor_i32(en[1] / CELL_M))
}

/// Every cell the segment `pa`-`pb` passes through (a grid traversal; where it passes a cell corner exactly, both side cells too), so
/// each point of it lies in one of them.
fn cells_on(pa: [f64; 2], pb: [f64; 2], mut visit: impl FnMut((i32, i32))) {
    let (mut c, end) = (cell(pa), cell(pb));
    visit(c);
    let dir = |d: f64| match d {
        d if d > 0.0 => 1,
        d if d < 0.0 => -1,
        _ => 0,
    };
    let (d, step) = ([pb[0] - pa[0], pb[1] - pa[1]], [dir(pb[0] - pa[0]), dir(pb[1] - pa[1])]);
    // Line parameter at the next cell boundary on each axis, and how much it grows per cell.
    let first = |k: usize, ci: i32| {
        if step[k] == 0 {
            return f64::INFINITY;
        }
        let boundary = CELL_M * f64::from(if step[k] > 0 { ci + 1 } else { ci });
        (boundary - pa[k]) / d[k]
    };
    let mut t = [first(0, c.0), first(1, c.1)];
    let per = [CELL_M / d[0].abs(), CELL_M / d[1].abs()];
    while c != end {
        // Never step past the end cell on an axis (rounding cannot make the walk overshoot or loop).
        let (x_done, y_done) = (c.0 == end.0, c.1 == end.1);
        if !x_done && !y_done && (t[0] - t[1]).abs() <= 1e-9 * t[0].abs().max(1.0) {
            visit((c.0 + step[0], c.1));
            visit((c.0, c.1 + step[1]));
            c = (c.0 + step[0], c.1 + step[1]);
            t = [t[0] + per[0], t[1] + per[1]];
        } else if !x_done && (y_done || t[0] < t[1]) {
            c.0 += step[0];
            t[0] += per[0];
        } else {
            c.1 += step[1];
            t[1] += per[1];
        }
        visit(c);
    }
}

fn cell_key((x, y): (i32, i32)) -> u64 {
    (u64::from(x.cast_unsigned()) << 32) | u64::from(y.cast_unsigned())
}

fn key(p: Point) -> (i64, i64) {
    (round_i64(p.lat * 1e7), round_i64(p.lon * 1e7))
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn wide(en: [f32; 2]) -> [f64; 2] {
    [f64::from(en[0]), f64::from(en[1])]
}

fn idx(i: u32) -> usize {
    i as usize
}

/// Every class may use a degraded link: the samples do not say who may use the street.
const ALL: u8 = way_class::FOOT | way_class::BIKE | way_class::CAR;

/// Collects nodes and segments, then packs them.
struct Builder {
    frame: Frame,
    keys: HashMap<(i64, i64), u32>,
    nodes: Vec<[f32; 2]>,
    segs: Vec<Packed>,
    /// Whether segment `i` is a degraded link (its loose ends get joined).
    loose: Vec<bool>,
    /// Segment id by its unordered node pair: a piece of street seen twice (one way in two atlases) is one segment.
    pairs: HashMap<(u32, u32), u32>,
}

impl Builder {
    fn new(origin: Point) -> Self {
        Self { frame: Frame::new(origin), keys: HashMap::new(), nodes: vec![], segs: vec![], loose: vec![], pairs: HashMap::new() }
    }

    fn node(&mut self, p: Point) -> u32 {
        if let Some(&n) = self.keys.get(&key(p)) {
            return n;
        }
        let en = self.frame.to_enu(p);
        self.nodes.push([to_f32(en[0]), to_f32(en[1])]);
        let n = count_u32(self.nodes.len() - 1);
        self.keys.insert(key(p), n);
        n
    }

    /// Add the segment `a`-`b`, or, if it is already there, let `class` use it too (see the rule for degraded links below).
    fn seg(&mut self, a: u32, b: u32, class: u8, loose: bool) {
        if a == b {
            return;
        }
        let id = count_u32(self.segs.len());
        let known = *self.pairs.entry((a.min(b), a.max(b))).or_insert(id);
        if known != id {
            // A way's class is the truth: a degraded link (every class) on the same nodes never widens it, and a way replaces one. Copies
            // join their modes, but the piece is a sidewalk only if every copy is.
            let k = idx(known);
            match (self.loose[k], loose) {
                (false, true) => {}
                (true, false) => (self.segs[k].class, self.loose[k]) = (class, false),
                _ => {
                    let old = self.segs[k].class;
                    self.segs[k].class = ((old | class) & !way_class::SIDE) | (old & class & way_class::SIDE);
                }
            }
            return;
        }
        let len_m = to_f32(dist(wide(self.nodes[idx(a)]), wide(self.nodes[idx(b)])));
        self.segs.push(Packed { a, b, len_m, class });
        self.loose.push(loose);
    }

    fn way(&mut self, w: &WayGeom) {
        let ids: Vec<u32> = w.pts.iter().map(|p| self.node(*p)).collect();
        for pair in ids.windows(2) {
            self.seg(pair[0], pair[1], w.class, false);
        }
    }

    fn link(&mut self, p: Point, q: Point) {
        let (a, b) = (self.node(p), self.node(q));
        self.seg(a, b, ALL, true);
    }

    /// Join each end of a degraded run (a node on one link and nothing else) to the nearest other node within [`JOIN_M`].
    fn join_loose_ends(&mut self) {
        if !self.loose.contains(&true) {
            return;
        }
        let mut degree = vec![0_u32; self.nodes.len()];
        let mut other = vec![0_u32; self.nodes.len()];
        let mut on_loose = vec![false; self.nodes.len()];
        for (s, loose) in self.segs.iter().zip(&self.loose) {
            for (n, m) in [(s.a, s.b), (s.b, s.a)] {
                degree[idx(n)] += 1;
                other[idx(n)] = m;
                on_loose[idx(n)] |= *loose;
            }
        }
        let mut by_cell: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        for (i, n) in self.nodes.iter().enumerate() {
            by_cell.entry(cell(wide(*n))).or_default().push(count_u32(i));
        }
        for e in (0..self.nodes.len()).filter(|&i| degree[i] == 1 && on_loose[i]).map(count_u32) {
            let pe = wide(self.nodes[idx(e)]);
            let (cx, cy) = cell(pe);
            let best = (cx - 1..=cx + 1)
                .flat_map(|x| (cy - 1..=cy + 1).map(move |y| (x, y)))
                .filter_map(|c| by_cell.get(&c))
                .flatten()
                .copied()
                .filter(|&n| n != e && n != other[idx(e)])
                .map(|n| (n, dist(pe, wide(self.nodes[idx(n)]))))
                .filter(|(_, d)| *d <= JOIN_M)
                .min_by(|x, y| x.1.total_cmp(&y.1));
            if let Some((n, _)) = best {
                self.seg(e, n, ALL, true);
            }
        }
    }

    /// The packed graph; `None` without segments.
    fn finish(mut self, degraded: bool) -> Option<StreetGraph> {
        if self.segs.is_empty() {
            return None;
        }
        self.join_loose_ends();
        let (nodes, mut segs) = (self.nodes, self.segs);
        drop((self.pairs, self.keys));
        segs.shrink_to_fit();
        let mut adj_off = vec![0_u32; nodes.len() + 1];
        for s in &segs {
            adj_off[idx(s.a) + 1] += 1;
            adj_off[idx(s.b) + 1] += 1;
        }
        for i in 1..adj_off.len() {
            adj_off[i] += adj_off[i - 1];
        }
        let mut fill = adj_off.clone();
        let mut adj = vec![0_u32; 2 * segs.len()];
        for (id, s) in segs.iter().enumerate() {
            for n in [s.a, s.b] {
                adj[idx(fill[idx(n)])] = count_u32(id);
                fill[idx(n)] += 1;
            }
        }
        let mut pairs: Vec<(u64, u32)> = Vec::new();
        for (id, s) in segs.iter().enumerate() {
            let (pa, pb) = (wide(nodes[idx(s.a)]), wide(nodes[idx(s.b)]));
            cells_on(pa, pb, |c| pairs.push((cell_key(c), count_u32(id))));
        }
        pairs.sort_unstable();
        pairs.dedup();
        let mut cell_keys = Vec::new();
        let mut cell_off = Vec::new();
        for (i, (k, _)) in pairs.iter().enumerate() {
            if cell_keys.last() != Some(k) {
                cell_keys.push(*k);
                cell_off.push(count_u32(i));
            }
        }
        cell_off.push(count_u32(pairs.len()));
        let cell_segs = pairs.into_iter().map(|(_, s)| s).collect();
        Some(StreetGraph { frame: self.frame, nodes, segs, adj_off, adj, cell_keys, cell_off, cell_segs, degraded })
    }
}

impl StreetGraph {
    /// The graph of `ways` (each consecutive pair of points a segment; equal points are one node). `None` without ways.
    #[must_use]
    pub fn from_ways(ways: &[WayGeom]) -> Option<Self> {
        let mut b = Builder::new(*ways.iter().find_map(|w| w.pts.first())?);
        for w in ways {
            b.way(w);
        }
        b.finish(false)
    }

    /// A degraded graph from street sample links (old atlases): each link a segment usable by every mode, and every run end joined to the
    /// nearest other node within [`JOIN_M`]. `None` without links.
    #[must_use]
    pub fn degraded(links: &[(Point, Point)]) -> Option<Self> {
        let mut b = Builder::new(links.first()?.0);
        for (p, q) in links {
            b.link(*p, *q);
        }
        b.finish(true)
    }

    /// One graph of a game's zones: every atlas's ways (each atlas's copy of a way, so a junction one copy simplified away is kept; a
    /// piece of street in two atlases is one segment), or, for an atlas scanned before ways were recorded, its street links as degraded
    /// segments (joined to any node nearby, so the old zone connects to the rest). Degraded when any part is. `None` without streets.
    #[must_use]
    pub fn for_atlases(atlases: &[&Atlas]) -> Option<Self> {
        let links: Vec<Vec<(Point, Point)>> =
            atlases.iter().map(|a| if a.ways.is_empty() { a.street_links(false).into_iter().chain(a.street_links(true)).collect() } else { vec![] }).collect();
        let origin = atlases.iter().flat_map(|a| a.ways.iter()).find_map(|w| w.pts.first().copied()).or_else(|| links.iter().flatten().map(|l| l.0).next())?;
        let mut b = Builder::new(origin);
        for (a, links) in atlases.iter().zip(&links) {
            for w in &a.ways {
                b.way(w);
            }
            for (p, q) in links {
                b.link(*p, *q);
            }
        }
        b.finish(links.iter().any(|l| !l.is_empty()))
    }

    /// Whether any part of this graph is degraded (matching confidence is capped).
    #[must_use]
    pub fn is_degraded(&self) -> bool {
        self.degraded
    }

    /// The graph's metric frame.
    #[must_use]
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Number of segments.
    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.segs.len()
    }

    /// A segment.
    #[must_use]
    pub fn seg(&self, id: usize) -> Segment {
        let s = self.segs[id];
        Segment { a: idx(s.a), b: idx(s.b), len_m: f64::from(s.len_m), class: s.class }
    }

    /// A node's position in the frame.
    #[must_use]
    pub fn node_en(&self, id: usize) -> [f64; 2] {
        wide(self.nodes[id])
    }

    /// The point `off_m` metres from node `a` along a segment, in the frame.
    #[must_use]
    pub fn en_at(&self, seg: usize, off_m: f64) -> [f64; 2] {
        let s = self.seg(seg);
        let (a, b) = (self.node_en(s.a), self.node_en(s.b));
        let t = if s.len_m > 0.0 { (off_m / s.len_m).clamp(0.0, 1.0) } else { 0.0 };
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }

    /// [`Self::en_at`] as a map point.
    #[must_use]
    pub fn geo_at(&self, seg: usize, off_m: f64) -> Point {
        self.frame.to_geo(self.en_at(seg, off_m))
    }

    /// Bearing of travel along a segment (`dir` +1 from `a` to `b`, -1 back), degrees from north.
    #[must_use]
    pub fn bearing_deg(&self, seg: usize, dir: f64) -> f64 {
        let s = self.seg(seg);
        let (a, b) = (self.node_en(s.a), self.node_en(s.b));
        let (de, dn) = if dir >= 0.0 { (b[0] - a[0], b[1] - a[1]) } else { (a[0] - b[0], a[1] - b[1]) };
        de.atan2(dn).to_degrees().rem_euclid(360.0)
    }

    fn at_node(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        self.adj[idx(self.adj_off[node])..idx(self.adj_off[node + 1])].iter().map(|&s| idx(s))
    }

    /// The segments at `node` usable with `mask`, each with the direction that leads away from the node.
    #[must_use]
    pub fn leaving(&self, node: usize, mask: u8) -> Vec<(usize, f64)> {
        self.at_node(node).filter(|&s| self.segs[s].class & mask != 0).map(|s| (s, if idx(self.segs[s].a) == node { 1.0 } else { -1.0 })).collect()
    }

    fn in_cell(&self, c: (i32, i32)) -> &[u32] {
        match self.cell_keys.binary_search(&cell_key(c)) {
            Ok(i) => &self.cell_segs[idx(self.cell_off[i])..idx(self.cell_off[i + 1])],
            Err(_) => &[],
        }
    }

    /// Projections of `p` onto the segments within `radius_m` (clamped to 0..[`MAX_RADIUS_M`]) usable with `mask`, nearest first, at most
    /// `max` (one per segment).
    #[must_use]
    pub fn candidates(&self, p: Point, radius_m: f64, max: usize, mask: u8) -> Vec<Cand> {
        debug_assert!(radius_m.is_finite(), "candidate radius {radius_m}");
        // Release builds clamp what a debug build rejects: infinity to the maximum, NaN to nothing.
        let radius_m = if radius_m.is_nan() { 0.0 } else { radius_m.clamp(0.0, MAX_RADIUS_M) };
        let en = self.frame.to_enu(p);
        let (c0, c1) = (cell([en[0] - radius_m, en[1] - radius_m]), cell([en[0] + radius_m, en[1] + radius_m]));
        let mut segs: Vec<usize> = (c0.0..=c1.0).flat_map(|x| (c0.1..=c1.1).map(move |y| (x, y))).flat_map(|c| self.in_cell(c)).map(|&s| idx(s)).collect();
        segs.sort_unstable();
        segs.dedup();
        let mut out: Vec<Cand> =
            segs.into_iter().filter(|&s| self.segs[s].class & mask != 0).map(|s| self.project_en(s, en)).filter(|c| c.d_m <= radius_m).collect();
        out.sort_by(|x, y| x.d_m.total_cmp(&y.d_m));
        out.truncate(max);
        out
    }

    /// The projection of `en` (in the frame) onto segment `seg`, clamped to it.
    fn project_en(&self, seg: usize, en: [f64; 2]) -> Cand {
        let sg = self.seg(seg);
        let (a, b) = (self.node_en(sg.a), self.node_en(sg.b));
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 { (((en[0] - a[0]) * dx + (en[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let proj = [a[0] + dx * t, a[1] + dy * t];
        Cand { seg, off_m: t * sg.len_m, d_m: dist(en, proj) }
    }

    /// The point of the street at `seg` nearest `p`. While `p` lies past the far end of the segment reached, it goes on along the segment
    /// there (usable with `mask`) that continues most nearly straight, within [`STRAIGHT_ON_DEG`], for at most [`CARRY_SEGMENTS`] segments
    /// or [`CARRY_M`] metres; with none, it stays at the end (ruling T19-R2, review I1).
    #[must_use]
    pub fn along(&self, seg: usize, p: Point, mask: u8) -> Point {
        let en = self.frame.to_enu(p);
        let mut c = self.project_en(seg, en);
        let mut came_from = None; // the node `c`'s segment was entered by
        let mut carried_m = 0.0;
        for _ in 0..CARRY_SEGMENTS {
            let s = self.seg(c.seg);
            let end = if c.off_m <= 0.0 && came_from != Some(s.a) {
                (s.a, self.bearing_deg(c.seg, -1.0))
            } else if c.off_m >= s.len_m && came_from != Some(s.b) {
                (s.b, self.bearing_deg(c.seg, 1.0))
            } else {
                break;
            };
            if came_from.is_some() {
                carried_m += s.len_m;
            }
            let next = self
                .leaving(end.0, mask)
                .into_iter()
                .filter(|&(n, _)| n != c.seg)
                .map(|(n, dir)| (n, crate::loc::heading::wrap_deg(self.bearing_deg(n, dir) - end.1).abs()))
                .filter(|&(_, turn)| turn <= STRAIGHT_ON_DEG)
                .min_by(|x, y| x.1.total_cmp(&y.1));
            let Some((n, _)) = next.filter(|_| carried_m < CARRY_M) else { break };
            c = self.project_en(n, en);
            came_from = Some(end.0);
        }
        self.frame.to_geo(self.en_at(c.seg, c.off_m))
    }

    /// Shortest street distances from `from` to every node within `limit_m` (segments usable with `mask`).
    #[must_use]
    pub fn dijkstra(&self, from: usize, limit_m: f64, mask: u8) -> HashMap<usize, f64> {
        let mut best: HashMap<usize, f64> = HashMap::new();
        let mut heap = BinaryHeap::new();
        heap.push(Reverse((0_u64, from)));
        while let Some(Reverse((mm, n))) = heap.pop() {
            let d = crate::num::i64_to_f64(i64::try_from(mm).unwrap_or(i64::MAX)) / 1000.0;
            if d > limit_m || best.contains_key(&n) {
                continue;
            }
            best.insert(n, d);
            for s in self.at_node(n).filter(|&s| self.segs[s].class & mask != 0) {
                let sg = self.seg(s);
                let m = if sg.a == n { sg.b } else { sg.a };
                if !best.contains_key(&m) {
                    heap.push(Reverse((round_u64((d + sg.len_m) * 1000.0), m)));
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;
    use crate::scan::way_class::{BIKE, CAR, FOOT};

    fn o() -> Point {
        Point::new(40.0, -111.0)
    }

    fn way(id: i64, class: u8, pts: Vec<Point>) -> WayGeom {
        WayGeom { id, class, pts }
    }

    #[test]
    fn a_point_is_placed_on_its_segment_and_carried_straight_through_a_crossing() {
        // Ruling T19-R2: the pin follows the current estimate along the matched street, and past the segment's end onto the street
        // that continues straight, never onto the cross street.
        let g = StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 5, 100.0)).unwrap();
        let row = destination(o(), 0.0, 100.0);
        let x = |p: Point| g.frame().to_enu(p);
        let seg = g.candidates(destination(destination(row, 90.0, 50.0), 180.0, 3.0), 10.0, 1, FOOT)[0].seg;
        let on = g.along(seg, destination(destination(row, 90.0, 60.0), 180.0, 3.0), FOOT);
        assert!((x(on)[0] - x(destination(row, 90.0, 60.0))[0]).abs() < 0.1 && (x(on)[1] - x(row)[1]).abs() < 0.1, "{:?}", x(on));
        // 4 m past the crossing at x = 100 m, 3 m south of the row and 1 m from the cross street: straight on, along the row.
        let past = g.along(seg, destination(destination(row, 90.0, 104.0), 180.0, 3.0), FOOT);
        let want = x(destination(row, 90.0, 104.0));
        assert!((x(past)[0] - want[0]).abs() < 0.1 && (x(past)[1] - x(row)[1]).abs() < 0.1, "{:?} vs {want:?}", x(past));
        // Review I1: on to the segment after next, one straight step at a time, at most 8 segments or 30 m.
        let short = way(1, FOOT, (0..20).map(|k| destination(o(), 90.0, 3.0 * f64::from(k))).collect());
        let gs = StreetGraph::from_ways(&[short]).unwrap();
        let first = gs.candidates(destination(o(), 90.0, 1.0), 2.0, 1, FOOT)[0].seg;
        let xs = |p: Point| gs.frame().to_enu(p)[0];
        assert!((xs(gs.along(first, destination(o(), 90.0, 10.0), FOOT)) - 10.0).abs() < 0.1, "three segments on");
        assert!((xs(gs.along(first, destination(o(), 90.0, 50.0), FOOT)) - 27.0).abs() < 0.1, "stops after 8 more segments (24 m)");
        // At the grid's edge nothing continues: the point stays at the segment's end.
        let edge = g.candidates(destination(row, 90.0, 395.0), 10.0, 1, FOOT)[0].seg;
        let beyond = g.along(edge, destination(row, 90.0, 410.0), FOOT);
        assert!((x(beyond)[0] - x(destination(row, 90.0, 400.0))[0]).abs() < 0.1, "{:?}", x(beyond));
    }

    fn tee() -> StreetGraph {
        let j = destination(o(), 90.0, 100.0);
        StreetGraph::from_ways(&[way(1, FOOT | BIKE | CAR, vec![o(), j, destination(o(), 90.0, 200.0)]), way(2, FOOT, vec![j, destination(j, 0.0, 100.0)])])
            .unwrap()
    }

    #[test]
    fn ways_sharing_a_point_share_a_node() {
        let g = tee();
        assert_eq!(g.segment_count(), 3);
        assert_eq!(g.leaving(1, FOOT).len(), 3, "the junction joins three segments");
        assert_eq!(g.leaving(1, CAR).len(), 2, "the footway is not for cars");
    }

    #[test]
    fn candidates_are_projections_nearest_first_filtered_by_mode() {
        let g = tee();
        let p = destination(destination(o(), 90.0, 100.0), 0.0, 6.0); // 6 m up the footway, on the main street's junction too
        let c = g.candidates(p, 30.0, 8, FOOT);
        assert!(c.len() >= 2 && c[0].d_m <= c[1].d_m);
        assert!(c[0].d_m < 0.5, "on the footway: {c:?}");
        let cars = g.candidates(p, 30.0, 8, CAR);
        assert!(cars.iter().all(|x| g.seg(x.seg).class & CAR != 0) && (cars[0].d_m - 6.0).abs() < 0.5);
    }

    #[test]
    fn dijkstra_follows_the_streets() {
        let g = tee();
        let d = g.dijkstra(0, 1000.0, FOOT);
        assert!((d[&3] - 200.0).abs() < 0.5, "from the west end to the footway's end: 100 + 100 m, {d:?}");
        assert!(!g.dijkstra(0, 50.0, FOOT).contains_key(&3), "bounded");
    }

    #[test]
    fn bearings_point_along_the_segment() {
        let g = tee();
        assert!((g.bearing_deg(0, 1.0) - 90.0).abs() < 0.5 && (g.bearing_deg(0, -1.0) - 270.0).abs() < 0.5);
    }

    #[test]
    fn a_degraded_graph_joins_run_ends_within_twelve_metres_only() {
        let a = [(o(), destination(o(), 90.0, 60.0))];
        let near = destination(o(), 90.0, 68.0);
        let far = destination(o(), 90.0, 90.0);
        let g = StreetGraph::degraded(&[a[0], (near, destination(near, 90.0, 60.0)), (far, destination(far, 0.0, 60.0))]).unwrap();
        assert!(g.is_degraded());
        assert!(g.dijkstra(0, 1000.0, FOOT).len() >= 4, "the 8 m gap is joined");
        let d = g.dijkstra(0, 1000.0, FOOT);
        assert!(d.values().all(|x| *x < 140.0), "the run 30 m away is not reachable");
    }

    #[test]
    fn atlases_without_ways_get_the_degraded_graph_and_empty_ones_none() {
        let with = Atlas { ways: vec![way(1, FOOT, vec![o(), destination(o(), 0.0, 50.0)])], ..Atlas::default() };
        assert!(!StreetGraph::for_atlases(&[&with]).unwrap().is_degraded());
        let pts: Vec<Point> = (0..3).map(|i| destination(o(), 90.0, 60.0 * f64::from(i))).collect();
        let old = Atlas { streets: pts, street_runs: vec![3], street_stride: 1, ..Atlas::default() };
        assert!(StreetGraph::for_atlases(&[&with, &old]).unwrap().is_degraded());
        assert!(StreetGraph::for_atlases(&[&Atlas::default()]).is_none());
    }

    #[test]
    fn a_synthetic_grid_is_connected() {
        let g = StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 5, 100.0)).unwrap();
        assert_eq!(g.segment_count(), 2 * 5 * 4);
        assert_eq!(g.dijkstra(0, 10_000.0, FOOT).len(), 25);
    }

    #[test]
    fn a_modern_atlas_next_to_an_old_one_keeps_its_full_geometry() {
        // T17-R3: one graph, full ways for the modern zone, degraded links only for the old one.
        let bend = destination(o(), 0.0, 40.0);
        let modern = Atlas { ways: vec![way(1, FOOT, vec![o(), bend, destination(bend, 90.0, 40.0)])], ..Atlas::default() };
        let far = destination(o(), 90.0, 5_000.0);
        let pts: Vec<Point> = (0..3).map(|i| destination(far, 90.0, 60.0 * f64::from(i))).collect();
        let old = Atlas { streets: pts, street_runs: vec![3], street_stride: 1, ..Atlas::default() };
        let g = StreetGraph::for_atlases(&[&modern, &old]).unwrap();
        assert!(g.is_degraded(), "a part is degraded");
        assert_eq!(g.segment_count(), 2 + 2, "the footway's two segments and the old zone's two links");
        assert!(g.candidates(bend, 5.0, 8, CAR).is_empty(), "the footway keeps its class: no cars");
        assert_eq!(g.candidates(bend, 5.0, 8, FOOT).len(), 2, "both segments of the bend");
        assert_eq!(g.candidates(far, 5.0, 8, CAR).len(), 1, "an old link is open to every mode");
    }

    #[test]
    fn a_way_simplified_differently_in_two_atlases_keeps_its_junctions() {
        // T17-I2: atlas `simple` dropped the junction vertex of way 5 (its side street is outside its scan); atlas `full` kept it.
        let (junction, east) = (destination(o(), 90.0, 100.0), destination(o(), 90.0, 200.0));
        let side = way(6, FOOT, vec![junction, destination(junction, 0.0, 80.0)]);
        let full = Atlas { ways: vec![way(5, FOOT, vec![o(), junction, east]), side], ..Atlas::default() };
        let simple = Atlas { ways: vec![way(5, FOOT, vec![o(), east])], ..Atlas::default() };
        for order in [[&simple, &full], [&full, &simple]] {
            let g = StreetGraph::for_atlases(&order).unwrap();
            let d = g.dijkstra(0, 1000.0, FOOT);
            assert_eq!(d.len(), 4, "the side street connects: {d:?}");
            assert!(d.values().any(|x| (x - 180.0).abs() < 0.5), "100 m along, 80 m up the side street: {d:?}");
        }
    }

    #[test]
    fn a_way_seen_twice_adds_its_segments_once_with_both_classes() {
        let pts = vec![o(), destination(o(), 0.0, 50.0)];
        let g = StreetGraph::from_ways(&[way(1, FOOT, pts.clone()), way(2, CAR, pts.into_iter().rev().collect())]).unwrap();
        assert_eq!(g.segment_count(), 1);
        assert_eq!(g.seg(0).class, FOOT | CAR);
    }

    #[test]
    fn a_piece_seen_twice_is_a_sidewalk_only_if_every_copy_is() {
        use crate::scan::way_class::SIDE;
        let pts = vec![o(), destination(o(), 0.0, 50.0)];
        let g = StreetGraph::from_ways(&[way(1, FOOT | SIDE, pts.clone()), way(2, FOOT | BIKE | CAR, pts.clone())]).unwrap();
        assert_eq!(g.seg(0).class, FOOT | BIKE | CAR, "the street wins: not a sidewalk");
        let g = StreetGraph::from_ways(&[way(1, FOOT | SIDE, pts.clone()), way(2, FOOT | BIKE | SIDE, pts)]).unwrap();
        assert_eq!(g.seg(0).class, FOOT | BIKE | SIDE, "mode bits are joined, the side bit kept");
        assert!([Mode::Walk, Mode::Run, Mode::Bike, Mode::Drive].iter().all(|m| mode_mask(*m) & SIDE == 0), "SIDE is no mode");
    }

    #[test]
    fn a_degraded_link_on_a_footway_never_opens_it_to_cars() {
        // Round 3: an old atlas's link (every class) over the same nodes as a modern footway keeps the footway's class.
        let east = destination(o(), 90.0, 60.0);
        let modern = Atlas { ways: vec![way(1, FOOT, vec![o(), east])], ..Atlas::default() };
        let old = Atlas { streets: vec![o(), east], street_runs: vec![2], street_stride: 1, ..Atlas::default() };
        for order in [[&modern, &old], [&old, &modern]] {
            let g = StreetGraph::for_atlases(&order).unwrap();
            assert_eq!(g.segment_count(), 1);
            assert_eq!(g.seg(0).class, FOOT, "the way's class wins over the link's");
            assert!(g.candidates(o(), 5.0, 8, CAR).is_empty(), "no cars on the footway");
        }
    }

    #[test]
    fn an_old_zones_run_end_joins_the_modern_streets_nearby() {
        let end = destination(o(), 90.0, 100.0);
        let modern = Atlas { ways: vec![way(1, FOOT, vec![o(), end])], ..Atlas::default() };
        let start = destination(end, 90.0, 8.0);
        let pts: Vec<Point> = (0..3).map(|i| destination(start, 90.0, 60.0 * f64::from(i))).collect();
        let old = Atlas { streets: pts, street_runs: vec![3], street_stride: 1, ..Atlas::default() };
        let g = StreetGraph::for_atlases(&[&modern, &old]).unwrap();
        let d = g.dijkstra(0, 1000.0, FOOT);
        assert_eq!(d.len(), 5, "both zones in one connected graph: {d:?}");
        assert!(d.values().any(|x| (x - 228.0).abs() < 0.5), "100 m + the 8 m join + 120 m: {d:?}");
    }

    #[test]
    fn an_atlas_without_any_streets_keeps_the_full_graph() {
        let with = Atlas { ways: vec![way(1, FOOT, vec![o(), destination(o(), 0.0, 50.0)])], ..Atlas::default() };
        assert!(!StreetGraph::for_atlases(&[&with, &Atlas::default()]).unwrap().is_degraded());
    }

    #[test]
    fn a_way_seen_in_two_atlases_is_one_set_of_segments() {
        let with = Atlas { ways: vec![way(1, FOOT, vec![o(), destination(o(), 0.0, 50.0)])], ..Atlas::default() };
        assert_eq!(StreetGraph::for_atlases(&[&with, &with]).unwrap().segment_count(), 1);
    }

    #[test]
    fn positions_along_a_segment_are_clamped_to_it() {
        let g = tee();
        let mid = g.en_at(0, 50.0);
        assert!((mid[0] - 50.0).abs() < 0.5 && mid[1].abs() < 0.5, "{mid:?}");
        assert_eq!(g.en_at(0, 500.0), g.node_en(1));
        assert!(crate::geo::distance_m(g.geo_at(0, 100.0), destination(o(), 90.0, 100.0)) < 0.01);
        assert_eq!(g.frame().to_enu(o()), [0.0, 0.0]);
    }

    /// Heap bytes the graph's cell index holds.
    fn grid_bytes(g: &StreetGraph) -> usize {
        g.cell_keys.capacity() * size_of::<u64>() + (g.cell_off.capacity() + g.cell_segs.capacity()) * size_of::<u32>()
    }

    /// Heap bytes the whole graph holds (vectors at their capacity).
    fn approx_bytes(g: &StreetGraph) -> usize {
        let vecs = g.nodes.capacity() * size_of::<[f32; 2]>() + g.segs.capacity() * size_of::<Packed>();
        let adj = (g.adj_off.capacity() + g.adj.capacity()) * size_of::<u32>();
        size_of::<StreetGraph>() + vecs + adj + grid_bytes(g)
    }

    #[test]
    fn a_long_diagonal_is_indexed_only_in_the_cells_it_crosses_and_still_found_everywhere() {
        // T17-M3: a bounding box would index about (10 km / cell)^2 / 2 cells; a grid traversal a few per cell length.
        for bearing in [45.0, 117.0, 200.0, 333.0] {
            let g = StreetGraph::from_ways(&[way(1, FOOT, vec![o(), destination(o(), bearing, 10_000.0)])]).unwrap();
            assert!(g.cell_keys.len() <= crate::num::ceil_usize(3.0 * 10_000.0 / CELL_M), "{bearing}: {} cells", g.cell_keys.len());
            // Points `along` the segment and `side` metres to its right, in the graph's own frame (where the segment is straight).
            let (e, len) = (g.node_en(1), g.seg(0).len_m);
            let at = |along: f64, side: f64| g.frame().to_geo([(e[0] * along + e[1] * side) / len, (e[1] * along - e[0] * side) / len]);
            for k in 0..2_000 {
                let (along, side) = ((f64::from(k) * 37.3) % len, (f64::from(k) * 13.7) % 160.0 - 80.0);
                if (side.abs() - 40.0).abs() < 0.01 {
                    continue;
                }
                let c = g.candidates(at(along, side), 40.0, 8, FOOT);
                assert_eq!(c.len(), usize::from(side.abs() < 40.0), "{bearing}: {along} m along, {side} m aside: {c:?}");
                assert!(c.iter().all(|x| (x.d_m - side.abs()).abs() < 0.01 && (x.off_m - along).abs() < 0.01), "{c:?}");
            }
        }
    }

    #[test]
    fn a_huge_candidate_radius_is_clamped() {
        // T17-M5: a 3 km grid; a radius far beyond MAX_RADIUS_M finds only what lies within it, and fast.
        let g = StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 31, 100.0)).unwrap();
        for r in [5_000.0, 1e12] {
            let c = g.candidates(o(), r, usize::MAX, FOOT);
            assert!(!c.is_empty() && c.iter().all(|x| x.d_m <= MAX_RADIUS_M), "radius {r}: farthest {:?}", c.last());
        }
        assert!(g.candidates(o(), -5.0, 8, FOOT).len() <= 2, "a negative radius is zero: only the segments through the point");
    }

    #[test]
    fn a_city_grid_fits_the_memory_budget() {
        // T17-R1: the 10 km / 100 m suburb (80 400 segments) in at most 5 MB, the same bytes per segment on a smaller grid.
        let g = StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 71, 40.0)).unwrap();
        let budget = 5_000_000 * g.segment_count() / 80_400;
        assert!(approx_bytes(&g) <= budget, "{} bytes for {} segments, budget {budget}", approx_bytes(&g), g.segment_count());
    }

    #[test]
    #[ignore = "size harness: cargo test --release -p apgo-core graph_size_on_a_large_realm -- --ignored --nocapture"]
    fn graph_size_on_a_large_realm() {
        // A 10 km-radius suburban realm: a 20 km square of streets 100 m (and, denser, 60 m) apart.
        for (n, spacing) in [(201, 100.0), (334, 60.0)] {
            let ways = crate::loc::bench::grid_ways(destination(destination(o(), 180.0, 10_000.0), 270.0, 10_000.0), n, spacing);
            let json = serde_json::to_string(&Atlas { ways: ways.clone(), ..Atlas::default() }).unwrap().len();
            let t = std::time::Instant::now();
            let g = StreetGraph::from_ways(&ways).unwrap();
            let built = t.elapsed();
            let t = std::time::Instant::now();
            let found: usize = (0..10_000).map(|i| g.candidates(destination(o(), f64::from(i % 360), f64::from(i % 97) * 50.0), 50.0, 8, FOOT).len()).sum();
            println!("  10 000 candidate lookups (50 m radius) in {:?}, {found} found", t.elapsed());
            println!(
                "{n}x{n} grid at {spacing} m: {} nodes, {} segments, built in {built:?}, about {:.1} MB in memory ({:.1} MB of it the grid index), ways JSON {:.1} MB",
                g.nodes.len(),
                g.segment_count(),
                crate::num::count_f64(approx_bytes(&g)) / 1e6,
                crate::num::count_f64(grid_bytes(&g)) / 1e6,
                crate::num::count_f64(json) / 1e6
            );
        }
    }

    #[test]
    #[ignore = "timing harness: cargo test --release -p apgo-core graph_build_time -- --ignored --nocapture"]
    fn graph_build_time() {
        let ways = crate::loc::bench::grid_ways(o(), 71, 40.0); // about 5000 vertices
        let t = std::time::Instant::now();
        let g = StreetGraph::from_ways(&ways).unwrap();
        println!("{} segments in {:?}, about {:.2} MB", g.segment_count(), t.elapsed(), crate::num::count_f64(approx_bytes(&g)) / 1e6);
    }
}
