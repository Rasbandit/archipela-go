# Walk Recap v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the recap screen per the approved canvas: map with quest pins on top, animated per-chain progress bars with
milestone confetti, expandable completed-quest rows.

**Architecture:** The core records, during the walk, each completed quest (time, point), the exact counted steps and every
chain's value at the start; on arrival `recap_of` turns them into `Recap.quests` and `Recap.chains`. FFI exposes them. Android
draws: a pure `RecapTimeline` sequences the bars, a Canvas `RecapChainBar` draws fills, dots, bounce and confetti, the MapLibre
map gets a pin layer, and quest rows expand.

**Tech Stack:** Rust (`apgo-core`, `apgo-ffi`, UniFFI, serde), Kotlin/Compose, MapLibre.

**Spec:** `docs/superpowers/specs/2026-10-14-walk-recap-v2-design.md` (visual reference: the canvas linked there)

## Global Constraints

- Hard rule: what the recap contains is decided in the Rust core; Kotlin only draws (animation timing is presentation and lives
  in Kotlin).
- Hard rule: events over polling. New outing data is written where the events already happen (`complete`, `credit_steps`,
  `open_outing`), never by a timer.
- New saved fields use `#[serde(default)]`; old outings and recaps load.
- Bar colours: before `ApgoPalette.questTodo`, gain `ApgoPalette.questProgress`, reached dot `ApgoPalette.questDone`; confetti
  `questProgress`, `questDone`, `sky`, `butter`, `questTodo`, `grass`. Every colour a palette token (`check_color_tokens.sh`).
- Bar timing: full bar 2600 ms, each bar clamped 450–2400 ms, 380 ms between bars, soft landing over the last 12 %, linear before.
- Steps line text exactly "4,812 walked · 4,090 counted" (numbers formatted with grouping), only when walked ≠ counted.
- Quest rows: badge number = pin number; one open at a time; details: Goal, Done at, Reward (Reward row hidden when `null`).
- Reduced motion (`Settings.Global.ANIMATOR_DURATION_SCALE == 0f`): final state, no confetti.
- Conventional commits, subject < 50 chars, body lines ≤ 72, ending `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- `just check-rust` and `ANDROID_HOME=~/Android/Sdk just check-android` pass; coverage floors only go up; no new lint
  suppressions unless the repo already suppresses that rule.

## Review Focus

1. A quest completed while a trap blocks checks or while counting is off never reaches `complete`, so it is not recorded —
   expected; but a quest completed by banking on arrival (inside `arrive_home`) must be recorded before the outing closes.
2. A chain whose zone unlocks mid-walk has no `progress_at_start` entry: it must count from 0, not be dropped or panic.
3. Wanderlust (minutes) runs live from `away_mark`: `before`/`after` must use `counter_at(c, t)`, not the credited value only.
4. A recap with no chains and no quests (only items or banked) must still render (empty cards hidden, no crash in the timeline
   with zero bars).
5. Quest with no point (`at: None`): row shown, no pin; numbering stays consistent between pins and rows (number by row index).

Tests: 1 → Task 1, 2 and 3 → Task 2, 4 → Task 4, 5 → Task 4 (pin builder) and Task 6.

## File Structure

| File | Responsibility |
| ---- | -------------- |
| `core/src/recap.rs` | `RecapQuest`, `RecapChain`, new `Recap` fields, worth-showing rule |
| `core/src/game.rs` | `OutingQuest`, outing recording in `complete`/`credit_steps`/`open_outing`, `recap_of` |
| `core/src/chain.rs` | serde on `ChainUnit` |
| `core/ffi/src/engine.rs` | `RecapChainOut`, `RecapQuestOut`, `RecapOut` fields |
| `android/.../RecapTimeline.kt` (new) | Bar start/duration and soft-landing progress (pure) |
| `android/.../RecapFormat.kt` | Subtitle, per-bar texts, steps line, quest rows/details |
| `android/.../ui/RecapChainBar.kt` (new) | One animated bar: fills, dots, bounce, confetti |
| `android/.../MapGeoJson.kt` (`RecapFeatures`) | Quest pin features |
| `android/.../RecapScreen.kt` | Screen order, map pins + tap selection, progress card, quests card |

---

### Task 1: Record quests completed on the walk (core)

**Files:** Modify `core/src/recap.rs`, `core/src/game.rs` (+ tests in both).

**Interfaces:**

- Produces: `pub struct OutingQuest { pub location_id: i64, pub t_ms: i64, pub at: Option<Point> }` (serde, `Debug, Clone,
  PartialEq`), `Outing.quests: Vec<OutingQuest>` (`#[serde(default)]`); `pub struct RecapQuest { pub name: String, pub place:
  String, pub goal: String, pub t_ms: i64, pub at: Option<Point>, pub reward: Option<String> }` (serde, `Debug, Clone,
  PartialEq`); `Recap.quests: Vec<RecapQuest>` (`#[serde(default)]`).
- Removes: `Outing.done_at_start` and the `mark_checked` hook that inserts into it.

- [ ] **Step 1: Failing tests** in `game.rs` `mod tests` (reuse `left_home`, `chain_game`, `forager_game`, `fixat`):

```rust
    #[test]
    fn a_quest_done_on_the_walk_is_recorded_with_its_time_and_place() {
        let (mut g, pts) = forager_game();
        left_home(&mut g, 0);
        for (i, p) in pts.iter().enumerate() {
            g.on_estimate(&fixat(*p, 600 + 600 * i64::try_from(i).unwrap()), None);
        }
        g.set_counting(false, 9_000_000);
        let r = g.arrive_home(9_100_000).1.expect("a forager done");
        let q = r.quests.iter().find(|q| !q.goal.is_empty()).expect("the forager is listed");
        assert!(q.at.is_some() && q.t_ms > 0);
        assert_eq!(q.reward.as_deref(), g.solo_rewards.get(&2000).map(String::as_str), "solo: the reward is known");
    }

    #[test]
    fn chain_milestones_count_as_done_but_are_not_listed_as_quests() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        left_home(&mut g, 0);
        g.on_steps(0, 1, None);
        g.on_steps(600, 2, None);
        let r = g.arrive_home(10 * 60_000).1.unwrap();
        assert_eq!(r.quests_done.len(), 1);
        assert!(r.quests.is_empty(), "the bar shows chain milestones");
    }

    #[test]
    fn archipelago_quests_have_no_known_reward() {
        let (mut g, pts) = forager_game();
        g.backend = Backend::Archipelago;
        left_home(&mut g, 0);
        for (i, p) in pts.iter().enumerate() {
            g.on_estimate(&fixat(*p, 600 + 600 * i64::try_from(i).unwrap()), None);
        }
        g.set_counting(false, 9_000_000);
        let r = g.arrive_home(9_100_000).1.unwrap();
        assert!(!r.quests.is_empty() && r.quests.iter().all(|q| q.reward.is_none()));
    }
```

Keep the existing `a_check_synced_from_the_server_mid_walk_is_not_a_walk_quest` test: it must still pass without the
`done_at_start` hook. If `forager_game`'s points do not complete the forager with three picks, read `forager_game` and walk
enough of its points (its `need`) — note what you did.

- [ ] **Step 2: Run, expect compile failure** (`r.quests` missing): `cd core && cargo test -p apgo-core game::tests::a_quest_done`
- [ ] **Step 3: Implement.**
  - `recap.rs`: add `RecapQuest` and `#[serde(default)] pub quests: Vec<RecapQuest>` to `Recap` (doc comments on every field).
  - `game.rs`: add `OutingQuest`; `Outing` gains `#[serde(default)] pub quests: Vec<OutingQuest>`; delete `done_at_start` (field,
    `open_outing` init, `mark_checked` hook).
  - In `complete`, right after `self.done.insert(id)` succeeded: `if let Some(o) = self.outing.as_mut() { o.quests.push(OutingQuest
    { location_id: id, t_ms: now_ms, at: pos }); }`.
  - `recap_of`: `quests_done` = names of `o.quests` (via assignment `quest_name`); `quests` = the same, skipping chain members
    (`let members: BTreeSet<i64> = self.chains().iter().flat_map(|c| c.marks.iter().map(|m| m.location_id)).collect();`), each
    with `name`, `place`, `goal: a.target.goal_text(self.units)`, `t_ms`, `at`, `reward: match self.backend { Backend::Solo =>
    self.solo_rewards.get(&id).cloned(), Backend::Archipelago => None }`. `checks_sent` stays `quests_done.len()` in Archipelago.
- [ ] **Step 4: Run** `cd core && cargo test -p apgo-core && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings`
- [ ] **Step 5: Commit** `feat: record quests done on the walk`

---

### Task 2: Exact counted steps and chain progress (core)

**Files:** Modify `core/src/chain.rs`, `core/src/recap.rs`, `core/src/game.rs` (+ tests).

**Interfaces:**

- Consumes: Task 1's `Outing`/`Recap`.
- Produces: `ChainUnit` derives `Serialize, Deserialize` with `#[serde(rename_all = "lowercase")]`;
  `Outing.counted_steps: f64`, `Outing.progress_at_start: BTreeMap<String, f64>` (both `#[serde(default)]`);
  `pub struct RecapChain { pub id: String, pub name: String, pub unit: ChainUnit, pub zone: u32, pub before: f64, pub after:
  f64, pub marks: Vec<f64> }`; `Recap.chains: Vec<RecapChain>`, `Recap.counted_steps: i64` (both `#[serde(default)]`);
  `worth_showing` also true when `!chains.is_empty()`.

- [ ] **Step 1: Failing tests** in `game.rs` `mod tests`:

```rust
    #[test]
    fn counted_steps_follow_the_backpack_at_each_reading_never_backwards() {
        let (mut g, pts) = forager_game();
        left_home(&mut g, 0);
        g.on_steps(0, 1, None);
        g.on_steps(1000, 2, None); // light bag: full rate
        g.on_estimate(&fixat(pts[0], 600), None);
        g.on_estimate(&fixat(pts[1], 1200), None);
        g.bag_size = 0; // now overloaded
        let f = g.step_factor();
        assert!(f < 1.0);
        g.on_steps(2000, 1_300_000, None);
        let o = g.outing.clone().unwrap();
        assert_eq!(o.steps, 2000);
        assert!((o.counted_steps - (1000.0 + 1000.0 * f)).abs() < 1e-6, "{}", o.counted_steps);
    }

    #[test]
    fn the_recap_has_each_grown_chain_before_and_after_in_unit_order() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }, Target::Steps { n: 1000 }]);
        g.on_steps(0, 1, None);
        g.on_steps(300, 2, None); // 300 before the walk
        left_home(&mut g, 1_000);
        g.on_steps(300, 1_001, None);
        g.on_steps(900, 1_002, None);
        let r = g.arrive_home(10 * 60_000).1.expect("a chain grew");
        assert_eq!(r.chains.len(), 1);
        let c = &r.chains[0];
        assert_eq!((c.unit, c.before, c.after, c.marks.clone()), (ChainUnit::Steps, 300.0, 900.0, vec![500.0, 1500.0]));
        assert_eq!(r.counted_steps, 600);
    }

    #[test]
    fn time_away_bars_use_the_live_counter() {
        let mut g = away_game(&[30.0, 60.0]);
        left_home(&mut g, 0);
        g.set_counting(false, 40 * 60_000);
        let r = g.arrive_home(40 * 60_000).1.expect("time away grew");
        let c = r.chains.iter().find(|c| c.unit == ChainUnit::Minutes).unwrap();
        assert!((c.after - c.before - 40.0).abs() < 0.5, "{} -> {}", c.before, c.after);
    }

    #[test]
    fn a_chain_with_no_start_value_counts_from_zero() {
        let mut g = chain_game("step_up", vec![Target::Steps { n: 500 }]);
        g.on_steps(0, 1, None);
        g.on_steps(200, 2, None);
        left_home(&mut g, 1_000);
        g.outing.as_mut().unwrap().progress_at_start.clear(); // as if the chain's zone unlocked mid-walk
        g.on_steps(200, 1_001, None);
        g.on_steps(400, 1_002, None);
        let r = g.arrive_home(10 * 60_000).1.unwrap();
        assert_eq!(r.chains[0].before, 0.0);
    }

    #[test]
    fn a_walk_that_only_moved_a_bar_is_worth_showing() {
        let r = Recap { start_ms: 0, end_ms: 10 * 60_000, chains: vec![RecapChain { id: "1:x".into(), name: "x".into(), unit: ChainUnit::Cells, zone: 1, before: 1.0, after: 2.0, marks: vec![5.0] }], ..Recap::default() };
        assert!(r.worth_showing());
    }
```

Put the `worth_showing` test in `recap.rs` tests. Steps test: if `chain_game`'s members unlock at different marks than `[500, 1500]`
(running totals), assert what `Chain.marks` really holds and say so.

- [ ] **Step 2: Run, expect compile failure.**
- [ ] **Step 3: Implement.**
  - `chain.rs`: serde derives on `ChainUnit`.
  - `credit_steps`: compute `counted` as today; then `if let Some(o) = &mut self.outing { o.steps += gained; o.counted_steps +=
    counted; }` (move the existing raw-steps line into this block; keep raw steps added before the `gained == 0` return as now —
    restructure so both are added from the same `gained`).
  - `open_outing`: `progress_at_start: self.chains().iter().map(|c| (c.id.clone(), self.counter_at(c, t_ms))).collect()`.
  - `recap_of`: `chains` = every chain where `counter_at(c, t_ms) > before` (before = `progress_at_start.get(&c.id)` or 0.0),
    sorted by unit (Steps, Minutes, Cells) then zone; `marks` = `c.marks.iter().map(|m| m.at)`; `counted_steps =
    o.counted_steps.round()` as i64 (use the crate's existing f64→i64 helper, e.g. `round_i64`).
  - `Recap::worth_showing`: add `|| !self.chains.is_empty()` to `happened`.
- [ ] **Step 4: Run** core tests + clippy as Task 1.
- [ ] **Step 5: Commit** `feat: chain progress and counted steps in recap`

---

### Task 3: FFI recap additions

**Files:** Modify `core/ffi/src/engine.rs` (+ test).

**Interfaces:**

- Produces (UniFFI): `RecapChainOut { id: String, name: String, unit: String /* steps|minutes|cells */, zone: u32, before: f64,
  after: f64, marks: Vec<f64> }`, `RecapQuestOut { name: String, place: String, goal: String, t_ms: i64, at: Option<GeoPoint>,
  reward: Option<String> }`; `RecapOut` gains `counted_steps: i64`, `chains: Vec<RecapChainOut>`, `quests: Vec<RecapQuestOut>`.

- [ ] **Step 1: Failing test:** extend `arriving_home_stores_one_recap_with_its_route_until_dismissed` or add a test: set
  `g.outing` steps and push an `OutingQuest` for a real assignment id plus a `progress_at_start` entry lower than the chain's
  value via `with_game`, arrive, and assert `pending_recap()` has `counted_steps`, one `chains` entry with `unit == "steps"`,
  and the quest's `name`/`at`. Use whatever the test game's assignments allow; keep it minimal.
- [ ] **Step 2: Run, expect compile failure.**
- [ ] **Step 3: Implement** the two records (doc comment per field) and extend `From<&Recap> for RecapOut`; map `ChainUnit` to
  the same strings `ChainOut` uses (reuse that match — extract a small `fn unit_name(u: ChainUnit) -> &'static str` if it is
  inline today, and use it in both places).
- [ ] **Step 4: Run** `cd core && cargo test -p apgo-ffi && cd .. && just check-rust`
- [ ] **Step 5: Commit** `feat: expose recap chains and quests`

---

### Task 4: Recap timeline and texts (Android, pure)

**Files:** Create `android/app/src/main/java/dev/apgo2/RecapTimeline.kt`; modify `RecapFormat.kt`, `MapGeoJson.kt`
(`RecapFeatures`); tests `RecapTimelineTest.kt`, `RecapFormatTest.kt`, `MapGeoJsonTest.kt`. Regenerate bindings first
(`bash scripts/android_bindings.sh`).

**Interfaces:**

- Produces:
  - `internal object RecapTimeline { const val FULL_BAR_MS = 2600L; const val MIN_BAR_MS = 450L; const val MAX_BAR_MS = 2400L;
    const val GAP_MS = 380L; const val START_MS = 450L; const val LANDING = 0.12f; data class Slot(val startMs: Long, val
    durationMs: Long); fun slots(gainFractions: List<Float>): List<Slot>; fun endMs(slots: List<Slot>): Long; fun
    progress(elapsedMs: Long, slot: Slot): Float /* softLand of linear t */; fun softLand(t: Float, tail: Float = LANDING): Float }`
  - `RecapFormat.subtitle(r): String` ("[date] · 3.1 km · 42 min": date via `java.text.DateFormat.getDateInstance(SHORT)` of
    `endMs`, distance via `Units.distance`, time via `AwayFormat.duration`), `RecapFormat.barGain(c: RecapChainOut, value:
    Double): String` ("+4,090 steps" / "+42 min" / "+14 new squares"), `barTotal(c, value)` ("2,140 → 6,230 total"),
    `barNext(c, value)` ("next: 7,500" or "All milestones reached"), `stepsLine(r): String?` ("4,812 walked · 4,090 counted"
    or `null` when equal), `questTime(q): String` (short time of `tMs`); `sections(r)` drops "Quests completed" (it has its own
    card) — keep Items received, Checks sent, Zones unlocked, Brought home. Remove `stats(r)`.
  - `RecapFeatures.questPins(quests: List<RecapQuestOut>, selected: Int?): List<JSONObject>` — one Point feature per quest
    with a point, properties `n` (1-based row index, counted over ALL quests so numbers match rows) and `selected` (bool).

- [ ] **Step 1: Failing tests.**

```kotlin
class RecapTimelineTest {
    @Test fun durationIsProportionalToGainAndClamped() {
        val s = RecapTimeline.slots(listOf(0.41f, 0.1f, 1.0f))
        assertEquals(1066L, s[0].durationMs) // 0.41 * 2600
        assertEquals(450L, s[1].durationMs)  // min
        assertEquals(2400L, s[2].durationMs) // max
    }

    @Test fun barsRunOneAfterAnother() {
        val s = RecapTimeline.slots(listOf(0.5f, 0.5f))
        assertEquals(RecapTimeline.START_MS, s[0].startMs)
        assertEquals(s[0].startMs + s[0].durationMs + RecapTimeline.GAP_MS, s[1].startMs)
        assertEquals(s[1].startMs + s[1].durationMs, RecapTimeline.endMs(s))
    }

    @Test fun noBarsIsEmpty() {
        assertEquals(emptyList<RecapTimeline.Slot>(), RecapTimeline.slots(emptyList()))
        assertEquals(0L, RecapTimeline.endMs(emptyList()))
    }

    @Test fun softLandIsLinearThenEndsExactlyAtOne() {
        assertEquals(0f, RecapTimeline.softLand(0f), 1e-6f)
        assertEquals(1f, RecapTimeline.softLand(1f), 1e-6f)
        val v = 1f / (1f - RecapTimeline.LANDING / 2f)
        assertEquals(0.5f * v, RecapTimeline.softLand(0.5f), 1e-5f) // linear part
        val a = RecapTimeline.softLand(0.879f)
        val b = RecapTimeline.softLand(0.881f)
        assertTrue("continuous at the joint", kotlin.math.abs(b - a) < 0.01f)
        assertTrue("never overshoots", RecapTimeline.softLand(0.99f) <= 1f)
    }
}
```

`RecapFormatTest`: assert `barGain`/`barTotal`/`barNext` for a steps chain (`before 2140, after 6230, marks [1000,2500,5000,
7500,10000]`) at `value = 6230.0` → "+4,090 steps", "2,140 → 6,230 total", "next: 7,500"; at `value = 10000.0` →
"All milestones reached"; minutes → "+42 min"; cells → "+14 new squares"; `stepsLine` → "4,812 walked · 4,090 counted" and
`null` when equal; `sections` no longer has "Quests completed". Update the `recap(...)` test helper in `RecapNoticeTest.kt` for
the new `RecapOut` fields (defaults: `countedSteps = 0`, empty `chains`/`quests`).
`MapGeoJsonTest`: `questPins` numbers by row index (a middle quest with `at = null` is skipped but the next keeps `n = 3`) and
sets `selected` only on the chosen index.

- [ ] **Step 2: Run, expect failure.** `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.Recap*'`
- [ ] **Step 3: Implement** (softLand: `if (tail <= 0f) t else { val v = 1f/(1f-tail/2f); if (t < 1f-tail) v*t else { val u =
  t-(1f-tail); min(1f, v*(1f-tail) + v*(u - u*u/(2f*tail))) } }`; `progress` = `softLand(((elapsed-start)/duration)
  .coerceIn(0f,1f))`; `slots` = durations `(f.coerceIn(0f,1f) * FULL_BAR_MS).toLong().coerceIn(MIN, MAX)`, sequential).
  Remove `stats` and its now-unused callers in `RecapScreen` only as far as needed to compile (Task 7 rebuilds the screen).
- [ ] **Step 4: Run** `ANDROID_HOME=~/Android/Sdk just check-android`
- [ ] **Step 5: Commit** `feat: recap bar timeline and texts`

---

### Task 5: Animated chain bar (Android)

**Files:** Create `android/app/src/main/java/dev/apgo2/ui/RecapChainBar.kt`; test `android/app/src/test/java/dev/apgo2/ui/
RecapChainBarTest.kt` (pure helpers only).

**Interfaces:**

- Consumes: `RecapTimeline` (Task 4), palette tokens.
- Produces: `@Composable internal fun RecapChainBar(before: Float, value: Float, max: Float, marks: List<Float>, crossedFrom:
  Float, animate: Boolean, modifier: Modifier = Modifier)` — `before`/`value` in the chain's units, `max` = last mark;
  `crossedFrom` = the chain's `before` (marks in `(crossedFrom, value]` are "crossed on this walk"). Pure helpers in the same
  file, `internal`: `fun markFraction(mark: Float, max: Float): Float` (clamped to 0.03–0.97 like the canvas), `fun
  crossed(marks: List<Float>, from: Float, value: Float): List<Int>` (indexes newly reached), `val CONFETTI: List<Bit>` (14
  bits: colour token, dx, dy in dp, rotation — same layout as the canvas: angle `i/14·2π ± small offset`, radius `16 + (i·7 mod
  5)·5`, dy scaled 0.8 plus 6).

Drawing (Canvas, 18 dp tall, track 6 dp, like `ChainBar`): track `mint`; gain fill `questProgress` from `before` to `value`
(drawn first, starting 2 % left of the before end so the join is hidden); before fill `questTodo` on top; dots as `ChainBar`
(reached `questDone` with `onMap` ring, else hollow `muted`). When `animate` and a mark index first appears in `crossed(...)`,
start for that dot an `Animatable` scale 1 → 1.7 → 1 (spring, ~480 ms) and a confetti `Animatable` 0 → 1 (~820 ms,
`FastOutSlowInEasing`): each bit drawn as a 6×4 dp rounded rect at `dot + (dx, dy)·p`, rotated `rot·p`, alpha `1` until 70 %
then fading to 0. `animate = false` draws the final state, no confetti.

- [ ] **Step 1: Failing tests** for `markFraction` (clamp), `crossed` (`marks [1000,2500,5000]`, from 2140 to 6230 → `[1, 2]`;
  from 2140 to 2140 → empty), and `CONFETTI` (14 bits, every colour one of the six palette tokens).
- [ ] **Step 2: Run, expect failure.**
- [ ] **Step 3: Implement** the helpers and the composable.
- [ ] **Step 4: Run** `ANDROID_HOME=~/Android/Sdk just check-android`
- [ ] **Step 5: Commit** `feat: animated recap chain bar`

---

### Task 6: Map pins and quest rows (Android)

**Files:** Modify `android/app/src/main/java/dev/apgo2/RecapScreen.kt`, `MapStyle.kt` only if a pin layer must be registered
there (prefer adding the recap pin source/layer inside the recap map's own style callback), `ui/HelpText.kt`.

**Interfaces:**

- Consumes: `RecapFeatures.questPins` (Task 4), `RecapFormat.questTime`.
- Produces: `@Composable private fun RecapMap(route, quests, selected: Int?, onPin: (Int) -> Unit)` and `@Composable private
  fun QuestsCard(quests, selected: Int?, onToggle: (Int) -> Unit)`.

Map: keep the existing route source/layer; add a GeoJSON source with `questPins(quests, selected)` and a circle layer
(radius 12 dp, 15 dp when `selected`, colour `questDone`, stroke `onMap`) plus a symbol layer for `n` (text colour `onBrand`).
Update the source's data when `selected` changes (no map rebuild). `map.addOnMapClickListener { ll -> features =
map.queryRenderedFeatures(map.projection.toScreenLocation(ll), PIN_LAYER); features.firstOrNull()?.getNumberProperty("n")
?.let { onPin(it.toInt() - 1); true } ?: false }`. Colours via palette `.hex()`.

Quests card (hidden when `quests` is empty): title "Quests completed"; per quest a full-width row with
`Modifier.clickable(role = Role.Button) { onToggle(i) }`, `semantics { stateDescription = if (open) "Expanded" else
"Collapsed" }`, min height 56 dp: numbered badge (28 dp circle `questDone`, number `onBrand`), name, "place · time" (muted),
chevron rotating 180° when open (`animateFloatAsState`). Open row (`AnimatedVisibility`) shows a two-column detail list: Goal,
Done at, Reward (Reward omitted when `null`). Labels in `Help` (`RECAP_GOAL`, `RECAP_DONE_AT`, `RECAP_REWARD`,
`RECAP_QUESTS`). Selection: `var selected by rememberSaveable { mutableStateOf<Int?>(null) }` in `RecapScreen`; tapping the
open one closes it.

- [ ] **Step 1: Test** — no new pure logic beyond Task 4's pin builder; if you add any helper, test it.
- [ ] **Step 2–3: Implement.**
- [ ] **Step 4: Run** `ANDROID_HOME=~/Android/Sdk just check-android`
- [ ] **Step 5: Commit** `feat: quest pins and rows on the recap`

---

### Task 7: Assemble the screen + docs (Android)

**Files:** Modify `RecapScreen.kt`, `docs/context/v1-architecture-and-status.md`.

Order: title + `RecapFormat.subtitle(r)` → `RecapMap` (only when the route or a pin exists) → progress card (hidden when
`r.chains` empty: title "This walk", legend "before" `questTodo` / "counted" `questProgress`, then per chain: name + live
`barGain`, `RecapChainBar`, row `barTotal` | `barNext`, and for steps chains `stepsLine(r)` fading in at 60 % of that bar) →
`QuestsCard` → `sections` → Done. Remove `StatTile`. One clock: `val clock = remember { Animatable(0f) }`;
`LaunchedEffect(r.endMs) { clock.snapTo(0f); clock.animateTo(endMs.toFloat(), tween(endMs.toInt(), easing = LinearEasing)) }`;
each bar's displayed value = `before + (after - before) * RecapTimeline.progress(clock.value.toLong(), slot)`; slots from
`RecapTimeline.slots(chains.map { gainFraction })` where gainFraction = `(min(after,max) - min(before,max)) / max`. A bar
before its start draws at 55 % alpha. Reduced motion: read `Settings.Global.getFloat(cr, ANIMATOR_DURATION_SCALE, 1f) == 0f`
→ `snapTo(endMs)` and `animate = false`. Card entrance: keep it simple (`AnimatedVisibility` fade + slide per card with a
small stagger) — no per-line pop needed.

Docs: update the "Walk recap" paragraph in `docs/context/v1-architecture-and-status.md`: outing records quests (`complete`),
counted steps (`credit_steps`) and `progress_at_start`; recap has `quests` (non-chain), `chains`, `counted_steps`; screen
layout and animation (timeline constants, confetti), pins via MapLibre layer + `queryRenderedFeatures`.

- [ ] **Step 1: Implement** (layout code; the pure parts are already tested).
- [ ] **Step 2: Run** `ANDROID_HOME=~/Android/Sdk just check-android && just check-hygiene`
- [ ] **Step 3: Commit** `feat: recap v2 screen layout`
