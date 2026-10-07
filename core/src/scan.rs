//! Scan a realm once: one bulk query per kind of data, then match features to quest kinds (the atlas).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::{Catalog, Geom, Kind, Mode};
use crate::geo::{centroid, distance_m, Point};
use crate::marks::Marks;
use crate::overpass::{fetch_cached_from, Error};
use std::time::{Duration, Instant};

/// A scan stops after this long and keeps what it has; finished tiles are cached so a rescan continues.
pub const SCAN_BUDGET: Duration = Duration::from_secs(150);
use crate::realm::Realm;
use crate::zone::Zone;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: String,
    pub point: Point,
    pub name: Option<String>,
    pub tags: BTreeMap<String, String>,
    /// Way geometry (polyline or polygon ring); empty for plain points.
    #[serde(default)]
    pub geometry: Vec<Point>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Atlas {
    pub realm_id: String,
    pub scanned_at_ms: u64,
    pub features: Vec<Feature>,
    pub streets: Vec<Point>,
    /// Street/path points that are rough going (unpaved, unknown-surface trails, stairs).
    #[serde(default)]
    pub streets_rough: Vec<Point>,
    /// quest kind id -> indexes into `features`.
    pub matches: BTreeMap<String, Vec<usize>>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// The player's favorite places, set by [`Atlas::apply_marks`] when a game is prepared; never saved with the scan.
    #[serde(skip)]
    pub favorites: BTreeSet<String>,
}

impl Atlas {
    /// Prepare the atlas for play with the player's marks: banned places drop out of every kind's matches, favorites are remembered.
    pub fn apply_marks(&mut self, marks: &Marks) {
        for idxs in self.matches.values_mut() {
            idxs.retain(|&i| !marks.is_banned(&self.features[i].id));
        }
        self.matches.retain(|_, v| !v.is_empty());
        self.favorites = marks.favorites.clone();
    }

    /// quest kind id -> number of places, only for kinds a realm of `mode` can actually offer.
    pub fn offers(&self, catalog: &Catalog, mode: Mode) -> BTreeMap<String, u32> {
        let mut out = BTreeMap::new();
        for k in &catalog.kinds {
            if !k.allows(mode) {
                continue;
            }
            let n = if k.any_of.is_empty() { 0 } else { self.matches.get(&k.id).map_or(0, |v| v.len() as u32) };
            if k.any_of.is_empty() || n >= k.min_features.max(1) {
                out.insert(k.id.clone(), n);
            }
        }
        out
    }
}

/// One statement per (main tag key, needs-a-name): values are unioned, exact filtering happens locally.
/// (A hundred separate statements time out on dense downtowns; ~20 unions do not.)
fn statements(kinds: &[&Kind], filter: &str, element: &str) -> BTreeSet<String> {
    let mut unions: BTreeMap<(String, bool), BTreeSet<String>> = BTreeMap::new();
    for k in kinds {
        for group in &k.any_of {
            // the most selective condition: the first one that names concrete values
            let Some(c) = group.iter().find(|c| !c.values.iter().any(|v| v == "*")) else { continue };
            unions.entry((c.key.clone(), k.require_name)).or_default().extend(c.values.iter().cloned());
        }
    }
    unions
        .into_iter()
        .map(|((key, named), values)| {
            let vals: Vec<String> = values.into_iter().collect();
            let sel = if vals.len() == 1 { format!("[\"{key}\"=\"{}\"]", vals[0]) } else { format!("[\"{key}\"~\"^({})$\"]", vals.join("|")) };
            format!("{element}({filter}){sel}{};", if named { "[\"name\"]" } else { "" })
        })
        .collect()
}

/// Query A: points and area centers for point/area kinds.
pub fn poi_query_in(filter: &str, catalog: &Catalog) -> String {
    let kinds: Vec<&Kind> = catalog.kinds.iter().filter(|k| matches!(k.geom, Geom::Point | Geom::Area)).collect();
    let body: String = statements(&kinds, filter, "nwr").into_iter().collect::<Vec<_>>().join("\n");
    format!("[out:json][timeout:40];\n(\n{body}\n);\nout center tags qt;")
}

/// Query B: full geometry (ways only) for line kinds and for area kinds (polygon outlines).
pub fn geom_query_in(filter: &str, catalog: &Catalog) -> String {
    let kinds: Vec<&Kind> = catalog.kinds.iter().filter(|k| matches!(k.geom, Geom::Line | Geom::Area)).collect();
    let body: String = statements(&kinds, filter, "way").into_iter().collect::<Vec<_>>().join("\n");
    format!("[out:json][timeout:40];\n(\n{body}\n);\nout geom tags qt;")
}

pub fn poi_query(zone: &Zone, catalog: &Catalog) -> String {
    poi_query_in(&zone.overpass_filter(), catalog)
}

pub fn geom_query(zone: &Zone, catalog: &Catalog) -> String {
    geom_query_in(&zone.overpass_filter(), catalog)
}

/// Overpass bbox filters (`south,west,north,east`) covering the zone: public servers handle many small queries far
/// better than one big one. Tiles are ~1.5 km, grown so there are never more than 16.
pub fn tiles(zone: &Zone) -> Vec<String> {
    let (sw, ne) = zone.bbox();
    let h_m = (ne.lat - sw.lat) * 111_195.0;
    let w_m = (ne.lon - sw.lon) * 111_195.0 * ((sw.lat + ne.lat) / 2.0).to_radians().cos();
    let mut tile_m = 1500.0_f64;
    while ((h_m / tile_m).ceil() * (w_m / tile_m).ceil()) > 16.0 {
        tile_m *= 1.25;
    }
    let (rows, cols) = ((h_m / tile_m).ceil().max(1.0) as usize, (w_m / tile_m).ceil().max(1.0) as usize);
    let (dlat, dlon) = ((ne.lat - sw.lat) / rows as f64, (ne.lon - sw.lon) / cols as f64);
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let (s, w) = (sw.lat + dlat * r as f64, sw.lon + dlon * c as f64);
            out.push(format!("{s},{w},{},{}", s + dlat, w + dlon));
        }
    }
    out
}

fn osm_prefix(t: &str) -> char {
    t.chars().next().unwrap_or('?')
}

/// Parse an Overpass body into features (`out center` and `out geom` shapes).
pub fn parse_features(body: &str) -> Result<Vec<Feature>, Error> {
    let v: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let els = v.get("elements").and_then(Value::as_array).ok_or_else(|| Error::Parse("no elements".into()))?;
    let mut out = Vec::new();
    for e in els {
        let (Some(t), Some(id)) = (e.get("type").and_then(Value::as_str), e.get("id").and_then(Value::as_i64)) else { continue };
        let tags: BTreeMap<String, String> = e
            .get("tags")
            .and_then(Value::as_object)
            .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
            .unwrap_or_default();
        let geometry: Vec<Point> = e
            .get("geometry")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|g| Some(Point::new(g.get("lat")?.as_f64()?, g.get("lon")?.as_f64()?))).collect())
            .unwrap_or_default();
        let center = e.get("center").unwrap_or(e);
        let point = match (center.get("lat").and_then(Value::as_f64), center.get("lon").and_then(Value::as_f64)) {
            (Some(la), Some(lo)) => Point::new(la, lo),
            _ if !geometry.is_empty() => centroid(&geometry),
            _ => continue,
        };
        out.push(Feature { id: format!("{}{}", osm_prefix(t), id), name: tags.get("name").cloned(), point, tags, geometry });
    }
    Ok(out)
}

/// Merge feature lists by id (later lists add geometry to earlier ones).
fn merge(a: Vec<Feature>, b: Vec<Feature>) -> Vec<Feature> {
    let mut by: BTreeMap<String, Feature> = BTreeMap::new();
    for f in a.into_iter().chain(b) {
        by.entry(f.id.clone())
            .and_modify(|e| {
                if e.geometry.is_empty() {
                    e.geometry = f.geometry.clone();
                }
            })
            .or_insert(f);
    }
    by.into_values().collect()
}

const JOIN_M: f64 = 8.0;

const UNPAVED: [&str; 12] = ["dirt", "ground", "unpaved", "grass", "sand", "gravel", "mud", "earth", "fine_gravel", "pebblestone", "woodchips", "compacted"];

/// Is this way rough going: stairs, an unpaved surface, or an unmarked trail/track (surface unknown)?
pub fn is_rough(tags: &BTreeMap<String, String>) -> bool {
    if tags.get("rough").is_some_and(|v| v == "yes") {
        return true; // set on stitched trails when any of their ways is rough
    }
    let highway = tags.get("highway").map(String::as_str);
    if highway == Some("steps") {
        return true;
    }
    match tags.get("surface") {
        Some(sf) => sf.split(';').any(|v| UNPAVED.contains(&v.trim())),
        None => matches!(highway, Some("path" | "track" | "bridleway")),
    }
}

/// Chain way geometries (same trail, same name) into longer polylines by joining shared endpoints.
pub fn stitch(ways: Vec<Vec<Point>>) -> Vec<Vec<Point>> {
    let mut pool: Vec<Vec<Point>> = ways.into_iter().filter(|w| w.len() >= 2).collect();
    pool.sort_by_key(|w| std::cmp::Reverse(w.len()));
    let mut chains = Vec::new();
    while let Some(mut chain) = (!pool.is_empty()).then(|| pool.remove(0)) {
        loop {
            let (head, tail) = (chain[0], *chain.last().unwrap());
            let pos = pool.iter().position(|w| {
                let (a, b) = (w[0], *w.last().unwrap());
                [a, b].iter().any(|e| distance_m(*e, tail) < JOIN_M || distance_m(*e, head) < JOIN_M)
            });
            let Some(i) = pos else { break };
            let mut w = pool.remove(i);
            if distance_m(w[0], tail) < JOIN_M {
                chain.extend(w.into_iter().skip(1));
            } else if distance_m(*w.last().unwrap(), tail) < JOIN_M {
                w.reverse();
                chain.extend(w.into_iter().skip(1));
            } else if distance_m(*w.last().unwrap(), head) < JOIN_M {
                w.extend(chain.into_iter().skip(1));
                chain = w;
            } else {
                w.reverse();
                w.extend(chain.into_iter().skip(1));
                chain = w;
            }
        }
        chains.push(chain);
    }
    chains
}

pub fn is_closed(pts: &[Point]) -> bool {
    pts.len() >= 4 && distance_m(pts[0], *pts.last().unwrap()) < 15.0
}

/// Match features to kinds; line kinds get same-name ways stitched into synthetic line features.
pub fn build_atlas(realm_id: &str, now_ms: u64, mut features: Vec<Feature>, streets: Vec<Point>, catalog: &Catalog) -> Atlas {
    let mut matches: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let base = features.len();
    for k in &catalog.kinds {
        if k.any_of.is_empty() {
            continue;
        }
        match k.geom {
            Geom::Point | Geom::Area => {
                let hits: Vec<usize> = (0..base).filter(|&i| k.matches(&features[i].tags)).collect();
                if !hits.is_empty() {
                    matches.insert(k.id.clone(), hits);
                }
            }
            Geom::Line => {
                let mut by_name: BTreeMap<String, (Vec<Vec<Point>>, bool)> = BTreeMap::new();
                for f in &features[..base] {
                    if f.geometry.len() >= 2 && k.matches(&f.tags) {
                        let e = by_name.entry(f.name.clone().unwrap_or_else(|| f.id.clone())).or_default();
                        e.0.push(f.geometry.clone());
                        e.1 |= is_rough(&f.tags);
                    }
                }
                let mut idxs = Vec::new();
                for (name, (ways, rough_any)) in by_name {
                    for (n, chain) in stitch(ways).into_iter().enumerate() {
                        if k.closed && !is_closed(&chain) {
                            continue;
                        }
                        let mut tags = BTreeMap::new();
                        tags.insert("name".to_string(), name.clone());
                        if rough_any {
                            tags.insert("rough".to_string(), "yes".to_string());
                        }
                        idxs.push(features.len());
                        features.push(Feature {
                            id: format!("L:{}:{}:{}", k.id, name, n),
                            point: centroid(&chain),
                            name: Some(name.clone()),
                            tags,
                            geometry: chain,
                        });
                    }
                }
                if !idxs.is_empty() {
                    matches.insert(k.id.clone(), idxs);
                }
            }
            Geom::None => {}
        }
    }
    Atlas {
        realm_id: realm_id.to_string(),
        scanned_at_ms: now_ms,
        features,
        streets,
        streets_rough: vec![],
        matches,
        warnings: vec![],
        favorites: BTreeSet::new(),
    }
}

/// Tiles are bounding boxes, so fetched features spill outside circles and polygons: keep only what touches the zone.
fn retain_in_zone(features: Vec<Feature>, zone: &Zone) -> Vec<Feature> {
    features.into_iter().filter(|f| zone.contains(f.point) || f.geometry.iter().any(|&p| zone.contains(p))).collect()
}

#[derive(Clone, Copy)]
enum Job {
    Poi,
    Geom,
    Streets,
}

/// Scan a realm over the network: small tiles, three at a time, spread over the public servers, each cached.
/// Tiles that still fail are skipped (the atlas is partial) unless everything fails.
pub fn scan_realm(realm: &Realm, catalog: &Catalog, cache_dir: Option<&Path>, now_ms: u64) -> Result<Atlas, Error> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let zone = realm.shape.to_zone();
    let tiles = tiles(&zone);
    // Places first, then streets (the generic quests), then trail/park geometry: the most valuable data lands before the budget runs out.
    let mut jobs: Vec<(Job, String)> = Vec::new();
    for t in &tiles {
        jobs.push((Job::Poi, poi_query_in(t, catalog)));
    }
    for t in &tiles {
        jobs.push((Job::Streets, crate::fill::streets_query_in(t)));
    }
    for t in &tiles {
        jobs.push((Job::Geom, geom_query_in(t, catalog)));
    }
    let deadline = Instant::now() + SCAN_BUDGET;
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(Job, Result<String, Error>)>> = Mutex::new(Vec::new());
    std::thread::scope(|s| {
        for _ in 0..3 {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some((job, q)) = jobs.get(i) else { break };
                let r = fetch_cached_from(q, cache_dir, i, Some(deadline));
                results.lock().unwrap_or_else(|e| e.into_inner()).push((*job, r));
            });
        }
    });
    let (mut a, mut b, mut streets, mut rough) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut failed, total) = (0usize, jobs.len());
    let mut last_err = None;
    for (job, r) in results.into_inner().unwrap_or_else(|e| e.into_inner()) {
        match r {
            Ok(body) => match job {
                Job::Poi => a.extend(parse_features(&body).unwrap_or_default()),
                Job::Geom => b.extend(parse_features(&body).unwrap_or_default()),
                Job::Streets => {
                    for c in crate::fill::parse_streets(&body, &zone, 60.0).unwrap_or_default() {
                        if c.rough {
                            rough.push(c.point)
                        } else {
                            streets.push(c.point)
                        }
                    }
                }
            },
            Err(e) => {
                failed += 1;
                last_err = Some(e);
            }
        }
    }
    if failed == total {
        return Err(last_err.unwrap_or(Error::Parse("nothing could be fetched".into())));
    }
    let stride = (streets.len() / 12_000).max(1);
    let streets = streets.into_iter().step_by(stride).collect();
    let rstride = (rough.len() / 4_000).max(1);
    let rough: Vec<Point> = rough.into_iter().step_by(rstride).collect();
    let mut atlas = build_atlas(&realm.id, now_ms, retain_in_zone(merge(a, b), &zone), streets, catalog);
    atlas.streets_rough = rough;
    atlas.warnings = if failed > 0 {
        vec![format!("{failed} of {total} map requests did not finish in time; the scan is partial. Tap Rescan to continue (finished parts are cached).")]
    } else {
        vec![]
    };
    Ok(atlas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    const A: &str = r#"{"elements":[
      {"type":"node","id":1,"lat":40.0,"lon":-111.0,"tags":{"tourism":"artwork","artwork_type":"mural","name":"Big Mural"}},
      {"type":"node","id":2,"lat":40.001,"lon":-111.0,"tags":{"amenity":"bench"}},
      {"type":"way","id":3,"center":{"lat":40.002,"lon":-111.0},"tags":{"leisure":"park","name":"City Park"}},
      {"type":"node","id":4,"tags":{"amenity":"bench"}}
    ]}"#;

    #[test]
    fn features_outside_the_zone_are_dropped_but_trails_touching_it_stay() {
        let zone = Zone::Circle { center: Point::new(40.0, -111.0), radius_m: 500.0 };
        let feat = |id: &str, p: Point, geometry: Vec<Point>| Feature { id: id.into(), point: p, name: None, tags: BTreeMap::new(), geometry };
        let inside = Point::new(40.001, -111.0);
        let outside = destination(Point::new(40.0, -111.0), 90.0, 900.0);
        let kept = retain_in_zone(vec![feat("in", inside, vec![]), feat("out", outside, vec![]), feat("trail", outside, vec![outside, inside])], &zone);
        let ids: Vec<&str> = kept.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["in", "trail"]);
    }

    #[test]
    fn parses_nodes_centers_and_skips_unlocatable() {
        let f = parse_features(A).unwrap();
        assert_eq!(f.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["n1", "n2", "w3"]);
        assert_eq!(f[0].name.as_deref(), Some("Big Mural"));
        assert!(parse_features("<html>busy</html>").is_err());
    }

    #[test]
    fn parses_geometry_ways_and_merges_polygon_into_center_feature() {
        let g = r#"{"elements":[{"type":"way","id":3,"tags":{"leisure":"park","name":"City Park"},
          "geometry":[{"lat":40.0,"lon":-111.0},{"lat":40.0,"lon":-110.99},{"lat":40.01,"lon":-110.99},{"lat":40.0,"lon":-111.0}]}]}"#;
        let merged = merge(parse_features(A).unwrap(), parse_features(g).unwrap());
        let park = merged.iter().find(|f| f.id == "w3").unwrap();
        assert_eq!(park.geometry.len(), 4);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn atlas_matches_kinds_and_counts_offers_by_mode() {
        let cat = Catalog::builtin();
        let atlas = build_atlas("r", 0, parse_features(A).unwrap(), vec![], &cat);
        assert_eq!(atlas.matches["mural_mural"].len(), 1);
        assert_eq!(atlas.matches["gallery_walls"].len(), 1);
        assert_eq!(atlas.matches["bench_warmer"].len(), 1);
        assert_eq!(atlas.matches["touch_grass"].len(), 1);
        let walk = atlas.offers(&cat, Mode::Walk);
        assert_eq!(walk["touch_grass"], 1);
        assert!(walk.contains_key("street_smarts"), "geometry-free kinds are always on offer");
        let drive = atlas.offers(&cat, Mode::Drive);
        assert!(!drive.contains_key("touch_grass"), "park quests are walk/run only");
        assert!(!walk.contains_key("hydrant_hunter"), "kinds with no matches are not offered");
    }

    #[test]
    fn stitching_joins_split_trail_ways_in_any_orientation() {
        let o = Point::new(40.0, -111.0);
        let p1 = destination(o, 0.0, 300.0);
        let p2 = destination(o, 0.0, 600.0);
        let p3 = destination(o, 0.0, 900.0);
        let chains = stitch(vec![vec![o, p1], vec![p2, p1], vec![p2, p3], vec![destination(o, 90.0, 5000.0), destination(o, 90.0, 5100.0)]]);
        assert_eq!(chains.len(), 2);
        let long = chains.iter().max_by_key(|c| c.len()).unwrap();
        assert_eq!(long.len(), 4);
        assert!(distance_m(long[0], o) < 1.0 || distance_m(*long.last().unwrap(), o) < 1.0);
    }

    #[test]
    fn line_kinds_become_named_stitched_features_and_closed_loops_are_detected() {
        let cat = Catalog::builtin();
        let o = Point::new(40.0, -111.0);
        let (a, b, c) = (destination(o, 0.0, 400.0), destination(o, 60.0, 400.0), destination(o, 120.0, 400.0));
        let way = |pts: Vec<Point>, id: i64| Feature {
            id: format!("w{id}"),
            point: pts[0],
            name: Some("Loop Trail".into()),
            tags: [("highway", "path"), ("name", "Loop Trail")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            geometry: pts,
        };
        let atlas = build_atlas("r", 0, vec![way(vec![a, b, c], 1), way(vec![c, o, a], 2)], vec![], &cat);
        assert_eq!(atlas.matches["trail_boss"].len(), 1);
        assert_eq!(atlas.matches["full_circle"].len(), 1, "stitched ring is closed");
        assert!(is_closed(&atlas.features[atlas.matches["full_circle"][0]].geometry));
    }

    #[test]
    fn rough_going_is_detected_from_surface_and_highway() {
        let t = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> { pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() };
        assert!(is_rough(&t(&[("highway", "steps")])));
        assert!(is_rough(&t(&[("highway", "footway"), ("surface", "dirt")])));
        assert!(is_rough(&t(&[("highway", "path")])), "an unmarked path is treated as rough");
        assert!(!is_rough(&t(&[("highway", "path"), ("surface", "asphalt")])));
        assert!(!is_rough(&t(&[("highway", "footway")])), "a bare footway is usually a sidewalk");
        assert!(!is_rough(&t(&[("highway", "residential"), ("surface", "concrete")])));
        assert!(is_rough(&t(&[("highway", "track"), ("surface", "paved;gravel")])));
    }

    #[test]
    fn tiles_cover_the_zone_in_few_small_queries() {
        let small = Zone::Circle { center: Point::new(40.0, -111.0), radius_m: 1200.0 };
        assert!((1..=4).contains(&tiles(&small).len()));
        let big = Zone::Circle { center: Point::new(40.0, -111.0), radius_m: 40_000.0 };
        assert!(tiles(&big).len() <= 16, "a huge drive realm must not explode into hundreds of queries");
        assert!(tiles(&small)[0].split(',').count() == 4);
    }

    #[test]
    fn queries_contain_filters_and_the_zone() {
        let cat = Catalog::builtin();
        let zone = Zone::Circle { center: Point::new(40.0, -111.0), radius_m: 1000.0 };
        let q = poi_query(&zone, &cat);
        assert!(q.contains("around:1000,40,-111") && q.contains("\"tourism\"") && q.contains("artwork") && q.contains("out center"));
        assert!(q.matches("nwr(").count() < 40, "queries must stay small: {} statements", q.matches("nwr(").count());
        let g = geom_query(&zone, &cat);
        assert!(g.contains("way(around:1000") && g.contains("out geom") && g.contains("highway"));
    }
}
