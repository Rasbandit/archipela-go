# Context Doc: Spike B Results (bulk POI location generation)

_Last verified: 2026-10-07_

## Status
Spike done on branch `feat/core-poi-spike` (`core/`, crate `apgo-core`). Proves the upstream hang is fixed by design.

## What It Does
One bulk Overpass query per ~5 km tile (named tourism/historic/leisure/amenity/natural POIs, `out center`), 30-day disk cache,
endpoint fallback list with backoff and HTTP/body validation, then local weighted sampling per distance tier
(`tier t` band = `((t-1)*step, t*step]`, no reuse, 75 m spacing, nearest-fallback flagged `in_band=false`).
Run: `cargo run --release --example gen_trips -- <lat> <lon> <max_m> <trips> [seed]` from `core/`.

## Measured (2026-10-07, public overpass-api.de et al., 100 trips, max 5 km)
| Case | Candidates | Trips | In band | Fetch | Sample |
|--|--|--|--|--|--|
| Portland OR (dense urban), cold | 1185 | 100/100 | 100 | 12.7 s | 0.13 s |
| Portland OR, cached | 1185 | 100/100 | 100 | 5 ms | 0.12 s |
| Moab UT (rural), cold | 93 | 84/100 | 45 | 101 s | 4 ms |
Upstream: 90 minutes for 20 locations (issue #16). One request replaces ~1 per candidate.

## Direction change (owner, 2026-10-07): geozone first, POIs optional
Upstream's real model: pick a random spot in the distance range, snap to the nearest street/path within 200 m. Our model: a
**play zone** (`Zone`: circle, annulus, or any drawn polygon) filled by pluggable **fill strategies**; POIs are an optional flavor.
The zone is client-side config (Archipelago cannot know geography); `slot_data` is unchanged. Run:
`cargo run --release --example gen_zone -- <streets|cells|pois|mixed> <circle lat lon r | annulus lat lon min max | poly "lat,lon;...">  <trips> [seed]`.

| Strategy | Case (100 trips) | Candidates | Placed | In band | Outside zone | Gather |
|--|--|--|--|--|--|--|
| cells (offline lattice) | Moab 5 km circle | 4026 | 100/100 | 100 | 0 | 0 ms |
| streets (bulk, `out geom`, points every 50 m) | Moab 5 km circle | 8928 | 100/100 | 100 | 0 | 21.5 s cold |
| streets | Portland ~2x2 km polygon | 6706 | 100/100 | 100 | 0 | 5.7 s cold |
The rural case that failed with POIs only (84/100, 45 in band) is now 100/100 with streets, and `cells` needs no network at all.
Sampling is 0.4-1.0 s for ~9k candidates (O(n*m) spacing check; add a spatial grid index later).

## Creative fill ideas (backlog)
Coverage game (visit cells to "fill the map", territory/hex capture), neighborhoods from OSM admin boundaries as zones, trail-only
or loop modes, blue-noise spread so trips cover the whole zone, street-length-weighted randomness, sector/angle limits,
POI/landmark flavor on top, quest chains (ordered points), elevation bands.

## Findings / Next Fixes
- Rural: POIs alone are too sparse (issue #14): solved by the streets strategy above (POIs now optional).
- Fallback "nearest" currently ignores `max_distance` (picked a 17 km park for a 5 km game). It must cap at max distance (or drop the trip).
- Rural cold fetch took 101 s: almost certainly retries/timeouts across endpoints. Log per-endpoint failures, shorten timeout, race endpoints.
- 100 trips from a dense city used only 1185 candidates; multiple trips near one place need `min_spacing` tuning.
- Tile slack (+4 km) makes queries bigger than needed; cache key ignores query version, add a version suffix.
- UA string is a placeholder: add a real contact URL before public release (Overpass etiquette).
- No clippy/rustfmt installed on host yet (`sudo dnf install clippy rustfmt`).

## References
- `docs/context/archipela-go-location-generation.md` (upstream bug + redesign), `docs/context/poi-atlas-and-server-options.md`
