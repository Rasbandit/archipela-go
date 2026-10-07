//! Scan a realm once: one bulk query per kind of data, then match features to quest kinds (the atlas).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::catalog::{Catalog, Geom, Kind, Mode, Verify};
use crate::geo::{centroid, distance_m, Point};
use crate::marks::Marks;
use crate::overpass::{fetch_cached_from, Error};
use std::time::{Duration, Instant};

/// A scan stops after this long and keeps what it has; finished tiles are cached so a rescan continues.
/// Street points are generated this far apart along each street.
pub const STREET_SPACING_M: f64 = 60.0;

pub const SCAN_BUDGET: Duration = Duration::from_secs(240);
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
    /// Every this-many street points were kept (the rest dropped to keep the atlas small); 0 or 1 means all.
    #[serde(default)]
    pub street_stride: u32,
    /// The player's favorite places, set by [`Atlas::apply_marks`] when a game is prepared; never saved with the scan.
    #[serde(skip)]
    pub favorites: BTreeSet<String>,
}

impl Atlas {
    /// Keep only what lies inside `zone`. Scans fetch whole map tiles and the map servers are not exact, and a realm can be resized after its
    /// scan, so every use of an atlas validates locations against the realm's current zone, not the one it was scanned for.
    pub fn restrict_to(&mut self, zone: &Zone) {
        for idxs in self.matches.values_mut() {
            idxs.retain(|&i| zone.contains(self.features[i].point));
        }
        self.matches.retain(|_, v| !v.is_empty());
        self.streets.retain(|&p| zone.contains(p));
        self.streets_rough.retain(|&p| zone.contains(p));
    }

    /// Walkable street length inside the atlas, in metres: the street points are spaced [`STREET_SPACING_M`] apart.
    pub fn walkable_m(&self) -> f64 {
        (self.streets.len() + self.streets_rough.len()) as f64 * f64::from(self.street_stride.max(1)) * STREET_SPACING_M
    }

    /// The share of walkable street that is rough going (unpaved, unknown-surface paths, stairs), 0..1.
    pub fn rough_share(&self) -> f64 {
        let all = self.streets.len() + self.streets_rough.len();
        if all == 0 {
            0.0
        } else {
            self.streets_rough.len() as f64 / all as f64
        }
    }

    /// Prepare the atlas for play with the player's marks: banned places drop out of every kind's matches, favorites are remembered.
    pub fn apply_marks(&mut self, marks: &Marks) {
        for idxs in self.matches.values_mut() {
            idxs.retain(|&i| !marks.is_banned(&self.features[i].id));
        }
        self.matches.retain(|_, v| !v.is_empty());
        self.favorites = marks.favorites.clone();
    }

    /// quest kind id -> number of places, only for kinds a realm allowing any of `modes` can actually offer.
    pub fn offers(&self, catalog: &Catalog, modes: &[Mode]) -> BTreeMap<String, u32> {
        let mut out = BTreeMap::new();
        for k in &catalog.kinds {
            if !modes.iter().any(|&m| k.allows(m)) {
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

fn osm_prefix(t: &str) -> char {
    t.chars().next().unwrap_or('?')
}

/// Parse an Overpass body into features (`out center` and `out geom` shapes).
/// Some ways carry their own OSM id as a `name` ("w999448118"); that is a data slip, not a name.
fn looks_like_osm_id(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('n' | 'w' | 'r')) && chars.clone().count() > 0 && chars.all(|c| c.is_ascii_digit())
}

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
        let name = tags.get("name").filter(|n| !looks_like_osm_id(n)).cloned();
        out.push(Feature { id: format!("{}{}", osm_prefix(t), id), name, point, tags, geometry });
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

/// Largest gap bridged when joining the pieces of one trail or staircase.
const LINK_M: f64 = 30.0;

/// Join chains whose ends are within [`LINK_M`] of each other (a straight connector spans the gap), closest pair first.
pub fn link(mut chains: Vec<Vec<Point>>) -> Vec<Vec<Point>> {
    loop {
        let mut best: Option<(f64, usize, usize, bool, bool)> = None; // distance, i, j, flip i, flip j
        for i in 0..chains.len() {
            for j in i + 1..chains.len() {
                let (a, b) = (&chains[i], &chains[j]);
                // join tail of the (maybe reversed) i to head of the (maybe reversed) j
                for (flip_i, end_i) in [(false, *a.last().unwrap()), (true, a[0])] {
                    for (flip_j, end_j) in [(false, b[0]), (true, *b.last().unwrap())] {
                        let d = distance_m(end_i, end_j);
                        if d < LINK_M && best.is_none_or(|(bd, ..)| d < bd) {
                            best = Some((d, i, j, flip_i, flip_j));
                        }
                    }
                }
            }
        }
        let Some((_, i, j, flip_i, flip_j)) = best else { return chains };
        let mut second = chains.remove(j);
        let mut first = chains.remove(i);
        if flip_i {
            first.reverse();
        }
        if flip_j {
            second.reverse();
        }
        first.extend(second);
        chains.push(first);
    }
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
                // Ways of a named trail go together; unnamed ways (stair flights, path scraps) are grouped by proximity alone.
                let mut by_name: BTreeMap<String, (Vec<Vec<Point>>, bool)> = BTreeMap::new();
                for f in &features[..base] {
                    if f.geometry.len() >= 2 && k.matches(&f.tags) {
                        let e = by_name.entry(f.name.clone().unwrap_or_default()).or_default();
                        e.0.push(f.geometry.clone());
                        e.1 |= is_rough(&f.tags);
                    }
                }
                let (min_len, max_len) = match k.verify {
                    Verify::FollowLine { min_len_m, max_len_m, .. } => (min_len_m, max_len_m),
                    _ => (0.0, f64::MAX),
                };
                let mut idxs = Vec::new();
                for (name, (ways, rough_any)) in by_name {
                    for chain in link(stitch(ways)) {
                        // A piece too short (or too long) for the kind could never become a quest, so it is not a find either.
                        let len = crate::geo::polyline_len_m(&chain);
                        if len < min_len || len > max_len || (k.closed && !is_closed(&chain)) {
                            continue;
                        }
                        let mut tags = BTreeMap::new();
                        if !name.is_empty() {
                            tags.insert("name".to_string(), name.clone());
                        }
                        if rough_any {
                            tags.insert("rough".to_string(), "yes".to_string());
                        }
                        // The id is the kind, the name and where the line starts, so it is the same after a rescan (marks are keyed by it).
                        let start = chain[0].min_by_coords(*chain.last().unwrap());
                        idxs.push(features.len());
                        features.push(Feature {
                            id: format!("L:{}:{}:{:.5}_{:.5}", k.id, if name.is_empty() { "~" } else { &name }, start.lat, start.lon),
                            point: centroid(&chain),
                            name: (!name.is_empty()).then(|| name.clone()),
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
        street_stride: 1,
        favorites: BTreeSet::new(),
    }
}

/// Tiles are bounding boxes, so fetched features spill outside circles and polygons: keep only what touches the zone.
fn retain_in_zone(features: Vec<Feature>, zone: &Zone) -> Vec<Feature> {
    features.into_iter().filter(|f| zone.contains(f.point) || f.geometry.iter().any(|&p| zone.contains(p))).collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Job {
    Poi,
    Geom,
    Streets,
}

/// The requests a scan of `zone` makes: per grid tile, places first, then streets (the generic quests), then trail/park geometry, so the most
/// valuable data lands first. A request's text depends only on its tile, which is what lets realms share the query cache.
pub fn jobs_for(zone: &Zone, catalog: &Catalog) -> Vec<(Job, String)> {
    let tiles: Vec<String> = crate::tilegrid::tiles_for(zone).into_iter().map(|t| t.filter()).collect();
    let mut jobs = Vec::new();
    jobs.extend(tiles.iter().map(|t| (Job::Poi, poi_query_in(t, catalog))));
    jobs.extend(tiles.iter().map(|t| (Job::Streets, crate::fill::streets_query_in(t))));
    jobs.extend(tiles.iter().map(|t| (Job::Geom, geom_query_in(t, catalog))));
    jobs
}

/// What a scan would cost: how many tiles and requests, and how many of those are already in the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanPlan {
    pub tiles: usize,
    pub jobs: usize,
    pub cached: usize,
}

impl ScanPlan {
    pub fn missing(&self) -> usize {
        self.jobs - self.cached
    }
}

pub fn plan(zone: &Zone, catalog: &Catalog, is_cached: &dyn Fn(&str) -> bool) -> ScanPlan {
    let jobs = jobs_for(zone, catalog);
    ScanPlan { tiles: jobs.len() / 3, jobs: jobs.len(), cached: jobs.iter().filter(|(_, q)| is_cached(q)).count() }
}

/// How requests are paced. The public map servers are shared and slow: few requests at once, a pause between them, and quiet retries.
#[derive(Debug, Clone, Copy)]
pub struct Pacing {
    pub workers: usize,
    /// Pause a worker takes after each request that went to the network.
    pub gap: Duration,
    /// How many times a failed request is tried in all.
    pub rounds: usize,
    /// Wait before retry round n is `backoff * n`.
    pub backoff: Duration,
}

impl Default for Pacing {
    fn default() -> Self {
        Pacing { workers: 2, gap: Duration::from_millis(250), rounds: 3, backoff: Duration::from_secs(4) }
    }
}

pub type Fetch<'a> = &'a (dyn Fn(&str, usize, Option<Instant>) -> Result<String, Error> + Sync);

/// Run the jobs, `done` out of `total` reported as they finish. Failed jobs are retried in later rounds (with growing waits) while the deadline allows.
fn run_jobs(
    jobs: &[(Job, String)],
    fetch: Fetch,
    pacing: &Pacing,
    deadline: Instant,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Vec<Option<Result<String, Error>>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    let results: Mutex<Vec<Option<Result<String, Error>>>> = Mutex::new((0..jobs.len()).map(|_| None).collect());
    let done = AtomicUsize::new(0);
    let mut todo: Vec<usize> = (0..jobs.len()).collect();
    for round in 0..pacing.rounds.max(1) {
        if todo.is_empty() || (round > 0 && Instant::now() >= deadline) {
            break;
        }
        if round > 0 {
            std::thread::sleep(pacing.backoff * round as u32);
        }
        let next = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..pacing.workers.max(1) {
                s.spawn(|| loop {
                    let at = next.fetch_add(1, Ordering::SeqCst);
                    let Some(&i) = todo.get(at) else { break };
                    let r = fetch(&jobs[i].1, i, Some(deadline));
                    let ok = r.is_ok();
                    results.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(r);
                    if ok {
                        progress(done.fetch_add(1, Ordering::SeqCst) + 1, jobs.len());
                    }
                    if !pacing.gap.is_zero() {
                        std::thread::sleep(pacing.gap);
                    }
                });
            }
        });
        let results_now = results.lock().unwrap_or_else(|e| e.into_inner());
        todo.retain(|&i| matches!(results_now[i], Some(Err(_))));
    }
    results.into_inner().unwrap_or_else(|e| e.into_inner())
}

/// Scan a realm with an injected fetcher (see [`scan_realm`]). Pieces that never arrive leave the atlas partial with a note, not an error;
/// only a scan where nothing arrived at all fails.
pub fn scan_with(
    realm: &Realm,
    catalog: &Catalog,
    now_ms: u64,
    fetch: Fetch,
    pacing: &Pacing,
    deadline: Instant,
    progress: &(dyn Fn(usize, usize) + Sync),
) -> Result<Atlas, Error> {
    let zone = realm.shape.to_zone();
    let jobs = jobs_for(&zone, catalog);
    let results = run_jobs(&jobs, fetch, pacing, deadline, progress);

    let (mut a, mut b, mut streets, mut rough) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    // A street that crosses a tile edge comes back from every tile it touches: each street point is kept once.
    let mut seen_street_points = std::collections::HashSet::new();
    let (mut failed, total) = (0usize, jobs.len());
    let mut last_err = None;
    for ((job, _), r) in jobs.iter().zip(results) {
        match r {
            Some(Ok(body)) => match job {
                Job::Poi => a.extend(parse_features(&body).unwrap_or_default()),
                Job::Geom => b.extend(parse_features(&body).unwrap_or_default()),
                Job::Streets => {
                    for c in crate::fill::parse_streets(&body, &zone, STREET_SPACING_M).unwrap_or_default() {
                        if !seen_street_points.insert(c.id.clone()) {
                            continue;
                        }
                        if c.rough {
                            rough.push(c.point)
                        } else {
                            streets.push(c.point)
                        }
                    }
                }
            },
            Some(Err(e)) => {
                failed += 1;
                last_err = Some(e);
            }
            None => failed += 1,
        }
    }
    if failed == total {
        return Err(last_err.unwrap_or(Error::Parse("nothing could be fetched".into())));
    }
    // Keep the atlas small: thin the street points, but remember by how much so lengths can still be worked out.
    let stride = ((streets.len() + rough.len()) / 16_000).max(1);
    let streets = streets.into_iter().step_by(stride).collect();
    let rough: Vec<Point> = rough.into_iter().step_by(stride).collect();
    let mut atlas = build_atlas(&realm.id, now_ms, retain_in_zone(merge(a, b), &zone), streets, catalog);
    atlas.streets_rough = rough;
    atlas.street_stride = stride as u32;
    atlas.warnings = if failed > 0 { vec![format!("{failed} of {total} map requests are still pending")] } else { vec![] };
    Ok(atlas)
}

/// Scan a realm over the network: one small fixed tile at a time, two at a time with a pause between, each cached (and shared with every other
/// realm that touches the same tile). `progress(done, total)` is called as requests finish.
pub fn scan_realm(realm: &Realm, catalog: &Catalog, cache_dir: Option<&Path>, now_ms: u64, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<Atlas, Error> {
    let fetch = |q: &str, start: usize, deadline: Option<Instant>| fetch_cached_from(q, cache_dir, start, deadline);
    scan_with(realm, catalog, now_ms, &fetch, &Pacing::default(), Instant::now() + SCAN_BUDGET, progress)
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

    // ---- scanning: plan, retries, pacing, sharing between realms
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Mutex as StdMutex;

    const BENCH_BODY: &str = r#"{"elements":[{"type":"node","id":1,"lat":40.0005,"lon":-111.0005,"tags":{"amenity":"bench"}}]}"#;

    fn quick() -> Pacing {
        Pacing { workers: 2, gap: Duration::ZERO, rounds: 3, backoff: Duration::ZERO }
    }

    fn small_realm(center: Point, r: f64) -> Realm {
        Realm { id: "r".into(), name: "R".into(), icon: None, shape: crate::realm::Shape::Circle { center, radius_m: r }, spare: None, scanned_at_ms: None }
    }

    #[test]
    fn failed_jobs_are_retried_quietly_and_a_late_success_leaves_no_warning() {
        let cat = Catalog::builtin();
        let attempts: StdMutex<BTreeMap<String, usize>> = StdMutex::new(BTreeMap::new());
        let fetch = |q: &str, _: usize, _: Option<Instant>| -> Result<String, Error> {
            let mut m = attempts.lock().unwrap();
            let n = m.entry(q.to_string()).or_insert(0);
            *n += 1;
            if *n < 3 {
                Err(Error::Parse("busy".into()))
            } else {
                Ok(BENCH_BODY.to_string())
            }
        };
        let a =
            scan_with(&small_realm(Point::new(40.0, -111.0), 600.0), &cat, 0, &fetch, &quick(), Instant::now() + Duration::from_secs(30), &|_, _| {}).unwrap();
        assert!(a.warnings.is_empty(), "everything worked on the third try: {:?}", a.warnings);
        assert!(attempts.lock().unwrap().values().all(|&n| n == 3));
        assert!(a.matches.contains_key("bench_warmer"));
    }

    #[test]
    fn a_job_that_keeps_failing_gives_a_partial_atlas_with_a_note_not_an_error() {
        let cat = Catalog::builtin();
        let fetch = |q: &str, _: usize, _: Option<Instant>| -> Result<String, Error> {
            if q.contains("\"highway\"") && q.contains("out geom") {
                Err(Error::Parse("down".into()))
            } else {
                Ok(BENCH_BODY.to_string())
            }
        };
        let a =
            scan_with(&small_realm(Point::new(40.0, -111.0), 600.0), &cat, 0, &fetch, &quick(), Instant::now() + Duration::from_secs(30), &|_, _| {}).unwrap();
        assert!(!a.warnings.is_empty());
        assert!(a.matches.contains_key("bench_warmer"), "what did arrive is kept");
    }

    #[test]
    fn a_street_crossing_two_tiles_is_counted_once_and_the_atlas_remembers_its_total_length() {
        let cat = Catalog::builtin();
        // one way, 1.2 km long, that every tile it touches returns in full
        let o = Point::new(40.0095, -111.0);
        let way = |a: Point, b: Point| {
            format!(
                r#"{{"type":"way","id":77,"tags":{{"highway":"residential"}},"geometry":[{{"lat":{},"lon":{}}},{{"lat":{},"lon":{}}}]}}"#,
                a.lat, a.lon, b.lat, b.lon
            )
        };
        let body = format!(r#"{{"elements":[{}]}}"#, way(o, crate::geo::destination(o, 0.0, 1200.0))); // crosses the 40.01 tile edge
        let fetch = |q: &str, _: usize, _: Option<Instant>| -> Result<String, Error> {
            Ok(if q.contains("\"highway\"~") && q.contains("out geom qt") { body.clone() } else { r#"{"elements":[]}"#.to_string() })
        };
        let a = scan_with(&small_realm(Point::new(40.0105, -111.0), 2000.0), &cat, 0, &fetch, &quick(), Instant::now() + Duration::from_secs(30), &|_, _| {})
            .unwrap();
        let metres = a.walkable_m();
        assert!((1100.0..1300.0).contains(&metres), "one 1.2 km street, not two copies of it: {metres}");
    }

    #[test]
    fn when_every_request_fails_the_scan_fails() {
        let cat = Catalog::builtin();
        let fetch = |_: &str, _: usize, _: Option<Instant>| -> Result<String, Error> { Err(Error::Parse("down".into())) };
        let r = scan_with(&small_realm(Point::new(40.0, -111.0), 600.0), &cat, 0, &fetch, &quick(), Instant::now() + Duration::from_secs(30), &|_, _| {});
        assert!(r.is_err());
    }

    #[test]
    fn a_passed_deadline_stops_retrying() {
        let cat = Catalog::builtin();
        let calls = AtomicUsize::new(0);
        let fetch = |_: &str, _: usize, _: Option<Instant>| -> Result<String, Error> {
            calls.fetch_add(1, AtomicOrdering::SeqCst);
            Err(Error::Parse("down".into()))
        };
        let jobs = jobs_for(&small_realm(Point::new(40.0, -111.0), 600.0).shape.to_zone(), &cat).len();
        let _ = scan_with(&small_realm(Point::new(40.0, -111.0), 600.0), &cat, 0, &fetch, &quick(), Instant::now() - Duration::from_secs(1), &|_, _| {});
        assert!(calls.load(AtomicOrdering::SeqCst) <= jobs, "no retry rounds once the budget is gone");
    }

    #[test]
    fn progress_counts_up_to_the_number_of_jobs() {
        let cat = Catalog::builtin();
        let fetch = |_: &str, _: usize, _: Option<Instant>| -> Result<String, Error> { Ok(BENCH_BODY.to_string()) };
        let last = AtomicUsize::new(0);
        let total = AtomicUsize::new(0);
        let _ = scan_with(&small_realm(Point::new(40.0, -111.0), 600.0), &cat, 0, &fetch, &quick(), Instant::now() + Duration::from_secs(30), &|done, of| {
            last.fetch_max(done, AtomicOrdering::SeqCst);
            total.store(of, AtomicOrdering::SeqCst);
        });
        assert_eq!(last.load(AtomicOrdering::SeqCst), total.load(AtomicOrdering::SeqCst));
        assert!(total.load(AtomicOrdering::SeqCst) >= 3);
    }

    #[test]
    fn the_plan_counts_what_is_cached_so_an_unchanged_area_costs_nothing() {
        let cat = Catalog::builtin();
        let z = small_realm(Point::new(40.0, -111.0), 2500.0).shape.to_zone();
        let all = jobs_for(&z, &cat);
        let everything = plan(&z, &cat, &|_| true);
        assert_eq!((everything.jobs, everything.cached, everything.missing()), (all.len(), all.len(), 0));
        let nothing = plan(&z, &cat, &|_| false);
        assert_eq!(nothing.missing(), all.len());
        assert_eq!(nothing.tiles * 3, nothing.jobs, "three requests per tile");
    }

    #[test]
    fn moving_a_realm_only_needs_the_queries_of_the_tiles_it_newly_touches() {
        let cat = Catalog::builtin();
        let c = Point::new(40.0, -111.0);
        let first: std::collections::BTreeSet<String> = jobs_for(&small_realm(c, 1500.0).shape.to_zone(), &cat).into_iter().map(|(_, q)| q).collect();
        let moved = small_realm(crate::geo::destination(c, 90.0, 1200.0), 1500.0).shape.to_zone();
        let p = plan(&moved, &cat, &|q| first.contains(q));
        assert!(p.cached > 0 && p.missing() > 0 && p.missing() < p.jobs, "only part of the moved area is new: {p:?}");
    }

    #[test]
    fn restricting_an_atlas_to_a_zone_drops_finds_and_streets_outside_it() {
        let cat = Catalog::builtin();
        let center = Point::new(40.0, -111.0);
        let zone = Zone::Circle { center, radius_m: 500.0 };
        let (inside, outside) = (destination(center, 0.0, 200.0), destination(center, 90.0, 900.0));
        let bench = |id: &str, p: Point| Feature {
            id: id.into(),
            point: p,
            name: None,
            tags: [("amenity".to_string(), "bench".to_string())].into_iter().collect(),
            geometry: vec![],
        };
        let mut a = build_atlas("r", 0, vec![bench("n1", inside), bench("n2", outside)], vec![inside, outside], &cat);
        a.streets_rough = vec![outside];
        assert_eq!(a.matches["bench_warmer"].len(), 2);
        a.restrict_to(&zone);
        assert_eq!(a.matches["bench_warmer"], vec![0], "only the bench inside the zone is kept");
        assert_eq!((a.streets, a.streets_rough), (vec![inside], vec![]));
    }

    fn way(id: &str, name: Option<&str>, tags: &[(&str, &str)], pts: Vec<Point>) -> Feature {
        let mut t: BTreeMap<String, String> = tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        if let Some(n) = name {
            t.insert("name".into(), n.into());
        }
        Feature { id: id.into(), point: pts[0], name: name.map(String::from), tags: t, geometry: pts }
    }

    #[test]
    fn fragments_of_one_trail_with_small_gaps_join_into_a_single_find() {
        let cat = Catalog::builtin();
        let o = Point::new(40.0, -111.0);
        let end1 = destination(o, 0.0, 600.0);
        let start2 = destination(o, 0.0, 625.0); // a 25 m gap
        let end2 = destination(o, 0.0, 1300.0);
        let tags = [("highway", "path")];
        let a = build_atlas(
            "r",
            0,
            vec![way("w1", Some("Ridge Trail"), &tags, vec![o, end1]), way("w2", Some("Ridge Trail"), &tags, vec![start2, end2])],
            vec![],
            &cat,
        );
        let hits = &a.matches["trail_boss"];
        assert_eq!(hits.len(), 1, "one trail, not two pieces");
        assert!(crate::geo::polyline_len_m(&a.features[hits[0]].geometry) > 1200.0);
    }

    #[test]
    fn a_trail_shorter_than_its_kind_minimum_is_not_a_find() {
        let cat = Catalog::builtin();
        let o = Point::new(40.0, -111.0);
        let a = build_atlas("r", 0, vec![way("w1", Some("Union Way"), &[("highway", "path")], vec![o, destination(o, 0.0, 12.0)])], vec![], &cat);
        assert!(!a.matches.contains_key("trail_boss"), "12 m is not a trail");
    }

    #[test]
    fn unnamed_stair_flights_close_together_become_one_staircase_and_a_lone_flight_is_dropped() {
        let cat = Catalog::builtin();
        let o = Point::new(40.0, -111.0);
        let flight = |id: &str, from: f64| way(id, None, &[("highway", "steps")], vec![destination(o, 0.0, from), destination(o, 0.0, from + 8.0)]);
        // three flights 8 m long with 12 m between them, and one flight far away
        let far = way("w9", None, &[("highway", "steps")], vec![destination(o, 90.0, 3000.0), destination(o, 90.0, 3008.0)]);
        let a = build_atlas("r", 0, vec![flight("w1", 0.0), flight("w2", 20.0), flight("w3", 40.0), far], vec![], &cat);
        let hits = &a.matches["stairmaster"];
        assert_eq!(hits.len(), 1, "the three flights are one staircase; the lone far flight is too short to be a find");
        assert!(crate::geo::polyline_len_m(&a.features[hits[0]].geometry) >= 20.0);
    }

    #[test]
    fn a_name_that_is_just_an_osm_id_is_not_a_name() {
        let body = r#"{"elements":[{"type":"way","id":5,"center":{"lat":40.0,"lon":-111.0},"tags":{"highway":"steps","name":"w999448118"}},
                                    {"type":"way","id":6,"center":{"lat":40.0,"lon":-111.0},"tags":{"highway":"path","name":"Ridge Trail"}}]}"#;
        let f = parse_features(body).unwrap();
        assert_eq!((f[0].name.clone(), f[1].name.clone()), (None, Some("Ridge Trail".to_string())));
    }

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
        let walk = atlas.offers(&cat, &[Mode::Walk]);
        assert_eq!(walk["touch_grass"], 1);
        assert!(walk.contains_key("street_smarts"), "geometry-free kinds are always on offer");
        let drive = atlas.offers(&cat, &[Mode::Drive]);
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
    fn a_small_realm_is_a_handful_of_tiles_and_a_huge_one_is_visibly_costly() {
        let cat = Catalog::builtin();
        let small = Zone::Circle { center: Point::new(40.0, -111.0), radius_m: 1200.0 };
        assert!((1..=6).contains(&plan(&small, &cat, &|_| false).tiles));
        // Nothing hides the cost of a huge realm: the plan reports it so the app can ask first.
        let big = Zone::Circle { center: Point::new(40.0, -111.0), radius_m: 20_000.0 };
        assert!(plan(&big, &cat, &|_| false).missing() > 100);
        assert_eq!(jobs_for(&small, &cat)[0].1, jobs_for(&small, &cat)[0].1, "the same area always asks the same questions");
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
