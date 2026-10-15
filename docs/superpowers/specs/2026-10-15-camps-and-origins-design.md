# Camps and zone origins design

_Date: 2026-10-15. Owner decisions from the new-game redesign session. Builds on `feat/walk-recap` (progress counts only inside
zones, walk recap, walk history). Check against `2026-10-13-progression-beta-design.md` before planning._

## Goal

Split the one "home" into two ideas:

- **Camps** are real places the player plays from (home, a parent's house). Arriving at a camp pauses the game the way home
  does today.
- **Origins** are one point per zone. Each quest's difficulty is measured from its zone's origin, scaled to the size of that
  zone.

Difficulty stops being a fixed distance from one home. It becomes "how far out from this zone's origin, given how big the zone
is". A zone far from home, or a small park, gets a sensible spread of near and far quests.

## Decisions (owner, 2026-10-15)

| Decision | Why |
| -- | -- |
| Name **Camp**, with a campfire icon | Fits the islands theme; plain enough to read at a glance |
| Camps and origins are **separate** concepts; an origin may sit on a camp or anywhere else | Maximum control: a trailhead parking spot can be an origin without being a camp |
| **Every camp banks** carried items | A camp is where you put things down |
| **No caps or policing**: any number of camps, placed anywhere | Not our job to police play; explain the effect of a choice, never block it |
| Home becomes **camp #1** | One concept, no special case |
| Difficulty is a **share of the zone's reach from its origin** | Zone size decides the scale; "far" means the far side of this zone |
| Bands **near 0-30 %, middle 30-60 %, far 60-90 %** of reach; each tier's share is an average, drawn within ±10 % | The farthest quests stay inside the zone's outer streets; quests of one tier vary a little |
| **No length cap**: the zone the player draws decides how far quests go | The player defines the play zone; we show the result ("far side is about a 2 h walk"), never trim it |
| `minutes_per_tier` is **removed** (YAML and slot_data) | Zone size now sets every distance |
| Carried items bank **at camps** (shops may bank later) | Round trips for foragers and fetch quests end at the nearest camp |
| One circle: the camp circle (**50 m**) is also the time-away circle | Today's separate 100 m `HOME_RADIUS_M` and 50 m `HOME_CIRCLE_M` merge into one |

## Concepts

### Camp (on the device)

- A camp is a named point, with an optional Wi-Fi network for detection. It belongs to the device, not to a game, and every game
  uses it.
- Arriving at a camp does what arriving home does today:
  - counting goes off;
  - time away stops;
  - carried items bank;
  - the walk closes, and the recap reads "Welcome back".
- Leaving a camp starts counting the way leaving home does.
- A camp never counts progress, since being at a camp is a pause. So extra camps cannot be farmed.
- Camps can be added, moved and removed at any time.
- Detection works like home today, for each camp:
  - A circle of `HOME_CIRCLE_M` (50 m) around the point.
  - Optionally that camp's Wi-Fi.
  - GPS is off at a camp and is asked for only on an event (a burst of steps, or the camp's Wi-Fi dropping).

### Origin (in the game, one per zone)

- The point a zone's quests are measured from. It is chosen when the game is made and saved with the game.
- It defaults to a camp inside the zone (the nearest to the zone's centre when there are several). With no camp inside, it
  defaults to the street point nearest the zone's centre.
- The player can move it before starting. It snaps to the nearest walkable street inside the zone.
- It is **locked once the game starts**, because quests are placed from it. Moving it later would need a reroll of that zone's
  quests (out of scope).
- An origin has no presence effect: standing on it neither pauses counting nor banks, unless it is also a camp.

## Zone-sized difficulty

Effort stays in minutes inside the core (dwell time, trail length and run speed all fit one scale). The zone sets how minutes
turn into distance.

- **Reach** of a zone is the 90th percentile of street distance from the origin to the zone's street points. A percentile is
  used because one long arm of the zone, or one far street, would otherwise stretch every tier.
- **No cap:** a huge zone makes long quests. The new-game screen shows how long the far side is on foot, so the player can
  draw a smaller zone if they want shorter quests.
- **Tier to distance:** each band spreads its tiers evenly to its top edge, as a share of reach:

  | Band | Tiers | Share of reach |
  | -- | -- | -- |
  | Near | 1, 2, 3 | 10, 20, 30 % |
  | Middle | 4, 5, 6, 7 | 37.5, 45, 52.5, 60 % |
  | Far | 8, 9, 10 | 70, 80, 90 % |

  Each share is an **average**: a quest's target is drawn within ±10 % of it (relative: tier 10 lands in 81-99 % of reach), so
  quests of one tier vary a little and far quests stay inside the zone's outer streets.

- **Effort of a tier:** the walking time to its share of reach (reach in minutes at the zone's walking speed x the share).
  - Quests that are not just a walk spend the same effort differently. A dwell splits it between the walk out and the stay; a
    trail or courier spreads it over its legs.
  - So a tier means the same amount of play in every quest type.
- **Small zones are short games, by the player's choice.** A park that reaches 0.3 mi makes every quest short, dwells
  included. Nothing stretches them. The new-game screen says so ("Lehi Park is small: even far quests are about a 6 min
  walk").
- **Round trips:** foragers and fetch quests count the walk back to the nearest camp. Plain reach quests stay one way.

## What changes, by layer

### Core

| Area | Change |
| -- | -- |
| `realm.rs` (store) | `home.json` becomes `camps.json`, a list of `{id, name, point, wifi}`. On first load, `home.json` migrates to camp #1 "Home" |
| `home_watch.rs` | Watches every camp's circle and Wi-Fi. The state says which camp (`AtCamp(id)`, `Checking`, `Away`) |
| `game.rs` | `home` becomes `origins: BTreeMap<zone, Point>`. `HOME_RADIUS_M` (100 m) goes: time away and `bank_at_home` use "within 50 m of any camp" (the camp circle). Rename `bank_at_home` to `bank_at_camp` |
| `assign.rs` | Each zone's `p.home` becomes its origin. Want-to-distance goes through the zone's reach |
| reach | New: reach of a zone from its origin over the zone's street graph (built with the streets) |
| recap | "Welcome back" (the camp's name optional). Walk history is unchanged |

There is no game-save migration. This follows the beta's full-cut policy: every game is a developer test game, and the slot_data
schema bump rejects old games anyway. The device's `home.json` does migrate to camp #1, because it is the player's geography,
not a game.

### Apworld and Archipelago

- The apworld still assigns tiers only, and the core maps them onto each zone. So Archipelago logic is unchanged.
- `minutes_per_tier` is **removed** from the YAML options and slot_data (a schema bump). Coordinate with the progression-beta
  roadmap (schema 5 today).

### Android

- **Camps screen** (from Settings or the realm map):
  - campfire pins;
  - add, rename, move and delete;
  - pick the camp's Wi-Fi, reusing the home Wi-Fi flow.
- **New game, Play zones:**
  - each zone shows its origin pin with a "Move origin" action;
  - the zone's reach, for example "far side about 1.8 mi from the origin";
  - education hints, never blocks.
- **Presence text:** "At Mom's camp: paused" instead of "At home: paused".
- **Quests section of the new-game screen:**
  - "How far quests reach" goes; the zone decides.
  - "Near or far" stays as the mix, and its bands show real distances per zone (30 / 60 / 90 % of reach).

## Out of scope

- Moving an origin after the game starts (it would need a zone reroll).
- Camps shared between devices or players.
- Shop or tool effects at camps (progression beta).

## Testing

- **Core:**
  - camp migration from `home.json`;
  - arriving at any camp pauses counting and banks;
  - the walk recap closes at a second camp;
  - origins default to a camp in the zone, else the street nearest the centre;
  - reach uses the percentile, with no cap;
  - tiers follow the band table (tier 3 at 30 %, tier 7 at 60 %, tier 10 at 90 % of reach);
  - tiers scale with zone size: a small and a big zone get different distances for the same tier;
  - a dwell splits its tier's effort between walk and stay;
  - a small zone makes short quests (no stretching).
- **Engine:**
  - camps persist;
  - `HomeWatch` with two camps (enter one, leave, enter the other);
  - a game started from a camp outside home.
- **Android:**
  - camps list round trip;
  - the origin pin and the hint text.

## Open questions

None. The four from the first draft were answered by the owner on 2026-10-15 (see Decisions).
