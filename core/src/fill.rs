//! Ways to fill a zone with candidate points. Streets and cells work almost anywhere.

use serde_json::Value;

use crate::geo::{distance_m, Point};
use crate::overpass::{fetch_cached, Candidate, Error};
use crate::zone::Zone;

const WALKABLE: &str = "^(residential|living_street|pedestrian|footway|path|cycleway|track|service|unclassified|tertiary|secondary|steps|bridleway)$";

pub fn streets_query_in(filter: &str) -> String {
    format!("[out:json][timeout:40];\nway({filter})[\"highway\"~\"{WALKABLE}\"][\"access\"!~\"^(private|no)$\"];\nout geom qt;")
}

pub fn streets_query(zone: &Zone) -> String {
    streets_query_in(&zone.overpass_filter())
}

/// Points every `spacing_m` along each street/path polyline, kept only inside the zone.
pub fn parse_streets(body: &str, zone: &Zone, spacing_m: f64) -> Result<Vec<Candidate>, Error> {
    let v: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let elements = v.get("elements").and_then(Value::as_array).ok_or_else(|| Error::Parse("no elements".into()))?;
    let mut out = Vec::new();
    for e in elements {
        let (Some(id), Some(geom)) = (e.get("id").and_then(Value::as_i64), e.get("geometry").and_then(Value::as_array)) else { continue };
        let tags = e.get("tags");
        let tag_map: std::collections::BTreeMap<String, String> = tags
            .and_then(Value::as_object)
            .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
            .unwrap_or_default();
        let rough = crate::scan::is_rough(&tag_map);
        let label = tags
            .and_then(|t| t.get("name").or_else(|| t.get("highway")))
            .and_then(Value::as_str)
            .unwrap_or("street")
            .to_string();
        let named = tags.is_some_and(|t| t.get("name").is_some());
        let pts: Vec<Point> = geom
            .iter()
            .filter_map(|g| Some(Point::new(g.get("lat")?.as_f64()?, g.get("lon")?.as_f64()?)))
            .collect();
        let mut carry = 0.0;
        let mut k = 0u32;
        for w in pts.windows(2) {
            let seg = distance_m(w[0], w[1]);
            let mut at = if carry == 0.0 { 0.0 } else { spacing_m - carry };
            while at <= seg {
                let t = if seg > 0.0 { at / seg } else { 0.0 };
                let p = Point::new(w[0].lat + (w[1].lat - w[0].lat) * t, w[0].lon + (w[1].lon - w[0].lon) * t);
                if zone.contains(p) {
                    out.push(Candidate { id: format!("w{id}.{k}"), point: p, name: label.clone(), score: u32::from(named), rough });
                }
                k += 1;
                at += spacing_m;
            }
            carry = (carry + seg) % spacing_m;
        }
    }
    Ok(out)
}

/// Streets and paths in the zone (one bulk request, cached).
pub fn fetch_streets(zone: &Zone, spacing_m: f64, cache_dir: Option<&std::path::Path>) -> Result<Vec<Candidate>, Error> {
    let body = fetch_cached(&streets_query(zone), cache_dir)?;
    parse_streets(&body, zone, spacing_m)
}

/// Offline hex-ish lattice of points covering the zone: works with no map data at all.
pub fn lattice(zone: &Zone, spacing_m: f64) -> Vec<Candidate> {
    let (sw, ne) = zone.bbox();
    let dlat = spacing_m * 0.866 / 111_195.0;
    let dlon = spacing_m / (111_195.0 * zone.home().lat.to_radians().cos().max(0.01));
    let mut out = Vec::new();
    let (mut r, mut lat) = (0u32, sw.lat);
    while lat <= ne.lat {
        let mut lon = sw.lon + if r % 2 == 0 { 0.0 } else { dlon / 2.0 };
        let mut c = 0u32;
        while lon <= ne.lon {
            let p = Point::new(lat, lon);
            if zone.contains(p) {
                out.push(Candidate { id: format!("c{r}_{c}"), point: p, name: format!("Cell {r},{c}"), score: 0, rough: false });
            }
            lon += dlon;
            c += 1;
        }
        lat += dlat;
        r += 1;
    }
    out
}
