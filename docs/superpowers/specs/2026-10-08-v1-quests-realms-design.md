# Archipela-Go 2 v1: Realms, Quests, Zones, Tools, Difficulty (shared spec)

Date: 2026-10-08 · Status: approved by owner direction ("build your best v1 of everything we covered") · Supersedes apworld schema v1.
Read with: `docs/context/archipelago-game-model.md`, `docs/context/progression-zones-and-tools.md`, `docs/context/map-data-capabilities.md`, `docs/context/quest-types-and-phone-apis.md`.

## 1. Concepts

- **Realm**: a saved, user-drawn geofence (polygon or circle) with a name and a **mode tag**: `walk | run | bike | drive`. The mode decides which quests it may generate. Saved on the phone. When saved, the app **scans** the realm once (Overpass, cached) and stores what it offers (the **atlas**: counts + features per quest kind).
- **Quest catalog** (`core/data/quest_catalog.json`): the list of quest *kinds* the map data can support. Each has a clever name, a **family**, OSM filters, a verification method and allowed modes.
- **Family** (apworld-level quest `type`): `reach, dwell, landmark, trail, park, water, courier, explore, steps, away` (+ `boss`). The apworld picks a family per slot; the phone picks a concrete *kind* and place from the realm's atlas.
- **Zone**: one entry of the game's zone list (an Archipelago region). Zone *k* has a mode and is unlocked by items. The player assigns one saved realm to each zone when starting a game (matching mode).
- **Game Builder**: the app turns a chosen set of realms into a ready-to-submit player **YAML** (zone_modes etc.).
- **Effort tier**: difficulty = estimated active minutes. `tier = ceil(effort_min / minutes_per_tier)`, clamped 1-10. Bands: Easy 1-3, Medium 4-7, Hard 8-10.

## 2. YAML options (apworld v2). Game name unchanged: `Archipela-Go 2: Electric Boogaloo`

| Option | Type / default | Meaning |
| -- | -- | -- |
| `goal` | Choice: `macguffin_short` (default), `macguffin_long`, `all_trips`, `boss`, `treasure_hunt`, `zone_conqueror`, `well_rounded`, `quest_dex`, `marathon`, `explorer`, `streak`, `boss_rush` | see section 7. `boss` and `treasure_hunt` add one Hard "Boss Quest" in the last zone; `treasure_hunt` also needs the APGO letters |
| `goal_target` | Range 1-1000, default 0 meaning "use the goal's default" | parameter of the goal (percent, count, km, days...) |
| `number_of_trips` | Range 1-1000, default 100 | quests across all zones (boss extra) |
| `zone_modes` | OptionList of `walk,run,bike,drive`, 1-6 entries, default `["walk"]` | ordered zones; zone 1 is free |
| `easy_share`, `medium_share`, `hard_share` | Range 0-100, defaults 50 / 35 / 15 | difficulty mix (normalized; at least one > 0) |
| `minutes_per_tier` | Range 5-30, default 10 | effort minutes per tier |
| `minimum_distance` | Range 50-5000, default 150 | closest a place may be to home (m) |
| `quest_types` | OptionSet of families, default all | enabled families (`reach` is always enabled) |
| `enabled_traps` | OptionSet of `freeze, fog, shuffle, silence, leash, detour, toll, slow, honor`, default all | trap pool |
| `trap_rate` | Range 0-100, default 30 | percent of free item slots that become traps |
| `enable_effort_reductions`, `enable_scouting_distance_bonuses`, `enable_collection_distance_bonuses` | Toggle, off | optional useful items |
| `reduction_percent` | Range 1-25, default 8 | per Effort Reduction item |
| `fog_of_war`, `return_home`, `death_link` | Toggle, off | client behaviors |

Removed from v1: `allowed_modes`, `number_of_locks`, `maximum_distance`, `enable_distance_reductions`. Players may use the standard AP options `exclude_locations` / `priority_locations` with the location groups below.

## 3. Items, locations, logic

**Items** (separate ID namespace, offset `8_902_400_000_000`): `Progressive Zone Key` (progression, count = zones-1); tools (progression, one each, only for modes other than zone 1's mode that are used by zones >= 2): `Running Shoes` (run), `Bike` (bike), `Car` (drive); `Progressive Effort Reduction`, `Progressive Scouting Distance`, `Progressive Collection Distance` (useful, optional); letters `Letter A,R,C,H,I,P,E,L,G,O` (progression; short goal APGO, long ARCHIPELAGO); app traps `Fog Of War Trap, Shuffle Trap, Silence Trap, Freeze Trap, Leash Trap, Detour Trap, Toll Trap, Slow Trap`; honor traps (existing five); fillers `Hydrate!`, `Take a Breather!`.
**Locations** (static pool, block per difficulty x mode): names `"{Difficulty} {Mode} Quest #{n}"` with Difficulty in Easy/Medium/Hard, Mode in Walk/Run/Bike/Drive, n = 1..1000. ID = `ID_OFFSET + block*1000 + n` where `block = difficulty_index*4 + mode_index` (difficulty order Easy, Medium, Hard; mode order Walk, Run, Bike, Drive). `Boss Quest` has ID `ID_OFFSET + 12*1000 + 1`. A seed uses the first k names of each needed block. `location_name_groups`: `Easy, Medium, Hard, Walk, Run, Bike, Drive, Boss`.
**Regions/logic**: `Menu -> Zone 1` free. `Zone k -> Zone k+1` requires `Progressive Zone Key >= k` and, if zone k+1 needs a tool (mode differs from zone 1's), that tool. `Goal` event: macguffin goals need all letters; `all_trips`, `boss` need access to the last zone / Boss location; `treasure_hunt` needs letters AND the Boss location; every other goal (`zone_conqueror, well_rounded, quest_dex, marathon, explorer, streak, boss_rush`) is evaluated by the client and only requires access to the last zone in logic (the client sends the goal status when its condition is met). Effort reductions are **not** logic.
**Distribution**: trips split evenly across zones (remainder to the earliest zones). Within a zone, difficulty counts from the shares by largest remainder. Tier chosen uniformly inside the difficulty band. Quest family per slot: uniformly among enabled families compatible with the zone mode (`reach` weighted x3); if none, `reach`.
**Family x mode compatibility** (apworld constant): reach/dwell/landmark/courier/away: all modes; explore: walk, run, bike; trail: walk, run, bike; water: walk, run, bike; park: walk, run; steps: walk, run.
**Item pool** = letters + zone keys + tools + optional useful + traps (from free slots x `trap_rate`, among enabled traps incl. `honor` group) + filler = number of locations (trips + boss).
**Validation (OptionError)**: empty/invalid `zone_modes`; all shares 0; `number_of_trips` too small for mandatory items (letters + zone keys + tools); unknown quest type.

## 4. slot_data v2 (`schema_version` = 2; JSON Schema in `apworld/docs/slot_data.schema.json`)

```json
{ "schema_version": 2, "goal": "boss", "goal_target": 0, "minutes_per_tier": 10, "reduction_percent": 8, "min_distance_m": 150,
  "fog_of_war": false, "return_home": false, "death_link": false,
  "enabled_traps": ["freeze","fog"],
  "zones": [ {"id":1,"mode":"walk","zone_keys_needed":0,"tool":null},
             {"id":2,"mode":"bike","zone_keys_needed":1,"tool":"Bike"} ],
  "trips": [ {"location_id": 8902400000001, "zone":1, "mode":"walk", "difficulty":"easy", "effort_tier":2, "type":"reach"} ],
  "boss": {"location_id": 8902400012001, "zone":2, "mode":"bike", "difficulty":"hard", "effort_tier":10, "type":"boss"} }
```

`boss` is `null` unless goal is `boss`. Clients must refuse an unknown `schema_version` and ignore unknown `type`s.

## 5. Client v1 (Rust core + Kotlin)

**Core modules**: `catalog` (parse JSON, kinds), `realm` (model, persistence), `scan` (build catalog queries, parse features, offers/counts), `effort` (estimates, tier math), `assign` (slot -> kind + feature per mode/family/tier, with fallback to street/cell reach), `verify` (state machines: reach, dwell, dwell-in-area, courier, follow-line coverage, cover-cells, steps, away), `yaml` (Game Builder), `fog`, `traps` (freeze/fog/shuffle/leash/detour/toll/slow state), `game` (persisted game state per seed: assignments, progress, discovered, traps).
**App screens**: Realms (list, draw polygon/circle, name, mode tag, scan, offers summary with counts and clever names, surface preference, delete) · New Game (select realms, preview, export/copy YAML) · Play (connect, assign realms to zones, map with fog, quest list with clever names/difficulty/status, locked zones, trap banner, simulated GPS for testing).
**Settings**: surface preference (Any / Prefer paved / Paved only), avoid steps.
**Verification trust**: only provable signals (GPS track, step counter, Activity Recognition later). Dropped: NFC, photo, touch-grass-by-sensor, weather, social.
**Fog**: points hidden until within the reveal radius (default 150 m + Scouting items) or discovered earlier; saved.
**Out of v1**: barometer climb quests, Health Connect, Activity Recognition mode proof, foreground service/background location, team features.

## 6. Standalone (solo) mode: the app is playable without Archipelago

The Rust core contains a **solo generator** that mirrors the apworld: from the same options it produces the same `slot_data` shape (zones, quests with difficulty/effort/family, goal) plus a local **reward table** (which item sits at which quest). Rewards come from one of two backends behind the same engine API: `Solo` (local table) or `Archipelago` (server items). The engine only sees "item received" events (zone keys, tools, letters, traps, fillers, useful items), so every feature works identically. Solo fill: place progression items (zone keys, tools, goal letters) into locations reachable under the items placed so far (beatable by construction), then traps/filler/useful at random. A solo game is saved on the phone; no network needed except the realm scan. The Game Builder offers two buttons: "Play solo" and "Export YAML for Archipelago".

## 7. Win conditions (goals)

| Goal | Win when | `goal_target` default | Notes |
| -- | -- | -- | -- |
| `macguffin_short` / `macguffin_long` | all letters of APGO / ARCHIPELAGO received | - | classic |
| `all_trips` ("Completionist") | every quest done | - | |
| `boss` ("The Big One") | the Boss Quest is done | - | |
| `treasure_hunt` | APGO letters collected, then the Boss Quest ("the treasure") is done | - | letters reveal the treasure location |
| `zone_conqueror` | at least N% of the quests in every zone done | 60 (%) | |
| `well_rounded` | at least one quest of every enabled family done | - | |
| `quest_dex` | N distinct quest kinds completed | 15 | the "gotta catch 'em all" goal |
| `marathon` | N km of tracked distance during quests | 42 (km) | cumulative |
| `explorer` | N map cells (150 m) revealed/visited | 300 | pairs with fog |
| `streak` | N consecutive days with at least one quest done | 7 | |
| `boss_rush` | N Hard quests done | 5 | |

Archipelago logic cannot see client conditions, so for client-evaluated goals the apworld only requires access to the last zone; the client reports the goal when satisfied.
