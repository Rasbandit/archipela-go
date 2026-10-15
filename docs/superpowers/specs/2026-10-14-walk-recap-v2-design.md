# Walk recap v2 (progress bars, quest pins) design

Builds on `2026-10-13-walk-recap-design.md` (same branch, `feat/walk-recap`).

## Goal

Redesign the recap screen from the owner-approved preview canvas
(<https://claude.ai/artifact/SNuGCeuDk6ZfxJeZbehdTQ>, version 13, 2026-10-14):

1. Route map at the top, with a numbered pin for each quest completed on the walk.
2. Progress card: one animated bar per chain (steps, time out, explored) instead of stat tiles.
3. Quests completed card: tap a row or its pin to see the quest's details.
4. Items received, brought home, Done (unchanged).

## Decisions (owner, 2026-10-14)

- Remove the stat tiles. Distance and wall-clock time move to the subtitle: "[date] · 3.1 km · 42 min".
- **Bars.** Blue (`questTodo`) is the chain's progress before the walk, orange (`questProgress`) is what the walk added. They
  animate one bar at a time.
  - Milestone dots work as in `ChainBar`: green (`questDone`) once reached, hollow otherwise.
  - A dot the walk passed bounces and fires a small confetti burst at the moment the orange crosses it.
- **Bar fill motion.**
  - Linear speed, with a short soft landing over the last 12 % of the bar's time (speed stays continuous).
  - Fill time is proportional to how much of the bar is gained: a full bar takes 2600 ms, clamped to 450–2400 ms.
  - 380 ms pause between bars, and a gain label counting up with the fill.
- **Steps bar.** Under it: "4,812 walked · 4,090 counted", shown only when the two differ.
  - No multiplier and no "from X" text.
  - Counted steps are credited per step reading at that moment's backpack factor, never retroactively (this is already how
    `credit_steps` works). The recap keeps an exact tally rather than an average.
- **Quest rows.** Each shows a numbered badge (same number as its map pin), the name, the place and the time.
  - Tapping a row or its pin opens it to show the goal, the time done and the reward. One row is open at a time, and the open
    quest's pin grows and glows.
- **Reduced motion** (system animator scale 0): everything appears in its final state, with no confetti.

## Core changes (`core/src/game.rs`, `core/src/recap.rs`)

### Quests completed on the walk: recorded as they happen

`Game::complete()` (the one place a quest finishes) pushes onto the open outing:

```rust
pub struct OutingQuest {
    pub location_id: i64,
    pub t_ms: i64,
    pub at: Option<Point>,        // where the player was (`pos` passed to complete)
}
```

`Outing.quests: Vec<OutingQuest>` (`#[serde(default)]`). This replaces the `done_at_start` snapshot diff:

- `mark_checked` (a server sync) never calls `complete`, so synced checks stay out with no special case.
- The `done_at_start` field and its `mark_checked` hook are removed. They exist only on this unmerged branch, so no released
  save carries them.

`Recap.quests: Vec<RecapQuest>`, built on arrival from `Outing.quests` and the assignments:

```rust
pub struct RecapQuest {
    pub name: String,             // Assignment.quest_name
    pub place: String,            // Assignment.place
    pub goal: String,             // Assignment.target.goal_text(units)
    pub t_ms: i64,
    pub at: Option<Point>,
    pub reward: Option<String>,   // solo_rewards for solo games; None in Archipelago (the item belongs to the multiworld)
}
```

- Progressive chain members (steps, away and cells milestones) are left out of `Recap.quests`, because the bars show them.
- `quests_done: Vec<String>` stays: the notification text uses it, and it now comes from `Outing.quests` (all quests, chain
  members included).

### Exact counted steps

`Outing.counted_steps: f64` (`#[serde(default)]`). `credit_steps` adds `gained * step_factor()`, the same value it credits to
the chains, at the same moment. `Recap.counted_steps: i64` is that tally rounded.

### Chain progress before and after

`Outing.progress_at_start: BTreeMap<String, f64>` (`#[serde(default)]`) is set in `open_outing` from `counter_at(c, t)` for
every chain.

`Recap.chains: Vec<RecapChain>` is built on arrival with `counter_at(c, t_ms)`:

```rust
pub struct RecapChain {
    pub id: String,
    pub name: String,             // Chain.name
    pub unit: ChainUnit,          // steps | minutes | cells
    pub before: f64,
    pub after: f64,
    pub marks: Vec<f64>,          // Milestone.at, in order
}
```

- Only chains whose value went up are included, ordered steps → minutes → cells, then by zone.
- A chain with no `progress_at_start` entry (one that appeared mid-walk) counts from 0.

### Worth showing

Unchanged, plus `!chains.is_empty()` as another reason a walk is worth a recap.

## FFI (`core/ffi/src/engine.rs`)

`RecapOut` gains:

- `counted_steps: i64`;
- `chains: Vec<RecapChainOut>` (with `unit` as a "steps" | "minutes" | "cells" string, like `ChainOut`);
- `quests: Vec<RecapQuestOut>` (`at: Option<GeoPoint>`).

## Android

- **`RecapTimeline` (new, pure Kotlin, unit-tested):** given each bar's gain fraction, returns its start time and duration, using
  the constants above. `softLand(t, tail = 0.12)` gives the fill progress. One `Animatable` clock drives all bars; each bar reads
  its own progress from it.
- **`RecapChainBar` (new, `ui/`):** a Canvas bar drawn in the `ChainBar` style.
  - Draws the before fill in `questTodo`, the gain fill in `questProgress`, and the milestone dots.
  - A dot crossed during the animation starts a spring scale bounce and a confetti burst of about 14 Canvas particles. The burst
    colours are palette tokens: `questProgress`, `questDone`, `sky`, `butter`, `questTodo`, `grass`.
  - `ChainBar` itself stays unchanged.
- **`RecapFormat`:**
  - The subtitle, plus per-bar text: "+4,090 steps", "2,140 → 6,230 total", "next: 7,500" or "All milestones reached".
  - The walked/counted line.
  - Quest row text and detail rows.
  - The stat tiles go away.
- **`RecapScreen`, in order:**
  1. Title and subtitle.
  2. The map, 200 dp: route plus quest pins as a MapLibre symbol layer from GeoJSON (`n` property, selected one bigger via a
     filter or feature-state). A map click runs `queryRenderedFeatures` on the pin layer, which selects or unselects that quest.
  3. The progress card.
  4. The quests card: rows are buttons with `aria`-style semantics (expanded or collapsed), and the open row shows the details.
  5. Items received.
  6. Brought home.
  7. Done.
- Selection is screen state (`rememberSaveable` of the open quest's index); the core is not involved.
- Strings go in `ui/HelpText.kt` or `RecapFormat` (same rule as v1). Colours are palette tokens only.

## Out of scope

- Multipliers other than the backpack (boosts).
- Sharing the recap.
- A history of past walks.
- iOS.

## Testing

- **Core (TDD):**
  - A quest completed mid-walk records its time and point.
  - A synced check (`mark_checked`) is not recorded.
  - Chain members are excluded from `Recap.quests` but counted in `quests_done`.
  - `reward` is `Some` for solo games and `None` for Archipelago.
  - `counted_steps` equals the chain credit with an overloaded backpack, and a pickup mid-walk lowers only the later readings.
  - `progress_at_start` → `RecapChain` before and after for steps, minutes and cells.
  - Chains with no gain are left out, and the order is steps → minutes → cells.
  - A chain-only gain makes the walk worth showing.
  - Old outings without the new fields load.
- **FFI:** `RecapOut` carries chains, quests and counted steps.
- **Android unit tests:**
  - `RecapTimeline`: durations proportional to the gain, the clamps, sequential starts, `softLand` ends exactly at 1 and is
    continuous.
  - `RecapFormat`: the texts above.
  - The pin GeoJSON builder: `n` and selected properties, quests with no point skipped.
- **Device check:** the owner tests once the work is done: animation feel, confetti, pin taps, scrolling over the map.

## Build order

1. Core: `OutingQuest` + `Recap.quests` (replaces the `done_at_start` diff).
2. Core: `counted_steps` + `progress_at_start` → `Recap.chains`, plus the worth-showing rule.
3. FFI: the `RecapOut` additions.
4. Android: `RecapTimeline` + `RecapFormat` texts (pure, tested).
5. Android: `RecapChainBar` (fills, dots, bounce, confetti) + progress card.
6. Android: map at top with quest pins + selection, and the quests card with expandable rows.
7. Android: screen order, subtitle, stat tiles removed; docs (`v1-architecture-and-status.md`).
