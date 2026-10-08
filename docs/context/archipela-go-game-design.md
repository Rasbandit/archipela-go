# Context Doc: Archipela-Go! Game Design (APWorld + client contract)

_Last verified: 2026-10-07_

## Status

Working reference. Based on apworld release 0.7.0 (`apgo.apworld`, read from source inside the zip) and upstream README. Items marked (UNVERIFIED) were not confirmed.

## What This Is

Archipela-Go! is an Archipelago (multiworld randomizer) game played by walking/jogging in the real world. Each "check" is a real-world point (a "trip") at some distance from home. Reaching it sends the check; items arrive in other slots/yours.

## Environment

- APWorld: Python, `apgo/` package (world code by agilbert1412 / "Kaito Kid"; app by aki665). Needs Archipelago >= 0.6.3 per yaml.
- Game name string (exact): `Archipela-Go!`. ID offset: `8902301100000` for both items and locations.
- Source location: only as release assets `apgo.apworld` + `Archipela-Go.yaml` on github.com/aki665/react-native-archipelago/releases (0.7.0). No separate apworld repo found (searched GitHub: agilbert1412 has none). Discord thread "future-games-design" is where upstream says to ask (not checked).
- apworld has NO license file and NO `archipelago.json` manifest (upstream issue #17: warns it stops working with AP 0.7.0 without one).

## Goals (slot_data `goal`)

| Value | Option | Win condition (apworld) | Client status upstream |
| -- | -- | -- | -- |
| 0 | one_hard_travel | `Goal` location needs ALL Progressive Distance Reductions | NOT implemented in app |
| 1 | allsanity | same rule in apworld (!), app wins when all trips checked | app OK |
| 2 | short_macguffin (default option value 2) | collect letters of `Ap-Go!` (A,p,-,G,o,!) = 6 | app OK |
| 3 | long_macguffin | letters of `Archipela-Go!` (13 items) | app OK |

Note: apworld `setup_victory` gives allsanity the distance-reduction rule too (likely copy/paste bug; UNVERIFIED intent). Victory event item "Victory" is locked on location `Goal` (id offset+0) in the last Area region.

## Items (id = offset + n, classification)

- +1 Progressive Distance Reduction (progression), +2 Progressive Key (progression), +3 Progressive Scouting Distance (useful), +4 Progressive Collection Distance (useful).
- Traps: +101 Shuffle, +102 Silence, +103 Fog Of War (app-based, "do nothing" upstream); +151 Push Up, +152 Socializing, +153 Sit Up, +154 Jumping Jack, +155 Touch Grass (honor system).
- Macguffins (progression): +201 A, +202 r, +203 c, +204 h, +205 i, +206 p, +207 e, +208 l, +209 a, +210 "-", +211 G, +212 o, +213 "!". Item groups "MacGuffins"/"Letters" = long set.
- Fillers: +251 "Hydrate!", +252 "Take a Breather!" (honor system).
- Item creation order: goal macguffins, then N keys (`number_of_locks`), then traps = `(trips - items) * trap_rate / 100`, then random fill from {Hydrate, Breather, + Distance Reduction/Scouting/Collection if toggled}. Distance reductions capped at `max(5, floor(0.15 * trips))`.
- Only trap of ID type is classification `trap`; app plays sound by highest flag.

## Locations ("trips")

- Name: `Trip Distance {d} [(Area {k})] [(Speed {s})] #{n}`; d in 1..10, s in 0..10 (0 = none), k in 0..10.
- location_table built from ALL templates (d x s x k) with `n` up to `max(1,(11-k)//2)` (x4, cap 20, when s==0). ID = offset + running index. Datapackage is large but ~4300 per 0.4.2 notes.
- Each generated trip has `distance_tier` (1..10), `key_needed` (= Area k), `speed_tier`.
- Generation (`Trips.generate_trips`): speed templates only if `speed_requirement>0` (else only s=0); templates with k > `number_of_locks` excluded; sample templates or increment copies up to max; then force at least one trip per key tier and per distance tier 1..10.
- `force_change_options_if_incompatible`: max_trips = 1210 - (10-locks)*100, //10 if speed==0; locks clamped to trips//2.

## Logic

- Regions: Menu -> Area 0 -> Area 1 ... Area N (N = highest key_needed). Entrance `Area i-1 -> Area i` requires `Progressive Key` count >= i. Location sits in region `Area {key_needed}`.
- If `enable_distance_reductions`: each trip with tier>1 requires `get_reductions_needed_to_be_reachable` reductions (distance.py). Math: reduction_percent = 1/(expected_reductions+4); remainder = 1 - that; per-tier out-of-logic range = max_distance / remainder^expected_reductions / num_tiers; distance(trip, r) = tier _range_ remainder^r. Trips may start beyond `maximum_distance` and become reachable as reductions arrive.
- Speed requirement: not enforced by app or logic upstream.

## YAML options (apworld defaults in parentheses)

goal (short_macguffin), number_of_trips 1-1000 (10; template yaml 20), minimum_distance 100-5000 m (500), maximum_distance 1000-50000 m (5000), speed_requirement 0-20 km/h (5; "not implemented"), number_of_locks 0-10 (3), enable_distance_reductions (off; "not implemented in app"), enable_scouting_distance_bonuses / enable_collection_distance_bonuses (off; not implemented), trap_rate 0-100 (50), death_link (off; no client handling known), plus standard AP options. Named ranges: 2k,5k,10k,half_marathon,marathon,50k,50_miler,100k,100_miler (distance); walk/jog/run/bicycle speeds.

## slot_data (exact keys from `fill_slot_data`)

`goal` (int), `minimum_distance` (m), `maximum_distance` (m), `speed_requirement`, `trips`: `{location_name: {distance_tier, key_needed, speed_tier}}`.
The client must derive real coordinates itself; the server never sends coordinates. Distance reductions / scouting / collection bonuses are not in slot_data (client counts received items).

## What the client must implement

1. Connect (game `Archipela-Go!`, items_handling all), fetch slot_data, map names to ids via datapackage `locationTable`.
2. Generate a real-world coordinate per trip, target distance from home within [min,max]; upstream uses `max_distance/10 * distance_tier` as the tier's max radius (ignores reduction math).
3. Lock trips whose `key_needed` > count of Progressive Key received; unlock on receipt.
4. Geofence/proximity detect -> `LocationChecks`; persist and re-send on reconnect (offline queue).
5. Track goal (macguffin letters / all trips) -> `updateStatus(goal)`; offer !release/!collect.
6. Hints, chat, reroll (2 min cooldown upstream), ban list.
7. Not done upstream (our improvement targets): distance reductions (shrink radius via `remainder^r`), scouting distance, collection distance (bigger check radius), speed requirement, traps, One Hard Travel goal, return-home between trips, death link, multiple checks per location (issue #19).

## Failed Approaches / Dead Ends

- Looking for a standalone apworld repo: none found; use release assets.

## Gotchas

- Upstream MapScreen computes key count as `received.map(item.id===KEY).length` (= total items, not keys) -> bug.
- Upstream handleItems macguffin removal uses case-sensitive `replace` on a string; "A" vs "a" and `-` need care.
- Item/location id offset literal is the same for both (items and locations are separate namespaces).

## References

- <https://github.com/aki665/react-native-archipelago> (branch `archipela-go`), releases 0.7.0
- Related: `docs/context/archipela-go-upstream-architecture.md`, `docs/context/archipela-go-location-generation.md`
