//! Overpass query building, fetching (with fallback + backoff) and response parsing.

use std::path::Path;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use crate::geo::Point;

pub const ENDPOINTS: [&str; 3] =
    ["https://overpass-api.de/api/interpreter", "https://overpass.private.coffee/api/interpreter", "https://maps.mail.ru/osm/tools/overpass/api/interpreter"];
const USER_AGENT: &str = "archipela-go2-spike/0.0";
const TILE_DEG: f64 = 0.05;
const TILE_SLACK_M: u32 = 4_000;
const CACHE_MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

#[derive(Debug)]
pub enum Error {
    Parse(String),
    AllEndpointsFailed(Vec<String>),
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(m) => write!(f, "bad overpass response: {m}"),
            Error::AllEndpointsFailed(v) => write!(f, "all endpoints failed: {}", v.join("; ")),
            Error::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub point: Point,
    pub name: String,
    pub score: u32,
    /// Unpaved, unknown-surface trail, or stairs (see scan::is_rough).
    pub rough: bool,
}

/// One bulk query for named points of interest around a center.
pub fn poi_query(center: Point, radius_m: u32) -> String {
    let (lat, lon) = (center.lat, center.lon);
    let a = format!("around:{radius_m},{lat},{lon}");
    format!(
        "[out:json][timeout:60];\n(\n\
         nwr({a})[\"name\"][\"tourism\"~\"^(attraction|viewpoint|museum|artwork|gallery|zoo|theme_park|picnic_site)$\"];\n\
         nwr({a})[\"name\"][\"historic\"];\n\
         nwr({a})[\"name\"][\"leisure\"~\"^(park|playground|garden|nature_reserve|stadium)$\"];\n\
         nwr({a})[\"name\"][\"amenity\"~\"^(library|place_of_worship|theatre|marketplace|fountain)$\"];\n\
         nwr({a})[\"name\"][\"natural\"~\"^(peak|waterfall|spring|beach)$\"];\n\
         );\nout center qt;"
    )
}

fn tile_index(p: Point) -> (i64, i64) {
    ((p.lat / TILE_DEG).round() as i64, (p.lon / TILE_DEG).round() as i64)
}

fn tile_center(p: Point) -> Point {
    let (i, j) = tile_index(p);
    Point::new(i as f64 * TILE_DEG, j as f64 * TILE_DEG)
}

pub fn cache_key(home: Point, radius_m: u32) -> String {
    let (i, j) = tile_index(home);
    format!("poi-{i}_{j}-r{radius_m}")
}

fn score(tags: &Value) -> u32 {
    let has = |k: &str| tags.get(k).is_some();
    1 + 2 * u32::from(has("wikidata") || has("wikipedia")) + u32::from(has("historic")) + u32::from(has("tourism"))
}

/// Parse an Overpass JSON body; unnamed or geometry-less elements are skipped.
pub fn parse(body: &str) -> Result<Vec<Candidate>, Error> {
    let v: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let elements = v
        .get("elements")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Parse(v.get("remark").and_then(Value::as_str).unwrap_or("no elements").to_string()))?;
    let mut out = Vec::new();
    for e in elements {
        let (Some(tags), Some(kind), Some(id)) = (e.get("tags"), e.get("type").and_then(Value::as_str), e.get("id").and_then(Value::as_i64)) else {
            continue;
        };
        let Some(name) = tags.get("name").and_then(Value::as_str) else { continue };
        let loc = e.get("center").unwrap_or(e);
        let (Some(lat), Some(lon)) = (loc.get("lat").and_then(Value::as_f64), loc.get("lon").and_then(Value::as_f64)) else {
            continue;
        };
        let prefix = kind.chars().next().unwrap_or('?');
        out.push(Candidate { id: format!("{prefix}{id}"), point: Point::new(lat, lon), name: name.to_string(), score: score(tags), rough: false });
    }
    Ok(out)
}

fn post(agent: &ureq::Agent, url: &str, query: &str) -> Result<String, String> {
    let mut resp = agent.post(url).header("User-Agent", USER_AGENT).send_form([("data", query)]).map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    let body = resp.body_mut().with_config().limit(100 * 1024 * 1024).read_to_string().map_err(|e| e.to_string())?;
    if status != 200 {
        return Err(format!("HTTP {status}"));
    }
    parse(&body).map(|_| body).map_err(|e| e.to_string())
}

/// Try each endpoint (two attempts each, with backoff) until one returns a valid body.
/// Process-wide health of each public endpoint (success +1, failure -3, clamped): healthy servers are tried first.
static HEALTH: [std::sync::atomic::AtomicI64; 3] =
    [std::sync::atomic::AtomicI64::new(0), std::sync::atomic::AtomicI64::new(0), std::sync::atomic::AtomicI64::new(0)];

fn bump(i: usize, delta: i64) {
    use std::sync::atomic::Ordering::Relaxed;
    let cur = HEALTH[i].load(Relaxed);
    HEALTH[i].store((cur + delta).clamp(-12, 6), Relaxed);
}

/// Endpoint indexes best-first; equally healthy ones are rotated by `start` to spread load.
pub fn order_endpoints(health: &[i64], start: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..health.len()).collect();
    idx.rotate_left(start % health.len().max(1));
    idx.sort_by(|a, b| health[*b].cmp(&health[*a]));
    idx
}

/// Try the endpoints best-first (45 s first round, a longer second round), updating their health.
pub fn fetch(endpoints: &[&str], query: &str) -> Result<String, Error> {
    fetch_from(endpoints, query, 0)
}

fn fetch_from(endpoints: &[&str], query: &str, start: usize) -> Result<String, Error> {
    use std::sync::atomic::Ordering::Relaxed;
    let mut failures = Vec::new();
    for (round, secs) in [(0u64, 30u64), (1, 70)] {
        let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(secs))).http_status_as_error(false).build().into();
        let health: Vec<i64> = (0..endpoints.len()).map(|i| ENDPOINTS.iter().position(|e| *e == endpoints[i]).map_or(0, |k| HEALTH[k].load(Relaxed))).collect();
        for i in order_endpoints(&health, start) {
            let url = endpoints[i];
            let slot = ENDPOINTS.iter().position(|e| *e == url);
            match post(&agent, url, query) {
                Ok(body) => {
                    if let Some(k) = slot {
                        bump(k, 1);
                    }
                    return Ok(body);
                }
                Err(e) => {
                    if let Some(k) = slot {
                        bump(k, -3);
                    }
                    failures.push(format!("{url}: {e}"));
                }
            }
        }
        std::thread::sleep(Duration::from_secs(2 + round * 3));
    }
    Err(Error::AllEndpointsFailed(failures))
}

fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// Fetch any query with an on-disk cache keyed by the query text (30-day freshness).
/// `start` rotates which endpoint is tried first so parallel jobs spread across servers.
pub fn fetch_cached_from(query: &str, cache_dir: Option<&Path>, start: usize) -> Result<String, Error> {
    let file = cache_dir.map(|d| d.join(format!("q-{:016x}.json", fnv1a(query))));
    if let Some(f) = &file {
        let fresh =
            std::fs::metadata(f).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age < CACHE_MAX_AGE);
        if fresh {
            return Ok(std::fs::read_to_string(f)?);
        }
    }
    let body = fetch_from(&ENDPOINTS, query, start)?;
    if let Some(f) = &file {
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(f, &body)?;
    }
    Ok(body)
}

pub fn fetch_cached(query: &str, cache_dir: Option<&Path>) -> Result<String, Error> {
    fetch_cached_from(query, cache_dir, 0)
}

/// Candidates around `home`, fetched once per ~5 km tile and cached on disk for 30 days.
pub fn fetch_pois(home: Point, radius_m: u32, cache_dir: Option<&Path>) -> Result<Vec<Candidate>, Error> {
    let file = cache_dir.map(|d| d.join(format!("{}.json", cache_key(home, radius_m))));
    if let Some(f) = &file {
        let fresh =
            std::fs::metadata(f).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age < CACHE_MAX_AGE);
        if fresh {
            return parse(&std::fs::read_to_string(f)?);
        }
    }
    let query = poi_query(tile_center(home), radius_m + TILE_SLACK_M);
    let body = fetch(&ENDPOINTS, &query)?;
    if let Some(f) = &file {
        if let Some(dir) = f.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(f, &body)?;
    }
    parse(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{"elements":[
      {"type":"node","id":1,"lat":45.5,"lon":-122.6,"tags":{"name":"Fountain","amenity":"fountain"}},
      {"type":"way","id":2,"center":{"lat":45.51,"lon":-122.61},"tags":{"name":"Old Mill","historic":"building","wikidata":"Q1"}},
      {"type":"node","id":3,"lat":45.52,"lon":-122.62,"tags":{"amenity":"bench"}},
      {"type":"relation","id":4,"tags":{"name":"No Geometry","tourism":"attraction"}}
    ]}"#;

    #[test]
    fn parses_nodes_and_way_centers_and_skips_unlocatable_or_unnamed() {
        let c = parse(FIXTURE).unwrap();
        let ids: Vec<&str> = c.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(ids, vec!["n1", "w2"]);
        assert_eq!(c[1].point.lat, 45.51);
        assert_eq!(c[1].name, "Old Mill");
    }

    #[test]
    fn wikidata_and_historic_score_higher_than_plain() {
        let c = parse(FIXTURE).unwrap();
        assert!(c[1].score > c[0].score);
    }

    #[test]
    fn malformed_json_is_an_error_not_a_panic() {
        assert!(parse("<html>busy</html>").is_err());
        assert!(parse(r#"{"remark":"runtime error: timeout"}"#).is_err());
    }

    #[test]
    fn query_mentions_center_radius_and_output_format() {
        let q = poi_query(Point::new(45.5, -122.6), 5000);
        assert!(q.contains("around:5000,45.5,-122.6"));
        assert!(q.contains("[out:json]"));
        assert!(q.contains("out center"));
    }

    #[test]
    fn healthy_endpoints_are_tried_first_and_ties_rotate() {
        assert_eq!(order_endpoints(&[0, 5, -3], 0), vec![1, 0, 2]);
        assert_eq!(order_endpoints(&[2, 2, 2], 1), vec![1, 2, 0]);
        assert_eq!(order_endpoints(&[-9, -9, 4], 2), vec![2, 0, 1]);
    }

    #[test]
    fn cache_key_is_stable_within_a_tile_and_differs_across_tiles() {
        let a = cache_key(Point::new(45.5101, -122.6101), 5000);
        let b = cache_key(Point::new(45.5149, -122.6149), 5000);
        let c = cache_key(Point::new(45.60, -122.70), 5000);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
