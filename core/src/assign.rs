//! Assign each apworld slot a concrete quest at a real place, using what the slot's realm offers.

use std::collections::{BTreeSet, HashMap};

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, Geom, Kind, Mode, Verify, FORAGE_THEMES};
use crate::effort::{cadence_steps_per_min, dist_for, mid, tier_for, travel_min};
use crate::geo::{bearing_deg, distance_m, point_in_polygon, point_inside, polyline_len_m, Point};
use crate::near_path::{PathIndex, NEAR_PATH_M};
use crate::num::{count_f64, round_u32};
use crate::realm::Realm;
use crate::scan::{Atlas, Feature};
use crate::units::{distance, distance_rounded, Round, UnitSystem};
use crate::verify::{HOME_RADIUS_M, LINE_SAMPLE_M};

/// A quest slot to fill: one Archipelago location and what it asks for.
#[derive(Debug, Clone)]
pub struct SlotIn {
    /// Archipelago location id of the check.
    pub location_id: i64,
    /// Zone number the quest belongs to.
    pub zone: u32,
    /// How the player travels in that zone.
    pub mode: Mode,
    /// Quest family wanted for the slot.
    pub family: String,
    /// Effort tier wanted, starting at 1.
    pub tier: u8,
    /// Whether this is the realm's boss quest.
    pub boss: bool,
}

/// What the player has to do to complete a quest, with the numbers it needs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Target {
    /// Reach a single place.
    Point {
        /// The place to reach.
        p: Point,
        /// How close counts as reached, in metres.
        r: f64,
    },
    /// Stay near a place for a while.
    Dwell {
        /// The place to stay at.
        p: Point,
        /// How close counts as there, in metres.
        r: f64,
        /// How long to stay, in minutes.
        minutes: f64,
    },
    /// Spend time inside an area.
    DwellArea {
        /// The outline of the area.
        poly: Vec<Point>,
        /// Middle of the area.
        center: Point,
        /// Radius of the circle used when no outline is available, in metres.
        r: f64,
        /// How long to stay, in minutes.
        minutes: f64,
    },
    /// Follow a path.
    Line {
        /// The path, in order.
        pts: Vec<Point>,
        /// How far off the path still counts, in metres.
        corridor_m: f64,
        /// Share of the path to cover, 0 to 1.
        coverage: f64,
    },
    /// Pick up at one place and deliver to another in time.
    Courier {
        /// Pick-up place.
        a: Point,
        /// Delivery place.
        b: Point,
        /// How close counts as there, in metres.
        r: f64,
        /// Time allowed between pick-up and delivery, in minutes.
        time_limit_min: f64,
    },
    /// Go to a far place and come back.
    RoundTrip {
        /// The turning point.
        far: Point,
        /// How close counts as there, in metres.
        r: f64,
    },
    /// Visit new map cells.
    Cells {
        /// Number of new cells to visit.
        n: u32,
        /// Edge length of a cell, in metres.
        cell_m: f64,
    },
    /// Take a number of steps.
    Steps {
        /// Number of steps.
        n: u32,
    },
    /// Spend time far from home.
    Away {
        /// How long to be away from home, in minutes.
        minutes: f64,
    },
    /// Pick up items around home and bring enough of them home (forager).
    Collect {
        /// Where the items lie: nearest home first. Indexes are stable (saved progress refers to them).
        pts: Vec<Point>,
        /// How many items must be brought home.
        need: u32,
        /// How close counts as picked up, in metres.
        r: f64,
        /// What the items are ("pinecones"), flavour only.
        theme: String,
    },
}

impl Target {
    /// What the player has to do, in one line, in the player's units ("Get within 40 m").
    #[must_use]
    pub fn goal_text(&self, units: UnitSystem) -> String {
        match self {
            Self::Point { r, .. } => format!("Get within {}", distance_rounded(*r, units, Round::Down)),
            Self::Dwell { r, minutes, .. } => format!("Stay {minutes:.0} min within {}", distance_rounded(*r, units, Round::Down)),
            Self::DwellArea { minutes, .. } => format!("Spend {minutes:.0} min inside the area"),
            Self::Line { pts, coverage, .. } => format!("Cover {:.0}% of this {} path", coverage * 100.0, distance(polyline_len_m(pts), units)),
            Self::Courier { time_limit_min, .. } => format!("Pick up at A, deliver to B within {time_limit_min:.0} min"),
            Self::RoundTrip { .. } => "Reach the far point, then come back home".to_string(),
            Self::Cells { n, .. } => format!("Visit {n} new map cells"),
            Self::Steps { n } => format!("Take {n} steps"),
            Self::Away { minutes } => format!("Spend {minutes:.0} min away from home"),
            Self::Collect { need, r, theme, .. } => format!("Bring home {need} {theme} (pick up within {})", distance_rounded(*r, units, Round::Down)),
        }
    }
}

/// A quest assigned to a slot: what to do, where, and how it is described.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    /// Archipelago location id of the check.
    pub location_id: i64,
    /// Zone number the quest belongs to.
    pub zone: u32,
    /// How the player travels in that zone.
    pub mode: Mode,
    /// Quest family of the chosen kind.
    pub family: String,
    /// Catalog id of the chosen kind.
    pub kind_id: String,
    /// Display name of the quest.
    pub quest_name: String,
    /// Short description of the quest kind.
    pub blurb: String,
    /// Name of the place the quest uses.
    pub place: String,
    /// Effort tier, starting at 1.
    pub tier: u8,
    /// Expected effort in minutes.
    pub effort_min: f64,
    /// What the player must do to complete it.
    pub target: Target,
    /// True when the realm could not offer the requested family and a street quest was used instead.
    pub fallback: bool,
    /// Whether this is the realm's boss quest.
    pub boss: bool,
}

/// A zone prepared for quest assignment: its travel mode, realm and scanned map data.
pub struct ZoneCtx<'a> {
    /// Zone number.
    pub zone: u32,
    /// How the player travels in this zone.
    pub mode: Mode,
    /// The realm the zone belongs to.
    pub realm: &'a Realm,
    /// Scanned places and streets of the realm.
    pub atlas: &'a Atlas,
}

/// How much rough going (unpaved paths, unknown-surface trails, stairs) the player accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SurfacePref {
    /// No preference.
    #[default]
    Any,
    /// Use paved routes when enough of them exist.
    PreferPaved,
    /// Use only paved routes.
    PavedOnly,
}

impl SurfacePref {
    /// Read a surface preference from its settings string; unknown values mean [`Self::Any`].
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "prefer_paved" => Self::PreferPaved,
            "paved_only" => Self::PavedOnly,
            _ => Self::Any,
        }
    }
}

/// Settings that steer quest assignment.
pub struct AssignParams {
    /// The home point distances are measured from.
    pub home: Point,
    /// Minutes of effort that one tier covers.
    pub minutes_per_tier: f64,
    /// Quests must be at least this far from home, in metres.
    pub min_distance_m: f64,
    /// Seed for the random choices, so the same inputs give the same quests.
    pub seed: u64,
    /// How much rough going the player accepts.
    pub surface: SurfacePref,
    /// Whether quests with stairs are dropped.
    pub avoid_stairs: bool,
    /// Whether progressive (chain) kinds may be placed. Off for a reroll, so a re-placed quest never joins or starts a chain.
    pub allow_progressive: bool,
}

struct Cand {
    score: f64,
    kind: Kind,
    target: Target,
    effort: f64,
    place: String,
    feature_id: Option<String>,
    favorite: bool,
}

const SPACING_M: f64 = 40.0;
/// The least share of a trail a quest asks for.
const MIN_TRAIL_SHARE: f64 = 0.25;
/// How many effort-minutes of misfit a favorite place can make up for.
const FAVORITE_BONUS_MIN: f64 = 6.0;
/// Least distance between two forager items (and between an item and another quest's point), in metres.
const ITEM_SPACING_M: f64 = 60.0;
/// How many shuffled street points a forager placement looks at.
const MAX_ITEM_CANDIDATES: usize = 2000;

/// The band forager items are placed in for effort `want` (minutes) in `mode`, as `(min_m, far_m)` from home: never nearer than the
/// minimum distance or the home radius plus the pick-up radius `pick_r_m` (an item there would be picked and banked in the same step),
/// and the farthest about half the effort's travel out, so out to it and back fits the effort.
fn item_band(p: &AssignParams, pick_r_m: f64, want: f64, mode: Mode) -> (f64, f64) {
    (p.min_distance_m.max(HOME_RADIUS_M + pick_r_m), dist_for(want / 2.0, mode))
}

/// `n` street points for forager items, spread outward from `home`: the k-th of them (counting from 1) about k/n of the way from `min_m`
/// to `far_m`, each at least [`ITEM_SPACING_M`] from the others and from `keep`. The rules are checked again on the final points (near a
/// street segment by `index`, at least `min_m` from home, spaced), never only on candidates. `None` when the pool cannot supply `n` such points.
#[allow(clippy::too_many_arguments)] // like free_candidate: the zone's pool and index, home, the band and the points to keep apart from
fn place_items(pool: &[Point], index: &PathIndex, home: Point, min_m: f64, far_m: f64, n: usize, keep: &[Point], rng: &mut StdRng) -> Option<Vec<Point>> {
    if n == 0 {
        return Some(Vec::new());
    }
    let far_m = far_m.max(min_m + 2.0 * ITEM_SPACING_M);
    let mut candidates: Vec<(Point, f64)> = pool.iter().map(|q| (*q, distance_m(home, *q))).filter(|(_, d)| (min_m..=far_m * 1.5).contains(d)).collect();
    candidates.shuffle(rng);
    candidates.truncate(MAX_ITEM_CANDIDATES);
    let mut out: Vec<Point> = Vec::with_capacity(n);
    // The farthest first: it is the hardest to fit.
    for k in (1..=n).rev() {
        let want = min_m + (far_m - min_m) * count_f64(k) / count_f64(n);
        let spaced = |q: Point| keep.iter().chain(&out).all(|o| distance_m(*o, q) >= ITEM_SPACING_M);
        let (q, _) = candidates.iter().filter(|(q, _)| spaced(*q)).min_by(|a, b| (a.1 - want).abs().total_cmp(&(b.1 - want).abs()))?;
        out.push(*q);
    }
    let ok = out
        .iter()
        .enumerate()
        .all(|(i, q)| distance_m(home, *q) >= min_m && index.near_path(*q) && keep.iter().chain(&out[..i]).all(|o| distance_m(*o, *q) >= ITEM_SPACING_M));
    ok.then_some(out)
}

/// The forager items a slot needs: by tier, tiers past the table use its last value.
fn need_for(need_by_tier: &[u32], want: f64, minutes_per_tier: f64) -> Option<u32> {
    let tier = usize::from(tier_for(want, minutes_per_tier));
    need_by_tier.get(tier.min(need_by_tier.len()).checked_sub(1)?).copied().filter(|n| *n > 0)
}

/// A quest's display name: a forager names its count and theme.
fn quest_title(kind_name: &str, t: &Target) -> String {
    match t {
        Target::Collect { need, theme, .. } => format!("{kind_name}: bring home {need} {theme}"),
        _ => kind_name.to_string(),
    }
}

/// Whether the rough points join the paved ones for this surface preference (the same choice for points and the streets between them).
fn uses_rough(z: &ZoneCtx<'_>, pref: SurfacePref) -> bool {
    match pref {
        SurfacePref::PreferPaved => z.atlas.streets.len() < 50,
        SurfacePref::PavedOnly => false,
        SurfacePref::Any => true,
    }
}

/// The street and path points quests in zone `z` may use. A sparse zone keeps the few it has: a quest point is never made up off the
/// streets (it used to fall back to a grid over the whole realm, which put quests in backyards).
pub(crate) fn street_pool(z: &ZoneCtx<'_>, pref: SurfacePref) -> Vec<Point> {
    let rough: &[Point] = if uses_rough(z, pref) { &z.atlas.streets_rough } else { &[] };
    z.atlas.streets.iter().chain(rough).copied().collect()
}

/// The segment-aware path index of zone `z` over `pool`: its street points and the streets between them (rough ones when the surface
/// preference uses them).
pub(crate) fn zone_index(z: &ZoneCtx<'_>, pool: &[Point], surface: SurfacePref) -> PathIndex {
    let mut links = z.atlas.street_links(false);
    if uses_rough(z, surface) {
        links.extend(z.atlas.street_links(true));
    }
    PathIndex::with_segments(pool, &links)
}

/// A zone's street points and the streets between them, indexed, and where each of its places can be reached from a path (worked out
/// once per place). Every spot it hands out is at least the minimum distance from home.
struct ZonePaths {
    pool: Vec<Point>,
    index: PathIndex,
    home: Point,
    min_dist: f64,
    points: HashMap<usize, Option<Point>>,
    centers: HashMap<usize, Option<Point>>,
    lines: HashMap<usize, Option<Vec<Point>>>,
    shares: HashMap<(usize, u64), f64>,
}

impl ZonePaths {
    fn new(z: &ZoneCtx<'_>, p: &AssignParams) -> Self {
        let pool = street_pool(z, p.surface);
        let index = zone_index(z, &pool, p.surface);
        let maps = (HashMap::new(), HashMap::new(), HashMap::new(), HashMap::new());
        Self { pool, index, home: p.home, min_dist: p.min_distance_m, points: maps.0, centers: maps.1, lines: maps.2, shares: maps.3 }
    }

    fn far(&self, q: Point) -> bool {
        distance_m(self.home, q) >= self.min_dist
    }

    /// The spot on the path in front of `spot` (the nearest within [`NEAR_PATH_M`]), else a path spot in or beside the area `poly`;
    /// either must be far enough from home and accepted by `ok`. The place itself is never the marker: it may be in a backyard.
    fn settle(&self, spot: Point, poly: &[Point], ok: &dyn Fn(Point) -> bool) -> Option<Point> {
        let fine = |q: Point| self.far(q) && ok(q);
        self.index.nearest_on_path(spot, NEAR_PATH_M).map(|(q, _)| q).filter(|q| fine(*q)).or_else(|| self.index.snap_into_area_where(poly, spot, &fine))
    }

    /// Where to mark an area to spend time in (a park is public ground): on a path inside it, else its own middle when a path is
    /// within reach, else a path spot at its edge.
    fn settle_area(&self, poly: &[Point]) -> Option<Point> {
        let mid = point_inside(poly);
        let far = |q: Point| self.far(q);
        self.index
            .snap_into_area_where(poly, mid, &far)
            .filter(|q| point_in_polygon(*q, poly))
            .or_else(|| (self.index.near_path(mid) && self.far(mid)).then_some(mid))
            .or_else(|| self.index.snap_into_area_where(poly, mid, &far))
    }

    /// The point to reach for place `f` (index `fi`): the place itself when it is near a path, else a path spot in or beside its area.
    /// `spaced` rejects spots too close to other quests; a cached spot that fails it is re-snapped once.
    fn point(&mut self, fi: usize, f: &Feature, spaced: &dyn Fn(Point) -> bool) -> Option<Point> {
        let cached = if let Some(c) = self.points.get(&fi) {
            *c
        } else {
            let c = self.settle(f.point, &f.geometry, &|_| true);
            self.points.insert(fi, c);
            c
        };
        let q = cached?;
        if spaced(q) {
            Some(q)
        } else {
            self.index.snap_into_area_where(&f.geometry, f.point, &|q| self.far(q) && spaced(q))
        }
    }

    /// The marker of an area to spend time in: a spot inside it near a path, or a path spot at its edge. With no outline, the place
    /// itself or the nearest path spot within the circle (`r`) the quest uses.
    fn center(&mut self, fi: usize, f: &Feature, r: f64, spaced: &dyn Fn(Point) -> bool) -> Option<Point> {
        let poly = &f.geometry;
        let cached = if let Some(c) = self.centers.get(&fi) {
            *c
        } else {
            let c = if poly.len() >= 3 { self.settle_area(poly) } else { self.index.nearest_on_path(f.point, r).map(|(q, _)| q).filter(|q| self.far(*q)) };
            self.centers.insert(fi, c);
            c
        };
        let q = cached?;
        if spaced(q) {
            Some(q)
        } else {
            self.index.snap_into_area_where(poly, point_inside(poly), &|q| self.far(q) && spaced(q))
        }
    }

    /// The line of place `f`, starting where it first comes near a path (and far enough from home).
    fn line(&mut self, fi: usize, f: &Feature) -> Option<Vec<Point>> {
        if !self.lines.contains_key(&fi) {
            let l = self.index.start_near_path(&f.geometry).filter(|l| l.first().is_some_and(|s| self.far(*s)));
            self.lines.insert(fi, l);
        }
        self.lines.get(&fi).cloned().flatten()
    }

    /// The share of line `pts` (of place `fi`) whose samples, as the tracker takes them, lie within `reach_m` of a path.
    fn share(&mut self, fi: usize, pts: &[Point], reach_m: f64) -> f64 {
        let index = &self.index;
        *self.shares.entry((fi, reach_m.to_bits())).or_insert_with(|| index.share_near(pts, LINE_SAMPLE_M, reach_m))
    }
}

/// The point that stands for a target on the map and keeps quests apart.
fn anchor(t: &Target) -> Option<Point> {
    match t {
        Target::Point { p, .. } | Target::Dwell { p, .. } => Some(*p),
        Target::DwellArea { center, .. } => Some(*center),
        Target::Line { pts, .. } | Target::Collect { pts, .. } => pts.first().copied(),
        Target::Courier { a, .. } => Some(*a),
        Target::RoundTrip { far, .. } => Some(*far),
        Target::Cells { .. } | Target::Steps { .. } | Target::Away { .. } => None,
    }
}

/// Pool point whose one-way travel effort from `origin` best matches `want_min`.
fn best_point(pool: &[Point], origin: Point, mode: Mode, want_min: f64, min_dist: f64, rng: &mut StdRng, ok: &dyn Fn(Point) -> bool) -> Option<Point> {
    let mut idx: Vec<usize> = (0..pool.len()).collect();
    idx.shuffle(rng);
    idx.into_iter()
        .take(300)
        .map(|i| pool[i])
        .filter(|p| distance_m(origin, *p) >= min_dist && ok(*p))
        .min_by(|a, b| (travel_min(distance_m(origin, *a), mode) - want_min).abs().total_cmp(&(travel_min(distance_m(origin, *b), mode) - want_min).abs()))
}

/// Radius of the circle used for an area to spend time in when the place has no outline, in metres.
const AREA_CIRCLE_M: f64 = 80.0;

/// The quest kind `k` makes of place `f` (index `fi`), with its effort. Every point to reach is near a path (see [`crate::near_path`]),
/// at least the minimum distance from home and `spaced` from the other quests; a place that cannot be reached so is not offered.
#[allow(clippy::too_many_arguments)] // the place, its zone's paths and the run's spacing rule all shape the target
fn feature_target(
    k: &Kind,
    fi: usize,
    f: &Feature,
    zp: &mut ZonePaths,
    mode: Mode,
    home: Point,
    want: f64,
    spaced: &dyn Fn(Point) -> bool,
) -> Option<(Target, f64)> {
    match &k.verify {
        Verify::Reach { radius_m } => {
            let p = zp.point(fi, f, spaced)?;
            Some((Target::Point { p, r: *radius_m }, travel_min(distance_m(home, p), mode)))
        }
        Verify::Dwell { minutes, radius_m } => {
            let p = zp.point(fi, f, spaced)?;
            Some((Target::Dwell { p, r: *radius_m, minutes: *minutes }, travel_min(distance_m(home, p), mode) + minutes))
        }
        Verify::DwellInArea { minutes } => {
            let poly = if f.geometry.len() >= 3 { f.geometry.clone() } else { vec![] };
            let center = zp.center(fi, f, AREA_CIRCLE_M, spaced)?;
            Some((Target::DwellArea { poly, center, r: AREA_CIRCLE_M, minutes: *minutes }, travel_min(distance_m(home, center), mode) + minutes))
        }
        Verify::FollowLine { corridor_m, coverage, min_len_m, max_len_m } => {
            if f.geometry.len() < 2 || polyline_len_m(&f.geometry) < *min_len_m {
                return None;
            }
            let pts = zp.line(fi, f)?;
            let len = polyline_len_m(&pts);
            if len < *min_len_m || len > *max_len_m || !pts.first().is_some_and(|s| spaced(*s)) {
                return None;
            }
            // The tracker counts covered samples anywhere on the line: ask for no more than the share a player can cover from a path.
            let beside = zp.share(fi, &pts, corridor_m.min(NEAR_PATH_M));
            if beside < MIN_TRAIL_SHARE {
                return None;
            }
            let most = coverage.min(beside);
            let nearest = pts.iter().map(|p| distance_m(home, *p)).fold(f64::MAX, f64::min);
            let pace = mode.m_per_min() * if mode == Mode::Walk { 0.8 } else { 1.0 };
            // Ask for the share of the line that fits the effort wanted, so a long trail makes a fair quest too: never above the kind's own
            // share (or what lies beside a path), and never less than a quarter of it (or 150 m).
            let travel = travel_min(nearest, mode);
            let full = len / pace;
            let floor = MIN_TRAIL_SHARE.max(150.0 / len).min(most);
            let share = ((want - travel) / full).clamp(floor, most);
            Some((Target::Line { pts, corridor_m: *corridor_m, coverage: share }, travel + share * full))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)] // pre-existing: flat argument lists keep the exported/geometry call sites explicit
#[allow(clippy::many_single_char_names)] // short names for zone/pool/params mirror the geometry vocabulary used across this module
fn free_candidate(
    k: &Kind,
    z: &ZoneCtx<'_>,
    pool: &[Point],
    index: &PathIndex,
    p: &AssignParams,
    want: f64,
    rng: &mut StdRng,
    used_pts: &[Point],
) -> Option<(Target, f64, String)> {
    let mode = z.mode;
    let far_from_used = |q: Point| used_pts.iter().all(|u| distance_m(*u, q) >= SPACING_M);
    match &k.verify {
        Verify::Reach { radius_m } if k.sector => {
            let (bearing, label) = *[(0.0, "Due North"), (90.0, "Due East"), (180.0, "Due South"), (270.0, "Due West")].choose(rng)?;
            let in_sector = |q: Point| {
                let d = (bearing_deg(p.home, q) - bearing + 540.0) % 360.0 - 180.0;
                d.abs() <= 45.0 && far_from_used(q)
            };
            let q = best_point(pool, p.home, mode, want, p.min_distance_m, rng, &in_sector)?;
            Some((Target::Point { p: q, r: *radius_m }, travel_min(distance_m(p.home, q), mode), label.to_string()))
        }
        Verify::Reach { radius_m } => {
            let q = best_point(pool, p.home, mode, want, p.min_distance_m, rng, &far_from_used)?;
            Some((Target::Point { p: q, r: *radius_m }, travel_min(distance_m(p.home, q), mode), "A spot on the street".into()))
        }
        Verify::Dwell { minutes, radius_m } => {
            let q = best_point(pool, p.home, mode, (want - minutes).max(1.0), p.min_distance_m, rng, &far_from_used)?;
            Some((Target::Dwell { p: q, r: *radius_m, minutes: *minutes }, travel_min(distance_m(p.home, q), mode) + minutes, "Anywhere outdoors".into()))
        }
        Verify::Courier { .. } => {
            let a = best_point(pool, p.home, mode, want * 0.4, p.min_distance_m, rng, &far_from_used)?;
            let b = best_point(pool, a, mode, want * 0.6, 100.0, rng, &|q| distance_m(p.home, q) >= p.min_distance_m && far_from_used(q))?;
            let leg2 = travel_min(distance_m(a, b), mode);
            let effort = travel_min(distance_m(p.home, a), mode) + leg2;
            Some((Target::Courier { a, b, r: 40.0, time_limit_min: leg2 * 1.6 + 5.0 }, effort, "Pick up, then deliver".into()))
        }
        Verify::RoundTrip => {
            let far = best_point(pool, p.home, mode, want / 2.0, p.min_distance_m, rng, &far_from_used)?;
            let one_way = travel_min(distance_m(p.home, far), mode);
            Some((Target::RoundTrip { far, r: 50.0 }, one_way * 2.0, "Out and back".into()))
        }
        Verify::CoverCells { cell_m, .. } => {
            let n = round_u32(want * mode.m_per_min() * 0.7 / cell_m).clamp(3, 60);
            Some((Target::Cells { n, cell_m: *cell_m }, f64::from(n) * cell_m / (mode.m_per_min() * 0.7), "New map cells".into()))
        }
        Verify::Steps { .. } => {
            let n = round_u32(want * cadence_steps_per_min(mode)).clamp(500, 40_000);
            Some((Target::Steps { n }, f64::from(n) / cadence_steps_per_min(mode), "Anywhere".into()))
        }
        Verify::Away { .. } => {
            let minutes = (want * 3.0).clamp(30.0, 480.0);
            Some((Target::Away { minutes }, want, "Away from home".into()))
        }
        Verify::Collect { need_by_tier, spare_factor, pick_r_m } => {
            let need = need_for(need_by_tier, want, p.minutes_per_tier)?;
            let total = usize::try_from(need.saturating_mul(*spare_factor)).ok()?;
            let (min_m, far_m) = item_band(p, *pick_r_m, want, mode);
            let mut pts = place_items(pool, index, p.home, min_m, far_m, total, used_pts, rng)?;
            pts.sort_by(|a, b| distance_m(p.home, *a).total_cmp(&distance_m(p.home, *b)));
            let theme = (*FORAGE_THEMES.choose(rng)?).to_string();
            let farthest = pts.last().map_or(0.0, |q| distance_m(p.home, *q));
            Some((Target::Collect { pts, need, r: *pick_r_m, theme }, 2.0 * travel_min(farthest, mode), "Around home".into()))
        }
        _ => None,
    }
}

/// Whether kind `k` may be placed in slot `s` of zone `z`.
fn offered(k: &Kind, s: &SlotIn, z: &ZoneCtx<'_>, p: &AssignParams) -> bool {
    k.allows(z.mode)
        && k.family != "boss"
        && (s.boss || k.family == s.family)
        && !(p.avoid_stairs && k.id == "stairmaster")
        && (p.allow_progressive || !k.is_progressive())
}

#[allow(clippy::too_many_arguments)] // the per-run state (zone paths, rng, used places) is threaded explicitly, like free_candidate
fn one(
    s: &SlotIn,
    z: &ZoneCtx<'_>,
    catalog: &Catalog,
    p: &AssignParams,
    zp: &mut ZonePaths,
    rng: &mut StdRng,
    used_feat: &mut BTreeSet<String>,
    used_pts: &mut Vec<Point>,
) -> Assignment {
    let want = mid(s.tier, p.minutes_per_tier);
    let kinds: Vec<&Kind> = catalog.kinds.iter().filter(|k| offered(k, s, z, p)).collect();
    let mut cands: Vec<Cand> = Vec::new();
    for k in &kinds {
        if k.geom != Geom::None {
            let Some(hits) = z.atlas.matches.get(&k.id) else { continue };
            let mut idx: Vec<usize> = hits.clone();
            idx.shuffle(rng);
            for fi in idx.into_iter().take(300) {
                let f = &z.atlas.features[fi];
                if used_feat.contains(&f.id) {
                    continue;
                }
                if p.surface == SurfacePref::PavedOnly && k.geom == Geom::Line && crate::scan::is_rough(&f.tags) {
                    continue;
                }
                let spaced = |q: Point| used_pts.iter().all(|u| distance_m(*u, q) >= SPACING_M);
                if let Some((target, effort)) = feature_target(k, fi, f, zp, z.mode, p.home, want, &spaced) {
                    cands.push(Cand {
                        // A favorite counts as a better fit than it is, so it is picked when it is anywhere near the right effort.
                        score: (effort - want).abs() - if z.atlas.favorites.contains(&f.id) { FAVORITE_BONUS_MIN } else { 0.0 },
                        kind: (*k).clone(),
                        target,
                        effort,
                        place: f.name.clone().unwrap_or_else(|| k.name.clone()),
                        feature_id: Some(f.id.clone()),
                        favorite: z.atlas.favorites.contains(&f.id),
                    });
                }
            }
        } else if let Some((target, effort, place)) = free_candidate(k, z, &zp.pool, &zp.index, p, want, rng, used_pts) {
            // Generic quests are the backup: prefer real places when the realm has them.
            cands.push(Cand {
                score: (effort - want).abs() + if s.boss { 6.0 } else { 3.0 },
                kind: (*k).clone(),
                target,
                effort,
                place,
                feature_id: None,
                favorite: false,
            });
        }
    }
    let mut fallback = false;
    if cands.is_empty() {
        fallback = true;
        if let Some(k) = catalog.kind("street_smarts") {
            // A sparse zone can run out of street points spaced apart from the other quests: then share one rather than leave the streets.
            let found =
                free_candidate(k, z, &zp.pool, &zp.index, p, want, rng, used_pts).or_else(|| free_candidate(k, z, &zp.pool, &zp.index, p, want, rng, &[]));
            if let Some((target, effort, place)) = found {
                cands.push(Cand { score: 0.0, kind: k.clone(), target, effort, place, feature_id: None, favorite: false });
            }
        }
    }
    cands.sort_by(|a, b| a.score.total_cmp(&b.score));
    let top = cands.len().min(4);
    // The boss always takes the best fit (the biggest thing on offer); other quests vary among the top few.
    let best_is_favorite = cands.first().is_some_and(|c| c.favorite);
    let pick = if top == 0 { None } else { Some(cands.swap_remove(if s.boss || best_is_favorite { 0 } else { rng.random_range(0..top) })) };
    let c = pick.unwrap_or_else(|| {
        // Absolutely nothing (e.g. an empty pool): a point at home keeps the slot playable.
        #[allow(clippy::expect_used)] // the builtin catalog always defines street_smarts
        let k = catalog.kind("street_smarts").expect("street_smarts exists").clone();
        fallback = true;
        Cand { score: 0.0, kind: k, target: Target::Point { p: p.home, r: 40.0 }, effort: want, place: "Home".into(), feature_id: None, favorite: false }
    });
    if let Some(id) = &c.feature_id {
        used_feat.insert(id.clone());
    }
    if let Some(a) = anchor(&c.target) {
        used_pts.push(a);
    }
    if let Target::Collect { pts, .. } = &c.target {
        used_pts.extend(pts.iter().skip(1)); // the first is the anchor, pushed above
    }
    let title = quest_title(&c.kind.name, &c.target);
    let (kind_id, quest_name) = if s.boss { ("the_big_one".to_string(), format!("The Big One: {title}")) } else { (c.kind.id.clone(), title) };
    Assignment {
        location_id: s.location_id,
        zone: s.zone,
        mode: z.mode,
        family: s.family.clone(),
        kind_id,
        quest_name,
        blurb: c.kind.blurb.clone(),
        place: c.place,
        tier: s.tier,
        effort_min: c.effort,
        target: c.target,
        fallback,
        boss: s.boss,
    }
}

/// Assign every slot (boss last so it gets the best leftovers). Output keeps the input slot order.
#[must_use]
pub fn assign(slots: &[SlotIn], zones: &[ZoneCtx<'_>], catalog: &Catalog, p: &AssignParams) -> Vec<Assignment> {
    let mut rng = StdRng::seed_from_u64(p.seed);
    let mut used_feat = BTreeSet::new();
    let mut used_pts: Vec<Point> = Vec::new();
    let mut paths: Vec<ZonePaths> = zones.iter().map(|z| ZonePaths::new(z, p)).collect();
    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by_key(|&i| slots[i].boss);
    let mut done: Vec<(usize, Assignment)> = Vec::new();
    for i in order {
        let s = &slots[i];
        if let Some(zi) = zones.iter().position(|z| z.zone == s.zone) {
            done.push((i, one(s, &zones[zi], catalog, p, &mut paths[zi], &mut rng, &mut used_feat, &mut used_pts)));
        }
    }
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, a)| a).collect()
}

/// A forager quest with its unpicked items moved to new street points under the placement rules (a Shuffle trap); `index` is the zone's
/// path index. Picked items, `need`, `r` and the theme stay, and so do the item indexes saved progress refers to. The new points fill the
/// open slots nearest home first when placed, so the first item stays the nearest and the quest still shows in the fog on the way out.
/// `None` for any other target, or when the zone cannot supply the new points: the caller then leaves the quest as it is.
#[must_use]
pub fn replace_unpicked(
    t: &Target,
    picked: &BTreeSet<u16>,
    z: &ZoneCtx<'_>,
    index: &PathIndex,
    p: &AssignParams,
    tier: u8,
    rng: &mut StdRng,
) -> Option<Target> {
    let Target::Collect { pts, need, r, theme } = t else { return None };
    let is_picked = |i: usize| u16::try_from(i).is_ok_and(|i| picked.contains(&i));
    let keep: Vec<Point> = pts.iter().enumerate().filter(|(i, _)| is_picked(*i)).map(|(_, q)| *q).collect();
    let (min_m, far_m) = item_band(p, *r, mid(tier, p.minutes_per_tier), z.mode);
    let pool = street_pool(z, p.surface);
    let mut fresh = place_items(&pool, index, p.home, min_m, far_m, pts.len() - keep.len(), &keep, rng)?;
    fresh.sort_by(|a, b| distance_m(p.home, *b).total_cmp(&distance_m(p.home, *a))); // descending, so pop() gives the nearest
    let pts = (0..pts.len()).map(|i| if is_picked(i) { Some(pts[i]) } else { fresh.pop() }).collect::<Option<Vec<Point>>>()?;
    Some(Target::Collect { pts, need: *need, r: *r, theme: theme.clone() })
}

#[cfg(test)]
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::many_single_char_names)] // test code: short names for points and coordinates in test fixtures; test fixtures use small, known-positive numbers and counts
mod tests {
    use super::*;
    use crate::effort::tier_for;
    use crate::geo::destination;
    use crate::realm::Shape;
    use std::collections::BTreeMap;

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }

    fn realm(_mode: Mode) -> Realm {
        Realm { id: "r".into(), name: "R".into(), icon: None, shape: Shape::Circle { center: home(), radius_m: 9000.0 }, spare: None, scanned_at_ms: None }
    }

    fn feature(id: &str, tags: &[(&str, &str)], p: Point, geometry: Vec<Point>) -> Feature {
        let tags: BTreeMap<String, String> = tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Feature { id: id.into(), name: tags.get("name").cloned(), point: p, tags, geometry }
    }

    fn atlas(cat: &Catalog, with_pois: bool) -> Atlas {
        let mut streets = Vec::new();
        for n in -45..=45 {
            for e in -45..=45 {
                streets.push(destination(destination(home(), 0.0, f64::from(n) * 120.0), 90.0, f64::from(e) * 120.0));
            }
        }
        let mut features = Vec::new();
        if with_pois {
            for i in 0..30 {
                features.push(feature(
                    &format!("n{i}"),
                    &[("amenity", "bench")],
                    destination(home(), 37.0 * f64::from(i), 400.0 + 150.0 * f64::from(i)),
                    vec![],
                ));
            }
            let a = destination(home(), 0.0, 1200.0);
            features.push(feature(
                "w1",
                &[("leisure", "park"), ("name", "City Park")],
                a,
                vec![a, destination(a, 90.0, 300.0), destination(a, 135.0, 300.0), a],
            ));
            let t0 = destination(home(), 90.0, 900.0);
            features.push(feature(
                "L:trail_boss:Ridge:0",
                &[("highway", "path"), ("name", "Ridge Trail")],
                t0,
                vec![t0, destination(t0, 90.0, 800.0), destination(t0, 90.0, 1600.0)],
            ));
        }
        crate::scan::build_atlas("r", 0, features, streets, cat)
    }

    fn params(seed: u64) -> AssignParams {
        AssignParams {
            home: home(),
            minutes_per_tier: 10.0,
            min_distance_m: 150.0,
            seed,
            surface: SurfacePref::Any,
            avoid_stairs: false,
            allow_progressive: true,
        }
    }

    #[test]
    fn a_kind_is_progressive_exactly_when_its_free_quest_is_a_chain_target() {
        // Drift guard: Kind::is_progressive reads the Verify, chain membership reads the Target that free_candidate makes from it.
        // The boss kind is only a label (the_big_one) and is never placed itself.
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, false));
        let mut checked = 0;
        for k in cat.kinds.iter().filter(|k| k.geom == Geom::None && k.family != "boss") {
            for mode in &k.modes {
                let z = ZoneCtx { zone: 1, mode: *mode, realm: &r, atlas: &a };
                let pool = street_pool(&z, SurfacePref::Any);
                let mut rng = StdRng::seed_from_u64(1);
                let (target, _, _) = free_candidate(k, &z, &pool, &zone_index(&z, &pool, SurfacePref::Any), &params(1), 20.0, &mut rng, &[])
                    .unwrap_or_else(|| panic!("{} gives no free quest", k.id));
                assert_eq!(k.is_progressive(), crate::chain::is_chain_target(&target), "{} in {mode:?}", k.id);
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    fn slot(i: i64, fam: &str, tier: u8, mode: Mode) -> SlotIn {
        SlotIn { location_id: i, zone: 1, mode, family: fam.into(), tier, boss: false }
    }

    #[test]
    fn reach_slots_land_in_their_effort_band_and_respect_min_distance() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, false));
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=10).map(|t| slot(i64::from(t), "reach", t, Mode::Walk)).collect();
        let out = assign(&slots, &z, &cat, &params(1));
        assert_eq!(out.len(), 10);
        for (s, o) in slots.iter().zip(&out) {
            assert_eq!(o.location_id, s.location_id);
            assert!(i16::from(tier_for(o.effort_min, 10.0)).abs_diff(i16::from(s.tier)) <= 1, "tier {} got effort {}", s.tier, o.effort_min);
            if let Target::Point { p, .. } = &o.target {
                assert!(distance_m(home(), *p) >= 150.0);
            }
        }
    }

    #[test]
    fn dwell_slots_use_real_benches_when_available_and_never_reuse_a_feature() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, true));
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=12).map(|i| slot(i, "dwell", 2 + (i % 4) as u8, Mode::Walk)).collect();
        let out = assign(&slots, &z, &cat, &params(3));
        assert!(out.iter().any(|o| o.kind_id == "bench_warmer" && matches!(o.target, Target::Dwell { .. })));
        let places: Vec<String> = out.iter().filter(|o| o.kind_id == "bench_warmer").map(|o| format!("{:?}", o.target)).collect();
        let mut dedup = places.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(places.len(), dedup.len());
    }

    #[test]
    fn banned_places_are_never_used_and_stop_being_offered() {
        use crate::marks::{Mark, Marks};
        let cat = Catalog::builtin();
        let (r, mut a) = (realm(Mode::Walk), atlas(&cat, true));
        let before = a.offers(&cat, &[Mode::Walk]).get("bench_warmer").copied().unwrap_or(0);
        assert_eq!(before, 30);
        let mut marks = Marks::default();
        for i in 0..30 {
            marks.set(&format!("n{i}"), Mark::Banned);
        }
        a.apply_marks(&marks);
        assert!(!a.offers(&cat, &[Mode::Walk]).contains_key("bench_warmer"), "a kind with every place banned is no longer on offer");
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=12).map(|i| slot(i, "dwell", 2 + (i % 4) as u8, Mode::Walk)).collect();
        let out = assign(&slots, &z, &cat, &params(3));
        assert!(out.iter().all(|o| o.kind_id != "bench_warmer"));
    }

    #[test]
    fn a_favorite_wins_over_an_equally_good_place_every_time() {
        use crate::marks::{Mark, Marks};
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let bench = |id: &str, bearing: f64| feature(id, &[("amenity", "bench")], destination(home(), bearing, 1440.0), vec![]); // on a street point (#51: places off the streets are not used)
        let streets: Vec<Point> = atlas(&cat, false).streets;
        let mut a = crate::scan::build_atlas("r", 0, vec![bench("n1", 0.0), bench("n2", 180.0)], streets, &cat);
        let mut marks = Marks::default();
        marks.set("n2", Mark::Favorite);
        a.apply_marks(&marks);
        for seed in 1..=20 {
            let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
            let out = assign(&[slot(1, "dwell", 3, Mode::Walk)], &z, &cat, &params(seed));
            let Target::Dwell { p, .. } = &out[0].target else { panic!("expected a bench dwell, got {:?}", out[0].target) };
            assert!(distance_m(*p, destination(home(), 180.0, 1440.0)) < 5.0, "seed {seed} did not pick the favorite");
        }
    }

    #[test]
    fn a_long_trail_asks_only_for_the_share_that_fits_the_effort_and_a_short_one_stays_whole() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let trail = |name: &str, len: f64| {
            let t0 = destination(home(), 90.0, 300.0);
            feature(
                &format!("L:trail_boss:{name}"),
                &[("highway", "path"), ("name", name)],
                t0,
                vec![t0, destination(t0, 90.0, len / 2.0), destination(t0, 90.0, len)],
            )
        };
        let make = |len: f64| {
            // the trail is a path itself, so a scan has its samples among the street points too
            let mut ways: Vec<Vec<Point>> = atlas(&cat, false).streets.into_iter().map(|p| vec![p]).collect();
            ways.push((0..=(len / 60.0) as i32).map(|i| destination(destination(home(), 90.0, 300.0), 90.0, 60.0 * f64::from(i))).collect());
            let a = atlas_of(vec![trail("Long Ridge", len)], ways, &cat);
            let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
            assign(&[slot(1, "trail", 3, Mode::Walk)], &z, &cat, &params(1)).remove(0)
        };
        let long = make(4000.0);
        let Target::Line { coverage, .. } = long.target else { panic!("expected a line, got {:?}", long.target) };
        assert!((0.25..0.9).contains(&coverage), "partial coverage {coverage}");
        assert!((long.effort_min - 25.0).abs() < 6.0, "effort {} should be near the 25 min asked for", long.effort_min);

        let short = make(900.0); // walking all of it is well under the effort asked for
        let Target::Line { coverage: whole, .. } = short.target else { panic!("expected a line") };
        assert!((whole - 0.9).abs() < 1e-9, "a short trail keeps the kind's own coverage, got {whole}");
    }

    #[test]
    fn family_with_no_places_falls_back_to_a_street_quest_and_says_so() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, false));
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let out = assign(&[slot(1, "landmark", 3, Mode::Walk)], &z, &cat, &params(1));
        assert!(out[0].fallback && out[0].kind_id == "street_smarts");
    }

    #[test]
    fn parks_trails_and_courier_produce_the_right_target_shapes() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, true));
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let out = assign(
            &[
                slot(1, "park", 3, Mode::Walk),
                slot(2, "trail", 6, Mode::Walk),
                slot(3, "courier", 4, Mode::Walk),
                slot(4, "steps", 3, Mode::Walk),
                slot(5, "explore", 3, Mode::Walk),
                slot(6, "away", 3, Mode::Walk),
            ],
            &z,
            &cat,
            &params(2),
        );
        // A park is a place to spend time in.
        assert!(matches!(out[0].target, Target::DwellArea { .. }), "{:?}", out[0].target);
        assert!(matches!(out[1].target, Target::Line { .. }), "{:?}", out[1].target);
        assert!(matches!(out[2].target, Target::Courier { .. } | Target::RoundTrip { .. } | Target::Collect { .. }));
        assert!(matches!(out[3].target, Target::Steps { .. }));
        assert!(matches!(out[4].target, Target::Cells { .. }));
        assert!(matches!(out[5].target, Target::Away { .. }));
        assert!(out.iter().all(|o| !o.fallback));
    }

    #[test]
    fn boss_gets_a_big_effort_and_drive_zones_never_get_walk_only_kinds() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Drive), atlas(&cat, true));
        let z = [ZoneCtx { zone: 1, mode: Mode::Drive, realm: &r, atlas: &a }];
        let mut slots: Vec<SlotIn> = (1..=15).map(|i| slot(i, if i % 2 == 0 { "landmark" } else { "dwell" }, 3, Mode::Drive)).collect();
        slots.push(SlotIn { location_id: 99, zone: 1, mode: Mode::Drive, family: "boss".into(), tier: 10, boss: true });
        let out = assign(&slots, &z, &cat, &params(5));
        for o in &out {
            let k = cat.kind(&o.kind_id);
            if let Some(k) = k {
                assert!(k.allows(Mode::Drive), "{} not allowed for drive", k.id);
            }
        }
        let boss = out.last().unwrap();
        assert!(boss.boss && boss.kind_id == "the_big_one" && boss.quest_name.starts_with("The Big One"));
        // A 9 km realm cannot offer 95 minutes of driving; the boss must still be the biggest thing available.
        let biggest_other = out[..out.len() - 1].iter().map(|o| o.effort_min).fold(0.0, f64::max);
        assert!(boss.effort_min >= biggest_other, "boss {} vs others {}", boss.effort_min, biggest_other);
    }

    #[test]
    fn surface_preference_controls_which_street_points_are_used() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        // paved points far east, rough points far west: easy to tell apart
        let mut a = atlas(&cat, false);
        let all = std::mem::take(&mut a.streets);
        a.streets = all.iter().copied().filter(|p| p.lon >= home().lon).collect();
        a.streets_rough = all.iter().copied().filter(|p| p.lon < home().lon).collect();
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=14).map(|i| slot(i, "reach", 2 + (i % 5) as u8, Mode::Walk)).collect();
        let lon_of = |o: &Assignment| if let Target::Point { p, .. } = &o.target { p.lon } else { f64::NAN };
        let mut p = params(2);
        p.surface = SurfacePref::PavedOnly;
        let paved_only = assign(&slots, &z, &cat, &p);
        assert!(paved_only.iter().all(|o| lon_of(o) >= home().lon), "paved only must never pick a rough point");
        p.surface = SurfacePref::PreferPaved;
        assert!(assign(&slots, &z, &cat, &p).iter().all(|o| lon_of(o) >= home().lon), "plenty of paved points exist");
        p.surface = SurfacePref::Any;
        let any = assign(&slots, &z, &cat, &p);
        assert!(any.iter().any(|o| lon_of(o) < home().lon), "any surface uses both pools");
        assert_eq!(SurfacePref::parse("paved_only"), SurfacePref::PavedOnly);
        assert_eq!(SurfacePref::parse("whatever"), SurfacePref::Any);
    }

    #[test]
    fn paved_only_skips_rough_trails_and_avoid_stairs_drops_the_stair_quest() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        // one dirt trail: the tag must survive stitching into the synthetic trail feature
        let t0 = destination(home(), 90.0, 900.0);
        let dirt = feature(
            "w9",
            &[("highway", "path"), ("surface", "dirt"), ("name", "Ridge Trail")],
            t0,
            vec![t0, destination(t0, 90.0, 800.0), destination(t0, 90.0, 1600.0)],
        );
        let base = atlas(&cat, false);
        let a = crate::scan::build_atlas("r", 0, vec![dirt], base.streets, &cat);
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots = vec![slot(1, "trail", 6, Mode::Walk)];
        let mut p = params(1);
        assert!(matches!(assign(&slots, &z, &cat, &p)[0].target, Target::Line { .. }), "a dirt trail is fine when any surface is accepted");
        p.surface = SurfacePref::PavedOnly;
        let o = &assign(&slots, &z, &cat, &p)[0];
        assert!(o.fallback && matches!(o.target, Target::Point { .. }), "paved only falls back instead of sending you on dirt");
        let mut b = atlas(&cat, false);
        let o2 = Point::new(40.0, -111.0);
        let stairs =
            Feature { id: "L:stairmaster:S:0".into(), point: o2, name: None, tags: BTreeMap::default(), geometry: vec![o2, destination(o2, 0.0, 200.0)] };
        b.features.push(stairs);
        b.matches.insert("stairmaster".into(), vec![b.features.len() - 1]);
        let zb = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &b }];
        let mut p2 = params(1);
        p2.avoid_stairs = true;
        assert_ne!(assign(&[slot(1, "trail", 3, Mode::Walk)], &zb, &cat, &p2)[0].kind_id, "stairmaster");
    }

    #[test]
    fn deterministic_for_a_seed_and_robust_to_empty_atlas() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, true));
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=8).map(|i| slot(i, "dwell", 3, Mode::Walk)).collect();
        let key = |v: &Vec<Assignment>| v.iter().map(|x| format!("{}{:?}", x.kind_id, x.target)).collect::<Vec<_>>();
        assert_eq!(key(&assign(&slots, &z, &cat, &params(9))), key(&assign(&slots, &z, &cat, &params(9))));
        let empty = Atlas::default();
        let z2 = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &empty }];
        let out = assign(&slots, &z2, &cat, &params(9));
        assert_eq!(out.len(), 8, "an empty atlas must still yield playable (fallback) quests");
    }

    // ---- The near-a-path rule (#51): every point a player must reach is within NEAR_PATH_M of a scanned street or path point. ----

    use crate::near_path::NEAR_PATH_M;

    /// A point `north_m` north and `east_m` east of `from`.
    fn at(from: Point, north_m: f64, east_m: f64) -> Point {
        destination(destination(from, 0.0, north_m), 90.0, east_m)
    }

    /// An atlas whose streets are `ways`, each a run of consecutive samples as a scan records them.
    fn atlas_of(features: Vec<Feature>, ways: Vec<Vec<Point>>, cat: &Catalog) -> Atlas {
        let runs = ways.iter().map(|w| crate::num::count_u32(w.len())).collect();
        let mut a = crate::scan::build_atlas("r", 0, features, ways.into_iter().flatten().collect(), cat);
        a.street_runs = runs;
        a
    }

    /// Streets sampled every 60 m (like a scan) on a grid 240 m apart, out to `half_m` from home: one way per street.
    fn town_streets(half_m: f64) -> Vec<Vec<Point>> {
        let (lines, steps) = ((half_m / 240.0) as i32, (half_m / 60.0) as i32);
        let mut out = Vec::new();
        for l in -lines..=lines {
            let a = f64::from(l) * 240.0;
            out.push((-steps..=steps).map(|s| at(home(), a, f64::from(s) * 60.0)).collect()); // an east-west street
            out.push((-steps..=steps).map(|s| at(home(), f64::from(s) * 60.0, a)).collect());
            // a north-south street
        }
        out
    }

    /// Every point the player must physically reach for a target.
    fn must_reach(t: &Target) -> Vec<Point> {
        match t {
            Target::Point { p, .. } | Target::Dwell { p, .. } => vec![*p],
            Target::DwellArea { center, .. } => vec![*center],
            Target::Line { pts, .. } => pts.first().copied().into_iter().collect(),
            Target::Courier { a, b, .. } => vec![*a, *b],
            Target::RoundTrip { far, .. } => vec![*far],
            Target::Collect { pts, .. } => pts.clone(),
            Target::Cells { .. } | Target::Steps { .. } | Target::Away { .. } => vec![],
        }
    }

    /// How far `p` is from the atlas's paths: its street points and the pieces of street between consecutive points of a way.
    fn gap_to_paths(p: Point, a: &Atlas) -> f64 {
        let pts = a.streets.iter().chain(&a.streets_rough).map(|s| distance_m(p, *s));
        let links = a.street_links(false).into_iter().chain(a.street_links(true)).map(|(x, y)| crate::geo::distance_to_segment_m(p, x, y));
        pts.chain(links).fold(f64::MAX, f64::min)
    }

    /// How close to a path counts as on it (the snapped spot is computed on the path; this only absorbs rounding).
    const ON_PATH_M: f64 = 0.5;

    /// A place to reach is marked on the path in front of it, never in a backyard; an area to spend time in is marked inside it
    /// (public ground) or on a path. Lines start where they first meet a path, as before.
    fn assert_on_path_or_in_its_area(o: &Assignment, a: &Atlas, ctx: &str) {
        let inside = |q: Point, poly: &[Point]| poly.len() >= 3 && point_in_polygon(q, poly);
        let points = match &o.target {
            Target::Line { .. } => vec![],
            Target::DwellArea { center, poly, .. } => vec![*center].into_iter().filter(|c| !inside(*c, poly)).collect(),
            t => must_reach(t),
        };
        for q in points {
            let gap = gap_to_paths(q, a);
            assert!(gap < ON_PATH_M, "{ctx}: {} ({}) at {}: point {gap:.1} m off the path", o.kind_id, o.place, o.location_id);
        }
    }

    #[test]
    fn a_park_marker_sits_inside_the_park_on_its_own_path() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let c = at(home(), 1320.0, 1320.0);
        let (park, keep) = park_hole(c, 250.0, 250.0);
        let mut a = atlas_of(vec![park.clone()], town_streets(4000.0), &cat);
        a.retain_streets(&keep);
        // a footpath across the park 15 m east of the spot the marker starts from: close enough that the spot itself used to be accepted
        let mid = point_inside(&park.geometry);
        let mut fp = atlas_of(vec![], vec![(0..9).map(|i| at(mid, f64::from(i) * 50.0 - 200.0, 15.0)).collect()], &cat);
        a.streets.append(&mut fp.streets);
        a.street_runs.append(&mut fp.street_runs);
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=6).map(|i| slot(i, "park", 1 + i as u8, Mode::Walk)).collect();
        let mut seen = 0;
        for seed in 1..=6 {
            for o in assign(&slots, &z, &cat, &params(seed)) {
                if let Target::DwellArea { center, .. } = o.target {
                    if o.place == "Big Park" {
                        seen += 1;
                        assert!(point_in_polygon(center, &park.geometry), "seed {seed}: the marker is inside the park");
                        assert!(gap_to_paths(center, &a) < ON_PATH_M, "seed {seed}: and on the park's own path");
                    }
                }
            }
        }
        assert!(seen > 0, "the park is used");
    }

    /// Checks every point a target needs: the points to reach, and enough samples of a line beside a path for its coverage.
    fn assert_reachable(o: &Assignment, a: &Atlas, ctx: &str) {
        for q in must_reach(&o.target) {
            let gap = gap_to_paths(q, a);
            assert!(gap <= NEAR_PATH_M + 1e-6, "{ctx}: {} ({}) at {}: point {gap:.0} m from any path", o.kind_id, o.place, o.location_id);
        }
        if let Target::Line { pts, corridor_m, coverage } = &o.target {
            let dense = crate::geo::densify(pts, LINE_SAMPLE_M);
            let near = dense.iter().filter(|q| gap_to_paths(**q, a) <= corridor_m.min(NEAR_PATH_M) + 1e-6).count();
            let share = count_f64(near) / count_f64(dense.len());
            assert!(share + 1e-9 >= *coverage, "{ctx}: {} ({}) asks for {coverage:.2} of a line only {share:.2} beside a path", o.kind_id, o.place);
        }
    }

    /// A square park of `half_m` around `c`, and a filter that takes the town's streets out of it (and `clear_m` around its centre).
    fn park_hole(c: Point, half_m: f64, clear_m: f64) -> (Feature, impl Fn(Point) -> bool) {
        let (sw, ne) = (at(c, -clear_m, -clear_m), at(c, clear_m, clear_m));
        let keep = move |p: Point| !(sw.lat..=ne.lat).contains(&p.lat) || !(sw.lon..=ne.lon).contains(&p.lon);
        let ring = vec![at(c, -half_m, -half_m), at(c, -half_m, half_m), at(c, half_m, half_m), at(c, half_m, -half_m), at(c, -half_m, -half_m)];
        (feature("wpark", &[("leisure", "park"), ("name", "Big Park")], c, ring), keep)
    }

    /// A town with on-street and backyard places, a big park whose centre is far from any path but with a footpath around it,
    /// a river that starts in backyards and a trail.
    fn town(cat: &Catalog) -> Atlas {
        let c = at(home(), 1320.0, 1320.0); // the middle of a block
        let (park, keep) = park_hole(c, 250.0, 250.0);
        let ways = town_streets(6000.0);
        // a footpath 10 m outside the park, sampled every 60 m along each side
        let side = |f: &dyn Fn(f64) -> Point| (0..9).map(|i| f(f64::from(i) * 60.0 - 260.0)).collect::<Vec<_>>();
        let footpath = [side(&|s| at(c, -260.0, s)), side(&|s| at(c, 260.0, s)), side(&|s| at(c, s, -260.0)), side(&|s| at(c, s, 260.0))];
        let mut features = vec![park];
        for i in 0..40 {
            let (n, e) = (f64::from(i % 8) * 240.0 - 960.0, f64::from(i / 8) * 240.0 - 480.0);
            // half on a street corner, half in the middle of a block (120 m from every street)
            let off = if i % 2 == 0 { 0.0 } else { 120.0 };
            features.push(feature(&format!("b{i}"), &[("amenity", "bench")], at(home(), n + off, e + off), vec![]));
            features.push(feature(&format!("m{i}"), &[("tourism", "museum"), ("name", &format!("Museum {i}"))], at(home(), n + off + 480.0, e + off), vec![]));
            features.push(feature(&format!("g{i}"), &[("leisure", "park"), ("name", &format!("Green {i}"))], at(home(), n + off, e + off + 480.0), vec![]));
        }
        let r0 = at(home(), -1500.0, -1500.0 + 120.0); // starts mid-block, then runs east across many streets
        features.push(feature("wriver", &[("waterway", "river"), ("name", "Long River")], r0, (0..20).map(|i| at(r0, 120.0, f64::from(i) * 100.0)).collect()));
        let t0 = at(home(), -720.0, 0.0);
        features.push(feature("wtrail", &[("highway", "path"), ("name", "Ridge Trail")], t0, (0..15).map(|i| at(t0, 0.0, f64::from(i) * 100.0)).collect()));
        let mut a = atlas_of(features, ways, cat);
        a.retain_streets(&keep);
        let mut fp = atlas_of(vec![], Vec::from(footpath), cat);
        a.streets.append(&mut fp.streets);
        a.street_runs.append(&mut fp.street_runs);
        a
    }

    fn every_slot() -> Vec<SlotIn> {
        let mut out = Vec::new();
        for (z, mode) in [Mode::Walk, Mode::Run, Mode::Bike, Mode::Drive].into_iter().enumerate() {
            let zone = z as u32 + 1;
            for fam in crate::solo::FAMILIES {
                for tier in [1, 3, 5, 8, 10] {
                    let id = out.len() as i64 + 1;
                    out.push(SlotIn { location_id: id, zone, mode, family: fam.into(), tier, boss: false });
                }
            }
            out.push(SlotIn { location_id: out.len() as i64 + 1, zone, mode, family: "boss".into(), tier: 10, boss: true });
        }
        out
    }

    #[test]
    fn every_quest_point_of_every_kind_and_mode_is_near_a_path() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let a = town(&cat);
        let zones: Vec<ZoneCtx<'_>> = [Mode::Walk, Mode::Run, Mode::Bike, Mode::Drive]
            .into_iter()
            .enumerate()
            .map(|(i, mode)| ZoneCtx { zone: i as u32 + 1, mode, realm: &r, atlas: &a })
            .collect();
        let slots = every_slot();
        let mut kinds = BTreeSet::new();
        for seed in 1..=4 {
            for o in assign(&slots, &zones, &cat, &params(seed)) {
                kinds.insert(if matches!(o.target, Target::Courier { .. }) { "courier".to_string() } else { o.kind_id.clone() });
                assert_reachable(&o, &a, &format!("seed {seed}"));
                assert_on_path_or_in_its_area(&o, &a, &format!("seed {seed}"));
            }
        }
        // the fixture really exercises feature places of every shape, not only street points
        for want in ["bench_warmer", "museum_mile", "touch_grass", "follow_the_flow", "trail_boss", "courier"] {
            assert!(kinds.contains(want), "{want} never placed: {kinds:?}");
        }
    }

    #[test]
    fn a_sparse_zone_never_gets_grid_points() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        // only 8 street points (fewer than the old 20-point threshold), along one short street 600 m east of home
        let streets: Vec<Point> = (0..8).map(|i| at(home(), 0.0, 600.0 + 60.0 * f64::from(i))).collect();
        let a = atlas_of(vec![], vec![streets], &cat);
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=12).map(|i| slot(i, ["reach", "dwell", "courier"][(i % 3) as usize], 2 + (i % 5) as u8, Mode::Walk)).collect();
        let out = assign(&slots, &z, &cat, &params(4));
        assert_eq!(out.len(), slots.len(), "every slot still gets a quest");
        for o in &out {
            assert_reachable(o, &a, "sparse zone");
        }
    }

    #[test]
    fn a_big_park_gets_a_point_on_its_path_or_is_not_used() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let c = at(home(), 1320.0, 1320.0);
        let slots: Vec<SlotIn> = (1..=6).map(|i| slot(i, "park", 1 + i as u8, Mode::Walk)).collect();

        // A footpath runs around the park: the quest is snapped onto it (or into the park next to it), never the far-away centre.
        let (park, keep) = park_hole(c, 250.0, 250.0);
        let mut ways = town_streets(4000.0);
        ways.push((0..9).map(|i| at(c, -260.0, f64::from(i) * 60.0 - 240.0)).collect());
        let mut a = atlas_of(vec![park], ways, &cat);
        a.retain_streets(&keep);
        assert!(gap_to_paths(c, &a) > 200.0, "the fixture centre is far from any path");
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let mut used = 0;
        for seed in 1..=6 {
            for o in assign(&slots, &z, &cat, &params(seed)) {
                used += usize::from(o.place == "Big Park");
                assert_reachable(&o, &a, &format!("seed {seed}"));
            }
        }
        assert!(used > 0, "a park with a path along it is still used");

        // Nothing walkable within 30 m of the park at all: it is never a quest.
        let (park, keep) = park_hole(c, 250.0, 320.0);
        let mut a = atlas_of(vec![park], town_streets(4000.0), &cat);
        a.retain_streets(&keep);
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        for seed in 1..=6 {
            assert!(assign(&slots, &z, &cat, &params(seed)).iter().all(|o| o.place != "Big Park"), "seed {seed} used an unreachable park");
        }
    }

    /// Street points every 30 m in both directions out to `half_m` from home: room for any forager.
    fn fine_streets(half_m: f64) -> Vec<Point> {
        let n = (half_m / 30.0) as i32;
        (-n..=n).flat_map(|i| (-n..=n).map(move |j| at(home(), f64::from(i) * 30.0, f64::from(j) * 30.0))).collect()
    }

    #[test]
    fn forager_places_twice_the_need_spaced_near_paths_away_from_home_and_spread_outward() {
        let cat = Catalog::builtin();
        let k = cat.kind("forager").unwrap();
        let r = realm(Mode::Walk);
        let a = crate::scan::build_atlas("r", 0, vec![], fine_streets(3700.0), &cat);
        for (mode, tier, need) in [(Mode::Walk, 1, 3), (Mode::Walk, 2, 5), (Mode::Run, 3, 7), (Mode::Walk, 4, 10), (Mode::Bike, 3, 7), (Mode::Walk, 7, 10)] {
            let z = ZoneCtx { zone: 1, mode, realm: &r, atlas: &a };
            let pool = street_pool(&z, SurfacePref::Any);
            let mut p = params(3);
            p.min_distance_m = 0.0; // the home-radius floor must hold on its own (Review Focus 3)
            let want = mid(tier, p.minutes_per_tier);
            let (t, effort, place) =
                free_candidate(k, &z, &pool, &zone_index(&z, &pool, p.surface), &p, want, &mut StdRng::seed_from_u64(3), &[]).expect("a dense town has room");
            let Target::Collect { pts, need: n, r: pick, theme } = &t else { panic!("{t:?}") };
            assert_eq!(*n, need, "{mode:?} tier {tier} (tiers above 4 use the last value)");
            assert_eq!(pts.len(), 2 * need as usize);
            assert!((pick - 25.0).abs() < f64::EPSILON);
            assert!(FORAGE_THEMES.contains(&theme.as_str()), "{theme}");
            assert_eq!(place, "Around home");
            let floor = HOME_RADIUS_M + 25.0;
            for (i, q) in pts.iter().enumerate() {
                assert!(gap_to_paths(*q, &a) <= NEAR_PATH_M + 1e-6, "item {i} is off the paths");
                assert!(pool.contains(q), "item {i} is not a street point of the zone");
                assert!(distance_m(home(), *q) >= floor, "item {i} is {:.0} m from home", distance_m(home(), *q));
                for o in &pts[..i] {
                    assert!(distance_m(*o, *q) >= 60.0 - 1e-6, "items {:.0} m apart", distance_m(*o, *q));
                }
            }
            let d: Vec<f64> = pts.iter().map(|q| distance_m(home(), *q)).collect();
            assert!(d.windows(2).all(|w| w[0] <= w[1]), "nearest first, so fog reveals the quest on the way (Review Focus 2)");
            let far = dist_for(want / 2.0, mode).max(floor + 120.0);
            let last = d[d.len() - 1];
            assert!((0.8 * far..=1.5 * far).contains(&last), "{mode:?} tier {tier}: farthest {last:.0} m, wanted about {far:.0} m");
            if far > 600.0 {
                assert!(d[0] < 0.6 * last, "spread between home and the farthest: {d:?}");
            }
            assert!((effort - 2.0 * travel_min(last, mode)).abs() < 1e-9, "effort is out to the farthest and back");
        }
    }

    #[test]
    fn a_sparse_zone_gets_no_forager_and_falls_back_to_another_courier_kind() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let streets: Vec<Point> = (0..8).map(|i| at(home(), 0.0, 600.0 + 60.0 * f64::from(i))).collect();
        let a = crate::scan::build_atlas("r", 0, vec![], streets, &cat);
        let z = ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a };
        let pool = street_pool(&z, SurfacePref::Any);
        let k = cat.kind("forager").unwrap();
        assert!(
            free_candidate(k, &z, &pool, &zone_index(&z, &pool, SurfacePref::Any), &params(1), 25.0, &mut StdRng::seed_from_u64(1), &[]).is_none(),
            "8 points cannot hold 14 items 60 m apart"
        );
        let slots: Vec<SlotIn> = (1..=8).map(|i| slot(i, "courier", 1 + (i % 4) as u8, Mode::Walk)).collect();
        for seed in 1..=4 {
            let out = assign(&slots, &[ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }], &cat, &params(seed));
            assert_eq!(out.len(), slots.len());
            assert!(out.iter().all(|o| o.kind_id != "forager"), "seed {seed}");
        }
    }

    #[test]
    fn forager_quests_join_the_courier_family_with_a_themed_title_but_never_drive() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, false));
        let mut seen = 0;
        for seed in 1..=8 {
            let zones = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }, ZoneCtx { zone: 2, mode: Mode::Drive, realm: &r, atlas: &a }];
            let mut slots: Vec<SlotIn> = (1..=4).map(|t| slot(i64::from(t), "courier", t, Mode::Walk)).collect();
            slots.extend((1..=4).map(|t| SlotIn {
                location_id: 10 + i64::from(t),
                zone: 2,
                mode: Mode::Drive,
                family: "courier".into(),
                tier: t,
                boss: false,
            }));
            for o in assign(&slots, &zones, &cat, &params(seed)) {
                if o.kind_id != "forager" {
                    continue;
                }
                assert_eq!(o.zone, 1, "no forager in a drive zone");
                let Target::Collect { need, theme, .. } = &o.target else { panic!("{:?}", o.target) };
                assert_eq!(o.quest_name, format!("Forager: bring home {need} {theme}"));
                seen += 1;
            }
        }
        assert!(seen > 0, "forager is offered to courier slots");
    }

    /// A straight street east of home sampled every `gap` m (a scan's samples at stride `gap / 60`).
    fn street_east(gap: f64, n: i32) -> Vec<Point> {
        (0..n).map(|i| at(home(), 0.0, 300.0 + gap * f64::from(i))).collect()
    }

    #[test]
    fn a_place_beside_the_street_between_two_samples_is_used_from_the_street_at_stride_one_and_two() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        for (gap, stride) in [(60.0, 1), (120.0, 2)] {
            // 20 m off the street, halfway between two samples: 36 m (stride 1) or 63 m (stride 2) from the nearest sample
            let spot = at(home(), 20.0, 300.0 + gap * 10.5);
            let mut a = atlas_of(vec![feature("n1", &[("amenity", "bench")], spot, vec![])], vec![street_east(gap, 40)], &cat);
            a.street_stride = stride;
            let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
            let used = (1..=10).any(|seed| {
                assign(&[slot(1, "dwell", 2, Mode::Walk)], &z, &cat, &params(seed))
                    .iter()
                    .any(|o| matches!(o.target, Target::Dwell { p, .. } if distance_m(p, spot) <= NEAR_PATH_M && gap_to_paths(p, &a) < ON_PATH_M))
            });
            assert!(used, "a bench 20 m from the street is used, its point on the street in front of it (stride {stride})");
        }
    }

    #[test]
    fn a_river_mostly_behind_houses_is_dropped_or_asks_only_for_its_share_beside_a_path() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let river = |beside_m: f64| {
            // runs 15 m beside the street for `beside_m`, then turns 400 m away behind the houses and carries on
            let s = at(home(), 15.0, 300.0);
            let e = at(home(), 15.0, 300.0 + beside_m);
            feature("wriver", &[("waterway", "river"), ("name", "Back River")], s, vec![s, e, at(e, 400.0, 0.0), at(e, 400.0, 1200.0)])
        };
        let slots: Vec<SlotIn> = (1..=3).map(|i| slot(i, "water", 2 + i as u8, Mode::Walk)).collect();
        let a = atlas_of(vec![river(100.0)], vec![street_east(60.0, 60)], &cat);
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        for seed in 1..=4 {
            assert!(assign(&slots, &z, &cat, &params(seed)).iter().all(|o| o.place != "Back River"), "a river barely beside a path is not a quest");
        }
        let a = atlas_of(vec![river(900.0)], vec![street_east(60.0, 60)], &cat);
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let mut used = 0;
        for seed in 1..=4 {
            for o in assign(&slots, &z, &cat, &params(seed)) {
                used += usize::from(o.place == "Back River");
                assert_reachable(&o, &a, &format!("seed {seed}"));
            }
        }
        assert!(used > 0, "a river beside a path for a good stretch is still a quest");
    }

    #[test]
    fn a_park_bordering_home_snaps_to_a_path_point_far_enough_from_home() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        // home sits on the park's south edge; a path runs just south of it (by the house) and one 25 m north of it
        let ring = vec![at(home(), 0.0, -200.0), at(home(), 0.0, 200.0), at(home(), 400.0, 200.0), at(home(), 400.0, -200.0), at(home(), 0.0, -200.0)];
        let park = feature("wpark", &[("leisure", "park"), ("name", "Home Park")], at(home(), 200.0, 0.0), ring);
        let south: Vec<Point> = (0..7).map(|i| at(home(), -10.0, f64::from(i) * 60.0 - 180.0)).collect();
        let north: Vec<Point> = (0..7).map(|i| at(home(), 425.0, f64::from(i) * 60.0 - 180.0)).collect();
        let a = atlas_of(vec![park], vec![south, north], &cat);
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=3).map(|i| slot(i, "park", 2 + i as u8, Mode::Walk)).collect();
        let mut used = 0;
        for seed in 1..=6 {
            for o in assign(&slots, &z, &cat, &params(seed)).iter().filter(|o| o.place == "Home Park") {
                used += 1;
                for q in must_reach(&o.target) {
                    assert!(distance_m(home(), q) >= 150.0, "seed {seed}: {} lands {:.0} m from home", o.kind_id, distance_m(home(), q));
                }
            }
        }
        assert!(used > 0, "the far path still makes the park a quest");
    }

    #[test]
    fn two_parks_snapped_onto_the_same_path_never_share_a_spot() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        // two parks side by side; the only path runs along the edge they share
        let square = |e0: f64| {
            vec![at(home(), 1000.0, e0), at(home(), 1000.0, e0 + 300.0), at(home(), 1300.0, e0 + 300.0), at(home(), 1300.0, e0), at(home(), 1000.0, e0)]
        };
        let parks = vec![
            feature("wa", &[("leisure", "park"), ("name", "Park A")], at(home(), 1150.0, 150.0), square(0.0)),
            feature("wb", &[("leisure", "park"), ("name", "Park B")], at(home(), 1150.0, 450.0), square(300.0)),
        ];
        let streets: Vec<Point> = (0..6).map(|i| at(home(), 1000.0 + 60.0 * f64::from(i), 300.0)).collect();
        let a = atlas_of(parks, vec![streets], &cat);
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let slots: Vec<SlotIn> = (1..=2).map(|i| slot(i, "park", 3, Mode::Walk)).collect();
        let mut both = 0;
        for seed in 1..=8 {
            let out = assign(&slots, &z, &cat, &params(seed));
            both += usize::from(out.iter().any(|o| o.place == "Park A") && out.iter().any(|o| o.place == "Park B"));
            let anchors: Vec<Point> = out.iter().filter(|o| !o.fallback).filter_map(|o| must_reach(&o.target).first().copied()).collect();
            for (i, x) in anchors.iter().enumerate() {
                for y in &anchors[i + 1..] {
                    assert!(distance_m(*x, *y) >= SPACING_M, "seed {seed}: two quests {:.0} m apart", distance_m(*x, *y));
                }
            }
        }
        assert!(both > 0, "the shared path is long enough for both parks");
    }

    #[test]
    fn replacing_unpicked_items_keeps_the_picked_ones_and_the_rules() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let a = atlas(&cat, false);
        let z = ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a };
        let pool = street_pool(&z, SurfacePref::Any);
        let p = params(5);
        let index = zone_index(&z, &pool, p.surface);
        let (t, _, _) =
            free_candidate(cat.kind("forager").unwrap(), &z, &pool, &index, &p, mid(2, p.minutes_per_tier), &mut StdRng::seed_from_u64(5), &[]).unwrap();
        let Target::Collect { pts: old, need: n0, r: r0, theme: th0 } = &t else { panic!("{t:?}") };
        let picked = BTreeSet::from([0u16, 3]);
        let fresh = replace_unpicked(&t, &picked, &z, &index, &p, 2, &mut StdRng::seed_from_u64(77)).expect("a dense zone has room");
        let Target::Collect { pts, need, r: pick, theme } = &fresh else { panic!("{fresh:?}") };
        assert_eq!((need, theme), (n0, th0));
        assert!((pick - r0).abs() < f64::EPSILON);
        assert_eq!(pts.len(), old.len());
        assert_eq!((pts[0], pts[3]), (old[0], old[3]), "picked items stay where they were");
        let moved = (0..pts.len()).filter(|i| !picked.contains(&(*i as u16)) && pts[*i] != old[*i]).count();
        assert!(moved > 0, "unpicked items move");
        let (min_m, _) = item_band(&p, *r0, mid(2, p.minutes_per_tier), Mode::Walk);
        for (i, q) in pts.iter().enumerate() {
            assert!(pool.contains(q) && distance_m(home(), *q) >= min_m, "item {i}");
            for o in &pts[..i] {
                assert!(distance_m(*o, *q) >= 60.0 - 1e-6, "items {:.0} m apart", distance_m(*o, *q));
            }
        }
        let unpicked: Vec<f64> = (0..pts.len()).filter(|i| !picked.contains(&(*i as u16))).map(|i| distance_m(home(), pts[i])).collect();
        assert!(unpicked.windows(2).all(|w| w[0] <= w[1]), "nearest home first when placed: {unpicked:?}");
        assert!(replace_unpicked(&Target::Point { p: home(), r: 40.0 }, &picked, &z, &index, &p, 2, &mut StdRng::seed_from_u64(1)).is_none(), "only foragers");
    }

    #[test]
    fn a_zone_with_too_few_street_points_leaves_a_forager_quest_as_it_is() {
        let cat = Catalog::builtin();
        let r = realm(Mode::Walk);
        let streets: Vec<Point> = (0..8).map(|i| at(home(), 0.0, 600.0 + 60.0 * f64::from(i))).collect();
        let a = crate::scan::build_atlas("r", 0, vec![], streets, &cat);
        let z = ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a };
        let pool = street_pool(&z, SurfacePref::Any);
        let index = zone_index(&z, &pool, SurfacePref::Any);
        let t = Target::Collect { pts: (0..6).map(|i| at(home(), 900.0 + 100.0 * f64::from(i), 0.0)).collect(), need: 3, r: 25.0, theme: "gems".into() };
        assert!(replace_unpicked(&t, &BTreeSet::new(), &z, &index, &params(1), 1, &mut StdRng::seed_from_u64(1)).is_none());
        let empty = Atlas::default();
        let ez = ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &empty };
        let epool = street_pool(&ez, SurfacePref::Any);
        let eindex = zone_index(&ez, &epool, SurfacePref::Any);
        assert!(replace_unpicked(&t, &BTreeSet::new(), &ez, &eindex, &params(1), 1, &mut StdRng::seed_from_u64(1)).is_none(), "an empty atlas");
    }
}

#[cfg(test)]
mod goal_text_tests {
    use super::*;
    use crate::geo::{destination, Point};

    #[test]
    fn an_old_save_with_a_round_trip_time_limit_still_loads() {
        let old = r#"{"RoundTrip":{"far":{"lat":40.0,"lon":-111.0},"r":50.0,"time_limit_min":42.4}}"#;
        let t: Target = serde_json::from_str(old).expect("old saves must keep loading");
        assert!(matches!(t, Target::RoundTrip { r, .. } if (r - 50.0).abs() < f64::EPSILON));
    }

    #[test]
    fn every_target_kind_says_what_to_do() {
        let p = Point::new(40.0, -111.0);
        let cases = [
            (Target::Point { p, r: 40.0 }, "Get within 40 m"),
            (Target::Dwell { p, r: 40.0, minutes: 3.0 }, "Stay 3 min within 40 m"),
            (Target::DwellArea { poly: vec![], center: p, r: 40.0, minutes: 5.0 }, "Spend 5 min inside the area"),
            (Target::Line { pts: vec![p, destination(p, 0.0, 1000.0)], corridor_m: 25.0, coverage: 0.9 }, "Cover 90% of this 1 km path"),
            (Target::Courier { a: p, b: p, r: 40.0, time_limit_min: 12.0 }, "Pick up at A, deliver to B within 12 min"),
            (Target::RoundTrip { far: p, r: 50.0 }, "Reach the far point, then come back home"),
            (Target::Cells { n: 12, cell_m: 100.0 }, "Visit 12 new map cells"),
            (Target::Steps { n: 500 }, "Take 500 steps"),
            (Target::Away { minutes: 20.0 }, "Spend 20 min away from home"),
            (Target::Collect { pts: vec![p], need: 5, r: 25.0, theme: "acorns".into() }, "Bring home 5 acorns (pick up within 25 m)"),
        ];
        for (t, want) in cases {
            assert_eq!(t.goal_text(UnitSystem::Metric), want);
        }
    }

    #[test]
    fn target_distances_read_in_the_players_units() {
        let p = Point::new(40.0, -111.0);
        assert_eq!(Target::Point { p, r: 17.07 }.goal_text(UnitSystem::Imperial), "Get within 50 ft", "56 ft rounds down: never promise more room");
        assert_eq!(Target::Dwell { p, r: 30.0, minutes: 5.0 }.goal_text(UnitSystem::Imperial), "Stay 5 min within 90 ft");
        let line = Target::Line { pts: vec![p, destination(p, 0.0, 1609.344 * 1.5)], corridor_m: 25.0, coverage: 0.9 };
        assert_eq!(line.goal_text(UnitSystem::Imperial), "Cover 90% of this 1.5 mi path");
    }
}
