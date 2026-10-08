# Context Doc: Economy, Eggs and Buddies (progression direction)

_Last verified: 2026-10-08_

## Status

Committed **direction** from the 2026-10-08 owner brainstorm. Not specced, not built. Core gameplay (quests, zones,
checks) comes first; each system below gets its own spec, plan and issue later. Supersedes the Bike/Car parts of
`progression-zones-and-tools.md`.

## What This Is

How a player progresses beyond AP checks: money, a backpack that is banked at home, a shop, eggs that hatch with
steps, buddies that give buffs, and the treehouse that shows it all. Theme is a guide for the mechanics only; anything
can be reskinned later.

## Pillar: "The Walking Randomizer"

- Every system turns **steps into progress**.
- The game is the game; AP support is a layer. A check goes out **when it is revealed** (banked or hatched), not when it
  is found. Stalling other players is accepted.
- Modes are **walk and run only**. Car and Bike are purged.
- Two layers: **AP items gate WHERE** (Zone Keys, Trail Pass, Running Shoes, Progressive Boots, Goal Eggs);
  **the local economy gates HOW WELL** (bag, vision, incubators, bank places, money tier). Local upgrades never decide
  whether something is reachable, so AP logic stays clean.

## Trip Loop (ASCII; Mermaid does not render for the owner)

```text
HOME (bank) -> walk/run out -> caches, quests, eggs -> walk home -> BANK -> REVEAL -> SHOP -> next trip
                                  |            |                      |
                               BACKPACK    INCUBATORS          checks go out,
                               (sealed)    (step count)        money comes in
```

## Systems

### Walk vs run

| | Walk | Run |
| -- | -- | -- |
| Feel | no rush, explore | urgent, strict time limit, fast movement |
| Proof | GPS track + Activity Recognition `WALKING` | sustained GPS speed band + `RUNNING` + time limit |
| Logic | default | location group `Run`; YAML `run_quests` on/off; excludable via `exclude_locations: [Run]` |
| Gate | open | AP item `Running Shoes` |

### Money (3 tiers, gated by upgrades, never by place)

- Caches spawn anywhere (no POI-type dependence: players have no guarantees about what is near home).
- Each cache rolls its tier from the player's **License** level: T1 only, then T1+T2, then T1+T2+T3.
- License comes from the shop (paid in the tier below) or as AP item `Progressive License`.
- Working names: Bottle Caps / Marbles / Star Shards.
- Caches are **not** AP locations; they respawn (per tile per day, reuse the global tile grid).
- AP filler can be money bundles (`Coin Purse`), so filler is never junk.

### Backpack and bank

- Everything found is **sealed** in the backpack: AP checks, money, crates, eggs.
- Banking = dwell at Home or at a bought **bank place**. Banking reveals everything and sends the checks.
- **Bag limit is soft.** Carrying over the limit gives a growing **step penalty** (counted steps shrink, e.g. -5% per
  item over, floor 50%), like carry weight in RPGs. Bag size is an upgrade.
- Haul persisted in SQLite; a dead phone never loses it.

### Shop

- **Shop Slots** are AP locations (`Shop Slot #n`, rising prices; YAML `shop_slots`, `shop_price_scale`).
- Local upgrades: bag size, vision radius, incubator slots, bank places, next License tier, quest QoL (reroll, shorter
  dwell), cosmetics (monetization-safe, no pay-to-win).
- **Tabled:** defense items (trap insurance, shields).

### Vision (fog of war)

Reveal radius around the player; upgrade widens it. Golden caches only spawn in unrevealed fog.

### Eggs (replace the Letter macguffins)

| | Goal Egg (AP item) | Found Egg (world drop) |
| -- | -- | -- |
| Source | multiworld, replaces `Letter X` | rare cache drop |
| Hatch | steps in an incubator slot | steps in an incubator slot (Easy/Med/Hard = e.g. 2k/5k/10k) |
| On hatch | counts toward the goal | sends AP check `Egg #n`, pays money |
| YAML | `goal_egg_count`, `goal_egg_steps` | `found_egg_count` |

- Only steps classified as walking/running count.
- Incubator slots are limited (upgrade), so eggs queue.
- Goal "hatch M of N Goal Eggs" replaces "collect all letters"; logic sees eggs like letters (hatching is client-side,
  steps are always possible, so beatability holds). Optional finale: **Legendary Egg** (big step count, boss-style).
- **Progressive Boots** (AP item): each level multiplies counted steps slightly (x1.1, x1.2, ...).

### Buddies

- Every hatched egg becomes a **buddy**.
- **One active buddy** travels with you and gives one buff; it levels up with distance walked together.
- Idle buddies stay at the treehouse and **bring gifts** while you are out (feeds the coming-home rush).
- Buff ideas: Magnet (pickup radius), Scout (vision), Packrat (bag), Nester (hatch speed), Lucky (higher money tier
  rolls), Sprinter (run-quest time), Forager (cache spawns).
- Local only; buffs change ease, never reachability.

### Treehouse (what you build towards)

Home base grows visibly with each upgrade (bag hook, incubator shelf, map wall, observatory). Win conditions stay
client-evaluated (any / all / at least N): Legendary Egg (default), Town Hero (N character quest arcs), Treehouse
Complete, Egg Dex (N distinct egg types).

### Theme guide (reskinnable)

Young teen helping townsfolk. Fetch quests are **skins over provable quest kinds** with recurring characters and silly
reasons (e.g. "Grandma lost her dentures, last seen north-ish" = cardinal quest). Templates live in the one help-text
file.

## Failed Approaches / Dead Ends

- **Money tier by place type** (street/park/trail): rejected; nothing is guaranteed near a player's home.
- **Car / Bike modes**: purged; walk and run only.
- **Mermaid diagrams** in chat: do not render for the owner; use ASCII.
- **Defense items**: tabled, owner does not love them.

## Gotchas

- Shop-bought upgrades must never gate AP logic, or fill can produce unbeatable seeds; only AP items gate regions.
- Changing Letters to Goal Eggs is a slot_data schema bump and rewrites `test_goal_needs_all_letters` etc. (TDD:
  new egg tests first).
- Run quests need an accessibility escape: location group + YAML toggle.

## References

`docs/context/progression-zones-and-tools.md` (zones, trails, traps), `docs/context/archipelago-game-model.md` (goals,
letters), `docs/context/quest-types-and-phone-apis.md` (provable signals), `docs/context/project-decisions.md`.
