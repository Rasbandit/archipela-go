# Context Doc: APWorld Pitfalls and Gotchas

_Last verified: 2026-10-07_

## Status

Working. Items tagged (V) were reproduced by running a toy world against Archipelago 0.6.8 on Python 3.12; (S) were read in 0.6.8 / main-HEAD source or docs; (U) unverified. See `apworld-development-guide.md` for the correct patterns.

## What This Is

Checklist of mistakes that break generation, tests, packaging or players' games when writing an apworld.

## Environment

Archipelago 0.6.8 (main HEAD 9b64e83 has same core), Python 3.11-3.13 (3.12 recommended), Fedora/Linux.

## Pool, filler, classification

- (S) Items in pool must equal unfilled locations (`test_item_count_equal_locations`). Event locations are pre-filled, so count `len(multiworld.get_unfilled_locations(player))` AFTER creating all regions, then pad with filler/traps. Cap traps/extras at the free slot count; clamp so counts never go negative.
- (S) Do not add manually-placed items to the itempool; do not modify the pool or create regions/locations after `create_items`; entrances must be connected by the end of `connect_entrances`.
- (S) `get_filler_item_name` must return an infinitely repeatable item (used by item links, panic-method start inventory); default picks any item, which may be non-repeatable. Override it.
- (S) Any item referenced by logic must have `progression`. A count-gated item (`state.has("Key", p, 3)`) needs >= 3 copies actually in the pool.
- (S) `ItemClassification.skip_balancing` on goal items (e.g. letters) stops progression balancing pulling them early; balancing otherwise moves progression to earlier spheres.
- (S) Pool with fewer free slots than mandatory items (key/goal items) must raise `OptionError` in `generate_early`, not crash later.

## Fill and accessibility

- (S) Restrictive starts (few sphere-1 locations, several items needed to reach sphere 2) cause FillError. Fixes: `self.multiworld.local_early_items[self.player]["Key"] = 1`, more sphere-1 locations, `push_precollected`, or `OptionError`. `test_fill` asserts full accessibility unless `accessibility == minimal`.
- (S) Logic is monotonic: receiving an item must never make something unreachable. No missable/one-time logic.
- (S) Indirect conditions: rules using `can_reach_region/location/entrance` need `multiworld.register_indirect_condition`; item-only rules are safe.
- (S) Loop lambdas: bind loop variables (`lambda s, k=k: s.has("Key", p, k)`) or all rules see the last value.
- (S) Archived Archipelago-index criteria (good quality bar): never use the global `random` module (use `self.random`; pass it into pure helpers), no remote resources at generation, <1% fuzzer failure rate excluding `OptionError`.

## IDs and names

- (S) Item/location IDs: integer, 1..2**53-1 (<= 0 reserved), unique within the world; items and locations may share numbers. Recommended < 2**31-1 so 32-bit client ints work (JVM `Int`, etc.); a 13-digit offset exceeds that.
- (V) `AutoWorldRegister` drops any entry whose id is falsy (`if id`), so id 0 silently disappears.
- (S) Names unique per game, not all-numeric (`test_item_names_format`), group names must not equal item names, groups must be non-empty and contain valid items.
- (S) Do not use enum members as keys/values in `item_name_to_id`/`location_name_to_id`/slot_data (pickle "forbidden global" on WebHost upload). Same for `Option` objects in slot_data: use `.value` / `as_dict`.
- (S) `game` must be globally unique; a beta of a core game needs a different name. `archipelago.json` `game` must equal it.
- (S) Event items/locations use `code/address = None`; they are filtered from the datapackage.

## Options

- (S) `options: MyOptions` needs a colon; `options = MyOptions` makes `test_options_are_not_set_by_world` fail.
- (S) Every option class needs a docstring (`test_options_have_doc_string`) and a valid default; `OptionSet` valid_keys must not contain `random`; the same option class cannot be used twice in a dataclass; `NamedRange` special names must be lowercase and values outside [range_start, range_end] are accepted only if named.
- (S) `OptionSet.value` is a `set` (not JSON); `as_dict` converts to sorted list but asserts you pass fewer than all option names and at least one.
- (S) Validate in `generate_early` and raise `OptionError` (a `ValueError` subclass) with player name + option name; the fuzzer does not count it as a failure.

## Testing

- (V) `WorldTestBase.collect_all_but(names)` also collects pre-placed event items (e.g. "Victory"), so `assertBeatable(False)` after `collect_all_but(["Letter"])` is True. Exclude the event name too: `collect_all_but(["Letter", "Victory"])`.
- (V) Storing a caught `OptionError` on `self` in `world_setup` makes `tearDown` fail with "leaked MultiWorld object" (traceback holds frames). Store `str(exc)`, or set `auto_construct = False` and use `assertRaises(OptionError)` around `self.world_setup()`.
- (S) Default base tests (`test_all_state_can_reach_everything`, `test_empty_state_can_reach_something`, `test_fill`) run only when `options` is non-empty (or setUp/world_setup overridden); `run_default_tests = False` disables. `world_setup` is skipped for those inherited tests.
- (S) Import base as `from test.bases import WorldTestBase` (needs AP root on `sys.path`). Convention: `worlds/<w>/test/{__init__.py,bases.py,test_*.py}`; defining TestBase in `test/__init__.py` is deprecated; classes `Test*`, methods `test_*`; no `pytest.mark.parametrize` (use `test.param`). Keep tests < 1 s each.
- (S) The `test` module's tearDown fails if anything (module globals, class attrs, caches) keeps a reference to a `MultiWorld`/`World` (Python >= 3.11).
- (S) Core `test/general` + `test/webhost` run on every world: manifest, docs (`tutorials`, `en_<safe_game>.md`), ids, names, groups, slot_data JSON-serializable, items == locations.
- (V) Plan-style external tests dir (outside `worlds/`) work if `.ap` root is on `sys.path`; ap-actions' `ap-tests` expects `worlds/<name>/test` instead.

## Environment and ModuleUpdate

- (S) `ModuleUpdate.py` raises `RuntimeError` unless Python is 3.11-3.13 (Linux); `uv` default Python 3.14 fails. Pin `requires-python = ">=3.11,<3.14"` or `uv venv --python 3.12`.
- (S) `ModuleUpdate.update()` runs `pip install` for requirements; skip with env `SKIP_REQUIREMENTS_UPDATE=1` or set `ModuleUpdate.update_ran = True` before importing generation code.
- (V) Importing `worlds` needs `bsdiff4` and `pathspec` (not just the usual list); all ~89 bundled worlds import (~2 s); a failing bundled world (zillion) only logs.
- (U) `typings/` in AP only has kivy/schema stubs; no published stub package; for mypy use `ignore_missing_imports` or `mypy_path = .ap`.

## Packaging

- (S) `.apworld` = lowercase zip with top-level folder equal to zip name; uppercase raises a bogus exception in frozen py3.10+. All subfolders need `__init__.py`.
- (S) Do not hand-write `version`/`compatible_version`; Launcher "Build APWorlds" adds them (a hand-rolled `zip` lacks them; 0.6.x logs an error, 0.7.0 refuses to load).
- (S) `world_version` must be `"major.minor.build"` digits only; no world_version = treated as oldest.
- (S) Never `self.multiworld.regions = [...]` / `itempool = [...]` (wipes other games); use `+=`.
- (S) In-world imports must be relative; world imported absolutely by its folder name loads as a separate top-level module (V).

## Universal Tracker

- (S) Anything randomized with `self.random` that affects logic (trip tiers, key gating, entrance rando) must be in `fill_slot_data` and restored in `interpret_slot_data`, else UT shows wrong logic.

## References

- `docs/apworld_dev_faq.md`, `test/general/*.py` in <https://github.com/ArchipelagoMW/Archipelago>
- <https://github.com/Eijebong/Archipelago-index> (archived; criteria), <https://github.com/Eijebong/Archipelago-fuzzer>
