# Context Doc: APWorld Development Guide (Archipelago 0.6.x)

_Last verified: 2026-10-07_

## Status
Working. Verified by reading source at tag 0.6.8 and main HEAD 9b64e83 (`Utils.__version__` 0.6.9; `worlds/AutoWorld.py` identical to 0.6.8), and by running a toy world + WorldTestBase suite (22 tests pass) on Python 3.12 against 0.6.8. "UNVERIFIED" marks anything not read or run.

## What This Is
Writing an Archipelago world (apworld) in Python 0.6.x. Companions: `apworld-pitfalls.md`, `apworld-reference-implementations.md`.

## Environment
- Python 3.11-3.13 only (`ModuleUpdate.py` raises `RuntimeError` otherwise; 3.14 fails). Use 3.12 (AP CI uses ~3.12.7).
- Minimal deps to import `worlds` + `test.bases` (verified): `pytest websockets pyyaml jellyfish jinja2 schema platformdirs certifi orjson typing_extensions colorama bsdiff4 pathspec`. pytest>=9 has subtests built in. No kivy needed. `zillion` import error in logs is harmless.
- Source of truth: `docs/world api.md`, `docs/apworld specification.md`, `docs/tests.md`, `docs/apworld_dev_faq.md`, `docs/options api.md`, `docs/rule builder.md`, and the official template world `worlds/apquest/` (heavily commented; read first).

## Layout and imports (apworld spec)
`worlds/<folder>/{__init__.py, archipelago.json, options.py, items.py, locations.py, docs/, test/}`. Folder name lowercase.
- Inside the world: RELATIVE imports (`from .options import X`). From core: ABSOLUTE (`from Options import Toggle`, `from worlds.AutoWorld import World`, `from BaseClasses import ...`).
- Every subfolder with .py needs `__init__.py`. Observed: a world doing `from <foldername> import x` loads as a top-level module, not `worlds.<folder>`; breaks when packaged.
- Needs at least one game-info doc `docs/en_<secure_filename(game)>.md` and one setup doc plus a `WebWorld` with `Tutorial`s, else webhost tests fail (unless `hidden = True`). `secure_filename("Archipela-Go 2: Electric Boogaloo")` = `Archipela-Go_2_Electric_Boogaloo`.

## Lifecycle (call order, per player)
`stage_assert_generate` (cls) -> `generate_early` (options + `self.random` available; raise `OptionError` here) -> `create_regions` -> `create_items` (no new regions/locations/items after this) -> `set_rules` -> `connect_entrances` -> `generate_basic` -> `pre_fill` / `fill_hook` / `post_fill` -> `generate_output` -> `fill_slot_data` / `modify_multidata`. Never create regions/items in `__init__`.

## Minimal skeleton (verified: toy world passed WorldTestBase defaults)
```python
# worlds/mygame/__init__.py
from typing import Any
from BaseClasses import Item, ItemClassification, Location, Region
from Options import OptionError
from worlds.AutoWorld import World
from .options import MyOptions            # relative!

IDS = {"Key": 1, "Filler": 2, "Letter A": 10}

class MyItem(Item): game = "My Game"
class MyLocation(Location): game = "My Game"

class MyWorld(World):
    """Description shown on WebHost."""
    game = "My Game"                       # must be globally unique
    options_dataclass = MyOptions
    options: MyOptions                     # colon, not '='
    item_name_to_id = IDS
    location_name_to_id = {f"Trip {n}": 1000 + n for n in range(1, 101)}  # ids > 0, < 2**53
    item_name_groups = {"Letters": {"Letter A"}}

    def generate_early(self) -> None:
        if not self.options.modes.value:
            raise OptionError(f"{self.player_name}: modes must not be empty")

    def create_regions(self) -> None:
        menu = Region("Menu", self.player, self.multiworld)   # origin_region_name default "Menu"
        far = Region("Far", self.player, self.multiworld)
        self.multiworld.regions += [menu, far]                # never '=' (wipes all games)
        menu.connect(far, "Unlock", lambda s: s.has("Key", self.player, 2))
        for n in range(1, self.options.trips.value + 1):
            reg = menu if n % 2 else far
            reg.locations.append(MyLocation(self.player, f"Trip {n}", 1000 + n, reg))
        goal = MyLocation(self.player, "Goal", None, menu)    # id None = event
        goal.place_locked_item(MyItem("Victory", ItemClassification.progression, None, self.player))
        goal.access_rule = lambda s: s.has("Letter A", self.player)
        menu.locations.append(goal)
        self.multiworld.completion_condition[self.player] = lambda s: s.has("Victory", self.player)

    def create_item(self, name: str) -> Item:
        cls = ItemClassification.filler if name == "Filler" else ItemClassification.progression
        return MyItem(name, cls, IDS[name], self.player)

    def create_items(self) -> None:
        pool = [self.create_item("Key"), self.create_item("Key"), self.create_item("Letter A")]
        free = len(self.multiworld.get_unfilled_locations(self.player)) - len(pool)  # events are filled
        self.multiworld.itempool += pool + [self.create_filler() for _ in range(free)]

    def get_filler_item_name(self) -> str:
        return "Filler"                    # must be infinitely repeatable

    def fill_slot_data(self) -> dict[str, Any]:
        return {"modes": sorted(self.options.modes.value), **self.options.as_dict("goal", "death_link")}
```
Signatures (0.6.8 source): `Region(name, player, multiworld, hint=None)`; `Region.connect(connecting_region, name=None, rule=None) -> Entrance`; `Region.add_locations(Mapping[str,int|None], location_type=None)`; `Region.add_event(location_name, item_name=None, rule=None, location_type=None, item_type=None, show_in_spoiler=True) -> Item`; `Location(player, name='', address=None, parent=None)`; `Location.place_locked_item(item)`; `Item(name, classification, code, player)`; `Entrance(player, name='', parent=None)`; `worlds.generic.Rules.set_rule(spot, rule)` / `add_rule(spot, rule, combine="and")` (just set `spot.access_rule`); `World.set_rule(spot, rule_or_Rule)`, `World.set_completion_rule`, `World.push_precollected(item)`, `World.create_filler()`, `World.get_location/get_region/get_entrance(name)`.
`ItemClassification`: `filler=0 progression=1 useful=2 trap=4 skip_balancing=8 deprioritized=16` (IntFlag, combine with `|`; presets `progression_skip_balancing`, `progression_deprioritized`...). Anything referenced in logic MUST include `progression`.

## Rules (CollectionState, all verified in BaseClasses.py)
`has(item, player, count=1)`, `has_all(items, player)`, `has_any`, `has_all_counts(Mapping, player)`, `has_any_count`, `count(item, player)`, `has_from_list(items, player, count)`, `has_group(group, player, count=1)`, `count_group`, `can_reach_region/location/entrance(name, player)`. Bind loop vars in lambdas (`lambda s, k=k: ...`).
Optional rule builder (0.6.7+, `docs/rule builder.md`): `from rule_builder.rules import Has, HasAll`; `self.set_rule(loc, Has("Key", count=2) & HasAll("A","B"))`; use `&`/`|`, never `and`/`or`; `self.set_completion_rule(...)`. Read from source, not run.
Entrance rules using `can_reach_region/location` need `self.multiworld.register_indirect_condition(region, entrance)` (`explicit_indirect_conditions = True` default); item-only rules need nothing. Count-based progression = N copies of one item + `state.has(name, player, k)`.

## Options (Options.py)
`Toggle`, `DefaultOnToggle`, `Choice` (`option_x = n`, `default`, `alias_x`), `Range(range_start, range_end, default)`, `NamedRange(special_range_names={lowercase: int})` (values outside range allowed only if named), `OptionSet(valid_keys=frozenset, default=frozenset)`, `OptionDict/OptionList/OptionCounter`, `DeathLink` (a Toggle; add `death_link: DeathLink` field or inherit `DeathLinkMixin`). `@dataclass class MyOptions(PerGameCommonOptions)` with annotated fields (adds accessibility, progression_balancing, start_inventory, local/non_local_items, exclude/priority locations, item_links, plando_items). Every option class needs a docstring + `display_name`. Read values with `self.options.x.value`; `OptionSet.value` is a set. `self.options.as_dict("a","b")` for slot_data (sets -> sorted lists; `toggles_as_bools=True`; asserts you name fewer than all options). Optional: `option_groups` / `options_presets` on WebWorld, `StartInventoryPool` field to support start_inventory_from_pool.

## Items, filler, start inventory, hints
- Pool size must equal unfilled location count (events excluded). Compute `free = len(get_unfilled_locations(player)) - len(pool)` and fill with `create_filler()`/traps (`ItemClassification.trap`). Traps by percent: pick trap vs filler in `get_filler_item_name`.
- Start inventory: `self.push_precollected(self.create_item(n))` (does not remove from pool). `local_early_items`: `self.multiworld.local_early_items[self.player]["Key"] = 1` for restrictive starts.
- Hints: `start_hints`, `hint_blacklist` (ClassVar), `extend_hint_information`. Groups: `item_name_groups`, `location_name_groups` (names must not clash with item names; `Everything`/`Everywhere` auto-added).

## Events
Event = location with `address=None` holding locked progression item with `code=None` (`place_locked_item` or `region.add_event`). Generation-only; the client still must send StatusUpdate(goal) for victory.

## slot_data
Return JSON-serializable dict of plain values (never Option objects or enums: `.value`). Keep it small; locations/items are obtainable via LocationScouts. Anything randomized via `self.random` that affects logic must be here (needed for Universal Tracker). DeathLink: client adds the `DeathLink` tag itself; world only exposes the option (and usually slot_data `death_link`).

## Universal Tracker (UT) support (docs: FarisTheAncient/Archipelago `worlds/tracker/docs/apworld-integration.md`)
UT regenerates your world with no seed. Put seed-dependent state in `fill_slot_data`; restore it in `def interpret_slot_data(self, slot_data)` (or `@staticmethod ... return slot_data` to trigger regen, then read `self.multiworld.re_gen_passthrough[self.game]` in `generate_early`). `getattr(self.multiworld, "generation_is_fake", False)` marks UT generation. Flags: `ut_can_gen_without_yaml`, `disable_ut`, `glitches_item_name`, `location_id_to_alias`.

## Testing (verified skeleton; `test` package from AP root must be importable)
```python
# worlds/mygame/test/bases.py  (test/__init__.py must exist, empty)
from test.bases import WorldTestBase
class MyTestBase(WorldTestBase):
    game = "My Game"

# worlds/mygame/test/test_logic.py
from Options import OptionError
from .bases import MyTestBase

class TestDefault(MyTestBase):
    options = {"trips": 10}                # strings/ints accepted via option.from_any
    def test_beatable(self) -> None:
        self.collect_all_but(["Letter A", "Victory"])   # MUST exclude event item "Victory"
        self.assertBeatable(False)
        self.collect_by_name("Letter A")
        self.assertBeatable(True)
    def test_dep(self) -> None:
        self.assertAccessDependency([f"Trip {n}" for n in range(2, 11, 2)], [["Key"]])

class TestBad(MyTestBase):
    options = {"modes": []}
    auto_construct = False                 # build manually
    def test_raises(self) -> None:
        with self.assertRaises(OptionError):
            self.world_setup()
```
Helpers: `world` (`multiworld.worlds[1]`), `multiworld`, `player=1`, `collect_all_but(names, state=None)`, `collect_by_name`, `collect(items)`, `remove_by_name`, `get_items_by_name`, `get_item_by_name`, `count`, `can_reach_location/region/entrance`, `assertAccessDependency(locations, possible_items, only_check_listed=False)`, `assertBeatable(bool)`, `world_setup(seed=None)`. Defaults auto-run when `options` non-empty: `test_all_state_can_reach_everything`, `test_empty_state_can_reach_something`, `test_fill`; disable with `run_default_tests = False`. Tests must be runner-agnostic (no `pytest.mark.parametrize`; use `test.param`). Run: `AP_TEST_WORLDS=mygame pytest` from AP root, or `pytest worlds/mygame`. Core also runs `test/general/*` on every world (manifest, ids, names, groups, options docstrings, items==locations, slot_data JSON).

## Packaging
- `archipelago.json`: `{"game": "...", "world_version": "1.2.3", "minimum_ap_version": "0.6.7", "authors": [...]}`. `game` must equal the class `game`; `world_version` strictly `major.minor.build` digits; do NOT write `version`/`compatible_version`.
- Build (verified, also with a symlinked world folder): `python Launcher.py "Build APWorlds" -- "Game Name" --skip_open_folder` from the AP checkout root; output `build/apworlds/<folder>.apworld` (lowercase zip with top-level `<folder>/`); injects `version: 7, compatible_version: 7` into the manifest. `.apignore` (gitignore syntax) in the world folder excludes files (tests ship by default). Needs the world loaded from `worlds/<folder>`.
- Install: drop `.apworld` into AP `custom_worlds/` or Launcher "Install APWorld".

## 0.7.0 gotchas
- VERIFIED in `worlds/__init__.py`: an `.apworld` with missing/invalid manifest logs "will stop working with Archipelago 0.7.0" now and raises on >=0.7.0. `test_world_manifest.py` has a TODO to make manifests mandatory for source worlds. Always ship a manifest; build with the Launcher.
- `minimum_ap_version`: set to current stable at creation, raise only when needing newer core features; `maximum_ap_version` rarely.
- `required_client_version` is a class attribute; `get_required_client_version` is asserted against (verified).
- UNVERIFIED (Perplexity only, not in source): class-level `world_version` becoming an error; do not set it in the class (core populates it from the manifest).

## References
- https://github.com/ArchipelagoMW/Archipelago (docs/, worlds/apquest); https://github.com/Eijebong/ap-actions (CI: `ap-tests`, `fuzz`, `package-apworld`, `release-apworld.yml`, MIT; expects tests under `worlds/<name>/test`)
- https://github.com/Eijebong/Archipelago-fuzzer (hooks; index criteria in `apworld-pitfalls.md`)
