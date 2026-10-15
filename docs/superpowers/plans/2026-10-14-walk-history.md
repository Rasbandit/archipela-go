# Walk History + Forager Progress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Forager bars in the recap; every recapped walk kept in the journal and opened from an Activity row; start/end times
in the recap subtitle.

**Architecture:** The outing snapshots each forager's banked/picked counts; `recap_of` builds `Recap.foragers`. On arrival the
engine stores the recap JSON in a new journal `walks` table and logs a `walk` event (the Activity row); `walk_recap(end_ms)`
reads it back. Android draws forager bars with `RecapChainBar`, makes `walk` rows tappable, and opens history recaps without
touching the core.

**Tech Stack:** Rust (`apgo-core`, `apgo-ffi`, rusqlite, serde_json, UniFFI), Kotlin/Compose.

**Spec:** `docs/superpowers/specs/2026-10-14-walk-history-design.md`

## Global Constraints

- Decisions in the core; Kotlin draws. Events, not polling. New saved fields `#[serde(default)]`; old games and journals load.
- Journal writes never block arrival: failures go to `note()`; game lock released before the journal is locked.
- Forager bar: blue `questTodo` banked before, orange `questProgress` banked this walk, one mark at `need`; text "Pinecones ·
  +2 banked · 5 / 7"; "1 still in your bag" when carried > 0 after arrival. Replaces the "Brought home" section.
- Subtitle: "Tue 10/14 · 5:12 – 5:54 PM · 3.1 km · 42 min" (weekday + short date of start, start–end times in the phone's
  12/24 h format, distance, time out).
- History recap animates like the popup; its button says "Close"; closing never calls `dismiss_recap`.
- Strings in `ui/HelpText.kt` or the format objects; colours only palette tokens.
- Conventional commits, subject < 50 chars, body ≤ 72, trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- `just check-rust` and `ANDROID_HOME=~/Android/Sdk just check-android` pass; no new lint suppressions.

## Review Focus

1. A forager completed by banking on arrival shows banked_after == need and its mark crosses (confetti).
2. A trap blocks banking on arrival: banked unchanged, `carried` > 0 → "still in your bag".
3. The journal opened from an older file (no `walks` table) gains it and keeps working.
4. A second arrival (no new recap) must not add a second walk or `walk` event.
5. Opening a history walk while a real pending recap exists must not dismiss the pending one.

Tests: 1, 2 → Task 1; 3, 4 → Task 2; 5 → Task 4.

---

### Task 1: Forager progress in the recap (core + FFI)

**Files:** `core/src/recap.rs`, `core/src/game.rs`, `core/ffi/src/engine.rs` (+ tests).

**Interfaces — produces:**

- `Outing.forage_at_start: BTreeMap<i64, (u32, u32)>` (banked, picked count) `#[serde(default)]`, set in `open_outing` from
  `self.collected` (`(c.banked, u32::try_from(c.picked.len()))`).
- `pub struct RecapForager { pub name: String, pub theme: String, pub need: u32, pub banked_before: u32, pub banked_after:
  u32, pub picked: u32, pub carried: u32 }` (serde, `Debug, Clone, PartialEq`); `Recap.foragers: Vec<RecapForager>`
  `#[serde(default)]`.
- `recap_of`: for each assignment with `Target::Collect { need, theme, .. }` and an entry in `collected`: before = start entry
  or (0, 0); include when banked or picked count changed; `picked` = picked count now − picked before (saturating);
  `carried` = `c.carried`. Order: assignment order.
- FFI: `RecapForagerOut` (same fields), `RecapOut.foragers`.

- [ ] **Step 1: Failing tests** in `game.rs` tests (use `forager_game`, `left_home`, `fixat`, `freeze`):
  - `a_forager_banked_on_arrival_shows_before_and_after`: leave, pick two items, arrive → one `RecapForager` with
    `banked_before == 0`, `banked_after == 2`, `picked == 2`, `carried == 0`, `need` = the quest's need, `theme` set.
  - `a_forager_completed_at_home_reaches_its_need`: pick `need` items, arrive → `banked_after == need`.
  - `items_still_carried_after_a_blocked_bank_show_in_the_recap`: pick one, `freeze(&mut g)`, arrive (with 900 outing steps
    so it is worth showing) → `banked_after == banked_before`, `carried == 1`.
  - `a_forager_with_no_start_entry_counts_from_zero`: clear `forage_at_start` mid-walk → `banked_before == 0`.
  - FFI: extend the recap engine test or add one asserting `pending_recap().foragers` maps the fields.
- [ ] **Step 2: Run, expect compile failure.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cd core && cargo fmt --all && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings &&
  cargo test -p apgo-core -p apgo-ffi && cd .. && just check-rust`
- [ ] **Step 5: Commit** `feat: forager progress in the walk recap`

---

### Task 2: Walk history in the journal (core + FFI)

**Files:** `core/src/journal.rs`, `core/ffi/src/engine.rs` (+ tests).

**Interfaces — produces:**

- `SCHEMA` gains `CREATE TABLE IF NOT EXISTS walks (id INTEGER PRIMARY KEY, game TEXT NOT NULL, end_ms INTEGER NOT NULL,
  recap TEXT NOT NULL); CREATE INDEX IF NOT EXISTS walks_game_end ON walks (game, end_ms);`.
- `kind::WALK: &str = "walk"` (doc comment).
- `Journal::add_walk(&self, game: &str, r: &Recap) -> rusqlite::Result<()>` (serde_json; map a serde error to
  `rusqlite::Error::ToSqlConversionFailure(Box::new(e))`), `Journal::walk(&self, game: &str, end_ms: i64) ->
  rusqlite::Result<Option<Recap>>` (newest row for that end_ms; a row that fails to parse → `Ok(None)`), and `clear_game`
  also `DELETE FROM walks WHERE game = ?1`.
- `pub fn walk_summary(r: &Recap, units: UnitSystem) -> String` in `core/src/recap.rs`: "3.1 km · 42 min · 2 quests"
  (distance via the crate's existing distance formatter for `units`, minutes as "N min" or "H h M min", quest count from
  `quests_done.len()` with singular "1 quest", omitted when 0).
- Engine `bank_at_home`: when `ready` (a new recap stored), in the same `journal_do` after logging the bank events:
  `j.add_walk(&game_id, &r)` and `j.log(&game_id, &JournalEvent { t_ms: r.end_ms, kind: kind::WALK.into(), detail:
  walk_summary(&r, units), at: None })` — clone the recap before it moves into the game, or build these from it inside the
  game-lock pass and carry them out. Units: the game's units (find how `journal_events` gets them; add a small accessor if
  needed).
- `Engine::walk_recap(&self, end_ms: i64) -> Option<RecapOut>`: game id like `activity` (open game, else last game); read via
  the journal; errors → `note`, `None`.

- [ ] **Step 1: Failing tests.**
  - journal: `add_walk`/`walk` round-trip (equal `Recap`); other game or other end_ms → `None`; `clear_game` removes it; a
    journal created with only the old tables (create the old schema by hand in a temp file, or open, `DROP TABLE walks`,
    reopen) gains `walks` on open.
  - recap: `walk_summary` for metric with 2 quests ("3.1 km · 42 min · 2 quests"), 1 quest, 0 quests, ≥ 60 min.
  - engine: arriving with a worth-showing walk adds one `walk` activity row (`activity()`) whose `t_ms == end_ms`, and
    `walk_recap(end_ms)` returns the recap; a second arrival adds no row; `dismiss_recap` leaves `walk_recap` working; a
    restarted engine (same dir) still returns it.
- [ ] **Step 2–5:** run (fail), implement, run Rust checks + `just check-rust`, commit `feat: keep each recapped walk`.

---

### Task 3: Forager bars, sample foragers, subtitle times (Android)

**Files:** `RecapFormat.kt`, `RecapScreen.kt`, `RecapSample.kt`, `ui/HelpText.kt` (+ tests). Run
`bash scripts/android_bindings.sh` first.

**Interfaces — produces:**

- `RecapFormat.foragerLine(f: RecapForagerOut, banked: Double): String` → "Pinecones · +2 banked · 5 / 7" (theme
  capitalised; banked is the animated value, rounded); `RecapFormat.bagLine(f): String?` → "1 still in your bag" /
  "2 still in your bag" or null when 0; `sections(r)` drops "Brought home".
- `RecapFormat.subtitle(r: RecapOut, time: java.text.DateFormat = DateFormat.getTimeInstance(DateFormat.SHORT), day:
  java.text.DateFormat = SimpleDateFormat("EEE M/d", Locale.getDefault())): String` →
  "Tue 10/14 · 5:12 – 5:54 PM · 3.1 km · 42 min"; `RecapScreen` passes `android.text.format.DateFormat.getTimeFormat(ctx)`
  for `time`. (When both times share the AM/PM marker it is fine to repeat it; keep it simple: "5:12 PM – 5:54 PM" is
  acceptable if the platform format includes the marker — test pins a format, e.g. `SimpleDateFormat("h:mm")` for start and
  `SimpleDateFormat("h:mm a")` for end, by taking two formats if needed.)
- Progress card: after the chain bars, one block per `r.foragers`: theme label + live `foragerLine`, `RecapChainBar(before =
  bankedBefore, value = animated banked, max = need, marks = listOf(need), crossedFrom = bankedBefore, animate)`, then
  `bagLine` when non-null. Forager bars join the same timeline after the chains (`RecapTimeline.slots` over chains then
  foragers, gain fraction = (after − before) / need). The card shows when chains or foragers are non-empty.
- `RecapSample`: add the two foragers from the spec (pinecones need 7, 5 → 7, picked 2; leaves need 7, 3 → 4, picked 1),
  `banked` = 3.

- [ ] **Step 1: Failing tests:** `foragerLine`/`bagLine` texts; `subtitle` with pinned formats and a fixed time zone
  (`TimeZone.setDefault` in the test or formats with an explicit zone) giving "Tue 10/14 · 5:12 – 5:54 PM · 3.1 km ·
  42 min"; `sections` without "Brought home"; `RecapSample` has the two foragers and the completing one crosses its mark.
- [ ] **Step 2–5:** run (fail), implement, `ANDROID_HOME=~/Android/Sdk just check-android`, commit
  `feat: forager bars and walk times in recap`.

---

### Task 4: Walk rows in the Activity log (Android)

**Files:** `ActivityScreen.kt`, `AwayFormat.kt` (label), `RecapNotice.kt` (`RecapController`), `RecapScreen.kt`,
`AppRoot.kt`, `ui/HelpText.kt` (+ tests).

**Interfaces — produces:**

- `AwayFormat` label `"walk" to "Walk"`.
- `RecapController.showHistory(r: RecapOut)`: like `preview` (no core call on close); a `closeLabel` state: "Done" normally,
  `Help.RECAP_CLOSE` ("Close") for history and preview. Rename the private `previewing` flag to `transient` if that reads
  better; keep `dismissTouchesCore(transient)` tested.
- `RecapScreen(r, closeLabel: String, onDone)`; `AppRoot` passes `m.recaps.closeLabel`.
- `ActivityScreen`: rows with `kind == "walk"` are a clickable `Row` (`Role.Button`, min height 48 dp) with a walk icon from
  `ui/ApgoIcons.kt` (or a simple existing icon) and the detail; tap → `m.engine.walkRecap(e.tMs)?.let(m.recaps::showHistory)`.
  Other rows unchanged.

- [ ] **Step 1: Failing tests:** `AwayFormat.kindLabel("walk") == "Walk"`; `RecapController` close decision for history
  (`dismissTouchesCore` false); the close label choice if extracted as a pure function.
- [ ] **Step 2–5:** run (fail), implement, `ANDROID_HOME=~/Android/Sdk just check-android && just check-hygiene`, update the
  "Walk recap" paragraph in `docs/context/v1-architecture-and-status.md` (walk history, `walks` table, `walk` rows,
  foragers), commit `feat: open past walks from the activity log`.
