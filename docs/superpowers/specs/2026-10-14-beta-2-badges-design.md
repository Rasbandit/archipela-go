# Beta 2: Badge system (needs per quest)

_Date: 2026-10-14. Issue: #185 (also #53). Parent: `2026-10-13-progression-beta-design.md` (roadmap step 2)._

## Goal

Make tools gate **quests**, not zones. The apworld writes each quest's `needs` (a list of item names) into slot_data and
locks those locations in logic; the core only tests `needs ⊆ inventory`; the app draws the badge. The first and only badge
in this step is **Running Shoes**, on run quests, when YAML `run_quests` is on.

## Decisions

| Decision | Why |
| -- | -- |
| slot_data **schema 5**; the core supports only 5; apworld version 0.5.0 | An app built for 4 would ignore `needs` and let locked quests be claimed. Full cut, no shims |
| Run moves from zones to quests. Zones lose `mode` and `tool`; only keys gate a zone | One lock mechanism per item. Running Shoes gating zones *and* quests would be two rules for one item |
| YAML `zone_modes` is replaced by **`zone_count`** (1-6, default 1) | Zones no longer have a mode; multiple zones stay dormant (tabled) |
| `MODES` stays `("walk", "run")`; **location ids unchanged** | The id table in `apworld/tests/test_locations.py` and `core/src/solo.rs` does not move |
| `needs` is a **list**; the core tests the full subset | Two-badge quests (roadmap rule 2) work the day the generator emits them; no later schema bump |
| The generator emits only `[]` or `["Running Shoes"]` in Beta 2 | Only one tool exists until Beta 5 |
| **`run_share`** (1-50 %, default 20) instead of `badge_shares` | Run is an opt-in pace rule, so its count must not shrink as tools arrive. `badge_shares` lands in Beta 5 with the tools it splits (YAGNI) |
| A badged quest without its tool is **Locked** (same state as a locked zone) | Reuses the existing Locked path; matches the roadmap rule "without the tool the reward cannot be claimed" |
| `MODE_TOOLS` / `tool_for` become one badge table | One place for Gloves, Satchel... in Beta 5 |

## Out of scope

- `badge_shares` and the pyramid's two-badge tier (Beta 5).
- Any tool other than Running Shoes; resources; shop.
- Progression rims on pins (#96).
- Run pace rules themselves (time limit, speed band): they exist today per mode and are unchanged.
- Holding a completed-but-locked quest until the tool arrives (rejected option B).

## Contract (slot_data schema 5)

Diff from schema 4:

```text
schema_version: 5
zones[]:        {id, zone_keys_needed}                 (- mode, - tool)
trips[], boss:  {location_id, zone, mode, difficulty,  (+ needs)
                 effort_tier, type, needs: [string]}
```

`needs` is always present (empty list when open). `mode` is `"walk"` or `"run"`; a run quest has
`needs: ["Running Shoes"]`. The boss is always walk with `needs: []`.

## PR (a): contract and gate

### apworld

| File | Change |
| -- | -- |
| `options.py` | Delete `ZoneModes`. Add `ZoneCount` (Range 1-6, default 1), `RunQuests` (Toggle, default off), `RunShare` (Range 1-50, default 20). Option groups updated |
| `constants.py` | Delete `MODE_TOOLS`. Add `BADGES: dict[str, str] = {"run": "Running Shoes"}` (badge key -> item). `SCHEMA_VERSION = 5` |
| `zones.py` | `Zone` keeps `id`, `keys_needed`; drop `mode`, `tool`, `required_tools`; `build_zones(count)`. `items_to_enter` returns keys only |
| `distribution.py` | Generate every trip as walk. If `run_quests`: pick `max(1, round(n * run_share / 100))` trips with the world's seeded RNG (`sample`) among trips whose family allows run; set `mode="run"`, `needs=["Running Shoes"]`. `Quest` gains `needs: tuple[str, ...]` |
| `validation.py` | Drop `zone_modes` checks; validate `zone_count` against trips |
| `item_plan.py` | Running Shoes (1, progression) in the pool only when `run_quests` is on |
| `__init__.py` | Zone entrance rule = keys only. Each quest with `needs`: `set_rule(loc, lambda s: s.has_all(needs, player))` |
| `locations.py` | Add group `"Running Shoes"` (all run quest names); keep `"Run"` |
| `slot_data.py` | Zones `{id, zone_keys_needed}`; `quest_entry` adds `needs`; docstring "contract v5" |
| `archipelago.json` | 0.5.0 |
| docs | `contract.md`, `slot_data.schema.json`, `slot_data.sample.json` (one run quest with `needs`), `example.yaml` |

### core

| File | Change |
| -- | -- |
| `slot.rs` | `CURRENT_SCHEMA = 5`, `SUPPORTED_SCHEMAS = [5]`. `ZoneSlot` drops `mode`, `tool`. `QuestSlot` gains `needs: Vec<String>` |
| `game.rs` | `zone_unlocked` = keys only. New `quest_unlocked(q) = zone_unlocked(q.zone) && q.needs ⊆ items`; replaces `zone_unlocked` at quest-level call sites (quest state, chains, trackers). `QuestView` gains `needs` and `missing` (needs not held) |
| `assign.rs` | Targets and distance bands use the quest's mode, not the zone's |
| `solo.rs` | `SoloOptions`: `zone_modes` -> `zone_count`, add `run_quests`, `run_share`. Same pick rule as the apworld (own RNG; parity is the rule, not the exact picks). `tool_for` -> badge table. `is_beatable` uses the same subset test |
| `yaml.rs` | Export `zone_count`, `run_quests`, `run_share` |
| `ffi/` | Expose the new options and `QuestView.needs`/`missing`; drop the hardcoded zone tool list |

### Android (only what the API change forces)

- `NewGame.kt`: zone mode picker -> zone count; AP join reads zone count, not `zoneModes`.
- `ApController.kt`: `zoneModes` -> zone count.
- `PlayScreen.kt`: locked-zone text is keys only (`"3key"`).
- Badged quests show as Locked through the existing path.

## PR (b): badges visible

- `ui/ApgoIcons.kt`: `badge(item)`, one icon per badge item (Running Shoes -> the run icon). Colours are palette tokens.
- `QuestCard.kt`: one chip per `needs` item; a `missing` one is dimmed with "Needs Running Shoes".
- `QuestMap.kt`: a small badge overlay on the pin, done with a renderer expression on a feature property (no app-side
  recompute).
- `NewGame.kt` (solo): `run_quests` toggle and `run_share` slider; help text in `ui/HelpText.kt`.

## Old seeds and saves

No migration. Schema 4 slot_data fails with the existing "unsupported slot_data schema" error; old game files fail to load.

## Testing (TDD)

Failing tests first in each PR.

- **apworld:** options and validation; run count is `max(1, round(n * share / 100))` when on and 0 when off; badged
  locations are unreachable without Running Shoes; group `"Running Shoes"` exists; no Running Shoes in the pool when off;
  the sample validates against the schema; extreme fill (minimum trips, 50 %, 6 zones) never raises `FillError`; the id
  table test is unchanged and still passes.
- **core:** schema 4 is refused; the gate with 0, 1 and 2 `needs` items; `missing` is correct; solo picks follow the same
  count rule; `is_beatable` with run on; `assign` uses the quest mode.
- **Android:** unit test for the badge icon mapping and missing-chip state (PR b); emulator e2e smoke.
- **Gates:** `just check`, `just test-ap`, coverage floors (py 99, rust 91, kotlin 17).

## Docs

- `docs/context/project-decisions.md`: 2026-10-14 entry (schema 5, run per quest, `run_share`, Locked).
- `docs/context/progression-zones-and-tools.md`: zone tools are gone.
- Roadmap row 2: "schema 4" -> "schema 5", `badge_shares` -> `run_share` (badge_shares moves to row 5).
- `CLAUDE.md` schema line.
