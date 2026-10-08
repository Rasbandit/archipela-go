# Context Doc: What OpenStreetMap Data Gives Us (measured) and Fog of War

_Last verified: 2026-10-07_

## Status

Measured on ONE suburban foothill area (3.5 km around the owner's home, public Overpass). Other regions differ a lot (tagging density varies by country and county): compute a per-zone "data richness" score and degrade gracefully. Nothing here is built yet.

## What Exists (3.5 km radius)

| Feature | Count / coverage | Use |
| -- | -- | -- |
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
| -- | -- | -- |
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

## Full Census (every category tag, 3.5 km around the owner's home, 2026-10-07)

3,295 features, 22 category keys, 208 distinct key=value kinds. Counts are features near ONE home; use them to see what exists, not what is typical.

| Category | Count | Top kinds (count) | Quest potential |
| -- | -- | -- | -- |
| barrier | 784 | kerb 543, fence 86, gate 58, retaining_wall 34 | mostly noise (kerbs); gates/walls as landmarks at most |
| amenity | 730 | parking 498, bench 36, restaurant 28, fast_food 26, shelter 19, bicycle_parking 17, place_of_worship 13 | rest/dwell spots, food stop, bike parking for bike quests |
| natural | 544 | tree 175, scrub 86, wood 77, sand 68, stone 61, water 35, ridge 21, bare_rock 13, peak 5 | notable trees, ridge walks, rock/sand areas, summits, forests |
| leisure | 364 | garden 193 (private), playground 47, pitch 40, swimming_pool 36, park 28, picnic_table 10 | parks, playgrounds, pitches |
| landuse | 364 | grass 265, residential 44, commercial 19, religious 8, vineyard 4, quarry 3 | area zones, landmark areas (vineyard, quarry) |
| tourism | 110 | artwork 85, hotel 8, information 6, picnic_site 4, museum 3, zoo 1, theme_park 1 | art walk, museums, zoo |
| waterway | 108 | stream 57, dam 17, river 13, canal 8, waterfall 4 | follow-the-creek, waterfall hunt, dam/bridge crossings |
| shop | 72 | clothes 28, shoes 9, convenience 3 | errands (needs dwell), low value |
| sport | 59 | free_flying 15, basketball 13, pickleball 13, tennis 6, baseball 3 | court tours, paragliding launch sites |
| man_made | 40 | pipeline 7, bridge 5, gantry 5, storage_tank 5, mast 3, tower 2, flagpole 2 | bridges, towers, flagpoles as landmarks |
| railway / public_transport | 38 / 32 | stations, stops, platforms | visit stations (cannot prove riding) |
| route (relations) | 31 | bus 8, bicycle 7, road 5, train 4, mtb 1, canoe 1 | **named bicycle/mtb routes with ordered geometry = bike quests** |
| emergency | 15 | fire_hydrant 11, defibrillator 1 | "hydrant hunt": dense points that exist anywhere in towns |
| boundary + place | 11 + 3 | administrative 11; neighbourhood 2, hamlet 1 | **auto zones: fill a neighbourhood/city boundary** |
| historic / attraction | 3 / 3 | monument 3; maze 2, carousel 1 | landmarks |

Elsewhere OSM also has (verify per region): lighthouses, castles, ruins, memorials, caves, hot springs, glaciers, via ferrata, marinas, golf courses, mountain huts, windmills.

## Cities (same census, 2026-10-07)

| Area | Features | Kinds | What dominates | Standouts for quests |
| -- | -- | -- | -- | -- |
| Portland OR downtown, 600 m | 3,500 | 241 | amenity 1,874 (bicycle parking 756, waste baskets 243, benches 185, restaurants 94), shops 290, transit 139+129 | artwork 56, memorials 30, hydrants 247, trees 209, hotels 27, museums 4, light rail/tram/bus stops, bus routes 66, columns 16, flagpoles 12 |
| Amsterdam centre, 400 m | 1,815 | 236 | amenity 522, shops 411, trees 215 | canals 42, bridges 18, towers 27, attractions 27, museums 9, viewpoints 4, tram/subway stops, hotels 28 |

Takeaways: downtowns are rich in POIs and transit but have almost no parks/trails; "visit N of category X" (art, memorials, hydrants, bike racks, bridges, towers, stops, museums) works anywhere dense. Public Overpass was flaky on these queries (one server timed out 3 times, another returned 504): reinforces the static atlas + endpoint fallback plan.

## New Quest Types From The Census

| Quest | Data | Proof |
| -- | -- | -- |
| Walk the park | park polygon | cover X% of the park by track, or walk its perimeter |
| Follow the creek | `waterway=stream\|river\|canal` line | stay in a corridor along the line for N m |
| Waterfall / dam / bridge | point/line features | geofence (crossing a bridge = track crosses it) |
| Ridge walk | `natural=ridge` line | corridor + elevation gain |
| Bike route | `route=bicycle\|mtb` relation (ordered geometry) | corridor coverage + bike mode |
| Court tour | `sport=*` pitches | visit N different sport types |
| Art walk | `tourism=artwork` (85 here) | visit N pieces (fog-discoverable) |
| Hydrant/bench hunt | dense point sets | visit N: works almost anywhere with towns |
| Neighbourhood sweep | `boundary=administrative` / `place` | fill the polygon (auto zone) |
| Summit | `natural=peak` + `ele` | geofence + barometer climb |

## Data-Driven Quest Menu (design)

The apworld cannot know what exists around a player, so it emits generic slot types (`visit_poi(category)`, `follow_line(kind)`, `cover_area(kind)`). On first setup the app **scans the zone once** (one bulk request, cached) and shows what is available with counts ("13 pickleball courts, 4 waterfalls, 7 cycle routes, 28 parks"), disabling quest types with none. Each quest type declares the OSM filter it needs; a curated allowlist plus scoring removes noise (kerbs, private gardens, parking spaces).

## Fog of War (client-side, reuses the Scouting item)

- Points are **hidden until discovered**: within a reveal radius (default ~150 m) of you or of the area you have already walked; discovered points stay revealed and are saved.
- `Progressive Scouting Distance` (already in the apworld) raises the reveal radius; `Fog Of War Trap` re-fogs the map for a while. Locked trips show as faint "?" until unlocked and revealed.
- Optional **sonar pulse** button (cooldown): shows the rough direction/distance band of the nearest undiscovered open point, so you are never lost.
- No apworld change needed; pure client logic. Benefit: no 100-pin clutter, real exploration. Risk: aimless wandering, so keep the pulse and a hint option.
- Distinct from AP "scouting" (revealing which ITEM is at a location); both can use the same distance item.

## References

`docs/context/progression-zones-and-tools.md`, `docs/context/poi-data-sources.md`, `docs/context/spike-b-location-generation-results.md`
