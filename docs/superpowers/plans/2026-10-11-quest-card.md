# Quest Card Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Play quest popup with the agreed quest card: a one-line goal headline on a state-coloured badge, the
quest name, meta row, detail line, place row, a persistent labelled progress block and a fold-out "About this place".

**Architecture:** All words and choices are made in the Rust core: a new `card` module turns an `Assignment` (plus its
live `QuestView`) into headline, detail, place and progress text; `poi_details` gains an `about` sheet. The FFI hands
Kotlin ready strings on `QuestOut` (cheap, no atlas read) and an `about(location_id)` call read lazily off the main thread.
Kotlin only lays the card out with palette tokens.

**Tech Stack:** Rust (apgo-core, apgo-ffi/UniFFI), Kotlin/Compose, JUnit.

**Spec:** `docs/superpowers/specs/2026-10-11-quest-card-design.md` (mock: Quest Card Lab artifact v24).

## Global Constraints

- Logic and wording in the core; Kotlin draws only (cross-platform hard rule). Every shown distance in the player's units via
  `units.rs`.
- No atlas read when building `QuestOut`; the about sheet is read lazily with the game lock released.
- Old games must work: new `Assignment` fields are `#[serde(default)]`; the existing relink fills them.
- Colours only from `ApgoPalette` (`scripts/check_color_tokens.sh`).
- Headline one line (`maxLines = 1`, ellipsis). Words the player can't see are not used ("squares", "cells", "blocks").
- Conventional commits, subject < 50 chars, session attribution footer. Coverage floors only go up.

## Review Focus

1. A line quest whose saved `coverage` is not a friendly fraction (old quests, until #140): headline must state it exactly
   ("Walk 37% of the trail"), never round. → Task 1 test.
2. Boss quests (`kind_id = "the_big_one"`) whose target is a line/park/dwell: wording follows the target, not the kind. →
   Task 1 test.
3. Courier picked up but the time ran out (tracker resets `picked_at`): the card must fall back to step ① with no stale
   "time left". → Task 2 test.
4. Old game with no `place_type` until relink lands: the place row shows the name alone, no " · " dangling. → Task 3/5.
5. A place whose about sheet is empty after a rescan: the fold-out row must not appear, or must show nothing broken when
   opened. → Task 4 test (`about` → `None`).

---

### Task 1: Core `card` module — headline and detail

**Files:**

- Create: `core/src/card.rs` (+ `pub mod card;` in `core/src/lib.rs`, alphabetical)
- Test: same file

**Interfaces:**

- Consumes: `Assignment`, `Target`, `units::{distance_rounded, Round, UnitSystem}`, `geo::{polyline_len_m, bearing_deg, distance_m, Point}`.
- Produces: `pub struct CardText { pub headline: String, pub detail: String }`,
  `pub fn card_text(a: &Assignment, home: Point, units: UnitSystem) -> CardText`,
  `pub fn line_share_phrase(coverage: f64) -> Option<&'static str>`, `pub const SIT_KINDS`, `pub const TITLED_KINDS`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assign::{Assignment, Target};
    use crate::catalog::Mode;
    use crate::geo::{destination, Point};
    use crate::units::UnitSystem::Imperial;

    fn home() -> Point { Point::new(40.0, -111.0) }

    fn quest(kind: &str, target: Target) -> Assignment {
        Assignment { location_id: 1, zone: 1, mode: Mode::Walk, family: "x".into(), kind_id: kind.into(), quest_name: "Q".into(),
            blurb: String::new(), place: "P".into(), feature_id: None, place_type: None, has_about: false, tier: 2,
            effort_min: 10.0, target, fallback: false, boss: kind == "the_big_one" }
    }
    fn text(a: &Assignment) -> (String, String) { let c = card_text(a, home(), Imperial); (c.headline, c.detail) }
    fn line(len_m: f64, coverage: f64) -> Target {
        Target::Line { pts: vec![home(), destination(home(), 0.0, len_m)], corridor_m: 25.0, coverage }
    }

    #[test]
    fn places_get_short_headlines_and_the_numbers_below() {
        assert_eq!(text(&quest("gallery_walls", Target::Point { p: home(), r: 40.0 })), ("Get here".into(), "Within 130 ft".into()));
        assert_eq!(text(&quest("bench_warmer", Target::Dwell { p: home(), r: 40.0, minutes: 3.0 })).0, "Sit 3 min");
        assert_eq!(text(&quest("swing_time", Target::Dwell { p: home(), r: 40.0, minutes: 3.0 })),
            ("Stay 3 min".into(), "Within 130 ft · leaving pauses the timer".into()));
        assert_eq!(text(&quest("touch_grass", Target::DwellArea { poly: vec![], center: home(), r: 60.0, minutes: 1.0 })),
            ("Hang out 1 min".into(), "Anywhere inside the outline".into()));
    }

    #[test]
    fn line_quests_say_a_friendly_share_and_the_distance() {
        assert_eq!(text(&quest("trail_boss", line(4506.0, 0.5))), ("Walk half the trail".into(), "1.4 mi of 2.8 mi · either direction".into()));
        assert_eq!(text(&quest("trail_boss", line(4506.0, 0.25))).0, "Walk a quarter of the trail");
        assert_eq!(text(&quest("pedal_pusher", line(4506.0, 1.0))), ("Ride the whole path".into(), "All 2.8 mi · either direction".into()));
        assert_eq!(text(&quest("full_circle", line(4506.0, 0.75))).0, "Walk three quarters of the loop");
        assert_eq!(text(&quest("stairmaster", line(60.0, 0.8))), ("Climb all the stairs".into(), "All 190 ft · up or down".into()));
    }

    #[test]
    fn a_share_that_is_not_a_friendly_fraction_is_stated_exactly() {
        assert_eq!(text(&quest("trail_boss", line(4506.0, 0.37))).0, "Walk 37% of the trail");
    }

    #[test]
    fn boss_wording_follows_its_target() {
        assert_eq!(text(&quest("the_big_one", line(4506.0, 0.5))).0, "Walk half the trail");
        assert_eq!(text(&quest("the_big_one", Target::Point { p: home(), r: 40.0 })).0, "Get here");
    }

    #[test]
    fn quests_without_a_place_read_plainly() {
        let ne = destination(home(), 45.0, 1300.0);
        assert_eq!(text(&quest("compass_rose", Target::Point { p: ne, r: 40.0 })), ("Head north-east".into(), "0.8 mi out, then within 130 ft".into()));
        assert_eq!(text(&quest("street_smarts", Target::Point { p: ne, r: 40.0 })), ("Get here".into(), "Within 130 ft of the pin".into()));
        assert_eq!(text(&quest("step_up", Target::Steps { n: 2000 })), ("Walk 2,000 steps".into(), "Any pace, anywhere".into()));
        assert_eq!(text(&quest("wanderlust", Target::Away { minutes: 60.0 })), ("Be out 1 hour".into(), "Time away from home".into()));
        assert_eq!(text(&quest("wanderlust", Target::Away { minutes: 90.0 })).0, "Be out 1 h 30 min");
        assert_eq!(text(&quest("cartographer", Target::Cells { n: 8, cell_m: 150.0 })), ("Explore somewhere new".into(), "Walk where you haven't been yet".into()));
        assert_eq!(text(&quest("special_delivery", Target::Courier { a: home(), b: ne, r: 40.0, time_limit_min: 10.0 })),
            ("Deliver the package".into(), "Pick up at A, drop at B in 10 min".into()));
        assert_eq!(text(&quest("there_and_back", Target::RoundTrip { far: ne, r: 50.0 })), ("Out and back".into(), "Touch the far point, then come home".into()));
        let collect = Target::Collect { pts: vec![ne], need: 5, r: 25.0, theme: "acorns".into() };
        assert_eq!(text(&quest("forager", collect)), ("Collect 5 acorns".into(), "Bring them home to bank".into()));
    }
}
```

(The `Assignment` literal names `place_type` and `has_about`, added in Task 3; until then add them to `Assignment` as
`#[serde(default)] pub place_type: Option<String>` and `#[serde(default)] pub has_about: bool` in this task's Step 3 so the
tests compile, and set them in the four existing `Assignment { .. }` literals (`assign.rs` `one()`, `chain.rs`, `fog.rs`,
`goal.rs` test helpers) to `None` / `false`.)

- [ ] **Step 2: Run them — expect compile failure (`card_text` missing)**

Run: `cd core && cargo test -p apgo-core --lib card::`

- [ ] **Step 3: Implement**

```rust
//! What the quest card says: a short headline (one line) and one detail line, from the quest itself, in the player's units.

use crate::assign::{Assignment, Target};
use crate::geo::{bearing_deg, distance_m, polyline_len_m, Point};
use crate::units::{distance, distance_rounded, Round, UnitSystem};

/// The card's headline and the grey line under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardText {
    /// What to do, short enough for one line.
    pub headline: String,
    /// The numbers and conditions behind it.
    pub detail: String,
}

/// Dwell kinds where you sit ("Sit 3 min"); other dwells say "Stay".
pub const SIT_KINDS: [&str; 3] = ["bench_warmer", "picnic_break", "under_the_roof"];
/// Kinds whose places are titled works, shown in quotes on the place row.
pub const TITLED_KINDS: [&str; 3] = ["gallery_walls", "mural_mural", "remember_when"];

/// The friendly words for a line share, when it is one of ¼ ⅓ ½ ⅔ ¾ or all.
#[must_use]
pub fn line_share_phrase(coverage: f64) -> Option<&'static str> {
    const SHARES: [(f64, &str); 6] =
        [(0.25, "a quarter of"), (1.0 / 3.0, "a third of"), (0.5, "half"), (2.0 / 3.0, "two thirds of"), (0.75, "three quarters of"), (1.0, "the whole")];
    SHARES.iter().find(|(f, _)| (coverage - f).abs() < 0.005).map(|(_, p)| *p)
}

/// "1 hour", "1 h 30 min", "45 min".
fn duration(minutes: f64) -> String {
    let m = minutes.round() as i64;
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (1, 0) => "1 hour".into(),
        (h, 0) => format!("{h} hours"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// 8 compass words from a bearing in degrees.
fn compass_word(deg: f64) -> &'static str {
    const W: [&str; 8] = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
    W[(((deg.rem_euclid(360.0) + 22.5) / 45.0) as usize) % 8]
}

/// The card text of quest `a` (with `home` for direction words).
#[must_use]
pub fn card_text(a: &Assignment, home: Point, units: UnitSystem) -> CardText {
    let within = |r: f64| format!("Within {}", distance_rounded(r, units, Round::Down));
    let (headline, detail) = match &a.target {
        Target::Point { p, r } if a.kind_id == "compass_rose" => (
            format!("Head {}", compass_word(bearing_deg(home, *p))),
            format!("{} out, then {}", distance(distance_m(home, *p), units), within(*r).to_lowercase()),
        ),
        Target::Point { r, .. } if a.kind_id == "street_smarts" || a.fallback => ("Get here".into(), format!("{} of the pin", within(*r))),
        Target::Point { r, .. } => ("Get here".into(), within(*r)),
        Target::Dwell { r, minutes, .. } => (
            format!("{} {minutes:.0} min", if SIT_KINDS.contains(&a.kind_id.as_str()) { "Sit" } else { "Stay" }),
            format!("{} · leaving pauses the timer", within(*r)),
        ),
        Target::DwellArea { minutes, .. } => (format!("Hang out {minutes:.0} min"), "Anywhere inside the outline".into()),
        Target::Line { pts, coverage, .. } => line_text(&a.kind_id, polyline_len_m(pts), *coverage, units),
        Target::Courier { time_limit_min, .. } => ("Deliver the package".into(), format!("Pick up at A, drop at B in {time_limit_min:.0} min")),
        Target::RoundTrip { .. } => ("Out and back".into(), "Touch the far point, then come home".into()),
        Target::Cells { .. } => ("Explore somewhere new".into(), "Walk where you haven't been yet".into()),
        Target::Steps { n } => (format!("Walk {} steps", thousands(u64::from(*n))), "Any pace, anywhere".into()),
        Target::Away { minutes } => (format!("Be out {}", duration(*minutes)), "Time away from home".into()),
        Target::Collect { need, theme, .. } => (format!("Collect {need} {theme}"), "Bring them home to bank".into()),
    };
    CardText { headline, detail }
}

fn line_text(kind: &str, len_m: f64, coverage: f64, units: UnitSystem) -> (String, String) {
    if kind == "stairmaster" {
        return ("Climb all the stairs".into(), format!("All {} · up or down", distance(len_m, units)));
    }
    let (verb, noun) = match kind {
        "pedal_pusher" => ("Ride", "path"),
        "full_circle" => ("Walk", "loop"),
        "ridge_runner" => ("Walk", "ridge"),
        "follow_the_flow" => ("Follow", "river"),
        "towpath_tour" => ("Walk", "towpath"),
        _ => ("Walk", "trail"),
    };
    let headline = match line_share_phrase(coverage) {
        Some("the whole") => format!("{verb} the whole {noun}"),
        Some(p) => format!("{verb} {p} the {noun}"),
        None => format!("{verb} {:.0}% of the {noun}", coverage * 100.0),
    };
    let detail = if coverage >= 0.995 {
        format!("All {} · either direction", distance(len_m, units))
    } else {
        format!("{} of {} · either direction", distance(len_m * coverage, units), distance(len_m, units))
    };
    (headline, detail)
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
```

Check `units::distance` output for the expected strings (`1.4 mi`, `2.8 mi`, `190 ft`); adjust the test lengths, not the
formatter, if rounding differs (the formatter is shared and already tested).

- [ ] **Step 4: Run** `cd core && cargo test -p apgo-core --lib card:: && cargo clippy -p apgo-core --all-targets -- -D warnings` → PASS.
- [ ] **Step 5: Commit** `feat: quest card headline and detail text`

---

### Task 2: Core `card` progress

**Files:** Modify `core/src/card.rs`, `core/src/verify.rs` (accessor), `core/src/game.rs` (`QuestView` fields + builder).

**Interfaces:**

- Consumes: `QuestView { state, progress, target, collected, .. }`.
- Produces: `pub struct Progress { pub label: String, pub value: String, pub frac: f32, pub hint: Option<String>, pub steps: Vec<String>, pub step_at: u8 }`,
  `pub fn card_progress(q: &QuestView, now_ms: i64, units: UnitSystem) -> Option<Progress>`,
  `QuestView.picked_at_ms: Option<i64>` (from `Tracker::picked_at()`).

- [ ] **Step 1: Failing tests** (in `card.rs` tests; build a `QuestView` with a helper `view(target, state, progress)`):
  - dwell 3 min at progress 0.4 → label "Time here", value "1:12 of 3:00"; at 0 → "0:00 of 3:00"; done → value "Done", frac 1.
  - dwell_in_area → "Time inside".
  - line 2.8 mi, coverage 0.5, progress 0.4 → label "Traveled", value "0.6 of 1.4 mi" (one unit for both).
  - steps 2000 at 0.4 → "800 of 2,000"; away 60 min at 0.4 → "24 of 60 min"; cells n=8 at 0.375 → label "New ground", value "38%".
  - collect: need 5, banked 1, carried 2 → label "Banked", value "1 of 5 acorns", hint `Some("Carrying 2")`.
  - courier: progress 0 → steps ["Pick up", "Deliver"], step_at 0, hint "10 min to deliver once picked up";
    progress 0.5 with `picked_at_ms = now - 200_000` → step_at 1, hint "6:40 left to deliver";
    progress 0 after a timeout (`picked_at_ms: None`) → step_at 0 (Review Focus 3).
  - round trip: 0 / 0.5 / done → step_at 0 / 1 / 2, steps ["Reach the far point", "Back home"].
  - reach (Point) → `None`.
- [ ] **Step 2: Run → fail.**
- [ ] **Step 3: Implement** `card_progress` with a `clock(minutes)` helper (`m:ss`), the same `distance` unit for both numbers
  of a line (format the target with `units::distance`, then the done amount in the target's unit — add
  `units::distance_in_unit_of(m, like_m, units)` if needed, with its own test in `units.rs`), and:
  - `verify.rs`: `pub fn picked_at(&self) -> Option<i64>` on `Tracker` (courier state only).
  - `game.rs`: `QuestView.picked_at_ms` filled from the tracker in `quest_views`.
- [ ] **Step 4: Run** `cargo test -p apgo-core --lib card:: verify:: game::` → PASS.
- [ ] **Step 5: Commit** `feat: quest card progress text`

---

### Task 3: Core place row and about sheet

**Files:** Modify `core/src/assign.rs` (`one()` sets `place_type`, `has_about`), `core/src/game.rs` (`relink_places` sets them
too), `core/src/poi_details.rs` (`About`, `about()`), `core/src/card.rs` (`place_row`).

**Interfaces:**

- Produces: `pub struct About { pub description: Option<String>, pub rows: Vec<(String, String)>, pub wikipedia_url: Option<String> }`,
  `pub fn about(f: &Feature, units: UnitSystem) -> Option<About>` (`None` when all empty),
  `pub fn has_about(f: &Feature) -> bool`, `pub struct PlaceRow { pub name: Option<String>, pub kind: Option<String>, pub quoted: bool }`,
  `pub fn place_row(a: &Assignment) -> Option<PlaceRow>` (`None` for quests with no place).

- [ ] **Step 1: Failing tests**
  - `poi_details`: rows in the fixed order (Kind, Artist, Made, Faith, Food, Species, Material, Surface, Difficulty, Elevation,
    Height, Lighting, Hours, Entry, Run by, Address), capped at 6; "Kind" skipped when it equals the place type; Entry joins
    fee / private access / wheelchair; elevation and height in player units; `about` of a bare bench → `None`;
    `has_about` matches `about(..).is_some()`.
  - `assign`: a bench quest gets `place_type = Some("Bench")`, `has_about = false`; an artwork with `wikipedia` gets
    `has_about = true`; a street quest gets `None`/`false`.
  - `game::relink_places`: an old quest gets `place_type` and `has_about` with its `feature_id`.
  - `card::place_row`: titled kinds quoted; unnamed place (place == kind name) → name `None`, kind "Bench"; no
    `place_type` (old game before relink) → kind `None` (Review Focus 4); free quest → `None`.
- [ ] **Step 2: Run → fail.**
- [ ] **Step 3: Implement** — rows table as in the mock (`ROWS` in the Quest Card Lab source, ported to Rust with the existing
  `poi_details` helpers `humanize`, `wikipedia_url`, `address`, `SAC_SCALE`); `one()` and `relink_places` call
  `poi_details::details(f).type_label` and `has_about(f)` on the chosen feature.
- [ ] **Step 4: Run** `cargo test -p apgo-core` → PASS (whole crate: `Assignment` changed).
- [ ] **Step 5: Commit** `feat: quest card place row and about sheet`

---

### Task 4: FFI

**Files:** Modify `core/ffi/src/engine.rs`.

**Interfaces (Kotlin sees camelCase):**

- `QuestOut` gains `headline: String`, `detail: String`, `place: Option<PlaceOut { name: Option<String>, kind: Option<String>, quoted: bool }>`,
  `progress: Option<ProgressOut { label, value, frac: f32, hint: Option<String>, steps: Vec<String>, stepAt: u8 }>`, `hasAbout: Boolean`
  (replaces `hasDetails`).
- `Engine::about(location_id: i64) -> Option<AboutOut { description: Option<String>, rows: Vec<AboutRowOut { label, value }>, wikipediaUrl: Option<String> }>`
  replaces `poi_details` / `PoiDetailsOut` (lazy atlas read, lock released, as today).

- [ ] **Step 1: Failing engine tests**: `quests()` carries headline/detail/progress for the test game's reach quests
  ("Get here", no progress); after setting a bench feature + `has_about`, `about(id)` returns rows; rescanned away → `None`
  (Review Focus 5); `hasAbout` false for street quests. Rework the existing `poi_details_*` test into `about_*`.
- [ ] **Step 2: Run → fail.**
- [ ] **Step 3: Implement** in `quests()`: `card_text(&a, g.home, units)`, `card_progress(&q, now_ms, units)`, `place_row`.
  `quests()` needs the `Assignment` per view: add `QuestView.place_type`/`has_about`/`fallback` already present —
  build `place_row` from the view (add `place_type`, `has_about` to `QuestView`) so `quests()` stays view-only.
- [ ] **Step 4: Run** `just check-rust` → PASS.
- [ ] **Step 5: Commit** `feat: quest card fields over the FFI`

---

### Task 5: Android quest card

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/QuestCard.kt` (the card composables)
- Modify: `PlayScreen.kt` (`QuestPopup` uses `QuestCard`), `PlayDetails.kt` (remove `QuestDetails`, keep `CollectItems` only if
  still used), `ui/Palette.kt` (`difficulty(d, dark)` tokens), `ui/ApgoIcons.kt` (`Target`, `ChevronDown`, `ExternalLink`, `Clock`
  imports as needed), test fixtures that build `QuestOut` (new fields).

**Layout (spec order):** badge 40dp (state colour `ApgoPalette.quest(state)`, white kind icon, done check) + headline
(19sp, `maxLines = 1`, ellipsis) over quest name (14sp, muted), centred on the badge; close button absolutely top-right
(`Box` + `Alignment.TopEnd`, 6dp inset); meta row (difficulty pill, `~N min`, mode icon + word) full width; detail line
(`maxLines = 1`); place row (bold name — quoted when `quoted` — then " · " + lowercased kind; kind alone when no name);
progress block (label/value row, `LinearProgressIndicator` in the state colour, hint) or the two-step tracker; "About this
place ▾" row only when `hasAbout`, expanding a `Column` loaded with `withContext(Dispatchers.IO) { engine.about(id) }`:
description, a two-column label/value list, "Read more on Wikipedia ↗" via `LocalUriHandler` wrapped in `runCatching`.

- [ ] **Step 1:** Regenerate bindings (`bash scripts/android_bindings.sh`), update `QuestOut(...)` fixtures, write
  `QuestCard.kt`, wire `QuestPopup`.
- [ ] **Step 2:** `just check-android` → PASS (spotless, detekt, lint, tests, kover).
- [ ] **Step 3:** Device check on the phone (`just android-run`): one of each — Get here (artwork with Wikipedia), Sit
  (bench), Hang out (park), trail (fraction or exact %), forager, steps, away, courier (pick up then watch time left), round
  trip; light and dark theme; About opens/closes; no ⓘ anywhere. Screenshot the set.
- [ ] **Step 4: Commit** `feat: quest card in the Play popup`

---

### Task 6: Docs and finish

- [ ] `docs/context/ui-design-system.md`: "Play quest popup" rules (badge = state, headline one line, detail line, place row,
  progress block, About fold-out); `v1-architecture-and-status.md` Play line.
- [ ] Remove the old `Target::goal_text` only if nothing else uses it (traps/near-miss text may; keep it otherwise).
- [ ] `just check` (py skipped if untouched), PR "feat: quest card", body with screenshots and the follow-up issues.
