# Context Doc: Archipelago Network Protocol

_Last verified: 2026-10-07_

## Status

Working. Stable protocol = 0.6.x (latest release 0.6.8, published 2026-10-04). 0.7.0 is NOT released (open GitHub milestone, no date; known 0.7.0 change: all worlds need an `archipelago.json` manifest). Treat 0.7.0 protocol as unknown.

## What This Is

Archipelago clients talk to the multiworld server over a WebSocket carrying JSON. Our app is a "custom client": it sends location checks (real-world map points) and receives items.

## Environment

- Source of truth: `docs/network protocol.md` in github.com/ArchipelagoMW/Archipelago (read at commit 9b64e83, 0.6.8 era).
  <https://github.com/ArchipelagoMW/Archipelago/blob/main/docs/network%20protocol.md>
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

## Our client's compliance (`just test-ap`)

`core/ffi/tests/ap_compliance.rs` runs our `ApSession` against the real pinned server (`.ap/MultiServer.py`, our apworld, slot `Tester`,
its own `.ap/compliance` on :38291), one test per rule; the pre-push hook runs it when AP client code, `scripts/ap_host.sh` or
`.ap-version` change. Verified 2026-10-12 on 0.6.8, all passing:

| Rule (source) | How we meet it | Test |
| --- | --- | --- |
| Handle ws and wss (client guide) | `archipelago_rs` tries `wss://` then `ws://` for a bare address, like `CommonClient` | `an_address_without_a_scheme_falls_back_to_ws_and_connects` |
| Send missed checks on connection (client guide) | every (re)connect sends every finished check in one `LocationChecks` and the goal (`Engine::server_report`) | `checks_reach_the_server_and_sending_them_again_is_harmless` |
| Keep an item index, resync on a gap (protocol) | `archipelago_rs` sends `Sync` when `index` skips; `index 0` replaces | `received_items_carry_a_gapless_index` |
| Goal `StatusUpdate` (client guide) | `ApSession::send_goal`, re-sent on connect | `the_goal_status_update_is_accepted` |
| Reconnect if lost while playing (client guide) | reconnect after `disconnected` (5 s doubling like `CommonClient`, capped 5 min), on foreground when stale, after a silent wait | `a_server_restart_drops_the_session_and_a_new_one_gets_every_item_again` |
| A refused first connect is not a loop | auto-reconnect only after a session was connected | `a_wrong_slot_is_refused_and_never_connected` |
| Server pings every 20 s (websockets defaults; `CommonClient` sends none) | the app sleeps in the socket; 60 s silence = dead | `the_server_pings_a_quiet_connection_within_twenty_five_seconds` |
| Servers that do not ping | a `Get` probe before reconnecting | `a_probe_is_answered` |

Only the pre-push hook runs the suite (`ci.yml` maps the py, rust and android checks, not `test-ap`); `just test-ap` holds a lock, so
parallel worktrees take turns on :38291. A game stores its server's `seed/slot` (`ApSession::server_identity`) and exchanges checks,
goal and items only with that server (`Engine::ap_server_match`); games from before keep live sends but get no automatic re-send. An
auto-reconnect that keeps being refused (the seed was regenerated, the slot is gone) retries every 5 min until the player connects.

Not covered by a test: changing the port of a saved connection (UI), and per-message compression. The protocol doc says
uncompressed connections are deprecated and may stop working; `tungstenite` has no permessage-deflate, so watch the server's
release notes. The app also does not reconnect on its own after an app restart (connect from New Game).

## References

- Protocol doc URL above; 0.6.8 release: <https://github.com/ArchipelagoMW/Archipelago/releases/tag/0.6.8>
- 0.7.0 milestone tracking issue (per search, unverified): <https://github.com/ArchipelagoMW/Archipelago/issues/6006>
- Related: `docs/context/archipelago-concepts.md`, `docs/context/archipelago-client-libraries.md`
