# Context Doc: How Archipela-Go 2 Maps Onto Archipelago

_Last verified: 2026-10-07_

## Status
Working mental model, matches the shipped apworld + Android spike. Read this before designing new quest types.

## The Core Idea
In Zelda, a location is a fixed place ("Deku Tree chest") with its rules baked into the world. In our game a location is an **abstract slot**
("Trip #17"). Its attributes (distance tier, key area, mode, later: quest type) are decided at seed-generation time. The phone binds each slot
to a real-world place or activity at play time. The server never knows geography.

## Who Decides What
| Step | Who | Knows geography? | Decides |
|--|--|--|--|
| 1. YAML | player | n/a | constraints: trip count, distance range, modes, locks, trap rate, (later) quest types and difficulty. NOT a list of quests |
| 2. Generation | apworld (Python) | no | the slots (count, tier, key area, type), our item pool (keys, letters, traps, fillers), access rules (what each slot needs) |
| 3. Multiworld fill | Archipelago core | no | which items (ours or other games') sit in which slots. The player does not choose |
| 4. Play | phone app (Rust core + Kotlin) | yes | a concrete place/activity per slot inside the player's zone; verifies it; sends `LocationChecks` |

## Refinements Worth Remembering
1. The YAML shapes only OUR item pool. What lands in our slots is decided by the multiworld fill (mostly other players' items; that is how our walking "contributes to the group").
2. The rando reasons only about what it knows: keys, distance reductions, quest type. It cannot know a tier-10 trip needs a real far place. So the client must either realize each slot or say it cannot (today: `in_band=false`). Never place silently wrong.
3. The real place is not part of a check's identity. Rerolling a place changes nothing on the server (safe to reroll/ban).
4. A single `type` field per slot (`reach_point` today) is the extension hook. New quest types = new `type` values + `params`, no contract rewrite (clients ignore unknown types).
5. Generation happens BEFORE the player grants phone permissions, so the YAML must declare intended capabilities and each slot should carry a fallback (see `quest-types-and-phone-apis.md`).

## Flow In One Picture
`YAML -> apworld generates N slots + item pool -> Archipelago places items across all games -> slot_data (slots with type/tier/key) -> phone fills the zone (core::fill + sampler) -> player acts -> geofence/sensors verify -> LocationChecks -> items arrive (keys unlock areas)`

## Reference Worlds (already on disk, git-ignored)
`.ap/worlds/` is a clone of Archipelago 0.6.8 (see `just setup-ap`); `.ap/` is in `.gitignore`. Zelda examples:
- `tloz/` (original Zelda): smallest, ~160-line `Rules.py`, `Locations.py`; best first read.
- `alttp/` (A Link to the Past): `Regions.py`, `Rules.py` (1788 lines), `Items.py`: classic fixed locations with access rules, progressive items.
- `oot/` (Ocarina of Time): `Rules.py`, `ItemPool.py`, `Hints.py`: large world, logic helpers.
- `tww/` (Wind Waker): newer style (`Macros.py`, `Rules.py`).
Contrast to note: those worlds hard-code every location and its rule; ours generates abstract slots whose meaning is bound client-side.

## References
`docs/context/archipela-go-game-design.md`, `docs/context/archipelago-concepts.md`, `apworld/docs/contract.md`, `docs/context/quest-types-and-phone-apis.md`


## Several win conditions (apworld 0.3.0, slot_data schema 3)
Follows Archipelago's own pattern (the Satisfactory world): `goal_selection` is an `OptionSet` of goal names, `goal_requirement` a `Choice`
(`require_any_one_goal`, `require_all_goals`, `require_at_least_n_goals` + `goals_required`), one named `Range` per counting goal, and an `OptionGroup`
"Goal Selection". The apworld never tells the server a player won: the client does, with `StatusUpdate` (`CLIENT_GOAL`) once `goal_need` goals are done
(the generator's `completion_condition` only proves a seed is beatable). Letters are demanded by logic only when unavoidable (every selected goal needs
letters, or all goals are required); otherwise the pool still holds them but logic ignores them. slot_data v3 carries `goals[{id,target}]`,
`goal_requirement` and `goal_need`; the app also reads v2 (one `goal`). Verified with real `Generate.py` + `MultiServer` and the app's own reader
(`cargo run --example parse_slot -- file.json`).
