# Context Doc: Archipelago Concepts

_Last verified: 2026-10-07_

## Status

Working reference. Archipelago stable = 0.6.8 (2026-10-04); 0.7.0 unreleased.

## What This Is

Domain model of Archipelago (archipelago.gg), a multiworld randomizer: items for many players' games are shuffled across all players' locations. Explains where a custom real-world-map client fits.

## Environment

- Generator/server are Python (`BaseClasses.py`, `Fill.py`, `MultiServer.py`). Clients can be any language that speaks WebSocket (see `archipelago-network-protocol.md`).
- Docs: `docs/world api.md`, `docs/adding games.md`, `docs/apworld specification.md` in github.com/ArchipelagoMW/Archipelago.

## Connection

N/A (conceptual). See protocol doc.

## Auth

N/A. Slot name (+ optional room password) identifies a player.

## Key Commands / Patterns

### Core model

- **Multiworld**: one seed containing N player _slots_, each playing a _game_. Items found in your world may belong to someone else's slot and vice versa. Teams (0-based) hold slots (1-based).
- **World / APWorld**: Python package defining one game's items, locations, regions, rules, options, and `fill_slot_data`. Lives in `worlds/<game>/`; distributed as `<game>.apworld` (zip, all-lowercase name, contains `<game>/__init__.py`). Metadata in `archipelago.json` (`game`, `minimum_ap_version`, `maximum_ap_version`, `world_version`, `authors`). Manifest becomes mandatory before 0.7.0. World classes are Python only; a custom game needs an APWorld to be generated and served.
- **Item**: `name`, `code` (id), classification. **Location**: `name`, `address` (id), lives in a Region, has access rule + `LocationProgressType` (DEFAULT/PRIORITY/EXCLUDED). IDs unique per game, 1..2^53-1 (recommended <= 2^31-1); names must not be purely numeric.
- **Region / Entrance / rule**: regions are logical containers (do not need physical meaning; can be abstract like tech trees); special origin region ("Menu") is always reachable. Entrances connect regions with rules. Rule = `lambda state: state.has("Sword", player)`. This is "logic": generation guarantees the seed is beatable under the rules (accessibility option: full/minimal etc.).
- **Item classification**: `progression` (may be required by logic; must be if any rule references it), `useful`, `filler`, `trap`, plus combos `skip_balancing`, `deprioritized`, and combined variants. Network flags only expose progression/useful/trap (0b001/010/100).
- **Events**: generation-only item+location pairs (id None); never seen by server. Goal is signalled via client `StatusUpdate` goal.
- **Completion condition**: per-player goal predicate set in the world.
- **Slot data**: world-supplied JSON (options the client must know), sent in `Connected`.
- **Item groups / location groups**: name groups usable in hints; readable via DataStorage specials.

### Player config

- **YAML**: one per slot; sets game, name, game options, plus global options (`accessibility`, `progression_balancing`, `local_items`, `non_local_items`, `start_inventory`, `start_hints`, `start_location_hints`, `exclude_locations`, `priority_locations`, `item_links`, `triggers`). Multiple docs via `---`. 0.6.8 adds `quantity` for repeating one YAML. Generated from the game's options page or launcher template. Parsed with safe loader (`Utils.parse_yaml`).
- Host uploads YAMLs -> generation -> output zip + multidata -> server (WebHost or self-hosted `MultiServer`).

### Runtime features

- **Hints**: server tracks per-player hint points. `hint_cost` is percentage of total locations; `location_check_points` earned per check. `!hint` costs points; `!hint_location` hints a location. `start_hints` / `start_location_hints` in YAML are free. Hint statuses: unspecified/no_priority/avoid/priority/found. `LocationScouts create_as_hint` makes free hints from seen-but-unchecked locations.
- **Release**: distributes all remaining items from your world to the others (permission `release`: auto, enabled, auto-enabled, disabled, goal). **Collect**: pulls the remaining items others hold for you (same permission modes). **Remaining**: `!remaining` lists unfound items (goal/enabled/disabled). Admin `/release <player>` forces it.
- **Co-op**: multiple clients on the same slot; other clients' checks arrive via `RoomUpdate.checked_locations`.
- **Item links, plando, starting inventory**: handled at generation; clients just see items (including from slot 0 "server" with arbitrary counts).
- **Trackers**: `Tracker` tag clients; Universal Tracker re-runs world logic from YAML + apworld (or slot_data for YAML-less) to show in-logic locations.

### How a custom client fits (this project)

- The client is the intermediary between "the game" and the server. Here "the game" = real-world map; each location = a map point; visiting it => `LocationChecks`. Received items => reward in-app (`ReceivedItems`); goal => `StatusUpdate` 30.
- A matching APWorld is required (Python): defines locations (ids/names for real-world points or abstract "checks"), items, regions/rules, options, `fill_slot_data` (e.g. coordinates or region seeds). Coordinates cannot live in ids; send via slot_data, or ship a fixed point table in the app keyed by location name, or scout/DataStorage.
- Hard requirements for any client (docs "adding games.md"): ws+wss, auto-reconnect, editable port, goal status update, send missed checks on connect, handle any item any number of times incl. slot-0 items, keep received-items index for resync.
- Using an existing game's slot with a generic client = possible, but then the app is a tracker/text client, not the game.

## Failed Approaches / Dead Ends

- None tried (research only). Open design question: whether to ship our own APWorld (needed for real-world locations) or reuse an existing "Manual" style world; unresearched. ManualForArchipelago exists (<https://github.com/ManualForArchipelago/Manual>) - unverified suitability.

## Gotchas

- Item/location ids and names are per-game; always look up via the player's game.
- `Menu` region is always reachable: logic assumes player can return to start at any time. For geo logic, model distance/travel as items or regions, not live position.
- Events do not signal goal; `StatusUpdate` does.
- Progression-classified items must be used in logic or balancing/fill behaves oddly; filler cannot be required.
- Uploading/hosting: archipelago.gg hosted rooms limited by WebHost; ports may change.
- Items can be received before a location is checked (other players send them); client must grant them regardless of map position.

## References

- <https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/world%20api.md>
- <https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/adding%20games.md>
- <https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/apworld%20specification.md>
- <https://archipelago.gg/tutorial/Archipelago/advanced_settings_en> (YAML options, via search - not directly read)
- <https://archipelago.gg/tutorial/Archipelago/commands_en> (commands, via search - not directly read)
- Related: `archipelago-network-protocol.md`, `archipelago-client-libraries.md`
