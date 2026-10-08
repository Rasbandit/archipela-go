//! Assign each apworld slot a concrete quest at a real place, using what the slot's realm offers.

use std::collections::BTreeSet;

use rand::rngs::StdRng;
use rand::seq::{IndexedRandom, SliceRandom};
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, Geom, Kind, Mode, Verify};
use crate::effort::{cadence_steps_per_min, mid, travel_min};
use crate::fill::lattice;
use crate::geo::{bearing_deg, distance_m, point_inside, polyline_len_m, Point};
use crate::realm::Realm;
use crate::scan::{Atlas, Feature};

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
        /// Minimum distance from home, in metres.
        min_distance_m: f64,
        /// How long to stay away, in minutes.
        minutes: f64,
    },
}

impl Target {
    /// What the player has to do, in one line ("Get within 40 m").
    #[must_use]
    pub fn goal_text(&self) -> String {
        match self {
            Self::Point { r, .. } => format!("Get within {r:.0} m"),
            Self::Dwell { r, minutes, .. } => format!("Stay {minutes:.0} min within {r:.0} m"),
            Self::DwellArea { minutes, .. } => format!("Spend {minutes:.0} min inside the area"),
            Self::Line { pts, coverage, .. } => format!("Cover {:.0}% of this {:.1} km path", coverage * 100.0, polyline_len_m(pts) / 1000.0),
            Self::Courier { time_limit_min, .. } => format!("Pick up at A, deliver to B within {time_limit_min:.0} min"),
            Self::RoundTrip { .. } => "Reach the far point, then come back home".to_string(),
            Self::Cells { n, .. } => format!("Visit {n} new map cells"),
            Self::Steps { n } => format!("Take {n} steps"),
            Self::Away { min_distance_m, minutes } => format!("Spend {minutes:.0} min at least {:.1} km from home", min_distance_m / 1000.0),
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

fn street_pool(z: &ZoneCtx<'_>, pref: SurfacePref) -> Vec<Point> {
    let (paved, rough) = (&z.atlas.streets, &z.atlas.streets_rough);
    let all = || paved.iter().chain(rough.iter()).copied().collect::<Vec<_>>();
    let pool = match pref {
        SurfacePref::PreferPaved if paved.len() >= 50 => paved.clone(),
        SurfacePref::PavedOnly => paved.clone(),
        SurfacePref::Any | SurfacePref::PreferPaved => all(),
    };
    if pool.len() >= 20 {
        pool
    } else {
        lattice(&z.realm.shape.to_zone(), 150.0).into_iter().map(|c| c.point).collect()
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

fn feature_target(k: &Kind, f: &Feature, mode: Mode, home: Point, want: f64) -> Option<(Target, f64)> {
    let to_f = distance_m(home, f.point);
    match &k.verify {
        Verify::Reach { radius_m } => Some((Target::Point { p: f.point, r: *radius_m }, travel_min(to_f, mode))),
        Verify::Dwell { minutes, radius_m } => Some((Target::Dwell { p: f.point, r: *radius_m, minutes: *minutes }, travel_min(to_f, mode) + minutes)),
        Verify::DwellInArea { minutes } => {
            let poly = if f.geometry.len() >= 3 { f.geometry.clone() } else { vec![] };
            // The OSM "center" can fall outside a concave park; always target a point inside the outline.
            let center = if poly.len() >= 3 { point_inside(&poly) } else { f.point };
            Some((Target::DwellArea { poly, center, r: 80.0, minutes: *minutes }, travel_min(distance_m(home, center), mode) + minutes))
        }
        Verify::FollowLine { corridor_m, coverage, min_len_m, max_len_m } => {
            let len = polyline_len_m(&f.geometry);
            if f.geometry.len() < 2 || len < *min_len_m || len > *max_len_m {
                return None;
            }
            let nearest = f.geometry.iter().map(|p| distance_m(home, *p)).fold(f64::MAX, f64::min);
            let pace = mode.m_per_min() * if mode == Mode::Walk { 0.8 } else { 1.0 };
            // Ask for the share of the line that fits the effort wanted, so a long trail makes a fair quest too: never above the kind's own
            // share, and never less than a quarter of it (or 150 m).
            let travel = travel_min(nearest, mode);
            let full = len / pace;
            let floor = MIN_TRAIL_SHARE.max(150.0 / len).min(*coverage);
            let share = ((want - travel) / full).clamp(floor, *coverage);
            Some((Target::Line { pts: f.geometry.clone(), corridor_m: *corridor_m, coverage: share }, travel + share * full))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)] // pre-existing: flat argument lists keep the exported/geometry call sites explicit
#[allow(clippy::many_single_char_names)] // short names for zone/pool/params mirror the geometry vocabulary used across this module
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // counts are rounded then clamped to a small range before the cast
fn free_candidate(
    k: &Kind,
    z: &ZoneCtx<'_>,
    pool: &[Point],
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
            let n = ((want * mode.m_per_min() * 0.7 / cell_m).round() as u32).clamp(3, 60);
            Some((Target::Cells { n, cell_m: *cell_m }, f64::from(n) * cell_m / (mode.m_per_min() * 0.7), "New map cells".into()))
        }
        Verify::Steps { .. } => {
            let n = ((want * cadence_steps_per_min(mode)).round() as u32).clamp(500, 40_000);
            Some((Target::Steps { n }, f64::from(n) / cadence_steps_per_min(mode), "Anywhere".into()))
        }
        Verify::Away { .. } => {
            let minutes = (want * 3.0).clamp(30.0, 480.0);
            Some((Target::Away { min_distance_m: 800.0 + 300.0 * (want / p.minutes_per_tier + 0.5), minutes }, want, "Away from home".into()))
        }
        _ => None,
    }
}

fn one(
    s: &SlotIn,
    z: &ZoneCtx<'_>,
    catalog: &Catalog,
    p: &AssignParams,
    rng: &mut StdRng,
    used_feat: &mut BTreeSet<String>,
    used_pts: &mut Vec<Point>,
) -> Assignment {
    let want = mid(s.tier, p.minutes_per_tier);
    let kinds: Vec<&Kind> = catalog
        .kinds
        .iter()
        .filter(|k| k.allows(z.mode) && k.family != "boss" && (s.boss || k.family == s.family) && !(p.avoid_stairs && k.id == "stairmaster"))
        .collect();
    let pool = street_pool(z, p.surface);
    let mut cands: Vec<Cand> = Vec::new();
    for k in &kinds {
        if k.geom != Geom::None {
            let Some(hits) = z.atlas.matches.get(&k.id) else { continue };
            let mut idx: Vec<usize> = hits.clone();
            idx.shuffle(rng);
            for fi in idx.into_iter().take(300) {
                let f = &z.atlas.features[fi];
                if used_feat.contains(&f.id) || distance_m(p.home, f.point) < p.min_distance_m {
                    continue;
                }
                if p.surface == SurfacePref::PavedOnly && k.geom == Geom::Line && crate::scan::is_rough(&f.tags) {
                    continue;
                }
                if used_pts.iter().any(|u| distance_m(*u, f.point) < SPACING_M) {
                    continue;
                }
                if let Some((target, effort)) = feature_target(k, f, z.mode, p.home, want) {
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
        } else if let Some((target, effort, place)) = free_candidate(k, z, &pool, p, want, rng, used_pts) {
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
            if let Some((target, effort, place)) = free_candidate(k, z, &pool, p, want, rng, used_pts) {
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
    let anchor = match &c.target {
        Target::Point { p, .. } | Target::Dwell { p, .. } => Some(*p),
        Target::DwellArea { center, .. } => Some(*center),
        Target::Line { pts, .. } => pts.first().copied(),
        Target::Courier { a, .. } => Some(*a),
        Target::RoundTrip { far, .. } => Some(*far),
        _ => None,
    };
    if let Some(a) = anchor {
        used_pts.push(a);
    }
    let (kind_id, quest_name) =
        if s.boss { ("the_big_one".to_string(), format!("The Big One: {}", c.kind.name)) } else { (c.kind.id.clone(), c.kind.name.clone()) };
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
    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by_key(|&i| slots[i].boss);
    let mut done: Vec<(usize, Assignment)> = Vec::new();
    for i in order {
        let s = &slots[i];
        if let Some(z) = zones.iter().find(|z| z.zone == s.zone) {
            done.push((i, one(s, z, catalog, p, &mut rng, &mut used_feat, &mut used_pts)));
        }
    }
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, a)| a).collect()
}

#[cfg(test)]
#[allow(clippy::cast_sign_loss, clippy::many_single_char_names)] // test code: short names for points and coordinates in test fixtures; test fixtures use small, known-positive numbers
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
        AssignParams { home: home(), minutes_per_tier: 10.0, min_distance_m: 150.0, seed, surface: SurfacePref::Any, avoid_stairs: false }
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
        let bench = |id: &str, bearing: f64| feature(id, &[("amenity", "bench")], destination(home(), bearing, 1500.0), vec![]);
        let streets: Vec<Point> = atlas(&cat, false).streets;
        let mut a = crate::scan::build_atlas("r", 0, vec![bench("n1", 0.0), bench("n2", 180.0)], streets, &cat);
        let mut marks = Marks::default();
        marks.set("n2", Mark::Favorite);
        a.apply_marks(&marks);
        for seed in 1..=20 {
            let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
            let out = assign(&[slot(1, "dwell", 3, Mode::Walk)], &z, &cat, &params(seed));
            let Target::Dwell { p, .. } = &out[0].target else { panic!("expected a bench dwell, got {:?}", out[0].target) };
            assert!(distance_m(*p, destination(home(), 180.0, 1500.0)) < 5.0, "seed {seed} did not pick the favorite");
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
            let a = crate::scan::build_atlas("r", 0, vec![trail("Long Ridge", len)], atlas(&cat, false).streets, &cat);
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
        // A park can be spent time in or walked around (Perimeter Patrol, whose share now fits the effort asked for).
        assert!(matches!(out[0].target, Target::DwellArea { .. } | Target::Line { .. }), "{:?}", out[0].target);
        assert!(matches!(out[1].target, Target::Line { .. }), "{:?}", out[1].target);
        assert!(matches!(out[2].target, Target::Courier { .. } | Target::RoundTrip { .. }));
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
        assert_eq!(out.len(), 8, "an empty atlas must still yield playable (lattice) quests");
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
            (Target::Line { pts: vec![p, destination(p, 0.0, 1000.0)], corridor_m: 25.0, coverage: 0.9 }, "Cover 90% of this 1.0 km path"),
            (Target::Courier { a: p, b: p, r: 40.0, time_limit_min: 12.0 }, "Pick up at A, deliver to B within 12 min"),
            (Target::RoundTrip { far: p, r: 50.0 }, "Reach the far point, then come back home"),
            (Target::Cells { n: 12, cell_m: 100.0 }, "Visit 12 new map cells"),
            (Target::Steps { n: 500 }, "Take 500 steps"),
            (Target::Away { min_distance_m: 1500.0, minutes: 20.0 }, "Spend 20 min at least 1.5 km from home"),
        ];
        for (t, want) in cases {
            assert_eq!(t.goal_text(), want);
        }
    }
}
