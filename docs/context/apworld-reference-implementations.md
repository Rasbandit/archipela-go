# Context Doc: APWorld Reference Implementations

_Last verified: 2026-10-07_

## Status
Working. Sizes/features below come from reading source (ArchipelagoMW tag 0.6.8 clone, `ManualForArchipelago/Manual`, `barretg/Taskipelago` clones). Licenses: the core repo is MIT (LICENSE, (c) 2017 LLCoolDave), so bundled worlds are MIT unless a world folder says otherwise (UNVERIFIED per world). Not copying code we have no license for.

## What This Is
Curated list of worlds worth reading when writing our apworld (count-based progression, many generic locations, traps/filler, real-world or tracker-style games), with what to copy and avoid. Map of the wiki: https://archipelago.miraheze.org/wiki/Category:Implementations has 951 pages; subcategories `Games created for Archipelago` (54), `Implementations with traps` (240), `Implementations with dedicated Universal Tracker support` (110), `Implementations that are broken on Archipelago Main` (10; avoid as models).

## Environment
Source paths are relative to https://github.com/ArchipelagoMW/Archipelago (worlds/). Python 3.12. See `apworld-development-guide.md`.

## The list (ordered by usefulness for us)
1. **APQuest** `worlds/apquest` (MIT, manifest min 0.6.7, author NewSoupVi). Official, heavily commented template. Copy: split into `world.py/regions.py/locations.py/rules.py/items.py/options.py/web_world.py`; `region.add_locations(dict, LocationClass)`; `region.add_event(...)` for Victory; `get_filler_item_name` mixing filler/trap by percent; item pool padded with `get_unfilled_locations - len(pool)`; `push_precollected`; rule builder (`Has`, `HasAll`, `OptionFilter`, `world.set_rule`); `OptionGroup` + `option_presets`; tests in `test/` with `bases.py` + `assertAccessDependency` + `subTest`. Avoid: its client/game code (not needed); rule builder requires core >= 0.6.7.
2. **Bumper Stickers** `worlds/bumpstik` (443 src lines, 2 test files, manifest min 0.6.4). Copy: count-based items (`Treasure Bumper` x32 gating `state.has("Treasure Bumper", p, n)` on 32 generic locations), traps via `_create_traps`, remaining slots padded with filler `Score Bonus`, `item_delta` pattern. Avoid: older style (`__init__(multiworld, player)`, `multiworld.get_location(...).access_rule =`); lambda default args are the correct late-binding fix.
3. **Muse Dash** `worlds/musedash` (~1.5k lines, 7 test files, min 0.6.3). Copy: hundreds of generic locations built in `generate_early` from a pool; `OptionError` when constraints cannot be met; progression "Music Sheet" count with a win count; weighted filler + `trap_count_percentage` + `chosen_traps` OptionSet capped by free slots; `item_name_groups` for Traps/Filler; tests `TestTrapOption`, `TestWorstCaseSettings`, plando tests. Avoid: its large data-collection class and plando handling (overkill).
4. **Yacht Dice** `worlds/yachtdice` (3.6k lines mostly tables; min none). Copy: location count derived from options in `generate_early`, completion via `state.has_all_counts`, cascade of filler/useful padding so generation never gets stuck. Avoid: its simulation/probability code.
5. **Paint** `worlds/paint` (347 lines, manifest, docs). Copy: tiny world with many generic percentage-based locations; custom `collect`/`remove` + `LogicMixin` to keep counts cheap. Avoid: no tests.
6. **ChecksFinder** `worlds/checksfinder` (134 lines, no manifest, no tests). Smallest official world; read for the minimum `WebWorld`/`Tutorial` wiring only (old style).
7. **Manual for Archipelago** https://github.com/ManualForArchipelago/Manual (MIT, (c) 2023 Manual for Archipelago). Data-driven world: JSON items/locations/regions + `hooks/`; "honor system" client, victory location. Copy: data-driven definitions and validation (`DataValidation.py`), docs folder structure. Avoid: using it as the real game (no automatic checking; our locations come from GPS), generated game names `Manual_<game>_<author>`.
8. **Taskipelago** https://github.com/barretg/Taskipelago (to-do list as checks; manifest min 0.6.7, 1.2.0; **no LICENSE file found**: read only, do not copy). Pattern: real-life user-defined locations from YAML, `build_apworld.py` (zip with top-level folder, required-file check, `--list`), tests/python that exercise `generate_early` headlessly with stubbed `BaseClasses` (fast, but stubs can drift from the real API). Avoid: 2.5k-line `__init__.py`, bundled web client in the apworld.
9. **Archipela-Go! (upstream, our predecessor)** https://github.com/aki665/react-native-archipelago (branch `archipela-go`; apworld only as release asset `apgo.apworld`; apworld has no license and no manifest; repo MIT). Read behavior only via our own docs `archipela-go-*.md`; never copy source.
10. **Universal Tracker** https://github.com/FarisTheAncient/Archipelago `worlds/tracker` (fork of AP with its own `archipelago.json`, min 0.6.2; license: fork LICENSE not opened, UNVERIFIED). Read `docs/apworld-integration.md` and `docs/re-gen-passthrough.md` for slot_data/regen hooks; example worlds with UT support in core: `worlds/tunic`, `worlds/witness/universal_tracker.py`, `worlds/yugioh06`, `worlds/stardew_valley` (seed via `re_gen_passthrough`).

## Tooling worth using
- https://github.com/Eijebong/ap-actions (MIT): `ap-tests` (runs only your world's tests), `fuzz`, `package-apworld`, `release-apworld.yml`. Takes `apworld-path: worlds/<name>`.
- https://github.com/Eijebong/Archipelago-fuzzer (random-YAML generation; hooks) and https://github.com/Eijebong/empty-apworld (filler world with 100 free locations, used to reduce restrictive-start failures in the fuzzer).
- https://github.com/Eijebong/Archipelago-index (archived; its criteria are a good quality bar) and https://github.com/silasary/apworlds (community index of .apworld JSON manifests).

## Failed Approaches / Dead Ends
- Wiki category pages show only 200 of 951 entries per page and carry no repo links; follow each game page to its GitHub. Perplexity answers on UT field names (`ut_map`, `tracker_world`) and 0.7.0 class-level `world_version` errors were NOT confirmed in source (`ut_map` does not exist in 0.6.8 worlds; `tracker_world` is a ClassVar only in tunic); use the UT docs.
- `search_repositories` found no apworld cookiecutter/template repo besides APQuest, Manual and `Eijebong/empty-apworld`.

## Gotchas
- Official worlds use patterns older than 0.6.7 (`self.multiworld.get_location(...)`, `Rules.py` helpers); prefer APQuest idioms for new code.
- Several bundled worlds lack manifests (checksfinder, shorthike, dlcquest, overcooked2, shapez); do not copy that.

## References
- https://github.com/ArchipelagoMW/Archipelago/tree/main/worlds/apquest
- `docs/context/apworld-development-guide.md`, `docs/context/apworld-pitfalls.md`, `docs/context/archipela-go-upstream-architecture.md`

## Zelda reference worlds
Already on disk and git-ignored: `.ap/worlds/{tloz,alttp,oot,tww}` (clone of Archipelago 0.6.8). Read `tloz` (smallest) first. See `docs/context/archipelago-game-model.md`.
