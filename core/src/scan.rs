//! Scan a realm once: one bulk query per kind of data, then match features to quest kinds (the atlas).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::{Catalog, Cond, Geom, Kind, Mode};
use crate::fill::fetch_streets;
use crate::geo::{centroid, distance_m, Point};
use crate::overpass::{fetch_cached, Error};
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
    /// quest kind id -> indexes into `features`.
    pub matches: BTreeMap<String, Vec<usize>>,
}

impl Atlas {
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

fn selector(c: &Cond) -> String {
    if c.values.iter().any(|v| v == "*") {
        format!("[\"{}\"]", c.key)
    } else if c.values.len() == 1 {
        format!("[\"{}\"=\"{}\"]", c.key, c.values[0])
    } else {
        format!("[\"{}\"~\"^({})$\"]", c.key, c.values.join("|"))
    }
}

fn statements(kinds: &[&Kind], filter: &str, element: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for k in kinds {
        for group in &k.any_of {
            let mut sel: String = group.iter().map(selector).collect();
            if k.require_name {
                sel.push_str("[\"name\"]");
            }
            out.insert(format!("{element}({filter}){sel};"));
        }
    }
    out
}

/// Query A: points and area centers for point/area kinds.
pub fn poi_query(zone: &Zone, catalog: &Catalog) -> String {
    let kinds: Vec<&Kind> = catalog.kinds.iter().filter(|k| matches!(k.geom, Geom::Point | Geom::Area)).collect();
    let body: String = statements(&kinds, &zone.overpass_filter(), "nwr").into_iter().collect::<Vec<_>>().join("\n");
    format!("[out:json][timeout:90];\n(\n{body}\n);\nout center tags qt;")
}

/// Query B: full geometry (ways only) for line kinds and for area kinds (polygon outlines).
pub fn geom_query(zone: &Zone, catalog: &Catalog) -> String {
    let kinds: Vec<&Kind> = catalog.kinds.iter().filter(|k| matches!(k.geom, Geom::Line | Geom::Area)).collect();
    let body: String = statements(&kinds, &zone.overpass_filter(), "way").into_iter().collect::<Vec<_>>().join("\n");
    format!("[out:json][timeout:90];\n(\n{body}\n);\nout geom tags qt;")
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

/// Chain way geometries (same trail, same name) into longer polylines by joining shared endpoints.
pub fn stitch(ways: Vec<Vec<Point>>) -> Vec<Vec<Point>> {
    let mut pool: Vec<Vec<Point>> = ways.into_iter().filter(|w| w.len() >= 2).collect();
    pool.sort_by(|a, b| b.len().cmp(&a.len()));
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
                let mut by_name: BTreeMap<String, Vec<Vec<Point>>> = BTreeMap::new();
                for i in 0..base {
                    let f = &features[i];
                    if f.geometry.len() >= 2 && k.matches(&f.tags) {
                        by_name.entry(f.name.clone().unwrap_or_else(|| f.id.clone())).or_default().push(f.geometry.clone());
                    }
                }
                let mut idxs = Vec::new();
                for (name, ways) in by_name {
                    for (n, chain) in stitch(ways).into_iter().enumerate() {
                        if k.closed && !is_closed(&chain) {
                            continue;
                        }
                        let mut tags = BTreeMap::new();
                        tags.insert("name".to_string(), name.clone());
                        idxs.push(features.len());
                        features.push(Feature { id: format!("L:{}:{}:{}", k.id, name, n), point: centroid(&chain), name: Some(name.clone()), tags, geometry: chain });
                    }
                }
                if !idxs.is_empty() {
                    matches.insert(k.id.clone(), idxs);
                }
            }
            Geom::None => {}
        }
    }
    Atlas { realm_id: realm_id.to_string(), scanned_at_ms: now_ms, features, streets, matches }
}

/// Scan a realm over the network (two bulk queries + streets), all cached on disk.
pub fn scan_realm(realm: &Realm, catalog: &Catalog, cache_dir: Option<&Path>, now_ms: u64) -> Result<Atlas, Error> {
    let zone = realm.shape.to_zone();
    let a = parse_features(&fetch_cached(&poi_query(&zone, catalog), cache_dir)?)?;
    let b = parse_features(&fetch_cached(&geom_query(&zone, catalog), cache_dir)?)?;
    let streets: Vec<Point> = fetch_streets(&zone, 60.0, cache_dir).map(|c| c.into_iter().map(|x| x.point).collect()).unwrap_or_default();
    let stride = (streets.len() / 12_000).max(1);
    let streets = streets.into_iter().step_by(stride).collect();
    Ok(build_atlas(&realm.id, now_ms, merge(a, b), streets, catalog))
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
    fn queries_contain_filters_and_the_zone() {
        let cat = Catalog::builtin();
        let zone = Zone::Circle { center: Point::new(40.0, -111.0), radius_m: 1000.0 };
        let q = poi_query(&zone, &cat);
        assert!(q.contains("around:1000,40,-111") && q.contains("[\"tourism\"=\"artwork\"]") && q.contains("out center"));
        let g = geom_query(&zone, &cat);
        assert!(g.contains("way(around:1000") && g.contains("out geom") && g.contains("highway"));
    }
}
