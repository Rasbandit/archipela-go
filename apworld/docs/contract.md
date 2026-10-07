# Client Contract v1 (`schema_version` = 1)

Normative description of what the apworld sends and what a client must do with it. The machine-readable
form is `slot_data.schema.json` in this folder; if the two disagree, fix the schema and this file together.

Game name: `Archipela-Go 2: Electric Boogaloo`. Item and location IDs are offset by `8_902_400_000_000`
(separate namespaces).

## slot_data

| Field | Type | Meaning |
|--|--|--|
| `schema_version` | int, always 1 | Clients **must refuse** seeds with a version they do not know |
| `goal` | `all_trips` / `macguffin_short` / `macguffin_long` | Win condition |
| `min_distance_m` | int | Closest a trip may be from home, meters |
| `max_distance_m` | int | Farthest a trip may be from home with no reductions, meters |
| `allowed_modes` | array of `walk` / `bike` / `drive` | Modes the player accepted |
| `return_home` | bool | Player must return to the home point between trips |
| `death_link` | bool | Enable the standard `DeathLink` tag |
| `reduction_percent` | int 1-25 | Each Distance Reduction item shrinks effective distance by this percent |
| `tier_step_m` | number | Meters per distance tier |
| `trips` | array | One entry per active trip, see below |

Each trip: `location_id` (int, = ID_OFFSET + n where the location name is `Trip #n`), `type` (string, currently
only `reach_point`), `distance_tier` (1-10), `key_needed` (0-10), `mode` (`walk` / `bike` / `drive`).

Rules for clients:
- Ignore (show as unsupported, never send) trips whose `type` is unknown; a later schema may add types.
- Coordinates are never sent. The client derives a real-world point per trip.

## Distance

- Base distance of a trip: `distance_tier * tier_step_m`.
- Effective distance: `base * (1 - reduction_percent / 100) ** reductions_received`, where
  `reductions_received` is the count of `Progressive Distance Reduction` items received.
- The pool may contain more reductions than gate logic (surplus); clients still apply all received reductions.
- Pick a point at the effective distance, but never closer than `min_distance_m`.

## Keys (areas)

A trip is checkable only when the number of `Progressive Key` items received is at least `key_needed`.
`key_needed = 0` trips are available from the start.

## Items

| Item | Class | Client behavior |
|--|--|--|
| Progressive Key | progression | Unlocks areas as above |
| Progressive Distance Reduction | progression | Counted for the distance formula |
| Progressive Scouting Distance | useful | Client-defined bonus: reveal the contents of nearby locations |
| Progressive Collection Distance | useful | Client-defined bonus: enlarge the check radius |
| Shuffle / Silence / Fog Of War Trap | trap | Client-implemented app traps |
| Push Up / Socializing / Sit Up / Jumping Jack / Touch Grass Trap | trap | Honor system: show a notification |
| Hydrate!, Take a Breather! | filler | Honor system: show a notification |
| Letter A, R, C, H, I, P, E, L, G, O | progression | Counted toward the macguffin goals |

Macguffin sets: short = A, P, G, O; long = A, R, C, H, I, P, E, L, A, G, O (two `Letter A`).

## Travel modes and speed bands

Each trip names the mode the player must use. Initial speed bands (tunable; changing them bumps
`schema_version`): walk 0-9 km/h, bike 8-35 km/h, drive 25+ km/h. The client checks GPS speed on the leg
into the trip; the server does not enforce speed.

## Goal

- `all_trips`: send goal complete when every active trip is checked.
- `macguffin_short` / `macguffin_long`: send goal complete when every letter of the chosen set is held.
- Use the standard Archipelago `StatusUpdate` (goal) packet.

## DeathLink and return_home

- `death_link`: when true, connect with the `DeathLink` tag and handle Bounce packets per the Archipelago
  protocol.
- `return_home`: the client enforces a return to home between trips. The server does nothing for it.
