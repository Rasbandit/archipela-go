//! What the map knows about a quest's place: its OpenStreetMap tags turned into labelled fields for the details card.

use crate::geo::polyline_len_m;
use crate::scan::Feature;

/// A place's details as the UI shows them; each field is `None` when the map does not have it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PoiDetails {
    /// Name of the place.
    pub name: Option<String>,
    /// What kind of place it is ("Cafe", "Playground", "Trail").
    pub type_label: Option<String>,
    /// Street address ("12 Main St, Springfield").
    pub address: Option<String>,
    /// Opening hours, as OpenStreetMap writes them.
    pub opening_hours: Option<String>,
    /// Phone number, as given.
    pub phone: Option<String>,
    /// Website; only `http://` or `https://` addresses.
    pub website: Option<String>,
    /// Wikipedia article address.
    pub wikipedia_url: Option<String>,
    /// Free-text description.
    pub description: Option<String>,
    /// Trail surface ("Fine gravel").
    pub surface: Option<String>,
    /// Length of a trail, in metres.
    pub length_m: Option<f64>,
    /// Whether a trail is lit: "Yes", "No" or "Partly".
    pub lit: Option<String>,
    /// Hiking difficulty, from `sac_scale`.
    pub difficulty: Option<String>,
    /// The place on openstreetmap.org; `None` for trails stitched from several ways.
    pub osm_url: Option<String>,
}

/// Tag keys whose value says what a place is, most telling first.
const TYPE_KEYS: [&str; 8] = ["amenity", "leisure", "tourism", "shop", "historic", "natural", "man_made", "highway"];

/// Hiking difficulty, easiest first (OpenStreetMap `sac_scale`), with how it reads.
pub(crate) const SAC_SCALE: [(&str, &str); 6] = [
    ("hiking", "Easy hike"),
    ("mountain_hiking", "Mountain hike"),
    ("demanding_mountain_hiking", "Demanding mountain hike"),
    ("alpine_hiking", "Alpine hike"),
    ("demanding_alpine_hiking", "Demanding alpine hike"),
    ("difficult_alpine_hiking", "Difficult alpine hike"),
];

/// The details of `f`.
#[must_use]
pub fn details(f: &Feature) -> PoiDetails {
    let tag = |k: &str| f.tags.get(k).map(|v| v.trim()).filter(|v| !v.is_empty()).map(str::to_string);
    let line_kind = f.id.strip_prefix("L:").and_then(|rest| rest.split(':').next());
    PoiDetails {
        name: f.name.clone(),
        type_label: line_kind.map(|k| line_label(k).to_string()).or_else(|| type_label(f)),
        address: address(f),
        opening_hours: tag("opening_hours"),
        phone: tag("phone").or_else(|| tag("contact:phone")),
        website: tag("website").or_else(|| tag("contact:website")).filter(|w| is_http(w)),
        wikipedia_url: tag("wikipedia").and_then(|w| wikipedia_url(&w)),
        description: tag("description"),
        surface: tag("surface").map(|s| humanize(&s)),
        length_m: (line_kind.is_some() && f.geometry.len() >= 2).then(|| polyline_len_m(&f.geometry)),
        lit: tag("lit").map(|l| match l.as_str() {
            "yes" => "Yes".to_string(),
            "no" => "No".to_string(),
            "partial" => "Partly".to_string(),
            other => humanize(other),
        }),
        difficulty: tag("sac_scale").map(|s| SAC_SCALE.iter().find(|(k, _)| *k == s).map_or_else(|| humanize(&s), |(_, l)| (*l).to_string())),
        osm_url: osm_url(&f.id),
    }
}

/// How a stitched line kind reads as a place type.
fn line_label(kind: &str) -> &'static str {
    match kind {
        "stairmaster" => "Steps",
        "ridge_runner" => "Ridge",
        "pedal_pusher" => "Cycleway",
        _ => "Trail",
    }
}

/// The first type tag with a real value ("yes" says nothing), made readable.
fn type_label(f: &Feature) -> Option<String> {
    TYPE_KEYS.iter().filter_map(|k| f.tags.get(*k)).find(|v| !v.is_empty() && *v != "yes").map(|v| humanize(v))
}

/// "12 Main St, Springfield" from the `addr:*` tags; `None` without a street.
fn address(f: &Feature) -> Option<String> {
    let street = f.tags.get("addr:street")?;
    let line = match f.tags.get("addr:housenumber") {
        Some(n) => format!("{n} {street}"),
        None => street.clone(),
    };
    Some(match f.tags.get("addr:city") {
        Some(c) => format!("{line}, {c}"),
        None => line,
    })
}

fn is_http(url: &str) -> bool {
    let l = url.to_ascii_lowercase();
    l.starts_with("https://") || l.starts_with("http://")
}

/// `en:Foo Bar` -> `https://en.wikipedia.org/wiki/Foo_Bar`; `None` without a language prefix.
fn wikipedia_url(tag: &str) -> Option<String> {
    let (lang, title) = tag.split_once(':')?;
    let title = title.trim();
    let lang_ok = (2..=12).contains(&lang.len()) && lang.chars().all(|c| c.is_ascii_alphabetic() || c == '-');
    (lang_ok && !title.is_empty()).then(|| format!("https://{lang}.wikipedia.org/wiki/{}", title.replace(' ', "_")))
}

/// The openstreetmap.org page of `n123` / `w123` / `r123`; `None` for any other id (stitched trails).
fn osm_url(id: &str) -> Option<String> {
    let mut chars = id.chars();
    let kind = match chars.next()? {
        'n' => "node",
        'w' => "way",
        'r' => "relation",
        _ => return None,
    };
    let num = chars.as_str();
    (!num.is_empty() && num.chars().all(|c| c.is_ascii_digit())).then(|| format!("https://www.openstreetmap.org/{kind}/{num}"))
}

/// `fine_gravel` -> `Fine gravel`.
fn humanize(v: &str) -> String {
    let s = v.replace('_', " ");
    let mut c = s.chars();
    c.next().map_or_else(String::new, |first| first.to_uppercase().chain(c).collect())
}

/// "About this place": a short description, labelled facts in a fixed order (at most six) and a Wikipedia link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    /// Free-text description from the map.
    pub description: Option<String>,
    /// Labelled facts, in the fixed order: what it is, the ground, visiting, who and where.
    pub rows: Vec<(String, String)>,
    /// The Wikipedia article, when the place has one.
    pub wikipedia_url: Option<String>,
}

/// The about sheet of `f`, in the player's units; `None` when the map knows nothing more than its name and type.
#[must_use]
pub fn about(f: &Feature, units: crate::units::UnitSystem) -> Option<About> {
    let tag = |k: &str| f.tags.get(k).map(|v| v.trim()).filter(|v| !v.is_empty());
    let human = |v: &str| humanize(v);
    let type_label = details(f).type_label.map(|t| t.to_lowercase());
    let feet = |m: f64| match units {
        crate::units::UnitSystem::Imperial => format!("{} ft", thousands(crate::num::round_u64(m / 0.3048))),
        crate::units::UnitSystem::Metric => format!("{} m", thousands(crate::num::round_u64(m))),
    };
    let number = |v: &str| v.split_whitespace().next().and_then(|n| n.replace(',', ".").parse::<f64>().ok());
    let kind = ["artwork_type", "memorial", "historic"].iter().filter_map(|k| tag(k)).find(|v| *v != "yes").map(human);
    let entry = [
        (tag("fee") == Some("yes")).then(|| "Fee".to_string()),
        tag("access").filter(|a| ["private", "no", "customers"].contains(a)).map(|a| format!("{} access", humanize(a))),
        (tag("wheelchair") == Some("yes")).then(|| "Wheelchair accessible".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let candidates: [(&str, Option<String>); 16] = [
        ("Kind", kind.filter(|k| type_label.as_deref() != Some(k.to_lowercase().as_str()))),
        ("Artist", tag("artist_name").map(str::to_string)),
        ("Made", tag("start_date").map(|d| d.chars().take(4).collect())),
        ("Faith", tag("denomination").or_else(|| tag("religion")).map(human)),
        ("Food", tag("cuisine").map(human)),
        ("Species", tag("species:en").or_else(|| tag("species")).or_else(|| tag("genus")).map(str::to_string)),
        ("Material", tag("material").map(human)),
        ("Surface", tag("surface").map(human)),
        ("Difficulty", tag("sac_scale").map(|v| SAC_SCALE.iter().find(|(k, _)| *k == v).map_or_else(|| humanize(v), |(_, l)| (*l).to_string()))),
        ("Elevation", tag("ele").and_then(number).map(feet)),
        ("Height", tag("height").and_then(number).map(|m| crate::units::distance(m, units))),
        (
            "Lighting",
            tag("lit")
                .and_then(|v| match v {
                    "yes" => Some("Lit at night"),
                    "no" => Some("Not lit"),
                    "partial" => Some("Partly lit"),
                    _ => None,
                })
                .map(str::to_string),
        ),
        ("Hours", tag("opening_hours").map(str::to_string)),
        ("Entry", (!entry.is_empty()).then_some(entry)),
        ("Run by", tag("operator").map(str::to_string)),
        ("Address", address(f)),
    ];
    let rows: Vec<(String, String)> = candidates.into_iter().filter_map(|(k, v)| v.map(|v| (k.to_string(), v))).take(6).collect();
    let description = tag("description").map(str::to_string);
    let wikipedia_url = tag("wikipedia").and_then(wikipedia_url);
    (!rows.is_empty() || description.is_some() || wikipedia_url.is_some()).then_some(About { description, rows, wikipedia_url })
}

/// "6,234" from 6234.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Whether `f` has an about sheet (the card shows "About this place" only then).
#[must_use]
pub fn has_about(f: &Feature) -> bool {
    about(f, crate::units::UnitSystem::Metric).is_some()
}

/// What a quest keeps about its place: the type label and whether there is an about sheet.
#[must_use]
pub fn place_facts(f: &Feature) -> (Option<String>, bool) {
    (details(f).type_label, has_about(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{destination, Point};
    use std::collections::BTreeMap;

    fn feat(id: &str, tags: &[(&str, &str)], geometry: Vec<Point>) -> Feature {
        let tags: BTreeMap<String, String> = tags.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
        Feature { id: id.into(), point: Point::new(40.0, -111.0), name: tags.get("name").cloned(), tags, geometry }
    }

    #[test]
    fn a_cafe_gets_its_type_address_hours_phone_and_website() {
        let d = details(&feat(
            "n42",
            &[
                ("amenity", "cafe"),
                ("name", "Bean There"),
                ("addr:housenumber", "12"),
                ("addr:street", "Main St"),
                ("addr:city", "Springfield"),
                ("opening_hours", "Mo-Fr 07:00-17:00"),
                ("phone", "+1 555 0100"),
                ("website", "https://bean.example"),
            ],
            vec![],
        ));
        assert_eq!(d.name.as_deref(), Some("Bean There"));
        assert_eq!(d.type_label.as_deref(), Some("Cafe"));
        assert_eq!(d.address.as_deref(), Some("12 Main St, Springfield"));
        assert_eq!(d.opening_hours.as_deref(), Some("Mo-Fr 07:00-17:00"));
        assert_eq!(d.phone.as_deref(), Some("+1 555 0100"));
        assert_eq!(d.website.as_deref(), Some("https://bean.example"));
        assert_eq!(d.osm_url.as_deref(), Some("https://www.openstreetmap.org/node/42"));
        assert_eq!(d.length_m, None, "a point has no length");
    }

    #[test]
    fn no_tags_gives_nothing_but_the_osm_link() {
        let d = details(&feat("w7", &[], vec![]));
        assert_eq!(d, PoiDetails { osm_url: Some("https://www.openstreetmap.org/way/7".into()), ..PoiDetails::default() });
    }

    #[test]
    fn type_label_humanizes_the_first_known_key_and_skips_bare_yes() {
        assert_eq!(details(&feat("n1", &[("amenity", "ice_cream")], vec![])).type_label.as_deref(), Some("Ice cream"));
        assert_eq!(details(&feat("n1", &[("historic", "yes"), ("tourism", "artwork")], vec![])).type_label.as_deref(), Some("Artwork"));
        assert_eq!(details(&feat("n1", &[("historic", "yes")], vec![])).type_label, None);
    }

    #[test]
    fn contact_tags_stand_in_and_an_address_needs_a_street() {
        let d = details(&feat("n1", &[("contact:website", "http://a.example"), ("contact:phone", "555"), ("addr:city", "X")], vec![]));
        assert_eq!((d.website.as_deref(), d.phone.as_deref(), d.address), (Some("http://a.example"), Some("555"), None));
        let d = details(&feat("n1", &[("addr:street", "Elm Rd")], vec![]));
        assert_eq!(d.address.as_deref(), Some("Elm Rd"));
    }

    #[test]
    fn only_http_websites_are_kept() {
        for bad in ["javascript:alert(1)", "ftp://x.example", "www.example.com", "intent://x"] {
            assert_eq!(details(&feat("n1", &[("website", bad)], vec![])).website, None, "{bad}");
        }
        assert!(details(&feat("n1", &[("website", "HTTPS://Caps.example")], vec![])).website.is_some());
    }

    #[test]
    fn wikipedia_needs_a_language_prefix() {
        let w = |v: &str| details(&feat("n1", &[("wikipedia", v)], vec![])).wikipedia_url;
        assert_eq!(w("en:Liberty Park (Salt Lake City)").as_deref(), Some("https://en.wikipedia.org/wiki/Liberty_Park_(Salt_Lake_City)"));
        assert_eq!(w("de:Foo").as_deref(), Some("https://de.wikipedia.org/wiki/Foo"));
        assert_eq!(w("Liberty Park"), None);
        assert_eq!(w("en:"), None);
        assert_eq!(w("e/n:Foo"), None, "a language is letters and dashes only");
    }

    #[test]
    fn synthetic_trails_have_no_osm_link_but_a_length_and_trail_fields() {
        let o = Point::new(40.0, -111.0);
        let line = vec![o, destination(o, 0.0, 500.0), destination(o, 0.0, 1000.0)];
        let d = details(&feat(
            "L:trail_boss:Ridge Trail:40.00000_-111.00000",
            &[("name", "Ridge Trail"), ("surface", "fine_gravel"), ("lit", "partial"), ("sac_scale", "mountain_hiking")],
            line,
        ));
        assert_eq!(d.osm_url, None);
        assert_eq!(d.type_label.as_deref(), Some("Trail"));
        assert!((d.length_m.unwrap() - 1000.0).abs() < 5.0);
        assert_eq!(d.surface.as_deref(), Some("Fine gravel"));
        assert_eq!(d.lit.as_deref(), Some("Partly"));
        assert_eq!(d.difficulty.as_deref(), Some("Mountain hike"));
    }

    #[test]
    fn line_kinds_name_their_type() {
        let o = Point::new(40.0, -111.0);
        let line = || vec![o, destination(o, 0.0, 50.0)];
        let t = |id: &str| details(&feat(id, &[], line())).type_label;
        assert_eq!(t("L:stairmaster:~:1_2").as_deref(), Some("Steps"));
        assert_eq!(t("L:ridge_runner:~:1_2").as_deref(), Some("Ridge"));
        assert_eq!(t("L:pedal_pusher:~:1_2").as_deref(), Some("Cycleway"));
        assert_eq!(t("L:full_circle:~:1_2").as_deref(), Some("Trail"));
    }

    #[test]
    fn lit_and_unknown_sac_scale_read_plainly() {
        let d = |tags: &[(&str, &str)]| details(&feat("w1", tags, vec![]));
        assert_eq!(d(&[("lit", "yes")]).lit.as_deref(), Some("Yes"));
        assert_eq!(d(&[("lit", "no")]).lit.as_deref(), Some("No"));
        assert_eq!(d(&[("sac_scale", "hiking")]).difficulty.as_deref(), Some("Easy hike"));
        assert_eq!(d(&[("sac_scale", "t9_custom")]).difficulty.as_deref(), Some("T9 custom"));
        assert_eq!(d(&[("description", "Shady loop")]).description.as_deref(), Some("Shady loop"));
    }

    #[test]
    fn relation_ids_link_to_relations_and_odd_ids_link_nowhere() {
        assert_eq!(details(&feat("r9", &[], vec![])).osm_url.as_deref(), Some("https://www.openstreetmap.org/relation/9"));
        assert_eq!(details(&feat("x9", &[], vec![])).osm_url, None);
        assert_eq!(details(&feat("n", &[], vec![])).osm_url, None);
        assert_eq!(details(&feat("n1a", &[], vec![])).osm_url, None);
    }

    fn sheet(tags: &[(&str, &str)]) -> Vec<(String, String)> {
        about(&feat("n1", tags, vec![]), crate::units::UnitSystem::Imperial).map(|a| a.rows).unwrap_or_default()
    }
    fn labels(rows: &[(String, String)]) -> Vec<&str> {
        rows.iter().map(|(k, _)| k.as_str()).collect()
    }

    #[test]
    fn about_rows_come_in_a_fixed_order() {
        let rows = sheet(&[
            ("tourism", "artwork"),
            ("artwork_type", "statue"),
            ("artist_name", "J. Seward Johnson"),
            ("start_date", "1983-05-01"),
            ("material", "bronze"),
            ("addr:street", "SW 6th Ave"),
            ("addr:housenumber", "701"),
        ]);
        assert_eq!(
            rows,
            vec![
                ("Kind".to_string(), "Statue".to_string()),
                ("Artist".into(), "J. Seward Johnson".into()),
                ("Made".into(), "1983".into()),
                ("Material".into(), "Bronze".into()),
                ("Address".into(), "701 SW 6th Ave".into()),
            ]
        );
    }

    #[test]
    fn about_rows_are_capped_at_six_and_skip_a_kind_that_repeats_the_type() {
        let rows = sheet(&[
            ("historic", "memorial"),
            ("memorial", "memorial"),
            ("artist_name", "A"),
            ("start_date", "1900"),
            ("religion", "christian"),
            ("cuisine", "pizza"),
            ("species", "Quercus"),
            ("material", "stone"),
            ("surface", "gravel"),
        ]);
        assert_eq!(labels(&rows), ["Artist", "Made", "Faith", "Food", "Species", "Material"]);
    }

    #[test]
    fn entry_joins_fee_access_and_wheelchair_and_heights_use_the_players_units() {
        let rows = sheet(&[("natural", "peak"), ("ele", "1900"), ("fee", "yes"), ("access", "private"), ("wheelchair", "yes"), ("lit", "partial")]);
        assert_eq!(
            rows,
            vec![
                ("Elevation".to_string(), "6,234 ft".to_string()),
                ("Lighting".into(), "Partly lit".into()),
                ("Entry".into(), "Fee · Private access · Wheelchair accessible".into()),
            ]
        );
    }

    #[test]
    fn a_bare_place_has_no_about_sheet_and_a_described_one_does() {
        let bench = feat("n1", &[("amenity", "bench")], vec![]);
        assert_eq!(about(&bench, crate::units::UnitSystem::Imperial), None);
        assert!(!has_about(&bench));
        let wiki = feat("n2", &[("tourism", "artwork"), ("wikipedia", "en:Allow Me")], vec![]);
        let a = about(&wiki, crate::units::UnitSystem::Imperial).expect("a sheet");
        assert_eq!(a.wikipedia_url.as_deref(), Some("https://en.wikipedia.org/wiki/Allow_Me"));
        assert!(has_about(&wiki));
        let described = feat("n3", &[("leisure", "park"), ("description", "Shady and quiet")], vec![]);
        assert_eq!(about(&described, crate::units::UnitSystem::Imperial).and_then(|a| a.description).as_deref(), Some("Shady and quiet"));
    }
}
