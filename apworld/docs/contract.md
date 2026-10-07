# Client Contract v2 (`schema_version` = 2)

Normative description of what the apworld sends and what a client must do with it. Machine-readable form:
`slot_data.schema.json`; a real example: `slot_data.sample.json` (3 zones walk/bike/drive, 60 trips, goal
boss); a player YAML: `example.yaml`. If the schema and this file disagree, fix both together.

Game name: `Archipela-Go 2: Electric Boogaloo`. Item and location IDs are offset by `8_902_400_000_000`
(items and locations are separate namespaces).

## Unknown versions and types

- Refuse (do not play) a seed whose `schema_version` is not 2; show a clear "update the app" message.
- Ignore quest `type` values you do not know: show them as unsupported, never send their location.
- Ignore unknown extra fields; a breaking change bumps `schema_version`.

## slot_data

| Field | Type | Meaning |
|--|--|--|
| `schema_version` | int, 2 | See above |
| `goal` | enum, 12 values | Win condition, see Goals |
| `goal_target` | int 0-1000 | N for counting goals; 0 = use the goal's default |
| `minutes_per_tier` | int 5-30 | Effort minutes per tier |
| `reduction_percent` | int 1-25 | Each Progressive Effort Reduction shrinks effort by this percent |
| `min_distance_m` | int | Closest a place may be to home, meters |
| `fog_of_war`, `return_home`, `death_link` | bool | Client behaviors |
| `enabled_traps` | array | Trap groups in the pool: freeze, fog, shuffle, silence, leash, detour, toll, slow, honor |
| `zones` | array | `{id, mode, zone_keys_needed, tool}`; zone 1 is free; `tool` is an item name or null |
| `trips` | array | One quest per entry (below) |
| `boss` | object or null | A quest entry; non-null for goals `boss` and `treasure_hunt` |

Quest entry: `location_id` (int), `zone` (1-6), `mode` (`walk|run|bike|drive`), `difficulty`
(`easy|medium|hard`), `effort_tier` (1-10), `type` (family, or `boss`).

## Effort

`effort_min = effort_tier * minutes_per_tier` active minutes (Effort Reductions multiply by
`(1 - reduction_percent/100) ** reductions_received`, rounded up to whole minutes).
Bands: easy tiers 1-3, medium 4-7, hard 8-10. Nominal mode speeds for turning minutes into distance:
walk 4.5, run 9, bike 15, drive 35 km/h. Never place a place closer than `min_distance_m`.

## Families (`type`) and modes

| Family | Modes |
|--|--|
| reach, dwell, landmark, courier, away | walk, run, bike, drive |
| explore, trail, water | walk, run, bike |
| park, steps | walk, run |
| boss | the last zone's mode |

The apworld picks the family; the client picks the concrete kind and place from the realm's atlas, falling
back to reach when nothing fits.

## Zones, keys, tools

A zone `k` is open when `Progressive Zone Key` count >= `zone_keys_needed` (= k-1) and, if `tool` is set, that
tool item is held (and all earlier zones are open). Tools exist only for zones whose mode differs from zone
1's and is run/bike/drive: `Running Shoes` (run), `Bike` (bike), `Car` (drive). Walking never needs a tool.
Quests of a closed zone must not be sent.

## Locations

Names `"{Difficulty} {Mode} Quest #{n}"` (e.g. `Hard Bike Quest #3`); ID = `ID_OFFSET + block*1000 + n`,
`block = difficulty_index*4 + mode_index` (Easy, Medium, Hard; Walk, Run, Bike, Drive). `Boss Quest` is
`ID_OFFSET + 12001`. Location groups (for `exclude_locations`, hints): Easy, Medium, Hard, Walk, Run, Bike,
Drive, Boss (the boss is also in Hard).

## Items

| Item | Class | Client behavior |
|--|--|--|
| Progressive Zone Key | progression | Opens zones |
| Running Shoes / Bike / Car | progression | Tools, see Zones |
| Progressive Effort Reduction | useful | Reduces effort (not logic) |
| Progressive Scouting Distance / Collection Distance | useful | Larger reveal / check radius |
| Freeze, Fog Of War, Shuffle, Silence, Leash, Detour, Toll, Slow Trap | trap | App-implemented traps |
| Push Up, Socializing, Sit Up, Jumping Jack, Touch Grass Trap | trap | Honor system notification |
| Hydrate!, Take a Breather! | filler | Notification |
| Letter A, R, C, H, I, P, E, L, G, O | progression | Counted for letter goals |

Item groups: Traps, App Traps, Honor Traps, Tools, Letters, Zone Keys, Fillers. Letters: short set APGO,
long set ARCHIPELAGO (two `Letter A`); letters are in the pool only for `macguffin_*` and `treasure_hunt`.

## Goals

Report goal complete with the standard `StatusUpdate` packet.

| Goal | Win when | Default target |
|--|--|--|
| macguffin_short / macguffin_long | all letters of APGO / ARCHIPELAGO held | - |
| all_trips | every quest done | - |
| boss | Boss Quest done | - |
| treasure_hunt | APGO held, then Boss Quest done | - |
| zone_conqueror | >= N% of quests in every zone done | 60 |
| well_rounded | one quest of every enabled family done | - |
| quest_dex | N distinct quest kinds done | 15 |
| marathon | N km tracked during quests | 42 |
| explorer | N map cells (150 m) visited | 300 |
| streak | N consecutive days with a quest done | 7 |
| boss_rush | N Hard quests done | 5 |

Archipelago logic only requires the last zone for the client-evaluated goals (zone_conqueror through
boss_rush); the client decides when they are satisfied.

## DeathLink and return_home

`death_link`: connect with the `DeathLink` tag and handle Bounce packets. `return_home` and `fog_of_war` are
enforced by the client only.
