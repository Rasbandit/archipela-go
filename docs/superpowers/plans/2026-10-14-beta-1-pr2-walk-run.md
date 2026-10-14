# Beta 1 PR 2: Walk and Run Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the game walk and run only: delete the Bike and Drive travel modes and the Bike/Car items everywhere. Bump slot_data to schema 4, where `goals` is a plain list of goal ids.

**Architecture:** Mostly deletion behind new tests, in four layers that share one contract:

- the apworld (Python), which writes slot_data;
- the Rust core, which reads slot_data and owns `Mode`, the quest catalog, solo generation and the location filter;
- the Android UI, which draws mode chips, icons and help;
- scripts and docs.

Location ids renumber because the block formula is `difficulty_index * len(MODES) + mode_index`. A pinned id table on both sides keeps the apworld and the core in agreement.

**Tech Stack:** Python 3 apworld (Archipelago 0.6.8, pytest via `uv`), Rust core (`apgo-core`, `apgo-ffi`, UniFFI), Kotlin/Compose (JUnit, Gradle).

**Spec:** `docs/superpowers/specs/2026-10-13-beta-1-cleanup-design.md` (section "PR 2: walk and run"). Parent: `docs/superpowers/specs/2026-10-13-progression-beta-design.md`. Issue #184.

## Global Constraints

- Modes are exactly `walk`, `run`, in that order, everywhere (apworld `MODES`, core `Mode::ALL`, Kotlin `MODES`).
- **Full cut, no legacy.** No save migration, no shims, no `"car"` alias. Old slot_data fails with "unsupported slot_data schema"; old game files fail to load. All existing games are dev games.
- Item ids are NOT renumbered. `Bike` (`ID_OFFSET + 3`) and `Car` (`ID_OFFSET + 4`) become unused gaps and are never reused.
- Location ids ARE renumbered: `block = difficulty_index * len(MODES) + mode_index`, Boss block = `len(DIFFICULTIES) * len(MODES)` = 6. The core computes this from `Mode::ALL.len()`, never a literal `4` or `12`.
- slot_data schema 4: core `CURRENT_SCHEMA = 4`, `SUPPORTED_SCHEMAS = [4]`; apworld `SCHEMA_VERSION = 4`, `archipelago.json` `world_version` `0.4.0`. `goals` is a JSON list of goal id strings.
- **Keep (out of scope):**
  - Bluetooth car presence detection and its setup step.
  - OSM `way_class` BIKE/CAR street bits (`core/src/fill.rs`, `core/src/scan.rs`, `loc/graph.rs` road classes, `locator/display.rs`, `loc/bench/*`).
  - The `rack_em_up` quest kind and the realm `"bike"` icon.
  - The zones code (dormant; one zone by default).
  - The filler items `Hydrate!` and `Take a Breather!`.
- Catalog: delete the `pedal_pusher` and `plug_in` kinds; strip `bike`/`drive` from every `modes` list.
- The location filter must not change walk/run behaviour. Every remaining scenario in `core/tests/loc_scenarios.rs` passes unchanged.
- TDD: failing test first. Tests that exist only for Bike/Drive behaviour are deleted. Tests that used Bike/Drive as "some other mode" switch to `Mode::Run` / `"run"`. Never weaken an assertion about kept behaviour.
- Conventional commits, subject under 50 characters, body lines at most 72 characters, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Coverage floors only go up: py 99, rust 91 (`--fail-under-lines 91` in the justfile), kotlin 17.
- Worktree guard: no compound shell commands, heredocs or `$VAR`s in git or bash. Use Edit/Write for files, plain single commands, and `just <recipe>`.

### Location id table (both sides pin this)

```text
difficulty  mode  number  -> id - ID_OFFSET
easy        walk     1    ->     1
easy        run      7    ->  1007
medium      walk     1    ->  2001
medium      run      3    ->  3003
hard        walk     5    ->  4005
hard        run   1000    ->  6000
Boss Quest                ->  6001
```

## Review Focus

- **A YAML with `zone_modes: [walk, bike]`** fails generation with a clear `zone_modes has unknown modes ['bike']; valid: walk, run` and no crash. The Task 2 test covers it.
- **An old (schema 3) seed connecting to the new app** shows "unsupported slot_data schema 3 (this app understands [4])", not a parse panic. The Task 1 test covers it.
- **The solo generator with two zones `[walk, run]`** puts Running Shoes in zone 1, makes every id match the pinned table, and is always beatable. The Task 3 test covers it.
- **A game file saved before this PR** (a zone with mode `"bike"`, or old `goals: [{id, target}]`) fails to load with an error and does not crash the app. The Task 3 test covers it (`Game::load` of a bike zone returns `Err`).
- **The quest catalog** loads with no kind left without modes, and with no `pedal_pusher`/`plug_in` references left in the card text, the POI details or the Android icons. The Task 3 and Task 5 tests cover it.

---

### Task 1: slot_data schema 4, goals are ids

**Files:**

- Modify: `apworld/ap_go2/constants.py:5, 23-24` (schema 4; delete `GOAL_TARGET_OPTIONS`)
- Modify: `apworld/ap_go2/slot_data.py:1, 26, 42` (goals as ids)
- Modify: `apworld/ap_go2/__init__.py:183`
- Modify: `apworld/archipelago.json:3` (`0.4.0`)
- Modify: `apworld/docs/slot_data.schema.json:23-24, 35-48` (const 4; goals are strings)
- Modify: `apworld/docs/slot_data.sample.json:2, 4-8` (schema 4; goals ids)
- Modify: `apworld/docs/contract.md:1, 12, 20-21, 93`
- Modify: `core/src/slot.rs` (delete `GoalSpec`, `is_zero`, `goal`, `goal_target`, `goal_list`; schema 4)
- Modify: `core/src/goal.rs:6, 43, 88-97, 147-205`, `core/src/game.rs:30, 622`, `core/src/yaml.rs:5, 22-23, 67-121`, `core/src/solo.rs:13, 170-180, 382-386, 451-467`, `core/ffi/src/engine.rs:279, 2128`, `core/examples/parse_slot.rs:7`
- Test: `apworld/tests/test_constants.py`, `apworld/tests/test_slot_data.py`, `apworld/tests/world/test_world.py`, `core/src/slot.rs` tests

**Interfaces:**

- Consumes: nothing.
- Produces:
  - Core: `SlotData.goals: Vec<String>`, `slot::check_goal_ids(goals: &[String], mode: GoalMode, need: u32) -> Result<(), String>` (renamed from `check_goal_specs`), `goal::evaluate_each(..) -> Vec<(String, GoalStatus)>`, `Game::goal_statuses() -> Vec<(String, GoalStatus)>`, `SoloOptions.goals: Vec<String>`, `SoloOptions::goal_list() -> Vec<String>`. `GoalSpec` no longer exists. `SlotData::goal_list` no longer exists; read `slot.goals`.
  - apworld: `build_slot_data(goals: Sequence[str], ...)`, `constants.SCHEMA_VERSION == 4`.

- [ ] **Step 1: Write the failing tests**

In `apworld/tests/test_constants.py`, change the `SCHEMA_VERSION == 3` assertion (line 7) to `== 4`. Replace the `GOAL_TARGET_OPTIONS == {}` assertion (line 31) with `assert not hasattr(constants, "GOAL_TARGET_OPTIONS")`.

In `apworld/tests/test_slot_data.py`:

- Change the schema assertion (line 50) to `== 4`.
- Change every `goals=[("x", n), ...]` call to `goals=["x", ...]`.
- Change the `("schema_version", 2)` negative case (line 95) to `("schema_version", 3)`.
- Replace the targets test body (lines 118-127) with:

```python
        goals=["treasure_hunt", "all_trips"],
        goal_requirement="at_least",
        goal_need=2,
    )
    jsonschema.validate(data, SCHEMA)
    assert data["goals"] == ["treasure_hunt", "all_trips"]
    assert (data["goal_requirement"], data["goal_need"]) == ("at_least", 2)
```

In `apworld/tests/world/test_world.py`, replace `test_targets_are_zero` (lines 348-351):

```python
    def test_goals_are_plain_ids(self) -> None:
        data = self.world.fill_slot_data()
        jsonschema.validate(data, SCHEMA)
        assert data["goals"] == ["treasure_hunt", "all_trips"]
        assert (data["goal_requirement"], data["goal_need"]) == ("all", 2)
```

In `core/src/slot.rs` tests:

- Replace the schema-2 test (lines 217-223) with a schema-3 refusal.
- Change the sample assertions (205-206).
- Change the schema-99 replace string (228) to `"\"schema_version\": 4"`.

```rust
    #[test]
    fn schema_3_is_refused_and_4_is_read() {
        let old = SAMPLE.replace("\"schema_version\": 4", "\"schema_version\": 3");
        let err = SlotData::from_json(&old).unwrap_err();
        assert!(err.contains("unsupported slot_data schema 3"), "{err}");
        assert_eq!(SUPPORTED_SCHEMAS, [4]);
        assert_eq!(SlotData::from_json(SAMPLE).unwrap().schema_version, 4);
    }

    #[test]
    fn goals_are_a_list_of_ids() {
        let d = SlotData::from_json(SAMPLE).unwrap();
        assert_eq!(d.goals, vec!["treasure_hunt".to_string()]);
        let objects = SAMPLE.replace("\"goals\": [\"treasure_hunt\"]", "\"goals\": [{\"id\": \"treasure_hunt\", \"target\": 0}]");
        assert!(SlotData::from_json(&objects).is_err(), "schema 4 goals are plain ids");
    }
```

(Use the constant name the file already gives the `include_str!` sample at line 188. If it is not `SAMPLE`, use that name.)

In the `check_goal_specs` tests (lines 190-199), build goals as `vec!["treasure_hunt".to_string(), ...]` and call `check_goal_ids`.

- [ ] **Step 2: Run them to verify they fail**

Run: `just check-py`
Expected: FAIL. `SCHEMA_VERSION` is still 3, `GOAL_TARGET_OPTIONS` exists, and goals are still objects.

Run: `cargo test -p apgo-core slot::` (from `core/`; or `just check-rust`)
Expected: FAIL to compile (`check_goal_ids` is not defined).

- [ ] **Step 3: Implement (apworld)**

- `constants.py`: set `SCHEMA_VERSION = 4`. Delete lines 23-24 (the `GOAL_TARGET_OPTIONS` comment and dict).
- `slot_data.py`:
  - Docstring line 1: "Client contract v4".
  - `goals: Sequence[str],`
  - Line 42: `"goals": list(goals),`
- `__init__.py:183`: `goals=list(self.goals),`
- `archipelago.json`: `"world_version": "0.4.0"`.
- `slot_data.schema.json`:
  - `schema_version` `"const": 4`.
  - Set the `goals` items to `{"type": "string", "enum": ["treasure_hunt", "all_trips"]}`. Keep the existing `minItems`/`uniqueItems` if present.
  - Delete the goal object `required`/`properties` (`id`, `target`).
- `slot_data.sample.json`: `"schema_version": 4`, `"goals": ["treasure_hunt"]` on one line (the core test's `replace` relies on that exact text).
- `contract.md`:
  - Title: `# Client Contract v4 (schema_version = 4)`.
  - Line 12: "The core reads schema 4 only. Older seeds are refused."
  - Line 20: `schema_version | int, 4`.
  - Line 21: `goals | list of goal ids`.
  - Delete "(targets are 0)" at 93.

- [ ] **Step 4: Implement (core)**

`core/src/slot.rs`:

- Delete `is_zero` (37-40) and `struct GoalSpec` (43-51).
- In `SlotData`, delete `goal` and `goal_target` (and their doc lines) and make:

```rust
    /// The win conditions, as goal ids.
    pub goals: Vec<String>,
```

(no `#[serde(default)]`: schema 4 always writes `goals`).

- Set the constants:

```rust
/// The `slot_data` version this app writes, and the only one it reads.
pub const CURRENT_SCHEMA: u32 = 4;
/// The `slot_data` versions this app can read.
pub const SUPPORTED_SCHEMAS: [u32; 1] = [4];
```

- Rename `check_goal_specs(goals: &[GoalSpec], ...)` to `check_goal_ids(goals: &[String], ...)`. Inside, `g.id` becomes `g`.
- Delete `SlotData::goal_list`. `check_goals` calls `check_goal_ids(&self.goals, self.goal_mode, self.goal_need)`.
- Update the module doc at line 1 from "`slot_data` v2" to "`slot_data` v4".

`core/src/goal.rs`:

- Imports: drop `GoalSpec`.
- Line 43: `let goals = &slot.goals;` (adjust uses).
- `evaluate_each`:

```rust
pub fn evaluate_each(c: &GoalCtx<'_>) -> Vec<(String, GoalStatus)> {
    c.slot.goals.iter().map(|g| (g.clone(), evaluate_one(c, g))).collect()
}
```

- Tests:
  - Delete `goal: String::new(), goal_target: 0,` from the test `SlotData` literal (162-164) and set `schema_version: CURRENT_SCHEMA`.
  - Line 187: `s.goals = goals.iter().map(|id| (*id).to_string()).collect();`
  - Line 205: `assert_eq!(s.goals, vec!["all_trips".to_string()]);`
  - Update the `each` destructuring at 244 to the String id.

`core/src/game.rs`: delete `use crate::slot::GoalSpec;` (line 30). `goal_statuses` returns `Vec<(String, GoalStatus)>`.

`core/src/solo.rs`:

- Imports: `check_goal_ids` (not `check_goal_specs`), and no `GoalSpec`.
- Make `SoloOptions.goals: Vec<String>`.
- `goal_list`:

```rust
    /// The win conditions: the `goals` list, or the single `goal`.
    #[must_use]
    pub fn goal_list(&self) -> Vec<String> {
        if self.goals.is_empty() {
            vec![self.goal.clone()]
        } else {
            self.goals.clone()
        }
    }
```

- Delete `goal: String::new(), // schema 3 carries goals only` and `goal_target: 0,` from the `SlotData` literal (382-386).
- In the tests (451-467), `GoalSpec { id: "x".into(), target: 0 }` becomes `"x".to_string()`.
- `g.slot.goals[0].id` becomes `g.slot.goals[0]`.
- `check_goal_specs` becomes `check_goal_ids`.
- Fix any helper that takes `&[GoalSpec]` (`has_treasure_hunt`, `letters_for_goals`) to take `&[String]` and compare `g == "treasure_hunt"`.

`core/src/yaml.rs`:

- `check_goal_ids`.
- In the tests, `let g = |id: &str| id.to_string();`
- Drop the `GoalSpec` import.
- Fix any `.id` access in `build_yaml` to use the string.

`core/ffi/src/engine.rs`:

- Line 279: `goals: o.goals,`
- Near 2128, map the `(String, GoalStatus)` pairs: `g.id` becomes `g`.

`core/examples/parse_slot.rs:7`: `d.goal_list()` becomes `d.goals`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `just check-py`
Expected: PASS, coverage at least 99.

Run: `just check-rust`
Expected: PASS (clippy clean, coverage at least 91).

- [ ] **Step 6: Commit**

```bash
git add apworld core
git commit -m "feat!: slot_data schema 4, goals are ids"
```

Body: "Drops GoalSpec.target and the schema-2 goal/goal_target fields. The core reads schema 4 only. Refs #184."

---

### Task 2: apworld is walk and run only

**Files:**

- Modify: `apworld/ap_go2/constants.py:12, 36-51, 55-56`
- Modify: `apworld/ap_go2/items.py:14-15` (delete Bike, Car)
- Modify: `apworld/ap_go2/locations.py:1-5` (docstring)
- Modify: `apworld/ap_go2/options.py:76` (ZoneModes docstring)
- Modify: `apworld/docs/slot_data.schema.json:135-136, 167-168` (tool and mode enums)
- Modify: `apworld/docs/slot_data.sample.json` (zones 37-45, trips 212-524, boss 533)
- Modify: `apworld/docs/example.yaml:9`, `apworld/docs/contract.md:33, 41, 43-48, 59, 64-67, 74`
- Modify: `core/src/slot.rs:209` (the sample's zone tool assertion)
- Test: `apworld/tests/test_constants.py`, `test_locations.py`, `test_validation.py`, `test_item_plan.py`, `test_zones.py`, `test_distribution.py`, `test_effort.py`, `test_slot_data.py`, `world/test_world.py`

**Interfaces:**

- Consumes: Task 1's schema-4 sample.
- Produces:
  - `constants.MODES == ("walk", "run")`.
  - `constants.MODE_TOOLS == {"run": "Running Shoes"}`.
  - `constants.MODE_SPEED_KMH == {"walk": 4.5, "run": 9.0}`.
  - `constants.BOSS_BLOCK == 6`.
  - `locations.quest_location_id(difficulty, mode, number)` matches the id table.
  - The sample's zones are walk, then run (tool `"Running Shoes"`). Task 3's core sample test relies on this.

- [ ] **Step 1: Write the failing tests**

`apworld/tests/test_constants.py`:

```python
def test_only_walk_and_run() -> None:
    assert constants.MODES == ("walk", "run")
    assert constants.MODE_SPEED_KMH == {"walk": 4.5, "run": 9.0}
    assert constants.MODE_TOOLS == {"run": "Running Shoes"}
    assert all(modes == constants.MODES for modes in constants.FAMILY_MODES.values())
    assert constants.BOSS_BLOCK == 6
```

(Delete the old assertions at lines 13-21 that list bike/drive.)

`apworld/tests/test_locations.py`, replacing the first three tests:

```python
# The location id table. core/src/solo.rs pins the same rows; change both together.
ID_TABLE = [
    ("easy", "walk", 1, 1),
    ("easy", "run", 7, 1007),
    ("medium", "walk", 1, 2001),
    ("medium", "run", 3, 3003),
    ("hard", "walk", 5, 4005),
    ("hard", "run", 1000, 6000),
]


def test_total_and_unique_ids() -> None:
    assert len(LOCATION_NAME_TO_ID) == 6 * MAX_TRIPS + 1
    assert len(set(LOCATION_NAME_TO_ID.values())) == len(LOCATION_NAME_TO_ID)
    assert all(v > 0 for v in LOCATION_NAME_TO_ID.values())


@pytest.mark.parametrize(("difficulty", "mode", "number", "offset"), ID_TABLE)
def test_id_table(difficulty: str, mode: str, number: int, offset: int) -> None:
    assert quest_location_id(difficulty, mode, number) == ID_OFFSET + offset
    name = f"{difficulty.title()} {mode.title()} Quest #{number}"
    assert LOCATION_NAME_TO_ID[name] == ID_OFFSET + offset


def test_boss_id() -> None:
    assert LOCATION_NAME_TO_ID[names.BOSS_LOCATION] == ID_OFFSET + 6001


def test_groups() -> None:
    assert set(LOCATION_NAME_GROUPS) == {"Easy", "Medium", "Hard", "Walk", "Run", "Boss"}
    assert "Easy Run Quest #3" in LOCATION_NAME_GROUPS["Easy"]
    assert "Easy Run Quest #3" in LOCATION_NAME_GROUPS["Run"]
    assert "Easy Run Quest #3" not in LOCATION_NAME_GROUPS["Hard"]
    assert LOCATION_NAME_GROUPS["Boss"] == {names.BOSS_LOCATION}
    assert names.BOSS_LOCATION in LOCATION_NAME_GROUPS["Hard"]
    assert all(g <= set(LOCATION_NAME_TO_ID) for g in LOCATION_NAME_GROUPS.values())
```

(Add `import pytest` at the top. Fix the `test_slot_data.py:66` boss pin `+12_001` to `+6001`.)

`apworld/tests/test_validation.py`:

- Line 49: `check(zone_modes=["Walk ", "RUN"]) == ["walk", "run"]`.
- Line 109: `zone_modes=["walk", "run"]`. (Re-check that `trips=2` is still too few: keys 1 + tools 1 + letters 4 > 2.)
- Add:

```python
@pytest.mark.parametrize("mode", ["bike", "drive", "car"])
def test_bike_and_drive_are_gone(mode: str) -> None:
    with pytest.raises(ValueError, match=rf"unknown modes \['{mode}'\]; valid: walk, run"):
        check(zone_modes=["walk", mode])
```

`apworld/tests/test_item_plan.py:41`: replace `counts["Bike"] == counts["Car"] == 1` with `"Bike" not in counts and "Car" not in counts`. Switch any multi-zone setup at lines 16, 27 and 52 to `["walk", "run"]`.

Add to `test_item_plan.py` (or `test_constants.py`):

```python
def test_bike_and_car_items_are_gone_and_ids_are_not_reused() -> None:
    from worlds.ap_go2.items import ITEM_NAME_TO_ID  # use the module's real table name
    assert "Bike" not in ITEM_NAME_TO_ID and "Car" not in ITEM_NAME_TO_ID
    assert ID_OFFSET + 3 not in ITEM_NAME_TO_ID.values()
    assert ID_OFFSET + 4 not in ITEM_NAME_TO_ID.values()
```

Other tests:

- `test_zones.py`, `test_distribution.py`, `test_effort.py`: every `"bike"`/`"drive"` zone becomes `"run"`.
  - The test at `test_distribution.py:68-89` checks that drive skips park/steps. Delete it, because every family now allows both modes.
  - `MODES[i % 4]` becomes `MODES[i % len(MODES)]`.
  - In `test_effort.py:38`, `distance_km(6, 10, "drive")` becomes `"run"`, with the expected value recomputed from 9.0 km/h.
- `test_slot_data.py:21, 62, 69, 114`: `["walk", "run"]`, tool `"Running Shoes"`, and sample zones `["walk", "run"]`.
- `world/test_world.py`:
  - 22: `MULTI = ["walk", "run"]`.
  - 99: `[None, "Running Shoes"]`.
  - Fix 86, 105, 191, 239 and 314 to the same two-zone world.

- [ ] **Step 2: Run them to verify they fail**

Run: `just check-py`
Expected: FAIL on `MODES`, the id table, the bike rejection and the item gaps.

- [ ] **Step 3: Implement**

`constants.py`:

```python
MODES = ("walk", "run")
```

```python
# Nominal speeds, km/h, used by clients to turn effort minutes into distance.
MODE_SPEED_KMH: dict[str, float] = {"walk": 4.5, "run": 9.0}

BOSS_FAMILY = "boss"
FAMILY_MODES: dict[str, tuple[str, ...]] = {
    "reach": MODES,
    "dwell": MODES,
    "landmark": MODES,
    "courier": MODES,
    "away": MODES,
    "explore": MODES,
    "trail": MODES,
    "water": MODES,
    "park": MODES,
    "steps": MODES,
}
```

(The literal dict stays: Android's `HelpTextTest` reads its keys.)

```python
# Default tool item for each mode that needs one. Walking is always allowed.
MODE_TOOLS: dict[str, str] = {"run": "Running Shoes"}
```

`items.py`: delete the `"Bike"` and `"Car"` lines. Add a comment above `"Running Shoes"`: `# ID_OFFSET + 3 and + 4 were Bike and Car; never reuse them.`

`locations.py` docstring:

```python
"""Location ID table and name groups. Pure.

ID = ID_OFFSET + block * 1000 + n, with block = difficulty_index * len(MODES) + mode_index
(difficulty order Easy, Medium, Hard; mode order Walk, Run). `Boss Quest` is block 6, id 6001.
"""
```

`options.py:76`: `"""Ordered zones, each walk or run (1 to 6). Zone 1 is free; the rest need keys."""`

`slot_data.schema.json`: tool enum `["Running Shoes"]`, mode enum `["walk", "run"]`.

`slot_data.sample.json`:

- Zones: keep zone 1 walk. Zone 2 becomes `"mode": "run"`, `"tool": "Running Shoes"`. Delete zone 3 if it was drive. If the sample needs three zones elsewhere, make zone 3 `walk` with `"tool": null`.
- Every trip with `"mode": "bike"`/`"drive"`: set the mode to the mode of its zone and renumber its `location_id` with the new formula. The ids must stay unique.
- Boss: `"mode"` = last zone's mode, `"location_id": 8902400006001`.
- Re-validate with the schema test.

`example.yaml:9`: `zone_modes: [walk, run]`.

`contract.md`:

- Mode `walk|run`.
- Speeds: walk 4.5, run 9.0.
- Families table: every family is walk and run.
- Tools: Running Shoes only. Bike and Car item ids are retired gaps.
- Id math: `block = difficulty_index * 2 + mode_index`, Boss 6001, groups Easy/Medium/Hard/Walk/Run/Boss. Paste the id table from this plan.

`core/src/slot.rs:209`: the sample assertion becomes `tool == Some("Running Shoes")` (with `zones[1]` matching the new sample).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `just check-py`
Expected: PASS, coverage at least 99. This includes real Archipelago generation.

Run: `just check-rust`
Expected: PASS. The core includes the sample and `example.yaml`. The core still has `Mode::Bike`, which is harmless here.

- [ ] **Step 5: Commit**

```bash
git add apworld core/src/slot.rs
git commit -m "feat!: apworld is walk and run only"
```

Body: "Removes bike/drive modes and the Bike/Car items (ids +3/+4 retired). Location ids renumber; the id table is pinned. Refs #184."

---

### Task 3: core is walk and run only

**Files:**

- Modify: `core/src/catalog.rs:9-57` (enum, `ALL`, delete `PLAY`, `parse`, `m_per_min`), tests 306-313, 400-406
- Modify: `core/data/quest_catalog.json` (strip modes; delete `plug_in`, `pedal_pusher`)
- Modify: `core/src/card.rs:80` (+ tests 299, 318), `core/src/poi_details.rs:100` (+ test 393), `core/src/assign.rs:173` (doc)
- Modify: `core/src/solo.rs:47-69, 263, 285-301` (`tool_for`, `family_allows`, id functions)
- Modify: `core/src/loc/imm.rs:76-80, 406-416` (`sigma_a` Drive arm, `mode_cap_mps`)
- Modify: `core/src/loc/graph.rs:25-32` (`mode_mask`)
- Modify: `core/src/game.rs:445-449` (`speed_ok`)
- Modify: `core/src/items.rs:12`
- Modify: `core/ffi/src/engine.rs:370, 538, 1410, 1436, 2124` (docs, `PLAY` becomes `ALL`, tool list)
- Modify: `core/examples/replay.rs:2, 53, 150`
- Modify: `core/tests/loc_scenarios.rs` (delete the bike scenarios at 243-260, 334-350, 406-428)
- Modify: every test site in the table below
- Regenerate: `docs/context/quest-catalog.md`

**Interfaces:**

- Consumes: Task 2's sample (zones walk, run).
- Produces:
  - `Mode { Walk, Run }`, `Mode::ALL: [Mode; 2]`. `Mode::PLAY` is gone; use `Mode::ALL`.
  - `Mode::parse("bike" | "drive" | "car") == None`.
  - `solo::quest_location_id(difficulty_index: usize, mode: Mode, number: i64) -> i64` and `solo::boss_location_id() -> i64`.

- [ ] **Step 1: Write the failing tests**

`core/src/catalog.rs` tests. Replace the `multi_values_and_modes` mode lines (403-405) and rename the forager test:

```rust
    #[test]
    fn only_walk_and_run_exist() {
        assert_eq!(Mode::ALL, [Mode::Walk, Mode::Run]);
        for gone in ["bike", "drive", "car"] {
            assert_eq!(Mode::parse(gone), None, "{gone}");
        }
        assert_eq!(Mode::parse("run"), Some(Mode::Run));
    }

    #[test]
    fn every_kind_has_a_walk_or_run_mode_and_the_ride_kinds_are_gone() {
        let c = Catalog::builtin();
        for k in &c.kinds {
            assert!(k.modes.iter().any(|m| Mode::ALL.contains(m)), "{} has no walk/run mode", k.id);
        }
        assert!(c.kind("pedal_pusher").is_none() && c.kind("plug_in").is_none());
    }
```

In `forager_is_a_courier_kind_for_walk_run_and_bike_with_a_collect_rule`, rename it to `forager_is_a_courier_kind_with_a_collect_rule` and assert `k.modes == [Mode::Walk, Mode::Run]`. In `multi_values_and_modes`, delete the `"car"` and `Drive` lines and keep `allows(Mode::Walk)`.

`core/src/solo.rs` tests. Add the pinned id table (the same rows as the apworld's `ID_TABLE`):

```rust
    // The location id table. apworld/tests/test_locations.py pins the same rows; change both together.
    #[test]
    fn location_ids_match_the_apworld_table() {
        let table: [(usize, Mode, i64, i64); 6] = [
            (0, Mode::Walk, 1, 1),
            (0, Mode::Run, 7, 1007),
            (1, Mode::Walk, 1, 2001),
            (1, Mode::Run, 3, 3003),
            (2, Mode::Walk, 5, 4005),
            (2, Mode::Run, 1000, 6000),
        ];
        for (di, mode, n, offset) in table {
            assert_eq!(quest_location_id(di, mode, n), ID_OFFSET + offset, "{di} {mode:?} {n}");
        }
        assert_eq!(boss_location_id(), ID_OFFSET + 6001);
    }

    #[test]
    fn a_run_zone_needs_running_shoes_and_walking_needs_nothing() {
        assert_eq!(tool_for(Mode::Walk), None);
        assert_eq!(tool_for(Mode::Run), Some("Running Shoes"));
        let g = generate(&opts(&[Mode::Walk, Mode::Run], 40, "treasure_hunt"), 2).unwrap();
        assert_eq!(g.slot.zones[1].tool.as_deref(), Some("Running Shoes"));
        assert_eq!(g.slot.boss.as_ref().unwrap().location_id, boss_location_id());
        assert!(is_beatable(&g));
    }
```

In `counts_ids_and_difficulty_bands_follow_the_apworld_layout` (499-516):

- Zones become `&[Mode::Walk, Mode::Run, Mode::Walk]`.
- The id check becomes `assert_eq!(q.location_id, quest_location_id(di as usize, q.mode, (q.location_id - ID_OFFSET) % BLOCK_SIZE), ...)`.
- `Some("Bike")` becomes `Some("Running Shoes")`.
- Zone 3 (walk, same as zone 1) has no tool.

In `reward_table_matches_the_pool_and_is_always_beatable` (522-527), the cases become:

```rust
            (vec![Mode::Walk], 20, "treasure_hunt"),
            (vec![Mode::Walk, Mode::Run], 40, "treasure_hunt"),
            (vec![Mode::Walk, Mode::Run, Mode::Walk, Mode::Run], 60, "treasure_hunt"),
            (vec![Mode::Run, Mode::Walk, Mode::Walk], 30, "all_trips"),
            (vec![Mode::Walk; 6], 80, "all_trips"),
```

`unlock_items_never_sit_behind_the_zone_they_unlock` (542): `&[Mode::Walk, Mode::Run, Mode::Walk]`. The `deterministic_...` test (567): `&[Mode::Walk, Mode::Run]`.

`core/src/game.rs`: add a load-refusal test next to the existing `load` tests. Save a walk game, replace `"mode":"walk"` with `"mode":"bike"` in the JSON, and assert that `Game::load` returns `Err`:

```rust
    #[test]
    fn a_saved_game_with_a_bike_zone_is_refused() {
        let g = /* the module's usual test game constructor */;
        let json = g.save_json(); // use the module's real save call
        let old = json.replacen("\"mode\":\"walk\"", "\"mode\":\"bike\"", 1);
        assert_ne!(old, json, "the fixture must contain a walk zone");
        assert!(Game::load(&old).is_err());
    }
```

(Use the real constructor and the real save/load function names from the neighbouring `load` tests. Do not invent any.)

- [ ] **Step 2: Run them to verify they fail**

Run: `just check-rust`
Expected: FAIL to compile (`quest_location_id`/`boss_location_id` are not defined; `Mode::ALL` has 4 elements).

- [ ] **Step 3: Implement the enum and the catalog**

`core/src/catalog.rs`:

```rust
/// How the player travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// On foot.
    Walk,
    /// Running.
    Run,
}

impl Mode {
    /// Every mode, slowest first.
    pub const ALL: [Self; 2] = [Self::Walk, Self::Run];

    /// The lowercase name of the mode, as used in settings and files.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Walk => "walk",
            Self::Run => "run",
        }
    }

    /// The mode called `s`, if any.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s)
    }

    /// Typical active speed in meters per minute.
    #[must_use]
    pub fn m_per_min(self) -> f64 {
        match self {
            Self::Walk => 75.0,
            Self::Run => 150.0,
        }
    }
}
```

Catalog JSON: write this script to the session scratchpad as `strip_modes.py` and run `python3 -I <scratchpad>/strip_modes.py core/data/quest_catalog.json`:

```python
import json
import sys

path = sys.argv[1]
with open(path, encoding="utf-8") as f:
    data = json.load(f)
keep = ["walk", "run"]
data["modes"] = keep
data["kinds"] = [k for k in data["kinds"] if k["id"] not in {"pedal_pusher", "plug_in"}]
for k in data["kinds"]:
    k["modes"] = [m for m in k["modes"] if m in keep]
    assert k["modes"], k["id"]
with open(path, "w", encoding="utf-8") as f:
    json.dump(data, f, indent=1, ensure_ascii=False)
    f.write("\n")
```

Then run `git diff --stat core/data/quest_catalog.json`. Expect only deletions. If formatting churn shows (for example escaped non-ASCII), stop and match the original style.

Then:

- `card.rs:80`: delete the `"pedal_pusher"` arm and its test lines (299, 318).
- `poi_details.rs:100`: delete the `pedal_pusher` arm and its test (393).
- `assign.rs:173`: drop `pedal_pusher` from the doc.
- `items.rs:12`: delete the `"Bike" | "Car"` arm and its test at `items.rs:42`.

- [ ] **Step 4: Implement solo**

`core/src/solo.rs`:

```rust
/// The tool item a zone in `mode` needs, if any (walking needs none).
#[must_use]
pub fn tool_for(mode: Mode) -> Option<&'static str> {
    match mode {
        Mode::Walk => None,
        Mode::Run => Some("Running Shoes"),
    }
}

/// Every quest family can be played walking or running.
fn family_allows(family: &str, _mode: Mode) -> bool {
    FAMILIES.contains(&family)
}
```

(If clippy flags `_mode` as an unused parameter under `pedantic`, drop the parameter and update its two callers, 278 and the test at 511.)

```rust
/// Location id of quest `number` (from 1) of difficulty `difficulty_index` in `mode`; mirrors the apworld's `quest_location_id`.
#[must_use]
pub fn quest_location_id(difficulty_index: usize, mode: Mode, number: i64) -> i64 {
    ID_OFFSET + block(difficulty_index, mode) * BLOCK_SIZE + number
}

/// Location id of the Boss Quest: the block after every difficulty and mode.
#[must_use]
pub fn boss_location_id() -> i64 {
    ID_OFFSET + count_i64(DIFFICULTIES.len() * Mode::ALL.len()) * BLOCK_SIZE + 1
}

fn block(difficulty_index: usize, mode: Mode) -> i64 {
    count_i64(difficulty_index * Mode::ALL.len()) + mode_index(mode)
}
```

In `generate` (285-301):

```rust
                let c = counters.entry((di, block(di, zone.mode))).or_insert(0);
                *c += 1;
                trips.push(QuestSlot {
                    location_id: quest_location_id(di, zone.mode, *c),
```

and `location_id: boss_location_id(),` for the boss. The `counters` key type stays `(usize, i64)`.

- [ ] **Step 5: Fix the remaining production match arms**

- `loc/imm.rs` `sigma_a`: delete the `F if mode == Mode::Drive => p.sigma_a_drive,` arm (Task 4 renames the rest).
- `loc/imm.rs` `mode_cap_mps`: arms `Walk => 12.0`, `Run => 25.0`. Doc: "The speed above which quests of a mode stop counting (`Game::speed_ok`), m/s."
- `loc/imm.rs:736-739` test: expected `[12.0, 25.0]`.
- `loc/graph.rs` `mode_mask`:

```rust
/// The way classes a travel mode may use.
#[must_use]
pub fn mode_mask(mode: Mode) -> u8 {
    match mode {
        Mode::Walk | Mode::Run => way_class::FOOT,
    }
}
```

- `game.rs` `speed_ok`:

```rust
/// Whether quests of `mode` count at `kmh`: the mode's speed cap (`loc::imm::mode_cap_mps`, one table for the filter and the game).
fn speed_ok(mode: Mode, kmh: f64) -> bool {
    kmh / 3.6 <= mode_cap_mps(mode)
}
```

- `ffi/src/engine.rs`:
  - 1410, 1436: `Mode::PLAY` becomes `Mode::ALL`.
  - 2124: `["Running Shoes"].iter()...`
  - 370, 538 docs: "walk or run".
- `examples/replay.rs`:
  - 2: `--mode walk|run`.
  - 53: `.expect("walk or run")`.
  - 150: `rts_reference(&rec.fixes, 15.0, 0.5)`.
- `loc/mod.rs:377` `mode_at` doc: "a runner between zones is never judged as a walker".

- [ ] **Step 6: Fix the test sites**

Rule: Bike or Drive used as "some other mode" becomes `Mode::Run`. A test that asserts Bike or Drive behaviour (bike speed caps, drive skips the cap, bike graph mask, bike gap handling, fast model priors) is deleted. Its walk/run twin stays.

| File | Lines |
| -- | -- |
| `core/src/assign.rs` | 1269-1277, 1530, 1548, 1630, 1699, 1704 |
| `core/src/game.rs` | 2155, 2588, 2617, 2627, 2631, 2633, 2678, 2790, 2794, 3318-3322, 4182 |
| `core/src/solo.rs` | done in Step 1 |
| `core/src/loc/imm.rs` | 440, 454, 482-483, 551 (551 is Task 4's; switch to `Mode::Walk` for now) |
| `core/src/loc/locator/display.rs` | 341, 579-584 |
| `core/src/loc/matcher.rs` | 635, 1146, 1198 |
| `core/src/loc/mod.rs` | 446-449 |
| `core/ffi/src/engine.rs` | 3363-3369 |
| `core/src/loc/locator/mod.rs` | 798, 805, 944 |
| `core/src/effort.rs` | 62 |
| `core/src/yaml.rs` | 85, 87 (`zone_modes: [walk, run]`) |
| `core/src/scan.rs` | 1567 |
| `core/src/loc/graph.rs` | 683 |
| `core/src/loc/locator/gap.rs` | 466-469 |
| `core/src/journal.rs` | 348 (`"Bike"` becomes `"Running Shoes"`) |
| `core/src/traps.rs` | 336 (`"Bike"` becomes `"Running Shoes"`) |
| `core/tests/loc_scenarios.rs` | delete the bike scenarios at 243-260, 334-350, 406-428; walk scenarios unchanged |

`core/src/realm.rs:466` (a legacy realm `"mode":"bike"`) stays: realms ignore mode.

Find leftovers with: `grep -rn "Mode::Bike\|Mode::Drive\|\"bike\"\|\"drive\"" core --include=*.rs`. Expected after this step: only `realm.rs:466` and street-bit code that uses `way_class::BIKE`/`CAR`.

- [ ] **Step 7: Regenerate the catalog doc**

Run: `python3 scripts/catalog_doc.py` (check its usage line first; it may need an output path). Expect `docs/context/quest-catalog.md` without Plug In and Pedal Pusher rows and with `walk, run` modes.

- [ ] **Step 8: Run the tests to verify they pass**

Run: `just check-rust`
Expected: PASS (clippy clean). All the remaining `loc_scenarios` pass with no tolerance edits. Coverage is at least 91; if it dropped, add tests, never lower the floor.

Run: `just check-py`
Expected: PASS (the catalog is shared with the apworld if it reads it; otherwise unchanged).

- [ ] **Step 9: Commit**

```bash
git add core docs/context/quest-catalog.md
git commit -m "feat!: core is walk and run only"
```

Body: "Deletes Mode::Bike/Drive, the pedal_pusher and plug_in kinds and the Drive speed exemption. Location ids come from Mode::ALL.len() and match the apworld table. Refs #184."

---

### Task 4: the location filter drops the fast-zone tuning

After Task 3 every zone is "slow", so `pi_fast`, `mu0_fast`, `v0_fast_mps` and `sigma_a_drive` are dead. `sigma_a_bike` is still live: it is the fast (F) model's noise in every zone. It is renamed, not deleted. This task changes no behaviour.

**Files:**

- Modify: `core/src/loc/params.rs:27-45, 278-280, 344-351`
- Modify: `core/src/loc/imm.rs:51-60, 76-80, 149-157, 200` and tests 440-554
- Modify: `core/src/loc/locator/mod.rs:483, 522`, `core/src/loc/locator/gap.rs:334`
- Modify: `core/src/loc/locator/steps.rs:145` (doc), `core/src/loc/locator/gap.rs:149` (doc if it names Bike/Drive)

**Interfaces:**

- Consumes: Task 3's two-mode `Mode`.
- Produces:
  - `LocParams` fields `sigma_a_fast`, `v0_mps`, `mu0`, `pi` replace `sigma_a_bike`, `v0_slow_mps`, `mu0_slow`, `pi_slow`.
  - `sigma_a_drive`, `v0_fast_mps`, `mu0_fast`, `pi_fast` are deleted.
  - `imm::transition(p: &LocParams, dt_s: f64)` and `Imm::new(z, sigma, t_ms, p)` lose their `mode` argument.

- [ ] **Step 1: Write the failing test**

In `core/src/loc/imm.rs` tests, pin today's walk/run numbers so the refactor cannot drift:

```rust
    #[test]
    fn the_filter_tuning_is_the_old_walk_and_run_tuning() {
        let p = params();
        assert_eq!(p.pi, [[0.95, 0.048, 0.002], [0.05, 0.94, 0.01], [0.02, 0.08, 0.90]]);
        assert_eq!(p.mu0, [0.6, 0.35, 0.05]);
        assert!((p.v0_mps - 2.0).abs() < 1e-12);
        assert!((sigma_a(&p, Mode::Walk, F) - 1.5).abs() < 1e-12, "the fast model keeps its old noise");
        assert!((sigma_a(&p, Mode::Run, W) - p.sigma_a_run).abs() < 1e-12);
        let t = transition(&p, 1.0);
        assert!((t[0][1] - 0.048).abs() < 1e-12);
    }
```

Run: `just check-rust`
Expected: FAIL to compile (`pi`, `mu0`, `v0_mps` are not defined; `transition` takes 3 args).

- [ ] **Step 2: Implement**

`params.rs`, the struct fields:

```rust
    /// White-noise acceleration of the walking model in a Walk zone, m/s^2.
    pub sigma_a_walk: f64,
    /// White-noise acceleration of the walking model in a Run zone, m/s^2.
    pub sigma_a_run: f64,
    /// White-noise acceleration of the fast model, m/s^2.
    pub sigma_a_fast: f64,
    /// Initial velocity sigma, m/s.
    pub v0_mps: f64,
    /// Initial model probabilities (S, W, F).
    pub mu0: [f64; 3],
    /// Per-second switching rates (the diagonal is ignored: it is the remainder).
    pub pi: [[f64; 3]; 3],
```

Defaults: `sigma_a_fast: 1.5, v0_mps: 2.0, mu0: [0.6, 0.35, 0.05], pi: [[0.95, 0.048, 0.002], [0.05, 0.94, 0.01], [0.02, 0.08, 0.90]],`. Delete the `sigma_a_drive`, `v0_fast_mps`, `mu0_fast` and `pi_fast` lines. Trim the gap-bridging doc at 278-280 to drop "(Bike and Drive never bridge ... ruling T22-R2.)".

`imm.rs`: delete `fn slow`.

```rust
pub fn transition(p: &LocParams, dt_s: f64) -> [[f64; 3]; 3] {
    let t = dt_s.clamp(0.0, p.max_predict_s);
    std::array::from_fn(|i| {
        let mut off: [f64; 3] = std::array::from_fn(|j| if i == j { 0.0 } else { p.pi[i][j] * t });
```

(rest unchanged)

```rust
pub fn sigma_a(p: &LocParams, mode: Mode, model: usize) -> f64 {
    match model {
        W if mode == Mode::Run => p.sigma_a_run,
        W => p.sigma_a_walk,
        F => p.sigma_a_fast,
        _ => 0.0,
    }
}
```

```rust
    /// A fresh filter at `z` (metres, ENU) with position sigma `sigma`: velocity 0 with the initial sigma, the model priors.
    #[must_use]
    pub fn new(z: [f64; 2], sigma: f64, t_ms: i64, p: &LocParams) -> Self {
        let v0 = p.v0_mps;
        ...
        Self { models: [g; 3], mu: normalized(p.mu0, p.mu_floor), t_ms }
    }
```

Callers:

- `imm.rs:200`: `transition(p, dt)`. If `mode` becomes unused in that function, keep it only if `sigma_a` still needs it.
- `locator/mod.rs:522`: `Imm::new([0.0, 0.0], sigma, f.t_ms, &self.params)`.
- `locator/mod.rs:483`: scale `sigma_a_walk`, `sigma_a_run`, `sigma_a_fast`.
- `gap.rs:334`: `self.params.mu0`.
- imm tests 440-554: drop the mode argument. Delete the `mu0_fast` test case (550-551). Keep the `mu0` zero-normalising case (547-548) using `mu0`.

- [ ] **Step 3: Run the tests**

Run: `just check-rust`
Expected: PASS. Every `loc_scenarios` walk test passes unchanged, and the new pin test passes.

- [ ] **Step 4: Commit**

```bash
git add core
git commit -m "refactor: filter drops bike/drive tuning"
```

Body: "Every zone is walk or run now, so the fast-zone priors and rates are dead. sigma_a_bike is the fast model's noise in all zones; renamed sigma_a_fast. No behaviour change (pinned by test). Refs #184."

---

### Task 5: Android is walk and run only

**Files:**

- Modify: `android/app/src/main/java/dev/apgo2/ui/Components.kt:38-44`
- Modify: `android/app/src/main/java/dev/apgo2/NewGame.kt:53, 56, 332-333, 492`
- Modify: `android/app/src/main/java/dev/apgo2/ui/ApgoIcons.kt:14, 22, 138-139, 305, 324, 358-363`
- Modify: `android/app/src/main/java/dev/apgo2/ui/Palette.kt:194`
- Modify: `android/app/src/main/java/dev/apgo2/ui/HelpText.kt:12-25`
- Modify: `android/app/src/main/java/dev/apgo2/PlayScreen.kt:629` (comment)
- Test: `android/app/src/test/java/dev/apgo2/ui/HelpTextTest.kt`

**Interfaces:**

- Consumes: the apworld `MODES` tuple (Task 2) and the core FFI (Task 3; bindings regenerated by `just check-android`).
- Produces: `internal val MODES = listOf("walk", "run")` in `ui/Components.kt`. `PLAY_MODES` and `modeLabel` are gone.

- [ ] **Step 1: Write the failing test**

In `HelpTextTest.kt`, add a tuple helper next to `pyDict` and two tests:

```kotlin
// The items of the top-level `NAME = ("a", "b")` tuple literal in [source].
private fun pyTuple(
    source: String,
    name: String,
): List<String> =
    Regex("""^$name\s*=\s*\(([^)]*)\)""", RegexOption.MULTILINE)
        .find(source)
        ?.groupValues
        ?.get(1)
        ?.let { body -> Regex(""""([^"]+)"""").findAll(body).map { it.groupValues[1] }.toList() }
        ?: error("$name tuple not found in apworld constants.py")
```

```kotlin
    @Test
    fun theAppOffersExactlyTheApworldModes() {
        assertEquals(pyTuple(constants, "MODES"), MODES)
    }

    @Test
    fun helpNeverMentionsBikesOrCars() {
        for (topic in topics) {
            val text = topic.body.lowercase() // use HelpTopic's real text property
            for (word in listOf("bike", "biking", "car ", "drive")) {
                assertTrue("${topic.title} mentions $word", word !in text)
            }
        }
    }
```

(Check the `HelpTopic` field names before writing. If the car presence help is in `topics` and legitimately says "car", limit the check to `zones` and `travelMode`.)

Run: `just check-android`
Expected: FAIL (`MODES` has 4 entries; the help mentions bikes).

- [ ] **Step 2: Implement**

`Components.kt`, replacing lines 38-44:

```kotlin
/** The ways to travel a zone can be for. Same order as the apworld's MODES. */
internal val MODES = listOf("walk", "run")
```

`NewGame.kt`:

- Imports: `dev.apgo2.ui.MODES`. Drop `PLAY_MODES` and `modeLabel`.
- Line 332: `"Travel by ${zone.mode}"`.
- Line 333: `IconChoices(MODES, zone.mode, { onChange(zone.copy(mode = it)) }, ApgoIcons::mode) { it }`. Pass the label lambda the way `IconChoices` expects; check its signature.
- Line 492: `"Zone ${i + 1} ($mode)"`.

`ApgoIcons.kt`:

- Delete `val Bike` and `val Car` (138-139) and their imports (14, 22), unless the realm `"bike"` icon at 220 uses them. If it does, keep the import and reference `Lucide.Bike` there directly.
- Delete the `"plug_in"` (305) and `"pedal_pusher"` (324) entries.
- `mode()`:

```kotlin
    fun mode(mode: String): ImageVector = if (mode == "run") Run else Walk
```

`Palette.kt:194`: drop `"plug_in"` from the list.

`HelpText.kt`:

```kotlin
        HelpTopic(
            "Zones",
            "A zone is one of your realms, played walking or running. You start in zone 1. Every later zone" +
                " opens when you have found enough Zone Keys, " +
                "and a running zone after a walking one also needs Running Shoes. The same realm can be used for several zones.",
        )
    val travelMode =
        HelpTopic(
            "Travel by",
            "How you move in this zone: walking or running. It decides how long a quest takes (running " +
                "covers more ground in the same time). " +
                "A running zone needs Running Shoes to be unlocked.",
        )
```

`PlayScreen.kt:629` comment: `like "3key+Running Shoes"`.

- [ ] **Step 3: Run the tests**

Run: `just check-android`
Expected: PASS. Bindings are regenerated, `MODES` matches the apworld, and coverage is at least 17.

- [ ] **Step 4: Commit**

```bash
git add android
git commit -m "feat!: app offers walk and run only"
```

Body: "Drops the bike/car chips, icons and help, and the plug_in and pedal_pusher icons. A test pins MODES to the apworld's. Refs #184."

---

### Task 6: scripts and docs

**Files:**

- Modify: `scripts/ap_host.sh:3, 49` (`walk,bike,drive` becomes `walk,run`)
- Modify: `scripts/e2e_autoplay.sh:13` (regex `(walk|run)`)
- Modify: `CLAUDE.md:9, 35` (`apworld 0.4.0 (slot_data schema 4)`, `slot_data is schema 4`)
- Modify: `docs/context/v1-architecture-and-status.md:8, 16, 67-68, 75`
- Modify: `docs/context/archipelago-game-model.md:50, 56`
- Modify: `docs/context/location-estimation.md:140, 163, 168, 243, 264`
- Modify: `docs/context/project-decisions.md` (append a 2026-10-14 line: "PR 2: walk and run only, schema 4, goals are ids, location ids renumbered; Bike/Car item ids retired")

**Interfaces:** none.

- [ ] **Step 1: Edit**

- `ap_host.sh`:
  - Line 3: `APGO_ZONES=walk,run`.
  - Line 49: `zones="${APGO_ZONES:-walk,run}"`. This matches today's spirit of a multi-zone default and exercises the Running Shoes tool. The handoff suggested `walk`. Use `walk,run` so `just test-ap` still covers a tool unlock. Note this in the PR.
- `e2e_autoplay.sh:13`: `(walk|run)`.
- Docs:
  - Replace mode lists `walk, run, bike(, drive)` with `walk, run`.
  - "schema 3" becomes "schema 4"; `goals[{id,target}]` becomes `goals[id]`.
  - `{Walk|Run|Bike|Drive} Quest #n` becomes `{Walk|Run} Quest #n`.
  - Delete "core still has Drive".
  - `--mode walk|run|bike|drive` becomes `--mode walk|run`.
  - Leave historical design notes that already say "purged" as they are.

- [ ] **Step 2: Verify**

Run: `just check-hygiene` (markdownlint and friends)
Expected: PASS.

Run: `grep -rn "walk,bike\|bike|car\|schema 3" scripts CLAUDE.md docs/context`
Expected: no hits except historical decision-log lines.

- [ ] **Step 3: Commit**

```bash
git add scripts CLAUDE.md docs/context
git commit -m "docs: walk and run only, schema 4"
```

---

### Task 7: gates, review, PR

- [ ] **Step 1: Full gate**

Run: `just check`
Expected: PASS (py at least 99, rust at least 91, kotlin at least 17; real Archipelago generation included).

Run: `just test-ap`
Expected: PASS (compliance against the real server with `walk,run` zones). See [[archipelago-connection-high-priority]].

- [ ] **Step 2: Leftover sweep**

Run: `grep -rni "mode::bike\|mode::drive\|pedal_pusher\|plug_in\|goal_target\|GoalSpec\|GOAL_TARGET" apworld core android/app/src scripts --include=*.py --include=*.rs --include=*.kt --include=*.sh --include=*.json`
Expected: no hits (the generated `uniffi` file is refreshed by `just check-android`).

- [ ] **Step 3: Fresh reviewer**

Dispatch one fresh reviewer agent on opus over `git diff origin/main...HEAD`, with this plan and the spec. Apply one fix pass (TDD for any bug found) and rerun Step 1.

- [ ] **Step 4: PR**

Push (the pre-push gate runs for several minutes). Open a PR titled `feat!: walk and run only, slot_data schema 4` with:

- what changed per layer;
- the id table;
- "old seeds and saves are refused, by design";
- the `ap_host.sh` default note;
- `Closes #184`.

End the body with the Claude Code attribution line.

- [ ] **Step 5: After merge**

Close #184 if the PR did not. The next roadmap step is Beta 2 badges (#185).
