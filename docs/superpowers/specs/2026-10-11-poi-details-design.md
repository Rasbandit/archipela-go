# POI details — design

> **Display superseded** by `2026-10-11-quest-card-design.md` (no ⓘ button or separate card; details fold out inside the
> quest card). The core parts here (feature link, relink, trail tag merge, tag mapping) still apply.

## Intent

Tapping a quest pin, trail or park already opens the quest popup with the place name. The owner wants to see more about
the real-world place behind a quest: an info icon in the popup opens a card with what OpenStreetMap knows about it.

Decided with the owner:

- **Fields:** type and address; opening hours, phone and website; description and Wikipedia; trail info (surface,
  length, lit, difficulty). Each row shows only when OSM has the value.
- **Display:** a `MapOverlayCard` above the bottom panel (same pattern as `ChainDetails`), with a close button. The map
  stays visible.
- **One PR** covering core, trail tag merge and Android.

Success: tapping ⓘ on a cafe quest shows "Cafe", its address, hours, phone and a website link; on a trail quest it shows
surface, length, lit and difficulty; quests with no linked feature (old saves) show no ⓘ and never crash.

## Approach

Keep a link from each quest to its atlas feature and look the details up lazily when ⓘ is tapped. The scan already keeps
all OSM tags on `Feature.tags` (`core/src/scan.rs`), but `Cand.feature_id` is dropped when the `Assignment` is built.

Rejected:

- Details as eager fields on `QuestOut`: every quest refresh would carry tags nobody looks at.
- Raw tags to Kotlin, parsed there: breaks the cross-platform rule (iOS would re-implement the mapping).

## Core

### Quest → feature link

- `Assignment` gets `pub feature_id: Option<String>` with `#[serde(default)]`, set from `Cand.feature_id`. Old saves
  load with `None`.
- `QuestOut` gets `has_details: bool`: true when `feature_id` is set (no atlas read per refresh); a place gone after a
  rescan is reported when ⓘ is tapped.

### Games saved before the link

Owner ruling: existing games must get ⓘ too, without starting a new game. When a game is opened, the background street build
(which already loads the zones' atlases) also calls `Game::relink_places`. For each quest with no `feature_id`, `place_of`
picks the atlas place of the same kind (any kind for the boss) and name nearest the quest's spot, within 100 m. For a park
it must be the same outline, and for a trail the distance is to its line. A linked game is saved once. The app reloads
quests when the build lands. Quests with no real place stay unlinked.

### Trail tag merge (`scan.rs`, synthetic `L:` features)

When ways are grouped and stitched into a trail chain, their tags are merged onto the synthetic feature (the existing
`name` and `rough` stay):

- `surface`: the value covering the most length.
- `lit`: `yes` if every way is lit, `no` if none, `partial` otherwise; omitted when no way has `lit`.
- `sac_scale`: the hardest value present (standard `sac_scale` order).
- `operator`, `description`, `website`, `wikipedia`: the first value found.

Only ways that end up in the chain contribute. Length is not stored; `poi_details` computes it from the geometry.

### `poi_details` (new module `core/src/poi_details.rs`)

```rust
pub struct PoiDetails {
    pub name: Option<String>,
    pub type_label: Option<String>,   // "Cafe", "Playground", "Trail"
    pub address: Option<String>,      // "123 Main St, Springfield"
    pub opening_hours: Option<String>,// raw OSM string, not parsed
    pub phone: Option<String>,
    pub website: Option<String>,      // http(s) only
    pub wikipedia_url: Option<String>,// "en:Foo" -> https://en.wikipedia.org/wiki/Foo
    pub description: Option<String>,
    pub surface: Option<String>,
    pub length_m: Option<f64>,        // line features only
    pub lit: Option<String>,
    pub difficulty: Option<String>,   // from sac_scale
    pub osm_url: Option<String>,      // real n/w/r ids only, not synthetic L: ids
}

pub fn details(feature: &Feature) -> PoiDetails;
```

- `type_label`: small table of common `key=value` → label (amenity, leisure, tourism, shop, historic, highway path
  kinds); fallback humanizes the value of the first known key (`ice_cream` → "Ice cream").
- `address`: `addr:housenumber addr:street`, then `addr:city`; `None` if no street.
- `website`: `website` or `contact:website`; dropped unless it starts with `http://` or `https://`.
- `phone`: `phone` or `contact:phone`, as given.
- Engine FFI: `poi_details(location_id) -> Option<PoiDetailsOut>`. `None` when the quest has no `feature_id` or the
  feature is no longer in the atlas (rescanned).

## Android

- `QuestDetails` (`PlayDetails.kt`) gets an ⓘ icon button when `quest.hasDetails`.
- Tapping it calls `engine.poiDetails(locationId)` and shows a new `PoiDetails` composable in `MapOverlayCard` (replacing
  the quest popup while open; close returns to the quest popup, and tapping the empty map closes everything).
- Rows render only for non-null fields; labels live in `ui/HelpText.kt`, colours from palette tokens.
- Website, Wikipedia and OSM open with `Intent.ACTION_VIEW`; phone with `tel:`. Kotlin builds no URLs itself beyond
  `tel:`.

## Errors and safety

- Missing link or feature → no icon / `None`; never a crash.
- Tag values are untrusted: only displayed as text or opened as `http(s)`/`tel:` intents. The scheme check is in the
  core.

## Testing (TDD)

- `poi_details`: each field mapping; empty tags → all `None`; non-http website dropped; wikipedia URL building;
  `osm_url` for `n`/`w`/`r` but not `L:`; length only for lines.
- Trail merge: mixed surfaces (longest wins), lit yes/no/partial/absent, hardest `sac_scale`, first operator.
- Assignment: `feature_id` set from the candidate; old JSON without it deserializes to `None`.
- Engine: `has_details` false for missing feature; `poi_details` returns `None` after the feature disappears.
- Android: verified on emulator/phone (tap ⓘ on a POI quest and a trail quest).
