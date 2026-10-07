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

## Findings / Next Fixes
- Rural: POIs alone are too sparse (issue #14 territory). Add fallback candidates from named paths/roads/trailheads (the old
  highway query), and merge them into the same bulk request.
- Fallback "nearest" currently ignores `max_distance` (picked a 17 km park for a 5 km game). It must cap at max distance (or drop the trip).
- Rural cold fetch took 101 s: almost certainly retries/timeouts across endpoints. Log per-endpoint failures, shorten timeout, race endpoints.
- 100 trips from a dense city used only 1185 candidates; multiple trips near one place need `min_spacing` tuning.
- Tile slack (+4 km) makes queries bigger than needed; cache key ignores query version, add a version suffix.
- UA string is a placeholder: add a real contact URL before public release (Overpass etiquette).
- No clippy/rustfmt installed on host yet (`sudo dnf install clippy rustfmt`).

## References
- `docs/context/archipela-go-location-generation.md` (upstream bug + redesign), `docs/context/poi-atlas-and-server-options.md`
