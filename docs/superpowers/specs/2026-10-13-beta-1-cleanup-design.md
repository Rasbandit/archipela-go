# Beta 1: Cleanup (walk and run, two goals)

_Date: 2026-10-13. Issue: #184. Parent: `2026-10-13-progression-beta-design.md` (roadmap step 1)._

## Goal

Shrink the game to what the progression beta builds on: travel modes **walk and run** only, and goals **Treasure Hunt**
and **Completionist** only. Pure deletion: no new behaviour, no legacy shims.

## Decisions

| Decision | Why |
| -- | -- |
| Full cut: no legacy enum variants, no save migration | Owner: do not carry removed things forward; all saves are dev games |
| Location ids renumbered: `block = difficulty_index * len(MODES) + mode_index`, Boss block right after (6) | Compact ids; no released seeds to keep |
| Item ids not renumbered: `Bike` (+3) and `Car` (+4) become unused gaps, never reused | Renumbering would shift every other item for no gain |
| slot_data schema 4; the core supports only 4; apworld version 0.4.0 | The mode enum and goal set changed |
| `GoalSelection` default becomes Treasure Hunt | Letter Hunt (the old default) is removed |
| Filler (`Hydrate!`, `Take a Breather!`) stays | Moved to Beta 3 (#186), where resource bundles replace it |
| Zones stay; `zone_modes` accepts walk and run | Zone code is dormant, not removed |

## Out of scope (stays as is)

- Bluetooth car **presence** (detecting that the player sits in a car) and its setup step.
- OSM street data: `way_class` BIKE/CAR bits and bicycle tags in `core/src/fill.rs`.
- Quest kind `rack_em_up` (bike racks are places, not a mode).
- Treasure Hunt's letters (A-P-G-O) and Boss Quest; Beta 8 (#191) turns them into Map Pieces and a dig.

## PR 1: two goals

| Layer | Change |
| -- | -- |
| apworld | `constants.py`: `GOALS = ("treasure_hunt", "all_trips")`, trim `GOAL_NAMES`, empty `GOAL_TARGET_OPTIONS`, `BOSS_GOALS = ("treasure_hunt",)`, delete `LETTER_GOALS`. `options.py`: delete the six `Goal*` target options and their groups; default Treasure Hunt; `GoalsRequired` range fits two goals. `validation.py`: `_LETTERS` keeps only `treasure_hunt`. `__init__.py`: drop the letter-only goal home (Menu) branch and target lookups. |
| core | `solo::GOALS` to two ids; `goal.rs` evaluation arms, YAML names and target options to two; `yaml.rs` names. Counters used only by removed goals (e.g. streak days, explorer cells, marathon km) are removed when nothing else reads them; keep any shared with quests, HUD or traps. |
| Android | `NewGame.kt` goal list, default and `GoalNumber`; `HelpText.kt` goal help; the streak HUD in `PlayScreen.kt` if it only served Daily Habit. |
| contract | `apworld/docs/contract.md`, `slot_data.schema.json`, `slot_data.sample.json` (goal `treasure_hunt`), `example.yaml`. |
| docs | `archipela-go-game-design.md` goals; any context doc listing the twelve goals. |

## PR 2: walk and run

| Layer | Change |
| -- | -- |
| apworld | `MODES = ("walk", "run")`; trim `MODE_SPEED_KMH`, `MODE_TOOLS` (run only), `FAMILY_MODES`; delete `Bike`/`Car` from `items.py`; location groups for Bike/Drive go; `SCHEMA_VERSION = 4`; `archipelago.json` 0.4.0. |
| core | `catalog.rs`: `Mode { Walk, Run }`, `ALL`, `PLAY`, `parse` (no "car"), `m_per_min`. `solo.rs`: tools, family filter, id math from `Mode::ALL.len()` (no literal 4 or 12). Location filter: delete bike/drive speed caps and noise (`loc/imm.rs`, `loc/params.rs`), mode masks (`loc/graph.rs`, keep the street bits), matcher/locator mode arms. `game.rs`: drop the Drive exemption in `speed_ok`. `slot.rs`: `CURRENT_SCHEMA = 4`, `SUPPORTED_SCHEMAS = [4]`. `ffi/src/engine.rs`: hardcoded tool list. Delete bike scenarios (`tests/loc_scenarios.rs`) and replay paths (`examples/replay.rs`). |
| catalog | `core/data/quest_catalog.json`: top-level `modes` and every kind's `modes` list to walk/run; delete `pedal_pusher` and `plug_in` (no walk or run mode). Regenerate `docs/context/quest-catalog.md` with `scripts/catalog_doc.py`. |
| Android | `ui/Components.kt` `MODES`, `PLAY_MODES`, `modeLabel`; mode icons in `ui/ApgoIcons.kt`; travel help in `ui/HelpText.kt`. |
| scripts | `scripts/ap_host.sh` default zones `walk`; `scripts/e2e_autoplay.sh` mode regex. |
| contract and docs | `contract.md`, schema and sample (zones and trips use walk/run only), `example.yaml`; context docs that list four modes; `CLAUDE.md` schema line. |
| goal target | With the schema 4 bump, slot_data `goals` becomes a list of ids: drop `GoalSpec.target`, the schema-2 `goal`/`goal_target` fields, the apworld `GOAL_TARGET_OPTIONS` and `(id, target)` tuples, and the schema `target` property. (PR 1 already removed it from the app input and solo options; found in the PR 1 code review.) |

## Old seeds and saves

No migration. Old slot_data fails the core's schema check ("unsupported slot_data schema"); old game files fail to load.
Acceptable: every existing game is a developer test game.

## Testing (TDD)

Write the failing tests first in each PR, then delete code until they pass.

- PR 1: apworld rejects every removed goal name in `goal_selection` and generates with each of the two goals and with
  both (any / all / at least 1); core `check_goal_specs` rejects removed ids; Android goal list shows two goals.
- PR 2: apworld rejects `bike` and `drive` in `zone_modes`; core `Mode::parse` rejects "bike", "drive", "car"; the
  catalog parses with only walk/run and contains no kind without a walk or run mode; slot_data schema 3 is rejected and
  4 accepted.
- Id agreement: one test per side pins the same table of (difficulty, mode, number) to location id, so the apworld and
  the core cannot drift.
- Gates: `just check` (real Archipelago generation included), `just test-ap`, coverage floors (py 99, rust 80,
  kotlin 17) still met.

## Risks

- **Coverage floors** can drop when well-tested code is deleted. Check `just check-py` and `just check-rust` coverage
  before pushing; add tests, never lower a floor.
- **Location filter tuning**: removing bike/drive caps must not change walk/run behaviour. The existing walk scenarios
  in `tests/loc_scenarios.rs` must pass unchanged.
- **Hidden mode uses**: the inventory found ~40 core test sites using `Mode::Bike`/`Drive`; tests that only used bike as
  "some other mode" switch to run.
