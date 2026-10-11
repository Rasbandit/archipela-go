# POI Details Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An ⓘ button in the Play quest popup opens a card with what OpenStreetMap knows about the quest's real place (type,
address, hours, phone, website, Wikipedia, trail surface/length/lit/difficulty).

**Architecture:** Each `Assignment` keeps the atlas `feature_id` it was built from. A new pure core module turns a `Feature`'s
tags into a `PoiDetails` record; the engine looks the feature up lazily (`poi_details(location_id)`) from the realm's saved
atlas when ⓘ is tapped. Stitched trail features get their ways' useful tags merged at scan time. Kotlin only draws rows.

**Tech Stack:** Rust (apgo-core, apgo-ffi, UniFFI), Kotlin/Compose (MapLibre Play screen), JUnit.

**Spec:** `docs/superpowers/specs/2026-10-11-poi-details-design.md`

## Global Constraints

- Game logic and tag interpretation live in the core; Kotlin only draws and opens links (cross-platform hard rule).
- Events over polling: details are computed only when ⓘ is tapped; quest refresh must not read the atlas from disk.
- Old saves (no `feature_id`) must load: `#[serde(default)]`; such quests show no ⓘ.
- Only `http://`/`https://` URLs leave the core as links; phone opens as `tel:`.
- Every colour from palette tokens (`scripts/check_color_tokens.sh`); no inline colours.
- Conventional commits, subject < 50 chars, ending with the session attribution lines.
- Coverage floors only go up (rust 80, kotlin 17).

**Spec amendments (decided while planning, recorded in Task 4/5):**

- `QuestOut.has_details` is `feature_id.is_some()`, not "feature is in the atlas": checking the atlas would read the atlas
  JSON on every quest refresh. A place gone after a rescan is handled when ⓘ is tapped (the card says so).
- Row labels live in `ui/PoiFormat.kt` (with the formatter), not `HelpText.kt`, which holds help topics, not field labels.
- Trail type label comes from the kind in the synthetic id (`L:{kind}:…`), so no `highway` tag is added to synthetic
  features (that tag feeds `is_rough`).

## Review Focus

1. A trail made of many unnamed ways (the `~` group) in a big realm: tag merge must stay linear in vertices, not
   ways × chain points. → Task 2 uses a vertex → way index; a test merges two separate chains of one name.
2. Tag values with odd content (`website=javascript:…`, `wikipedia=Foo` with no language, `historic=yes`): no unsafe link,
   no "Yes" label. → Task 1 tests.
3. Tapping ⓘ after the realm was rescanned and the place is gone: no crash, a clear message. → Task 4 engine test, Task 5
   "gone" state.
4. Loading an older game save without `feature_id`. → Task 3 test.
5. Atlas load on tap is a disk read (can be MBs): must run off the main thread. → Task 5 uses `Dispatchers.IO`.

---

### Task 1: Core `poi_details` module (tags → fields)

**Files:**

- Create: `core/src/poi_details.rs`
- Modify: `core/src/lib.rs` (add `pub mod poi_details;` in alphabetical order, after `pub mod overpass;`)

**Interfaces:**

- Consumes: `crate::scan::Feature { id, point, name, tags: BTreeMap<String,String>, geometry: Vec<Point> }`,
  `crate::geo::polyline_len_m(&[Point]) -> f64`.
- Produces: `pub struct PoiDetails` (fields below) and `pub fn details(f: &Feature) -> PoiDetails`.

- [ ] **Step 1: Write the failing tests** (bottom of the new file; the module body is just the struct + `todo!()` stub for
  now so it compiles)

```rust
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
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd core && cargo test -p apgo-core poi_details`
Expected: FAIL (panics at `todo!()`).

- [ ] **Step 3: Write the implementation** (replace the stub; keep the tests)

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd core && cargo test -p apgo-core poi_details && cargo clippy -p apgo-core --all-targets -- -D warnings`
Expected: all `poi_details` tests PASS, no clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add core/src/poi_details.rs core/src/lib.rs
git commit -m "feat: map tags to POI details in core"
```

---

### Task 2: Merge way tags onto stitched trail features

**Files:**

- Modify: `core/src/scan.rs` (`build_atlas`, the `Geom::Line` arm around lines 507-545; new private fn `merge_trail_tags`)
- Test: `core/src/scan.rs` (`mod tests`, next to `fragments_of_one_trail_with_small_gaps_join_into_a_single_find`)

**Interfaces:**

- Consumes: `crate::poi_details::SAC_SCALE` (order = easiest first), `crate::geo::polyline_len_m`.
- Produces: synthetic `L:` features whose `tags` may also hold `surface`, `lit` (`yes`/`no`/`partial`), `sac_scale`,
  `operator`, `description`, `website`, `wikipedia`. `name` and `rough` unchanged. No `highway` tag.

- [ ] **Step 1: Write the failing tests**

```rust
    #[test]
    fn a_stitched_trail_carries_its_ways_merged_tags() {
        let cat = Catalog::builtin();
        let o = Point::new(40.0, -111.0);
        let mid = destination(o, 0.0, 600.0);
        let end = destination(o, 0.0, 1300.0);
        let a = build_atlas(
            "r",
            0,
            vec![
                way("w1", Some("Ridge Trail"), &[("highway", "path"), ("surface", "gravel"), ("lit", "yes"), ("sac_scale", "hiking"), ("operator", "City")], vec![o, mid]),
                way("w2", Some("Ridge Trail"), &[("highway", "path"), ("surface", "dirt"), ("lit", "no"), ("sac_scale", "mountain_hiking"), ("operator", "County")], vec![mid, end]),
            ],
            vec![],
            &cat,
        );
        let t = &a.features[a.matches["trail_boss"][0]].tags;
        assert_eq!(t.get("surface").map(String::as_str), Some("dirt"), "the surface covering the most length (700 m of 1300)");
        assert_eq!(t.get("lit").map(String::as_str), Some("partial"));
        assert_eq!(t.get("sac_scale").map(String::as_str), Some("mountain_hiking"), "the hardest");
        assert_eq!(t.get("operator").map(String::as_str), Some("City"), "the first found");
        assert_eq!(t.get("name").map(String::as_str), Some("Ridge Trail"));
        assert_eq!(t.get("rough").map(String::as_str), Some("yes"));
        assert!(!t.contains_key("highway"), "synthetic trails get no highway tag (it feeds is_rough)");
    }

    #[test]
    fn untagged_ways_add_nothing_and_all_lit_ways_read_yes() {
        let cat = Catalog::builtin();
        let o = Point::new(40.0, -111.0);
        let mid = destination(o, 0.0, 600.0);
        let end = destination(o, 0.0, 1300.0);
        let a = build_atlas(
            "r",
            0,
            vec![
                way("w1", Some("Lamp Way"), &[("highway", "footway"), ("lit", "yes")], vec![o, mid]),
                way("w2", Some("Lamp Way"), &[("highway", "footway")], vec![mid, end]),
            ],
            vec![],
            &cat,
        );
        let t = &a.features[a.matches["trail_boss"][0]].tags;
        assert_eq!(t.get("lit").map(String::as_str), Some("yes"), "ways with no lit tag do not count");
        for k in ["surface", "sac_scale", "operator", "description", "website", "wikipedia"] {
            assert!(!t.contains_key(k), "{k}");
        }
    }

    #[test]
    fn two_far_apart_trails_of_one_name_keep_their_own_tags() {
        let cat = Catalog::builtin();
        let o = Point::new(40.0, -111.0);
        let far = destination(o, 90.0, 5000.0);
        let a = build_atlas(
            "r",
            0,
            vec![
                way("w1", Some("Creek Trail"), &[("highway", "path"), ("surface", "asphalt")], vec![o, destination(o, 0.0, 900.0)]),
                way("w2", Some("Creek Trail"), &[("highway", "path"), ("surface", "dirt")], vec![far, destination(far, 0.0, 900.0)]),
            ],
            vec![],
            &cat,
        );
        let mut surfaces: Vec<&str> = a.matches["trail_boss"].iter().map(|&i| a.features[i].tags["surface"].as_str()).collect();
        surfaces.sort_unstable();
        assert_eq!(surfaces, ["asphalt", "dirt"]);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd core && cargo test -p apgo-core scan::tests::a_stitched_trail scan::tests::untagged_ways scan::tests::two_far_apart`
Expected: FAIL (`surface` missing).

- [ ] **Step 3: Implement**

In the `Geom::Line` arm, group way *indices* instead of cloned geometry, and merge tags of the ways each chain is made of.
Replace the `by_name` block and the tag building with:

```rust
                // Ways of a named trail go together; unnamed ways (stair flights, path scraps) are grouped by proximity alone.
                let mut by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
                for (i, f) in features[..base].iter().enumerate() {
                    if f.geometry.len() >= 2 && k.matches(&f.tags) {
                        by_name.entry(f.name.clone().unwrap_or_default()).or_default().push(i);
                    }
                }
```

```rust
                for (name, way_idx) in by_name {
                    let rough_any = way_idx.iter().any(|&i| is_rough(&features[i].tags));
                    let ways: Vec<Vec<Point>> = way_idx.iter().map(|&i| features[i].geometry.clone()).collect();
                    // Which ways a vertex belongs to, so each chain finds its own ways in one pass over its points.
                    let mut owners: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
                    for &i in &way_idx {
                        for p in &features[i].geometry {
                            owners.entry(key(*p)).or_default().push(i);
                        }
                    }
                    for chain in link(stitch(ways)) {
                        // (existing length / closed check unchanged)
                        let mut mine: Vec<usize> = chain.iter().filter_map(|p| owners.get(&key(*p))).flatten().copied().collect();
                        mine.sort_unstable();
                        mine.dedup();
                        let mut tags = merge_trail_tags(mine.iter().map(|&i| &features[i]));
                        if !name.is_empty() {
                            tags.insert("name".to_string(), name.clone());
                        }
                        if rough_any {
                            tags.insert("rough".to_string(), "yes".to_string());
                        }
                        // (existing id / push unchanged)
```

Add `use std::collections::HashMap;` at the top if missing, and these private helpers near `is_rough`:

```rust
/// A point as an exact map key (stitching keeps the ways' own vertices, so equal bits mean the same vertex).
fn key(p: Point) -> (u64, u64) {
    (p.lat.to_bits(), p.lon.to_bits())
}

/// The useful tags of a stitched trail from the ways it is made of: the surface covering the most length, lit yes/no/partial,
/// the hardest `sac_scale`, and the first operator, description, website and wikipedia found.
fn merge_trail_tags<'a>(ways: impl Iterator<Item = &'a Feature>) -> BTreeMap<String, String> {
    let ways: Vec<&Feature> = ways.collect();
    let mut out = BTreeMap::new();
    let mut by_surface: BTreeMap<&str, f64> = BTreeMap::new();
    for w in &ways {
        if let Some(s) = w.tags.get("surface") {
            *by_surface.entry(s.as_str()).or_default() += crate::geo::polyline_len_m(&w.geometry);
        }
    }
    if let Some((s, _)) = by_surface.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)) {
        out.insert("surface".to_string(), s.to_string());
    }
    let lit: Vec<&str> = ways.iter().filter_map(|w| w.tags.get("lit")).map(String::as_str).collect();
    if !lit.is_empty() {
        let v = if lit.iter().all(|l| *l == "yes") {
            "yes"
        } else if lit.iter().all(|l| *l == "no") {
            "no"
        } else {
            "partial"
        };
        out.insert("lit".to_string(), v.to_string());
    }
    let rank = |s: &str| crate::poi_details::SAC_SCALE.iter().position(|(k, _)| *k == s);
    if let Some(s) = ways.iter().filter_map(|w| w.tags.get("sac_scale")).filter(|s| rank(s).is_some()).max_by_key(|s| rank(s)) {
        out.insert("sac_scale".to_string(), s.clone());
    }
    for k in ["operator", "description", "website", "wikipedia"] {
        if let Some(v) = ways.iter().find_map(|w| w.tags.get(k)) {
            out.insert(k.to_string(), v.clone());
        }
    }
    out
}
```

Note: `stitch` and `link` only concatenate the ways' vertices (one shared endpoint is skipped per join), so every way in
a chain keeps at least one exact vertex in it.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core --all-targets -- -D warnings`
Expected: all PASS (including the existing stitching, stairs and `tests/stairs.rs` tests).

- [ ] **Step 5: Commit**

```bash
git add core/src/scan.rs
git commit -m "feat: merge way tags onto stitched trails"
```

---

### Task 3: Quests remember their feature

**Files:**

- Modify: `core/src/assign.rs` (`Assignment` struct ~145-171, `one()` ~711)
- Modify: `core/src/game.rs` (`QuestView` ~61, its builder ~597, new `Game::quest_feature`)
- Modify test helpers that build `Assignment`: `core/src/chain.rs:173`, `core/src/fog.rs:95`, `core/src/goal.rs:279`
  (add `feature_id: None`)

**Interfaces:**

- Produces: `Assignment.feature_id: Option<String>`; `QuestView.feature_id: Option<String>`;
  `Game::quest_feature(&self, location_id: i64) -> Option<(&str /*realm id*/, &str /*feature id*/)>`.

- [ ] **Step 1: Write the failing tests**

In `core/src/assign.rs` tests:

```rust
    #[test]
    fn quests_on_real_places_remember_the_feature_and_free_ones_do_not() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, true));
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let mut slots: Vec<SlotIn> = (1..=6).map(|i| slot(i, "dwell", 2 + (i % 4) as u8, Mode::Walk)).collect();
        slots.push(slot(7, "steps", 1, Mode::Walk));
        let out = assign(&slots, &z, &cat, &params(3));
        let bench = out.iter().find(|o| o.kind_id == "bench_warmer").expect("a bench quest");
        assert!(bench.feature_id.as_deref().is_some_and(|id| id.starts_with('n')), "{:?}", bench.feature_id);
        let steps = out.iter().find(|o| o.location_id == 7).unwrap();
        assert_eq!(steps.feature_id, None);
    }

    #[test]
    fn an_assignment_saved_before_feature_ids_loads_without_one() {
        let cat = Catalog::builtin();
        let (r, a) = (realm(Mode::Walk), atlas(&cat, true));
        let z = [ZoneCtx { zone: 1, mode: Mode::Walk, realm: &r, atlas: &a }];
        let out = assign(&[slot(1, "dwell", 3, Mode::Walk)], &z, &cat, &params(3));
        let mut v = serde_json::to_value(&out[0]).unwrap();
        v.as_object_mut().unwrap().remove("feature_id");
        let back: Assignment = serde_json::from_value(v).unwrap();
        assert_eq!(back.feature_id, None);
    }
```

In `core/src/game.rs` tests:

```rust
    #[test]
    fn quest_feature_names_the_zone_realm_and_place() {
        let mut g = game(&reach_only(&[Mode::Walk, Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let (first, other) = (g.assignments[0].location_id, g.assignments[1].location_id);
        g.assignments[0].feature_id = Some("n7".into());
        let zone = g.assignments[0].zone;
        let realm = format!("r{}", g.slot.zones.iter().position(|z| z.id == zone).unwrap());
        assert_eq!(g.quest_feature(first), Some((realm.as_str(), "n7")));
        assert_eq!(g.quest_feature(other), None, "a quest with no place");
        assert_eq!(g.quest_feature(-1), None, "no such quest");
        assert!(g.quest_views(0).iter().any(|q| q.location_id == first && q.feature_id.as_deref() == Some("n7")));
    }
```

(If `g.assignments[1]` happens to have a feature in this all-streets realm, it does not: the test realm has no features.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd core && cargo test -p apgo-core feature`
Expected: compile errors (`no field feature_id`, `no method quest_feature`).

- [ ] **Step 3: Implement**

`Assignment`, after `place`:

```rust
    /// Atlas id of the place the quest uses (`n123`, `w45`, `L:…`), for its details card; `None` for quests with no real place
    /// and for games saved before it was kept.
    #[serde(default)]
    pub feature_id: Option<String>,
```

In `one()`'s `Assignment { … }` literal: `feature_id: c.feature_id,` (move it after the `used_feat.insert` block, which only
borrows it). Add `feature_id: None,` to the three test helpers.

`QuestView`, after `place`:

```rust
    /// Atlas id of the quest's place, if it has a real one (see `Game::quest_feature`).
    pub feature_id: Option<String>,
```

and in `quest_views`: `feature_id: a.feature_id.clone(),`.

`Game` method (next to `quest_views`):

```rust
    /// The realm and atlas feature id behind quest `location_id`, for its place details; `None` for a quest with no real place.
    #[must_use]
    pub fn quest_feature(&self, location_id: i64) -> Option<(&str, &str)> {
        let a = self.assignments.iter().find(|a| a.location_id == location_id)?;
        let feature = a.feature_id.as_deref()?;
        let i = self.slot.zones.iter().position(|z| z.id == a.zone)?;
        Some((self.zone_realms.get(i)?.as_str(), feature))
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core --all-targets -- -D warnings`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/assign.rs core/src/game.rs core/src/chain.rs core/src/fog.rs core/src/goal.rs
git commit -m "feat: keep a quest's atlas feature id"
```

---

### Task 4: Engine `poi_details` and `QuestOut.has_details`

**Files:**

- Modify: `core/ffi/src/engine.rs` (`QuestOut` ~356, `quests()` ~1652, new `PoiDetailsOut` record + `Engine::poi_details`,
  tests module ~2118)
- Modify: `docs/superpowers/specs/2026-10-11-poi-details-design.md` (record the `has_details` amendment)

**Interfaces:**

- Consumes: `apgo_core::poi_details::{details, PoiDetails}`, `Game::quest_feature`, `QuestView.feature_id`,
  `apgo_core::units::distance(m, UnitSystem) -> String`.
- Produces (Kotlin sees, UniFFI camelCase): `QuestOut.hasDetails: Boolean`; `engine.poiDetails(locationId: Long): PoiDetailsOut?`;
  `PoiDetailsOut(name, typeLabel, address, openingHours, phone, website, wikipediaUrl, description, surface, length, lit,
  difficulty, osmUrl)`, all `String?` (`length` is already formatted in the player's units).

- [ ] **Step 1: Write the failing test** (engine tests module; add `use apgo_core::scan::Feature;` and
  `use std::collections::BTreeMap;` there if missing)

```rust
    #[test]
    fn poi_details_reads_the_quest_place_from_the_realm_scan_and_is_none_once_it_is_gone() {
        let e = engine_with_game("poi-details");
        let id = e.quests(0)[0].location_id;
        let has = |e: &TestEngine| e.quests(0).into_iter().find(|q| q.location_id == id).unwrap().has_details;
        assert!(!has(&e), "a street quest has no place");
        assert!(e.poi_details(id).is_none());
        let cafe = Feature {
            id: "n42".into(),
            point: home(),
            name: Some("Bean There".into()),
            tags: BTreeMap::from([("amenity".to_string(), "cafe".to_string()), ("name".to_string(), "Bean There".to_string())]),
            geometry: vec![],
        };
        e.store().save_atlas(&build_atlas("r0", 0, vec![cafe], vec![], &e.catalog)).unwrap();
        e.with_game(|g| g.assignments.iter_mut().find(|a| a.location_id == id).unwrap().feature_id = Some("n42".into()));
        assert!(has(&e));
        let d = e.poi_details(id).expect("the cafe's details");
        assert_eq!((d.name.as_deref(), d.type_label.as_deref()), (Some("Bean There"), Some("Cafe")));
        assert_eq!(d.osm_url.as_deref(), Some("https://www.openstreetmap.org/node/42"));
        e.store().save_atlas(&build_atlas("r0", 0, vec![], vec![], &e.catalog)).unwrap();
        assert!(e.poi_details(id).is_none(), "rescanned without the cafe");
        assert!(e.poi_details(-1).is_none(), "no such quest");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd core && cargo test -p apgo-ffi poi_details`
Expected: compile errors (`has_details`, `poi_details` missing).

- [ ] **Step 3: Implement**

`QuestOut`, after `collect`:

```rust
    /// Whether the quest is on a real place whose details `Engine::poi_details` can show (the ⓘ button).
    pub has_details: bool,
```

In `quests()`: `has_details: q.feature_id.is_some(),` (capture before `q.place` etc. are moved if needed: compute
`let has_details = q.feature_id.is_some();` at the top of the closure).

New record (near `QuestOut`):

```rust
/// What the map knows about a quest's place, for its details card; each field is `None` when the map does not have it.
#[derive(Debug, uniffi::Record)]
pub struct PoiDetailsOut {
    /// Name of the place.
    pub name: Option<String>,
    /// What kind of place it is ("Cafe", "Trail").
    pub type_label: Option<String>,
    /// Street address.
    pub address: Option<String>,
    /// Opening hours, as OpenStreetMap writes them.
    pub opening_hours: Option<String>,
    /// Phone number.
    pub phone: Option<String>,
    /// Website (`http(s)` only).
    pub website: Option<String>,
    /// Wikipedia article address.
    pub wikipedia_url: Option<String>,
    /// Free-text description.
    pub description: Option<String>,
    /// Trail surface.
    pub surface: Option<String>,
    /// Trail length in the player's units.
    pub length: Option<String>,
    /// Whether the trail is lit: Yes, No or Partly.
    pub lit: Option<String>,
    /// Hiking difficulty.
    pub difficulty: Option<String>,
    /// The place on openstreetmap.org.
    pub osm_url: Option<String>,
}
```

Engine method (in the `// ---------- play ----------` section, after `quests`):

```rust
    /// What the map knows about the place of quest `location_id`, for its details card. Reads the realm's saved scan, so call
    /// it off the UI thread. `None` with no game open, for a quest with no real place, or when the place is gone from the
    /// realm's current scan (rescanned).
    pub fn poi_details(&self, location_id: i64) -> Option<PoiDetailsOut> {
        let (realm_id, feature_id) = self.with_game(|g| g.quest_feature(location_id).map(|(r, f)| (r.to_owned(), f.to_owned())))??;
        let atlas = self.store().load_atlas(&realm_id)?; // read with the game lock released
        let f = atlas.features.iter().find(|f| f.id == feature_id)?;
        let d = apgo_core::poi_details::details(f);
        let units = self.unit_system();
        Some(PoiDetailsOut {
            name: d.name,
            type_label: d.type_label,
            address: d.address,
            opening_hours: d.opening_hours,
            phone: d.phone,
            website: d.website,
            wikipedia_url: d.wikipedia_url,
            description: d.description,
            surface: d.surface,
            length: d.length_m.map(|m| apgo_core::units::distance(m, units)),
            lit: d.lit,
            difficulty: d.difficulty,
            osm_url: d.osm_url,
        })
    }
```

Spec: in "Core → Quest → feature link", change the `has_details` bullet to: "true when `feature_id` is set (no atlas read
per refresh); a place gone after a rescan is reported when ⓘ is tapped."

- [ ] **Step 4: Run tests and the Rust gate**

Run: `just check-rust`
Expected: PASS (fmt, clippy, doc, deny, tests, coverage floor).

- [ ] **Step 5: Commit**

```bash
git add core/ffi/src/engine.rs docs/superpowers/specs/2026-10-11-poi-details-design.md
git commit -m "feat: poi_details engine call"
```

---

### Task 5: Android ⓘ button and details card

> **Superseded:** built, then removed from PR #136; the quest card (`specs/2026-10-11-quest-card-design.md`) replaces it.

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/ui/PoiFormat.kt`
- Create: `android/app/src/test/java/dev/apgo2/ui/PoiFormatTest.kt`
- Modify: `android/app/src/main/java/dev/apgo2/ui/ApgoIcons.kt` (add `val Info = Lucide.Info` under Actions)
- Modify: `android/app/src/main/java/dev/apgo2/PlayDetails.kt` (`QuestDetails` gets `onInfo`; new `PoiDetailsCard`)
- Modify: `android/app/src/main/java/dev/apgo2/AppModel.kt` (`placeFor` state, cleared with `selectedChain`)
- Modify: `android/app/src/main/java/dev/apgo2/PlayScreen.kt` (`QuestPopup` wiring + card)
- Modify: `android/app/src/test/java/dev/apgo2/ui/CollectFormatTest.kt`, `MapGeoJsonTest.kt`, `PlayLayoutTest.kt` (add
  `hasDetails = false` to their `QuestOut(...)` fixtures — new required constructor field)
- Modify: `docs/context/v1-architecture-and-status.md` (one line under the Play screen: ⓘ opens place details from the
  atlas via `poi_details`)

**Interfaces:**

- Consumes: `PoiDetailsOut` and `engine.poiDetails(locationId: Long): PoiDetailsOut?` from Task 4; `QuestOut.hasDetails`.
- Produces: `internal data class PoiRow(val label: String, val value: String, val link: String? = null)`,
  `internal object PoiFormat { fun rows(d: PoiDetailsOut): List<PoiRow>; fun title(d: PoiDetailsOut): String }`.

- [ ] **Step 1: Regenerate bindings and write the failing test**

Run: `bash scripts/android_bindings.sh` (picks up `hasDetails`, `PoiDetailsOut`, `poiDetails`).

```kotlin
package dev.apgo2.ui

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.apgo_ffi.PoiDetailsOut

class PoiFormatTest {
    private fun d(
        name: String? = null,
        typeLabel: String? = null,
        address: String? = null,
        openingHours: String? = null,
        phone: String? = null,
        website: String? = null,
        wikipediaUrl: String? = null,
        description: String? = null,
        surface: String? = null,
        length: String? = null,
        lit: String? = null,
        difficulty: String? = null,
        osmUrl: String? = null,
    ) = PoiDetailsOut(name, typeLabel, address, openingHours, phone, website, wikipediaUrl, description, surface, length, lit, difficulty, osmUrl)

    @Test fun onlyKnownFieldsBecomeRowsInOrder() {
        val rows = PoiFormat.rows(d(address = "12 Main St", openingHours = "Mo-Fr 07:00-17:00", surface = "Gravel", length = "1.2 km"))
        assertEquals(listOf("Address", "Hours", "Surface", "Length"), rows.map { it.label })
        assertEquals(listOf(null, null, null, null), rows.map { it.link })
    }

    @Test fun linksOpenTheRightPlaces() {
        val rows = PoiFormat.rows(d(phone = "+1 555 0100", website = "https://a.example", wikipediaUrl = "https://en.wikipedia.org/wiki/X", osmUrl = "https://www.openstreetmap.org/node/1"))
        assertEquals(
            listOf(
                PoiRow("Phone", "+1 555 0100", "tel:+15550100"),
                PoiRow("Website", "https://a.example", "https://a.example"),
                PoiRow("Wikipedia", "Read the article", "https://en.wikipedia.org/wiki/X"),
                PoiRow("OpenStreetMap", "View on the map", "https://www.openstreetmap.org/node/1"),
            ),
            rows,
        )
    }

    @Test fun nothingKnownGivesNoRows() {
        assertEquals(emptyList<PoiRow>(), PoiFormat.rows(d()))
    }

    @Test fun titleUsesNameThenTypeThenAFallback() {
        assertEquals("Bean There · Cafe", PoiFormat.title(d(name = "Bean There", typeLabel = "Cafe")))
        assertEquals("Cafe", PoiFormat.title(d(typeLabel = "Cafe")))
        assertEquals("Place", PoiFormat.title(d()))
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.ui.PoiFormatTest' --console=plain -q`
Expected: FAIL (unresolved `PoiFormat`, `PoiRow`). Fix the three `QuestOut(...)` fixtures (`hasDetails = false`) now if the
compile also fails there.

- [ ] **Step 3: Implement `PoiFormat.kt`**

```kotlin
package dev.apgo2.ui

import uniffi.apgo_ffi.PoiDetailsOut

/** One line of a place's details card; [link] opens when tapped. */
internal data class PoiRow(
    val label: String,
    val value: String,
    val link: String? = null,
)

/** A place's details as card rows: only what the map knows, in a fixed order. The core already filtered links to http(s). */
internal object PoiFormat {
    fun title(d: PoiDetailsOut): String = listOfNotNull(d.name, d.typeLabel).joinToString(" · ").ifEmpty { "Place" }

    fun rows(d: PoiDetailsOut): List<PoiRow> =
        listOfNotNull(
            d.description?.let { PoiRow("About", it) },
            d.address?.let { PoiRow("Address", it) },
            d.openingHours?.let { PoiRow("Hours", it) },
            d.phone?.let { PoiRow("Phone", it, "tel:" + it.filter { c -> c.isDigit() || c == '+' }) },
            d.website?.let { PoiRow("Website", it, it) },
            d.surface?.let { PoiRow("Surface", it) },
            d.length?.let { PoiRow("Length", it) },
            d.lit?.let { PoiRow("Lit", it) },
            d.difficulty?.let { PoiRow("Difficulty", it) },
            d.wikipediaUrl?.let { PoiRow("Wikipedia", "Read the article", it) },
            d.osmUrl?.let { PoiRow("OpenStreetMap", "View on the map", it) },
        )
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.ui.PoiFormatTest' --console=plain -q`
Expected: PASS.

- [ ] **Step 5: Wire the UI**

`ApgoIcons.kt`, under `// Actions`: `val Info = Lucide.Info`.

`AppModel.kt`, next to `selectedChain`:

```kotlin
    // The quest whose place details card is open (ⓘ in the quest popup), if any.
    var placeFor by mutableStateOf<Long?>(null)
```

and clear it where `selectedChain = null` is cleared (line ~231): `placeFor = null`.

`PlayDetails.kt` — `QuestDetails` gets `onInfo: (() -> Unit)? = null` after `q`, and before the close button:

```kotlin
        onInfo?.let { IconButton(onClick = it) { Icon(ApgoIcons.Info, contentDescription = "About this place") } }
```

New composable in `PlayDetails.kt`:

```kotlin
// What the map knows about a quest's place: a title, then one row per known field; links open in the browser or dialer.
// `d == null` means the place is gone from the realm's scan (rescanned since the game was made).
@Composable
internal fun ColumnScope.PoiDetailsCard(
    d: PoiDetailsOut?,
    onClose: () -> Unit,
) {
    val uri = LocalUriHandler.current
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(
            d?.let { PoiFormat.title(it) } ?: "Place",
            style = MaterialTheme.typography.titleSmall,
            modifier = Modifier.weight(1f),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
        IconButton(onClick = onClose) { Icon(ApgoIcons.Close, contentDescription = "Close") }
    }
    if (d == null) {
        FeedbackText("This place is no longer in the realm's map data.", Tone.Warning)
        return
    }
    val rows = PoiFormat.rows(d)
    if (rows.isEmpty()) Text("The map has no more about this place.", fontSize = 12.sp)
    rows.forEach { r ->
        Column(if (r.link != null) Modifier.fillMaxWidth().clickable { uri.openUri(r.link) } else Modifier.fillMaxWidth()) {
            Text(r.label, fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(r.value, fontSize = 13.sp, color = if (r.link != null) MaterialTheme.colorScheme.primary else LocalContentColor.current)
        }
    }
}
```

(Imports: `androidx.compose.ui.platform.LocalUriHandler`, `androidx.compose.material3.LocalContentColor`,
`uniffi.apgo_ffi.PoiDetailsOut`, `dev.apgo2.ui.PoiFormat`; the rest are already used in the file.)

`PlayScreen.kt` — in the play `Box`, replace `selected?.let { q -> QuestPopup(...) }` with:

```kotlin
        // ⓘ swaps the quest popup for the place card; the scan is read from disk, so off the main thread.
        var place by remember { mutableStateOf<Pair<Long, PoiDetailsOut?>?>(null) }
        LaunchedEffect(m.selected) { if (m.placeFor != m.selected) m.placeFor = null }
        LaunchedEffect(m.placeFor) {
            val id = m.placeFor
            place = if (id == null) null else id to withContext(Dispatchers.IO) { m.engine.poiDetails(id) }
        }
        val open = place?.takeIf { it.first == m.placeFor }
        if (open != null) {
            MapOverlayCard(Modifier.align(Alignment.BottomCenter).above(coverDp)) { PoiDetailsCard(open.second) { m.placeFor = null } }
        } else {
            selected?.let { q -> QuestPopup(m, q, anchorPx, pinPx, coverDp) { bubblePx = it } }
        }
```

and in `QuestPopup`'s `details` lambda:

```kotlin
        QuestDetails(q, onInfo = if (q.hasDetails) ({ m.placeFor = q.locationId }) else null) { m.selected = null }
```

(Imports: `kotlinx.coroutines.Dispatchers`, `kotlinx.coroutines.withContext`, `uniffi.apgo_ffi.PoiDetailsOut`.)

Add to `docs/context/v1-architecture-and-status.md`, in the Play screen part: "Quest popup ⓘ (quests on a real place) opens
place details: `Engine::poi_details` reads the quest's `feature_id` from the realm atlas; trails carry merged way tags."

- [ ] **Step 6: Run the Android gate**

Run: `just check-android`
Expected: PASS (colour tokens, bindings, spotless, detekt, lint, unit tests, kover floor).

- [ ] **Step 7: Verify on a device**

Run: `just emu-start && just emu-run` (or `just android-run` on the phone). Start a game in a scanned realm with POIs:

1. Tap a bench/cafe/park quest pin → popup shows ⓘ → tap → card with title and the known rows; website and OSM links open the
   browser; phone opens the dialer.
2. Tap a trail line → popup ⓘ → card shows Trail, Surface, Length (in the chosen units), Lit/Difficulty when tagged; no
   OpenStreetMap row.
3. A steps/time-away quest shows no ⓘ.
4. Close the card → back to the quest popup; tap the empty map → everything closes.

Ask the owner to confirm on the phone.

- [ ] **Step 8: Commit**

```bash
git add android/app/src docs/context/v1-architecture-and-status.md
git commit -m "feat: place details card in quest popup"
```

---

### Task 6: Finish

- [ ] `just check` passes.
- [ ] Push the branch and open one PR (`feat: POI details card`), body: what it does, the three spec amendments, how it was
  verified, and the attribution footer.
