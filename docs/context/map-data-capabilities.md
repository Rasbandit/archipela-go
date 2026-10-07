# Context Doc: What OpenStreetMap Data Gives Us (measured) and Fog of War

_Last verified: 2026-10-07_

## Status
Measured on ONE suburban foothill area (3.5 km around the owner's home, public Overpass). Other regions differ a lot (tagging density varies by country and county): compute a per-zone "data richness" score and degrade gracefully. Nothing here is built yet.

## What Exists (3.5 km radius)
| Feature | Count / coverage | Use |
|--|--|--|
| Ways total (paths, sidewalks, service, roads) | 5,540 | street/path fill (done) |
| Footway / service / residential / cycleway / path / track / steps | 2165 / 2062 / 580 / 272 / 145 / 41 / 29 | foot vs bike vs car zones |
| Parks and reserves (`leisure=park\|nature_reserve`) | 28, all mapped as polygons, 17 named | park quests, area coverage, perimeter loops |
| Playgrounds / benches / shelters / toilets | 47 / 36 / 19 / 12 | family, rest/dwell spots |
| Artwork (`tourism=artwork`) | 85 | "art walk" discovery points |
| Peaks (with `ele`) | 5 (3 have elevation) | summit quests |
| Picnic sites / trailheads / viewpoints / drinking water | 4 / 1 (8 within 5 km) / 0 / 1 | start points, rest stops |
| Gardens (`leisure=garden`) | 193 | mostly private yards: EXCLUDE from quests |
| Named POIs / with Wikidata | 13% / 0% | genuineness scoring is weak here |

## Tag Coverage on Foot/Bike Ways (2,657 ways): the "avoid dirt paths" reality
| Tag | Tagged | Notes |
|--|--|--|
| `surface` | 18% (42% on `highway=path`) | values: asphalt 196, concrete 171, dirt 62, unpaved 14, ground 9, paved 8 |
| `lit` | 0% | cannot offer a "well-lit only" night filter here |
| `sac_scale` (hiking difficulty) | 1% | compute difficulty ourselves (length, elevation) |
| `trail_visibility`, `smoothness`, `incline`, `wheelchair` | 1%, 0%, 0%, 0% | do not rely on them |
| `bicycle` / `foot` access | 13% / 12% | partial; useful when present |
| `name` | 5% of ways (47 distinct named trails within 3.5 km) | trail quests by name |

## Surface Filter Design
Setting: **Any / Prefer paved / Paved only**.
- Known paved: `asphalt, concrete, paved, paving_stones, sett, compacted`. Known unpaved: `dirt, ground, unpaved, grass, sand, gravel, mud, earth`.
- Untagged ways use a type heuristic: sidewalks/crossings/cycleways/pedestrian/residential/service = paved; `highway=path` and `track` = unknown (treat as unpaved in "Paved only", neutral in "Prefer paved").
- Also: **Avoid steps** (`highway=steps`), **Trails only / no trails**, **max trail length**. Always exclude `access=private|no`.
- Show the player how much data backed the choice ("12% of paths here are surface-tagged").

## What Else We Can Build From It
Park visits and park-perimeter loops, "visit 3 different parks", playground/art walks, peak bagging (use `ele`; real climb via the barometer), rest-spot dwell quests (benches, shelters, picnic sites), trailhead collection, trail-network completion %, trail length classes (short <1 km, medium 1-3, long 3-8, epic >8). Elevation profiles need a DEM (not OSM): free options to evaluate (Open-Meteo elevation API, terrain tiles, SRTM). (verify) Treat OSM as crowd-sourced; keep the fallback to plain streets/cells.

## Fog of War (client-side, reuses the Scouting item)
- Points are **hidden until discovered**: within a reveal radius (default ~150 m) of you or of the area you have already walked; discovered points stay revealed and are saved.
- `Progressive Scouting Distance` (already in the apworld) raises the reveal radius; `Fog Of War Trap` re-fogs the map for a while. Locked trips show as faint "?" until unlocked and revealed.
- Optional **sonar pulse** button (cooldown): shows the rough direction/distance band of the nearest undiscovered open point, so you are never lost.
- No apworld change needed; pure client logic. Benefit: no 100-pin clutter, real exploration. Risk: aimless wandering, so keep the pulse and a hint option.
- Distinct from AP "scouting" (revealing which ITEM is at a location); both can use the same distance item.

## References
`docs/context/progression-zones-and-tools.md`, `docs/context/poi-data-sources.md`, `docs/context/spike-b-location-generation-results.md`
