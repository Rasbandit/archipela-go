# Beta 1 PR 1: Two Goals Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut the game's win conditions from twelve to two, Treasure Hunt (`treasure_hunt`) and Completionist (`all_trips`), in the apworld, the Rust core and the Android app.

**Architecture:** Pure deletion behind new tests. The apworld (Python) defines goal ids, YAML names and options; the core mirrors them (`solo::GOALS`, `goal.rs` evaluation, `yaml.rs` YAML writer, `solo.rs` solo generator); Android lists them on the New Game screen and in `HelpText.kt`. Goal combining (any / all / at least N) stays. Treasure Hunt keeps its A-P-G-O letters and Boss Quest.

**Tech Stack:** Python 3 apworld (Archipelago 0.6.8, pytest via `uv`), Rust core (`apgo-core`, `apgo-ffi`, UniFFI), Kotlin/Compose (JUnit, Gradle).

**Spec:** `docs/superpowers/specs/2026-10-13-beta-1-cleanup-design.md` (section "PR 1: two goals"). Parent: `docs/superpowers/specs/2026-10-13-progression-beta-design.md`. Issue #184.

## Global Constraints

- Goals kept: `treasure_hunt` (YAML name "Treasure Hunt") and `all_trips` (YAML name "Completionist"), in that order; default goal Treasure Hunt.
- Removed goal ids: `macguffin_short`, `macguffin_long`, `boss`, `zone_conqueror`, `well_rounded`, `quest_dex`, `marathon`, `explorer`, `streak`, `boss_rush`. No legacy handling.
- Removed YAML options: `goal_zone_conqueror_percent`, `goal_quest_dex_kinds`, `goal_marathon_kilometers`, `goal_explorer_cells`, `goal_streak_days`, `goal_boss_rush_hard_quests`.
- Do NOT touch travel modes, Bike/Car, zones, filler items (`Hydrate!`, `Take a Breather!`) or the slot_data `schema_version` (that is PR 2).
- Keep `GoalSpec.target` / `GoalPickIn.target` fields (always 0 now); keep the distance and streak **stats** shown in the Play summary line (they are player stats, not goals).
- TDD: write the failing test first. Tests that exist only for a removed goal are deleted; tests that used a removed goal as "some goal" switch to `treasure_hunt` or `all_trips`. Never weaken an assertion about kept behaviour.
- Conventional commits, subject under 50 characters, body lines at most 72 characters, ending with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Coverage floors only go up: py 99, rust 80, kotlin 17.
- Fresh worktree: run `just setup-ap` once before any Python test.

## Review Focus

- A YAML naming a removed goal (e.g. `goal_selection: [Letter Hunt]`) must fail generation with a clear "unknown goal" `OptionError`, not a crash. Test in Task 1.
- A YAML still carrying a removed target option (e.g. `goal_streak_days: 7`) is an unknown option to Archipelago; generation must fail loudly, not silently ignore it. Covered by Archipelago itself; Task 1 removes the options so the example YAML no longer lists them.
- Two goals with `require_at_least_n_goals` and `goals_required: 2` must generate and be beatable; `goals_required: 3` must be refused. Test in Task 1 (apworld) and Task 2 (core `check_goal_specs`).
- Slot data naming a removed goal must be refused by the core (`check_goal_specs` "unknown goal"), not shown as "Unknown goal". Test in Task 2.
- The app's own solo games (core `solo.rs`) with the default options must generate Treasure Hunt with 4 letters and a Boss Quest and be beatable. Test in Task 2.

---

### Task 1: apworld and contract keep two goals

**Files:**

- Modify: `apworld/ap_go2/constants.py:14-59` (goal tables)
- Modify: `apworld/ap_go2/options.py:17-147, 280-285, 305-320` (goal options and groups)
- Modify: `apworld/ap_go2/validation.py:11-15` (`_LETTERS`)
- Modify: `apworld/ap_go2/__init__.py:18, 150-155, 180-195` (letter-goal home, target lookup)
- Modify: `apworld/docs/slot_data.schema.json:38-53`, `apworld/docs/slot_data.sample.json:3-8`, `apworld/docs/example.yaml:5-13`, `apworld/docs/contract.md:4-5, 21-31, 80, 87-113`
- Modify: `core/src/slot.rs:194, 210` (the two sample-goal assertions, so core stays green with the new sample)
- Test: `apworld/tests/test_constants.py`, `apworld/tests/test_validation.py`, `apworld/tests/test_item_plan.py`, `apworld/tests/test_slot_data.py`, `apworld/tests/world/test_world.py`

**Interfaces:**

- Consumes: nothing.
- Produces: `constants.GOALS == ("treasure_hunt", "all_trips")`, `constants.GOAL_NAMES == {"treasure_hunt": "Treasure Hunt", "all_trips": "Completionist"}`, `constants.GOAL_TARGET_OPTIONS == {}`, `constants.BOSS_GOALS == ("treasure_hunt",)`; `constants.LETTER_GOALS` no longer exists. Task 3's `HelpTextTest` reads `GOAL_NAMES` from this file.

- [ ] **Step 1: Write the failing tests**

Add to `apworld/tests/test_constants.py`:

```python
def test_only_the_two_beta_goals_exist() -> None:
    assert constants.GOALS == ("treasure_hunt", "all_trips")
    assert constants.GOAL_NAMES == {"treasure_hunt": "Treasure Hunt", "all_trips": "Completionist"}
    assert constants.BOSS_GOALS == ("treasure_hunt",)
    assert constants.GOAL_TARGET_OPTIONS == {}
    assert not hasattr(constants, "LETTER_GOALS")
```

Add to `apworld/tests/test_validation.py`:

```python
REMOVED_GOAL_NAMES = [
    "Letter Hunt",
    "Letter Hunt XL",
    "The Big One",
    "Zone Conqueror",
    "Well Rounded",
    "Quest-dex",
    "Marathon",
    "Explorer",
    "Daily Habit",
    "Boss Rush",
]


@pytest.mark.parametrize("name", REMOVED_GOAL_NAMES)
def test_removed_goals_are_unknown(name: str) -> None:
    with pytest.raises(ValueError, match="unknown goal"):
        goal_ids_from_selection([name])


def test_the_two_goals_are_read_from_their_yaml_names() -> None:
    assert goal_ids_from_selection(["completionist", "Treasure Hunt"]) == ["treasure_hunt", "all_trips"]
```

Add to `apworld/tests/world/test_world.py` (import `ApGo2Options` from `worlds.ap_go2.options` alongside `GoalSelection`):

```python
def test_goal_selection_defaults_to_treasure_hunt_and_target_options_are_gone() -> None:
    assert GoalSelection.default == frozenset({"Treasure Hunt"})
    fields = set(ApGo2Options.__dataclass_fields__)
    assert not {f for f in fields if f.startswith("goal_") and f not in {"goal_selection", "goal_requirement"}}


class TestCompletionistOnly(Base):
    options = {"goal_selection": ["Completionist"], "number_of_trips": 30}  # noqa: RUF012

    def test_no_letters_and_no_boss(self) -> None:
        names_in_pool = {i.name for i in self.multiworld.itempool}
        assert not {n for n in names_in_pool if n.startswith("Letter ")}
        assert self.world.fill_slot_data()["boss"] is None


class TestBothGoalsAtLeastTwo(Base):
    options = {  # noqa: RUF012
        "goal_selection": ["Treasure Hunt", "Completionist"],
        "goal_requirement": "require_at_least_n_goals",
        "goals_required": 2,
        "number_of_trips": 30,
    }

    def test_slot_data_lists_both_goals(self) -> None:
        data = self.world.fill_slot_data()
        jsonschema.validate(data, SCHEMA)
        assert [g["id"] for g in data["goals"]] == ["treasure_hunt", "all_trips"]
        assert (data["goal_requirement"], data["goal_need"]) == ("at_least", 2)


def test_removed_goal_names_are_not_valid_keys() -> None:
    assert GoalSelection.valid_keys == frozenset({"Treasure Hunt", "Completionist"})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `uv run --project apworld pytest apworld/tests/test_constants.py apworld/tests/test_validation.py apworld/tests/world/test_world.py -q -k "beta_goals or removed_goal or two_goals or defaults_to_treasure or Completionist or BothGoals"`
Expected: FAIL (GOALS has 12 entries, "Letter Hunt" is still known, default is Letter Hunt).

- [ ] **Step 3: Cut the goal tables in `constants.py`**

Replace the `GOALS` through `GOAL_TARGET_OPTIONS` block (lines 14-59) with:

```python
GOALS = ("treasure_hunt", "all_trips")
BOSS_GOALS = ("treasure_hunt",)  # goals that add the Boss Quest location

# Player-facing names of the goals, as written in a YAML's goal_selection.
GOAL_NAMES: dict[str, str] = {
    "treasure_hunt": "Treasure Hunt",
    "all_trips": "Completionist",
}

# Goals that count something, and the option holding that count. None in the beta.
GOAL_TARGET_OPTIONS: dict[str, str] = {}

# How the chosen goals combine into winning (slot_data `goal_requirement`).
REQUIREMENTS = ("any", "all", "at_least")
```

Delete `MAX_GOAL_TARGET` if nothing else imports it after Step 4 (`grep -rn MAX_GOAL_TARGET apworld`).

- [ ] **Step 4: Cut the options in `options.py`**

- `GoalSelection` docstring becomes:

```python
    """What will be your goal(s)? Choose one or both.

    - **Treasure Hunt**: collect the letters of APGO, then finish the Boss Quest.
    - **Completionist**: finish every quest.

    Both are checked by the app once the last zone can be reached.
    """
```

- `default = frozenset({GOAL_NAMES["treasure_hunt"]})`.
- Delete the classes `GoalZoneConquerorPercent`, `GoalQuestDexKinds`, `GoalMarathonKilometers`, `GoalExplorerCells`, `GoalStreakDays`, `GoalBossRushHardQuests`, their six fields in `ApGo2Options`, and their six entries in the "Goal Selection" `OptionGroup` (which keeps `GoalSelection`, `GoalRequirement`, `GoalsRequired`).
- `GoalsRequired.range_end = len(GOAL_NAMES)` stays (now 2; default 2 still fits). Drop `MAX_GOAL_TARGET` from the import line.

- [ ] **Step 5: Cut letters and letter-goal logic**

`validation.py` lines 11-15:

```python
_LETTERS = {"treasure_hunt": "APGO"}
```

`letters_needed_by_logic` keeps its rule; with only Treasure Hunt holding letters its docstring stays true.

`__init__.py`:

- Line 18 import becomes `from .constants import BOSS_GOALS, GAME_NAME, GOAL_TARGET_OPTIONS`.
- In `create_regions`, replace

```python
        home = menu if all(g in LETTER_GOALS for g in self.goals) else regions[-1]
```

with

```python
        home = regions[-1]
```

- `fill_slot_data` keeps its `GOAL_TARGET_OPTIONS` lookup unchanged (the dict is empty, so every target is 0).

- [ ] **Step 6: Update the existing tests that used removed goals**

Apply exactly this table (delete = remove the test function; switch = replace the goal id or name, keep every other assertion):

| File | Test / line | Action |
| -- | -- | -- |
| `test_constants.py:15-16` | asserts on `GOALS[0]` and `BOSS_GOALS` | delete those two lines (the new test covers them) |
| `test_validation.py:34-55` | letter-count tests for `boss`, the counting goals, `macguffin_short`, `macguffin_long` | keep only `assert goal_letter_counts("treasure_hunt") == {"Letter A": 1, "Letter P": 1, "Letter G": 1, "Letter O": 1}` and `assert goal_letter_counts("all_trips") == {}` |
| `test_validation.py:98-122` | `mandatory_count` / `min_trips` with `macguffin_*` | switch to `"treasure_hunt"`; expected numbers: letters 4 instead of 11 for `macguffin_long` (recompute: `mandatory_count("treasure_hunt", ["walk"]) == 4`; `min_trips("treasure_hunt", ["walk"]) == 4`) |
| `test_validation.py:132-135` | several-goal letter counts | replace with `assert goal_letter_counts(["treasure_hunt", "all_trips"]) == goal_letter_counts("treasure_hunt")` |
| `test_validation.py:139-142` | `goal_ids_from_selection` with Quest-dex etc. | delete (new test covers it) |
| `test_validation.py:154-158` | `letters_needed_by_logic` | replace with: `assert letters_needed_by_logic(["treasure_hunt"], "any")`; `assert not letters_needed_by_logic(["treasure_hunt", "all_trips"], "any")`; `assert not letters_needed_by_logic(["treasure_hunt", "all_trips"], "at_least")`; `assert letters_needed_by_logic(["treasure_hunt", "all_trips"], "all")` |
| `test_validation.py:162-167` | `check_requirement` | switch ids to `["treasure_hunt", "all_trips"]` (need 2 ok, 3 refused) and `["treasure_hunt"]` |
| `test_validation.py:171-173` | `check(goal=[...])` | switch to `["treasure_hunt", "all_trips"]` and `["treasure_hunt", "nope"]` |
| `test_item_plan.py:15, 28, 47` | default goal, loop over goals, `plan(goal="boss")` | default `"treasure_hunt"`; loop `("all_trips", "treasure_hunt")`; `plan(goal="all_trips")` (no letters) |
| `test_slot_data.py:31` | `"boss" if boss else "all_trips"` | `"treasure_hunt" if boss else "all_trips"` |
| `test_slot_data.py:120-128` | goals `quest_dex`, `marathon`, `boss` | goals `[("treasure_hunt", 0), ("all_trips", 0)]` and the expected list to match |
| `world/test_world.py:38-46` | `TestDefaultSeed.test_goal_needs_all_letters` | keep (default is now Treasure Hunt, which needs letters) and add `assert self.world.fill_slot_data()["boss"] is not None` in `test_slot_data_valid`, replacing `assert data["boss"] is None` |
| `world/test_world.py:61, 106, 241, 274, 308` | `macguffin_short`, `boss`, `macguffin_long`, `"Letter Hunt", "The Big One"` | switch to `GOAL_NAMES["treasure_hunt"]` (boss-tests: Treasure Hunt has the boss) or `GOAL_NAMES["all_trips"]`; `["Treasure Hunt", "Completionist"]` for the two-goal case |
| `world/test_world.py` any remaining `GOAL_NAMES[...]` loop over all goals | parametrized over `GOALS` | stays; it now runs twice |

After the edits, `grep -rn "macguffin\|quest_dex\|zone_conqueror\|marathon\|explorer\|streak\|boss_rush\|well_rounded\|Letter Hunt\|The Big One" apworld` must print nothing except the new `REMOVED_GOAL_NAMES` list.

- [ ] **Step 7: Update the contract files**

- `slot_data.schema.json`: the goal `id` enum becomes `["treasure_hunt", "all_trips"]`; `goals` `maxItems` and `goal_need` `maximum` become 2 if they are 12.
- `slot_data.sample.json` lines 3-8: `"id": "treasure_hunt"`.
- `example.yaml` lines 5-13: `goal_selection: [Treasure Hunt, Completionist]`, keep `goal_requirement` and `goals_required: 2`, delete the six `goal_*` target lines.
- `contract.md`: line 4-5 "goal boss" becomes "goal treasure_hunt"; the `goals` row says "array, 1-2"; `goal_need` "int 1-2"; the `boss` row says "non-null when `treasure_hunt` is among the goals"; the Letter row says "Counted for Treasure Hunt (A, P, G, O)"; lines 91-94 drop the six counting-goal options; lines 110-113 keep the letter rule with "Treasure Hunt" in place of "letter goals". Remove any goal table rows for removed goals in the Goals section.
- `core/src/slot.rs` lines 194 and 210: `GoalSpec { id: "treasure_hunt".into(), target: 0 }`; in the schema-2 test (line 205-208) replace `\"goal\": \"boss\"` with `\"goal\": \"treasure_hunt\"`.

- [ ] **Step 8: Run the apworld suite and the core slot tests**

Run: `just check-py` then `cd core && cargo test -q -p apgo-core slot::`
Expected: PASS, coverage at or above 99 for Python.

- [ ] **Step 9: Commit**

```bash
git add apworld core/src/slot.rs
git commit -m "feat: apworld keeps two goals" -m "Treasure Hunt and Completionist remain; ten goals and their
six target options are removed. Contract docs follow." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: core keeps two goals

**Files:**

- Modify: `core/src/solo.rs:19-33` (`GOALS`), `:137` (default goal), `:172-191` (`letters_for`, `has_boss`)
- Modify: `core/src/goal.rs:1-240` (ctx, names, targets, evaluation) and its tests `:240-428`
- Modify: `core/src/yaml.rs:1-70` and its tests
- Modify: `core/src/game.rs:616-627` (`goal_ctx`)
- Test: `core/src/solo.rs` tests (`:440-600`), `core/src/goal.rs` tests, `core/src/yaml.rs` tests, `core/src/slot.rs` tests

**Interfaces:**

- Consumes: the contract from Task 1 (`example.yaml`, `slot_data.sample.json` with goal `treasure_hunt`).
- Produces: `pub const GOALS: [&str; 2] = ["treasure_hunt", "all_trips"]` in `solo.rs`; `goal_yaml_name(id) -> &'static str` for the two ids; `goal_target_option` deleted; `GoalCtx { slot, assignments, done, items }` (fields `distance_m`, `cells_discovered`, `streak_days`, `units` removed). `Game::streak_days` and `stats.distance_m` stay (HUD stats).

- [ ] **Step 1: Write the failing tests**

In `core/src/slot.rs` tests:

```rust
    #[test]
    fn removed_goals_are_refused() {
        for id in ["macguffin_short", "macguffin_long", "boss", "zone_conqueror", "well_rounded", "quest_dex", "marathon", "explorer", "streak", "boss_rush"] {
            let goals = vec![GoalSpec { id: id.into(), target: 0 }];
            let err = check_goal_specs(&goals, GoalMode::Any, 1).unwrap_err();
            assert!(err.contains("unknown goal"), "{id}: {err}");
        }
        let both = vec![GoalSpec { id: "treasure_hunt".into(), target: 0 }, GoalSpec { id: "all_trips".into(), target: 0 }];
        assert!(check_goal_specs(&both, GoalMode::AtLeast, 2).is_ok());
        assert!(check_goal_specs(&both, GoalMode::AtLeast, 3).is_err());
    }
```

In `core/src/solo.rs` tests:

```rust
    #[test]
    fn only_the_two_beta_goals_exist_and_treasure_hunt_is_the_default() {
        assert_eq!(GOALS, ["treasure_hunt", "all_trips"]);
        assert_eq!(SoloOptions::default().goal, "treasure_hunt");
        let g = generate(&SoloOptions::default(), 1).unwrap();
        assert!(g.slot.boss.is_some(), "Treasure Hunt needs its Boss Quest");
        assert_eq!(g.rewards.values().filter(|r| r.starts_with("Letter ")).count(), 4);
        assert!(is_beatable(&g));
    }
```

In `core/src/yaml.rs` tests:

```rust
    #[test]
    fn yaml_names_only_the_two_goals_and_no_target_options() {
        let y = build_yaml("Me", &SoloOptions::default());
        assert!(y.contains("goal_selection: [Treasure Hunt]"), "{y}");
        assert!(!y.contains("goal_streak_days") && !y.contains("goal_quest_dex_kinds"), "{y}");
        let g = |id: &str| GoalSpec { id: id.into(), target: 0 };
        let both = SoloOptions { goals: vec![g("treasure_hunt"), g("all_trips")], goal_mode: GoalMode::AtLeast, goal_need: 5, ..SoloOptions::default() };
        let y = build_yaml("Me", &both);
        assert!(y.contains("goal_selection: [Treasure Hunt, Completionist]") && y.contains("goals_required: 2"), "{y}");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd core && cargo test -q -p apgo-core removed_goals_are_refused only_the_two_beta_goals yaml_names_only_the_two_goals`
Expected: FAIL (12 goals known, default `macguffin_short`, YAML lists target options).

- [ ] **Step 3: Cut the goal list and solo defaults (`solo.rs`)**

```rust
/// Ids of the win conditions a game can use.
pub const GOALS: [&str; 2] = ["treasure_hunt", "all_trips"];
```

In `SoloOptions::default`: `goal: "treasure_hunt".into(),`. Replace `letters_for` and `has_boss` with:

```rust
fn letters_for(goal: &str) -> &'static str {
    if goal == "treasure_hunt" {
        "APGO"
    } else {
        ""
    }
}

fn has_boss(goal: &str) -> bool {
    goal == "treasure_hunt"
}
```

- [ ] **Step 4: Cut evaluation (`goal.rs`)**

- `GoalCtx` keeps only `slot`, `assignments`, `done`, `items`; delete the `units` import and `distance_rounded`/`Round` import.
- `goal_yaml_name`:

```rust
pub fn goal_yaml_name(id: &str) -> &'static str {
    match id {
        "treasure_hunt" => "Treasure Hunt",
        "all_trips" => "Completionist",
        _ => "",
    }
}
```

- Delete `goal_target_option` and `or_default`.
- `evaluate_one` keeps exactly the `"all_trips"`, `"treasure_hunt"` and `other` arms; delete the rest (and the now-unused `done_assign` closure and `count_f32` import if unused — let `cargo clippy` say).
- `letter_progress`, `letters`, `letters_needed` stay (Treasure Hunt uses them).

- [ ] **Step 5: Cut the YAML writer (`yaml.rs`)**

- Import only `goal_yaml_name`.
- Delete the `target` closure and the six `goal_*` lines and named arguments (`zc`, `qd`, `mk`, `ec`, `sd`, `br`) from the `format!`.
- `need = o.goal_need.clamp(1, 12)` becomes `need = o.goal_need.clamp(1, crate::num::count_u32(goals.len()).max(1))`.

- [ ] **Step 6: Shrink the goal context in `game.rs`**

```rust
    fn goal_ctx(&self) -> GoalCtx<'_> {
        GoalCtx { slot: &self.slot, assignments: &self.assignments, done: &self.done, items: &self.items }
    }
```

Drop the `now_ms` parameter and fix its callers (`cargo build` lists them). Keep `streak()` and `Game::streak_days` (HUD).

- [ ] **Step 7: Update the existing core tests that used removed goals**

| File | Test | Action |
| -- | -- | -- |
| `goal.rs` | `eval(...)` helper and the `GoalCtx {..}` literals at ~306, ~357, ~410 | drop the `dist`, `cells`, `streak` parameters and fields; callers pass only `s, done, items` |
| `goal.rs` | `a_slot_without_a_goals_list_plays_its_single_goal_exactly_as_before` | switch `"boss"` to `"all_trips"` and adjust its expected label/progress to the Completionist arm |
| `goal.rs` | `any_of_…`, `all_of_…`, `at_least_n_…`, `each_goal_reports_…`, `an_unusable_combination_is_refused` | switch goal ids to `treasure_hunt` and `all_trips`; keep the combining assertions |
| `goal.rs` | `letters_goals_count_duplicates` | keep, using `treasure_hunt` (APGO) |
| `goal.rs` | `all_trips_boss_and_treasure` | keep the `all_trips` and `treasure_hunt` parts, delete the `boss` part |
| `goal.rs` | `marathon_progress_reads_in_the_players_units`, `counting_goals_use_target_or_their_default` | delete |
| `solo.rs` | `several_goals_together_…` | goals `treasure_hunt` + `all_trips`; letters `"APGO".len()`; first id `treasure_hunt` |
| `solo.rs` | `a_goal_list_is_checked_like_a_single_goal` | `opts(.., "treasure_hunt")`; the pair becomes `treasure_hunt` + `all_trips` |
| `solo.rs` | `counts_ids_…`, `reward_table_…`, `options_are_validated_…`, `extreme_shares_…` | replace `"boss"`, `"macguffin_short"`, `"macguffin_long"`, `"boss_rush"` with `"treasure_hunt"` or `"all_trips"` (use `treasure_hunt` where the test asserts a boss or letters; for `options_are_validated…` "too small" use `opts(&[Mode::Walk], 3, "treasure_hunt")`, which needs 4) |
| `yaml.rs` | `modes_and_lists_are_rendered_…` | keep as is (goal already `treasure_hunt`) |
| `yaml.rs` | `several_goals_use_the_apworlds_names_requirement_and_targets` | delete (the new YAML test replaces it) |

Then `grep -rn "macguffin\|quest_dex\|zone_conqueror\|\"marathon\"\|\"explorer\"\|\"streak\"\|boss_rush\|well_rounded" core` must print only the `removed_goals_are_refused` list.

- [ ] **Step 8: Run the core suite**

Run: `just check-rust`
Expected: PASS (clippy clean, rust coverage at or above 80).

- [ ] **Step 9: Commit**

```bash
git add core
git commit -m "feat: core keeps two goals" -m "solo::GOALS, evaluation and the YAML writer follow the apworld;
the goal context drops the stats only removed goals read." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Android shows two goals

**Files:**

- Modify: `android/app/src/main/java/dev/apgo2/NewGame.kt:82-113` (`GOALS`, `GoalNumber`, `GOAL_NUMBERS`), `:142-143` (form defaults), `:431-483` (goal section, `GoalDetail`)
- Modify: `android/app/src/main/java/dev/apgo2/ui/HelpText.kt:60-86` (`goalTarget`, `goalDescriptions`)
- Test: `android/app/src/test/java/dev/apgo2/ui/HelpTextTest.kt`

**Interfaces:**

- Consumes: `apworld/ap_go2/constants.py` `GOAL_NAMES` from Task 1 (read by `HelpTextTest`); FFI `GoalPickIn(id, target)` unchanged (target always `0u`).
- Produces: nothing later tasks use.

- [ ] **Step 1: Write the failing test**

`HelpTextTest.everyGoalIsExplainedUnderTheNameTheApworldUses` already compares `Help.goalDescriptions` with the apworld's `GOAL_NAMES`; after Task 1 it fails. Add one more assertion test so the order and default are pinned:

```kotlin
    @Test fun onlyTheTwoBetaGoalsAreExplained() {
        assertEquals(listOf("treasure_hunt", "all_trips"), Help.goalDescriptions.keys.toList())
    }
```

and remove `goalTarget,` from the `topics` list.

- [ ] **Step 2: Run it to verify it fails**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.ui.HelpTextTest' --console=plain -q`
Expected: FAIL (12 goals described; `goalTarget` unresolved after Step 3 only).

- [ ] **Step 3: Cut the help text (`HelpText.kt`)**

Delete `val goalTarget = ...` and make `goalDescriptions`:

```kotlin
    val goalDescriptions: Map<String, HelpTopic> =
        mapOf(
            "treasure_hunt" to HelpTopic("Treasure Hunt", "Collect APGO, then finish the boss quest to claim the treasure."),
            "all_trips" to HelpTopic("Completionist", "Finish every quest in the game."),
        )
```

- [ ] **Step 4: Cut the New Game goal UI (`NewGame.kt`)**

```kotlin
// The win conditions, in the game's own order: id, title.
private val GOALS =
    listOf(
        "treasure_hunt" to "Treasure Hunt",
        "all_trips" to "Completionist",
    )
```

- Delete `GoalNumber`, `GOAL_NUMBERS`, `NewGameForm.targets`, the `targets`/`onTarget` parameters of the goal section, the `GoalDetail` composable and its call (`goals.forEach { id -> GoalDetail(...) }`).
- Form default: `val goals = mutableStateListOf("treasure_hunt")`.
- The pick list sends `GoalPickIn(it, 0u)`.

- [ ] **Step 5: Run the Android checks**

Run: `just check-android`
Expected: PASS (spotless, detekt, lint, unit tests, kover at or above 17).

- [ ] **Step 6: Commit**

```bash
git add android
git commit -m "feat: New Game offers two goals" -m "Treasure Hunt (default) and Completionist; the goal number field
goes, since no goal counts anything now." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: docs and the whole-repo gate

**Files:**

- Modify: `docs/context/archipela-go-game-design.md:26-80` (goal list), `docs/context/quest-types-and-phone-apis.md:59, 90, 115`, `docs/context/working-in-this-repo.md:57`, `docs/context/economy-eggs-buddies.md:81`, `scripts/ap_host.sh:3` (APGO_GOALS example)
- Test: `just check`

**Interfaces:**

- Consumes: Tasks 1-3.
- Produces: the PR.

- [ ] **Step 1: Update docs that list the twelve goals**

In each file above, replace a twelve-goal list or a removed goal used as an example with the two beta goals, and add one line where the list was: "Beta 1 (2026-10-13) kept Treasure Hunt and Completionist; the other goals are tabled in `docs/superpowers/specs/2026-10-13-progression-beta-design.md`." In `scripts/ap_host.sh` the example becomes `APGO_GOALS="Treasure Hunt,Completionist"`. `docs/context/upstream-architecture.md` describes the upstream app; leave it.

- [ ] **Step 2: Run the full gate**

Run: `just check` and `just test-ap`
Expected: PASS everywhere; coverage floors met.

- [ ] **Step 3: Commit, push, open the PR**

```bash
git add docs scripts
git commit -m "docs: goal lists follow Beta 1" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git push -u origin HEAD
gh pr create --title "feat: Beta 1 PR 1, keep two goals" --body "Part of #184 (Beta 1). Treasure Hunt and Completionist remain; ten goals, six target options and the goal number field are removed across apworld, core and Android. Spec: docs/superpowers/specs/2026-10-13-beta-1-cleanup-design.md.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```
