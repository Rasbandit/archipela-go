# Progression Beta: Design and Roadmap

_Date: 2026-10-13. Status: umbrella design from the owner brainstorm; each sub-project below gets its own spec, plan and PRs._

Visual map (draft, private): <https://claude.ai/artifact/CAdw1dbVcjErFnvRGqfJSr>

## Goal

Make progression come from **items and quests**, not zones. Five solid tools beat twenty thin ones. If the beta ships
everything in "In scope", the game has a real Archipelago sphere structure, tangible rewards and a shop.

## What changes

| Before | After |
| -- | -- |
| Modes walk, run, bike, drive | **walk and run only** (Bike and Car removed) |
| Zones are the only lock in logic | **Badges on quests** are the lock; one zone by default, zone code kept dormant for later |
| 12 goals, Letters as the macguffin | **Treasure Map, Toolmaster and Completionist**; Letters become Map Pieces |
| Filler `Hydrate!`, `Take a Breather!` | **Resource bundles** (every reward is usable in game) |
| One flat sphere in a one-zone game | **A pyramid**: wide sphere 1, one-badge slices, two-badge quests, goal |

## Rules

1. **Only permanent tools are in logic.** Progression items are tools. Speed-ups, resources and shop goods never decide
   what is reachable.
2. **A badge is a lock on a normal quest.** A badged quest is the same reach, dwell, landmark, courier... quest with a small
   tool icon attached. Without the tool the reward cannot be claimed. A quest can carry two badges (it needs both).
3. **Difficulty is not a gate.** Easy, medium and hard quests appear at every level of the pyramid.
4. **Every reward is tangible.** Filler is a bundle of resources.
5. **Rewards first, picking up needs the tool.** Any resource can arrive as a quest reward at any time; picking one up off
   the map needs that resource's tool (litter needs none).
6. **One resource, one tool.** The same table decides a forager quest's badge and whether a map pickup is allowed.
7. **Only fixed, sure things.** No content that depends on water, postboxes or other places many areas lack.
8. **Never pull the player home early.** Fetch quests return to the quest giver; banking is when the player chooses.
9. **The apworld decides, the core checks.** The apworld writes each quest's `needs` into slot_data; the core only tests
   `needs ⊆ inventory` and draws the badges. Kotlin draws icons only.

## The pyramid

```text
                    GOAL                         Treasure Map / Toolmaster / Completionist
            two badges   (~10%)                  e.g. Dowsing + Satchel: dig up the parcel, return it
       one badge, any order   (~35%)             Gloves · Satchel · Dowsing Rod · Magnet · Trowel (+ Running Shoes)
  open from the start: sphere 1   (~55%)         reach · dwell · landmark · park · trail · explore · steps · away
                                                 · courier · fetch & return · litter forager · Regulars
```

Shares are draft defaults (YAML `badge_shares`). A wide sphere 1 avoids `FillError` (see `apworld-pitfalls.md`).

## Tools (progression)

| Tool | Badge on | Own quest shape | Picks up |
| -- | -- | -- | -- |
| Gloves | some base quests; pinecone foragers; a **courier variant** that needs gloves (handle-with-care parcel) | Forager (pinecones), gloved courier | pinecones |
| Satchel | **some** courier and fetch & return quests (not all) | none of its own (badge only) | (none) |
| Dowsing Rod | some base quests | **Hot & Cold**: one hidden point, warmer/colder hints only | (none) |
| Magnet | some base quests; bottle-cap foragers | **Lost & Found**: sweep a shaded area until it pings within ~20 m | bottle caps |
| Trowel | some base quests; seed foragers | **Plant & Harvest**: plant at X, harvest after >= 24 h | seeds |
| Running Shoes (opt-in) | run variants, only if YAML `run_quests` is on | (pace rule, not a shape) | (none) |
| Map Pieces | (none) | Treasure Map goal only (replaces Letters) | (none) |

## Resources

| Resource | Tool to pick up | Comes from |
| -- | -- | -- |
| litter | none (good deed: "clean up the park") | map, rewards |
| pinecones | Gloves | map, rewards |
| bottle caps | Magnet | map, rewards |
| seeds | Trowel | map, rewards; planted by Plant & Harvest |

Forager is one quest shape with a variant per resource; the variant's badge is its resource's tool.
Resources are spent in the shop. They respawn and spawn anywhere (reusing forager top-ups), so logic never depends on luck.

## Quest shapes in the beta

| Shape | Status | Proof |
| -- | -- | -- |
| Base kinds (reach, dwell, landmark, park, trail, explore, steps, away) | built | as today |
| Courier: accept at A, deliver at B | in progress (#183) | geofences |
| Fetch & Return: collect at B, back to the giver at A | new | geofence B, then A |
| Forager (per resource) | built for one generic item; needs variants | pickup radius |
| Hot & Cold | new | GPS distance to a hidden point |
| Lost & Found | new | coverage cells (as Cartographer) |
| Plant & Harvest | new | two visits with a time gap |
| Regulars at the player's pins | new | N visits with a time gap; the last visit is the check |

**Return-visit engine:** one core mechanism ("visit X, come back after a gap") serves Regulars and Plant & Harvest.

**Regulars:** the player drops pins next to the realm for places they already go (gym, grocery). "Visit 3 times, at
least 20 h apart"; only the last visit is an Archipelago location. Archipelago logic has no notion of time, so a YAML
switch can keep progression out of Regulars (via the `exclude_locations` group).

## Goals

Picked by YAML `goal_selection`; any / all / at least N still combine them (client-evaluated, `StatusUpdate`).

**Why tools sit in goal rules.** A macguffin-only goal (`has all Letters`) lets the fill put every piece in sphere 1 of any
world, so nothing forces a player up the pyramid. Archipelago has no setting for depth; the world's goal access rule
provides it (as Ganon's door needs the Master Sword). Every beta goal therefore needs tools in logic.

| Goal | Win when | Logic rule on the goal | Built today |
| -- | -- | -- | -- |
| **Treasure Map** | Collect `map_pieces_needed` of `map_pieces_total` Map Pieces (e.g. 12 of 18); the map reveals "X marks the spot"; finish the dig quest there | pieces >= needed AND Dowsing Rod AND Trowel | mostly: today's Treasure Hunt (letters A-P-G-O + Boss Quest); rename Letters to Map Pieces, add N of M and the tool rule |
| **Toolmaster** | Finish one quest carrying each tool's badge | has every enabled tool | no; counts badges like Quest-dex counts kinds |
| **Completionist** | Finish every quest | every badged quest needs its tools, so it climbs the pyramid by itself | yes (`all_trips`) |

Spare pieces (N of M) keep one stuck partner from blocking the win. Removed goals: Letter Hunt (both lengths), The Big One,
Zone Conqueror, Well Rounded, Quest-dex, Marathon, Explorer, Daily Habit, Boss Rush.

## Shop (in the beta)

Spend resources. Contents and whether purchases are Archipelago checks (`Shop Slot #n`, see #56) are decided in the shop
spec. Constraint: nothing bought in the shop is ever required by logic.

## Speed-ups (useful, never in logic)

| Item | Effect | Source |
| -- | -- | -- |
| Progressive Comfy Shoes | +0.1x counted steps each (draft: 5 copies) | AP (#60, renamed from Progressive Boots) |
| Bigger Backpack | more slots before the step penalty | AP or shop |
| Scouting Distance | fog reveals quests from farther away | AP (built) |

All step bonuses add to one multiplier capped at 2.0x.

## Roadmap

GitHub: tracking issue #192, steps #184-#191 (milestone "Progression beta"); parked ideas carry the `tabled` label.
Each step is its own spec, plan and PRs. Order: 1, then 2, then (3, 4) and (5, 6) in parallel lanes, then 7 and 8 (the goals need the tools).

| # | Sub-project | Scope | Depends on | Existing issues |
| -- | -- | -- | -- | -- |
| 1 | Cleanup | Remove Bike/Car (modes, items, catalog, UI); one zone by default; goals down to Treasure Hunt + Completionist; remove `Hydrate!`/`Take a Breather!` (temporary filler: litter bundle) | none | |
| 2 | Badge system | slot_data `needs` per quest (schema 4), badge location groups, core gate, badge on card and pin; Running Shoes as the first badge; YAML `run_quests`, `badge_shares` | 1 | #53, #96 |
| 3 | Resources | one stash of 4 resources, resource bundles as filler, tool-gated map pickups, forager variants | 2 | #55 |
| 4 | Shop | spend resources; contents; AP Shop Slots decision | 3 | #56 |
| 5 | Quest shapes | one per PR: Gloves (forager + gloved courier), Satchel (courier + fetch & return), Dowsing Rod (Hot & Cold), Magnet (Lost & Found) | 2 (and #183) | #183 |
| 6 | Return visits | the engine, then Regulars at pins, then Trowel (Plant & Harvest) | 2 | |
| 7 | Speed-ups | Progressive Comfy Shoes multiplier (cap 2x), Bigger Backpack | 3 | #60 |
| 8 | Goals | Treasure Map (Letters to Map Pieces, N of M, tool rule on the dig), Toolmaster | 5, 6 | #58 |

## Tabled (bring in later)

| Idea | Why it waits | Existing issue |
| -- | -- | -- |
| Camera / Snapshot quests (in-app shutter + GPS fix, opt-in real photo) | Honor mode alone is thin; reopens the "no camera" decision | |
| Critter Box and critters (buddies with buffs) | Needs a buff system designed first | #61 |
| Shop chains (Gloves -> pinecones -> Bird Feeder -> birdwatch quests) | After the shop exists | |
| Time-window quests ("walk 20 min before noon", opt-in) | Small, any time | |
| Multiple zones | Kept in code; most players will use one area | |
| Traps (all, incl. Freeze) and trap counters (Thermos, Antidote) | Owner is avoiding traps for now | |
| Compass | Current "go east" quests are not fun; needs a new use | |
| Stamps / postbox Mail Run | OSM postbox coverage varies | |
| Binoculars (collection distance) | Unclear value | |
| Money tiers and License | Replaced by resources for the beta | #54 |
| Vision radius and golden caches | After the beta | #57 |
| Eggs (Found Eggs) and Treehouse | After the beta; Goal Eggs are replaced by Map Pieces | #59, #62 |
| The 9 removed goals (Marathon, Explorer, Quest-dex...) | Trimmed for focus; the code is deleted, the ideas stay here | #102, #113 |

## Open questions (for the sub-specs)

1. Shop contents and whether purchases are AP checks (spec 4).
2. Badge shares 55 / 35 / 10: right default? (spec 2)
3. How many copies of each speed-up and the exact multiplier steps (spec 7).
4. What the gloved courier variant is called and how it differs in play (spec 5).

## Supersedes

- `docs/context/progression-zones-and-tools.md`: zones as the main lock, Bike/Car tools.
- `docs/context/economy-eggs-buddies.md`: money tiers, License, eggs for the beta (tabled, not dropped).
