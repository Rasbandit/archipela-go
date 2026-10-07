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

#[derive(Debug, Clone)]
pub struct SlotIn {
    pub location_id: i64,
    pub zone: u32,
    pub mode: Mode,
    pub family: String,
    pub tier: u8,
    pub boss: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Target {
    Point { p: Point, r: f64 },
    Dwell { p: Point, r: f64, minutes: f64 },
    DwellArea { poly: Vec<Point>, center: Point, r: f64, minutes: f64 },
    Line { pts: Vec<Point>, corridor_m: f64, coverage: f64 },
    Courier { a: Point, b: Point, r: f64, time_limit_min: f64 },
    RoundTrip { far: Point, r: f64, time_limit_min: f64 },
    Cells { n: u32, cell_m: f64 },
    Steps { n: u32 },
    Away { min_distance_m: f64, minutes: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assignment {
    pub location_id: i64,
    pub zone: u32,
    pub mode: Mode,
    pub family: String,
    pub kind_id: String,
    pub quest_name: String,
    pub blurb: String,
    pub place: String,
    pub tier: u8,
    pub effort_min: f64,
    pub target: Target,
    /// True when the realm could not offer the requested family and a street quest was used instead.
    pub fallback: bool,
    pub boss: bool,
}

pub struct ZoneCtx<'a> {
    pub zone: u32,
    pub mode: Mode,
    pub realm: &'a Realm,
    pub atlas: &'a Atlas,
}

/// How much rough going (unpaved paths, unknown-surface trails, stairs) the player accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SurfacePref {
    #[default]
    Any,
    PreferPaved,
    PavedOnly,
}

impl SurfacePref {
    pub fn parse(s: &str) -> SurfacePref {
        match s {
            "prefer_paved" => SurfacePref::PreferPaved,
            "paved_only" => SurfacePref::PavedOnly,
            _ => SurfacePref::Any,
        }
    }
}

pub struct AssignParams {
    pub home: Point,
    pub minutes_per_tier: f64,
    pub min_distance_m: f64,
    pub seed: u64,
    pub surface: SurfacePref,
    pub avoid_stairs: bool,
}

struct Cand {
    score: f64,
    kind: Kind,
    target: Target,
    effort: f64,
    place: String,
    feature_id: Option<String>,
}

const SPACING_M: f64 = 40.0;

fn street_pool(z: &ZoneCtx, pref: SurfacePref) -> Vec<Point> {
    let (paved, rough) = (&z.atlas.streets, &z.atlas.streets_rough);
    let all = || paved.iter().chain(rough.iter()).copied().collect::<Vec<_>>();
    let pool = match pref {
        SurfacePref::Any => all(),
        SurfacePref::PreferPaved if paved.len() >= 50 => paved.clone(),
        SurfacePref::PreferPaved => all(),
        SurfacePref::PavedOnly => paved.clone(),
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

fn feature_target(k: &Kind, f: &Feature, mode: Mode, home: Point) -> Option<(Target, f64)> {
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
            Some((Target::Line { pts: f.geometry.clone(), corridor_m: *corridor_m, coverage: *coverage }, travel_min(nearest, mode) + len / pace))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn free_candidate(k: &Kind, z: &ZoneCtx, pool: &[Point], p: &AssignParams, want: f64, rng: &mut StdRng, used_pts: &[Point]) -> Option<(Target, f64, String)> {
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
            Some((Target::RoundTrip { far, r: 50.0, time_limit_min: one_way * 2.0 * 1.5 + 5.0 }, one_way * 2.0, "Out and back".into()))
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
    z: &ZoneCtx,
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
                if let Some((target, effort)) = feature_target(k, f, z.mode, p.home) {
                    cands.push(Cand {
                        score: (effort - want).abs(),
                        kind: (*k).clone(),
                        target,
                        effort,
                        place: f.name.clone().unwrap_or_else(|| k.name.clone()),
                        feature_id: Some(f.id.clone()),
                    });
                }
            }
        } else if let Some((target, effort, place)) = free_candidate(k, z, &pool, p, want, rng, used_pts) {
            // Generic quests are the backup: prefer real places when the realm has them.
            cands.push(Cand { score: (effort - want).abs() + if s.boss { 6.0 } else { 3.0 }, kind: (*k).clone(), target, effort, place, feature_id: None });
        }
    }
    let mut fallback = false;
    if cands.is_empty() {
        fallback = true;
        if let Some(k) = catalog.kind("street_smarts") {
            if let Some((target, effort, place)) = free_candidate(k, z, &pool, p, want, rng, used_pts) {
                cands.push(Cand { score: 0.0, kind: k.clone(), target, effort, place, feature_id: None });
            }
        }
    }
    cands.sort_by(|a, b| a.score.total_cmp(&b.score));
    let top = cands.len().min(4);
    // The boss always takes the best fit (the biggest thing on offer); other quests vary among the top few.
    let pick = if top == 0 { None } else { Some(cands.swap_remove(if s.boss { 0 } else { rng.random_range(0..top) })) };
    let c = pick.unwrap_or_else(|| {
        // Absolutely nothing (e.g. an empty pool): a point at home keeps the slot playable.
        let k = catalog.kind("street_smarts").expect("street_smarts exists").clone();
        fallback = true;
        Cand { score: 0.0, kind: k, target: Target::Point { p: p.home, r: 40.0 }, effort: want, place: "Home".into(), feature_id: None }
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
pub fn assign(slots: &[SlotIn], zones: &[ZoneCtx], catalog: &Catalog, p: &AssignParams) -> Vec<Assignment> {
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
mod tests {
    use super::*;
    use crate::effort::tier_for;
    use crate::geo::destination;
    use crate::realm::Shape;
    use std::collections::BTreeMap;

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }

    fn realm(mode: Mode) -> Realm {
        Realm { id: "r".into(), name: "R".into(), mode, shape: Shape::Circle { center: home(), radius_m: 9000.0 }, spare: None, scanned_at_ms: None }
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
        assert!(matches!(out[0].target, Target::DwellArea { .. }), "{:?}", out[0].target);
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
        let a = crate::scan::build_atlas("r", 0, vec![dirt], base.streets.clone(), &cat);
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
            Feature { id: "L:stairmaster:S:0".into(), point: o2, name: None, tags: Default::default(), geometry: vec![o2, destination(o2, 0.0, 200.0)] };
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
