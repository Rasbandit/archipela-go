//! Ways to fill a zone with candidate points. Streets and cells work almost anywhere.

use serde_json::Value;

use crate::geo::{distance_m, normal_lon, Point};
use crate::num::round_i64;
use crate::overpass::{fetch_cached, Candidate, Error};
use crate::zone::Zone;

const WALKABLE: &str = "^(residential|living_street|pedestrian|footway|path|cycleway|track|service|unclassified|tertiary|secondary|steps|bridleway)$";

/// Overpass query for walkable streets matching an area `filter`.
#[must_use]
pub fn streets_query_in(filter: &str) -> String {
    format!("[out:json][timeout:40];\nway({filter})[\"highway\"~\"{WALKABLE}\"][\"access\"!~\"^(private|no)$\"];\nout geom qt;")
}

/// Overpass query for walkable streets in `zone`.
#[must_use]
pub fn streets_query(zone: &Zone) -> String {
    streets_query_in(&zone.overpass_filter())
}

/// Points every `spacing_m` along each street/path polyline, kept only inside the zone.
///
/// # Errors
/// Returns [`Error::Parse`] if the body is not JSON or has no `elements`.
pub fn parse_streets(body: &str, zone: &Zone, spacing_m: f64) -> Result<Vec<Candidate>, Error> {
    let v: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let elements = v.get("elements").and_then(Value::as_array).ok_or_else(|| Error::Parse("no elements".into()))?;
    let mut out = Vec::new();
    for e in elements {
        let (Some(id), Some(geom)) = (e.get("id").and_then(Value::as_i64), e.get("geometry").and_then(Value::as_array)) else { continue };
        let tags = e.get("tags");
        let tag_map: std::collections::BTreeMap<String, String> =
            tags.and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default();
        let rough = crate::scan::is_rough(&tag_map);
        let label = tags.and_then(|t| t.get("name").or_else(|| t.get("highway"))).and_then(Value::as_str).unwrap_or("street").to_string();
        let named = tags.is_some_and(|t| t.get("name").is_some());
        let pts: Vec<Point> = geom.iter().filter_map(|g| Some(Point::new(g.get("lat")?.as_f64()?, g.get("lon")?.as_f64()?))).collect();
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
///
/// # Errors
/// Returns an error if the request or the cache fails, or the response cannot be parsed.
pub fn fetch_streets(zone: &Zone, spacing_m: f64, cache_dir: Option<&std::path::Path>) -> Result<Vec<Candidate>, Error> {
    let body = fetch_cached(&streets_query(zone), cache_dir)?;
    parse_streets(&body, zone, spacing_m)
}

/// Offline hex-ish lattice of points covering the zone: works with no map data at all.
#[must_use]
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
            let p = Point::new(lat, normal_lon(lon)); // the bbox may run past ±180 across the antimeridian
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

/// One piece of a walkable street: a unique key (so a street returned by two tiles is counted once), its length in metres and whether it is rough going.
pub struct StreetSegment {
    /// Quantised start and end coordinates of the segment, used to drop duplicates.
    pub key: (i64, i64, i64, i64),
    /// Length of the segment in metres.
    pub len_m: f64,
    /// True when the way is tagged with a rough surface.
    pub rough: bool,
}

/// The real length of the walkable streets in a response, inside `zone`. Sidewalks and crossings are left out: they run alongside streets that are
/// counted already, and would double the total. A segment counts when its middle is inside the zone.
///
/// # Errors
/// Returns [`Error::Parse`] if the body is not JSON or has no `elements`.
pub fn street_segments(body: &str, zone: &Zone) -> Result<Vec<StreetSegment>, Error> {
    let v: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let elements = v.get("elements").and_then(Value::as_array).ok_or_else(|| Error::Parse("no elements".into()))?;
    let q = |x: f64| round_i64(x * 1e6);
    let mut out = Vec::new();
    for e in elements {
        let Some(geom) = e.get("geometry").and_then(Value::as_array) else { continue };
        let tag_map: std::collections::BTreeMap<String, String> = e
            .get("tags")
            .and_then(Value::as_object)
            .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
            .unwrap_or_default();
        if is_side(&tag_map) {
            continue;
        }
        let rough = crate::scan::is_rough(&tag_map);
        let pts: Vec<Point> = geom.iter().filter_map(|g| Some(Point::new(g.get("lat")?.as_f64()?, g.get("lon")?.as_f64()?))).collect();
        for w in pts.windows(2) {
            let mid = Point::new(f64::midpoint(w[0].lat, w[1].lat), f64::midpoint(w[0].lon, w[1].lon));
            if !zone.contains(mid) {
                continue;
            }
            let (a, b) = ((q(w[0].lat), q(w[0].lon)), (q(w[1].lat), q(w[1].lon)));
            let (a, b) = if a <= b { (a, b) } else { (b, a) };
            out.push(StreetSegment { key: (a.0, a.1, b.0, b.1), len_m: distance_m(w[0], w[1]), rough });
        }
    }
    Ok(out)
}

/// Whether a way is a sidewalk or a crossing (`footway=sidewalk|crossing` or `highway=crossing`): it runs beside a street.
fn is_side(tags: &std::collections::BTreeMap<String, String>) -> bool {
    tags.get("footway").is_some_and(|f| f == "sidewalk" || f == "crossing") || tags.get("highway").is_some_and(|h| h == "crossing")
}

/// One street or path as the map server returned it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawWay {
    /// OSM way id.
    pub id: i64,
    /// [`crate::scan::way_class`] bits.
    pub class: u8,
    /// Every point of the way.
    pub pts: Vec<Point>,
}

/// Who may use a way: on foot unless `foot` forbids it; by bike except steps and footways or pedestrian streets without `bicycle=yes`, or
/// when `bicycle` forbids it; by car on the street kinds the scan fetches that carry traffic, unless `motor_vehicle`, `motorcar` or
/// `vehicle` forbids it. A mode is forbidden by `no` or `private` (and, for foot and bike, `use_sidepath`). Sidewalks and crossings also
/// get [`crate::scan::way_class::SIDE`].
#[must_use]
pub fn way_class_of(tags: &std::collections::BTreeMap<String, String>) -> u8 {
    use crate::scan::way_class::{BIKE, CAR, FOOT, SIDE};
    let tag = |k: &str| tags.get(k).map_or("", String::as_str);
    let hw = tag("highway");
    let barred = |k: &str| matches!(tag(k), "no" | "private" | "use_sidepath");
    let foot = if barred("foot") { 0 } else { FOOT };
    let bike_ok = matches!(tag("bicycle"), "yes" | "designated" | "permissive");
    let bike = match hw {
        "steps" => 0,
        "footway" | "pedestrian" if !bike_ok => 0,
        _ if barred("bicycle") => 0,
        _ => BIKE,
    };
    let car = if matches!(hw, "residential" | "living_street" | "service" | "unclassified" | "tertiary" | "secondary")
        && !["motor_vehicle", "motorcar", "vehicle"].iter().any(|k| matches!(tag(k), "no" | "private"))
    {
        CAR
    } else {
        0
    };
    let side = if is_side(tags) { SIDE } else { 0 };
    foot | bike | car | side
}

/// Every way of a streets response with its class and full geometry.
///
/// # Errors
/// Returns [`Error::Parse`] if the body is not JSON or has no `elements`.
pub fn raw_ways(body: &str) -> Result<Vec<RawWay>, Error> {
    let v: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let elements = v.get("elements").and_then(Value::as_array).ok_or_else(|| Error::Parse("no elements".into()))?;
    Ok(elements
        .iter()
        .filter_map(|e| {
            let id = e.get("id").and_then(Value::as_i64)?;
            let geom = e.get("geometry").and_then(Value::as_array)?;
            let tags: std::collections::BTreeMap<String, String> = e
                .get("tags")
                .and_then(Value::as_object)
                .map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
                .unwrap_or_default();
            let mut pts: Vec<Point> = geom.iter().filter_map(|g| Some(Point::new(g.get("lat")?.as_f64()?, g.get("lon")?.as_f64()?))).collect();
            pts.dedup();
            (pts.len() >= 2).then(|| RawWay { id, class: way_class_of(&tags), pts })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lattice_across_lon_180_covers_both_sides_with_real_longitudes() {
        let zone = Zone::Circle { center: Point::new(-17.0, 179.99), radius_m: 3000.0 };
        let pts = lattice(&zone, 500.0);
        assert!(pts.len() > 50, "{} points", pts.len());
        assert!(pts.iter().all(|c| c.point.lon.abs() <= 180.0 && zone.contains(c.point)));
        assert!(pts.iter().any(|c| c.point.lon < 0.0) && pts.iter().any(|c| c.point.lon > 0.0));
    }

    #[test]
    fn way_classes_follow_who_may_use_the_way() {
        use crate::scan::way_class::{BIKE, CAR, FOOT};
        let t = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(way_class_of(&t(&[("highway", "residential")])), FOOT | BIKE | CAR);
        assert_eq!(way_class_of(&t(&[("highway", "footway")])), FOOT);
        assert_eq!(way_class_of(&t(&[("highway", "footway"), ("bicycle", "yes")])), FOOT | BIKE);
        assert_eq!(way_class_of(&t(&[("highway", "steps")])), FOOT);
        assert_eq!(way_class_of(&t(&[("highway", "cycleway")])), FOOT | BIKE);
        assert_eq!(way_class_of(&t(&[("highway", "cycleway"), ("foot", "no")])), BIKE);
        assert_eq!(way_class_of(&t(&[("highway", "service"), ("motor_vehicle", "no")])), FOOT | BIKE);
    }

    #[test]
    fn sidewalks_and_crossings_carry_the_side_bit() {
        use crate::scan::way_class::{BIKE, FOOT, SIDE};
        let t = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(way_class_of(&t(&[("highway", "footway"), ("footway", "sidewalk")])), FOOT | SIDE);
        assert_eq!(way_class_of(&t(&[("highway", "footway"), ("footway", "crossing")])), FOOT | SIDE);
        assert_eq!(way_class_of(&t(&[("highway", "crossing")])), FOOT | BIKE | SIDE);
        assert_eq!(way_class_of(&t(&[("highway", "footway")])) & SIDE, 0, "a plain footway is not a sidewalk");
        let body = r#"{"elements":[{"type":"way","id":3,"tags":{"highway":"footway","footway":"sidewalk"},"geometry":[{"lat":40.0,"lon":-111.0},{"lat":40.001,"lon":-111.0}]}]}"#;
        assert_eq!(raw_ways(body).unwrap()[0].class, FOOT | SIDE);
    }

    #[test]
    fn raw_ways_read_id_class_and_geometry() {
        let body = r#"{"elements":[{"type":"way","id":7,"tags":{"highway":"footway"},"geometry":[{"lat":40.0,"lon":-111.0},{"lat":40.001,"lon":-111.0}]},{"type":"node","id":1}]}"#;
        let w = raw_ways(body).unwrap();
        assert_eq!((w.len(), w[0].id, w[0].class, w[0].pts.len()), (1, 7, crate::scan::way_class::FOOT, 2));
        assert!(raw_ways("nope").is_err());
    }

    #[test]
    fn access_tags_take_away_who_may_not_use_the_way() {
        use crate::scan::way_class::{BIKE, CAR, FOOT};
        let t = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect::<std::collections::BTreeMap<_, _>>();
        for key in ["motor_vehicle", "motorcar", "vehicle"] {
            for v in ["no", "private"] {
                assert_eq!(way_class_of(&t(&[("highway", "residential"), (key, v)])) & CAR, 0, "{key}={v}");
            }
        }
        assert_eq!(way_class_of(&t(&[("highway", "residential"), ("motor_vehicle", "yes")])) & CAR, CAR);
        for v in ["no", "private", "use_sidepath"] {
            assert_eq!(way_class_of(&t(&[("highway", "residential"), ("foot", v)])), BIKE | CAR, "foot={v}");
            assert_eq!(way_class_of(&t(&[("highway", "residential"), ("bicycle", v)])), FOOT | CAR, "bicycle={v}");
        }
    }

    #[test]
    fn raw_ways_drop_repeated_points_and_ways_left_with_one() {
        let body = r#"{"elements":[
          {"type":"way","id":1,"tags":{"highway":"path"},"geometry":[{"lat":40.0,"lon":-111.0},{"lat":40.0,"lon":-111.0},{"lat":40.001,"lon":-111.0},{"lat":40.001,"lon":-111.0}]},
          {"type":"way","id":2,"tags":{"highway":"path"},"geometry":[{"lat":40.0,"lon":-111.0},{"lat":40.0,"lon":-111.0}]}
        ]}"#;
        let w = raw_ways(body).unwrap();
        assert_eq!(w.len(), 1, "a way that is one point repeated is no way");
        assert_eq!(w[0].pts, vec![Point::new(40.0, -111.0), Point::new(40.001, -111.0)]);
    }
}
