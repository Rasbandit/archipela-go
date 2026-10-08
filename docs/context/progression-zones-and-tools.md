# Context Doc: Progression Design (zones, tools, trails, traps)

_Last verified: 2026-10-07_

## Status

Design proposal from owner brainstorm; not yet in the apworld. Next spec candidate. Builds on `archipelago-game-model.md`.

## Owner Decisions (2026-10-07)

- **Only quests the phone can prove reliably.** Dropped: NFC, photo/camera quests, "touch grass", weather, social/team features (for now).
- Provable signals we trust: GPS track (position, dwell, speed, bearing, coverage), step counter, barometer, Activity Recognition (walking / cycling / in-vehicle), Health Connect (non-manual records).
- The game needs a **concept of home** (already used as the tier reference).
- Keep: hikes/trails ("do X hike"), cardinal quests (see below), traps (esp. a Freeze trap), progression gating through zones and modes. Users define much of this in the app.

## Do OSM trails work? (measured near the owner's home, 2026-10-07)

- Formal hiking route relations (`route=hiking`): **0** within 5 km. Do not rely on them.
- Trailheads (`highway=trailhead`): **8** within 5 km (names like "... Trailhead").
- Named path/footway/track ways within 3.5 km: **47 trails** (66 ways). Top ones 3.5-9.2 km, with `surface=dirt` and `sac_scale=hiking` on most.
- Group ways by `name`; trail **ends** are nodes that appear once (2 ends = a simple trail, 4-6 = a branching network; 0 = a loop). Length comes from the geometry; difficulty from `sac_scale`, surface from `surface`.
- Caveats: plain ways, not curated routes; stitching by name + connectivity is needed; some trails split in many ways or share names. Quality varies by region (fallback: any named path).

## Trail Quests

| Quest | How it plays | Proof |
| -- | -- | -- |
| Do trail X | walk the named trail end to end | start and end reached + >= 90% of the polyline within a 25 m corridor, mode = walking |
| Trailhead run | reach a trailhead, then the trail's far end | two geofences + track coverage |
| Loop trail | complete a loop trail | start = end, coverage |
| Out and back | reach the far end and return | far-end geofence + return |

`params`: `trail` (resolved client-side by name or OSM id), `min_km`, `max_km`, `difficulty`. The apworld only says "a walk-only trail slot of length class N".

## Cardinal Quest (replaces "force walk north")

Use the GPS-derived bearing, not the compass (reliable). "Walk 300 m with track bearing within +/-45 degrees of north", or "reach a point in the northern sector of your zone". Sectors N/E/S/W are relative to home.

## Progression: Zones Are Archipelago Regions

Archipelago logic is regions + access rules, which maps directly onto geozones.

- **Player-defined geometry, YAML-defined structure.** In the app a "Zone Setup" screen lets the player draw each zone (polygon/circle), name it, and pick a fill strategy (the polygon draw/fill already works). The YAML only says how many zones exist, their mode and how they unlock.
- **Auto default:** concentric rings from home (e.g. 0-2 km walk, 2-8 km bike, 8-40 km car), zero setup; custom polygons are optional.
- **Items:** `Bike` and `Car` (tool items), `Zone Key` (progressive or per zone), `Trail Pass` (unlocks trail quests), existing `Progressive Key` and distance reductions.
- **Logic sketch:** Zone 1 (home, walk) open from start. Zone 2 (more walking zones) needs a Zone Key. Bike zone needs `Bike`. Car zone needs `Car`. A "summit" or long trail can be the goal gate.
- **Mode on each slot:** trips carry `mode` and `zone`; bike/drive slots require the tool item in logic. A slower mode cannot satisfy a walk-only slot.
- **Proving mode:** GPS speed bands + Activity Recognition (`WALKING`, `ON_BICYCLE`, `IN_VEHICLE`). Car vs bus/passenger cannot be told apart: "vehicle". Acceptable for a cooperative game.

## More Gating Ideas

| Idea | Idea in one line | Provable by |
| -- | -- | -- |
| Zone keys | many walking zones unlocked one by one | geofence |
| Tool items | Bike/Car unlock modes and wider areas | speed/activity |
| Time-of-day zones | "Night zone" only checkable after dark (opt-in, safe areas) | local clock + geofence |
| Boss gate | goal requires a hard quest (summit/long trail/16 km day) | track + elevation |
| Stamina | daily step budget limits how many checks per day | step counter |
| Day streak gate | unlock needs N active days | daily totals |

## Difficulty-Based Progression (owner idea: weigh progression on easy / medium / hard)

**Effort model.** Every quest has an estimated _effort in active minutes_, computed client-side from its type and the player's mode: reach point = distance / mode speed (walk ~4.5 km/h, bike ~15, drive ~35, with a detour factor); dwell = travel + minutes; trail = length / 3 km/h + 1 min per 10 m of climb; step milestone = steps / 100 per minute. Effort tier = effort / `minutes_per_tier` (YAML, default 10 min). This replaces pure distance tiers and makes quests comparable across modes and types (a 30-minute drive and a 30-minute walk are the same tier). Bands: **Easy** tiers 1-3, **Medium** 4-7, **Hard** 8-10.
**Archipelago already supports weighting.** Verified in the source (0.6.8): every world gets `exclude_locations` ("Prevent these locations from having an important item") and `priority_locations`, and a world can define `location_name_groups` that players reference by name in YAML. So:

- Encode difficulty (and mode) in static location blocks, e.g. `Easy Walk Quest #n`, `Hard Bike Quest #n` (the apworld uses the first k of each block per seed). Then groups `Easy`, `Medium`, `Hard`, `Walk`, `Bike`, `Drive` work in YAML, hints are readable, and no custom option is needed. This changes the compact `Trip #n` pool of schema v1 (schema v2).
- Player choice via standard options: `exclude_locations: [Hard]` = hard quests are pure optional challenges (only junk there); `priority_locations: [Hard]` = boss-style, important items live in hard quests. Default fill = mixed.
- YAML knobs: `easy_share / medium_share / hard_share` (default 50/35/15), `minutes_per_tier`.
- Logic stays gated by zones, tool items and keys; difficulty decides which slots may hold progression. Goal can require a Hard "boss" quest. Guarantee sphere 0 has enough Easy slots.
- Client duty: realize each slot at its effort tier (like distance tiers today) and flag when it cannot.

## Freeze Trap (and friends)

**Freeze:** while frozen, no other checks count until you reach a **thaw point** X.

- Thaw point chosen on receipt: within tier-1 distance of the player (a landmark or just a point), never on roads/private land, and always reachable.
- Safety valves: a timer fallback (e.g. 30 min), and a YAML option to disable or cap distance. Persist the frozen state across app restarts.
- Variants: **Fog** (hide the map), **Shuffle** (reroll places), **Leash** (stay within r m of home for N min), **Detour** (next check needs a waypoint first), **Toll** (walk N steps to pay), **Slow** (double dwell time).
- Traps are normal items; the client implements the effect. Each trap type needs a YAML on/off.

## Contract Impact (schema v2 sketch)

`zones: [{id, name, mode, unlock}]`; each trip gets `zone`, `type`, `params`, `needs`, `fallback`; new item semantics (`Bike`, `Car`, `Zone Key`, `Trail Pass`, trap effects). Apworld work: new options (`zone_count`, `zone_modes`, `enabled_trap_types`, `quest_types`), regions per zone, entrances with item rules, tests (beatability with every tool order, no unreachable zone).

## Suggested Build Order

1. Spec + apworld schema v2: zones, mode tools, zone keys.
2. App: Zone Setup (multi-polygon) + per-zone fill + lock display (reuse the map).
3. Trail quests (OSM trail stitching + corridor verification).
4. Freeze trap, then other traps.
5. Dwell/courier quests, passive step/away-from-home quests.

## References

`docs/context/quest-types-and-phone-apis.md`, `docs/context/archipelago-game-model.md`, `apworld/docs/contract.md`
