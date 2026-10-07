# Context Doc: Archipelago Network Protocol

_Last verified: 2026-10-07_

## Status
Working. Stable protocol = 0.6.x (latest release 0.6.8, published 2026-10-04). 0.7.0 is NOT released (open GitHub milestone, no date; known 0.7.0 change: all worlds need an `archipelago.json` manifest). Treat 0.7.0 protocol as unknown.

## What This Is
Archipelago clients talk to the multiworld server over a WebSocket carrying JSON. Our app is a "custom client": it sends location checks (real-world map points) and receives items.

## Environment
- Source of truth: `docs/network protocol.md` in github.com/ArchipelagoMW/Archipelago (read at commit 9b64e83, 0.6.8 era).
  https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md
- Server: `MultiServer.py` (Python). archipelago.gg hosted rooms require TLS (wss://); self-hosted may be plain ws://.

## Connection
- WebSocket to `wss://archipelago.gg:<port>` (port is per-room and can change; client must allow editing it).
- Each frame = JSON **list** of command objects, each with a `"cmd"` key. Per-message compression should be supported (uncompressed is deprecated).
- Handshake: connect -> server `RoomInfo` -> (optional) `GetDataPackage` -> `DataPackage` -> client `Connect` -> `Connected` | `ConnectionRefused` -> server `ReceivedItems` -> `PrintJSON` join notice. After a refusal the socket stays open; send a new `Connect`.
- Version objects need `"class":"Version"`: `{"major":0,"minor":6,"build":8,"class":"Version"}`.

## Auth
- No token. Slot `name` (+ optional room `password`) in `Connect`; `uuid` is a client-generated id.
- Refusal `errors`: `InvalidSlot`, `InvalidGame`, `IncompatibleVersion`, `InvalidPassword`, `InvalidItemsHandling`.

## Key Commands / Patterns
Server -> client: `RoomInfo`, `ConnectionRefused`, `Connected`, `ReceivedItems`, `LocationInfo`, `RoomUpdate`, `PrintJSON`, `DataPackage`, `Bounced`, `InvalidPacket`, `Retrieved`, `SetReply`.
Client -> server: `Connect`, `ConnectUpdate`, `Sync`, `LocationChecks`, `LocationScouts`, `CreateHints`, `UpdateHint`, `StatusUpdate`, `Say`, `GetDataPackage`, `Bounce`, `Get`, `Set`, `SetNotify`.

```json
[{"cmd":"Connect","password":"","game":"<GameName>","name":"<Slot>","uuid":"<uuid>",
  "version":{"major":0,"minor":6,"build":8,"class":"Version"},
  "items_handling":7,"tags":["AP"],"slot_data":true}]
```
- `Connected`: `team`, `slot`, `players[NetworkPlayer]`, `missing_locations`, `checked_locations`, `slot_data`, `slot_info{slot->NetworkSlot}`, `hint_points`.
- `RoomInfo`: `version`, `generator_version`, `tags`, `password`, `permissions{release,collect,remaining}`, `hint_cost` (% of locations), `location_check_points`, `games`, `datapackage_checksums`, `seed_name`, `time`.
- `LocationChecks {locations:[int]}`: duplicates are harmless; re-send on connect for anything done while offline.
- `LocationScouts {locations, create_as_hint:int}`: reply `LocationInfo` (items at those locations; `player` = receiver in that packet). `create_as_hint` non-zero creates a hint without spending points (2 = broadcast only new hints). Non-zero ALWAYS creates a persistent hint, even if already found.
- `ReceivedItems {index, items:[NetworkItem]}`: `NetworkItem = {item, location, player, flags}`. `index==0` means full inventory (replace). Non-matching `index` => resync via `Sync` then `LocationChecks`. Persist "last processed index" locally.
- `NetworkItem.flags` bits: 0b001 progression, 0b010 useful, 0b100 trap, 0 filler.
- `items_handling` bits: 0b001 items from other worlds, 0b010 own-world items (needs 001), 0b100 starting inventory (needs 001). 0 = never receive. Use 0b111 for a fully remote client with no ROM patch.
- `StatusUpdate {status}`: `ClientStatus` 0 unknown, 5 connected, 10 ready, 20 playing, **30 goal** (send to finish the game).
- `Say {text}`: chat or `!` commands (`!hint`, `!release`, `!collect`, `!remaining`, `!getitem`).
- `PrintJSON`: render `data:[JSONMessagePart]` (types: text, player_id, item_id, location_id, color, hint_status...). `type` may be ItemSend, Hint, Join, Part, Chat, Goal, Release, Collect, Countdown, etc.; unknown types still display `data`.
- Hints (0.6.x): `CreateHints {locations, player, status}`, `UpdateHint {player, location, status}`; `HintStatus` 0 unspecified, 10 no_priority, 20 avoid, 30 priority, 40 found (cannot be set/changed). Read hints via `Get` key `_read_hints_{team}_{slot}`.
- Data storage: `Get {keys}` -> `Retrieved`; `Set {key, default, want_reply, operations[{operation,value}]}`; `SetNotify {keys}` -> `SetReply {key,value,original_value,slot}`. Ops: replace, default, add, mul, pow, mod, floor, ceil, max, min, and, or, xor, left_shift, right_shift, remove, pop, update. Keys starting `_read_` are read-only specials (`hints_`, `slot_data_`, `item_name_groups_`, `location_name_groups_`, `client_status_`, `race_mode`).
- Bounce: `Bounce {teams, games, slots, tags, operator("or"|"and"|"legacy"), data}` -> `Bounced`. `and` treats a missing key as True, empty list as False.
- Tags: `AP`, `DeathLink`, `HintGame`, `Tracker`, `TextOnly`, `NoText` (skip chat to save bandwidth). Tracker/TextOnly/HintGame may send empty `game`.
- DeathLink = Bounce to tag `DeathLink` with data `{time, cause?, source}`. Optional; probably irrelevant for a map game but cheap to support.
- DataPackage: per game `item_name_to_id`, `location_name_to_id`, `checksum`. Cache on disk keyed by `datapackage_checksums`. Request single games via `GetDataPackage {games:[...]}`.
- IDs: world IDs 1..2^53-1 (safe in JS doubles; use i64 in Rust/Kotlin). IDs <= 0 reserved (e.g. location -1 Cheat Console, -2 Server). Names/IDs are only unique per game; resolve via the player's game in `slot_info`.
- `slot_data`: arbitrary JSON per slot, only if `Connect.slot_data=true`. Prefer `LocationScouts` over bloating slot_data.
- Team numbers start 0, slots start 1; slot 0 = server.

## Failed Approaches / Dead Ends
- None tried yet (research only, no code). Do not assume 0.7.0 behaviors.

## Gotchas
- Clients must: handle ws and wss, reconnect on drop, allow port change, send goal `StatusUpdate`, send missed checks on connect, handle items from slot 0 / any count / any order.
- Version in `Connect` must be `class:"Version"` shaped or the server cannot compare.
- Receiving an unrecognised packet field/type: ignore, do not crash (docs say types may be added).
- "Event" items/locations in worlds are generation-only; goal is signalled by `StatusUpdate`, never by an event.
- Server has been observed to send negative `hint_points` (undocumented; noted in archipelago_rs commit, 2026-04).
- `received json depth limited to 16` since 0.6.8 release notes (#6378): keep outgoing JSON shallow.

## References
- Protocol doc URL above; 0.6.8 release: https://github.com/ArchipelagoMW/Archipelago/releases/tag/0.6.8
- 0.7.0 milestone tracking issue (per search, unverified): https://github.com/ArchipelagoMW/Archipelago/issues/6006
- Related: `docs/context/archipelago-concepts.md`, `docs/context/archipelago-client-libraries.md`
