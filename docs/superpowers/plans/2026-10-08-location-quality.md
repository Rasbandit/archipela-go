# Location Quality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to
> implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Execution method chosen by the owner:
> **subagent-driven** (a fresh implementer per task, a fresh reviewer after each task, a whole-branch review at the end).

**Goal:** Replace the raw-fix rules with a layered location estimator (IMM Kalman truth, HMM map matching for display, a particle filter
for GPS gaps), the best location request on every Android phone, a gliding map pin, and a record/replay bench that measures every layer.

**Architecture:** A new `apgo-core` module `loc` (`core/src/loc/`) owns everything: `Locator` is a `#[serde(skip)]` field of `Game`, fed raw
fixes, steps and headings; `Game::on_estimate` (today's `on_fix` body) only ever sees its `Estimate`. The FFI gains `FixIn`, `position()`,
`on_heading`, step calibration and the matched trace; Android records raw tracks in debug builds, asks for the best request per phone, and
draws the pin with MapLibre's `LocationComponent`. A bench (`loc::bench` plus `examples/replay.rs`) scores synthetic walks in CI and real
walks locally.

**Tech Stack:** Rust 2021 (`apgo-core`, `apgo-ffi` via UniFFI 0.32, `rand` 0.10, `serde_json`, `rusqlite`), Kotlin + Jetpack Compose,
MapLibre Android 13.6.1, JUnit 4, `just`, `cargo llvm-cov`, Kover.

**Spec:** `docs/superpowers/specs/2026-10-08-location-quality-design.md` (owner-approved 2026-10-08). Read it before every task; this plan
argues from it and only fills in what the spec leaves open (each such choice is marked "plan choice").

## Global Constraints

Copied verbatim from the spec. Every task's requirements implicitly include this section.

- Everything ships in **one PR** (`feat/location-quality`), built as the sequenced tasks at the end of this spec.
- Architecture: IMM Kalman (truth) -> HMM matcher (display) -> particle filter (gaps only).
- What quests use: The IMM estimate: pickups, reach, dwell, away, fog, chains, odometer, near-miss diagnostics. Never raw fixes, never the matched position.
- Smoothing: Adapts by speed and mode: heavy with a stationary hold when slow, light at speed.
- Raw tracks: Recorded in debug builds only; release never stores raw fixes (the journal stores estimates).
- Filter state: Not saved. Restart = fresh filter. Saved games load unchanged. Quest radii unchanged.
- iOS: No app yet: shared core plus the integration section below, no Swift code.
- Raw GNSS measurements: Out of scope.
- Where the estimator lives: `apgo-core` module `loc` (`core/src/loc/`), owned by `Game` as a `#[serde(skip)]` field, so Android and iOS get it through the same FFI.
- Linear algebra: Hand-rolled fixed-size 2x2 / 4x4 math in `loc/mat.rs` (Joseph-form update), no new crate.
- Frame: Local east-north (ENU tangent plane) around an anchor; re-anchored when the estimate is 5 km from it.
- Timestamps: The fix's own time (`Location.time`, corrected by `elapsedRealtimeNanos`), not `now()` at delivery; out-of-order or duplicate fixes are dropped.
- Sample rate: InZone while playing: 1 s while the screen is on, 5 s while it is off; HIGH_ACCURACY in both (decided in `GpsPolicy.forDecision`, unit-tested).
- Journal: Stores accepted estimates (throttled to 5 s or 5 m), not raw fixes; old raw rows stay.
- Map pin: MapLibre `LocationComponent` in custom-location mode (we push estimates, it animates), drawables rendered by `MapMarkers`, colours from `ApgoPalette`.
- Map matching input: IMM estimates (not raw fixes), display and trace only.
- Bridged positions: Display, fog and Cartographer squares only; never quests, chains other than Cartographer, the odometer or the journal (`accepted = false`).
- Step length: Cadence model times a calibration saved on the phone in app preferences (not the game save), keyed by step source; re-validated against good GPS every session and adapting fast after a carry change.
- Step source: Only the phone's `TYPE_STEP_COUNTER` (key `phone.step_counter`). The app never reads watch steps, so a forgotten watch changes nothing.
- Real walks in CI: Never committed (they show the owner's home). CI runs synthetic walks; real walks run locally.
- No Play services: Detected by package; then the `gps` provider is used directly and our filter does the fusion. No Play services library is added.
  (Superseded 2026-10-09 by the owner, Tasks 20b/20c: `play-services-location` is added; Play services is detected by
  `GoogleApiAvailability` `SUCCESS` plus the package, and gives fixes and the on-screen compass. See the spec's decisions table.)
- Simulated fixes (dev simulator): Bypass the filter: each one resets it to an exact estimate (3 m), as today's teleports.
- `accepted` is true when the verdict is `Used`, `Soft`, `Relocated` or `Reset`, the source is `Gps`, and `uncertainty_m <= 35 m` (`MAX_UNCERTAINTY_M`, the old `MAX_ACCURACY_M` value, now applied to the estimate).
- Text: Any new help text ("Why is my dot hollow?") lives in `ui/HelpText.kt`.
- Each scenario runs 20 seeds; a threshold must hold on every seed unless the column says p90.
- Performance budget (A53, p99): IMM update 50 us; HMM step 3 ms; particle filter step batch 1 ms; whole `Engine::on_fix` 10 ms; graph build on game open 300 ms, off the main thread; memory (graph + lattice + particles) 5 MB.
- Coverage floors only go up (`check-rust` 80 %). (The live floors are higher today: rust 84 in `justfile`, kotlin 18 in `android/app/build.gradle.kts`.)

Repo rules that also apply to every task (from `CLAUDE.md` and the owner's brief):

- Work only in `/home/rasbandit/Documents/code-projects/Archipela-Go/.claude/worktrees/gps-quality` on `feat/location-quality`; never switch
  branches, never push, never edit `CLAUDE.md`.
- TDD: the failing test first (run it, see the RED message this plan names), then the code. Never edit a test to fit bad code.
- Gates per task: `just check-rust` (Rust changes), `ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android` (any Android or FFI
  change: the bindings are rebuilt from `core/ffi`), `just check-hygiene` (docs, scripts or Markdown changes). Coverage floors only go up:
  rust `--fail-under-lines 84` (justfile `check-rust`), kotlin `minBound(18)` (`android/app/build.gradle.kts`).
- Commits: conventional, subject under 50 characters, imperative, body lines under 72 characters, `Refs #89` in the body, `Closes #8x` on
  the task that finishes each sub-issue, and the trailer `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Clippy is `pedantic` + `unwrap_used`/`expect_used` deny (tests may unwrap), `missing_docs` deny: every `pub` item gets a `///` doc. Casts
  go through `crate::num` helpers (`count_f64`, `i64_to_f64`, `round_i64`, ...); every `#[allow]`/`@Suppress` is as local as possible with a
  reason comment. rustfmt width 160.
- Design system: colours only in `ApgoPalette`, map pins only from `MapMarkers.render(MarkerSpec)`, shown distances through `ui/Units.kt`,
  help copy only in `ui/HelpText.kt`.
- Real walk data is never committed (privacy): it lives in the main checkout's `diag/` (git-ignored) or the session scratchpad.

## Review Focus

The five inputs the spec implies but no spec-listed test exercises, most likely to bite a player first. Each has a test in the task that
owns the code (marked "Review Focus" in that task).

1. **Batched or out-of-order delivery** (screen-off batches of several fixes, a fix older than the last one, the same fix twice): fixes
   are processed oldest first and a stale or duplicate fix is `Unusable`, never a backwards step. Tests: Task 1 (`FixTimeTest`), Task 6
   (`unusable_rules`).
2. **A phone with no step counter** (many cheap phones, permission denied): standing still still holds, walking is still tracked, step
   evidence is neutral, bridging never starts and nothing panics. Test: Task 7 (`without_a_step_counter_hold_and_walking_still_work`).
3. **Switching from the dev simulator back to real GPS** (simulated fixes end, the next real fix is kilometres away): the first real fix
   after a simulated one starts a fresh filter (`Reset`) instead of being gated three times. Test: Task 7 (`a_real_fix_after_simulated_ones_starts_fresh`).
4. **Crossing the 5 km re-anchor** on a long bike or drive: no jump in the estimate, the odometer or the matched pin at the moment the
   frame moves. Test: Task 7 (`crossing_the_reanchor_distance_moves_nothing`).
5. **Zero, negative or missing fix fields** (accuracy 0, speed accuracy 0, iOS `-1` for unknown, bearing without accuracy): the sigma
   floor (2 m) holds, a velocity measurement is only used when all its fields are present and positive, and the estimate stays finite.
   Test: Task 7 (`odd_fix_fields_never_break_the_estimate`).

---

## File structure

Core (`core/src/loc/`, new module, registered in `core/src/lib.rs`):

| File | Responsibility | Task |
| --- | --- | --- |
| `mod.rs` | Public types (`Provider`, `RawFix`, `Estimate`, `Verdict`, `Motion`, `Source`, `HeadingIn`, `CompassAccuracy`, `DisplayPosition`), `mode_at`, `Locator` facade | 2, 7, 14, 17, 20 to 22 |
| `frame.rs` | ENU anchor, `to_enu` / `to_geo`, re-anchor test | 2 |
| `mat.rs` | const-generic small matrices, inverse, Joseph-form Kalman update, 2x2 eigenvalue | 5 |
| `params.rs` | `LocParams`: every number of the spec, `Default`, serde (JSON for the replay CLI) | 5 to 22 |
| `imm.rs` | Three models, `Pi(dt)`, mixing, predict, update, combine, gate, relocation, reset, unusable | 5, 6 |
| `bench.rs` + `bench/{synth,metrics,legacy,record,reference}.rs` | Synthetic walks, metrics, `LegacyRules`, recording readers, RTS reference | 2, 3, 4, 8 |
| `graph.rs` | `StreetGraph` from `Atlas::ways` or degraded from `street_links`, segment grid, candidates, bounded Dijkstra | 17 |
| `matcher.rs` | Online Viterbi HMM with off-network state, fixed-lag trace | 18, 19 |
| `heading.rs` | Compass history, "held flat and steady" rule, carry-offset estimator (plan choice: own file, the spec's table puts it under bridge) | 14, 20 |
| `calib.rs` | Step length model and per-source step calibration (plan choice: own file) | 21 |
| `bridge.rs` | Particle filter for GPS gaps | 22 |

Core, changed: `core/src/game.rs` (on_fix -> on_estimate, Task 9; graph, Task 17; bridged steps, Task 22), `core/src/verify.rs` (old jump
rules deleted, Task 9), `core/src/scan.rs` + `core/src/fill.rs` + `core/src/geo.rs` (`Atlas::ways`, Task 16), `core/ffi/src/engine.rs` (Tasks
10, 14, 19, 21), `core/examples/replay.rs` (new, Task 4), `core/examples/play_sim.rs` (Task 9), `core/tests/loc_scenarios.rs` (new, Tasks 2, 8,
19, 22).

Android (`android/app/src/main/java/dev/apgo2/`):

| File | Change | Task |
| --- | --- | --- |
| `RawTrack.kt` (new) | `FixSample`, `FixTime`, `RawLines`, `FixSamples` (Location -> sample) | 1, 10 |
| `DiagLog.kt` | `prefix` parameter, `Diag.raw` / `Diag.initRaw` | 1 |
| `Sensors.kt` | batched listener, request builder, providers with GMS, cold-start network, GNSS callback, 2 s steps, rotation vector | 1, 11, 12, 15 |
| `GpsPolicy.kt` | `forDecision(d, appVisible, screenOn)`, `providers(enabled, sdk, gms)`, `request(rate, sdk)` | 11 |
| `Gnss.kt` (new) | `GnssBands`, `GnssSummary` (pure) | 12 |
| `PresenceController.kt` | screen on/off receiver, raw state lines, home-Wi-Fi offer from `position()`, heading start/stop | 1, 11, 15 |
| `FieldDiagnostics.kt` | perf percentiles in the heartbeat, GNSS lines | 12 |
| `BatteryGuide.kt` (new), `SetupFlow.kt`, `SetupSteps.kt`, `presence/SetupProgress.kt` | "Keep tracking alive" step | 13 |
| `MePin.kt` (new) | `MePin`, `MePins` (PositionOut -> pin, pure) | 15 |
| `QuestMap.kt`, `MapStyle.kt`, `ui/MapMarkers.kt`, `ui/ApgoIcons.kt`, `ui/Palette.kt`, `ui/HelpText.kt` | LocationComponent pin, `MarkerSpec.Me`, `meUncertain`, `Heading` icon, help text | 15 |
| `AppModel.kt`, `DevSimulator.kt`, `PlayScreen.kt`, `HomePicker.kt`, `RealmEditor.kt` | `FixIn`, `position()`, pin, matched trace | 10, 15, 19 |
| `StepCalStore.kt` (new) | SharedPreferences `stepcal` by source (codec pure) | 21 |

Scripts and docs: `scripts/pull_diag.sh`, `scripts/diag_report.py` (Task 23), `docs/context/location-estimation.md` (new) and
`docs/context/v1-architecture-and-status.md` (Task 24).

## Task overview

| # | Task | Issue |
| --- | --- | --- |
| 1 | Android raw recording (debug), fix time, batched delivery | #83 |
| 2 | `loc` types, ENU frame, synthetic walk generator | #83 |
| 3 | Bench metrics, `LegacyRules`, scorecard | #83 |
| 4 | Recording readers, replay example, baseline scorecards | closes #83 |
| 5 | `mat.rs`, `LocParams`, IMM models and cycle | #84 |
| 6 | Gating, soft gate, relocation, reset, unusable rules | #84 |
| 7 | `Locator` facade, hold, step evidence, `Estimate` | #84 |
| 8 | Layer 1 CI scenarios, property test, timing, RTS reference, replay on `Locator` | #84 |
| 9 | `Game::on_estimate`, old rules deleted | #84 |
| 10 | FFI `FixIn`, journal of estimates, Kotlin callers | closes #84 |
| 11 | Rate by screen, providers with/without GMS, request builder, mock | #85 |
| 12 | GNSS status lines, 2 s step batches with event time, perf heartbeat | #85 |
| 13 | Battery "Keep tracking alive" setup step | closes #85 |
| 14 | `Locator::display`, `on_heading`, compass rule, FFI `position()` | #86 |
| 15 | LocationComponent pin, glide, circle, heading, `me` from `position()` | closes #86 |
| 16 | `Atlas::ways` in scans | #87 |
| 17 | `StreetGraph` (incl. degraded) attached to the game | #87 |
| 18 | HMM matcher | #87 |
| 19 | Display rule, matched trace, turn scenario | closes #87 |
| 20 | Carry-offset estimator | #88 |
| 21 | Step calibration by source, saved on the phone | #88 |
| 22 | Particle filter bridge, bridged fog and squares, gap scenarios | closes #88 |
| 23 | Tune `LocParams` on real walks | #89 |
| 24 | Docs: context doc, architecture status, iOS plan | #89 |

Every commit message below ends with this footer (not repeated in each task):

```text
Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
```

---

## Task 1: Android raw recording, fix time and batched delivery

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/RawTrack.kt`
- Modify: `android/app/src/main/java/dev/apgo2/DiagLog.kt` (constructor, `files()`, `name()`, `PATTERN`, `Diag`)
- Modify: `android/app/src/main/java/dev/apgo2/Sensors.kt` (`startLocation`, `startSteps`)
- Modify: `android/app/src/main/java/dev/apgo2/AppModel.kt` (`onFix`, `onSteps`)
- Modify: `android/app/src/main/java/dev/apgo2/ApgoApp.kt` (`onCreate`)
- Modify: `android/app/src/main/java/dev/apgo2/PresenceController.kt` (`evaluate`)
- Test: `android/app/src/test/java/dev/apgo2/RawTrackTest.kt`, `android/app/src/test/java/dev/apgo2/DiagLogTest.kt`

**Interfaces:**

- Produces: `FixSample` (data class, fields below), `FixTime.wallMs(fixTimeMs, fixElapsedNs, nowWallMs, nowElapsedNs): Long`,
  `FixTime.oldestFirst(items, elapsedNs)`, `RawLines.fix(s)`, `RawLines.steps(total, eventMs)`, `RawLines.heading(azimuthDeg, accuracy)`,
  `RawLines.state(presence, counting, zone, appVisible)`, `FixSamples.of(loc, nowWallMs, nowElapsedNs)`, `Diag.raw(tag, fields)`,
  `Diag.initRaw(ctx)`, `DiagLog(dir, maxFileBytes, keep, clock, prefix)`. Line format (read by Task 4):
  `{"t":<log ms>,"lvl":"I","tag":"rawfix","msg":"","tf":..,"ert":..,"lat":..,"lon":..,"acc":..,"spd":..,"spd_acc":..,"brg":..,"brg_acc":..,"alt":..,"valt":..,"prov":"fused","mock":false}`,
  `rawsteps` (`total`, `te`), `rawhead` (`az`, `acc`), `rawstate` (`presence`, `counting`, `zone`, `app_visible`).
- Plan choice: `rawstate` logs `zone` (`inside` / `near` / `far` / `unknown`, what Kotlin knows) instead of the spec's `zone_mode`; the
  replay takes the mode from `--mode` (listed under spec gaps).

- [ ] **Step 1: Write the failing tests**

`android/app/src/test/java/dev/apgo2/RawTrackTest.kt`:

```kotlin
package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class RawTrackTest {
    private fun sample(
        tMs: Long = 1_000L,
        elapsedNs: Long = 5_000_000_000L,
    ) = FixSample(
        tMs = tMs,
        elapsedNs = elapsedNs,
        lat = 40.5,
        lon = -111.9,
        accuracyM = 4.5f,
        speedMps = 1.25f,
        speedAccMps = 0.5f,
        bearingDeg = 90f,
        bearingAccDeg = 12f,
        altitudeM = 1400.0,
        verticalAccM = 3f,
        provider = "fused",
        mock = false,
    )

    @Test fun fixTimeComesFromTheMonotonicClockWhenThePhoneHasIt() {
        // Fix taken 2 s before now on the elapsed clock: its wall time is now minus 2 s, whatever Location.time says.
        assertEquals(98_000L, FixTime.wallMs(fixTimeMs = 50_000L, fixElapsedNs = 8_000_000_000L, nowWallMs = 100_000L, nowElapsedNs = 10_000_000_000L))
    }

    @Test fun withoutAnElapsedTimeTheFixTimeIsUsedAsIs() {
        assertEquals(50_000L, FixTime.wallMs(fixTimeMs = 50_000L, fixElapsedNs = 0L, nowWallMs = 100_000L, nowElapsedNs = 10_000_000_000L))
    }

    @Test fun aBatchIsDeliveredOldestFirst() {
        // Review Focus 1: screen-off batches arrive newest first on some phones.
        val batch = listOf(sample(elapsedNs = 3L), sample(elapsedNs = 1L), sample(elapsedNs = 2L))
        assertEquals(listOf(1L, 2L, 3L), FixTime.oldestFirst(batch) { it.elapsedNs }.map { it.elapsedNs })
    }

    @Test fun aRawFixLineHasEveryFieldInTheBenchFormat() {
        val f = RawLines.fix(sample())
        assertEquals(
            listOf("tf", "ert", "lat", "lon", "acc", "spd", "spd_acc", "brg", "brg_acc", "alt", "valt", "prov", "mock"),
            f.keys.toList(),
        )
        assertEquals(1_000L, f["tf"])
        assertEquals("fused", f["prov"])
    }

    @Test fun missingFieldsAreNullNotZero() {
        val f = RawLines.fix(sample().copy(speedMps = null, bearingDeg = null, accuracyM = null))
        assertEquals(null, f["spd"])
        assertEquals(null, f["brg"])
        assertEquals(null, f["acc"])
    }

    @Test fun stepHeadingAndStateLinesCarryTheirFields() {
        assertEquals(mapOf("total" to 12L, "te" to 34L), RawLines.steps(12L, 34L))
        assertEquals(mapOf("az" to 270.0, "acc" to "high"), RawLines.heading(270.0, "high"))
        assertEquals(
            mapOf("presence" to "InZone", "counting" to true, "zone" to "inside", "app_visible" to false),
            RawLines.state("InZone", true, "inside", false),
        )
    }
}
```

Append to `android/app/src/test/java/dev/apgo2/DiagLogTest.kt` (inside `class DiagLogTest`):

```kotlin
    @Test fun aSecondLogWithItsOwnPrefixRotatesOnItsOwnFiles() {
        val dir = tmp.newFolder()
        val main = DiagLog(dir, 1_000_000, 5, { 1L })
        val raw = DiagLog(java.io.File(dir, "raw"), maxFileBytes = 120, keep = 2, clock = { 1L }, prefix = "raw")
        main.write("I", "a", "b")
        repeat(10) { raw.write("I", "rawfix", "", mapOf("tf" to it)) }
        assertEquals(listOf("diag-0001.jsonl"), main.files().map { it.name })
        assertEquals(2, raw.files().size)
        assertTrue(raw.files().all { it.name.matches(Regex("raw-\\d{4}\\.jsonl")) })
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.RawTrackTest' --tests 'dev.apgo2.DiagLogTest' --console=plain -q`
Expected: FAIL to compile: `Unresolved reference 'FixSample'`, `Unresolved reference 'FixTime'`, `No parameter with name 'prefix' found`.

- [ ] **Step 3: Write `RawTrack.kt`**

```kotlin
package dev.apgo2

import android.location.Location
import android.os.Build

private const val NANOS_PER_MS = 1_000_000L

/** One location fix as the phone delivered it, every field the filter and the bench can use; `null` = the phone did not say. */
internal data class FixSample(
    val tMs: Long,
    val elapsedNs: Long,
    val lat: Double,
    val lon: Double,
    val accuracyM: Float?,
    val speedMps: Float?,
    val speedAccMps: Float?,
    val bearingDeg: Float?,
    val bearingAccDeg: Float?,
    val altitudeM: Double?,
    val verticalAccM: Float?,
    val provider: String,
    val mock: Boolean,
)

/** When a fix was taken. The fix's own clock, not the moment it reached the app (batched fixes arrive seconds late). */
internal object FixTime {
    /**
     * Wall time of a fix in ms: the monotonic `elapsedRealtimeNanos` of the fix measured back from now (immune to a wrong GPS or
     * wall clock), or `Location.time` when the phone did not fill in the elapsed time.
     */
    fun wallMs(
        fixTimeMs: Long,
        fixElapsedNs: Long,
        nowWallMs: Long,
        nowElapsedNs: Long,
    ): Long = if (fixElapsedNs > 0L) nowWallMs - (nowElapsedNs - fixElapsedNs) / NANOS_PER_MS else fixTimeMs

    /** A batch in the order the fixes were taken. */
    fun <T> oldestFirst(
        items: List<T>,
        elapsedNs: (T) -> Long,
    ): List<T> = items.sortedBy(elapsedNs)
}

/** The fields of the raw-track lines (`diag/raw/raw-NNNN.jsonl`, debug builds only), in the order the replay bench reads them. */
internal object RawLines {
    fun fix(s: FixSample): Map<String, Any?> =
        linkedMapOf(
            "tf" to s.tMs,
            "ert" to s.elapsedNs,
            "lat" to s.lat,
            "lon" to s.lon,
            "acc" to s.accuracyM,
            "spd" to s.speedMps,
            "spd_acc" to s.speedAccMps,
            "brg" to s.bearingDeg,
            "brg_acc" to s.bearingAccDeg,
            "alt" to s.altitudeM,
            "valt" to s.verticalAccM,
            "prov" to s.provider,
            "mock" to s.mock,
        )

    fun steps(
        total: Long,
        eventMs: Long,
    ): Map<String, Any?> = linkedMapOf("total" to total, "te" to eventMs)

    fun heading(
        azimuthDeg: Double,
        accuracy: String,
    ): Map<String, Any?> = linkedMapOf("az" to azimuthDeg, "acc" to accuracy)

    fun state(
        presence: String,
        counting: Boolean,
        zone: String,
        appVisible: Boolean,
    ): Map<String, Any?> = linkedMapOf("presence" to presence, "counting" to counting, "zone" to zone, "app_visible" to appVisible)
}

/** Reads a [FixSample] off an Android [Location] (thin: the logic is in [FixTime]). */
internal object FixSamples {
    fun of(
        loc: Location,
        nowWallMs: Long,
        nowElapsedNs: Long,
    ): FixSample =
        FixSample(
            tMs = FixTime.wallMs(loc.time, loc.elapsedRealtimeNanos, nowWallMs, nowElapsedNs),
            elapsedNs = loc.elapsedRealtimeNanos,
            lat = loc.latitude,
            lon = loc.longitude,
            accuracyM = loc.accuracy.takeIf { loc.hasAccuracy() },
            speedMps = loc.speed.takeIf { loc.hasSpeed() },
            speedAccMps = loc.speedAccuracyMetersPerSecond.takeIf { loc.hasSpeedAccuracy() },
            bearingDeg = loc.bearing.takeIf { loc.hasBearing() },
            bearingAccDeg = loc.bearingAccuracyDegrees.takeIf { loc.hasBearingAccuracy() },
            altitudeM = loc.altitude.takeIf { loc.hasAltitude() },
            verticalAccM = loc.verticalAccuracyMeters.takeIf { loc.hasVerticalAccuracy() },
            provider = loc.provider ?: "?",
            mock = isMock(loc),
        )

    // isFromMockProvider is deprecated from Android 12, where isMock replaces it; older phones only have the old call.
    @Suppress("DEPRECATION")
    private fun isMock(loc: Location): Boolean = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) loc.isMock else loc.isFromMockProvider
}
```

- [ ] **Step 4: Give `DiagLog` a file prefix and add the raw log to `Diag`**

In `DiagLog.kt` change the constructor and the file helpers (the rest of the class is unchanged):

```kotlin
internal class DiagLog(
    private val dir: File,
    private val maxFileBytes: Long = 2_000_000,
    private val keep: Int = 10,
    private val clock: () -> Long = System::currentTimeMillis,
    private val prefix: String = "diag",
) {
    private val pattern = Regex("${Regex.escape(prefix)}-(\\d{4})\\.jsonl")
```

```kotlin
    /** Log files, oldest first. */
    fun files(): List<File> = dir.listFiles { f -> f.isFile && pattern.matches(f.name) }?.sortedBy { seq(it) } ?: emptyList()

    private fun seq(f: File) = pattern.matchEntire(f.name)!!.groupValues[1].toInt()

    private fun name(n: Int) = "$prefix-%04d.jsonl".format(n)
```

Delete the `companion object { private val PATTERN ... }`. In `object Diag` add:

```kotlin
    @Volatile private var rawLog: DiagLog? = null

    /**
     * Debug builds only: raw fixes, steps and headings for the replay bench, in `diag/raw/` (10 x 2 MB of their own, so they never push
     * the normal log out). Release builds never call this, so they never store raw positions.
     */
    fun initRaw(ctx: android.content.Context) {
        val debuggable = ctx.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0
        if (!debuggable) return
        val base = ctx.getExternalFilesDir(null) ?: ctx.filesDir
        rawLog = DiagLog(java.io.File(base, "diag/raw"), prefix = "raw")
    }

    /** A raw-track line (no-op unless [initRaw] enabled it). */
    fun raw(
        tag: String,
        fields: Map<String, Any?>,
    ) {
        rawLog?.write("I", tag, "", fields)
    }
```

In `ApgoApp.onCreate` call `Diag.initRaw(this)` right after `Diag.init(this)`.

- [ ] **Step 5: Deliver batches oldest first, record raw lines, use the fix time**

In `Sensors.startLocation` replace the `LocationListener { loc -> ... }` lambda with:

```kotlin
        val l =
            object : LocationListener {
                override fun onLocationChanged(loc: Location) = deliver(loc)

                // Android 12+ hands a screen-off batch over at once; the filter needs them in the order they were taken.
                override fun onLocationChanged(locations: List<Location>) = FixTime.oldestFirst(locations) { it.elapsedRealtimeNanos }.forEach(::deliver)
            }
```

and add to `Sensors`:

```kotlin
    private fun deliver(loc: Location) {
        val sample = FixSamples.of(loc, System.currentTimeMillis(), SystemClock.elapsedRealtimeNanos())
        Diag.raw("rawfix", RawLines.fix(sample))
        model.realLoc = loc
        model.onFix(loc, sample)
    }
```

(imports `android.location.Location`, `android.os.SystemClock`). In `startSteps` the listener body becomes:

```kotlin
                override fun onSensorChanged(e: SensorEvent) {
                    val total = e.values[0].toLong()
                    val eventMs = FixTime.wallMs(0L, e.timestamp, System.currentTimeMillis(), SystemClock.elapsedRealtimeNanos())
                    Diag.raw("rawsteps", RawLines.steps(total, eventMs))
                    model.onSteps(total, eventMs)
                }
```

In `AppModel`:

```kotlin
    /** The phone's step counter changed at [eventMs]: credit it to the open game (the engine ignores it when no game is open). */
    fun onSteps(
        total: Long,
        eventMs: Long = now(),
    ) {
        stepsTotal = total
        if (!engine.hasGame()) return
        val events = engine.onSteps(total, eventMs)
        handle(events)
        if (events.isNotEmpty() || stepRefreshThrottle.due(now())) refreshPlay(withTrace = false)
    }

    /** A location fix arrived: feed the engine (at the time the fix was taken), update presence and the screen. */
    fun onFix(
        loc: Location,
        sample: FixSample,
    ) {
        if (!engine.hasGame() || simPos != null) return
        diag.recordFix(loc)
        handle(engine.onFix(loc.latitude, loc.longitude, sample.tMs, loc.accuracy.toDouble(), stepsTotal, false))
        presence.updateZone(engine.zoneProximity(loc.latitude, loc.longitude))
        presence.evaluate()
        refreshPlay(withTrace = traceThrottle.due(now()))
        diag.logProgress()
    }
```

In `PresenceController.evaluate`, just after `decision = d` (the "changed" branch), add the raw state line, and also log it when the
visibility changes: replace `var appVisible = true` with

```kotlin
    var appVisible = true
        set(v) {
            if (field != v) Diag.raw("rawstate", RawLines.state(decision.state.name, decision.counting, zone.name.lowercase(), v))
            field = v
        }
```

and after `decision = d`:

```kotlin
        Diag.raw("rawstate", RawLines.state(d.state.name, d.counting, zone.name.lowercase(), appVisible))
```

- [ ] **Step 6: Run the tests and the Android gate**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.RawTrackTest' --tests 'dev.apgo2.DiagLogTest' --console=plain -q`
Expected: PASS.
Run: `ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS (spotless, detekt, lint, tests, Kover >= 18).

- [ ] **Step 7: Commit**

```bash
git add android/app/src/main/java/dev/apgo2/RawTrack.kt android/app/src/main/java/dev/apgo2/DiagLog.kt \
  android/app/src/main/java/dev/apgo2/Sensors.kt android/app/src/main/java/dev/apgo2/AppModel.kt \
  android/app/src/main/java/dev/apgo2/ApgoApp.kt android/app/src/main/java/dev/apgo2/PresenceController.kt \
  android/app/src/test/java/dev/apgo2/RawTrackTest.kt android/app/src/test/java/dev/apgo2/DiagLogTest.kt
git commit -m "feat: record raw fixes in debug builds" -m "Raw fixes, steps and presence changes go to diag/raw for the replay
bench; fixes are timed by their own clock and batches are processed
oldest first." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 2: `loc` types, ENU frame and the synthetic walk generator

**Files:**

- Create: `core/src/loc/mod.rs`, `core/src/loc/frame.rs`, `core/src/loc/bench.rs`, `core/src/loc/bench/synth.rs`
- Create: `core/tests/loc_scenarios.rs`
- Modify: `core/src/lib.rs` (add `pub mod loc;`)

**Interfaces:**

- Produces (used by every later core task):
  - `loc::Provider { Fused, Gps, Network, Ios, Sim, Other }` with `parse(&str)`, `name()`, `is_gnss()`.
  - `loc::RawFix { t_ms: i64, lat, lon, accuracy_m: f64, speed_mps, speed_acc_mps, bearing_deg, bearing_acc_deg, altitude_m, vertical_acc_m: Option<f64>, provider: Provider, mock: bool }`, `RawFix::at(lat, lon, t_ms, accuracy_m)`, `RawFix::point()`.
  - `loc::Motion { Stationary, Walking, Fast }`, `loc::Source { Gps, Bridged, Predicted }`,
    `loc::Verdict { Used, Soft, Gated, Blurry, Relocated, Reset, Unusable }` with `may_count()`.
  - `loc::Estimate` (fields exactly as the spec), `Estimate::point()`, `Estimate::exact(lat, lon, t_ms)`.
  - `loc::CompassAccuracy { High, Medium, Low, Unreliable }` with `sigma_deg() -> Option<f64>` and `parse(&str)`,
    `loc::HeadingIn { t_ms: i64, azimuth_deg: f64, accuracy: CompassAccuracy, pitch_deg: f64, roll_deg: f64 }`.
  - `loc::MAX_UNCERTAINTY_M = 35.0`, `loc::ACC_TO_SIGMA = 1.515`, `loc::mode_at(&[(Shape, Mode)], Point) -> Option<Mode>`.
  - `loc::frame::Frame { new(Point), origin(), to_enu(Point) -> [f64; 2], to_geo([f64; 2]) -> Point, needs_reanchor([f64; 2]) }`, `REANCHOR_M = 5000.0`.
  - `loc::bench::{Leg, Scenario, Spike, HeadingSim, Run, TruthPoint, gauss, truth_at}`; `Scenario::generate(seed) -> Run`.

- [ ] **Step 1: Write the failing tests**

Unit tests at the bottom of `core/src/loc/frame.rs` (the file starts with the code of Step 3; write the test module first and a stub-free
file will not compile, which is the RED):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{destination, distance_m};

    #[test]
    fn enu_round_trip_is_exact_and_distances_match_great_circle_within_half_a_percent() {
        let o = Point::new(40.76, -111.89);
        let f = Frame::new(o);
        for (b, d) in [(0.0, 1000.0), (90.0, 2500.0), (225.0, 4000.0)] {
            let p = destination(o, b, d);
            let en = f.to_enu(p);
            let back = f.to_geo(en);
            assert!((back.lat - p.lat).abs() < 1e-12 && (back.lon - p.lon).abs() < 1e-12);
            assert!((en[0].hypot(en[1]) - distance_m(o, p)).abs() < d * 0.005, "{b} deg {d} m: {en:?}");
        }
        assert_eq!(f.to_enu(o), [0.0, 0.0]);
    }

    #[test]
    fn east_is_x_and_north_is_y() {
        let o = Point::new(10.0, 20.0);
        let f = Frame::new(o);
        let e = f.to_enu(destination(o, 90.0, 100.0));
        let n = f.to_enu(destination(o, 0.0, 100.0));
        assert!(e[0] > 99.0 && e[1].abs() < 0.1, "{e:?}");
        assert!(n[1] > 99.0 && n[0].abs() < 0.1, "{n:?}");
    }

    #[test]
    fn reanchor_is_due_from_five_km() {
        let f = Frame::new(Point::new(0.0, 0.0));
        assert!(!f.needs_reanchor([3000.0, 3999.0]));
        assert!(f.needs_reanchor([3000.0, 4001.0]));
    }
}
```

Unit tests at the bottom of `core/src/loc/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::realm::Shape;

    #[test]
    fn only_used_soft_relocated_and_reset_may_count() {
        let yes = [Verdict::Used, Verdict::Soft, Verdict::Relocated, Verdict::Reset];
        let no = [Verdict::Gated, Verdict::Blurry, Verdict::Unusable];
        assert!(yes.iter().all(|v| v.may_count()) && no.iter().all(|v| !v.may_count()));
    }

    #[test]
    fn providers_parse_and_only_satellite_ones_are_gnss() {
        assert_eq!(Provider::parse("gps"), Provider::Gps);
        assert_eq!(Provider::parse("fused"), Provider::Fused);
        assert_eq!(Provider::parse("network"), Provider::Network);
        assert_eq!(Provider::parse("whatever"), Provider::Other);
        assert!(Provider::Fused.is_gnss() && Provider::Gps.is_gnss() && !Provider::Network.is_gnss() && !Provider::Sim.is_gnss());
        assert_eq!(Provider::parse(Provider::Ios.name()), Provider::Ios);
    }

    #[test]
    fn compass_accuracy_maps_to_the_spec_sigmas() {
        assert_eq!(CompassAccuracy::High.sigma_deg(), Some(15.0));
        assert_eq!(CompassAccuracy::Medium.sigma_deg(), Some(30.0));
        assert_eq!(CompassAccuracy::Low.sigma_deg(), Some(45.0));
        assert_eq!(CompassAccuracy::Unreliable.sigma_deg(), None);
        assert_eq!(CompassAccuracy::parse("medium"), CompassAccuracy::Medium);
        assert_eq!(CompassAccuracy::parse("?"), CompassAccuracy::Unreliable);
    }

    #[test]
    fn an_exact_estimate_is_accepted_at_three_metres() {
        let e = Estimate::exact(40.0, -111.0, 5_000);
        assert!(e.accepted && e.verdict == Verdict::Used && e.source == Source::Gps);
        assert!((e.uncertainty_m - 3.0).abs() < 1e-9 && e.point() == Point::new(40.0, -111.0));
    }

    #[test]
    fn the_mode_is_the_containing_zones_else_the_fastest_of_the_game() {
        let c = |lat: f64| Shape::Circle { center: Point::new(lat, 0.0), radius_m: 500.0 };
        let zones = [(c(0.0), Mode::Walk), (c(1.0), Mode::Bike)];
        assert_eq!(mode_at(&zones, Point::new(0.0, 0.0)), Some(Mode::Walk));
        assert_eq!(mode_at(&zones, Point::new(1.0, 0.0)), Some(Mode::Bike));
        assert_eq!(mode_at(&zones, Point::new(0.5, 0.0)), Some(Mode::Bike), "between zones: the fastest mode");
        assert_eq!(mode_at(&[], Point::new(0.5, 0.0)), None);
    }
}
```

The first scenario-file tests, `core/tests/loc_scenarios.rs` (later tasks append to this file):

```rust
//! Synthetic walks with known truth: the CI thresholds of the location program (spec "CI synthetic walks").

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)] // test-sized counts and seeds

use apgo_core::geo::{distance_m, Point};
use apgo_core::loc::bench::{Leg, Scenario};

const SEEDS: u64 = 20;

fn origin() -> Point {
    Point::new(40.0, -111.0)
}

#[test]
fn the_generator_is_deterministic_per_seed_and_differs_between_seeds() {
    let s = Scenario::walk(origin(), vec![Leg::Move { bearing_deg: 90.0, dist_m: 300.0, speed_mps: 1.4 }], 5.0);
    let (a, b, c) = (s.generate(1), s.generate(1), s.generate(2));
    assert_eq!(a.fixes, b.fixes);
    assert_ne!(a.fixes, c.fixes);
    assert_eq!(a.truth.len(), b.truth.len());
}

#[test]
fn truth_follows_the_legs_at_their_speed() {
    let s = Scenario::walk(origin(), vec![Leg::Move { bearing_deg: 0.0, dist_m: 140.0, speed_mps: 1.4 }, Leg::Stop { secs: 30 }], 5.0);
    let r = s.generate(1);
    assert_eq!(r.truth.len(), 131, "100 s of walking and 30 s standing, one truth point per second plus the start");
    let end = r.truth.last().unwrap();
    assert!((distance_m(origin(), end.p) - 140.0).abs() < 0.5);
    assert!(r.truth[50].speed_mps > 1.3 && end.speed_mps == 0.0);
}

#[test]
fn gnss_error_has_the_scenario_sigma_and_is_correlated_in_time() {
    let s = Scenario::walk(origin(), vec![Leg::Stop { secs: 3600 }], 8.0);
    let (mut sq, mut n, mut lag1) = (0.0, 0.0, 0.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let err: Vec<f64> = r.fixes.iter().map(|f| distance_m(f.point(), origin())).collect();
        sq += err.iter().map(|e| e * e).sum::<f64>();
        n += err.len() as f64;
        lag1 += r.fixes.windows(2).map(|w| distance_m(w[0].point(), w[1].point())).sum::<f64>() / err.len() as f64;
    }
    let rms = (sq / n).sqrt();
    let sigma = 8.0 / 1.515;
    assert!((rms - sigma * 2f64.sqrt()).abs() < sigma * 0.25, "2-D rms {rms} vs per-axis sigma {sigma}");
    assert!(lag1 / SEEDS as f64 < rms, "consecutive fixes are closer than independent ones would be (AR(1) drift)");
}

#[test]
fn reported_accuracy_jitters_thirty_percent_around_the_true_one() {
    let r = Scenario::walk(origin(), vec![Leg::Stop { secs: 600 }], 10.0).generate(3);
    assert!(r.fixes.iter().all(|f| (7.0..=13.0).contains(&f.accuracy_m)), "acc 10 m +-30 %");
}

#[test]
fn spikes_gaps_steps_and_headings_appear_where_the_scenario_puts_them() {
    let mut s = Scenario::walk(origin(), vec![Leg::Move { bearing_deg: 90.0, dist_m: 420.0, speed_mps: 1.4 }], 5.0);
    s.spikes = vec![apgo_core::loc::bench::Spike { at_s: 60, offset_m: 150.0, bearing_deg: 0.0, len: 1 }];
    s.gaps = vec![(100, 160)];
    s.steps = true;
    s.heading = Some(apgo_core::loc::bench::HeadingSim::pocket(vec![(0, 90.0)]));
    let r = s.generate(5);
    let spike = r.fixes.iter().find(|f| f.t_ms == s.t0_ms + 60_000).unwrap();
    let truth = r.truth.iter().find(|t| t.t_ms == spike.t_ms).unwrap();
    assert!(distance_m(spike.point(), truth.p) > 120.0);
    assert!(r.fixes.iter().all(|f| !(s.t0_ms + 100_000..s.t0_ms + 160_000).contains(&f.t_ms)), "no fixes in the gap");
    assert!(r.steps.len() > 100 && r.steps.windows(2).all(|w| w[1].1 >= w[0].1), "cumulative steps every 2 s");
    let walked = (r.steps.last().unwrap().1 - r.steps[0].1) as f64;
    assert!((walked * 0.73 - 420.0).abs() < 30.0, "about 0.73 m a step at 1.4 m/s: {walked} steps");
    let h = &r.headings[200];
    let offset = (truth_course_at(&r, h.t_ms) - h.azimuth_deg).rem_euclid(360.0);
    assert!((offset - 90.0).abs() < 10.0, "a pocketed phone points 90 deg off the walking direction: {offset}");
}

fn truth_course_at(r: &apgo_core::loc::bench::Run, t_ms: i64) -> f64 {
    apgo_core::loc::bench::truth_at(&r.truth, t_ms).course_deg
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc`
Expected: FAIL to compile: `could not find 'loc' in 'apgo_core'` (scenarios) and `file not found for module 'loc'` once `lib.rs` names it.

- [ ] **Step 3: Write `core/src/loc/frame.rs`** (above its test module)

```rust
//! Local east-north frame (a tangent plane around an anchor): the filters work in metres, not degrees.

use crate::geo::Point;

const M_PER_DEG: f64 = 111_195.0;
/// The estimate moves to a new anchor once it is this far from the old one, so the flat-earth error stays small.
pub const REANCHOR_M: f64 = 5_000.0;

/// An anchor point and the length of a degree of longitude there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    origin: Point,
    m_per_deg_lon: f64,
}

impl Frame {
    /// A frame anchored at `origin`.
    #[must_use]
    pub fn new(origin: Point) -> Self {
        Self { origin, m_per_deg_lon: M_PER_DEG * origin.lat.to_radians().cos().max(1e-6) }
    }

    /// The anchor.
    #[must_use]
    pub fn origin(&self) -> Point {
        self.origin
    }

    /// `p` as metres east and north of the anchor.
    #[must_use]
    pub fn to_enu(&self, p: Point) -> [f64; 2] {
        [(p.lon - self.origin.lon) * self.m_per_deg_lon, (p.lat - self.origin.lat) * M_PER_DEG]
    }

    /// Metres east and north of the anchor as a point.
    #[must_use]
    pub fn to_geo(&self, en: [f64; 2]) -> Point {
        Point::new(self.origin.lat + en[1] / M_PER_DEG, self.origin.lon + en[0] / self.m_per_deg_lon)
    }

    /// Whether a position this far out should move the anchor.
    #[must_use]
    pub fn needs_reanchor(&self, en: [f64; 2]) -> bool {
        en[0].hypot(en[1]) >= REANCHOR_M
    }
}
```

- [ ] **Step 4: Write `core/src/loc/mod.rs`** (above its test module; later tasks add submodules and the `Locator`)

```rust
//! Location estimation: one source of truth for where the player is (an IMM Kalman filter), what the map shows (map matching) and how a
//! GPS gap is bridged (steps and heading). Spec: `docs/superpowers/specs/2026-10-08-location-quality-design.md`.

pub mod bench;
pub mod frame;

use serde::{Deserialize, Serialize};

use crate::catalog::Mode;
use crate::geo::Point;
use crate::realm::Shape;

/// An estimate this uncertain (68 % radius, metres) or worse may not complete or advance a quest. The old raw-fix limit, now on the estimate.
pub const MAX_UNCERTAINTY_M: f64 = 35.0;
/// Android's `accuracy` (and iOS `horizontalAccuracy`) is the 68 % radius: for a circular 2-D Gaussian that is 1.515 sigma per axis.
pub const ACC_TO_SIGMA: f64 = 1.515;

/// Where a fix came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// Android's fused provider (HIGH_ACCURACY: the GNSS chip when it has a fix).
    #[default]
    Fused,
    /// Android's GNSS provider.
    Gps,
    /// Wi-Fi and cell towers.
    Network,
    /// iOS Core Location.
    Ios,
    /// The developer simulator.
    Sim,
    /// Anything else.
    Other,
}

impl Provider {
    /// The provider named `s` as the phone reports it; anything unknown is [`Provider::Other`].
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "fused" => Self::Fused,
            "gps" => Self::Gps,
            "network" => Self::Network,
            "ios" => Self::Ios,
            "sim" => Self::Sim,
            _ => Self::Other,
        }
    }

    /// The name [`Self::parse`] reads.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fused => "fused",
            Self::Gps => "gps",
            Self::Network => "network",
            Self::Ios => "ios",
            Self::Sim => "sim",
            Self::Other => "other",
        }
    }

    /// Whether the fix comes from a satellite receiver.
    #[must_use]
    pub fn is_gnss(self) -> bool {
        matches!(self, Self::Fused | Self::Gps | Self::Ios)
    }
}

/// One position reading as the phone delivered it (`None` = the phone did not report that field).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct RawFix {
    /// When the fix was taken, Unix ms (the fix's own clock).
    pub t_ms: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees.
    pub lon: f64,
    /// 68 % horizontal radius, metres.
    pub accuracy_m: f64,
    /// Ground speed, m/s.
    pub speed_mps: Option<f64>,
    /// 68 % speed accuracy, m/s.
    pub speed_acc_mps: Option<f64>,
    /// Course over ground, degrees from north.
    pub bearing_deg: Option<f64>,
    /// 68 % bearing accuracy, degrees.
    pub bearing_acc_deg: Option<f64>,
    /// Altitude, metres.
    pub altitude_m: Option<f64>,
    /// 68 % vertical accuracy, metres.
    pub vertical_acc_m: Option<f64>,
    /// Which provider made it.
    pub provider: Provider,
    /// Whether a mock-location app made it.
    pub mock: bool,
}

impl RawFix {
    /// A fused fix with position, time and accuracy only.
    #[must_use]
    pub fn at(lat: f64, lon: f64, t_ms: i64, accuracy_m: f64) -> Self {
        Self { t_ms, lat, lon, accuracy_m, ..Self::default() }
    }

    /// The fix as a map point.
    #[must_use]
    pub fn point(&self) -> Point {
        Point::new(self.lat, self.lon)
    }
}

/// The motion model that explains the player best right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Motion {
    /// Standing still.
    #[default]
    Stationary,
    /// Walking or running.
    Walking,
    /// Biking or driving.
    Fast,
}

/// Where an estimate's position comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Source {
    /// A GPS fix through the filter.
    #[default]
    Gps,
    /// Steps and heading on the street graph during a GPS gap.
    Bridged,
    /// The filter's prediction with no new fix.
    Predicted,
}

/// What became of the newest fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Verdict {
    /// Used as measured.
    #[default]
    Used,
    /// Used with a down-weighted measurement (it was a bit far from the prediction).
    Soft,
    /// Rejected as a GPS jump.
    Gated,
    /// The estimate is too uncertain to count.
    Blurry,
    /// A real relocation: the filter restarted at the newest fix.
    Relocated,
    /// The filter restarted (first fix, long gap, or lost).
    Reset,
    /// Dropped before the filter (too coarse, out of order, mock, invalid).
    Unusable,
}

impl Verdict {
    /// Whether an estimate with this verdict may count for quests (if it is also sure enough and from GPS).
    #[must_use]
    pub fn may_count(self) -> bool {
        matches!(self, Self::Used | Self::Soft | Self::Relocated | Self::Reset)
    }
}

/// The filter's best estimate of where the player is. Quests, fog, chains, the odometer and the journal use this, never a raw fix.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Estimate {
    /// Time of the fix (or step batch) it is for, Unix ms.
    pub t_ms: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees.
    pub lon: f64,
    /// 68 % radius: 1.515 * sqrt(largest eigenvalue of the position covariance), metres.
    pub uncertainty_m: f64,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// One sigma of the speed, m/s.
    pub speed_sigma_mps: f64,
    /// Direction of travel, degrees from north, when moving clearly enough to say.
    pub course_deg: Option<f64>,
    /// The most likely motion model.
    pub motion: Motion,
    /// Probabilities of stationary, walking and fast.
    pub mode_probs: [f32; 3],
    /// Where the position comes from.
    pub source: Source,
    /// What became of the fix.
    pub verdict: Verdict,
    /// Whether it may complete or advance a quest.
    pub accepted: bool,
}

impl Estimate {
    /// The estimate as a map point.
    #[must_use]
    pub fn point(&self) -> Point {
        Point::new(self.lat, self.lon)
    }

    /// An accepted, exact (3 m) estimate at a point, standing still: tests and simulated fixes.
    #[must_use]
    pub fn exact(lat: f64, lon: f64, t_ms: i64) -> Self {
        Self { t_ms, lat, lon, uncertainty_m: 3.0, mode_probs: [0.0, 1.0, 0.0], motion: Motion::Walking, accepted: true, ..Self::default() }
    }
}

/// How sure the phone is of its compass (Android `SENSOR_STATUS_ACCURACY_*`, iOS `headingAccuracy` bands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CompassAccuracy {
    /// High.
    High,
    /// Medium.
    Medium,
    /// Low.
    Low,
    /// Unreliable: ignore the reading.
    #[default]
    Unreliable,
}

impl CompassAccuracy {
    /// One sigma of the azimuth, degrees (15 / 30 / 45); `None` when unreliable.
    #[must_use]
    pub fn sigma_deg(self) -> Option<f64> {
        match self {
            Self::High => Some(15.0),
            Self::Medium => Some(30.0),
            Self::Low => Some(45.0),
            Self::Unreliable => None,
        }
    }

    /// `high`, `medium`, `low`; anything else is unreliable.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "high" => Self::High,
            "medium" => Self::Medium,
            "low" => Self::Low,
            _ => Self::Unreliable,
        }
    }
}

/// One compass reading: true-north azimuth of the phone's top edge and how the phone is tilted.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct HeadingIn {
    /// When it was read, Unix ms.
    pub t_ms: i64,
    /// Azimuth, degrees from true north.
    pub azimuth_deg: f64,
    /// Sensor accuracy.
    pub accuracy: CompassAccuracy,
    /// Pitch, degrees.
    pub pitch_deg: f64,
    /// Roll, degrees.
    pub roll_deg: f64,
}

/// The travel mode at `p`: the mode of a zone that contains it (the fastest if several do), else the fastest mode of all `zones`, so a
/// cyclist between zones is never judged as a walker. `None` with no zones.
#[must_use]
pub fn mode_at(zones: &[(Shape, Mode)], p: Point) -> Option<Mode> {
    let inside = zones.iter().filter(|(s, _)| s.distance_m(p) == 0.0).map(|(_, m)| *m).max();
    inside.or_else(|| zones.iter().map(|(_, m)| *m).max())
}
```

Add `pub mod loc;` to `core/src/lib.rs` (alphabetical, after `pub mod journal;`).

- [ ] **Step 5: Write the generator** `core/src/loc/bench.rs` and `core/src/loc/bench/synth.rs`

`core/src/loc/bench.rs`:

```rust
//! The bench: synthetic walks with known truth, metrics on a shown track, today's rules as a baseline, and readers for recorded walks.
//! Shared by `core/tests/loc_scenarios.rs` and `core/examples/replay.rs`.

mod synth;

pub use synth::{gauss, truth_at, HeadingSim, Leg, Run, Scenario, Spike, TruthPoint};
```

`core/src/loc/bench/synth.rs`:

```rust
//! Synthetic walks: a seeded truth trajectory, GNSS error as AR(1) drift plus white noise, spikes, gaps, steps and compass readings.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::geo::{destination, Point};
use crate::loc::{CompassAccuracy, HeadingIn, Provider, RawFix, ACC_TO_SIGMA};
use crate::num::{i64_to_f64, round_i64};

/// One piece of the route.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Leg {
    /// Move `dist_m` on a straight line at `bearing_deg`, at `speed_mps`.
    Move {
        /// Direction, degrees from north.
        bearing_deg: f64,
        /// Length, metres.
        dist_m: f64,
        /// Speed, m/s.
        speed_mps: f64,
    },
    /// Stand still.
    Stop {
        /// How long, seconds.
        secs: u32,
    },
}

/// Fixes from `at_s` on (`len` of them) are pushed `offset_m` away at `bearing_deg` (multipath, a network fix).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spike {
    /// Seconds after the start.
    pub at_s: u32,
    /// How far off, metres.
    pub offset_m: f64,
    /// Which way, degrees from north.
    pub bearing_deg: f64,
    /// How many fixes in a row.
    pub len: u32,
}

/// How the phone's compass relates to the walking direction.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadingSim {
    /// From these seconds on, the offset `course - azimuth`, degrees (a phone in a pocket points anywhere).
    pub offset: Vec<(u32, f64)>,
    /// Compass noise, one sigma, degrees.
    pub noise_deg: f64,
    /// Reported accuracy.
    pub accuracy: CompassAccuracy,
    /// Reported pitch and roll (0, 0 = flat in the hand).
    pub tilt_deg: (f64, f64),
}

impl HeadingSim {
    /// A phone in a pocket: upright (pitch 80), medium accuracy, 5 degrees of noise.
    #[must_use]
    pub fn pocket(offset: Vec<(u32, f64)>) -> Self {
        Self { offset, noise_deg: 5.0, accuracy: CompassAccuracy::Medium, tilt_deg: (80.0, 0.0) }
    }

    /// A phone held flat in the hand pointing where the player walks.
    #[must_use]
    pub fn in_hand() -> Self {
        Self { offset: vec![(0, 0.0)], noise_deg: 3.0, accuracy: CompassAccuracy::High, tilt_deg: (10.0, 0.0) }
    }
}

/// A synthetic outing.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    /// Start point.
    pub start: Point,
    /// Start time, Unix ms.
    pub t0_ms: i64,
    /// The route.
    pub legs: Vec<Leg>,
    /// Seconds between fixes.
    pub fix_every_s: u32,
    /// True 68 % accuracy, metres; or a range sampled per fix (`acc_spread_m` > 0: acc +- spread).
    pub acc_m: f64,
    /// Fix-to-fix spread of the true accuracy, metres (the BALANCED stress uses 15).
    pub acc_spread_m: f64,
    /// Reported accuracy jitter around the true one (0.3 = +-30 %).
    pub acc_jitter: f64,
    /// AR(1) time constant of the drift, seconds.
    pub ar_tau_s: f64,
    /// Spikes and bursts.
    pub spikes: Vec<Spike>,
    /// Seconds `[from, to)` with no fixes.
    pub gaps: Vec<(u32, u32)>,
    /// Whether the phone has a step counter.
    pub steps: bool,
    /// From these seconds on, the true step-length scale against the cadence model.
    pub step_scale: Vec<(u32, f64)>,
    /// Compass readings, if any.
    pub heading: Option<HeadingSim>,
    /// Whether fixes carry speed and bearing (with accuracies).
    pub with_velocity: bool,
    /// Provider of every fix.
    pub provider: Provider,
}

/// The truth at one second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TruthPoint {
    /// Time, Unix ms.
    pub t_ms: i64,
    /// Position.
    pub p: Point,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// Direction of travel (the last one while standing), degrees.
    pub course_deg: f64,
}

/// What a scenario produced for one seed.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// One truth point per second.
    pub truth: Vec<TruthPoint>,
    /// The fixes, in time order.
    pub fixes: Vec<RawFix>,
    /// Step counter readings `(t_ms, cumulative total)` every 2 s.
    pub steps: Vec<(i64, i64)>,
    /// Compass readings at 2 Hz.
    pub headings: Vec<HeadingIn>,
}

/// One standard normal sample (Box-Muller; `rand_distr` is not a dependency).
pub fn gauss(rng: &mut StdRng) -> f64 {
    let u1: f64 = rng.random::<f64>().max(f64::MIN_POSITIVE);
    let u2: f64 = rng.random::<f64>();
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// The truth at `t_ms` (the last point at or before it; the first one before the start).
#[must_use]
pub fn truth_at(truth: &[TruthPoint], t_ms: i64) -> TruthPoint {
    let i = truth.partition_point(|t| t.t_ms <= t_ms);
    truth[i.saturating_sub(1).min(truth.len() - 1)]
}

/// Cadence (steps/s) at which the cadence model `L = 0.25 + 0.25 f` (times `scale`) covers `speed` m/s: solves `f * L(f) * scale = v`.
#[must_use]
pub fn cadence_for(speed_mps: f64, scale: f64) -> f64 {
    let a = 0.25 * scale;
    (-a + (a * a + 4.0 * a * speed_mps).sqrt()) / (2.0 * a)
}

fn value_at(table: &[(u32, f64)], s: u32, default: f64) -> f64 {
    table.iter().rev().find(|(from, _)| *from <= s).map_or(default, |(_, v)| *v)
}

impl Scenario {
    /// A walk at 1 Hz with fixes of accuracy `acc_m`, no steps, no compass, no velocity in the fixes.
    #[must_use]
    pub fn walk(start: Point, legs: Vec<Leg>, acc_m: f64) -> Self {
        Self {
            start,
            t0_ms: 1_800_000_000_000,
            legs,
            fix_every_s: 1,
            acc_m,
            acc_spread_m: 0.0,
            acc_jitter: 0.3,
            ar_tau_s: 30.0,
            spikes: vec![],
            gaps: vec![],
            steps: false,
            step_scale: vec![(0, 1.0)],
            heading: None,
            with_velocity: false,
            provider: Provider::Fused,
        }
    }

    fn truth(&self) -> Vec<TruthPoint> {
        let mut out = vec![TruthPoint { t_ms: self.t0_ms, p: self.start, speed_mps: 0.0, course_deg: 0.0 }];
        let (mut p, mut course) = (self.start, 0.0);
        for leg in &self.legs {
            match *leg {
                Leg::Move { bearing_deg, dist_m, speed_mps } => {
                    course = bearing_deg;
                    let n = round_i64(dist_m / speed_mps).max(1);
                    let step = dist_m / i64_to_f64(n);
                    for _ in 0..n {
                        p = destination(p, bearing_deg, step);
                        let t = out.last().map_or(self.t0_ms, |l| l.t_ms) + 1000;
                        out.push(TruthPoint { t_ms: t, p, speed_mps, course_deg: course });
                    }
                }
                Leg::Stop { secs } => {
                    for _ in 0..secs {
                        let t = out.last().map_or(self.t0_ms, |l| l.t_ms) + 1000;
                        out.push(TruthPoint { t_ms: t, p, speed_mps: 0.0, course_deg: course });
                    }
                }
            }
        }
        out
    }

    /// Everything the phone would have delivered on this route, for one `seed`.
    #[must_use]
    pub fn generate(&self, seed: u64) -> Run {
        let mut rng = StdRng::seed_from_u64(seed);
        let truth = self.truth();
        let a = (-1.0 / self.ar_tau_s).exp();
        let (mut ar_e, mut ar_n) = (0.0, 0.0);
        let mut fixes = Vec::new();
        for (i, tp) in truth.iter().enumerate() {
            let s = u32::try_from(i).unwrap_or(u32::MAX);
            let true_acc = (self.acc_m + self.acc_spread_m * (2.0 * rng.random::<f64>() - 1.0)).max(1.0);
            let sigma = true_acc / ACC_TO_SIGMA;
            // AR(1) drift (64 % of the variance) plus white noise (36 %): the total per-axis sigma is the scenario's.
            ar_e = a * ar_e + (1.0 - a * a).sqrt() * 0.8 * sigma * gauss(&mut rng);
            ar_n = a * ar_n + (1.0 - a * a).sqrt() * 0.8 * sigma * gauss(&mut rng);
            let (we, wn) = (0.6 * sigma * gauss(&mut rng), 0.6 * sigma * gauss(&mut rng));
            if s % self.fix_every_s.max(1) != 0 || self.gaps.iter().any(|(from, to)| (*from..*to).contains(&s)) {
                continue;
            }
            let (e, n) = (ar_e + we, ar_n + wn);
            let mut p = destination(destination(tp.p, 90.0, e), 0.0, n);
            if let Some(sp) = self.spikes.iter().find(|sp| (sp.at_s..sp.at_s + sp.len.max(1)).contains(&s)) {
                p = destination(p, sp.bearing_deg, sp.offset_m);
            }
            let reported = true_acc * (1.0 + self.acc_jitter * (2.0 * rng.random::<f64>() - 1.0));
            let mut f = RawFix { provider: self.provider, ..RawFix::at(p.lat, p.lon, tp.t_ms, reported) };
            if self.with_velocity {
                f.speed_mps = Some((tp.speed_mps + 0.3 * gauss(&mut rng)).max(0.0));
                f.speed_acc_mps = Some(0.5);
                f.bearing_deg = Some((tp.course_deg + 5.0 * gauss(&mut rng)).rem_euclid(360.0));
                f.bearing_acc_deg = Some(if tp.speed_mps > 0.5 { 10.0 } else { 90.0 });
            }
            fixes.push(f);
        }
        let steps = if self.steps { self.steps_of(&truth) } else { vec![] };
        let headings = self.heading.as_ref().map(|h| self.headings_of(h, &truth, &mut rng)).unwrap_or_default();
        Run { truth, fixes, steps, headings }
    }

    fn steps_of(&self, truth: &[TruthPoint]) -> Vec<(i64, i64)> {
        let mut total = 10_000.0_f64;
        let mut out = vec![(self.t0_ms, 10_000)];
        for (i, tp) in truth.iter().enumerate().skip(1) {
            let s = u32::try_from(i).unwrap_or(u32::MAX);
            let scale = value_at(&self.step_scale, s, 1.0);
            if tp.speed_mps > 0.0 {
                total += cadence_for(tp.speed_mps, scale);
            }
            if i % 2 == 0 {
                out.push((tp.t_ms, round_i64(total.floor())));
            }
        }
        out
    }

    fn headings_of(&self, h: &HeadingSim, truth: &[TruthPoint], rng: &mut StdRng) -> Vec<HeadingIn> {
        let mut out = Vec::new();
        for (i, tp) in truth.iter().enumerate() {
            let s = u32::try_from(i).unwrap_or(u32::MAX);
            let offset = value_at(&h.offset, s, 0.0);
            for half in 0..2_i64 {
                let azimuth = (tp.course_deg - offset + h.noise_deg * gauss(rng)).rem_euclid(360.0);
                out.push(HeadingIn { t_ms: tp.t_ms + half * 500, azimuth_deg: azimuth, accuracy: h.accuracy, pitch_deg: h.tilt_deg.0, roll_deg: h.tilt_deg.1 });
            }
        }
        out
    }
}
```

- [ ] **Step 6: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core loc:: && cargo test -p apgo-core --test loc_scenarios`
Expected: PASS (frame, mod and the five generator tests).
Run: `just check-rust`
Expected: PASS (fmt, clippy pedantic, docs, deny, coverage >= 84).

- [ ] **Step 7: Commit**

```bash
git add core/src/lib.rs core/src/loc core/tests/loc_scenarios.rs
git commit -m "feat: add loc types and synthetic walks" -m "The loc module starts with the fix and estimate types, the ENU frame
and a seeded walk generator (AR(1) GNSS error, spikes, gaps, steps,
compass) for the CI scenarios." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 3: Bench metrics, `LegacyRules` and the scorecard

**Files:**

- Create: `core/src/loc/bench/metrics.rs`, `core/src/loc/bench/legacy.rs`
- Modify: `core/src/loc/bench.rs` (module list and re-exports)
- Modify: `core/tests/loc_scenarios.rs` (append)

**Interfaces:**

- Consumes: `TruthPoint`, `truth_at`, `Scenario`, `Run` (Task 2); `RawFix`, `Verdict` (Task 2).
- Produces:
  - `bench::Shown { t_ms: i64, p: Point, uncertainty_m: f64, accepted: bool, verdict: Verdict, course_deg: Option<f64>, odometer_m: f64 }`.
  - `bench::Jitter { rms_m, path_m_per_min, drift_m_per_min, held_share: f64 }`.
  - `bench::{interp(&[TruthPoint], i64) -> Point, position_error(..) -> (f64, f64), false_jumps(..) -> usize, stationary_jitter(..) -> Option<Jitter>,
    arrival_lag_s(reference, shown, target: Point, r: f64) -> Option<f64>, completes(shown, target, r) -> bool, turn_lags_s(reference, shown) -> Vec<f64>,
    overshoot_m(reference, shown) -> f64, verdict_counts(shown) -> BTreeMap<String, usize>, virtual_targets(reference, every_m, r) -> Vec<(Point, f64)>,
    percentile(&[f64], q) -> f64, Scorecard, score(name, reference, shown, targets) -> Scorecard, columns(&[&Scorecard]) -> String}`.
  - `bench::LegacyRules { new(), feed(&RawFix) -> Shown }` and `bench::legacy_implied_speed_kmh(&RawFix, &RawFix) -> Option<f64>`.

- [ ] **Step 1: Write the failing tests**

At the bottom of `core/src/loc/bench/metrics.rs` (the module code comes in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn line(n: i64, speed: f64) -> Vec<TruthPoint> {
        let o = Point::new(40.0, -111.0);
        (0..n).map(|i| TruthPoint { t_ms: i * 1000, p: destination(o, 90.0, speed * i64_to_f64(i)), speed_mps: speed, course_deg: 90.0 }).collect()
    }

    fn shown_from(reference: &[TruthPoint], delay_s: i64) -> Vec<Shown> {
        reference
            .iter()
            .map(|t| Shown { t_ms: t.t_ms, p: truth_at(reference, t.t_ms - delay_s * 1000).p, uncertainty_m: 3.0, accepted: true, verdict: Verdict::Used, course_deg: Some(90.0), odometer_m: 0.0 })
            .collect()
    }

    #[test]
    fn a_perfect_track_has_no_error_no_jumps_and_no_lag() {
        let r = line(120, 1.4);
        let s = shown_from(&r, 0);
        let (rms, p95) = position_error(&r, &s);
        assert!(rms < 1e-6 && p95 < 1e-6);
        assert_eq!(false_jumps(&r, &s), 0);
        assert_eq!(arrival_lag_s(&r, &s, r[80].p, 25.0), Some(0.0));
    }

    #[test]
    fn a_shown_track_three_seconds_behind_arrives_three_seconds_late() {
        let r = line(120, 1.4);
        let lag = arrival_lag_s(&r, &shown_from(&r, 3), r[80].p, 25.0).unwrap();
        assert!((lag - 3.0).abs() < 1e-9, "{lag}");
    }

    #[test]
    fn a_target_the_reference_never_reaches_has_no_lag_and_one_never_shown_is_infinite() {
        let r = line(60, 1.4);
        let far = destination(r[0].p, 0.0, 500.0);
        assert_eq!(arrival_lag_s(&r, &shown_from(&r, 0), far, 25.0), None);
        let mut s = shown_from(&r, 0);
        s.iter_mut().for_each(|x| x.accepted = false);
        assert_eq!(arrival_lag_s(&r, &s, r[30].p, 25.0), Some(f64::INFINITY), "only accepted positions count");
    }

    #[test]
    fn a_hundred_metre_hop_while_walking_is_a_false_jump() {
        let r = line(60, 1.4);
        let mut s = shown_from(&r, 0);
        s[30].p = destination(s[30].p, 0.0, 100.0);
        assert_eq!(false_jumps(&r, &s), 2, "out and back");
    }

    #[test]
    fn a_frozen_position_while_standing_has_no_jitter_and_is_held() {
        let o = Point::new(40.0, -111.0);
        let r: Vec<TruthPoint> = (0..120).map(|i| TruthPoint { t_ms: i * 1000, p: o, speed_mps: 0.0, course_deg: 0.0 }).collect();
        let j = stationary_jitter(&r, &shown_from(&r, 0)).unwrap();
        assert!(j.rms_m < 1e-9 && j.path_m_per_min < 1e-9 && (j.held_share - 1.0).abs() < 1e-9);
    }

    #[test]
    fn turn_lag_counts_until_the_shown_course_is_within_twenty_degrees() {
        let o = Point::new(40.0, -111.0);
        let mut r = Vec::new();
        for i in 0..60_i64 {
            let (p, c) = if i < 30 { (destination(o, 90.0, 1.4 * i64_to_f64(i)), 90.0) } else { (destination(destination(o, 90.0, 42.0), 0.0, 1.4 * i64_to_f64(i - 30)), 0.0) };
            r.push(TruthPoint { t_ms: i * 1000, p, speed_mps: 1.4, course_deg: c });
        }
        let s: Vec<Shown> = r
            .iter()
            .map(|t| Shown { t_ms: t.t_ms, p: t.p, uncertainty_m: 3.0, accepted: true, verdict: Verdict::Used, course_deg: Some(if t.t_ms < 32_000 { 90.0 } else { 0.0 }), odometer_m: 0.0 })
            .collect();
        let lags = turn_lags_s(&r, &s);
        assert_eq!(lags.len(), 1, "{lags:?}");
        assert!((lags[0] - 2.0).abs() < 1.01, "{lags:?}");
    }

    #[test]
    fn percentiles_and_verdict_counts() {
        assert!((percentile(&[1.0, 2.0, 3.0, 4.0], 0.5) - 2.5).abs() < 1e-9);
        assert!(percentile(&[], 0.9).is_nan());
        let mut s = shown_from(&line(3, 1.0), 0);
        s[1].verdict = Verdict::Gated;
        let c = verdict_counts(&s);
        assert_eq!((c["Used"], c["Gated"]), (2, 1));
    }
}
```

At the bottom of `core/src/loc/bench/legacy.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn at(p: Point, t_s: i64, acc: f64) -> RawFix {
        RawFix::at(p.lat, p.lon, t_s * 1000, acc)
    }

    #[test]
    fn the_legacy_rules_reproduce_the_old_game_behaviour() {
        let o = Point::new(40.0, -111.0);
        let mut l = LegacyRules::new();
        assert!(l.feed(&at(o, 1000, 5.0)).accepted);
        assert_eq!(l.feed(&at(o, 1001, 40.0)).verdict, Verdict::Blurry, "worse than 35 m");
        let far = destination(o, 90.0, 5000.0);
        let v: Vec<bool> = [1003, 1006, 1009].iter().map(|t| l.feed(&at(far, *t, 5.0)).accepted).collect();
        assert_eq!(v, [false, false, true], "the third far fix in a row is believed");
        assert!(legacy_implied_speed_kmh(&at(o, 0, 5.0), &at(o, 0, 5.0)).is_none(), "same instant");
    }

    #[test]
    fn the_legacy_odometer_ignores_wobble_under_six_metres() {
        let o = Point::new(40.0, -111.0);
        let mut l = LegacyRules::new();
        let mut last = 0.0;
        for i in 0..60 {
            last = l.feed(&at(destination(o, f64::from(i * 97 % 360), 3.0 + f64::from(i % 3)), 1005 + i64::from(i) * 5, 5.0)).odometer_m;
        }
        assert!(last < 12.0, "{last}");
    }
}
```

Append to `core/tests/loc_scenarios.rs`:

```rust
use apgo_core::loc::bench::{stationary_jitter, LegacyRules};

#[test]
fn the_bench_sees_the_standing_jitter_of_the_legacy_rules() {
    // Baseline: the old map showed every raw fix, so standing still wanders by metres. The IMM must beat this (Task 8).
    let s = Scenario::walk(origin(), vec![Leg::Stop { secs: 300 }], 8.0);
    let r = s.generate(1);
    let mut legacy = LegacyRules::new();
    let shown: Vec<_> = r.fixes.iter().map(|f| legacy.feed(f)).collect();
    let j = stationary_jitter(&r.truth, &shown).unwrap();
    assert!(j.rms_m > 2.0, "raw fixes jitter {} m", j.rms_m);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core bench`
Expected: FAIL to compile: `cannot find type 'Shown'`, `cannot find function 'position_error'`, `cannot find struct 'LegacyRules'`.

- [ ] **Step 3: Write `core/src/loc/bench/metrics.rs`**

```rust
//! Scorecard metrics of a shown track against a reference (the truth of a synthetic walk, or the smoothed track of a real one).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::geo::{bearing_deg, distance_m, distance_to_segment_m, Point};
use crate::loc::bench::{truth_at, TruthPoint};
use crate::loc::Verdict;
use crate::num::{count_f64, i64_to_f64};

/// One position as a filter (or the legacy rules) showed it after a fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown {
    /// Time, Unix ms.
    pub t_ms: i64,
    /// Shown position.
    pub p: Point,
    /// Its 68 % radius, metres.
    pub uncertainty_m: f64,
    /// Whether quests could use it.
    pub accepted: bool,
    /// What became of the fix.
    pub verdict: Verdict,
    /// Shown course, degrees.
    pub course_deg: Option<f64>,
    /// Odometer after this fix, metres.
    pub odometer_m: f64,
}

/// How still the shown position stays while the reference stands still.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jitter {
    /// RMS distance from each window's median position, metres.
    pub rms_m: f64,
    /// Shown path length per minute standing, metres.
    pub path_m_per_min: f64,
    /// Worst first-to-last drift of a window per minute, metres.
    pub drift_m_per_min: f64,
    /// Share of consecutive shown positions that did not move at all.
    pub held_share: f64,
}

/// The `q` quantile (0..1, linear between ranks) of `v`; NaN when empty.
#[must_use]
pub fn percentile(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let rank = q.clamp(0.0, 1.0) * count_f64(s.len() - 1);
    let (lo, hi) = (rank.floor(), rank.ceil());
    let (a, b) = (s[crate::num::floor_usize(lo)], s[crate::num::floor_usize(hi)]);
    a + (b - a) * (rank - lo)
}

/// The reference position at `t_ms`, linear between its points.
#[must_use]
pub fn interp(reference: &[TruthPoint], t_ms: i64) -> Point {
    let i = reference.partition_point(|t| t.t_ms <= t_ms);
    if i == 0 || i >= reference.len() {
        return truth_at(reference, t_ms).p;
    }
    let (a, b) = (reference[i - 1], reference[i]);
    let f = i64_to_f64(t_ms - a.t_ms) / i64_to_f64((b.t_ms - a.t_ms).max(1));
    Point::new(a.p.lat + (b.p.lat - a.p.lat) * f, a.p.lon + (b.p.lon - a.p.lon) * f)
}

/// RMS and 95th percentile of the distance from each shown position to the reference at the same time, metres.
#[must_use]
pub fn position_error(reference: &[TruthPoint], shown: &[Shown]) -> (f64, f64) {
    let e: Vec<f64> = shown.iter().map(|s| distance_m(s.p, interp(reference, s.t_ms))).collect();
    let rms = (e.iter().map(|x| x * x).sum::<f64>() / count_f64(e.len().max(1))).sqrt();
    (rms, percentile(&e, 0.95))
}

/// Shown steps longer than `max(15 m, 3 x (reference speed x dt + uncertainty))` while the reference moves slower than 2 m/s.
#[must_use]
pub fn false_jumps(reference: &[TruthPoint], shown: &[Shown]) -> usize {
    shown
        .windows(2)
        .filter(|w| {
            let ref_speed = truth_at(reference, w[1].t_ms).speed_mps;
            let dt = i64_to_f64(w[1].t_ms - w[0].t_ms) / 1000.0;
            ref_speed < 2.0 && distance_m(w[0].p, w[1].p) > (3.0 * (ref_speed * dt + w[1].uncertainty_m)).max(15.0)
        })
        .count()
}

/// Time ranges (ms) of at least 20 s in which the reference does not move.
fn still_windows(reference: &[TruthPoint]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut start: Option<i64> = None;
    for t in reference {
        // 0.2 m/s: a smoothed real track never reads exactly 0 while the player stands.
        match (t.speed_mps < 0.2, start) {
            (true, None) => start = Some(t.t_ms),
            (false, Some(s)) => {
                if t.t_ms - s >= 20_000 {
                    out.push((s, t.t_ms));
                }
                start = None;
            }
            _ => {}
        }
    }
    if let (Some(s), Some(last)) = (start, reference.last()) {
        if last.t_ms - s >= 20_000 {
            out.push((s, last.t_ms));
        }
    }
    out
}

fn median_point(ps: &[Point]) -> Point {
    let lat: Vec<f64> = ps.iter().map(|p| p.lat).collect();
    let lon: Vec<f64> = ps.iter().map(|p| p.lon).collect();
    Point::new(percentile(&lat, 0.5), percentile(&lon, 0.5))
}

/// Jitter of the shown position over every still window of the reference; `None` if the reference never stands still for 20 s.
#[must_use]
pub fn stationary_jitter(reference: &[TruthPoint], shown: &[Shown]) -> Option<Jitter> {
    let (mut sq, mut n, mut path, mut minutes, mut held, mut pairs, mut drift) = (0.0, 0usize, 0.0, 0.0, 0usize, 0usize, 0.0_f64);
    for (a, b) in still_windows(reference) {
        let w: Vec<&Shown> = shown.iter().filter(|s| (a..=b).contains(&s.t_ms)).collect();
        if w.len() < 2 {
            continue;
        }
        let ps: Vec<Point> = w.iter().map(|s| s.p).collect();
        let m = median_point(&ps);
        sq += ps.iter().map(|p| distance_m(*p, m).powi(2)).sum::<f64>();
        n += ps.len();
        let len: f64 = ps.windows(2).map(|x| distance_m(x[0], x[1])).sum();
        let mins = i64_to_f64(b - a) / 60_000.0;
        path += len;
        minutes += mins;
        held += ps.windows(2).filter(|x| x[0] == x[1]).count();
        pairs += ps.len() - 1;
        drift = drift.max(distance_m(ps[0], ps[ps.len() - 1]) / mins);
    }
    (n > 0).then(|| Jitter {
        rms_m: (sq / count_f64(n)).sqrt(),
        path_m_per_min: path / minutes,
        drift_m_per_min: drift,
        held_share: count_f64(held) / count_f64(pairs.max(1)),
    })
}

/// Seconds from the reference entering the circle (`target`, `r`) to the first accepted shown position inside it (negative = early). `None`
/// when the reference never enters; infinity when the shown track never does.
#[must_use]
pub fn arrival_lag_s(reference: &[TruthPoint], shown: &[Shown], target: Point, r: f64) -> Option<f64> {
    let t_ref = reference.iter().find(|t| distance_m(t.p, target) <= r)?.t_ms;
    Some(shown.iter().find(|s| s.accepted && distance_m(s.p, target) <= r).map_or(f64::INFINITY, |s| i64_to_f64(s.t_ms - t_ref) / 1000.0))
}

/// Whether an accepted shown position ever enters the circle: the quest would complete.
#[must_use]
pub fn completes(shown: &[Shown], target: Point, r: f64) -> bool {
    shown.iter().any(|s| s.accepted && distance_m(s.p, target) <= r)
}

fn wrap_deg(d: f64) -> f64 {
    (d + 540.0).rem_euclid(360.0) - 180.0
}

/// Turns of the reference (course change over 60 degrees between the 10 m before and the 10 m after a point): (time, point, new course).
fn turns(reference: &[TruthPoint]) -> Vec<(i64, Point, f64)> {
    let mut out: Vec<(i64, Point, f64)> = Vec::new();
    for (i, t) in reference.iter().enumerate() {
        let before = reference[..i].iter().rev().find(|b| distance_m(b.p, t.p) >= 10.0);
        let after = reference[i + 1..].iter().find(|a| distance_m(a.p, t.p) >= 10.0);
        let (Some(b), Some(a)) = (before, after) else { continue };
        let (c_in, c_out) = (bearing_deg(b.p, t.p), bearing_deg(t.p, a.p));
        if wrap_deg(c_out - c_in).abs() > 60.0 && out.last().is_none_or(|l| distance_m(l.1, t.p) > 20.0) {
            out.push((t.t_ms, t.p, c_out));
        }
    }
    out
}

/// Seconds from each turn of the reference until the shown course is within 20 degrees of the new course (infinity if never).
#[must_use]
pub fn turn_lags_s(reference: &[TruthPoint], shown: &[Shown]) -> Vec<f64> {
    turns(reference)
        .into_iter()
        .map(|(t, _, c)| {
            shown
                .iter()
                .filter(|s| s.t_ms >= t)
                .find(|s| s.course_deg.is_some_and(|sc| wrap_deg(sc - c).abs() <= 20.0))
                .map_or(f64::INFINITY, |s| i64_to_f64(s.t_ms - t) / 1000.0)
        })
        .collect()
}

/// Worst distance, in the 10 s after any turn, of a shown position from the reference path after the turn, metres.
#[must_use]
pub fn overshoot_m(reference: &[TruthPoint], shown: &[Shown]) -> f64 {
    let mut worst = 0.0_f64;
    for (t, p, _) in turns(reference) {
        let out_end = interp(reference, t + 20_000);
        for s in shown.iter().filter(|s| (t..=t + 10_000).contains(&s.t_ms)) {
            worst = worst.max(distance_to_segment_m(s.p, p, out_end));
        }
    }
    worst
}

/// How many fixes got each verdict, by name.
#[must_use]
pub fn verdict_counts(shown: &[Shown]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for s in shown {
        *m.entry(format!("{:?}", s.verdict)).or_insert(0) += 1;
    }
    m
}

/// Virtual targets of radius `r` every `every_m` metres along the reference.
#[must_use]
pub fn virtual_targets(reference: &[TruthPoint], every_m: f64, r: f64) -> Vec<(Point, f64)> {
    let mut out = Vec::new();
    let mut since = 0.0;
    for w in reference.windows(2) {
        since += distance_m(w[0].p, w[1].p);
        if since >= every_m {
            out.push((w[1].p, r));
            since = 0.0;
        }
    }
    out
}

/// The scorecard of one shown track.
#[derive(Debug, Clone, PartialEq)]
pub struct Scorecard {
    /// Which filter.
    pub name: String,
    /// Fixes fed.
    pub fixes: usize,
    /// RMS and p95 position error, metres.
    pub error_m: (f64, f64),
    /// False jumps.
    pub false_jumps: usize,
    /// Standing-still jitter.
    pub jitter: Option<Jitter>,
    /// Arrival lags at targets the shown track reached, seconds.
    pub arrival_lag_s: Vec<f64>,
    /// Targets the reference reached and the shown track never did.
    pub missed: usize,
    /// Turn lags, seconds.
    pub turn_lag_s: Vec<f64>,
    /// Verdict counts.
    pub verdicts: BTreeMap<String, usize>,
    /// Final odometer, metres.
    pub odometer_m: f64,
}

/// Score `shown` against `reference` with `targets` (quests, plus virtual targets if the caller adds them).
#[must_use]
pub fn score(name: &str, reference: &[TruthPoint], shown: &[Shown], targets: &[(Point, f64)]) -> Scorecard {
    let lags: Vec<Option<f64>> = targets.iter().map(|(p, r)| arrival_lag_s(reference, shown, *p, *r)).collect();
    Scorecard {
        name: name.to_string(),
        fixes: shown.len(),
        error_m: position_error(reference, shown),
        false_jumps: false_jumps(reference, shown),
        jitter: stationary_jitter(reference, shown),
        arrival_lag_s: lags.iter().flatten().copied().filter(|l| l.is_finite()).collect(),
        missed: lags.iter().flatten().filter(|l| l.is_infinite()).count(),
        turn_lag_s: turn_lags_s(reference, shown).into_iter().filter(|l| l.is_finite()).collect(),
        verdicts: verdict_counts(shown),
        odometer_m: shown.last().map_or(0.0, |s| s.odometer_m),
    }
}

fn rows(s: &Scorecard) -> Vec<(String, String)> {
    let j = s.jitter;
    vec![
        ("fixes".into(), s.fixes.to_string()),
        ("error rms / p95 m".into(), format!("{:.1} / {:.1}", s.error_m.0, s.error_m.1)),
        ("false jumps".into(), s.false_jumps.to_string()),
        ("jitter rms m".into(), j.map_or("-".into(), |j| format!("{:.2}", j.rms_m))),
        ("standing path m/min".into(), j.map_or("-".into(), |j| format!("{:.1}", j.path_m_per_min))),
        ("held share".into(), j.map_or("-".into(), |j| format!("{:.2}", j.held_share))),
        ("arrival lag p50 / p90 s".into(), format!("{:.1} / {:.1}", percentile(&s.arrival_lag_s, 0.5), percentile(&s.arrival_lag_s, 0.9))),
        ("missed arrivals".into(), s.missed.to_string()),
        ("turn lag p50 s".into(), format!("{:.1}", percentile(&s.turn_lag_s, 0.5))),
        ("verdicts".into(), format!("{:?}", s.verdicts)),
        ("odometer m".into(), format!("{:.0}", s.odometer_m)),
    ]
}

/// Scorecards as aligned columns, one per card, in order.
#[must_use]
pub fn columns(cards: &[&Scorecard]) -> String {
    let mut out = format!("{:<26}", "metric");
    cards.iter().for_each(|c| out.push_str(&format!("{:<36}", c.name)));
    out.push('\n');
    let all: Vec<Vec<(String, String)>> = cards.iter().map(|c| rows(c)).collect();
    for (i, (k, _)) in all.first().map(Vec::as_slice).unwrap_or_default().iter().enumerate() {
        let _ = write!(out, "{k:<26}");
        all.iter().for_each(|r| out.push_str(&format!("{:<36}", r[i].1)));
        out.push('\n');
    }
    out
}
```

Add to `core/src/num.rs` (used by `percentile`; same style as the other helpers):

```rust
/// `x` (finite, non-negative, already floored) as an index.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // callers pass floored, non-negative ranks
pub(crate) fn floor_usize(x: f64) -> usize {
    x.floor().max(0.0) as usize
}
```

- [ ] **Step 4: Write `core/src/loc/bench/legacy.rs`**

```rust
//! The rules the game used before the location program, kept only for `--compare baseline`: accuracy 35 m, an implied speed of 100 km/h
//! is a jump, the third outlier in a row is believed, and the odometer moves in steps of at least 6 m (or the accuracy).

use crate::geo::{distance_m, Point};
use crate::loc::bench::Shown;
use crate::loc::{RawFix, Verdict};
use crate::num::i64_to_f64;

const MAX_ACCURACY_M: f64 = 35.0;
const MAX_PLAUSIBLE_KMH: f64 = 100.0;
const MAX_OUTLIER_STREAK: u32 = 3;
const ODOMETER_MIN_STEP_M: f64 = 6.0;

/// Speed between two fixes in km/h, ignoring what both error radii could explain; `None` for gaps under 1 s or over 2 min.
#[must_use]
pub fn legacy_implied_speed_kmh(prev: &RawFix, cur: &RawFix) -> Option<f64> {
    let dt = i64_to_f64(cur.t_ms - prev.t_ms) / 1000.0;
    if !(1.0..=120.0).contains(&dt) {
        return None;
    }
    let effective = (distance_m(prev.point(), cur.point()) - prev.accuracy_m - cur.accuracy_m).max(0.0);
    Some(effective / dt * 3.6)
}

/// Today's rules as a filter. The map showed every raw fix, so the shown position is the raw one; `accepted` is what quests got.
#[derive(Debug, Clone, Default)]
pub struct LegacyRules {
    last: Option<RawFix>,
    streak: u32,
    anchor: Option<Point>,
    odometer_m: f64,
}

impl LegacyRules {
    /// A fresh rule set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one fix.
    pub fn feed(&mut self, f: &RawFix) -> Shown {
        let shown = |accepted, verdict, odometer_m| Shown { t_ms: f.t_ms, p: f.point(), uncertainty_m: f.accuracy_m, accepted, verdict, course_deg: None, odometer_m };
        if f.accuracy_m > MAX_ACCURACY_M {
            return shown(false, Verdict::Blurry, self.odometer_m);
        }
        let jump = self.last.as_ref().and_then(|l| legacy_implied_speed_kmh(l, f)).is_some_and(|k| k > MAX_PLAUSIBLE_KMH);
        if jump && self.streak < MAX_OUTLIER_STREAK - 1 {
            self.streak += 1;
            return shown(false, Verdict::Gated, self.odometer_m);
        }
        self.streak = 0;
        if self.last.is_some_and(|l| f.t_ms - l.t_ms > 300_000) {
            self.anchor = None;
        }
        let p = f.point();
        match self.anchor {
            Some(a) if distance_m(a, p) >= f.accuracy_m.max(ODOMETER_MIN_STEP_M) => {
                self.odometer_m += distance_m(a, p);
                self.anchor = Some(p);
            }
            Some(_) => {}
            None => self.anchor = Some(p),
        }
        self.last = Some(*f);
        shown(true, Verdict::Used, self.odometer_m)
    }
}
```

Replace `core/src/loc/bench.rs` with:

```rust
//! The bench: synthetic walks with known truth, metrics on a shown track, today's rules as a baseline, and readers for recorded walks.
//! Shared by `core/tests/loc_scenarios.rs` and `core/examples/replay.rs`.

mod legacy;
mod metrics;
mod synth;

pub use legacy::{legacy_implied_speed_kmh, LegacyRules};
pub use metrics::{
    arrival_lag_s, columns, completes, false_jumps, interp, overshoot_m, percentile, position_error, score, stationary_jitter, turn_lags_s,
    verdict_counts, virtual_targets, Jitter, Scorecard, Shown,
};
pub use synth::{cadence_for, gauss, truth_at, HeadingSim, Leg, Run, Scenario, Spike, TruthPoint};
```

- [ ] **Step 5: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core bench && cargo test -p apgo-core --test loc_scenarios`
Expected: PASS.
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add core/src/loc core/src/num.rs core/tests/loc_scenarios.rs
git commit -m "feat: score tracks on the location bench" -m "Metrics from the spec scorecard (error, false jumps, jitter, arrival
and turn lag, verdicts) and today's rules as LegacyRules, so every
later layer is measured against the baseline." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 4: Recording readers, the replay example and the baseline scorecards

**Files:**

- Create: `core/src/loc/bench/record.rs`, `core/src/loc/bench/reference.rs`, `core/examples/replay.rs`
- Modify: `core/src/loc/bench.rs` (modules and re-exports), `core/src/game.rs` (new `Game::reach_targets`)

**Interfaces:**

- Consumes: `RawFix`, `Provider`, `HeadingIn`, `CompassAccuracy` (Task 2); `LegacyRules`, `score`, `columns`, `Shown`, `TruthPoint` (Task 3).
- Produces:
  - `bench::Recording { fixes: Vec<RawFix>, steps: Vec<(i64, i64)>, headings: Vec<HeadingIn>, skipped: usize }`,
    `bench::parse_raw_lines(text: &str, rec: &mut Recording)`, `bench::read_raw_dir(dir: &Path) -> Result<Recording, String>`,
    `bench::read_journal(path: &Path, from_ms: i64, to_ms: i64) -> Result<Vec<RawFix>, String>`.
  - `bench::good_fix_reference(fixes: &[RawFix], max_acc_m: f64) -> Vec<TruthPoint>` (provisional reference until Task 8's RTS).
  - `Game::reach_targets(&self) -> Vec<(i64, Point, f64)>` (location id, anchor, reach radius) for pickup scoring.
  - CLI: `cargo run --release --example replay -- <pulled-dir | raw.jsonl | journal.db> [--mode walk|run|bike|drive] [--from-ms N] [--to-ms N]
    [--game games/<id>.json] [--geojson out.geojson] [--csv out.csv]` (plan choice: `--compare baseline` and `--params` arrive in Task 8
    and `--atlas` in Task 19, once there is something to compare or apply them to; until then the legacy column is the only one).
- Plan choice: journal points have no provider; they are read as `Provider::Fused` (both recorded builds used fused only).

- [ ] **Step 1: Write the failing tests**

At the bottom of `core/src/loc/bench/record.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::{Journal, TrackPoint};

    const LINES: &str = concat!(
        r#"{"t":5,"lvl":"I","tag":"rawfix","msg":"","tf":1000,"ert":7,"lat":40.0,"lon":-111.0,"acc":4.5,"spd":1.2,"spd_acc":0.5,"brg":90.0,"brg_acc":10.0,"alt":null,"valt":null,"prov":"gps","mock":false}"#,
        "\n",
        r#"{"t":6,"lvl":"I","tag":"rawsteps","msg":"","total":1234,"te":1500}"#,
        "\n",
        r#"{"t":7,"lvl":"I","tag":"rawhead","msg":"","az":270.0,"acc":"high"}"#,
        "\n",
        "not json\n",
        r#"{"t":8,"lvl":"I","tag":"rawstate","msg":"","presence":"InZone","counting":true,"zone":"inside","app_visible":true}"#,
        "\n"
    );

    #[test]
    fn raw_lines_become_fixes_steps_and_headings_and_junk_is_counted() {
        let mut r = Recording::default();
        parse_raw_lines(LINES, &mut r);
        assert_eq!(r.fixes.len(), 1);
        let f = r.fixes[0];
        assert_eq!((f.t_ms, f.provider, f.speed_mps, f.bearing_acc_deg, f.altitude_m), (1000, Provider::Gps, Some(1.2), Some(10.0), None));
        assert_eq!(r.steps, vec![(1500, 1234)]);
        assert_eq!((r.headings[0].azimuth_deg, r.headings[0].accuracy), (270.0, CompassAccuracy::High));
        assert_eq!(r.skipped, 1, "the line that is not JSON");
    }

    #[test]
    fn a_fix_without_accuracy_reads_as_unusably_coarse() {
        let mut r = Recording::default();
        parse_raw_lines(r#"{"t":5,"tag":"rawfix","tf":1,"lat":1.0,"lon":2.0,"acc":null,"prov":"fused","mock":false}"#, &mut r);
        assert!(r.fixes[0].accuracy_m > 100.0);
    }

    #[test]
    fn journal_points_in_the_window_are_read_without_simulated_ones_and_the_file_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("apgo-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.db");
        {
            let j = Journal::open(&path).unwrap();
            for (t, sim) in [(1000, false), (2000, true), (3000, false), (9000, false)] {
                j.add_point("g", &TrackPoint { t_ms: t, lat: 40.0, lon: -111.0, accuracy_m: 5.0, simulated: sim }).unwrap();
            }
        }
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        let fixes = read_journal(&path, 0, 5000).unwrap();
        assert_eq!(fixes.iter().map(|f| f.t_ms).collect::<Vec<_>>(), [1000, 3000]);
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before, "opened read-only");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
```

At the bottom of `core/src/loc/bench/reference.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_provisional_reference_keeps_only_good_fixes_and_gives_them_a_speed() {
        let f = |t: i64, lat: f64, acc: f64| RawFix::at(lat, -111.0, t, acc);
        let r = good_fix_reference(&[f(0, 40.0, 5.0), f(1000, 40.000_01, 30.0), f(2000, 40.000_02, 5.0)], 15.0);
        assert_eq!(r.len(), 2);
        assert!((r[1].speed_mps - 1.11).abs() < 0.05, "2.2 m in 2 s: {}", r[1].speed_mps);
    }
}
```

In `core/src/game.rs` tests:

```rust
    #[test]
    fn reach_targets_list_every_quest_with_a_point_to_reach() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let t = g.reach_targets();
        assert_eq!(t.len(), g.assignments.len(), "reach-only game: every quest has a point");
        assert!(t.iter().all(|(_, _, r)| *r > 0.0));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core record reference reach_targets`
Expected: FAIL to compile: `cannot find struct 'Recording'`, `cannot find function 'good_fix_reference'`, `no method named 'reach_targets'`.

- [ ] **Step 3: Write `core/src/loc/bench/record.rs`**

```rust
//! Readers for recorded walks: the debug raw track (`diag/raw/raw-NNNN.jsonl`) and, for older outings, the journal's points.

use std::path::Path;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::loc::{CompassAccuracy, HeadingIn, Provider, RawFix};

/// What a recording holds, in time order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recording {
    /// Raw fixes.
    pub fixes: Vec<RawFix>,
    /// Step counter readings `(event ms, cumulative total)`.
    pub steps: Vec<(i64, i64)>,
    /// Compass readings.
    pub headings: Vec<HeadingIn>,
    /// Lines that could not be read.
    pub skipped: usize,
}

fn num(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

fn int(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(Value::as_i64)
}

/// Add every `rawfix`, `rawsteps` and `rawhead` line of `text` to `rec` (other tags are ignored, unreadable lines counted).
pub fn parse_raw_lines(text: &str, rec: &mut Recording) {
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            rec.skipped += 1;
            continue;
        };
        match v.get("tag").and_then(Value::as_str) {
            Some("rawfix") => {
                let (Some(t_ms), Some(lat), Some(lon)) = (int(&v, "tf"), num(&v, "lat"), num(&v, "lon")) else {
                    rec.skipped += 1;
                    continue;
                };
                rec.fixes.push(RawFix {
                    t_ms,
                    lat,
                    lon,
                    accuracy_m: num(&v, "acc").unwrap_or(1000.0),
                    speed_mps: num(&v, "spd"),
                    speed_acc_mps: num(&v, "spd_acc"),
                    bearing_deg: num(&v, "brg"),
                    bearing_acc_deg: num(&v, "brg_acc"),
                    altitude_m: num(&v, "alt"),
                    vertical_acc_m: num(&v, "valt"),
                    provider: Provider::parse(v.get("prov").and_then(Value::as_str).unwrap_or("other")),
                    mock: v.get("mock").and_then(Value::as_bool).unwrap_or(false),
                });
            }
            Some("rawsteps") => {
                if let (Some(te), Some(total)) = (int(&v, "te"), int(&v, "total")) {
                    rec.steps.push((te, total));
                }
            }
            Some("rawhead") => {
                if let (Some(t_ms), Some(az)) = (int(&v, "t"), num(&v, "az")) {
                    rec.headings.push(HeadingIn {
                        t_ms,
                        azimuth_deg: az,
                        accuracy: CompassAccuracy::parse(v.get("acc").and_then(Value::as_str).unwrap_or("")),
                        pitch_deg: num(&v, "pitch").unwrap_or(0.0),
                        roll_deg: num(&v, "roll").unwrap_or(0.0),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Every `raw-*.jsonl` file of `dir` (a pulled `diag/raw/`), oldest file first, then everything sorted by time.
///
/// # Errors
/// Returns a message if the directory cannot be listed.
pub fn read_raw_dir(dir: &Path) -> Result<Recording, String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("raw-") && n.ends_with(".jsonl")))
        .collect();
    files.sort();
    let mut rec = Recording::default();
    for f in files {
        parse_raw_lines(&std::fs::read_to_string(&f).unwrap_or_default(), &mut rec);
    }
    rec.fixes.sort_by_key(|f| f.t_ms);
    rec.steps.sort_unstable();
    rec.headings.sort_by_key(|h| h.t_ms);
    Ok(rec)
}

/// The journal's real (not simulated) points in `from_ms..=to_ms`, as fused fixes with position, time and accuracy only. The file is opened
/// read-only, so a pulled journal is never changed.
///
/// # Errors
/// Returns a message if the file cannot be opened or read.
pub fn read_journal(path: &Path, from_ms: i64, to_ms: i64) -> Result<Vec<RawFix>, String> {
    let c = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(|e| e.to_string())?;
    let mut st = c
        .prepare("SELECT t_ms, lat, lon, accuracy_m FROM points WHERE simulated = 0 AND t_ms BETWEEN ?1 AND ?2 ORDER BY t_ms, id")
        .map_err(|e| e.to_string())?;
    let rows = st.query_map((from_ms, to_ms), |r| Ok(RawFix::at(r.get(1)?, r.get(2)?, r.get(0)?, r.get(3)?))).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Write `core/src/loc/bench/reference.rs`**

```rust
//! The reference track of a real walk (no ground truth exists). Provisional: the good fixes themselves; Task 8 replaces the default with
//! an RTS-smoothed track.

use crate::geo::{bearing_deg, distance_m};
use crate::loc::bench::TruthPoint;
use crate::loc::RawFix;
use crate::num::i64_to_f64;

/// The fixes with accuracy at most `max_acc_m`, with speed and course from the previous good fix.
#[must_use]
pub fn good_fix_reference(fixes: &[RawFix], max_acc_m: f64) -> Vec<TruthPoint> {
    let good: Vec<&RawFix> = fixes.iter().filter(|f| f.accuracy_m <= max_acc_m).collect();
    let mut out: Vec<TruthPoint> = Vec::with_capacity(good.len());
    for (i, f) in good.iter().enumerate() {
        let (speed_mps, course_deg) = match i.checked_sub(1).map(|j| good[j]) {
            Some(p) if f.t_ms > p.t_ms => (distance_m(p.point(), f.point()) / (i64_to_f64(f.t_ms - p.t_ms) / 1000.0), bearing_deg(p.point(), f.point())),
            _ => (0.0, 0.0),
        };
        out.push(TruthPoint { t_ms: f.t_ms, p: f.point(), speed_mps, course_deg });
    }
    out
}
```

Update `core/src/loc/bench.rs` modules: add `mod record; mod reference;` and
`pub use record::{parse_raw_lines, read_journal, read_raw_dir, Recording}; pub use reference::good_fix_reference;`.

- [ ] **Step 5: Add `Game::reach_targets`** (in `impl Game`, next to `explain_near`)

```rust
    /// Every open-or-done quest with a point to reach: (location id, the point, how close counts). For the location bench.
    #[must_use]
    pub fn reach_targets(&self) -> Vec<(i64, Point, f64)> {
        self.assignments.iter().filter_map(|a| Some((a.location_id, anchor(&a.target)?, reach_radius(&a.target)?))).collect()
    }
```

- [ ] **Step 6: Write `core/examples/replay.rs`**

```rust
//! Replay a recorded walk through the location filters and print the scorecard.
//! Usage: `cargo run --release --example replay -- <pulled-dir | raw.jsonl | journal.db> [--mode walk|run|bike|drive] [--from-ms N]
//! [--to-ms N] [--game games/<id>.json] [--geojson out.geojson] [--csv out.csv]`
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)] // CLI example: prints and fails fast

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use apgo_core::catalog::Mode;
use apgo_core::game::Game;
use apgo_core::geo::Point;
use apgo_core::loc::bench::{columns, good_fix_reference, read_journal, read_raw_dir, score, LegacyRules, Recording, Shown};

struct Args {
    input: PathBuf,
    mode: Mode,
    from_ms: i64,
    to_ms: i64,
    game: Option<PathBuf>,
    geojson: Option<PathBuf>,
    csv: Option<PathBuf>,
}

fn args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args { input: PathBuf::new(), mode: Mode::Walk, from_ms: 0, to_ms: i64::MAX, game: None, geojson: None, csv: None };
    while let Some(x) = it.next() {
        match x.as_str() {
            "--mode" => a.mode = Mode::parse(&it.next().expect("--mode needs a value")).expect("walk, run, bike or drive"),
            "--from-ms" => a.from_ms = it.next().expect("--from-ms N").parse().expect("a number"),
            "--to-ms" => a.to_ms = it.next().expect("--to-ms N").parse().expect("a number"),
            "--game" => a.game = it.next().map(PathBuf::from),
            "--geojson" => a.geojson = it.next().map(PathBuf::from),
            "--csv" => a.csv = it.next().map(PathBuf::from),
            _ => a.input = PathBuf::from(x),
        }
    }
    assert!(!a.input.as_os_str().is_empty(), "usage: replay <pulled-dir | raw.jsonl | journal.db> [options]");
    a
}

fn load(a: &Args) -> Recording {
    let p = &a.input;
    let raw_dir = p.join("diag").join("raw");
    let mut rec = if raw_dir.is_dir() {
        read_raw_dir(&raw_dir).unwrap()
    } else if p.extension().is_some_and(|e| e == "jsonl") {
        let mut r = Recording::default();
        apgo_core::loc::bench::parse_raw_lines(&std::fs::read_to_string(p).unwrap(), &mut r);
        r
    } else {
        let db = if p.is_dir() { p.join("files").join("journal.db") } else { p.clone() };
        Recording { fixes: read_journal(&db, a.from_ms, a.to_ms).unwrap(), ..Recording::default() }
    };
    rec.fixes.retain(|f| (a.from_ms..=a.to_ms).contains(&f.t_ms));
    rec
}

fn targets(game: Option<&Path>) -> Vec<(Point, f64)> {
    let Some(path) = game else { return vec![] };
    let (dir, id) = (path.parent().and_then(Path::parent).expect("files/games/<id>.json"), path.file_stem().unwrap().to_string_lossy());
    Game::load(dir, &id).expect("a game save").reach_targets().into_iter().map(|(_, p, r)| (p, r)).collect()
}

fn geojson(rec: &Recording, cols: &[(&str, &[Shown])]) -> String {
    let line = |pts: Vec<Point>| serde_json::json!({"type": "LineString", "coordinates": pts.iter().map(|p| [p.lon, p.lat]).collect::<Vec<_>>()});
    let mut features = vec![serde_json::json!({"type": "Feature", "properties": {"name": "raw"}, "geometry": line(rec.fixes.iter().map(|f| f.point()).collect())})];
    for (name, shown) in cols {
        features.push(serde_json::json!({"type": "Feature", "properties": {"name": name}, "geometry": line(shown.iter().map(|s| s.p).collect())}));
        for s in shown.iter().filter(|s| !s.accepted) {
            features.push(serde_json::json!({"type": "Feature", "properties": {"name": format!("{name} {:?}", s.verdict)}, "geometry": {"type": "Point", "coordinates": [s.p.lon, s.p.lat]}}));
        }
    }
    serde_json::json!({"type": "FeatureCollection", "features": features}).to_string()
}

fn csv(cols: &[(&str, &[Shown])]) -> String {
    let mut out = String::from("filter,t_ms,lat,lon,uncertainty_m,accepted,verdict,odometer_m\n");
    for (name, shown) in cols {
        for s in *shown {
            let _ = writeln!(out, "{name},{},{:.7},{:.7},{:.1},{},{:?},{:.1}", s.t_ms, s.p.lat, s.p.lon, s.uncertainty_m, s.accepted, s.verdict, s.odometer_m);
        }
    }
    out
}

fn main() {
    let a = args();
    let rec = load(&a);
    println!("{} fixes, {} step readings, {} headings ({} lines skipped), mode {}", rec.fixes.len(), rec.steps.len(), rec.headings.len(), rec.skipped, a.mode.name());
    let reference = good_fix_reference(&rec.fixes, 15.0);
    let mut targets = targets(a.game.as_deref());
    targets.extend(apgo_core::loc::bench::virtual_targets(&reference, 200.0, 25.0));
    let mut legacy = LegacyRules::new();
    let legacy_shown: Vec<Shown> = rec.fixes.iter().map(|f| legacy.feed(f)).collect();
    let legacy_card = score("legacy", &reference, &legacy_shown, &targets);
    // Task 8 adds the Locator column here (and --compare baseline to print both).
    println!("{}", columns(&[&legacy_card]));
    let cols: Vec<(&str, &[Shown])> = vec![("legacy", &legacy_shown)];
    if let Some(p) = &a.geojson {
        std::fs::write(p, geojson(&rec, &cols)).unwrap();
    }
    if let Some(p) = &a.csv {
        std::fs::write(p, csv(&cols)).unwrap();
    }
}
```

- [ ] **Step 7: Run the tests, the example and the gate**

Run: `cd core && cargo test -p apgo-core record reference reach_targets`
Expected: PASS.
Run: `cd core && cargo build --release --example replay`
Expected: builds without warnings.
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 8: Record the baseline scorecards locally (never committed)**

```bash
S=/tmp/claude-1000/-home-rasbandit-Documents-code-projects-Archipela-Go/2ebb9a6e-09f0-458e-9145-e238b2943d4d/scratchpad
M=/home/rasbandit/Documents/code-projects/Archipela-Go/diag/20261007-190430
D=$S/diag-2001
cd core
cargo run --release --example replay -- "$M/files/journal.db" --mode walk \
  --game "$M/files/games/$(ls "$M/files/games" | head -1)" --geojson "$S/walk-1007-legacy.geojson" | tee "$S/scorecard-1007-baseline.txt"
cargo run --release --example replay -- "$D/files/journal.db" --mode walk --from-ms 1791509000000 \
  --game "$D/files/games/0fe19fe8-d3da-42b3-9a9c-0a91955d2ae4.json" | tee "$S/scorecard-1008-baseline.txt"
```

Expected: the 2026-10-07 walk prints about 384 fixes (the main checkout's journal) and a legacy column; the 2026-10-08 walk prints 218
fixes (the journal only holds fixes the old rules accepted: the 88 rejected ones are not in it, a known limit of the journal fallback).
Keep both files in the scratchpad: they go into the PR description in Task 23. `git status` must show nothing under `diag/` or the
scratchpad.

- [ ] **Step 9: Commit**

```bash
git add core/src/loc core/src/game.rs core/examples/replay.rs
git commit -m "feat: replay recorded walks on the bench" -m "Reads the debug raw track or the journal, scores the legacy rules and
writes GeoJSON or CSV for a visual check. Real walks stay local." -m "Closes #83
Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

(The commit footer here carries `Closes #83` above `Refs #89`.)

---

## Task 5: `mat.rs`, `LocParams` and the IMM models and cycle

**Files:**

- Create: `core/src/loc/mat.rs`, `core/src/loc/params.rs`, `core/src/loc/imm.rs`
- Modify: `core/src/loc/mod.rs` (add `pub mod imm; pub mod mat; pub mod params;` and `pub use params::LocParams;`)

**Interfaces:**

- Consumes: `Mode` (`crate::catalog`), `crate::num::{count_f64, i64_to_f64}`.
- Produces:
  - `mat::Mat<const R, const C>` = `[[f64; C]; R]`; `zeros`, `identity`, `mul`, `mul_vec`, `transpose`, `add`, `sub`, `scale`, `symmetrize`,
    `outer`, `dot`, `inverse<N>(&Mat<N, N>) -> Option<(Mat<N, N>, f64)>` (inverse and determinant), `max_eig2(&Mat<2, 2>) -> f64`,
    `block_diag(&Mat<2, 2>, &Mat<2, 2>) -> Mat<4, 4>`, `mahalanobis2<M>(x, p, z, h, r) -> Option<f64>`,
    `kalman_update<M>(x, p, z, h, r) -> Option<Update>`, `Update { x: [f64; 4], p: Mat<4, 4>, d2: f64, log_likelihood: f64 }`.
  - `params::LocParams` (every number below, `Default`, `Serialize`/`Deserialize` with `#[serde(default)]`).
  - `imm::{S, W, F}` (model indexes 0, 1, 2), `imm::Gaussian { x: [f64; 4], p: Mat<4, 4> }`, `imm::Meas { pos: [f64; 2], r_pos: Mat<2, 2>, vel: Option<([f64; 2], Mat<2, 2>)> }`,
    `imm::transition(&LocParams, Mode, dt_s) -> [[f64; 3]; 3]`, `imm::sigma_a(&LocParams, Mode, model) -> f64`, `imm::cv_noise(sigma_a, dt) -> Mat<4, 4>`,
    `imm::predict(&Gaussian, model, dt, sigma_a, &LocParams) -> Gaussian`, `imm::mix(&[Gaussian; 3], &[f64; 3], &[[f64; 3]; 3]) -> ([Gaussian; 3], [f64; 3])`,
    `imm::combine(&[Gaussian; 3], &[f64; 3]) -> Gaussian`, `imm::Imm { models, mu, t_ms }` with `Imm::new(z, sigma, t_ms, Mode, &LocParams)`,
    `predict_to(&self, t_ms, Mode, &LocParams) -> ([Gaussian; 3], [f64; 3])`,
    `update(&mut self, preds, c, &Meas, r_scale, weights_by_model: [f64; 3], t_ms, mu_floor) -> bool`, `coast(&mut self, preds, c, t_ms, mu_floor)`,
    `output(&self) -> Gaussian`, `map_positions(&mut self, &dyn Fn([f64; 2]) -> [f64; 2])`, and `imm::pos_block(&Mat<4, 4>) -> Mat<2, 2>`, `imm::vel_block(&Mat<4, 4>) -> Mat<2, 2>`.
- Plan choices (spec silent): the W model uses `sigma_a` 1.0 in Run zones and 0.5 otherwise; the F model uses 3.0 in Drive zones and 1.5
  otherwise. `Pi(dt)`: when a row's off-diagonal sum `rate x min(dt, 10)` exceeds `max_offdiag_share` (0.9) it is scaled down to 0.9 (the
  spec's Bike/Drive W row reaches 2.0 at 10 s, which would make the diagonal negative; listed under spec gaps).

- [ ] **Step 1: Write the failing tests**

At the bottom of `core/src/loc/mat.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn close<const R: usize, const C: usize>(a: &Mat<R, C>, b: &Mat<R, C>) -> bool {
        a.iter().zip(b).all(|(x, y)| x.iter().zip(y).all(|(u, v)| (u - v).abs() < 1e-9))
    }

    #[test]
    fn inverse_times_matrix_is_identity_and_the_determinant_is_right() {
        let a: Mat<4, 4> = [[4.0, 1.0, 0.0, 0.5], [1.0, 3.0, 0.2, 0.0], [0.0, 0.2, 2.0, 0.1], [0.5, 0.0, 0.1, 1.0]];
        let (inv, det) = inverse(&a).unwrap();
        assert!(close(&mul(&a, &inv), &identity::<4>()));
        let (_, d2) = inverse(&[[2.0, 1.0], [1.0, 3.0]]).unwrap();
        assert!((d2 - 5.0).abs() < 1e-12);
        assert!(det > 0.0);
        assert!(inverse(&[[1.0, 2.0], [2.0, 4.0]]).is_none(), "singular");
    }

    #[test]
    fn products_transposes_and_eigenvalues() {
        let a: Mat<2, 3> = [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        assert_eq!(transpose(&a), [[1.0, 4.0], [2.0, 5.0], [3.0, 6.0]]);
        assert_eq!(mul(&a, &transpose(&a)), [[14.0, 32.0], [32.0, 77.0]]);
        assert_eq!(mul_vec(&a, &[1.0, 0.0, 1.0]), [4.0, 10.0]);
        assert!((max_eig2(&[[4.0, 0.0], [0.0, 9.0]]) - 9.0).abs() < 1e-12);
        assert!((max_eig2(&[[2.0, 1.0], [1.0, 2.0]]) - 3.0).abs() < 1e-12);
    }

    #[test]
    fn a_one_axis_update_matches_the_scalar_kalman_formula() {
        // prior x = 0 var 4, measurement 2 var 4: posterior 1, var 2; Joseph form keeps the covariance symmetric.
        let mut p = identity::<4>();
        p[0][0] = 4.0;
        let h: Mat<1, 4> = [[1.0, 0.0, 0.0, 0.0]];
        let u = kalman_update(&[0.0; 4], &p, &[2.0], &h, &[[4.0]]).unwrap();
        assert!((u.x[0] - 1.0).abs() < 1e-12 && (u.p[0][0] - 2.0).abs() < 1e-12);
        assert!((u.d2 - 0.5).abs() < 1e-12, "y^2 / S = 4 / 8");
        assert_eq!(u.p, symmetrize(&u.p));
        assert!((mahalanobis2(&[0.0; 4], &p, &[2.0], &h, &[[4.0]]).unwrap() - 0.5).abs() < 1e-12);
    }
}
```

At the bottom of `core/src/loc/imm.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> LocParams {
        LocParams::default()
    }

    #[test]
    fn pi_rows_are_probabilities_for_any_gap_and_both_mode_groups() {
        for mode in Mode::ALL {
            for dt in [0.0, 0.5, 1.0, 5.0, 10.0, 60.0] {
                for row in transition(&params(), mode, dt) {
                    assert!((row.iter().sum::<f64>() - 1.0).abs() < 1e-12, "{mode:?} {dt}: {row:?}");
                    assert!(row.iter().all(|v| (0.0..=1.0).contains(v)), "{mode:?} {dt}: {row:?}");
                }
            }
        }
    }

    #[test]
    fn one_second_of_pi_is_the_spec_table() {
        let p = transition(&params(), Mode::Walk, 1.0);
        assert!((p[S][S] - 0.95).abs() < 1e-12 && (p[W][F] - 0.01).abs() < 1e-12 && (p[F][W] - 0.08).abs() < 1e-12);
        let b = transition(&params(), Mode::Bike, 1.0);
        assert!((b[F][F] - 0.95).abs() < 1e-12 && (b[W][F] - 0.15).abs() < 1e-12);
    }

    #[test]
    fn the_stationary_model_keeps_the_position_and_drops_the_velocity() {
        let g = Gaussian { x: [3.0, 4.0, 1.0, 1.0], p: scale(&identity::<4>(), 2.0) };
        let out = predict(&g, S, 10.0, 0.0, &params());
        assert_eq!(&out.x, &[3.0, 4.0, 0.0, 0.0]);
        assert!((out.p[0][0] - (2.0 + 0.0025 * 10.0)).abs() < 1e-12);
        assert!((out.p[2][2] - 0.01).abs() < 1e-12);
    }

    #[test]
    fn constant_velocity_moves_by_v_dt_and_adds_white_acceleration_noise() {
        let g = Gaussian { x: [0.0, 0.0, 1.5, -0.5], p: zeros() };
        let out = predict(&g, W, 2.0, 0.5, &params());
        assert!((out.x[0] - 3.0).abs() < 1e-12 && (out.x[1] + 1.0).abs() < 1e-12);
        let q = cv_noise(0.5, 2.0);
        assert!((q[0][0] - 0.25 * 16.0 / 4.0).abs() < 1e-12 && (q[0][2] - 0.25 * 8.0 / 2.0).abs() < 1e-12 && (q[2][2] - 0.25 * 4.0).abs() < 1e-12);
        assert_eq!(out.p, q);
    }

    #[test]
    fn sigma_a_follows_the_zone_mode() {
        let p = params();
        assert_eq!((sigma_a(&p, Mode::Walk, W), sigma_a(&p, Mode::Run, W)), (0.5, 1.0));
        assert_eq!((sigma_a(&p, Mode::Bike, F), sigma_a(&p, Mode::Drive, F)), (1.5, 3.0));
    }

    #[test]
    fn mixing_without_switching_changes_nothing_and_combining_adds_the_spread() {
        let a = Gaussian { x: [0.0, 0.0, 0.0, 0.0], p: identity::<4>() };
        let b = Gaussian { x: [2.0, 0.0, 0.0, 0.0], p: identity::<4>() };
        let (mixed, c) = mix(&[a, b, a], &[0.5, 0.5, 0.0], &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert_eq!(mixed[0].x, a.x);
        assert_eq!(mixed[1].x, b.x);
        assert_eq!(c, [0.5, 0.5, 0.0]);
        let out = combine(&[a, b, a], &[0.5, 0.5, 0.0]);
        assert!((out.x[0] - 1.0).abs() < 1e-12 && (out.p[0][0] - 2.0).abs() < 1e-12, "1 + spread 1");
    }

    fn feed(imm: &mut Imm, mode: Mode, pts: impl Iterator<Item = (i64, [f64; 2])>) {
        let p = params();
        for (t, z) in pts {
            let (preds, c) = imm.predict_to(t, mode, &p);
            let m = Meas { pos: z, r_pos: scale(&identity::<2>(), 9.0), vel: None };
            assert!(imm.update(preds, c, &m, 1.0, [1.0; 3], t, p.mu_floor));
        }
    }

    #[test]
    fn walking_fixes_raise_the_walking_model_and_standing_ones_the_stationary() {
        let p = params();
        let mut walk = Imm::new([0.0, 0.0], 3.0, 0, Mode::Walk, &p);
        feed(&mut walk, Mode::Walk, (1..=60).map(|i| (i * 1000, [1.4 * i64_to_f64(i), 0.0])));
        assert!(walk.mu[W] > 0.6, "{:?}", walk.mu);
        assert!((walk.output().x[2] - 1.4).abs() < 0.3, "velocity learned: {:?}", walk.output().x);
        let mut stand = Imm::new([0.0, 0.0], 3.0, 0, Mode::Walk, &p);
        feed(&mut stand, Mode::Walk, (1..=60).map(|i| (i * 1000, [0.0, 0.0])));
        assert!(stand.mu[S] > 0.8, "{:?}", stand.mu);
        assert!((stand.mu.iter().sum::<f64>() - 1.0).abs() < 1e-9 && stand.mu.iter().all(|m| *m >= p.mu_floor * 0.99));
    }

    #[test]
    fn coasting_keeps_the_prediction_and_advances_the_time() {
        let p = params();
        let mut imm = Imm::new([0.0, 0.0], 3.0, 0, Mode::Walk, &p);
        let (preds, c) = imm.predict_to(5_000, Mode::Walk, &p);
        imm.coast(preds, c, 5_000, p.mu_floor);
        assert_eq!(imm.t_ms, 5_000);
        assert!(imm.output().p[0][0] > 9.0, "uncertainty grew");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc::mat loc::imm`
Expected: FAIL to compile: `file not found for module 'mat'` (after adding the `pub mod` lines) / `cannot find function 'inverse'`.

- [ ] **Step 3: Write `core/src/loc/mat.rs`**

```rust
//! Small fixed-size matrices for the filters (2x2 and 4x4 are all they need), so no linear-algebra crate is added.

use crate::num::count_f64;

/// A matrix of `R` rows and `C` columns, row-major.
pub type Mat<const R: usize, const C: usize> = [[f64; C]; R];

/// The zero matrix.
#[must_use]
pub fn zeros<const R: usize, const C: usize>() -> Mat<R, C> {
    [[0.0; C]; R]
}

/// The identity.
#[must_use]
pub fn identity<const N: usize>() -> Mat<N, N> {
    std::array::from_fn(|i| std::array::from_fn(|j| if i == j { 1.0 } else { 0.0 }))
}

/// `a * b`.
#[must_use]
pub fn mul<const R: usize, const K: usize, const C: usize>(a: &Mat<R, K>, b: &Mat<K, C>) -> Mat<R, C> {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..K).map(|k| a[i][k] * b[k][j]).sum()))
}

/// `a * v`.
#[must_use]
pub fn mul_vec<const R: usize, const C: usize>(a: &Mat<R, C>, v: &[f64; C]) -> [f64; R] {
    std::array::from_fn(|i| a[i].iter().zip(v).map(|(x, y)| x * y).sum())
}

/// The transpose.
#[must_use]
pub fn transpose<const R: usize, const C: usize>(a: &Mat<R, C>) -> Mat<C, R> {
    std::array::from_fn(|j| std::array::from_fn(|i| a[i][j]))
}

/// `a + b`.
#[must_use]
pub fn add<const R: usize, const C: usize>(a: &Mat<R, C>, b: &Mat<R, C>) -> Mat<R, C> {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] + b[i][j]))
}

/// `a - b`.
#[must_use]
pub fn sub<const R: usize, const C: usize>(a: &Mat<R, C>, b: &Mat<R, C>) -> Mat<R, C> {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] - b[i][j]))
}

/// `s * a`.
#[must_use]
pub fn scale<const R: usize, const C: usize>(a: &Mat<R, C>, s: f64) -> Mat<R, C> {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][j] * s))
}

/// `(a + a^T) / 2`: keeps a covariance exactly symmetric under rounding.
#[must_use]
pub fn symmetrize<const N: usize>(a: &Mat<N, N>) -> Mat<N, N> {
    std::array::from_fn(|i| std::array::from_fn(|j| 0.5 * (a[i][j] + a[j][i])))
}

/// `a b^T`.
#[must_use]
pub fn outer<const N: usize>(a: &[f64; N], b: &[f64; N]) -> Mat<N, N> {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i] * b[j]))
}

/// `a . b`.
#[must_use]
pub fn dot<const N: usize>(a: &[f64; N], b: &[f64; N]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// The inverse and the determinant of `a` (Gauss-Jordan with partial pivoting); `None` when it is singular.
#[must_use]
pub fn inverse<const N: usize>(a: &Mat<N, N>) -> Option<(Mat<N, N>, f64)> {
    let (mut m, mut inv, mut det) = (*a, identity::<N>(), 1.0);
    for col in 0..N {
        let pivot = (col..N).max_by(|&x, &y| m[x][col].abs().total_cmp(&m[y][col].abs()))?;
        if m[pivot][col].abs() < 1e-12 * (1.0 + a[col][col].abs()) {
            return None;
        }
        if pivot != col {
            m.swap(pivot, col);
            inv.swap(pivot, col);
            det = -det;
        }
        let d = m[col][col];
        det *= d;
        m[col] = m[col].map(|v| v / d);
        inv[col] = inv[col].map(|v| v / d);
        let (prow, irow) = (m[col], inv[col]);
        for r in (0..N).filter(|&r| r != col) {
            let f = m[r][col];
            m[r] = std::array::from_fn(|j| m[r][j] - f * prow[j]);
            inv[r] = std::array::from_fn(|j| inv[r][j] - f * irow[j]);
        }
    }
    Some((inv, det))
}

/// The largest eigenvalue of a symmetric 2x2 matrix.
#[must_use]
pub fn max_eig2(a: &Mat<2, 2>) -> f64 {
    let (m, d) = (0.5 * (a[0][0] + a[1][1]), 0.5 * (a[0][0] - a[1][1]));
    m + d.hypot(a[0][1])
}

/// One Kalman measurement update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Update {
    /// Posterior state.
    pub x: [f64; 4],
    /// Posterior covariance (Joseph form).
    pub p: Mat<4, 4>,
    /// Squared Mahalanobis distance of the innovation.
    pub d2: f64,
    /// Log likelihood of the measurement, `ln N(y; 0, S)`.
    pub log_likelihood: f64,
}

/// `[[a, 0], [0, b]]` of two 2x2 blocks.
#[must_use]
pub fn block_diag(a: &Mat<2, 2>, b: &Mat<2, 2>) -> Mat<4, 4> {
    std::array::from_fn(|i| {
        std::array::from_fn(|j| match (i < 2, j < 2) {
            (true, true) => a[i][j],
            (false, false) => b[i - 2][j - 2],
            _ => 0.0,
        })
    })
}

fn innovation<const M: usize>(x: &[f64; 4], p: &Mat<4, 4>, z: &[f64; M], h: &Mat<M, 4>, r: &Mat<M, M>) -> Option<([f64; M], Mat<M, M>, f64)> {
    let hx = mul_vec(h, x);
    let y: [f64; M] = std::array::from_fn(|i| z[i] - hx[i]);
    let s = add(&mul(&mul(h, p), &transpose(h)), r);
    let (s_inv, det) = inverse(&s)?;
    (det > 0.0).then_some((y, s_inv, det))
}

/// `y^T S^-1 y` of measuring `z = H x + v`, `v ~ N(0, R)` against the state; `None` when `S` is singular.
#[must_use]
pub fn mahalanobis2<const M: usize>(x: &[f64; 4], p: &Mat<4, 4>, z: &[f64; M], h: &Mat<M, 4>, r: &Mat<M, M>) -> Option<f64> {
    let (y, s_inv, _) = innovation(x, p, z, h, r)?;
    Some(dot(&y, &mul_vec(&s_inv, &y)))
}

/// Kalman update of `x`, `p` with `z = H x + v`, `v ~ N(0, R)`. The covariance uses the Joseph form `(I-KH) P (I-KH)^T + K R K^T`, which
/// stays symmetric and positive under rounding. `None` when the innovation covariance is singular.
#[must_use]
pub fn kalman_update<const M: usize>(x: &[f64; 4], p: &Mat<4, 4>, z: &[f64; M], h: &Mat<M, 4>, r: &Mat<M, M>) -> Option<Update> {
    let (y, s_inv, det) = innovation(x, p, z, h, r)?;
    let k = mul(&mul(p, &transpose(h)), &s_inv);
    let ky = mul_vec(&k, &y);
    let i_kh = sub(&identity::<4>(), &mul(&k, h));
    let p2 = add(&mul(&mul(&i_kh, p), &transpose(&i_kh)), &mul(&mul(&k, r), &transpose(&k)));
    let d2 = dot(&y, &mul_vec(&s_inv, &y));
    Some(Update {
        x: std::array::from_fn(|i| x[i] + ky[i]),
        p: symmetrize(&p2),
        d2,
        log_likelihood: -0.5 * (d2 + det.ln() + count_f64(M) * std::f64::consts::TAU.ln()),
    })
}
```

- [ ] **Step 4: Write `core/src/loc/params.rs`**

```rust
//! Every number of the location program in one place, so the bench can tune them (`replay --params p.json`). Defaults are the spec's.

use serde::{Deserialize, Serialize};

/// Parameters of the location filters. Later tasks add the matcher, bridge, calibration and carry-offset groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[allow(clippy::struct_field_names)] // names mirror the spec's symbols
pub struct LocParams {
    /// Measurement sigma floor, metres.
    pub sigma_floor_m: f64,
    /// Fixes coarser than this (68 % radius, metres) are unusable.
    pub unusable_acc_m: f64,
    /// A network fix (cold start, no GNSS yet) has its R multiplied by this.
    pub network_r_factor: f64,
    /// Below this speed a fix's velocity is not measured, m/s.
    pub min_speed_for_velocity_mps: f64,
    /// Stationary model: position random walk, m^2 per second.
    pub q_stationary_m2_per_s: f64,
    /// Stationary model: velocity sigma, m/s.
    pub stationary_vel_sigma_mps: f64,
    /// White-noise acceleration of the walking model in a Walk zone (and Bike/Drive zones), m/s^2.
    pub sigma_a_walk: f64,
    /// White-noise acceleration of the walking model in a Run zone, m/s^2.
    pub sigma_a_run: f64,
    /// White-noise acceleration of the fast model (Walk, Run and Bike zones), m/s^2.
    pub sigma_a_bike: f64,
    /// White-noise acceleration of the fast model in a Drive zone, m/s^2.
    pub sigma_a_drive: f64,
    /// Initial velocity sigma in Walk/Run zones, m/s.
    pub v0_slow_mps: f64,
    /// Initial velocity sigma in Bike/Drive zones, m/s.
    pub v0_fast_mps: f64,
    /// Initial model probabilities in Walk/Run zones (S, W, F).
    pub mu0_slow: [f64; 3],
    /// Initial model probabilities in Bike/Drive zones.
    pub mu0_fast: [f64; 3],
    /// Per-second switching rates in Walk/Run zones (the diagonal is ignored: it is the remainder).
    pub pi_slow: [[f64; 3]; 3],
    /// Per-second switching rates in Bike/Drive zones.
    pub pi_fast: [[f64; 3]; 3],
    /// Longest single prediction step, seconds (longer gaps are predicted in steps).
    pub max_predict_s: f64,
    /// Most of a `Pi` row that may switch away in one step.
    pub max_offdiag_share: f64,
    /// Model probability floor.
    pub mu_floor: f64,
    /// Soft gate, 2 dof (99 %).
    pub gate_soft_2: f64,
    /// Hard gate, 2 dof (99.9 %).
    pub gate_hard_2: f64,
    /// Soft gate, 4 dof (99 %).
    pub gate_soft_4: f64,
    /// Hard gate, 4 dof (99.9 %).
    pub gate_hard_4: f64,
    /// Consecutive agreeing gated fixes that mean a real relocation.
    pub reloc_count: usize,
    /// ... spanning at least this, ms.
    pub reloc_span_ms: i64,
    /// After this much continuous gating, ms, two good agreeing fixes are enough.
    pub reloc_quick_after_ms: i64,
    /// "Good" for the quick rule, metres.
    pub reloc_quick_acc_m: f64,
    /// `v_max` = this x the mode's speed cap.
    pub reloc_vmax_factor: f64,
    /// Reset after this long without an accepted fix, ms.
    pub reset_gap_ms: i64,
    /// Reset when the position covariance trace exceeds this, m^2.
    pub reset_trace_m2: f64,
    /// Hold: stationary probability above this ...
    pub hold_mu_s: f64,
    /// ... speed below this, m/s ...
    pub hold_speed_mps: f64,
    /// ... and no new steps for this long, ms.
    pub hold_quiet_steps_ms: i64,
    /// Hold ends after this many consecutive far fixes.
    pub hold_exit_fixes: u32,
    /// "Far" is at least this, metres (or 2 sigma of the estimate, if more).
    pub hold_exit_min_m: f64,
    /// Stationary likelihood factor when steps are coming in.
    pub steps_moving_factor: f64,
    /// Walking likelihood factor when no steps came for a while (Walk/Run).
    pub steps_quiet_factor: f64,
    /// "Steps are coming in": at least `steps_moving_min` in this window, ms.
    pub steps_moving_window_ms: i64,
    /// Minimum steps in the moving window.
    pub steps_moving_min: i64,
    /// "No steps for a while" window, ms.
    pub steps_quiet_window_ms: i64,
    /// Course is reported from this speed, m/s ...
    pub course_min_speed_mps: f64,
    /// ... when its sigma is below this, degrees.
    pub course_max_sigma_deg: f64,
    /// Accepted estimates are at most this uncertain, metres.
    pub max_uncertainty_m: f64,
    /// Simulated fixes become exact estimates of this uncertainty, metres.
    pub sim_uncertainty_m: f64,
    /// Whether mock-location fixes are used (debug bench only).
    pub allow_mock: bool,
}

impl Default for LocParams {
    fn default() -> Self {
        Self {
            sigma_floor_m: 2.0,
            unusable_acc_m: 100.0,
            network_r_factor: 4.0,
            min_speed_for_velocity_mps: 0.5,
            q_stationary_m2_per_s: 0.05 * 0.05,
            stationary_vel_sigma_mps: 0.1,
            sigma_a_walk: 0.5,
            sigma_a_run: 1.0,
            sigma_a_bike: 1.5,
            sigma_a_drive: 3.0,
            v0_slow_mps: 2.0,
            v0_fast_mps: 10.0,
            mu0_slow: [0.6, 0.35, 0.05],
            mu0_fast: [0.4, 0.1, 0.5],
            pi_slow: [[0.95, 0.048, 0.002], [0.05, 0.94, 0.01], [0.02, 0.08, 0.90]],
            pi_fast: [[0.93, 0.02, 0.05], [0.05, 0.80, 0.15], [0.04, 0.01, 0.95]],
            max_predict_s: 10.0,
            max_offdiag_share: 0.9,
            mu_floor: 1e-4,
            gate_soft_2: 9.21,
            gate_hard_2: 13.8,
            gate_soft_4: 13.28,
            gate_hard_4: 18.5,
            reloc_count: 3,
            reloc_span_ms: 2_000,
            reloc_quick_after_ms: 20_000,
            reloc_quick_acc_m: 20.0,
            reloc_vmax_factor: 1.5,
            reset_gap_ms: 5 * 60_000,
            reset_trace_m2: 200.0 * 200.0,
            hold_mu_s: 0.8,
            hold_speed_mps: 0.3,
            hold_quiet_steps_ms: 10_000,
            hold_exit_fixes: 2,
            hold_exit_min_m: 3.0,
            steps_moving_factor: 0.1,
            steps_quiet_factor: 0.3,
            steps_moving_window_ms: 5_000,
            steps_moving_min: 2,
            steps_quiet_window_ms: 10_000,
            course_min_speed_mps: 0.8,
            course_max_sigma_deg: 25.0,
            max_uncertainty_m: crate::loc::MAX_UNCERTAINTY_M,
            sim_uncertainty_m: 3.0,
            allow_mock: false,
        }
    }
}
```

- [ ] **Step 5: Write `core/src/loc/imm.rs`** (models and cycle; Task 6 adds the gate and relocation to this file)

```rust
//! Layer 1: an interacting multiple model (IMM) Kalman filter with three models sharing the state `[e, n, ve, vn]` (metres, m/s, ENU):
//! S stationary, W walking (constant velocity, gentle), F fast (constant velocity, agile).

use crate::catalog::Mode;
use crate::loc::mat::{add, block_diag, identity, kalman_update, mul, mul_vec, outer, scale, symmetrize, transpose, zeros, Mat};
use crate::loc::params::LocParams;
use crate::num::i64_to_f64;

/// Index of the stationary model.
pub const S: usize = 0;
/// Index of the walking model.
pub const W: usize = 1;
/// Index of the fast model.
pub const F: usize = 2;

const H_POS: Mat<2, 4> = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]];

/// A state and its covariance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gaussian {
    /// `[e, n, ve, vn]`.
    pub x: [f64; 4],
    /// Covariance.
    pub p: Mat<4, 4>,
}

/// One fix as a measurement in the filter frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meas {
    /// Position, metres east and north.
    pub pos: [f64; 2],
    /// Its covariance.
    pub r_pos: Mat<2, 2>,
    /// Velocity and its covariance, when the fix has a usable one.
    pub vel: Option<([f64; 2], Mat<2, 2>)>,
}

/// The position block of a covariance.
#[must_use]
pub fn pos_block(p: &Mat<4, 4>) -> Mat<2, 2> {
    [[p[0][0], p[0][1]], [p[1][0], p[1][1]]]
}

/// The velocity block of a covariance.
#[must_use]
pub fn vel_block(p: &Mat<4, 4>) -> Mat<2, 2> {
    [[p[2][2], p[2][3]], [p[3][2], p[3][3]]]
}

fn slow(mode: Mode) -> bool {
    matches!(mode, Mode::Walk | Mode::Run)
}

/// The model transition matrix for a step of `dt_s`: per-second rates times `min(dt, max_predict_s)` off the diagonal (scaled down if a row
/// would switch away more than `max_offdiag_share`), the remainder on it.
#[must_use]
pub fn transition(p: &LocParams, mode: Mode, dt_s: f64) -> [[f64; 3]; 3] {
    let rates = if slow(mode) { p.pi_slow } else { p.pi_fast };
    let t = dt_s.clamp(0.0, p.max_predict_s);
    std::array::from_fn(|i| {
        let mut off: [f64; 3] = std::array::from_fn(|j| if i == j { 0.0 } else { rates[i][j] * t });
        let sum: f64 = off.iter().sum();
        if sum > p.max_offdiag_share {
            off = off.map(|v| v * p.max_offdiag_share / sum);
        }
        let sum: f64 = off.iter().sum();
        std::array::from_fn(|j| if i == j { 1.0 - sum } else { off[j] })
    })
}

/// White-noise acceleration of `model` in a zone of `mode` (S has none).
#[must_use]
pub fn sigma_a(p: &LocParams, mode: Mode, model: usize) -> f64 {
    match model {
        W if mode == Mode::Run => p.sigma_a_run,
        W => p.sigma_a_walk,
        F if mode == Mode::Drive => p.sigma_a_drive,
        F => p.sigma_a_bike,
        _ => 0.0,
    }
}

/// Constant-velocity process noise over `dt`: per axis `sigma_a^2 [[dt^4/4, dt^3/2], [dt^3/2, dt^2]]`.
#[must_use]
pub fn cv_noise(sigma_a: f64, dt: f64) -> Mat<4, 4> {
    let s2 = sigma_a * sigma_a;
    let (a, b, c) = (s2 * dt.powi(4) / 4.0, s2 * dt.powi(3) / 2.0, s2 * dt * dt);
    [[a, 0.0, b, 0.0], [0.0, a, 0.0, b], [b, 0.0, c, 0.0], [0.0, b, 0.0, c]]
}

/// Predict `g` under `model` for `dt` seconds.
#[must_use]
pub fn predict(g: &Gaussian, model: usize, dt: f64, sigma_a: f64, p: &LocParams) -> Gaussian {
    if model == S {
        let pb = pos_block(&g.p);
        let q = p.q_stationary_m2_per_s * dt;
        let vs = p.stationary_vel_sigma_mps * p.stationary_vel_sigma_mps;
        let cov = [[pb[0][0] + q, pb[0][1], 0.0, 0.0], [pb[1][0], pb[1][1] + q, 0.0, 0.0], [0.0, 0.0, vs, 0.0], [0.0, 0.0, 0.0, vs]];
        return Gaussian { x: [g.x[0], g.x[1], 0.0, 0.0], p: cov };
    }
    let f: Mat<4, 4> = [[1.0, 0.0, dt, 0.0], [0.0, 1.0, 0.0, dt], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
    Gaussian { x: mul_vec(&f, &g.x), p: symmetrize(&add(&mul(&mul(&f, &g.p), &transpose(&f)), &cv_noise(sigma_a, dt))) }
}

/// IMM mixing: each model's starting state is the blend of all models weighted by how likely each switched into it. Returns the mixed
/// states and the predicted model probabilities `c_j = sum_i pi_ij mu_i`.
#[must_use]
pub fn mix(models: &[Gaussian; 3], mu: &[f64; 3], pi: &[[f64; 3]; 3]) -> ([Gaussian; 3], [f64; 3]) {
    let c: [f64; 3] = std::array::from_fn(|j| (0..3).map(|i| pi[i][j] * mu[i]).sum());
    let mixed = std::array::from_fn(|j| {
        let w: [f64; 3] = std::array::from_fn(|i| if c[j] > 0.0 { pi[i][j] * mu[i] / c[j] } else { 0.0 });
        combine(models, &w)
    });
    (mixed, c)
}

/// The moment-matched blend of the models: `x = sum w x_j`, `P = sum w (P_j + (x_j - x)(x_j - x)^T)`.
#[must_use]
pub fn combine(models: &[Gaussian; 3], w: &[f64; 3]) -> Gaussian {
    let x: [f64; 4] = std::array::from_fn(|k| (0..3).map(|j| w[j] * models[j].x[k]).sum());
    let mut p = zeros::<4, 4>();
    for (g, wj) in models.iter().zip(w) {
        let d: [f64; 4] = std::array::from_fn(|k| g.x[k] - x[k]);
        p = add(&p, &scale(&add(&g.p, &outer(&d, &d)), *wj));
    }
    Gaussian { x, p: symmetrize(&p) }
}

/// The filter: three models, their probabilities, and the time of the state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Imm {
    /// Per-model states.
    pub models: [Gaussian; 3],
    /// Model probabilities (S, W, F).
    pub mu: [f64; 3],
    /// Time of the state, Unix ms.
    pub t_ms: i64,
}

fn normalized(v: [f64; 3], floor: f64) -> [f64; 3] {
    let s: f64 = v.iter().sum();
    let v = if s > 0.0 && s.is_finite() { v.map(|x| x / s) } else { [1.0 / 3.0; 3] };
    let v = v.map(|x| x.max(floor));
    let s: f64 = v.iter().sum();
    v.map(|x| x / s)
}

impl Imm {
    /// A fresh filter at `z` (metres, ENU) with position sigma `sigma`: velocity 0 with the mode's initial sigma, the mode's model priors.
    #[must_use]
    pub fn new(z: [f64; 2], sigma: f64, t_ms: i64, mode: Mode, p: &LocParams) -> Self {
        let v0 = if slow(mode) { p.v0_slow_mps } else { p.v0_fast_mps };
        let mut cov = zeros::<4, 4>();
        cov[0][0] = sigma * sigma;
        cov[1][1] = sigma * sigma;
        cov[2][2] = v0 * v0;
        cov[3][3] = v0 * v0;
        let g = Gaussian { x: [z[0], z[1], 0.0, 0.0], p: cov };
        Self { models: [g; 3], mu: if slow(mode) { p.mu0_slow } else { p.mu0_fast }, t_ms }
    }

    /// Mixed and predicted model states at `t_ms` (gaps over `max_predict_s` are predicted in steps), and the predicted probabilities.
    #[must_use]
    pub fn predict_to(&self, t_ms: i64, mode: Mode, p: &LocParams) -> ([Gaussian; 3], [f64; 3]) {
        let dt = (i64_to_f64(t_ms - self.t_ms) / 1000.0).max(0.0);
        let (mut preds, c) = mix(&self.models, &self.mu, &transition(p, mode, dt));
        for (j, g) in preds.iter_mut().enumerate() {
            let mut left = dt;
            while left > 0.0 {
                let step = left.min(p.max_predict_s);
                *g = predict(g, j, step, sigma_a(p, mode, j), p);
                left -= step;
            }
        }
        (preds, c)
    }

    /// Update the predicted models with `m` (its R scaled by `r_scale` for a soft-gated fix), weigh each model's likelihood by `weights_by_model` (step
    /// evidence) and the predicted probabilities `c`. False (and nothing changed) if no model could take the measurement.
    #[allow(clippy::too_many_arguments)] // one IMM step: everything it needs, nothing it keeps
    pub fn update(&mut self, preds: [Gaussian; 3], c: [f64; 3], m: &Meas, r_scale: f64, weights_by_model: [f64; 3], t_ms: i64, mu_floor: f64) -> bool {
        let mut post = preds;
        let mut logs = [f64::NEG_INFINITY; 3];
        for (j, g) in preds.iter().enumerate() {
            let u = match m.vel {
                Some((v, rv)) => kalman_update(&g.x, &g.p, &[m.pos[0], m.pos[1], v[0], v[1]], &identity::<4>(), &block_diag(&scale(&m.r_pos, r_scale), &rv)),
                None => kalman_update(&g.x, &g.p, &m.pos, &H_POS, &scale(&m.r_pos, r_scale)),
            };
            if let Some(u) = u {
                post[j] = Gaussian { x: u.x, p: u.p };
                logs[j] = u.log_likelihood + weights_by_model[j].max(1e-300).ln() + c[j].max(1e-300).ln();
            }
        }
        let best = logs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !best.is_finite() {
            return false;
        }
        self.models = post;
        self.mu = normalized(logs.map(|l| (l - best).exp()), mu_floor);
        self.t_ms = t_ms;
        true
    }

    /// No usable measurement (a gated fix): keep the predictions and the predicted probabilities, move the clock.
    pub fn coast(&mut self, preds: [Gaussian; 3], c: [f64; 3], t_ms: i64, mu_floor: f64) {
        self.models = preds;
        self.mu = normalized(c, mu_floor);
        self.t_ms = t_ms;
    }

    /// The combined estimate.
    #[must_use]
    pub fn output(&self) -> Gaussian {
        combine(&self.models, &self.mu)
    }

    /// Move every model's position through `f` (re-anchoring the frame: old ENU -> geo -> new ENU, exact, so nothing jumps).
    pub fn map_positions(&mut self, f: &dyn Fn([f64; 2]) -> [f64; 2]) {
        for g in &mut self.models {
            let [e, n] = f([g.x[0], g.x[1]]);
            g.x[0] = e;
            g.x[1] = n;
        }
    }
}
```

In `core/src/loc/mod.rs` add after `pub mod frame;`: `pub mod imm; pub mod mat; pub mod params;` and `pub use params::LocParams;`.

- [ ] **Step 6: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core loc::`
Expected: PASS (mat 3, imm 8, earlier loc tests).
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add core/src/loc
git commit -m "feat: add the IMM Kalman models" -m "Hand-rolled 2x2/4x4 matrices (Joseph-form update), every spec number in
LocParams, and the three-model IMM cycle: mix, predict, update,
combine." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 6: Gating, soft gate, relocation, reset and unusable fixes

**Files:**

- Modify: `core/src/loc/imm.rs` (append the gate section and its tests)

**Interfaces:**

- Consumes: `Gaussian`, `Meas`, `H_POS`, `LocParams` (Task 5); `RawFix`, `Provider` (Task 2); `mat::mahalanobis2`.
- Produces: `imm::Gate { Accept { r_scale: f64, soft: bool }, Reject }`, `imm::gate(d2, with_velocity, &LocParams) -> Gate`,
  `imm::min_d2(&[Gaussian; 3], &Meas) -> f64`, `imm::GatedFix { en: [f64; 2], acc_m: f64, t_ms: i64 }`,
  `imm::Relocator` (`Default`, `push(&mut self, GatedFix, vmax_mps, &LocParams) -> bool`, `clear(&mut self)`, `map_positions(&mut self, &dyn Fn([f64; 2]) -> [f64; 2])`),
  `imm::unusable(&RawFix, last_t_ms: Option<i64>, gnss_seen: bool, &LocParams) -> bool`,
  `imm::needs_reset(last_accepted_ms: Option<i64>, t_ms, pos_trace_m2, &LocParams) -> bool`, `imm::mode_cap_mps(Mode) -> f64`.
- Plan choices: the 4-dof soft gate is 13.28 (chi-square 99 %, the spec gives only the 2-dof one); Drive has no speed cap in `speed_ok`,
  so its relocation `v_max` uses 150 km/h; a negative accuracy (iOS "invalid") is unusable.

- [ ] **Step 1: Write the failing tests** (append inside `mod tests` of `imm.rs`)

```rust
    #[test]
    fn the_gate_follows_the_chi_square_boundaries() {
        let p = params();
        assert_eq!(gate(9.2, false, &p), Gate::Accept { r_scale: 1.0, soft: false });
        assert_eq!(gate(13.79, false, &p), Gate::Accept { r_scale: 13.79 / 9.21, soft: true });
        assert_eq!(gate(13.81, false, &p), Gate::Reject);
        assert_eq!(gate(18.4, true, &p), Gate::Accept { r_scale: 18.4 / 13.28, soft: true });
        assert_eq!(gate(18.6, true, &p), Gate::Reject);
        assert_eq!(gate(f64::NAN, false, &p), Gate::Reject);
    }

    #[test]
    fn the_gate_distance_is_the_smallest_over_the_models() {
        let near = Gaussian { x: [0.0; 4], p: identity::<4>() };
        let far = Gaussian { x: [100.0, 0.0, 0.0, 0.0], p: identity::<4>() };
        let m = Meas { pos: [1.0, 0.0], r_pos: identity::<2>(), vel: None };
        assert!((min_d2(&[far, near, far], &m) - 0.5).abs() < 1e-12);
    }

    fn gf(e: f64, t_s: i64, acc: f64) -> GatedFix {
        GatedFix { en: [e, 0.0], acc_m: acc, t_ms: t_s * 1000 }
    }

    #[test]
    fn three_agreeing_gated_fixes_over_two_seconds_mean_a_relocation() {
        let p = params();
        let mut r = Relocator::default();
        assert!(!r.push(gf(2000.0, 1, 5.0), 5.0, &p));
        assert!(!r.push(gf(2003.0, 2, 5.0), 5.0, &p));
        assert!(r.push(gf(2001.0, 3, 5.0), 5.0, &p), "third agreeing fix, 2 s span");
    }

    #[test]
    fn scattered_or_too_quick_gated_fixes_are_not_a_relocation() {
        let p = params();
        let mut r = Relocator::default();
        for (e, t) in [(2000.0, 1), (2200.0, 2), (1800.0, 3)] {
            assert!(!r.push(gf(e, t, 5.0), 5.0, &p), "scattered");
        }
        let mut q = Relocator::default();
        for t_ms in [1000, 1500, 1900] {
            assert!(!q.push(GatedFix { en: [2000.0, 0.0], acc_m: 5.0, t_ms }, 5.0, &p), "under 2 s");
        }
    }

    #[test]
    fn after_twenty_seconds_of_gating_two_good_agreeing_fixes_are_enough() {
        let p = params();
        let mut r = Relocator::default();
        assert!(!r.push(gf(2000.0, 0, 40.0), 5.0, &p));
        assert!(!r.push(gf(2500.0, 10, 40.0), 5.0, &p));
        assert!(!r.push(gf(2000.0, 21, 15.0), 5.0, &p));
        assert!(r.push(gf(2004.0, 22, 15.0), 5.0, &p), "two fixes <= 20 m that agree, after 20 s of gating");
    }

    #[test]
    fn unusable_rules() {
        // Review Focus 1: stale, duplicate and invalid fixes never reach the filter.
        let p = params();
        let ok = RawFix::at(40.0, -111.0, 10_000, 5.0);
        assert!(!unusable(&ok, Some(9_000), false, &p));
        assert!(unusable(&RawFix { accuracy_m: 101.0, ..ok }, None, false, &p), "coarser than 100 m");
        assert!(unusable(&RawFix { accuracy_m: -1.0, ..ok }, None, false, &p), "negative accuracy");
        assert!(unusable(&ok, Some(10_000), false, &p), "duplicate time");
        assert!(unusable(&ok, Some(11_000), false, &p), "older than the last fix");
        assert!(unusable(&RawFix { lat: f64::NAN, ..ok }, None, false, &p));
        assert!(unusable(&RawFix { lat: 91.0, ..ok }, None, false, &p));
        assert!(unusable(&RawFix { mock: true, ..ok }, None, false, &p));
        assert!(!unusable(&RawFix { mock: true, ..ok }, None, false, &LocParams { allow_mock: true, ..params() }));
        let net = RawFix { provider: Provider::Network, ..ok };
        assert!(!unusable(&net, None, false, &p), "a cold-start network fix is fine");
        assert!(unusable(&net, None, true, &p), "never mixed in once GNSS fixes arrive");
    }

    #[test]
    fn a_long_gap_or_a_lost_filter_resets() {
        let p = params();
        assert!(needs_reset(None, 0, 1.0, &p));
        assert!(!needs_reset(Some(0), 300_000, 1.0, &p));
        assert!(needs_reset(Some(0), 300_001, 1.0, &p));
        assert!(needs_reset(Some(0), 1_000, 40_001.0, &p));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc::imm`
Expected: FAIL to compile: `cannot find function 'gate'`, `cannot find type 'Relocator'`.

- [ ] **Step 3: Append the gate section to `imm.rs`** (above `#[cfg(test)]`)

```rust
use crate::loc::mat::mahalanobis2;
use crate::loc::{Provider, RawFix};

/// What the gate decided for one fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gate {
    /// Use it; a soft-gated fix has its R inflated by `r_scale` (robust, Huber-like).
    Accept {
        /// R multiplier (1 unless soft).
        r_scale: f64,
        /// Whether it was soft-gated.
        soft: bool,
    },
    /// A GPS jump: ignore it.
    Reject,
}

/// Gate by the squared innovation distance: hard gate 13.8 (2 dof, 99.9 %) or 18.5 (4 dof); between the 99 % and 99.9 % bounds the fix is
/// used with R scaled by `d2 / soft`.
#[must_use]
pub fn gate(d2: f64, with_velocity: bool, p: &LocParams) -> Gate {
    let (soft, hard) = if with_velocity { (p.gate_soft_4, p.gate_hard_4) } else { (p.gate_soft_2, p.gate_hard_2) };
    if !d2.is_finite() || d2 > hard {
        Gate::Reject
    } else if d2 > soft {
        Gate::Accept { r_scale: d2 / soft, soft: true }
    } else {
        Gate::Accept { r_scale: 1.0, soft: false }
    }
}

/// The smallest squared innovation distance of `m` over the three predicted models (infinite when none can take it).
#[must_use]
pub fn min_d2(preds: &[Gaussian; 3], m: &Meas) -> f64 {
    preds
        .iter()
        .filter_map(|g| match m.vel {
            Some((v, rv)) => mahalanobis2(&g.x, &g.p, &[m.pos[0], m.pos[1], v[0], v[1]], &identity::<4>(), &block_diag(&m.r_pos, &rv)),
            None => mahalanobis2(&g.x, &g.p, &m.pos, &H_POS, &m.r_pos),
        })
        .fold(f64::INFINITY, f64::min)
}

/// A gated fix kept to judge a relocation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatedFix {
    /// Position, metres ENU.
    pub en: [f64; 2],
    /// Reported accuracy, metres.
    pub acc_m: f64,
    /// Time, Unix ms.
    pub t_ms: i64,
}

/// Consecutive gated fixes, to tell a real relocation (they agree with each other) from a burst of bad fixes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Relocator {
    pending: Vec<GatedFix>,
    since_ms: Option<i64>,
}

fn agree(a: &GatedFix, b: &GatedFix, vmax: f64) -> bool {
    let d = (a.en[0] - b.en[0]).hypot(a.en[1] - b.en[1]);
    d <= a.acc_m + b.acc_m + vmax * (i64_to_f64((a.t_ms - b.t_ms).abs()) / 1000.0)
}

impl Relocator {
    /// Forget the gated fixes (a fix was accepted, or the filter restarted).
    pub fn clear(&mut self) {
        self.pending.clear();
        self.since_ms = None;
    }

    /// Move the kept fixes through `f` (re-anchoring).
    pub fn map_positions(&mut self, f: &dyn Fn([f64; 2]) -> [f64; 2]) {
        for g in &mut self.pending {
            g.en = f(g.en);
        }
    }

    /// Add a gated fix; true when the newest one should be believed: the last `reloc_count` agree pairwise and span `reloc_span_ms`, or,
    /// after `reloc_quick_after_ms` of continuous gating, two fixes of at most `reloc_quick_acc_m` agree.
    pub fn push(&mut self, f: GatedFix, vmax_mps: f64, p: &LocParams) -> bool {
        let since = *self.since_ms.get_or_insert(f.t_ms);
        self.pending.push(f);
        if self.pending.len() > 16 {
            self.pending.remove(0);
        }
        let n = p.reloc_count.max(2);
        if self.pending.len() >= n {
            let last = &self.pending[self.pending.len() - n..];
            let all_agree = last.iter().enumerate().all(|(i, a)| last[i + 1..].iter().all(|b| agree(a, b, vmax_mps)));
            if all_agree && last[n - 1].t_ms - last[0].t_ms >= p.reloc_span_ms {
                return true;
            }
        }
        f.t_ms - since >= p.reloc_quick_after_ms
            && f.acc_m <= p.reloc_quick_acc_m
            && self.pending[..self.pending.len() - 1].iter().any(|g| g.acc_m <= p.reloc_quick_acc_m && agree(g, &f, vmax_mps))
    }
}

/// Whether a fix is dropped before the filter: invalid, coarser than `unusable_acc_m`, not newer than the last fix, a mock (unless the
/// bench allows it), or a network fix once GNSS fixes have arrived.
#[must_use]
pub fn unusable(f: &RawFix, last_t_ms: Option<i64>, gnss_seen: bool, p: &LocParams) -> bool {
    let invalid = !(f.lat.is_finite() && f.lon.is_finite() && f.accuracy_m.is_finite())
        || !(-90.0..=90.0).contains(&f.lat)
        || !(-180.0..=180.0).contains(&f.lon)
        || f.accuracy_m < 0.0;
    invalid
        || f.accuracy_m > p.unusable_acc_m
        || last_t_ms.is_some_and(|t| f.t_ms <= t)
        || (f.mock && !p.allow_mock)
        || (f.provider == Provider::Network && gnss_seen)
}

/// Whether the filter restarts at this fix: none yet, more than `reset_gap_ms` since the last accepted fix, or lost (`P` trace too big).
#[must_use]
pub fn needs_reset(last_accepted_ms: Option<i64>, t_ms: i64, pos_trace_m2: f64, p: &LocParams) -> bool {
    last_accepted_ms.is_none_or(|l| t_ms - l > p.reset_gap_ms) || pos_trace_m2 > p.reset_trace_m2
}

/// The speed above which quests of a mode stop counting (`Game::speed_ok`), m/s; Drive has none there, 150 km/h here.
#[must_use]
pub fn mode_cap_mps(mode: Mode) -> f64 {
    let kmh = match mode {
        Mode::Walk => 12.0,
        Mode::Run => 25.0,
        Mode::Bike => 50.0,
        Mode::Drive => 150.0,
    };
    kmh / 3.6
}
```

(Move the two new `use` lines to the top of the file with the others when rustfmt or clippy asks.)

- [ ] **Step 4: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core loc::imm`
Expected: PASS.
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/loc/imm.rs
git commit -m "feat: gate GPS jumps and detect relocation" -m "Chi-square hard and soft gates over the three models, relocation from
agreeing gated fixes, resets after a 5 min gap or a lost filter, and
the unusable-fix rules (stale, duplicate, mock, network after GNSS)." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 7: The `Locator` facade, stationary hold, step evidence and the `Estimate`

**Files:**

- Create: `core/src/loc/locator.rs`
- Modify: `core/src/loc/mod.rs` (add `mod locator;` and `pub use locator::{Locator, Odometer, StepHistory};`)

**Interfaces:**

- Consumes: everything of Tasks 2, 5 and 6.
- Produces:
  - `Locator` (`Clone`, `Default`): `new(LocParams)`, `params()`, `set_mode(Mode)`, `mode()`, `reset()`, `last() -> Option<Estimate>`,
    `holding() -> bool`, `steps() -> &StepHistory`, `on_fix(&RawFix) -> Estimate`, `on_steps(total, t_ms, cadence: Option<f64>) -> Option<Estimate>`
    (always `None` until Task 22).
  - `StepHistory`: `push(total, t_ms)`, `present()`, `gained(from_ms, to_ms) -> i64`, `cadence(t_ms) -> Option<f64>`.
  - `Odometer` (`Copy`, `Default`): `step(&Estimate) -> f64` (metres to add), `clear()`, `Odometer::MAX_GAP_MS = 300_000`. One rule for
    the game (Task 9) and the bench: an accepted estimate adds the distance from the previous accepted one unless it is a `Reset` or
    `Relocated`, the gap is over 5 min, or the motion is `Stationary`.
- Plan choices: `Locator` lives in `locator.rs` (re-exported from `mod.rs`, which the spec names) to keep files small. A velocity measurement
  needs speed >= 0.5 m/s, speed accuracy > 0 and a bearing accuracy in (0, 180) degrees. The first real fix after simulated ones resets the
  filter and forgets the last fix time (the simulator's clock runs ahead of the wall clock).

- [ ] **Step 1: Write the failing tests** (bottom of `core/src/loc/locator.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;
    use crate::loc::frame::REANCHOR_M;

    fn o() -> Point {
        Point::new(40.0, -111.0)
    }

    fn fix(p: Point, t_s: i64, acc: f64) -> RawFix {
        RawFix::at(p.lat, p.lon, t_s * 1000, acc)
    }

    fn stand(l: &mut Locator, from_s: i64, n: i64) -> i64 {
        for i in 0..n {
            l.on_fix(&fix(destination(o(), f64::from(u16::try_from(i * 97 % 360).unwrap()), 2.0), from_s + i, 6.0));
        }
        from_s + n
    }

    #[test]
    fn the_first_fix_starts_the_filter_at_the_fix() {
        let mut l = Locator::default();
        let e = l.on_fix(&fix(o(), 1, 5.0));
        assert_eq!(e.verdict, Verdict::Reset);
        assert!(e.accepted && distance_m(e.point(), o()) < 1e-6);
        assert!((e.uncertainty_m - 5.0).abs() < 0.01, "{}", e.uncertainty_m);
    }

    #[test]
    fn a_coarse_first_fix_is_blurry() {
        let e = Locator::default().on_fix(&fix(o(), 1, 50.0));
        assert_eq!(e.verdict, Verdict::Blurry);
        assert!(!e.accepted);
    }

    #[test]
    fn a_spike_is_gated_and_does_not_move_the_estimate() {
        let mut l = Locator::default();
        for t in 1..=20 {
            l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, 5.0));
        }
        let before = l.last().unwrap();
        let e = l.on_fix(&fix(destination(o(), 0.0, 150.0), 21, 5.0));
        assert_eq!(e.verdict, Verdict::Gated);
        assert!(!e.accepted && distance_m(e.point(), before.point()) < 3.0);
    }

    #[test]
    fn three_far_fixes_in_a_row_relocate() {
        let mut l = Locator::default();
        let t = stand(&mut l, 1, 10);
        let far = destination(o(), 90.0, 5000.0);
        let v: Vec<Verdict> = (0..3).map(|i| l.on_fix(&fix(far, t + i, 5.0)).verdict).collect();
        assert_eq!(v, [Verdict::Gated, Verdict::Gated, Verdict::Relocated]);
        assert!(distance_m(l.last().unwrap().point(), far) < 1e-6);
    }

    #[test]
    fn a_gap_over_five_minutes_resets() {
        let mut l = Locator::default();
        l.on_fix(&fix(o(), 0, 5.0));
        assert_eq!(l.on_fix(&fix(destination(o(), 0.0, 20_000.0), 301, 5.0)).verdict, Verdict::Reset);
    }

    #[test]
    fn standing_still_holds_the_position_until_two_far_fixes() {
        let mut l = Locator::default();
        let mut t = stand(&mut l, 0, 40);
        assert!(l.holding());
        let held = l.last().unwrap().point();
        assert_eq!(l.on_fix(&fix(destination(o(), 90.0, 4.0), t, 6.0)).point(), held, "one far fix does not end the hold");
        t += 1;
        let mut moved = false;
        for k in 1..=5 {
            moved |= l.on_fix(&fix(destination(o(), 90.0, 10.0 + 1.4 * i64_to_f64(k)), t, 6.0)).point() != held;
            t += 1;
        }
        assert!(moved && !l.holding());
    }

    #[test]
    fn steps_coming_in_end_the_hold_at_once() {
        let mut l = Locator::default();
        l.on_steps(100, 0, None);
        l.on_steps(100, 30_000, None);
        stand(&mut l, 1, 40);
        assert!(l.holding());
        l.on_steps(104, 41_500, None);
        assert!(!l.holding(), "4 steps in the last 5 s");
    }

    #[test]
    fn simulated_fixes_bypass_the_filter() {
        let mut l = Locator::default();
        let sim = |p: Point, t: i64| RawFix { provider: Provider::Sim, ..fix(p, t, 5.0) };
        let a = l.on_fix(&sim(o(), 1));
        let b = l.on_fix(&sim(destination(o(), 0.0, 5000.0), 2));
        assert!(a.accepted && b.accepted && a.verdict == Verdict::Used && b.verdict == Verdict::Used);
        assert!((b.uncertainty_m - 3.0).abs() < 1e-9 && distance_m(b.point(), destination(o(), 0.0, 5000.0)) < 1e-6);
    }

    #[test]
    fn a_real_fix_after_simulated_ones_starts_fresh() {
        // Review Focus 3: the simulator's clock runs ahead and it teleports; the first real fix must not be gated or dropped as stale.
        let mut l = Locator::default();
        l.on_fix(&RawFix { provider: Provider::Sim, ..fix(destination(o(), 0.0, 3000.0), 1_000_000, 5.0) });
        let e = l.on_fix(&fix(o(), 10, 5.0));
        assert_eq!(e.verdict, Verdict::Reset);
        assert!(e.accepted && distance_m(e.point(), o()) < 1e-6);
    }

    #[test]
    fn without_a_step_counter_hold_and_walking_still_work() {
        // Review Focus 2: no on_steps call at all.
        let mut l = Locator::default();
        let t = stand(&mut l, 0, 40);
        assert!(l.holding());
        let mut last = l.last().unwrap();
        for k in 1..=40 {
            last = l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(k)), t + k, 5.0));
        }
        assert!(!l.holding() && last.motion != Motion::Stationary, "{last:?}");
        assert!(distance_m(last.point(), destination(o(), 90.0, 56.0)) < 6.0);
    }

    #[test]
    fn crossing_the_reanchor_distance_moves_nothing() {
        // Review Focus 4: a 7.2 km ride at 8 m/s crosses the 5 km re-anchor; the estimate and the odometer stay smooth.
        let mut l = Locator::default();
        l.set_mode(Mode::Bike);
        let mut odo = Odometer::default();
        let (mut total, mut prev): (f64, Option<Estimate>) = (0.0, None);
        for t in 0..=900 {
            let e = l.on_fix(&fix(destination(o(), 90.0, 8.0 * i64_to_f64(t)), t, 4.0));
            total += odo.step(&e);
            if let Some(p) = prev {
                assert!(distance_m(p.point(), e.point()) < 9.5, "jump at {t}: {}", distance_m(p.point(), e.point()));
            }
            prev = Some(e);
        }
        assert!(8.0 * 900.0 > REANCHOR_M);
        assert!((total - 7200.0).abs() < 72.0, "odometer {total}");
    }

    #[test]
    fn odd_fix_fields_never_break_the_estimate() {
        // Review Focus 5: accuracy 0, negative or NaN speed, missing or negative (iOS "unknown") accuracies.
        let mut l = Locator::default();
        let base = fix(o(), 1, 0.0);
        let odd = [
            base,
            RawFix { t_ms: 2000, speed_mps: Some(-1.0), speed_acc_mps: Some(0.0), bearing_deg: Some(90.0), bearing_acc_deg: None, ..base },
            RawFix { t_ms: 3000, speed_mps: Some(1.0), speed_acc_mps: Some(0.5), bearing_deg: Some(90.0), bearing_acc_deg: Some(-1.0), ..base },
            RawFix { t_ms: 4000, speed_mps: Some(f64::NAN), speed_acc_mps: Some(0.5), bearing_deg: Some(f64::NAN), bearing_acc_deg: Some(10.0), ..base },
        ];
        let first = l.on_fix(&odd[0]);
        assert!((first.uncertainty_m - 2.0 * ACC_TO_SIGMA).abs() < 1e-6, "accuracy 0 still gets the 2 m sigma floor: {first:?}");
        for f in &odd[1..] {
            let e = l.on_fix(f);
            assert!(e.accepted, "{e:?}");
            assert!(e.uncertainty_m.is_finite() && e.uncertainty_m > 0.0 && e.lat.is_finite() && e.speed_mps.is_finite(), "{e:?}");
        }
        assert!(l.measurement(&odd[1], [0.0, 0.0]).vel.is_none() && l.measurement(&odd[2], [0.0, 0.0]).vel.is_none());
        assert!(l.measurement(&odd[3], [0.0, 0.0]).vel.is_none());
    }

    #[test]
    fn a_network_fix_counts_double_sigma_before_gnss_and_is_dropped_after() {
        let mut l = Locator::default();
        let net = RawFix { provider: Provider::Network, ..fix(o(), 1, 30.0) };
        let e = l.on_fix(&net);
        assert!((e.uncertainty_m - 60.0).abs() < 0.1, "R x 4 = sigma x 2: {}", e.uncertainty_m);
        l.on_fix(&RawFix { provider: Provider::Gps, ..fix(o(), 2, 5.0) });
        assert_eq!(l.on_fix(&RawFix { t_ms: 3000, ..net }).verdict, Verdict::Unusable);
    }

    #[test]
    fn the_odometer_counts_walking_and_not_standing_or_gaps() {
        let mut odo = Odometer::default();
        let at = |m: f64, t_s: i64, motion: Motion| Estimate { motion, ..Estimate::exact(destination(o(), 90.0, m).lat, destination(o(), 90.0, m).lon, t_s * 1000) };
        assert_eq!(odo.step(&at(0.0, 0, Motion::Walking)), 0.0);
        assert!((odo.step(&at(10.0, 5, Motion::Walking)) - 10.0).abs() < 0.01);
        assert_eq!(odo.step(&at(12.0, 10, Motion::Stationary)), 0.0);
        assert_eq!(odo.step(&at(500.0, 400, Motion::Walking)), 0.0, "over 5 min");
        assert_eq!(odo.step(&Estimate { accepted: false, ..at(600.0, 401, Motion::Walking) }), 0.0);
        assert_eq!(odo.step(&Estimate { verdict: Verdict::Relocated, ..at(5000.0, 402, Motion::Walking) }), 0.0);
    }

    #[test]
    fn step_history_gains_cadence_and_counter_restarts() {
        let mut h = StepHistory::default();
        for (t, n) in [(0, 100), (2_000, 104), (4_000, 108), (6_000, 112)] {
            h.push(n, t);
        }
        assert_eq!(h.gained(1_000, 6_000), 8);
        assert!((h.cadence(6_000).unwrap() - 2.0).abs() < 1e-9);
        h.push(5, 8_000);
        assert_eq!(h.gained(7_000, 8_000), 0, "a restarted counter is a new baseline");
        h.push(3, 7_000);
        assert_eq!(h.gained(0, 9_000), 0, "an older reading is ignored");
    }

    #[test]
    fn random_fixes_never_panic_and_stay_finite() {
        // Property test (spec "proptest-style"): seeded random walks, jumps, NaNs and time jitter.
        use rand::rngs::StdRng;
        use rand::{RngExt, SeedableRng};
        for seed in 0..20 {
            let mut rng = StdRng::seed_from_u64(seed);
            let mut l = Locator::default();
            let mut t = 0_i64;
            for _ in 0..500 {
                t += rng.random_range(-500..3_000);
                let p = destination(o(), rng.random_range(0.0..360.0), rng.random_range(0.0..3_000.0));
                let acc = if rng.random_range(0..50) == 0 { f64::NAN } else { rng.random_range(0.0..150.0) };
                let e = l.on_fix(&RawFix { speed_mps: Some(rng.random_range(-1.0..40.0)), ..RawFix::at(p.lat, p.lon, t, acc) });
                assert!(e.uncertainty_m.is_finite() && e.uncertainty_m > 0.0, "{e:?}");
                let sum: f32 = e.mode_probs.iter().sum();
                assert!(e.verdict == Verdict::Unusable || (sum - 1.0).abs() < 1e-4, "{e:?}");
            }
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc::locator`
Expected: FAIL to compile: `file not found for module 'locator'` / `cannot find type 'Locator'`.

- [ ] **Step 3: Write `core/src/loc/locator.rs`** (above the tests)

```rust
//! The `Locator`: the one thing the game talks to. Raw fixes, steps and compass readings in; estimates out. Not saved: a restart starts
//! fresh (spec "Filter state").

use std::collections::VecDeque;

use crate::catalog::Mode;
use crate::geo::{distance_m, Point};
use crate::loc::frame::Frame;
use crate::loc::imm::{self, Gate, GatedFix, Imm, Meas, Relocator, S, W};
use crate::loc::mat::{identity, max_eig2, scale, Mat};
use crate::loc::params::LocParams;
use crate::loc::{Estimate, Motion, Provider, RawFix, Source, Verdict, ACC_TO_SIGMA};
use crate::num::{i64_to_f64, to_f32};

/// Recent readings of the phone's cumulative step counter (the last two minutes).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StepHistory {
    pts: VecDeque<(i64, i64)>,
}

impl StepHistory {
    /// Add a reading; one not newer than the last is ignored, a counter that went down (the phone restarted it) starts over.
    pub fn push(&mut self, total: i64, t_ms: i64) {
        if let Some(&(t, last)) = self.pts.back() {
            if t_ms <= t {
                return;
            }
            if total < last {
                self.pts.clear();
            }
        }
        self.pts.push_back((t_ms, total));
        while self.pts.front().is_some_and(|(t, _)| t_ms - t > 120_000) {
            self.pts.pop_front();
        }
    }

    /// Whether the phone has reported steps this session.
    #[must_use]
    pub fn present(&self) -> bool {
        !self.pts.is_empty()
    }

    fn total_at(&self, t_ms: i64) -> Option<i64> {
        self.pts.iter().rev().find(|(t, _)| *t <= t_ms).or(self.pts.front()).map(|(_, n)| *n)
    }

    /// Steps taken between `from_ms` and `to_ms` (0 without readings).
    #[must_use]
    pub fn gained(&self, from_ms: i64, to_ms: i64) -> i64 {
        match (self.total_at(from_ms), self.total_at(to_ms)) {
            (Some(a), Some(b)) => (b - a).max(0),
            _ => 0,
        }
    }

    /// Steps per second over the 10 s before `t_ms`, when the readings span at least 2 s.
    #[must_use]
    pub fn cadence(&self, t_ms: i64) -> Option<f64> {
        let first = self.pts.iter().find(|(t, _)| *t >= t_ms - 10_000)?;
        let last = self.pts.iter().rev().find(|(t, _)| *t <= t_ms)?;
        let dt = i64_to_f64(last.0 - first.0) / 1000.0;
        (dt >= 2.0).then(|| i64_to_f64(last.1 - first.1) / dt)
    }
}

/// Distance travelled, from accepted estimates. One rule for the game and the bench.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Odometer {
    last: Option<Estimate>,
}

impl Odometer {
    /// A gap longer than this between accepted estimates is not travelled distance, ms.
    pub const MAX_GAP_MS: i64 = 300_000;

    /// Forget the previous point (counting paused, game reopened).
    pub fn clear(&mut self) {
        self.last = None;
    }

    /// Metres to add for `e`: the step from the previous accepted estimate, unless `e` is not accepted, restarts the filter, comes after a
    /// gap over 5 min, or the player is standing.
    pub fn step(&mut self, e: &Estimate) -> f64 {
        if !e.accepted {
            return 0.0;
        }
        let moved = match self.last {
            Some(l) if !matches!(e.verdict, Verdict::Reset | Verdict::Relocated) && e.t_ms - l.t_ms <= Self::MAX_GAP_MS && e.motion != Motion::Stationary => {
                distance_m(l.point(), e.point())
            }
            _ => 0.0,
        };
        self.last = Some(*e);
        moved
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Hold {
    at: [f64; 2],
    far: u32,
}

/// The location filter of one game session.
#[derive(Debug, Clone)]
pub struct Locator {
    params: LocParams,
    mode: Mode,
    frame: Option<Frame>,
    imm: Option<Imm>,
    last_t_ms: Option<i64>,
    last_accepted_ms: Option<i64>,
    gnss_seen: bool,
    last_sim: bool,
    reloc: Relocator,
    hold: Option<Hold>,
    steps: StepHistory,
    last: Option<Estimate>,
}

impl Default for Locator {
    fn default() -> Self {
        Self::new(LocParams::default())
    }
}

impl Locator {
    /// A fresh locator with `params`, in a Walk zone until told otherwise.
    #[must_use]
    pub fn new(params: LocParams) -> Self {
        Self {
            params,
            mode: Mode::Walk,
            frame: None,
            imm: None,
            last_t_ms: None,
            last_accepted_ms: None,
            gnss_seen: false,
            last_sim: false,
            reloc: Relocator::default(),
            hold: None,
            steps: StepHistory::default(),
            last: None,
        }
    }

    /// The parameters in use.
    #[must_use]
    pub fn params(&self) -> &LocParams {
        &self.params
    }

    /// The travel mode at the player's position (see [`crate::loc::mode_at`]); applies from the next fix.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// The travel mode in use.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Start fresh at the next fix (counting toggled, game opened). The fix clock, the step history and "GNSS seen" are kept.
    pub fn reset(&mut self) {
        self.frame = None;
        self.imm = None;
        self.last_accepted_ms = None;
        self.reloc.clear();
        self.hold = None;
        self.last = None;
    }

    /// The newest estimate made from a fix that reached the filter.
    #[must_use]
    pub fn last(&self) -> Option<Estimate> {
        self.last
    }

    /// Whether the stationary hold is freezing the position.
    #[must_use]
    pub fn holding(&self) -> bool {
        self.hold.is_some()
    }

    /// The step counter readings of this session.
    #[must_use]
    pub fn steps(&self) -> &StepHistory {
        &self.steps
    }

    /// A step counter reading. Steps coming in end a stationary hold at once. Returns a bridged estimate while a GPS gap is being bridged
    /// (Task 22); `None` otherwise.
    pub fn on_steps(&mut self, total: i64, t_ms: i64, _cadence: Option<f64>) -> Option<Estimate> {
        self.steps.push(total, t_ms);
        if self.moving_steps(t_ms) {
            self.hold = None;
        }
        None
    }

    /// Feed one raw fix; the estimate says what became of it.
    pub fn on_fix(&mut self, f: &RawFix) -> Estimate {
        if f.provider == Provider::Sim {
            return self.simulated(f);
        }
        if std::mem::take(&mut self.last_sim) {
            self.reset();
            self.last_t_ms = None;
        }
        if imm::unusable(f, self.last_t_ms, self.gnss_seen, &self.params) {
            return self.unusable(f);
        }
        self.last_t_ms = Some(f.t_ms);
        self.gnss_seen |= f.provider.is_gnss();
        let restart = self.imm.as_ref().is_none_or(|imm| {
            let o = imm.output();
            imm::needs_reset(self.last_accepted_ms, f.t_ms, o.p[0][0] + o.p[1][1], &self.params)
        });
        let Some(frame) = self.frame.filter(|_| !restart) else {
            self.start(f, self.sigma_of(f));
            return self.emit(f.t_ms, Verdict::Reset);
        };
        let z = frame.to_enu(f.point());
        let meas = self.measurement(f, z);
        let weights_by_model = self.step_factors(f.t_ms);
        let (mode, vmax) = (self.mode, imm::mode_cap_mps(self.mode) * self.params.reloc_vmax_factor);
        let Some(imm) = self.imm.as_mut() else {
            self.start(f, self.sigma_of(f));
            return self.emit(f.t_ms, Verdict::Reset);
        };
        let (preds, c) = imm.predict_to(f.t_ms, mode, &self.params);
        match imm::gate(imm::min_d2(&preds, &meas), meas.vel.is_some(), &self.params) {
            Gate::Reject => {
                imm.coast(preds, c, f.t_ms, self.params.mu_floor);
                if self.reloc.push(GatedFix { en: z, acc_m: f.accuracy_m, t_ms: f.t_ms }, vmax, &self.params) {
                    self.start(f, self.sigma_of(f));
                    return self.emit(f.t_ms, Verdict::Relocated);
                }
                self.emit(f.t_ms, Verdict::Gated)
            }
            Gate::Accept { r_scale, soft } => {
                if !imm.update(preds, c, &meas, r_scale, weights_by_model, f.t_ms, self.params.mu_floor) {
                    self.start(f, self.sigma_of(f));
                    return self.emit(f.t_ms, Verdict::Reset);
                }
                self.reloc.clear();
                self.last_accepted_ms = Some(f.t_ms);
                self.update_hold(z, f.t_ms);
                self.emit(f.t_ms, if soft { Verdict::Soft } else { Verdict::Used })
            }
        }
    }

    fn moving_steps(&self, t_ms: i64) -> bool {
        self.steps.present() && self.steps.gained(t_ms - self.params.steps_moving_window_ms, t_ms) >= self.params.steps_moving_min
    }

    fn quiet_steps(&self, t_ms: i64, window_ms: i64) -> bool {
        !self.steps.present() || self.steps.gained(t_ms - window_ms, t_ms) == 0
    }

    /// Step evidence as model likelihood factors (S, W, F): steps coming in make standing unlikely; none for 10 s makes walking unlikely
    /// in a Walk/Run zone. Neutral without a step counter.
    fn step_factors(&self, t_ms: i64) -> [f64; 3] {
        let mut l = [1.0; 3];
        if self.steps.present() {
            if self.moving_steps(t_ms) {
                l[S] *= self.params.steps_moving_factor;
            }
            if matches!(self.mode, Mode::Walk | Mode::Run) && self.quiet_steps(t_ms, self.params.steps_quiet_window_ms) {
                l[W] *= self.params.steps_quiet_factor;
            }
        }
        l
    }

    fn sigma_of(&self, f: &RawFix) -> f64 {
        let s = (f.accuracy_m / ACC_TO_SIGMA).max(self.params.sigma_floor_m);
        if f.provider == Provider::Network {
            s * self.params.network_r_factor.sqrt()
        } else {
            s
        }
    }

    pub(crate) fn measurement(&self, f: &RawFix, z: [f64; 2]) -> Meas {
        let sigma = self.sigma_of(f);
        let vel = match (f.speed_mps, f.speed_acc_mps, f.bearing_deg, f.bearing_acc_deg) {
            (Some(v), Some(va), Some(b), Some(bacc))
                if v.is_finite() && b.is_finite() && v >= self.params.min_speed_for_velocity_mps && va > 0.0 && bacc > 0.0 && bacc < 180.0 =>
            {
                let (sb, cb) = b.to_radians().sin_cos();
                let (along, cross) = (va * va, (v * bacc.to_radians().sin()).powi(2).max(0.01));
                let (u, w) = ([sb, cb], [cb, -sb]);
                let r: Mat<2, 2> = std::array::from_fn(|i| std::array::from_fn(|j| along * u[i] * u[j] + cross * w[i] * w[j]));
                Some(([v * sb, v * cb], r))
            }
            _ => None,
        };
        Meas { pos: z, r_pos: scale(&identity::<2>(), sigma * sigma), vel }
    }

    fn start(&mut self, f: &RawFix, sigma: f64) {
        self.frame = Some(Frame::new(f.point()));
        self.imm = Some(Imm::new([0.0, 0.0], sigma, f.t_ms, self.mode, &self.params));
        self.last_accepted_ms = Some(f.t_ms);
        self.reloc.clear();
        self.hold = None;
    }

    fn simulated(&mut self, f: &RawFix) -> Estimate {
        self.reset();
        self.last_sim = true;
        self.last_t_ms = Some(f.t_ms);
        self.start(f, self.params.sim_uncertainty_m / ACC_TO_SIGMA);
        self.emit(f.t_ms, Verdict::Used)
    }

    fn unusable(&self, f: &RawFix) -> Estimate {
        let finite = |x: f64| if x.is_finite() { x } else { 0.0 };
        let fallback = Estimate {
            t_ms: f.t_ms,
            lat: finite(f.lat),
            lon: finite(f.lon),
            uncertainty_m: if f.accuracy_m.is_finite() { f.accuracy_m.clamp(1.0, 1e6) } else { 1e6 },
            ..Estimate::default()
        };
        Estimate { verdict: Verdict::Unusable, accepted: false, ..self.last.unwrap_or(fallback) }
    }

    fn update_hold(&mut self, z: [f64; 2], t_ms: i64) {
        let Some(imm) = &self.imm else { return };
        let p = &self.params;
        let out = imm.output();
        let sigma = max_eig2(&imm::pos_block(&out.p)).max(0.0).sqrt();
        let moving = self.moving_steps(t_ms);
        match self.hold {
            Some(mut h) => {
                let far = (z[0] - h.at[0]).hypot(z[1] - h.at[1]) > p.hold_exit_min_m.max(2.0 * sigma);
                h.far = if far { h.far + 1 } else { 0 };
                self.hold = (h.far < p.hold_exit_fixes && !moving).then_some(h);
            }
            None => {
                let still = imm.mu[S] > p.hold_mu_s && out.x[2].hypot(out.x[3]) < p.hold_speed_mps && self.quiet_steps(t_ms, p.hold_quiet_steps_ms);
                if still {
                    self.hold = Some(Hold { at: [out.x[0], out.x[1]], far: 0 });
                }
            }
        }
    }

    /// Move the anchor to the estimate once it is 5 km out; every position is mapped exactly (old ENU -> geo -> new ENU).
    fn reanchor_if_far(&mut self) {
        let (Some(imm), Some(frame)) = (self.imm.as_mut(), self.frame) else { return };
        let out = imm.output();
        let pos = self.hold.map_or([out.x[0], out.x[1]], |h| h.at);
        if !frame.needs_reanchor(pos) {
            return;
        }
        let new = Frame::new(frame.to_geo(pos));
        let map = |en: [f64; 2]| new.to_enu(frame.to_geo(en));
        imm.map_positions(&map);
        self.reloc.map_positions(&map);
        if let Some(h) = &mut self.hold {
            h.at = map(h.at);
        }
        self.frame = Some(new);
    }

    fn emit(&mut self, t_ms: i64, verdict: Verdict) -> Estimate {
        self.reanchor_if_far();
        let (Some(imm), Some(frame)) = (self.imm.as_ref(), self.frame.as_ref()) else {
            return Estimate { t_ms, verdict: Verdict::Unusable, uncertainty_m: 1e6, ..Estimate::default() };
        };
        let p = &self.params;
        let out = imm.output();
        let pos = self.hold.map_or([out.x[0], out.x[1]], |h| h.at);
        let uncertainty_m = ACC_TO_SIGMA * max_eig2(&imm::pos_block(&out.p)).max(0.0).sqrt();
        let (ve, vn) = (out.x[2], out.x[3]);
        let speed = ve.hypot(vn);
        let vb = imm::vel_block(&out.p);
        let course_deg = if speed >= p.course_min_speed_mps {
            let w = [-vn / speed, ve / speed]; // across the direction of travel
            let cross = w[0] * w[0] * vb[0][0] + 2.0 * w[0] * w[1] * vb[0][1] + w[1] * w[1] * vb[1][1];
            let sigma_deg = (cross.max(0.0).sqrt() / speed).atan().to_degrees();
            (sigma_deg < p.course_max_sigma_deg).then(|| ve.atan2(vn).to_degrees().rem_euclid(360.0))
        } else {
            None
        };
        let best = (0..3).max_by(|&a, &b| imm.mu[a].total_cmp(&imm.mu[b])).unwrap_or(S);
        let at = frame.to_geo(pos);
        let verdict = if verdict.may_count() && uncertainty_m > p.max_uncertainty_m { Verdict::Blurry } else { verdict };
        let est = Estimate {
            t_ms,
            lat: at.lat,
            lon: at.lon,
            uncertainty_m,
            speed_mps: speed,
            speed_sigma_mps: (0.5 * (vb[0][0] + vb[1][1])).max(0.0).sqrt(),
            course_deg,
            motion: [Motion::Stationary, Motion::Walking, Motion::Fast][best],
            mode_probs: imm.mu.map(to_f32),
            source: Source::Gps,
            verdict,
            accepted: verdict.may_count(),
        };
        self.last = Some(est);
        est
    }
}
```

In `core/src/loc/mod.rs` add `mod locator;` and `pub use locator::{Locator, Odometer, StepHistory};`.

- [ ] **Step 4: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core loc::`
Expected: PASS (all locator tests). If a threshold-style assertion fails (hold not engaging in 40 fixes, walking motion), tune only
`LocParams` defaults inside the ranges the spec states and say so in the commit body; never loosen a test.
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/loc
git commit -m "feat: add the Locator with stationary hold" -m "One facade for fixes and steps: gating, relocation, resets, the
stationary hold, step evidence, simulated fixes, re-anchoring and the
Estimate with its accepted rule; plus the shared odometer rule." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 8: Layer 1 CI scenarios, RTS reference, timing and the replay on the `Locator`

**Files:**

- Create: `core/src/loc/bench/run.rs`
- Modify: `core/src/loc/bench/reference.rs` (add `rts_reference`), `core/src/loc/bench.rs` (re-exports), `core/examples/replay.rs`
- Modify: `core/tests/loc_scenarios.rs` (append the layer 1 scenarios)

**Interfaces:**

- Consumes: `Locator`, `Odometer`, `Estimate`, `LocParams` (Task 7); metrics (Task 3); `Scenario`, `Run` (Task 2); `imm::cv_noise`, `mat` (Task 5).
- Produces:
  - `bench::ReplayOpts { mode: Mode, params: LocParams }` (`Default`: Walk, defaults; Task 19 and 22 add fields with defaults),
    `bench::Replay { shown: Vec<Shown>, estimates: Vec<Estimate> }`,
    `bench::run_locator(fixes: &[RawFix], steps: &[(i64, i64)], headings: &[HeadingIn], opts: &ReplayOpts) -> Replay`,
    `bench::run_scenario(r: &Run, opts: &ReplayOpts) -> Replay`, `bench::shown(e: &Estimate, odometer_m: f64) -> Shown`.
  - `bench::rts_reference(fixes: &[RawFix], max_acc_m: f64, sigma_a: f64) -> Vec<TruthPoint>`.
  - Replay CLI gains `--params p.json` (a `LocParams` JSON, missing fields default) and `--compare baseline`.
- Plan choices: the spec's "RTS-smoothed IMM track" is implemented as an RTS smoother of the constant-velocity model over the fixes with
  accuracy <= 15 m (a full IMM smoother is not worth it for a reference; listed under spec gaps). `--params` reads JSON, not TOML: no new
  crate (Global Constraints). "Held at both stops" is checked as `held_share >= 0.7` over the stop windows (the spec gives no number).
  "Shown deviation <= 3 m" for a spike is the shown step at the spike fix minus the true step.

- [ ] **Step 1: Write the failing tests**

Bottom of `core/src/loc/bench/reference.rs` test module, add:

```rust
    #[test]
    fn the_rts_reference_is_smoother_than_the_fixes_and_close_to_the_truth() {
        use crate::geo::Point;
        use crate::loc::bench::{Leg, Scenario};
        let s = Scenario::walk(Point::new(40.0, -111.0), vec![Leg::Move { bearing_deg: 90.0, dist_m: 420.0, speed_mps: 1.4 }], 8.0);
        let r = s.generate(3);
        let refr = rts_reference(&r.fixes, 15.0, 0.5);
        let err = |p: Point, t: i64| crate::geo::distance_m(p, crate::loc::bench::interp(&r.truth, t));
        let raw: f64 = r.fixes.iter().map(|f| err(f.point(), f.t_ms)).sum::<f64>() / 300.0;
        let smooth: f64 = refr.iter().map(|t| err(t.p, t.t_ms)).sum::<f64>() / 300.0;
        assert!(smooth < raw * 0.7, "smoothed {smooth} vs raw {raw}");
        assert!((refr[150].speed_mps - 1.4).abs() < 0.3 && (refr[150].course_deg - 90.0).abs() < 10.0);
    }
```

Append to `core/tests/loc_scenarios.rs`:

```rust
use apgo_core::catalog::Mode;
use apgo_core::geo::destination;
use apgo_core::loc::bench::{arrival_lag_s, completes, false_jumps, overshoot_m, percentile, position_error, run_scenario, turn_lags_s, ReplayOpts, Spike};
use apgo_core::loc::Verdict;

fn opts(mode: Mode) -> ReplayOpts {
    ReplayOpts { mode, ..ReplayOpts::default() }
}

fn east(m: f64, speed: f64) -> Leg {
    Leg::Move { bearing_deg: 90.0, dist_m: m, speed_mps: speed }
}

#[test]
fn straight_walk_is_within_three_metres_rms_and_never_jumps() {
    let s = Scenario::walk(origin(), vec![east(420.0, 1.4)], 5.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let (rms, p95) = position_error(&r.truth, &out.shown);
        assert!(rms <= 3.0 && p95 <= 6.0, "seed {seed}: rms {rms:.2} p95 {p95:.2}");
        assert_eq!(false_jumps(&r.truth, &out.shown), 0, "seed {seed}");
    }
}

#[test]
fn a_corner_is_turned_within_four_seconds_without_overshooting_eight_metres() {
    let s = Scenario::walk(origin(), vec![east(100.0, 1.4), Leg::Move { bearing_deg: 0.0, dist_m: 100.0, speed_mps: 1.4 }], 5.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let lags = turn_lags_s(&r.truth, &out.shown);
        assert!(lags.len() == 1 && lags[0] <= 4.0, "seed {seed}: {lags:?}");
        assert!(overshoot_m(&r.truth, &out.shown) <= 8.0, "seed {seed}: {}", overshoot_m(&r.truth, &out.shown));
    }
}

#[test]
fn standing_five_minutes_holds_still_and_adds_no_distance() {
    let mut s = Scenario::walk(origin(), vec![Leg::Stop { secs: 300 }], 8.0);
    s.steps = true;
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let j = apgo_core::loc::bench::stationary_jitter(&r.truth, &out.shown).unwrap();
        assert!(j.rms_m <= 2.0 && j.drift_m_per_min <= 5.0, "seed {seed}: {j:?}");
        assert!(j.held_share >= 0.95, "seed {seed}: held {:.2}", j.held_share);
        assert!(out.shown.last().unwrap().odometer_m <= 5.0, "seed {seed}: {}", out.shown.last().unwrap().odometer_m);
    }
}

#[test]
fn a_spike_and_a_burst_are_gated_and_complete_nothing() {
    let mut s = Scenario::walk(origin(), vec![east(420.0, 1.4)], 5.0);
    let burst = |at_s, bearing_deg| Spike { at_s, offset_m: 80.0, bearing_deg, len: 1 };
    s.spikes = vec![Spike { at_s: 100, offset_m: 150.0, bearing_deg: 0.0, len: 1 }, burst(200, 0.0), burst(201, 120.0), burst(202, 240.0)];
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        for at_s in [100, 200, 201, 202] {
            let t = s.t0_ms + i64::from(at_s) * 1000;
            let k = out.shown.iter().position(|x| x.t_ms == t).unwrap();
            assert_eq!(out.shown[k].verdict, Verdict::Gated, "seed {seed} at {at_s}");
            let dev = distance_m(out.shown[k].p, out.shown[k - 1].p) - 1.4;
            assert!(dev <= 3.0, "seed {seed} at {at_s}: shown moved {dev:.1} m more than the walker");
        }
        let spike_spot = destination(apgo_core::loc::bench::truth_at(&r.truth, s.t0_ms + 100_000).p, 0.0, 150.0);
        assert!(!completes(&out.shown, spike_spot, 25.0), "seed {seed}: the spike would complete a quest");
    }
}

#[test]
fn a_real_relocation_is_believed_within_three_fixes() {
    let mut s = Scenario::walk(origin(), vec![east(84.0, 1.4), Leg::Move { bearing_deg: 0.0, dist_m: 2000.0, speed_mps: 2000.0 / 60.0 }, Leg::Stop { secs: 60 }], 5.0);
    s.gaps = vec![(61, 121)];
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let after: Vec<_> = out.shown.iter().filter(|x| x.t_ms >= s.t0_ms + 121_000).collect();
        let k = after.iter().position(|x| x.verdict == Verdict::Relocated || x.verdict == Verdict::Reset).expect("believed");
        assert!(k < 3 || after[k].t_ms - after[0].t_ms <= 15_000, "seed {seed}: believed at fix {k}");
    }
}

#[test]
fn a_bike_arrives_within_two_seconds_and_holds_at_both_stops() {
    let legs = vec![east(600.0, 5.0), Leg::Stop { secs: 30 }, east(600.0, 5.0), Leg::Stop { secs: 30 }, east(300.0, 5.0)];
    let s = Scenario::walk(origin(), legs, 5.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Bike));
        for d in [300.0, 600.0, 900.0, 1200.0] {
            let lag = arrival_lag_s(&r.truth, &out.shown, destination(origin(), 90.0, d), 25.0).unwrap();
            assert!(lag <= 2.0, "seed {seed} at {d} m: {lag}");
        }
        let j = apgo_core::loc::bench::stationary_jitter(&r.truth, &out.shown).unwrap();
        assert!(j.held_share >= 0.7, "seed {seed}: held {:.2}", j.held_share);
    }
}

#[test]
fn balanced_quality_fixes_never_complete_an_off_route_target_and_arrive_within_ten_seconds() {
    let mut s = Scenario::walk(origin(), vec![east(600.0, 1.4)], 25.0);
    s.acc_spread_m = 15.0;
    s.fix_every_s = 5;
    let off_route = destination(destination(origin(), 90.0, 300.0), 0.0, 60.0);
    let mut lags = Vec::new();
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        assert!(!completes(&out.shown, off_route, 25.0), "seed {seed}");
        for d in [100.0, 200.0, 300.0, 400.0, 500.0] {
            lags.push(arrival_lag_s(&r.truth, &out.shown, destination(origin(), 90.0, d), 25.0).unwrap());
        }
    }
    assert!(percentile(&lags, 0.9) <= 10.0, "p90 {}", percentile(&lags, 0.9));
}

#[test]
fn walking_in_arrives_within_three_seconds_and_a_pass_at_35_m_never_counts() {
    let target = destination(origin(), 90.0, 200.0);
    let walk_in = Scenario::walk(origin(), vec![east(220.0, 1.4)], 5.0);
    let pass = Scenario::walk(destination(origin(), 0.0, 35.0), vec![east(400.0, 1.4)], 5.0);
    for seed in 0..SEEDS {
        let r = walk_in.generate(seed);
        let lag = arrival_lag_s(&r.truth, &run_scenario(&r, &opts(Mode::Walk)).shown, target, 25.0).unwrap();
        assert!(lag <= 3.0, "seed {seed}: {lag}");
        let p = pass.generate(seed);
        assert!(!completes(&run_scenario(&p, &opts(Mode::Walk)).shown, target, 25.0), "seed {seed}: a 35 m pass counted");
    }
}

#[test]
fn driving_in_a_walk_zone_is_tracked_and_too_fast_to_count() {
    let s = Scenario::walk(origin(), vec![east(2000.0, 50.0 / 3.6)], 5.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let mut streak = 0;
        let mut worst = 0;
        for x in &out.shown {
            streak = if x.verdict == Verdict::Gated { streak + 1 } else { 0 };
            worst = worst.max(streak);
        }
        assert!(worst <= 3, "seed {seed}: {worst} gated in a row");
        let late: Vec<_> = out.estimates.iter().filter(|e| e.accepted && e.t_ms > s.t0_ms + 30_000).collect();
        let blocked = late.iter().filter(|e| (e.speed_mps - 2.0 * e.speed_sigma_mps) * 3.6 > 12.0).count();
        assert!(blocked * 10 >= late.len() * 9, "seed {seed}: speed_ok would block {blocked} of {}", late.len());
    }
}

#[test]
#[ignore = "timing harness: cargo test --release --test loc_scenarios timing -- --ignored --nocapture"]
fn timing_of_on_fix() {
    let s = Scenario::walk(origin(), vec![east(4200.0, 1.4)], 5.0);
    let r = s.generate(1);
    let mut l = apgo_core::loc::Locator::default();
    let mut us: Vec<f64> = r
        .fixes
        .iter()
        .map(|f| {
            let t = std::time::Instant::now();
            let _ = l.on_fix(f);
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    us.sort_by(f64::total_cmp);
    println!("on_fix: mean {:.1} us, p99 {:.1} us over {} fixes", us.iter().sum::<f64>() / us.len() as f64, percentile(&us, 0.99), us.len());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core --test loc_scenarios`
Expected: FAIL to compile: `cannot find function 'run_scenario'`, `cannot find struct 'ReplayOpts'`.

- [ ] **Step 3: Write `core/src/loc/bench/run.rs`**

```rust
//! Feed fixes, steps and headings through a `Locator` in time order, the way the phone does, with the shared odometer.

use crate::catalog::Mode;
use crate::loc::bench::{Run, Shown};
use crate::loc::{Estimate, HeadingIn, LocParams, Locator, Odometer, RawFix};

/// How to replay.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayOpts {
    /// Travel mode of the zone.
    pub mode: Mode,
    /// Filter parameters.
    pub params: LocParams,
}

impl Default for ReplayOpts {
    fn default() -> Self {
        Self { mode: Mode::Walk, params: LocParams::default() }
    }
}

/// What a replay produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Replay {
    /// Shown positions, one per fix (and per bridged step batch).
    pub shown: Vec<Shown>,
    /// The estimates behind them.
    pub estimates: Vec<Estimate>,
}

/// An estimate as a shown position.
#[must_use]
pub fn shown(e: &Estimate, odometer_m: f64) -> Shown {
    Shown { t_ms: e.t_ms, p: e.point(), uncertainty_m: e.uncertainty_m, accepted: e.accepted, verdict: e.verdict, course_deg: e.course_deg, odometer_m }
}

enum Ev {
    Steps(i64),
    Head(HeadingIn),
    Fix(RawFix),
}

/// Replay a recording through a fresh `Locator`.
#[must_use]
pub fn run_locator(fixes: &[RawFix], steps: &[(i64, i64)], headings: &[HeadingIn], opts: &ReplayOpts) -> Replay {
    let mut evs: Vec<(i64, u8, Ev)> = steps.iter().map(|(t, n)| (*t, 0, Ev::Steps(*n))).collect();
    evs.extend(headings.iter().map(|h| (h.t_ms, 1, Ev::Head(*h))));
    evs.extend(fixes.iter().map(|f| (f.t_ms, 2, Ev::Fix(*f))));
    evs.sort_by_key(|(t, order, _)| (*t, *order));
    let mut loc = Locator::new(opts.params.clone());
    loc.set_mode(opts.mode);
    let (mut odo, mut total) = (Odometer::default(), 0.0);
    let mut out = Replay { shown: Vec::new(), estimates: Vec::new() };
    for (t, _, ev) in evs {
        let est = match ev {
            Ev::Fix(f) => Some(loc.on_fix(&f)),
            Ev::Steps(n) => loc.on_steps(n, t, None),
            Ev::Head(_) => None, // Task 14 feeds headings
        };
        if let Some(e) = est {
            total += odo.step(&e);
            out.shown.push(shown(&e, total));
            out.estimates.push(e);
        }
    }
    out
}

/// Replay a synthetic run.
#[must_use]
pub fn run_scenario(r: &Run, opts: &ReplayOpts) -> Replay {
    run_locator(&r.fixes, &r.steps, &r.headings, opts)
}
```

Re-export from `bench.rs`: `mod run;` and `pub use run::{run_locator, run_scenario, shown, Replay, ReplayOpts};`.

- [ ] **Step 4: Add `rts_reference` to `core/src/loc/bench/reference.rs`**

```rust
use crate::loc::frame::Frame;
use crate::loc::imm::cv_noise;
use crate::loc::mat::{add, identity, inverse, kalman_update, mul, mul_vec, scale, sub, transpose, Mat};
use crate::loc::ACC_TO_SIGMA;

/// The reference track of a real walk: an RTS (forward-backward) smoother of the constant-velocity model (`sigma_a`) over the fixes with
/// accuracy at most `max_acc_m`, forward and backward over the whole recording.
#[must_use]
pub fn rts_reference(fixes: &[RawFix], max_acc_m: f64, sigma_a: f64) -> Vec<TruthPoint> {
    let good: Vec<&RawFix> = fixes.iter().filter(|f| f.accuracy_m <= max_acc_m && f.accuracy_m.is_finite()).collect();
    let Some(first) = good.first() else { return vec![] };
    let frame = Frame::new(first.point());
    let h: Mat<2, 4> = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]];
    let sigma_of = |f: &RawFix| (f.accuracy_m / ACC_TO_SIGMA).max(2.0);
    let (s0, z0) = (sigma_of(first), frame.to_enu(first.point()));
    let mut x = [z0[0], z0[1], 0.0, 0.0];
    let mut p: Mat<4, 4> = [[s0 * s0, 0.0, 0.0, 0.0], [0.0, s0 * s0, 0.0, 0.0], [0.0, 0.0, 4.0, 0.0], [0.0, 0.0, 0.0, 4.0]];
    let (mut xs, mut ps, mut xp, mut pp, mut fs) = (vec![x], vec![p], vec![x], vec![p], vec![identity::<4>()]);
    for w in good.windows(2) {
        let (prev, f) = (w[0], w[1]);
        let (sigma, z) = (sigma_of(f), frame.to_enu(f.point()));
        let dt = i64_to_f64(f.t_ms - prev.t_ms) / 1000.0;
        let fm: Mat<4, 4> = [[1.0, 0.0, dt, 0.0], [0.0, 1.0, 0.0, dt], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        x = mul_vec(&fm, &x);
        p = add(&mul(&mul(&fm, &p), &transpose(&fm)), &cv_noise(sigma_a, dt));
        xp.push(x);
        pp.push(p);
        fs.push(fm);
        if let Some(u) = kalman_update(&x, &p, &z, &h, &scale(&identity::<2>(), sigma * sigma)) {
            x = u.x;
            p = u.p;
        }
        xs.push(x);
        ps.push(p);
    }
    for k in (0..xs.len().saturating_sub(1)).rev() {
        let Some((pp_inv, _)) = inverse(&pp[k + 1]) else { continue };
        let c = mul(&mul(&ps[k], &transpose(&fs[k + 1])), &pp_inv);
        let dx: [f64; 4] = std::array::from_fn(|i| xs[k + 1][i] - xp[k + 1][i]);
        let corr = mul_vec(&c, &dx);
        xs[k] = std::array::from_fn(|i| xs[k][i] + corr[i]);
        ps[k] = add(&ps[k], &mul(&mul(&c, &sub(&ps[k + 1], &pp[k + 1])), &transpose(&c)));
    }
    good.iter()
        .zip(&xs)
        .map(|(f, x)| TruthPoint { t_ms: f.t_ms, p: frame.to_geo([x[0], x[1]]), speed_mps: x[2].hypot(x[3]), course_deg: x[2].atan2(x[3]).to_degrees().rem_euclid(360.0) })
        .collect()
}
```

Re-export: `pub use reference::{good_fix_reference, rts_reference};`.

- [ ] **Step 5: Put the `Locator` into the replay** (`core/examples/replay.rs`)

Add to `Args`: `params: apgo_core::loc::LocParams` (default `LocParams::default()`) and `compare: bool`; parse
`"--params" => a.params = serde_json::from_str(&std::fs::read_to_string(it.next().expect("--params p.json")).unwrap()).expect("LocParams JSON"),`
and `"--compare" => a.compare = it.next().as_deref() == Some("baseline"),`. Replace the body of `main` after `let rec = load(&a);` with:

```rust
    let reference = apgo_core::loc::bench::rts_reference(&rec.fixes, 15.0, if matches!(a.mode, Mode::Bike | Mode::Drive) { 1.5 } else { 0.5 });
    let mut targets = targets(a.game.as_deref());
    targets.extend(apgo_core::loc::bench::virtual_targets(&reference, 200.0, 25.0));
    let opts = apgo_core::loc::bench::ReplayOpts { mode: a.mode, params: a.params.clone() };
    let t0 = std::time::Instant::now();
    let filtered = apgo_core::loc::bench::run_locator(&rec.fixes, &rec.steps, &rec.headings, &opts);
    let per_fix_us = t0.elapsed().as_secs_f64() * 1e6 / rec.fixes.len().max(1) as f64;
    let filter_card = score("filter", &reference, &filtered.shown, &targets);
    let mut legacy = LegacyRules::new();
    let legacy_shown: Vec<Shown> = rec.fixes.iter().map(|f| legacy.feed(f)).collect();
    let legacy_card = score("legacy", &reference, &legacy_shown, &targets);
    if a.compare {
        println!("{}", columns(&[&legacy_card, &filter_card]));
    } else {
        println!("{}", columns(&[&filter_card]));
    }
    println!("cost: {per_fix_us:.1} us per fix (host, release)");
    let cols: Vec<(&str, &[Shown])> = vec![("legacy", &legacy_shown), ("filter", &filtered.shown)];
```

(keep the GeoJSON and CSV writes after it; add `#![allow(clippy::cast_precision_loss)]` to the example's allow list for the per-fix cost.)

- [ ] **Step 6: Run the scenarios and the gate**

Run: `cd core && cargo test -p apgo-core --release --test loc_scenarios`
Expected: PASS on all 20 seeds of every scenario above. If one fails, change `LocParams` defaults (never a threshold, never a test),
keeping every value in the spec's stated range (for example `sigma_a_walk` 0.3 to 0.8, `beta` later 3 to 10 m), record the change and the
failing seed in the commit body, and re-run Tasks 5 to 7's unit tests. If no parameter in range can meet a threshold, stop and report
BLOCKED with the scenario, the seed and the metric to the owner.
Run: `cd core && cargo test -p apgo-core --release --test loc_scenarios timing -- --ignored --nocapture`
Expected: prints `on_fix: mean ... us, p99 ... us`; p99 well under 50 us on the host.
Run: `just check-rust`
Expected: PASS (debug-mode `cargo llvm-cov` runs the scenarios too; they must finish in the CI time budget: if the suite takes more than
about 60 s in debug, reduce the scenario length, never the seed count).

- [ ] **Step 7: Re-run the baseline scorecards with the filter column (local, not committed)**

Same commands as Task 4 Step 8 with `--compare baseline`, writing `scorecard-1007-filter.txt` and `scorecard-1008-filter.txt` to the
scratchpad. Expected: the filter column shows lower jitter and fewer false jumps than legacy on both walks.

- [ ] **Step 8: Commit**

```bash
git add core/src/loc core/examples/replay.rs core/tests/loc_scenarios.rs
git commit -m "test: run layer 1 scenarios in CI" -m "The spec's synthetic walks for the IMM (straight, corner, standing,
spike and burst, relocation, bike stops, BALANCED stress, walk-in,
mode mismatch) on 20 seeds, an RTS reference for real walks, and the
Locator column in the replay." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 9: `Game::on_estimate`: quests see only the estimate, the old rules go

**Files:**

- Modify: `core/src/game.rs` (imports; delete `enum Verdict`, `ODOMETER_MIN_STEP_M`; fields; `create`; `on_steps`; `set_counting`; `on_fix`;
  new `on_estimate`, `reveal`, `last_estimate`, `set_travel_mode`; `explain_near`; tests)
- Modify: `core/src/verify.rs` (delete `implied_speed_kmh`, `MAX_PLAUSIBLE_KMH`, `MAX_OUTLIER_STREAK` and their two tests;
  `MAX_ACCURACY_M` becomes `crate::loc::MAX_UNCERTAINTY_M`)
- Modify: `core/ffi/src/engine.rs` (`on_fix`, `on_steps` call sites only; the FFI signature changes in Task 10)
- Modify: `core/examples/play_sim.rs` (feeds simulated fixes)

**Interfaces:**

- Consumes: `Locator`, `Odometer`, `Estimate`, `RawFix`, `Provider`, `Source`, `Verdict`, `MAX_UNCERTAINTY_M` (Tasks 2, 7).
- Produces: `Game::on_fix(&mut self, raw: &RawFix, steps_total: Option<i64>) -> Vec<Event>`,
  `Game::on_estimate(&mut self, est: &Estimate, steps_total: Option<i64>) -> Vec<Event>`,
  `Game::on_steps(&mut self, total: i64, t_ms: i64, cadence: Option<f64>) -> Vec<Event>`, `Game::last_estimate(&self) -> Option<Estimate>`,
  `Game::set_travel_mode(&mut self, Mode)`, `Game::explain_near(&self, est: &Estimate, radius_m: f64) -> Vec<NearMiss>`.
- Plan choice (spec ambiguity): fog is updated by accepted estimates and by bridged ones; a gated, blurry or unusable estimate uncovers
  nothing (the spec row "Any accepted raw fix updates fog -> any estimate with source != Predicted" read together with "Bridged positions:
  display, fog and Cartographer squares only").

- [ ] **Step 1: Write the failing tests** (in `core/src/game.rs` `mod tests`)

Add the raw-fix helper next to `fixat` and change `fixat` to an exact estimate:

```rust
    fn fixat(p: Point, t_s: i64) -> Estimate {
        Estimate::exact(p.lat, p.lon, t_s * 1000)
    }

    fn raw(p: Point, t_s: i64, acc: f64) -> RawFix {
        RawFix::at(p.lat, p.lon, t_s * 1000, acc)
    }
```

New tests:

```rust
    #[test]
    fn a_gated_fix_never_completes_a_quest() {
        let (mut g, id, _) = start_near_a_quest();
        let target = g.assignments.iter().find(|a| a.location_id == id).and_then(|a| anchor(&a.target)).unwrap();
        let ev = g.on_fix(&raw(target, 1003, 5.0), None);
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Gated));
        assert!(ev.is_empty() && !g.done.contains(&id));
    }

    #[test]
    fn a_filter_reset_pauses_a_dwell_and_a_full_dwell_after_it_still_counts() {
        let mut g = chain_game("dwell", vec![Target::Dwell { p: home(), r: 50.0, minutes: 10.0 }]);
        for t in (0..=300).step_by(5) {
            g.on_fix(&raw(home(), t, 5.0), None);
        }
        let ev = g.on_fix(&raw(home(), 700, 5.0), None);
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Reset), "400 s without a fix");
        assert!(ev.is_empty() && g.done.is_empty(), "the timer starts again after a reset");
        let mut done = Vec::new();
        for t in (705..=1320).step_by(5) {
            done.extend(done_ids(&g.on_fix(&raw(home(), t, 5.0), None)));
        }
        assert_eq!(done, vec![1000]);
    }

    #[test]
    fn a_counting_toggle_restarts_the_filter() {
        let (mut g, _, p0) = start_near_a_quest();
        g.set_counting(false);
        g.set_counting(true);
        g.on_fix(&raw(destination(p0, 90.0, 3000.0), 1003, 5.0), None);
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Reset), "a fresh filter believes the first fix after a pause");
    }

    #[test]
    fn the_filter_is_not_saved_and_a_save_loads_without_it() {
        let (g, _, _) = start_near_a_quest();
        let v = serde_json::to_value(&g).unwrap();
        assert!(v.get("locator").is_none() && v.get("odometer").is_none() && v.get("last_est").is_none());
        let back: Game = serde_json::from_value(v).unwrap();
        assert!(back.last_estimate().is_none() && back.last_pos().is_none());
    }

    #[test]
    fn a_simulated_fix_teleports_and_completes_like_today() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        g.on_fix(&RawFix { provider: Provider::Sim, ..raw(destination(q.anchor.unwrap(), 0.0, 5000.0), 1, 5.0) }, None);
        let ev = g.on_fix(&RawFix { provider: Provider::Sim, ..raw(q.anchor.unwrap(), 2, 5.0) }, None);
        assert!(done_ids(&ev).contains(&q.location_id), "5 km in 1 s is fine for the simulator: {ev:?}");
    }
```

Add `use crate::loc::Provider;` to the test module imports if `super::*` does not bring it in.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core game::`
Expected: FAIL to compile: `cannot find type 'Estimate'`, `no method named 'last_estimate'`, `mismatched types: expected 'Fix', found '&RawFix'`.

- [ ] **Step 3: Implement in `core/src/game.rs`**

Imports: replace the `verify` line and add the `loc` one:

```rust
use crate::loc::{Estimate, Locator, Odometer, RawFix, Source, Verdict as LocVerdict, MAX_UNCERTAINTY_M};
use crate::verify::{collect_progress, Collected, Fix, Status, Tracker};
```

Delete `enum Verdict` (lines ~190-200) and `const ODOMETER_MIN_STEP_M` with its doc line. In `struct Game` delete the fields
`outlier_streak`, `last_verdict`, `last_speed`, `odo_anchor` and add (after `last_fix`):

```rust
    /// The location filter: raw fixes in, estimates out. Never saved (a restart starts fresh).
    #[serde(skip)]
    locator: Locator,
    /// Distance travelled, from accepted estimates.
    #[serde(skip)]
    odometer: Odometer,
    /// The newest estimate the filter made.
    #[serde(skip)]
    last_est: Option<Estimate>,
    /// The lower bound of the speed of the last accepted estimate, km/h.
    #[serde(skip)]
    last_speed_kmh: Option<f64>,
```

In `create` replace `outlier_streak: 0, last_verdict: Verdict::Used, last_speed: None, odo_anchor: None,` with
`locator: Locator::default(), odometer: Odometer::default(), last_est: None, last_speed_kmh: None,`.

Replace `on_steps`, `set_counting` and `on_fix` with:

```rust
    /// A step-counter reading outside a fix (the sensor reports on its own). While a GPS gap is bridged the bridged estimate uncovers fog
    /// and map squares, nothing else.
    pub fn on_steps(&mut self, total: i64, t_ms: i64, cadence: Option<f64>) -> Vec<Event> {
        let bridged = self.locator.on_steps(total, t_ms, cadence);
        if !self.counting {
            self.counters.steps_last = Some(total);
            return Vec::new();
        }
        self.credit_steps(total);
        let mut ev = bridged.map(|b| self.on_estimate(&b, None)).unwrap_or_default();
        ev.extend(self.complete_reached(t_ms, self.last_pos()));
        ev
    }

    /// Presence rules (at home, in the car) switch counting off: nothing is checked, credited or added while it is off.
    pub fn set_counting(&mut self, on: bool) {
        if self.counting == on {
            return;
        }
        self.counting = on;
        // Whatever the player did while it was off must not be compared with what they do next.
        self.last_fix = None;
        self.odometer.clear();
        self.locator.reset();
        if !on {
            // A dwell or away timer started before a pause must not finish on the first fix after it (progress is kept).
            self.trackers.values_mut().for_each(Tracker::pause);
        }
    }

    /// The travel mode at the player's position (the engine knows the zone shapes), for the filter's motion models.
    pub fn set_travel_mode(&mut self, mode: Mode) {
        self.locator.set_mode(mode);
    }

    /// The newest estimate the filter made (what the last fix became).
    #[must_use]
    pub fn last_estimate(&self) -> Option<Estimate> {
        self.last_est
    }

    /// Feed a raw GPS fix (and the cumulative step counter if the phone has one). The filter turns it into an estimate, which is all that
    /// quests, fog, chains and the odometer ever see.
    pub fn on_fix(&mut self, raw: &RawFix, steps_total: Option<i64>) -> Vec<Event> {
        if let Some(total) = steps_total {
            let _ = self.locator.on_steps(total, raw.t_ms, None);
        }
        let est = self.locator.on_fix(raw);
        self.last_est = Some(est);
        self.on_estimate(&est, steps_total)
    }

    /// Feed one position estimate. Only an accepted one checks quests, advances chains, adds distance or counts time away; a bridged one (steps
    /// in a GPS gap) only uncovers fog and map squares; a filter restart pauses dwell and away timers (progress is kept).
    pub fn on_estimate(&mut self, est: &Estimate, steps_total: Option<i64>) -> Vec<Event> {
        let mut ev = Vec::new();
        if !self.counting {
            if let Some(t) = steps_total {
                self.counters.steps_last = Some(t);
            }
            return ev;
        }
        if let Some(total) = steps_total {
            self.credit_steps(total);
        }
        self.last_est = Some(*est);
        if matches!(est.verdict, LocVerdict::Reset | LocVerdict::Relocated) {
            self.trackers.values_mut().for_each(Tracker::pause);
            self.last_fix = None;
        }
        if est.source == Source::Bridged {
            self.reveal(est.point(), &mut ev);
            return ev;
        }
        if !est.accepted {
            return ev;
        }
        let fix = Fix { lat: est.lat, lon: est.lon, t_ms: est.t_ms, accuracy_m: est.uncertainty_m };
        let pos = fix.point();
        // The lower bound of the speed, so GPS noise does not block a slow walker.
        let kmh = (est.speed_mps - 2.0 * est.speed_sigma_mps).max(0.0) * 3.6;
        self.last_speed_kmh = Some(kmh);
        let moved = self.odometer.step(est);
        self.stats.distance_m += moved;
        self.reveal(pos, &mut ev);
        for text in self.traps.tick(fix.t_ms, pos, moved) {
            ev.push(Event::Info { text });
        }
        let blocked = self.traps.blocks_checks(pos);
        if blocked != self.last_block {
            if let Some(b) = &blocked {
                ev.push(Event::Info { text: b.clone() });
            }
            self.last_block.clone_from(&blocked);
        }
        let in_chain: BTreeSet<i64> = self.assignments.iter().filter(|a| is_chain_target(&a.target)).map(|a| a.location_id).collect();
        let mut finished = Vec::new();
        if blocked.is_none() {
            let ids: Vec<(i64, Mode, u32)> = self.assignments.iter().map(|a| (a.location_id, a.mode, a.zone)).collect();
            for (id, mode, zone) in ids {
                if self.done.contains(&id) || !self.zone_unlocked(zone) || in_chain.contains(&id) {
                    continue;
                }
                if self.fog_on() && !self.fog.discovered.contains(&id) {
                    continue;
                }
                if !speed_ok(mode, kmh) {
                    continue;
                }
                if self.update_tracker(id, &fix, steps_total) == Some(Status::Done) {
                    finished.push(id);
                }
            }
        }
        for id in finished {
            ev.extend(self.complete(id, fix.t_ms, Some(pos)));
        }
        if let (Some(prev), None) = (self.last_fix, &blocked) {
            self.accrue_away(&prev, &fix);
        }
        ev.extend(self.complete_reached(fix.t_ms, Some(pos)));
        self.last_fix = Some(fix);
        ev
    }

    /// Uncover fog and map squares around `pos` (Cartographer chains count the new squares).
    fn reveal(&mut self, pos: Point, ev: &mut Vec<Event>) {
        let scout = self.count("Progressive Scouting Distance");
        let cells_before = self.fog.cells.len();
        for id in self.fog.update(pos, &self.assignments, reveal_radius(scout)) {
            if self.fog_on() {
                ev.push(Event::Discovered { location_id: id });
            }
        }
        self.credit_cells(self.fog.cells.len() - cells_before);
    }
```

Replace `explain_near`:

```rust
    /// For every open quest within `radius_m` of the estimate: the distance and why it would or would not count right now.
    #[must_use]
    pub fn explain_near(&self, est: &Estimate, radius_m: f64) -> Vec<NearMiss> {
        let here = est.point();
        let blocked = self.traps.blocks_checks(here);
        let kmh = (est.speed_mps - 2.0 * est.speed_sigma_mps).max(0.0) * 3.6;
        self.assignments
            .iter()
            .filter(|a| !self.done.contains(&a.location_id))
            .filter_map(|a| {
                let at = anchor(&a.target)?;
                let distance_m = distance_m(here, at);
                if distance_m > radius_m {
                    return None;
                }
                let reason = if est.source == Source::Bridged {
                    "position estimated from steps (GPS gap)".to_string()
                } else if est.verdict == LocVerdict::Blurry {
                    format!("GPS uncertain ({:.0} m, needs {MAX_UNCERTAINTY_M:.0} m)", est.uncertainty_m)
                } else if est.verdict == LocVerdict::Gated {
                    "ignored as a GPS jump".to_string()
                } else if est.verdict == LocVerdict::Unusable {
                    "GPS fix unusable (too coarse, stale or from a mock app)".to_string()
                } else if !self.zone_unlocked(a.zone) {
                    format!("zone {} is still locked", a.zone)
                } else if self.fog_on() && !self.fog.discovered.contains(&a.location_id) {
                    "not discovered yet (fog of war)".to_string()
                } else if let Some(b) = &blocked {
                    b.clone()
                } else if !speed_ok(a.mode, kmh) {
                    format!("moving too fast for {:?} ({kmh:.0} km/h)", a.mode)
                } else {
                    match reach_radius(&a.target) {
                        Some(r) if distance_m <= r => format!("in range ({distance_m:.0} m, needs {r:.0} m): counting"),
                        Some(r) => format!("{distance_m:.0} m away, needs {r:.0} m"),
                        None => format!("{distance_m:.0} m away"),
                    }
                };
                Some(NearMiss { location_id: a.location_id, name: a.quest_name.clone(), distance_m, reason })
            })
            .collect()
    }
```

- [ ] **Step 4: Move the existing game tests to estimates**

Most tests teleport between quests: they become exact estimates (their expectations do not change). Run, from the worktree:

```bash
perl -0pi -e 's/\.on_fix\(Fix \{ accuracy_m: [0-9.]+, \.\.fixat\(((?:[^()]|\((?:[^()]|\([^()]*\))*\))*)\) \}, /.on_estimate(&fixat($1), /g; s/\.on_fix\(fixat\(/.on_estimate(&fixat(/g; s/\bg\.on_steps\(([^()]*)\)/g.on_steps($1, None)/g' core/src/game.rs
```

Then replace these tests (they test the filter through the game, so they feed raw fixes) with the code below:
`start_near_a_quest`, `the_first_fix_after_resuming_is_not_judged_against_a_stale_one`, `distance_is_not_added_while_counting_is_off`,
`a_far_off_network_style_fix_is_dropped_even_on_the_target`, `walking_to_the_target_still_completes_after_an_outlier`,
`several_far_fixes_in_a_row_are_believed`, `fixes_worse_than_35_m_are_ignored`, `standing_still_with_gps_jitter_adds_no_distance`,
`a_long_gap_is_not_walked_distance`, `walking_adds_about_the_distance_walked`, `near_a_quest_the_reason_it_does_or_does_not_count_is_explained`,
`poor_accuracy_fixes_are_ignored`, `walking_quests_do_not_count_while_moving_at_vehicle_speed`:

```rust
    fn start_near_a_quest() -> (Game, i64, Point) {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        let p0 = destination(q.anchor.unwrap(), 0.0, 200.0);
        g.on_fix(&raw(p0, 1000, 5.0), None);
        (g, q.location_id, p0)
    }

    fn target_of(g: &Game, id: i64) -> Point {
        g.assignments.iter().find(|a| a.location_id == id).and_then(|a| anchor(&a.target)).unwrap()
    }

    #[test]
    fn the_first_fix_after_resuming_is_not_judged_against_a_stale_one() {
        let (mut g, id, p0) = start_near_a_quest();
        g.set_counting(false);
        g.set_counting(true);
        // 200 m from the last fix 3 s later would be gated if the old filter were kept
        let ev = g.on_fix(&raw(target_of(&g, id), 1003, 5.0), None);
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { location_id, .. } if *location_id == id)), "{ev:?} from {p0:?}");
    }

    #[test]
    fn distance_is_not_added_while_counting_is_off() {
        let (mut g, _, p0) = start_near_a_quest();
        let before = g.stats.distance_m;
        g.set_counting(false);
        for i in 1..=10 {
            g.on_fix(&raw(destination(p0, 90.0, 30.0 * f64::from(i)), 1000 + i64::from(i) * 10, 5.0), None);
        }
        assert_eq!(g.stats.distance_m, before);
    }

    #[test]
    fn a_far_off_network_style_fix_is_dropped_even_on_the_target() {
        let (mut g, id, p0) = start_near_a_quest();
        let ev = g.on_fix(&raw(target_of(&g, id), 1003, 5.0), None);
        assert!(ev.is_empty() && !g.done.contains(&id), "a 200 m jump in 3 s is gated and must not complete the quest");
        assert_eq!(g.last_pos(), Some(p0), "the last good position is kept");
    }

    #[test]
    fn walking_to_the_target_still_completes_after_an_outlier() {
        let (mut g, id, _) = start_near_a_quest();
        let target = target_of(&g, id);
        g.on_fix(&raw(target, 1003, 5.0), None); // gated
        let ev = g.on_fix(&raw(target, 1100, 5.0), None); // 200 m in 100 s: a brisk walk
        assert!(ev.iter().any(|e| matches!(e, Event::QuestDone { location_id, .. } if *location_id == id)));
    }

    #[test]
    fn several_far_fixes_in_a_row_are_believed() {
        let (mut g, _, p0) = start_near_a_quest();
        let far = destination(p0, 90.0, 5000.0);
        for (i, t) in [1003, 1006, 1009].into_iter().enumerate() {
            g.on_fix(&raw(far, t, 5.0), None);
            assert_eq!(g.last_pos() == Some(far), i == 2, "believed only on the third in a row (step {i})");
        }
    }

    #[test]
    fn fixes_worse_than_35_m_are_ignored() {
        let (mut g, id, _) = start_near_a_quest();
        let ev = g.on_fix(&raw(target_of(&g, id), 2000, 40.0), None);
        assert!(ev.is_empty() && !g.done.contains(&id));
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Blurry));
    }

    #[test]
    fn standing_still_with_gps_jitter_adds_no_distance() {
        let (mut g, _, p0) = start_near_a_quest();
        for i in 0..60 {
            let wobble = destination(p0, (i * 97 % 360) as f64, 3.0 + (i % 3) as f64);
            g.on_fix(&raw(wobble, 1005 + i * 5, 5.0), None);
        }
        assert!(g.stats.distance_m < 12.0, "jitter counted as {} m", g.stats.distance_m);
    }

    #[test]
    fn a_long_gap_is_not_walked_distance() {
        let (mut g, _, p0) = start_near_a_quest();
        g.on_fix(&raw(destination(p0, 90.0, 20_000.0), 1000 + 3600, 5.0), None);
        assert!(g.stats.distance_m < 1.0, "an hour later 20 km away is a relocation, counted {}", g.stats.distance_m);
    }

    #[test]
    fn walking_adds_about_the_distance_walked() {
        let (mut g, _, p0) = start_near_a_quest();
        for i in 1..=40 {
            g.on_fix(&raw(destination(p0, 90.0, 7.0 * i as f64), 1000 + i * 5, 5.0), None);
        }
        let d = g.stats.distance_m;
        assert!((d - 280.0).abs() < 28.0, "walked 280 m, counted {d}");
    }

    #[test]
    fn near_a_quest_the_reason_it_does_or_does_not_count_is_explained() {
        let (g, id, p0) = start_near_a_quest();
        let target = target_of(&g, id);
        let near = |e: Estimate| g.explain_near(&e, 100.0).into_iter().find(|n| n.location_id == id);
        assert!(near(fixat(p0, 1000)).is_none(), "200 m away is not near");
        let nm = near(fixat(destination(target, 0.0, 70.0), 1000)).expect("70 m away is near");
        assert!((nm.distance_m - 70.0).abs() < 2.0 && nm.reason.contains("needs 40"), "{nm:?}");
        // The estimate's verdict decides the reason.
        let gated = Estimate { verdict: LocVerdict::Gated, accepted: false, ..fixat(target, 1003) };
        assert!(near(gated).unwrap().reason.contains("jump"));
        let blurry = Estimate { verdict: LocVerdict::Blurry, accepted: false, uncertainty_m: 60.0, ..fixat(target, 2000) };
        assert!(near(blurry).unwrap().reason.contains("GPS uncertain (60 m, needs 35 m)"));
        let bridged = Estimate { source: Source::Bridged, accepted: false, ..fixat(target, 2001) };
        assert!(near(bridged).unwrap().reason.contains("estimated from steps"));
    }

    #[test]
    fn poor_accuracy_fixes_are_ignored() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        assert!(g.on_fix(&raw(q.anchor.unwrap(), 5, 300.0), None).is_empty());
        assert_eq!(g.last_estimate().map(|e| e.verdict), Some(LocVerdict::Unusable));
    }

    #[test]
    fn walking_quests_do_not_count_while_moving_at_vehicle_speed() {
        let mut g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        let q = g.quest_views().remove(0);
        let ev = g.on_estimate(&Estimate { speed_mps: 100.0, ..fixat(q.anchor.unwrap(), 10) }, None); // 360 km/h
        assert!(!ev.iter().any(|e| matches!(e, Event::QuestDone { .. })));
    }
```

Check that nothing else still calls the old API: `grep -n "on_fix(" core/src/game.rs` lists only `on_fix(&raw(` lines and the
`RawFix { provider: Provider::Sim` ones.

- [ ] **Step 5: `verify.rs`, the FFI call sites and `play_sim`**

In `core/src/verify.rs` delete `implied_speed_kmh`, `MAX_PLAUSIBLE_KMH`, `MAX_OUTLIER_STREAK` and the tests
`implied_speed_discounts_the_error_radii` and `implied_speed_needs_a_sensible_gap` (their behaviour lives in `bench::LegacyRules` and its
tests), and change the constant to:

```rust
/// Fixes (now estimates) less sure than this are ignored by a tracker; the location filter applies the same limit first.
pub const MAX_ACCURACY_M: f64 = crate::loc::MAX_UNCERTAINTY_M;
```

In `core/ffi/src/engine.rs` (signature unchanged until Task 10), `on_fix` builds a raw fix and explains near misses from the estimate:

```rust
        let raw = RawFix { provider: if simulated { Provider::Sim } else { Provider::Fused }, ..RawFix::at(lat, lon, t_ms, accuracy_m) };
        ...
                let ev = g.on_fix(&raw, steps);
                self.save_if_due(g, t_ms, !ev.is_empty());
                let est = g.last_estimate().unwrap_or_default();
                (g.id.clone(), g.journal_events(&ev, t_ms, at), g.explain_near(&est, NEAR_MISS_RADIUS_M), ev)
```

(drop the `let fix = Fix { .. }` line and the `Fix` import; add `use apgo_core::loc::{Provider, RawFix};`) and `on_steps` calls
`g.on_steps(total, t_ms, None)`.

In `core/examples/play_sim.rs` the loop becomes
`for e in g.on_fix(&RawFix { provider: Provider::Sim, ..RawFix::at(p.lat, p.lon, ts, 5.0) }, st) {` with
`use apgo_core::loc::{Provider, RawFix};` instead of `use apgo_core::verify::Fix;`.

- [ ] **Step 6: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core`
Expected: PASS (every game test, the five new ones, all loc tests and scenarios).
Run: `just check-rust`
Expected: PASS, coverage still >= 84 (the deleted rules had tests; the new ones replace them).

- [ ] **Step 7: Commit**

```bash
git add core/src/game.rs core/src/verify.rs core/ffi/src/engine.rs core/examples/play_sim.rs
git commit -m "feat: let quests use the filtered estimate" -m "Game::on_fix runs the Locator and on_estimate holds the old body:
only accepted estimates check quests, the odometer sums estimate steps,
speed checks use the lower speed bound, a reset pauses timers. The
accuracy, jump and outlier rules are gone." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 10: FFI `FixIn`, a journal of estimates and the Kotlin callers

**Files:**

- Modify: `core/ffi/src/engine.rs` (`FixIn`, `raw_fix`, `journal_due`, `rejected_detail`, fields `zone_modes` and `last_journal`, `install`,
  `close_game`, `delete_game`, `on_fix`, `on_steps`; tests)
- Modify: `android/app/src/main/java/dev/apgo2/RawTrack.kt` (`FixSample.toFixIn`), `AppModel.kt` (`onFix`, `onSteps`), `DevSimulator.kt` (`fix`)
- Test: `core/ffi/src/engine.rs` tests, `android/app/src/test/java/dev/apgo2/RawTrackTest.kt`

**Interfaces:**

- Consumes: `Game::on_fix(&RawFix, ..)`, `last_estimate`, `set_travel_mode`, `explain_near(&Estimate, ..)`, `on_steps(.., cadence)` (Task 9);
  `loc::mode_at` (Task 2).
- Produces (FFI, Kotlin names in brackets):
  - `FixIn { t_ms, lat, lon, accuracy_m, speed_mps, speed_acc_mps, bearing_deg, bearing_acc_deg, altitude_m, vertical_acc_m, provider: String, mock }`
    [`FixIn(tMs, lat, lon, accuracyM, speedMps, speedAccMps, bearingDeg, bearingAccDeg, altitudeM, verticalAccM, provider, mock)`].
  - `Engine::on_fix(fix: FixIn, steps: Option<i64>, simulated: bool) -> Vec<EventOut>` [`onFix(fix, steps, simulated)`].
  - `Engine::on_steps(total: i64, t_ms: i64, cadence: Option<f64>) -> Vec<EventOut>` [`onSteps(total, tMs, cadence)`].
  - Kotlin `FixSample.toFixIn(mockAllowed: Boolean = false): FixIn`.
- Journal: `add_point` gets accepted estimates only, throttled to 5 s or 5 m; `fix_rejected` (1 per minute) for Blurry, Gated and Unusable.
- Plan choice: `Engine::set_record_raw` from the spec is not added: the spec says the core does nothing with it, and raw recording is
  decided in Kotlin by the build type (listed under spec gaps).

- [ ] **Step 1: Write the failing tests**

In `core/ffi/src/engine.rs` `mod tests`:

```rust
    fn fix_in(provider: &str) -> FixIn {
        FixIn {
            t_ms: 5_000,
            lat: 40.0,
            lon: -111.0,
            accuracy_m: 6.0,
            speed_mps: None,
            speed_acc_mps: None,
            bearing_deg: None,
            bearing_acc_deg: None,
            altitude_m: None,
            vertical_acc_m: None,
            provider: provider.into(),
            mock: false,
        }
    }

    #[test]
    fn a_fix_with_every_option_missing_is_a_plain_fix_and_simulated_ones_are_sim() {
        let r = raw_fix(&fix_in("gps"), false);
        assert_eq!((r.t_ms, r.provider, r.speed_mps, r.bearing_acc_deg), (5_000, Provider::Gps, None, None));
        assert_eq!(raw_fix(&fix_in("fused"), true).provider, Provider::Sim);
        assert_eq!(raw_fix(&fix_in("weird"), false).provider, Provider::Other);
    }

    #[test]
    fn journal_points_are_throttled_to_five_seconds_or_five_metres() {
        let p = |t_ms: i64, east_m: f64| {
            let q = apgo_core::geo::destination(Point::new(40.0, -111.0), 90.0, east_m);
            TrackPoint { t_ms, lat: q.lat, lon: q.lon, accuracy_m: 4.0, simulated: false }
        };
        assert!(journal_due(None, &p(0, 0.0)));
        assert!(!journal_due(Some(&p(0, 0.0)), &p(4_000, 3.0)));
        assert!(journal_due(Some(&p(0, 0.0)), &p(5_000, 0.0)));
        assert!(journal_due(Some(&p(0, 0.0)), &p(1_000, 6.0)));
    }

    #[test]
    fn rejected_fixes_are_described_by_verdict() {
        use apgo_core::loc::{Estimate, Verdict};
        let e = |verdict| Estimate { verdict, accepted: false, uncertainty_m: 41.0, ..Estimate::exact(40.0, -111.0, 0) };
        assert_eq!(rejected_detail(&e(Verdict::Blurry)).as_deref(), Some("uncertain 41 m"));
        assert_eq!(rejected_detail(&e(Verdict::Gated)).as_deref(), Some("ignored as a GPS jump"));
        assert_eq!(rejected_detail(&e(Verdict::Unusable)).as_deref(), Some("unusable fix"));
        assert_eq!(rejected_detail(&Estimate::exact(40.0, -111.0, 0)), None);
    }

    #[test]
    fn a_fix_without_a_game_does_nothing() {
        let dir = std::env::temp_dir().join(format!("apgo-ffi-fix-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        assert!(e.on_fix(fix_in("gps"), None, false).is_empty());
        assert!(e.on_steps(10, 1, Some(1.8)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
```

In `RawTrackTest.kt`:

```kotlin
    @Test fun aSampleBecomesTheCoreFixAndAMockIsKeptUnlessAllowed() {
        val f = sample().copy(mock = true).toFixIn()
        assertEquals(1_000L, f.tMs)
        assertEquals(4.5, f.accuracyM, 1e-6)
        assertEquals(1.25, f.speedMps!!, 1e-6)
        assertEquals("fused", f.provider)
        assertEquals(true, f.mock)
        assertEquals(false, sample().copy(mock = true).toFixIn(mockAllowed = true).mock)
    }

    @Test fun aSampleWithoutAccuracyIsUnusablyCoarse() {
        assertEquals(1_000.0, sample().copy(accuracyM = null).toFixIn().accuracyM, 0.0)
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-ffi`
Expected: FAIL to compile: `cannot find struct 'FixIn'`, `cannot find function 'raw_fix'`, `this method takes 6 arguments but 3 were supplied`.

- [ ] **Step 3: Implement the FFI** (`core/ffi/src/engine.rs`)

```rust
/// A position fix from the phone, every field the filter can use (`None` = not reported; iOS sends `None` for its negative "unknown").
#[derive(Debug, Clone, uniffi::Record)]
pub struct FixIn {
    /// When the fix was taken, Unix ms (the fix's own clock).
    pub t_ms: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees.
    pub lon: f64,
    /// 68 % horizontal radius, metres.
    pub accuracy_m: f64,
    /// Ground speed, m/s.
    pub speed_mps: Option<f64>,
    /// 68 % speed accuracy, m/s.
    pub speed_acc_mps: Option<f64>,
    /// Course, degrees from north.
    pub bearing_deg: Option<f64>,
    /// 68 % course accuracy, degrees.
    pub bearing_acc_deg: Option<f64>,
    /// Altitude, metres.
    pub altitude_m: Option<f64>,
    /// 68 % vertical accuracy, metres.
    pub vertical_acc_m: Option<f64>,
    /// `fused`, `gps`, `network`, `ios`; anything else is "other".
    pub provider: String,
    /// Made by a mock-location app (and not allowed by the debug bench setting).
    pub mock: bool,
}

fn raw_fix(f: &FixIn, simulated: bool) -> RawFix {
    RawFix {
        t_ms: f.t_ms,
        lat: f.lat,
        lon: f.lon,
        accuracy_m: f.accuracy_m,
        speed_mps: f.speed_mps,
        speed_acc_mps: f.speed_acc_mps,
        bearing_deg: f.bearing_deg,
        bearing_acc_deg: f.bearing_acc_deg,
        altitude_m: f.altitude_m,
        vertical_acc_m: f.vertical_acc_m,
        provider: if simulated { Provider::Sim } else { Provider::parse(&f.provider) },
        mock: f.mock,
    }
}

/// Whether an accepted estimate goes into the journal: 5 s or 5 m after the last point written.
fn journal_due(last: Option<&TrackPoint>, p: &TrackPoint) -> bool {
    last.is_none_or(|l| p.t_ms - l.t_ms >= 5_000 || distance_m(Point::new(l.lat, l.lon), Point::new(p.lat, p.lon)) >= 5.0)
}

/// The `fix_rejected` line for an estimate that may not count, if it is one of those.
fn rejected_detail(e: &Estimate) -> Option<String> {
    match e.verdict {
        Verdict::Blurry => Some(format!("uncertain {:.0} m", e.uncertainty_m)),
        Verdict::Gated => Some("ignored as a GPS jump".into()),
        Verdict::Unusable => Some("unusable fix".into()),
        _ => None,
    }
}
```

`Engine` gets two fields (initialised in `new` as `Mutex::new(Vec::new())` and `Mutex::new(None)`):

```rust
    /// Shape and travel mode of each zone of the open game, for the filter's mode at the player's position.
    zone_modes: Mutex<Vec<(Shape, Mode)>>,
    /// The last journal point written, for the 5 s / 5 m throttle.
    last_journal: Mutex<Option<TrackPoint>>,
```

In `install`, after the `shapes` line:

```rust
        let modes: Vec<(Shape, Mode)> = game.slot.zones.iter().zip(&game.zone_realms).filter_map(|(z, id)| store.get(id).map(|r| (r.shape, z.mode))).collect();
        *self.zone_modes.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = modes;
        *self.last_journal.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
```

and clear `zone_modes` wherever `zone_shapes` is cleared (`close_game`, `delete_game`). Replace `on_fix` and `on_steps`:

```rust
    /// Feed a position fix (and the step counter, if any) to the open game; returns what happened.
    pub fn on_fix(&self, fix: FixIn, steps: Option<i64>, simulated: bool) -> Vec<EventOut> {
        let raw = raw_fix(&fix, simulated);
        let t_ms = raw.t_ms;
        let inside = self.zone_distance_m(raw.point()).is_none_or(|d| d == 0.0);
        let mode = mode_at(&self.zone_modes.lock().unwrap_or_else(std::sync::PoisonError::into_inner), raw.point());
        let Some((game_id, ev, entries, near, est)) = self.with_game(|g| {
            if let Some(m) = mode {
                g.set_travel_mode(m);
            }
            g.set_in_zone(inside);
            let ev = g.on_fix(&raw, steps);
            let est = g.last_estimate().unwrap_or_default();
            self.save_if_due(g, t_ms, !ev.is_empty());
            let entries = g.journal_events(&ev, t_ms, Some((est.lat, est.lon)));
            (g.id.clone(), ev, entries, g.explain_near(&est, NEAR_MISS_RADIUS_M), est)
        }) else {
            return Vec::new();
        };
        let at = Some((est.lat, est.lon));
        let point = est.accepted.then_some(TrackPoint { t_ms, lat: est.lat, lon: est.lon, accuracy_m: est.uncertainty_m, simulated }).filter(|p| {
            let mut last = self.last_journal.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let due = journal_due(last.as_ref(), p);
            if due {
                *last = Some(p.clone());
            }
            due
        });
        let rejected = rejected_detail(&est).filter(|_| {
            let last_ms = self.last_reject_log_ms.load(std::sync::atomic::Ordering::Relaxed);
            let due = t_ms.saturating_sub(last_ms) >= 60_000;
            if due {
                self.last_reject_log_ms.store(t_ms, std::sync::atomic::Ordering::Relaxed);
            }
            due
        });
        self.journal_do(|j| {
            if let Some(p) = &point {
                j.add_point(&game_id, p)?;
            }
            if let Some(detail) = rejected {
                j.log(&game_id, &JournalEvent { t_ms, kind: kind::FIX_REJECTED.into(), detail, at })?;
            }
            for n in self.new_near_misses(near, t_ms) {
                j.log(&game_id, &JournalEvent { t_ms, kind: kind::NEAR_MISS.into(), detail: format!("{}: {} ({:.0} m)", n.name, n.reason, n.distance_m), at })?;
            }
            entries.iter().try_for_each(|e| j.log(&game_id, e))
        });
        ev.into_iter().map(ev_out).collect()
    }
```

```rust
    /// A step-counter reading from the phone (cumulative since boot) at the sensor event's time, with the cadence if the phone knows it.
    pub fn on_steps(&self, total: i64, t_ms: i64, cadence: Option<f64>) -> Vec<EventOut> {
        let Some((game_id, ev, entries)) = self.with_game(|g| {
            let ev = g.on_steps(total, t_ms, cadence);
            self.save_if_due(g, t_ms, !ev.is_empty());
            let entries = g.journal_events(&ev, t_ms, None);
            (g.id.clone(), ev, entries)
        }) else {
            return Vec::new();
        };
        self.journal_do(|j| entries.iter().try_for_each(|e| j.log(&game_id, e)));
        ev.into_iter().map(ev_out).collect()
    }
```

Imports: `use apgo_core::loc::{mode_at, Estimate, Provider, RawFix, Verdict};`, remove `MAX_ACCURACY_M` (and `Fix` if unused). If
`significant_drop_tightening` flags the guard in the `filter` closure, move the lock into a small `fn throttle_point(&self, p) -> bool`.

- [ ] **Step 4: Kotlin callers**

`RawTrack.kt`:

```kotlin
// A fix without an accuracy is unusable in the core (anything over 100 m is).
private const val NO_ACCURACY_M = 1_000.0

/** The fix as the core takes it. A mock fix stays a mock (the core drops it) unless the debug bench allows mocks. */
internal fun FixSample.toFixIn(mockAllowed: Boolean = false): FixIn =
    FixIn(
        tMs = tMs,
        lat = lat,
        lon = lon,
        accuracyM = accuracyM?.toDouble() ?: NO_ACCURACY_M,
        speedMps = speedMps?.toDouble(),
        speedAccMps = speedAccMps?.toDouble(),
        bearingDeg = bearingDeg?.toDouble(),
        bearingAccDeg = bearingAccDeg?.toDouble(),
        altitudeM = altitudeM,
        verticalAccM = verticalAccM?.toDouble(),
        provider = provider,
        mock = mock && !mockAllowed,
    )
```

(import `uniffi.apgo_ffi.FixIn`). In `AppModel.onFix` the engine call becomes `handle(engine.onFix(sample.toFixIn(), stepsTotal, false))`
and in `onSteps` `engine.onSteps(total, eventMs, null)`. In `DevSimulator.fix`:

```kotlin
        val f =
            FixIn(
                tMs = tick(advanceMs),
                lat = p.lat,
                lon = p.lon,
                accuracyM = SIM_ACCURACY_M,
                speedMps = null,
                speedAccMps = null,
                bearingDeg = null,
                bearingAccDeg = null,
                altitudeM = null,
                verticalAccM = null,
                provider = "sim",
                mock = false,
            )
        return model.engine.onFix(f, if (withSteps) steps else null, true)
```

- [ ] **Step 5: Run the tests and both gates**

Run: `cd core && cargo test -p apgo-ffi`
Expected: PASS.
Run: `just check-rust && ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS (the bindings are regenerated by `scripts/android_bindings.sh` inside `check-android`).

- [ ] **Step 6: Commit**

```bash
git add core/ffi/src/engine.rs android/app/src/main/java/dev/apgo2/RawTrack.kt android/app/src/main/java/dev/apgo2/AppModel.kt \
  android/app/src/main/java/dev/apgo2/DevSimulator.kt android/app/src/test/java/dev/apgo2/RawTrackTest.kt
git commit -m "feat: pass full fixes and journal estimates" -m "FixIn carries every Location field to the core; the zone mode picks the
filter models; the journal stores accepted estimates (5 s or 5 m) and
logs gated, blurry and unusable fixes once a minute." -m "Closes #84
Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 11: The best request on every phone: rate by screen, providers with and without Play services, mock

> Superseded in part by Task 20c (2026-10-09): with Play services, fixes come from `FusedLocationProviderClient` on every Android
> version and detection is `GoogleApiAvailability` `SUCCESS` plus the package (`GpsPolicy.providers(enabled, gms)`). The code below is
> the Task 11 state.

**Files:**

- Modify: `android/app/src/main/java/dev/apgo2/GpsPolicy.kt` (rewrite), `Sensors.kt` (`startLocation`, `request`, provider plan, cold start, GMS check),
  `PresenceController.kt` (`screenOn`, `watchScreen`, `applyLocation`), `AppModel.kt` (`onFix` mock flag, init), `RawTrack.kt` (`MockPolicy`)
- Modify: `android/app/src/main/AndroidManifest.xml` (`<queries>`)
- Test: `android/app/src/test/java/dev/apgo2/GpsPolicyTest.kt` (rewrite), `RawTrackTest.kt`

**Interfaces:**

- Produces: `GpsPolicy.Rate(intervalMs, minDistanceM, highAccuracy = false, maxDelayMs = 0L)`, `GpsPolicy.inZone(screenOn): Rate`,
  `GpsPolicy.IDLE`, `GpsPolicy.forDecision(d, appVisible, screenOn): Rate?`, `GpsPolicy.ProviderPlan(main: String?, coldStart: String?)`,
  `GpsPolicy.providers(enabled, sdk, gms): ProviderPlan`, `GpsPolicy.RequestSpec(intervalMs, minIntervalMs, maxDelayMs, minDistanceM, quality)`,
  `GpsPolicy.request(rate): RequestSpec`, `MockPolicy.allowed(debuggable: Boolean, flagFileExists: Boolean): Boolean`,
  `PresenceController.screenOn`, `PresenceController.watchScreen(ctx)`.
- Plan choices: the debug "allow mock fixes" setting is a flag file (`adb shell run-as dev.apgo2.app touch files/allow_mock`), read
  only in debuggable builds (no settings UI for a bench-only switch). The quality constants are copied (`100`, `102`) so the pure policy
  carries no API 31 reference (lint `InlinedApi` is an error here).

- [ ] **Step 1: Write the failing tests** (replace `GpsPolicyTest.kt`)

```kotlin
package dev.apgo2

import android.os.Build
import dev.apgo2.presence.Decision
import dev.apgo2.presence.GpsMode
import dev.apgo2.presence.PresenceState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GpsPolicyTest {
    private val inZone = Decision(PresenceState.InZone, GpsMode.Rate(5_000L, 0f), counting = true)

    @Test fun inAZoneTheScreenPicksOneOrFiveSecondsAndBothUseGpsQuality() {
        for (visible in listOf(true, false)) {
            assertEquals("on, visible=$visible", GpsPolicy.Rate(1_000L, 0f, highAccuracy = true), GpsPolicy.forDecision(inZone, visible, screenOn = true))
            assertEquals(
                "off, visible=$visible",
                GpsPolicy.Rate(5_000L, 0f, highAccuracy = true, maxDelayMs = 10_000L),
                GpsPolicy.forDecision(inZone, visible, screenOn = false),
            )
        }
    }

    @Test fun insideAZoneTheChipNeverDropsToBalanced() {
        for (visible in listOf(true, false)) for (screen in listOf(true, false)) {
            assertTrue(GpsPolicy.forDecision(inZone, visible, screen)!!.highAccuracy)
        }
    }

    @Test fun outsideEveryZoneTheCoarseRateIgnoresTheScreen() {
        val d = Decision(PresenceState.OutsideZones, GpsMode.Rate(90_000L, 0f), counting = true)
        for (screen in listOf(true, false)) {
            assertEquals(GpsPolicy.Rate(90_000L, 0f, highAccuracy = false), GpsPolicy.forDecision(d, appVisible = false, screenOn = screen))
        }
    }

    @Test fun stoppedUsesTheIdleRuleOnlyWhileTheAppIsVisible() {
        val d = Decision(PresenceState.Stopped, GpsMode.Off, counting = false)
        assertEquals(GpsPolicy.IDLE, GpsPolicy.forDecision(d, appVisible = true, screenOn = true))
        assertNull(GpsPolicy.forDecision(d, appVisible = false, screenOn = true))
    }

    @Test fun offMeansNoLocationEvenWhenTheAppIsOnScreen() {
        assertNull(GpsPolicy.forDecision(Decision(PresenceState.AtHome, GpsMode.Off, counting = false), appVisible = true, screenOn = true))
        assertNull(GpsPolicy.forDecision(Decision(PresenceState.InCar, GpsMode.Off, counting = false), appVisible = true, screenOn = true))
    }

    @Test fun playingStaysFarUnderTheCoreGapLimit() {
        // The core resets the filter after 5 minutes without a fix: even screen-off batches must be far below that.
        val off = GpsPolicy.inZone(screenOn = false)
        assertTrue(off.intervalMs + off.maxDelayMs < 60_000L)
    }

    @Test fun theRequestCarriesIntervalBatchingAndQuality() {
        assertEquals(GpsPolicy.RequestSpec(1_000L, 1_000L, 0L, 0f, 100), GpsPolicy.request(GpsPolicy.inZone(screenOn = true)))
        assertEquals(GpsPolicy.RequestSpec(5_000L, 5_000L, 10_000L, 0f, 100), GpsPolicy.request(GpsPolicy.inZone(screenOn = false)))
        assertEquals(102, GpsPolicy.request(GpsPolicy.IDLE).quality)
    }
}

class GpsProvidersTest {
    private val all = setOf("fused", "gps", "network")

    @Test fun withPlayServicesAndroid12UsesFusedAlone() {
        assertEquals(GpsPolicy.ProviderPlan("fused", null), GpsPolicy.providers(all, Build.VERSION_CODES.S, gms = true))
    }

    @Test fun withoutPlayServicesTheGpsProviderIsUsedAndNetworkOnlyForTheColdStart() {
        // AOSP's fused provider only picks between gps and network: our filter does the fusion.
        assertEquals(GpsPolicy.ProviderPlan("gps", "network"), GpsPolicy.providers(all, Build.VERSION_CODES.S, gms = false))
        assertEquals(GpsPolicy.ProviderPlan("gps", null), GpsPolicy.providers(setOf("fused", "gps"), Build.VERSION_CODES.S, gms = false))
    }

    @Test fun beforeAndroid12TheGpsProviderIsUsed() {
        assertEquals(GpsPolicy.ProviderPlan("gps", null), GpsPolicy.providers(all, Build.VERSION_CODES.R, gms = true))
    }

    @Test fun networkIsTheLastResortAndNothingEnabledMeansNothing() {
        assertEquals(GpsPolicy.ProviderPlan("network", null), GpsPolicy.providers(setOf("network"), Build.VERSION_CODES.S, gms = true))
        assertEquals(GpsPolicy.ProviderPlan(null, null), GpsPolicy.providers(emptySet(), Build.VERSION_CODES.S, gms = true))
    }
}
```

Append to `RawTrackTest.kt`:

```kotlin
    @Test fun mockFixesAreOnlyAllowedInADebugBuildWithTheFlag() {
        assertEquals(false, MockPolicy.allowed(debuggable = false, flagFileExists = true))
        assertEquals(false, MockPolicy.allowed(debuggable = true, flagFileExists = false))
        assertEquals(true, MockPolicy.allowed(debuggable = true, flagFileExists = true))
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.Gps*' --tests 'dev.apgo2.RawTrackTest' --console=plain -q`
Expected: FAIL to compile: `No value passed for parameter 'screenOn'`, `Unresolved reference 'inZone'`, `Unresolved reference 'ProviderPlan'`,
`Unresolved reference 'MockPolicy'`.

- [ ] **Step 3: Rewrite `GpsPolicy.kt`**

```kotlin
package dev.apgo2

import android.os.Build

/** How the phone is asked for location: rate, batching and quality from the presence decision and the screen; which provider. */
internal object GpsPolicy {
    /**
     * [highAccuracy] asks for the GNSS chip (a fused request without it is served as BALANCED: Wi-Fi and cell, 20-100 m off).
     * [maxDelayMs] lets the phone batch fixes (screen off) to save battery.
     */
    data class Rate(
        val intervalMs: Long,
        val minDistanceM: Float,
        val highAccuracy: Boolean = false,
        val maxDelayMs: Long = 0L,
    )

    /** What a request is built from. [quality] is a `LocationRequest.QUALITY_*` value. */
    data class RequestSpec(
        val intervalMs: Long,
        val minIntervalMs: Long,
        val maxDelayMs: Long,
        val minDistanceM: Float,
        val quality: Int,
    )

    /** The provider to listen to, and one to ask once for a cold-start fix (no Play services: network, then never again). */
    data class ProviderPlan(
        val main: String?,
        val coldStart: String?,
    )

    // LocationRequest.QUALITY_HIGH_ACCURACY and QUALITY_BALANCED_POWER_ACCURACY (Android 12), copied so this policy has no API 31 reference.
    private const val QUALITY_HIGH_ACCURACY = 100
    private const val QUALITY_BALANCED = 102
    private const val SCREEN_ON_MS = 1_000L
    private const val SCREEN_OFF_MS = 5_000L
    private const val SCREEN_OFF_BATCH_MS = 10_000L

    /** Not playing, app on screen: a relaxed rate for the map marker (time-based, plus a distance filter). */
    val IDLE = Rate(intervalMs = 15_000L, minDistanceM = 20f)

    /** In a zone while playing: every second with the screen on, every 5 s (batched up to 10 s) with it off; GNSS quality in both. */
    fun inZone(screenOn: Boolean): Rate =
        if (screenOn) {
            Rate(SCREEN_ON_MS, 0f, highAccuracy = true)
        } else {
            Rate(SCREEN_OFF_MS, 0f, highAccuracy = true, maxDelayMs = SCREEN_OFF_BATCH_MS)
        }

    /** The rate a presence decision asks for; `null` means location is off. Stopped keeps "map marker while the app is on screen". */
    fun forDecision(
        d: dev.apgo2.presence.Decision,
        appVisible: Boolean,
        screenOn: Boolean,
    ): Rate? =
        when {
            d.state == dev.apgo2.presence.PresenceState.Stopped -> if (appVisible) IDLE else null
            d.state == dev.apgo2.presence.PresenceState.InZone -> inZone(screenOn)
            d.gps is dev.apgo2.presence.GpsMode.Rate -> Rate(d.gps.intervalMs, d.gps.minDistanceM)
            else -> null
        }

    /** The request for [rate]: the minimum interval equals the interval, so the phone never floods the filter. */
    fun request(rate: Rate): RequestSpec =
        RequestSpec(rate.intervalMs, rate.intervalMs, rate.maxDelayMs, rate.minDistanceM, if (rate.highAccuracy) QUALITY_HIGH_ACCURACY else QUALITY_BALANCED)

    /**
     * One provider, never mixed (network fixes 100 m off made the position jump streets): fused on Android 12+ with Play services, else
     * gps (our filter does the fusion; AOSP's fused only picks between gps and network), network only if nothing else. Without Play
     * services the network provider may give the first fix once.
     */
    fun providers(
        enabled: Set<String>,
        sdk: Int,
        gms: Boolean,
    ): ProviderPlan {
        val main =
            when {
                sdk >= Build.VERSION_CODES.S && gms && "fused" in enabled -> "fused"
                "gps" in enabled -> "gps"
                "network" in enabled -> "network"
                else -> null
            }
        return ProviderPlan(main, "network".takeIf { main == "gps" && !gms && it in enabled })
    }
}
```

`RawTrack.kt`:

```kotlin
/** Mock-location fixes reach the filter only in a debuggable build with the bench flag file (`files/allow_mock`). */
internal object MockPolicy {
    fun allowed(
        debuggable: Boolean,
        flagFileExists: Boolean,
    ): Boolean = debuggable && flagFileExists
}
```

- [ ] **Step 4: Use it in `Sensors`, `PresenceController`, `AppModel` and the manifest**

`Sensors.kt`: add

```kotlin
private const val GMS_PACKAGE = "com.google.android.gms"
```

```kotlin
    private val gms: Boolean by lazy { appEnabled(GMS_PACKAGE) && "fused" in lm.allProviders }

    // The flags overload exists from Android 13; older phones only have the int one.
    @Suppress("DEPRECATION")
    private fun appEnabled(pkg: String): Boolean =
        runCatching {
            val pm = ctx.packageManager
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                pm.getApplicationInfo(pkg, android.content.pm.PackageManager.ApplicationInfoFlags.of(0)).enabled
            } else {
                pm.getApplicationInfo(pkg, 0).enabled
            }
        }.getOrDefault(false)

    private fun plan(): GpsPolicy.ProviderPlan {
        val enabled = lm.allProviders.filter { lm.isProviderEnabled(it) }.toSet()
        return GpsPolicy.providers(enabled, Build.VERSION.SDK_INT, gms)
    }
```

In `startLocation` replace `providers().forEach { p -> ... }` with:

```kotlin
        val plan = plan()
        plan.main?.let { p ->
            runCatching {
                request(p, rate, l)
                registered = true
                lm.getLastKnownLocation(p)?.let { model.realLoc = it }
            }.onFailure { Diag.error(TAG, "requestLocationUpdates failed for $p", it) }
        }
        plan.coldStart?.let { p -> runCatching { coldStart(p) }.onFailure { Diag.error(TAG, "cold start fix failed for $p", it) } }
```

and log `"gms" to gms, "fused_listed" to ("fused" in lm.allProviders), "providers" to listOfNotNull(plan.main, plan.coldStart).joinToString(","),
"max_delay_ms" to rate.maxDelayMs` in the "location started" line. Replace `request` and add `coldStart`:

```kotlin
    @SuppressLint("MissingPermission")
    private fun request(
        provider: String,
        rate: GpsPolicy.Rate,
        l: LocationListener,
    ) {
        val spec = GpsPolicy.request(rate)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
            return lm.requestLocationUpdates(provider, spec.intervalMs, spec.minDistanceM, l)
        }
        val req =
            LocationRequest
                .Builder(spec.intervalMs)
                .setMinUpdateIntervalMillis(spec.minIntervalMs)
                .setMaxUpdateDelayMillis(spec.maxDelayMs)
                .setMinUpdateDistanceMeters(spec.minDistanceM)
                .setQuality(spec.quality)
                .build()
        lm.requestLocationUpdates(provider, req, ctx.mainExecutor, l)
    }

    // One network fix to start from while the GNSS chip searches (no Play services); the core inflates its error 4x and drops network fixes
    // once GNSS fixes arrive.
    @SuppressLint("MissingPermission")
    private fun coldStart(provider: String) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            lm.getCurrentLocation(provider, null, ctx.mainExecutor) { loc -> loc?.let(::deliver) }
        } else {
            lm.getLastKnownLocation(provider)?.let(::deliver)
        }
    }
```

Delete the old `private fun providers()`. `PresenceController`:

```kotlin
    /** Whether the screen is on (the in-zone rate is 1 s with it on, 5 s with it off). */
    var screenOn = true
        private set

    /** Follow screen on/off for the life of the process: each change re-applies the location rate (an unchanged rate is a no-op). */
    fun watchScreen(ctx: Context) {
        screenOn = ctx.getSystemService(android.os.PowerManager::class.java).isInteractive
        val r =
            object : android.content.BroadcastReceiver() {
                override fun onReceive(
                    c: Context,
                    i: android.content.Intent,
                ) {
                    screenOn = i.action == android.content.Intent.ACTION_SCREEN_ON
                    applyLocation()
                }
            }
        val filter =
            android.content.IntentFilter().apply {
                addAction(android.content.Intent.ACTION_SCREEN_ON)
                addAction(android.content.Intent.ACTION_SCREEN_OFF)
            }
        androidx.core.content.ContextCompat.registerReceiver(ctx, r, filter, androidx.core.content.ContextCompat.RECEIVER_NOT_EXPORTED)
    }
```

and in `applyLocation` use `GpsPolicy.forDecision(decision, appVisible, screenOn)`. Call `presence.watchScreen(ctx)` from the `AppModel`
`init` block (add `init { presence.watchScreen(ctx) }` after the collaborators). In `AppModel` add
`private val mockAllowed = MockPolicy.allowed(ctx.applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE != 0, java.io.File(ctx.filesDir, "allow_mock").exists())`
and pass it: `engine.onFix(sample.toFixIn(mockAllowed), stepsTotal, false)`.

`AndroidManifest.xml`, before `<application`:

```xml
    <!-- Android 11+: lets the app see whether Google Play services is installed (it then uses the fused provider) -->
    <queries>
        <package android:name="com.google.android.gms" />
    </queries>
```

- [ ] **Step 5: Run the tests and the gate**

Run: `ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add android/app/src/main android/app/src/test
git commit -m "feat: request GPS by screen and Play services" -m "In a zone: 1 s with the screen on, 5 s batched with it off, GNSS
quality in both. Phones without Play services use the gps provider and
one network cold-start fix; mock fixes are dropped unless a debug flag
allows them." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 12: GNSS status lines, 2 s step batches with event time, perf in the heartbeat

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/Gnss.kt`
- Modify: `Sensors.kt` (`startGnss`, `stopGnss`, `setFastSteps`), `PresenceController.kt` (`applyLocation`), `FieldDiagnostics.kt`
  (`recordOnFix`, perf fields), `AppModel.kt` (`onFix` timing)
- Test: `android/app/src/test/java/dev/apgo2/GnssTest.kt`, `android/app/src/test/java/dev/apgo2/PercentilesTest.kt`

**Interfaces:**

- Produces: `Sat(constellation: Int, used: Boolean, cn0: Float, carrierHz: Float?)`, `GnssBands.of(hz: Float): String` (`L1`, `L5`, `E5b`, `L2`,
  `E6`, `other`), `GnssSummary.fields(sats: List<Sat>): Map<String, Any?>` (`in_view`, `used`, `by_constellation`, `cn0_top4`, `bands`,
  `dual_freq`), `Percentiles.of(values: List<Long>, q: Double): Long`, `Sensors.startGnss()/stopGnss()`, `Sensors.setFastSteps(on: Boolean)`.
- Diag lines (all builds, no positions): `gnss status` every 10 s while in a zone; `gnss hardware` once per session (`model` from
  `gnssHardwareModelName`, `capabilities` from `gnssCapabilities` on Android 12+). Heartbeat gains `perf_n`, `perf_p50_us`, `perf_p99_us`
  (debug builds; time of `engine.onFix`).

- [ ] **Step 1: Write the failing tests**

`android/app/src/test/java/dev/apgo2/GnssTest.kt`:

```kotlin
package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class GnssTest {
    @Test fun carrierFrequenciesMapToBands() {
        assertEquals("L1", GnssBands.of(1_575.42e6f)) // GPS L1, Galileo E1
        assertEquals("L1", GnssBands.of(1_561.098e6f)) // BeiDou B1
        assertEquals("L1", GnssBands.of(1_602.0e6f)) // GLONASS G1
        assertEquals("L5", GnssBands.of(1_176.45e6f)) // GPS L5, Galileo E5a
        assertEquals("E5b", GnssBands.of(1_207.14e6f))
        assertEquals("L2", GnssBands.of(1_227.6e6f))
        assertEquals("E6", GnssBands.of(1_278.75e6f))
        assertEquals("other", GnssBands.of(0f))
    }

    @Test fun aSummaryCountsSatellitesByConstellationAndSeesDualFrequency() {
        val sats =
            listOf(
                Sat(constellation = 1, used = true, cn0 = 40f, carrierHz = 1_575.42e6f),
                Sat(constellation = 1, used = true, cn0 = 38f, carrierHz = 1_176.45e6f),
                Sat(constellation = 6, used = false, cn0 = 30f, carrierHz = 1_575.42e6f),
                Sat(constellation = 6, used = true, cn0 = 36f, carrierHz = null),
                Sat(constellation = 5, used = false, cn0 = 20f, carrierHz = 1_561.098e6f),
            )
        val f = GnssSummary.fields(sats)
        assertEquals(5, f["in_view"])
        assertEquals(3, f["used"])
        assertEquals("beidou=0/1,galileo=1/2,gps=2/2", f["by_constellation"])
        assertEquals(36.0, f["cn0_top4"] as Double, 1e-9) // 40, 38, 36, 30
        assertEquals("L1,L5", f["bands"])
        assertEquals(true, f["dual_freq"])
    }

    @Test fun anEmptySkyIsAllZeros() {
        val f = GnssSummary.fields(emptyList())
        assertEquals(0, f["in_view"])
        assertEquals(0.0, f["cn0_top4"] as Double, 0.0)
        assertEquals(false, f["dual_freq"])
    }
}
```

`android/app/src/test/java/dev/apgo2/PercentilesTest.kt`:

```kotlin
package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Test

class PercentilesTest {
    @Test fun nearestRankPercentiles() {
        val v = (1L..100L).toList().shuffled()
        assertEquals(50L, Percentiles.of(v, 0.5))
        assertEquals(99L, Percentiles.of(v, 0.99))
        assertEquals(0L, Percentiles.of(emptyList(), 0.5))
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.GnssTest' --tests 'dev.apgo2.PercentilesTest' --console=plain -q`
Expected: FAIL to compile: `Unresolved reference 'GnssBands'`, `Unresolved reference 'Percentiles'`.

- [ ] **Step 3: Write `Gnss.kt`**

```kotlin
package dev.apgo2

private const val MHZ = 1e6f
private const val TOP_CN0 = 4

/** One satellite of a `GnssStatus` (constellation is a `GnssStatus.CONSTELLATION_*` value; [carrierHz] when the phone reports it). */
internal data class Sat(
    val constellation: Int,
    val used: Boolean,
    val cn0: Float,
    val carrierHz: Float?,
)

/** Which frequency band a carrier frequency is in (dual-frequency phones also see L5/E5a). */
internal object GnssBands {
    fun of(hz: Float): String {
        val mhz = hz / MHZ
        return when {
            mhz in 1_559f..1_610f -> "L1"
            mhz in 1_164f..1_189f -> "L5"
            mhz in 1_189f..1_214f -> "E5b"
            mhz in 1_215f..1_240f -> "L2"
            mhz in 1_260f..1_300f -> "E6"
            else -> "other"
        }
    }
}

/** The fields of a `gnss` diagnostics line: satellites in view and used per constellation, signal strength and bands. No positions. */
internal object GnssSummary {
    private val names = mapOf(1 to "gps", 2 to "sbas", 3 to "glonass", 4 to "qzss", 5 to "beidou", 6 to "galileo", 7 to "irnss")

    fun fields(sats: List<Sat>): Map<String, Any?> {
        val bands = sats.mapNotNull { s -> s.carrierHz?.let { GnssBands.of(it) } }.filter { it != "other" }.toSortedSet()
        val byConstellation =
            sats
                .groupBy { names[it.constellation] ?: "other" }
                .toSortedMap()
                .entries
                .joinToString(",") { (name, list) -> "$name=${list.count { it.used }}/${list.size}" }
        val top = sats.map { it.cn0.toDouble() }.sortedDescending().take(TOP_CN0)
        return linkedMapOf(
            "in_view" to sats.size,
            "used" to sats.count { it.used },
            "by_constellation" to byConstellation,
            "cn0_top4" to if (top.isEmpty()) 0.0 else top.average(),
            "bands" to bands.joinToString(","),
            "dual_freq" to (bands.size > 1),
        )
    }
}

/** Nearest-rank percentiles of a minute's `onFix` times (heartbeat perf fields). */
internal object Percentiles {
    fun of(
        values: List<Long>,
        q: Double,
    ): Long {
        if (values.isEmpty()) return 0L
        val sorted = values.sorted()
        val rank = kotlin.math.ceil(q * sorted.size).toInt().coerceIn(1, sorted.size)
        return sorted[rank - 1]
    }
}
```

- [ ] **Step 4: Wire the GNSS callback, fast steps and perf**

`Sensors.kt` (imports `android.location.GnssStatus`, `android.os.Handler`, `android.os.Looper`):

```kotlin
    private var gnssCallback: GnssStatus.Callback? = null
    private var gnssHardwareLogged = false
    private val gnssThrottle = Throttle(GNSS_LINE_MS)
    private var fastSteps = false

    /** While in a zone: a `gnss` line every 10 s (no positions), and the chip's model and capabilities once per session. */
    @SuppressLint("MissingPermission")
    fun startGnss() {
        if (gnssCallback != null) return
        val cb =
            object : GnssStatus.Callback() {
                override fun onSatelliteStatusChanged(status: GnssStatus) {
                    if (!gnssThrottle.due(System.currentTimeMillis())) return
                    val sats =
                        (0 until status.satelliteCount).map { i ->
                            Sat(status.getConstellationType(i), status.usedInFix(i), status.getCn0DbHz(i), status.getCarrierFrequencyHz(i).takeIf { status.hasCarrierFrequencyHz(i) })
                        }
                    Diag.info("gnss", "status", *GnssSummary.fields(sats).toList().toTypedArray())
                }
            }
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) lm.registerGnssStatusCallback(ctx.mainExecutor, cb) else lm.registerGnssStatusCallback(cb, Handler(Looper.getMainLooper()))
            gnssCallback = cb
        }.onFailure { Diag.error(TAG, "gnss status failed", it) }
        if (!gnssHardwareLogged) {
            gnssHardwareLogged = true
            val model = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) lm.gnssHardwareModelName else null
            val caps = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) lm.gnssCapabilities.toString() else null
            Diag.info("gnss", "hardware", "model" to model, "capabilities" to caps)
        }
    }

    fun stopGnss() {
        gnssCallback?.let { lm.unregisterGnssStatusCallback(it) }
        gnssCallback = null
    }

    /** Steps every 2 s while in a zone (cadence for the filter), 10 s otherwise; re-registers when it changes. */
    fun setFastSteps(on: Boolean) {
        if (fastSteps == on) return
        fastSteps = on
        if (stepListener != null) {
            stopSteps()
            startSteps()
        }
    }
```

with `private const val GNSS_LINE_MS = 10_000L` and `private const val FAST_STEP_BATCH_US = 2_000_000` at the top, and in `startSteps`
`sm.registerListener(l, sensor, SensorManager.SENSOR_DELAY_NORMAL, if (fastSteps) FAST_STEP_BATCH_US else STEP_BATCH_US)`.
`PresenceController.applyLocation` ends with:

```kotlin
        val inZone = rate != null && decision.state == PresenceState.InZone
        if (inZone) model.sensors.startGnss() else model.sensors.stopGnss()
        model.sensors.setFastSteps(inZone)
```

`FieldDiagnostics`: add `private val onFixMicros = ArrayList<Long>()`,

```kotlin
    /** Time of one `engine.onFix` (debug builds pass it; the heartbeat reports p50/p99 per minute). */
    fun recordOnFix(nanos: Long) {
        onFixMicros.add(nanos / NANOS_PER_US)
    }
```

(`private const val NANOS_PER_US = 1_000L`) and in `sensorFields()` add
`"perf_n" to onFixMicros.size, "perf_p50_us" to Percentiles.of(onFixMicros, 0.5), "perf_p99_us" to Percentiles.of(onFixMicros, 0.99)`,
clearing `onFixMicros` in `heartbeat()` next to the other resets. In `AppModel.onFix` wrap the engine call:

```kotlin
        val t0 = System.nanoTime()
        val events = engine.onFix(sample.toFixIn(mockAllowed), stepsTotal, false)
        if (debuggable) diag.recordOnFix(System.nanoTime() - t0)
        handle(events)
```

(`private val debuggable = ctx.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0`, reused for `mockAllowed`).

- [ ] **Step 5: Run the tests and the gate**

Run: `ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add android/app/src/main/java/dev/apgo2 android/app/src/test/java/dev/apgo2
git commit -m "feat: log GNSS status and time the filter" -m "A gnss line every 10 s in a zone (satellites per constellation, C/N0,
bands, dual frequency), the chip model once, 2 s step batches with
the sensor event time, and on_fix p50/p99 in the heartbeat." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 13: "Keep tracking alive": the battery setup step

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/BatteryGuide.kt`
- Modify: `presence/SetupProgress.kt` (`SetupStep.Battery`), `SetupFlow.kt`, `SetupSteps.kt` (`BatteryStep`), `ui/HelpText.kt` (battery topics)
- Test: `android/app/src/test/java/dev/apgo2/BatteryGuideTest.kt`, `android/app/src/test/java/dev/apgo2/ui/HelpTextTest.kt`

**Interfaces:**

- Produces: `BatteryGuide.forMaker(manufacturer: String): HelpTopic`, `BatteryGuide.needed(ignoringOptimizations: Boolean): Boolean`,
  `Help.batterySamsung`, `Help.batteryXiaomi`, `Help.batteryHuawei`, `Help.batteryOppo`, `Help.batteryOther`, `SetupText.BATTERY_TITLE`,
  `SetupText.BATTERY_WHY`, `SetupText.BATTERY_BUTTON`, `SetupStep.Battery`.

- [ ] **Step 1: Write the failing tests**

`android/app/src/test/java/dev/apgo2/BatteryGuideTest.kt`:

```kotlin
package dev.apgo2

import dev.apgo2.ui.Help
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BatteryGuideTest {
    @Test fun eachMakerGetsItsOwnGuidanceAndUnknownMakersTheGeneralOne() {
        assertEquals(Help.batterySamsung, BatteryGuide.forMaker("samsung"))
        assertEquals(Help.batteryXiaomi, BatteryGuide.forMaker("Xiaomi"))
        assertEquals(Help.batteryXiaomi, BatteryGuide.forMaker("Redmi"))
        assertEquals(Help.batteryXiaomi, BatteryGuide.forMaker("POCO"))
        assertEquals(Help.batteryHuawei, BatteryGuide.forMaker("HUAWEI"))
        assertEquals(Help.batteryHuawei, BatteryGuide.forMaker("honor"))
        assertEquals(Help.batteryOppo, BatteryGuide.forMaker("OnePlus"))
        assertEquals(Help.batteryOppo, BatteryGuide.forMaker("realme"))
        assertEquals(Help.batteryOppo, BatteryGuide.forMaker("OPPO"))
        assertEquals(Help.batteryOther, BatteryGuide.forMaker("Google"))
        assertEquals(Help.batteryOther, BatteryGuide.forMaker(""))
    }

    @Test fun theStepIsOnlyShownWhileBatteryOptimizationIsOn() {
        assertTrue(BatteryGuide.needed(ignoringOptimizations = false))
        assertFalse(BatteryGuide.needed(ignoringOptimizations = true))
    }
}
```

In `HelpTextTest.kt` add the five topics to the `topics` list (`Help.batterySamsung, Help.batteryXiaomi, Help.batteryHuawei,
Help.batteryOppo, Help.batteryOther`) and `SetupText.BATTERY_WHY` to `setupCopyHasNoBrokenJoins`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd android && ./gradlew :app:testDebugUnitTest --tests 'dev.apgo2.BatteryGuideTest' --tests 'dev.apgo2.ui.HelpTextTest' --console=plain -q`
Expected: FAIL to compile: `Unresolved reference 'BatteryGuide'`, `Unresolved reference 'batterySamsung'`.

- [ ] **Step 3: Write the copy, the picker and the step**

In `ui/HelpText.kt`, inside `object Help` (new section):

```kotlin
    // ---- keeping tracking alive (battery optimisation), by phone maker
    val batterySamsung =
        HelpTopic(
            "Keep tracking alive on Samsung",
            "Samsung puts apps to sleep to save battery, which stops quest tracking with the screen off. Tap the button, find " +
                "Archipela-Go 2 and choose Don't optimise. Also open Settings, Battery, Background usage limits and make sure the app " +
                "is not in Sleeping apps or Deep sleeping apps.",
        )
    val batteryXiaomi =
        HelpTopic(
            "Keep tracking alive on Xiaomi, Redmi and POCO",
            "MIUI and HyperOS stop apps in the background. Tap the button and turn optimisation off for Archipela-Go 2. Then open the " +
                "app's info page, choose Battery saver and pick No restrictions, and turn on Autostart.",
        )
    val batteryHuawei =
        HelpTopic(
            "Keep tracking alive on Huawei and Honor",
            "These phones close apps in the background. Tap the button and allow Archipela-Go 2 to ignore optimisation. Then open " +
                "Settings, Battery, App launch, find the app and switch it to Manage manually with every option on.",
        )
    val batteryOppo =
        HelpTopic(
            "Keep tracking alive on OnePlus, OPPO and realme",
            "These phones freeze apps in the background. Tap the button and turn optimisation off for Archipela-Go 2. Then open the " +
                "app's info page, choose Battery and allow background activity.",
        )
    val batteryOther =
        HelpTopic(
            "Keep tracking alive",
            "Android may pause apps in the background to save battery, which stops quest tracking with the screen off. Tap the button, " +
                "find Archipela-Go 2 and turn battery optimisation off for it.",
        )
```

In `object SetupText`:

```kotlin
    const val BATTERY_TITLE = "Keep tracking alive · optional"
    const val BATTERY_WHY =
        "Quests are tracked with the screen off. Your phone's battery saver can stop that; this lets the game keep running while you play."
    const val BATTERY_BUTTON = "Open battery settings"
```

`android/app/src/main/java/dev/apgo2/BatteryGuide.kt`:

```kotlin
package dev.apgo2

import dev.apgo2.ui.Help
import dev.apgo2.ui.HelpTopic

/** Which battery guidance a phone gets, by `Build.MANUFACTURER`. */
internal object BatteryGuide {
    fun forMaker(manufacturer: String): HelpTopic =
        when (manufacturer.trim().lowercase()) {
            "samsung" -> Help.batterySamsung
            "xiaomi", "redmi", "poco" -> Help.batteryXiaomi
            "huawei", "honor" -> Help.batteryHuawei
            "oneplus", "oppo", "realme" -> Help.batteryOppo
            else -> Help.batteryOther
        }

    /** The step is shown only while the app is still battery-optimised. */
    fun needed(ignoringOptimizations: Boolean): Boolean = !ignoringOptimizations
}
```

`presence/SetupProgress.kt`: `internal enum class SetupStep { Home, Wifi, Car, Battery }` (`nextStep` is unchanged: the step is optional).
`SetupFlow.kt`: the `Car` branch's `onDone` becomes

```kotlin
            CarStep(m, onBack = { step = SetupStep.Wifi }, onDone = {
                if (BatteryGuide.needed(m.ignoringBatteryOptimizations())) step = SetupStep.Battery else m.setup.finish()
            })
```

and a new branch `SetupStep.Battery -> BatteryStep(onBack = { step = SetupStep.Car }, onDone = { m.setup.finish() })`.
`AppModel` gains:

```kotlin
    /** Whether the app is exempt from battery optimisation (the setup step and the heartbeat read it). */
    fun ignoringBatteryOptimizations(): Boolean = ctx.getSystemService(android.os.PowerManager::class.java).isIgnoringBatteryOptimizations(ctx.packageName)
```

`SetupSteps.kt`:

```kotlin
/** Step 4 (optional, only while the app is battery-optimised): the phone maker's guidance and a button to the battery settings list. */
@Composable
internal fun BatteryStep(
    onBack: () -> Unit,
    onDone: () -> Unit,
) {
    val ctx = LocalContext.current
    val guide = remember { BatteryGuide.forMaker(android.os.Build.MANUFACTURER) }
    StepPage(title = SetupText.BATTERY_TITLE, why = SetupText.BATTERY_WHY, next = "Finish", skip = "Skip", onBack = onBack, onNext = onDone) {
        Text(guide.title, style = MaterialTheme.typography.titleSmall)
        Text(guide.body, style = MaterialTheme.typography.bodyMedium)
        // The settings list, not the direct "ignore optimisation?" dialog (Play policy reserves that for a few app kinds).
        Button(onClick = { ctx.startActivity(android.content.Intent(android.provider.Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)) }) {
            Text(SetupText.BATTERY_BUTTON)
        }
    }
}
```

- [ ] **Step 4: Run the tests and the gate**

Run: `ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS.

- [ ] **Step 5: Check it on the emulator** (`just emu-start && just emu-run`): open Home Base setup, finish the car step, see the battery step
with the general guidance (the emulator reports `Google`); the button opens the battery optimisation list. Screenshot to
`/tmp/claude-screenshots/location-quality/<HHMMSS>-battery-step.png`.

- [ ] **Step 6: Commit**

```bash
git add android/app/src/main/java/dev/apgo2 android/app/src/test/java/dev/apgo2
git commit -m "feat: guide players past battery savers" -m "An optional setup step, shown while the app is battery-optimised, with
guidance per phone maker and a button to the battery settings list." -m "Closes #85
Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 14: `Locator::display`, compass readings and FFI `position()` / `on_heading`

**Files:**

- Create: `core/src/loc/heading.rs`
- Modify: `core/src/loc/mod.rs` (`DisplayPosition`, `HeadingSource`, `DisplaySource`, `pub mod heading;`), `core/src/loc/locator.rs`
  (`compass` field, `on_heading`, `display`), `core/src/loc/params.rs` (display group), `core/src/loc/bench/run.rs` (feed headings),
  `core/src/game.rs` (`on_heading`, `position`), `core/ffi/src/engine.rs` (`HeadingIn`, `PositionOut`, `on_heading`, `position`)

**Interfaces:**

- Consumes: `Locator`, `Estimate` (Task 7); `HeadingIn`, `CompassAccuracy` (Task 2).
- Produces:
  - `loc::heading::{wrap_deg(f64) -> f64, circular_mean_deg(&[f64]) -> Option<f64>, tilt_deg(pitch, roll) -> f64, Compass}`;
    `Compass::push(HeadingIn)`, `Compass::latest(now_ms, fresh_ms) -> Option<HeadingIn>`, `Compass::spread_deg(now_ms, window_ms) -> Option<f64>`,
    `Compass::held_flat_and_steady(now_ms, &LocParams) -> Option<f64>`, `Compass::rate_deg_s(now_ms, window_ms) -> Option<f64>`.
  - `loc::HeadingSource { Course, Compass, None }`, `loc::DisplaySource { Gps, Bridged, Predicted, Stale }`,
    `loc::DisplayPosition { lat, lon, est_lat, est_lon, uncertainty_m, speed_mps, course_deg: Option<f64>, heading_deg: Option<f64>,
    heading_source, matched: bool, match_confidence: f64, source: DisplaySource, age_ms: i64, snap: bool }`.
  - `Locator::on_heading(&HeadingIn)`, `Locator::display(now_ms) -> Option<DisplayPosition>`; `Game::on_heading(&HeadingIn)`,
    `Game::position(now_ms) -> Option<DisplayPosition>`.
  - FFI `HeadingIn { azimuth_deg, accuracy: String, pitch_deg, roll_deg, t_ms }` [Kotlin `HeadingIn(azimuthDeg, accuracy, pitchDeg, rollDeg, tMs)`],
    `PositionOut { lat, lon, est_lat, est_lon, uncertainty_m, speed_mps, course_deg, heading_deg, heading_source: String, matched,
    match_confidence, source: String, age_ms, snap }`, `Engine::on_heading(h: HeadingIn)`, `Engine::position(now_ms) -> Option<PositionOut>`.
- `LocParams` additions: `display_predict_max_ms: 3000`, `predicted_after_ms: 6000`, `stale_after_ms: 30_000`, `compass_fresh_ms: 1000`,
  `compass_window_ms: 2000`, `compass_max_spread_deg: 10.0`, `compass_max_tilt_deg: 60.0`.
- Plan choices: a position counts as "predicted" 6 s after its fix (more than the 5 s screen-off interval; the spec gives no number) and
  its uncertainty grows by `max(speed sigma, 0.5 m/s)` per second. "App visible and screen on" is not checked in the core: the map, the
  only place the arrow shows, is only drawn then.

- [ ] **Step 1: Write the failing tests**

Bottom of `core/src/loc/heading.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::loc::CompassAccuracy;

    fn h(t_ms: i64, az: f64, acc: CompassAccuracy, pitch: f64) -> HeadingIn {
        HeadingIn { t_ms, azimuth_deg: az, accuracy: acc, pitch_deg: pitch, roll_deg: 0.0 }
    }

    #[test]
    fn angles_wrap_and_average_across_north() {
        assert!((wrap_deg(190.0) + 170.0).abs() < 1e-9 && (wrap_deg(-190.0) - 170.0).abs() < 1e-9);
        let m = circular_mean_deg(&[350.0, 10.0]).unwrap();
        assert!(m < 1e-9 || (m - 360.0).abs() < 1e-9, "{m}");
        assert!(circular_mean_deg(&[0.0, 180.0]).is_none(), "no mean direction");
        assert!((tilt_deg(0.0, 0.0)).abs() < 1e-9 && (tilt_deg(60.0, 0.0) - 60.0).abs() < 1e-9);
    }

    #[test]
    fn the_compass_counts_only_when_held_flat_steady_and_accurate() {
        let p = LocParams::default();
        let mut c = Compass::default();
        for i in 0..5 {
            c.push(h(1000 + i * 500, 90.0 + f64::from(u8::try_from(i).unwrap()), CompassAccuracy::High, 10.0));
        }
        assert!((c.held_flat_and_steady(3000, &p).unwrap() - 92.0).abs() < 1.0);
        assert!(c.held_flat_and_steady(5000, &p).is_none(), "stale reading");
        let mut tilted = Compass::default();
        (0..5).for_each(|i| tilted.push(h(1000 + i * 500, 90.0, CompassAccuracy::High, 80.0)));
        assert!(tilted.held_flat_and_steady(3000, &p).is_none(), "phone upright in a pocket");
        let mut swinging = Compass::default();
        (0..5).for_each(|i| swinging.push(h(1000 + i * 500, if i % 2 == 0 { 60.0 } else { 120.0 }, CompassAccuracy::High, 10.0)));
        assert!(swinging.held_flat_and_steady(3000, &p).is_none(), "spread over 10 degrees");
        let mut low = Compass::default();
        (0..5).for_each(|i| low.push(h(1000 + i * 500, 90.0, CompassAccuracy::Low, 10.0)));
        assert!(low.held_flat_and_steady(3000, &p).is_none(), "accuracy below medium");
    }

    #[test]
    fn the_azimuth_rate_is_degrees_per_second() {
        let mut c = Compass::default();
        (0..5).for_each(|i| c.push(h(i * 500, f64::from(u8::try_from(i).unwrap()) * 20.0, CompassAccuracy::High, 0.0)));
        assert!((c.rate_deg_s(2000, 2000).unwrap() - 40.0).abs() < 1e-6);
    }
}
```

In `locator.rs` tests:

```rust
    #[test]
    fn nothing_is_shown_before_the_first_fix() {
        assert!(Locator::default().display(0).is_none());
    }

    #[test]
    fn the_shown_position_predicts_at_most_three_seconds_ahead_and_ages() {
        let mut l = Locator::default();
        for t in 0..=30 {
            l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, 4.0));
        }
        let e = l.last().unwrap();
        let d = |now_s: i64| l.display(now_s * 1000).unwrap();
        assert_eq!(d(30).source, DisplaySource::Gps);
        let ahead = distance_m(Point::new(d(32).lat, d(32).lon), e.point());
        assert!((ahead - 2.8).abs() < 0.6, "2 s at 1.4 m/s: {ahead}");
        let capped = distance_m(Point::new(d(40).lat, d(40).lon), e.point());
        assert!(capped < 1.4 * 3.0 + 0.6, "never more than 3 s ahead: {capped}");
        assert_eq!(d(37).source, DisplaySource::Predicted);
        assert!(d(37).uncertainty_m > e.uncertainty_m);
        assert_eq!(d(61).source, DisplaySource::Stale);
        assert_eq!(d(30).heading_source, HeadingSource::Course);
        assert!((d(30).heading_deg.unwrap() - 90.0).abs() < 10.0);
    }

    #[test]
    fn standing_still_shows_the_compass_only_when_held_flat() {
        let mut l = Locator::default();
        let t = stand(&mut l, 0, 40);
        assert_eq!(l.display(t * 1000).unwrap().heading_source, HeadingSource::None);
        for i in 0..5 {
            l.on_heading(&HeadingIn { t_ms: t * 1000 - 2000 + i * 500, azimuth_deg: 45.0, accuracy: crate::loc::CompassAccuracy::High, pitch_deg: 5.0, roll_deg: 5.0 });
        }
        let d = l.display(t * 1000).unwrap();
        assert_eq!((d.heading_source, d.heading_deg.map(f64::round)), (HeadingSource::Compass, Some(45.0)));
    }

    #[test]
    fn a_reset_or_relocation_snaps_instead_of_gliding() {
        let mut l = Locator::default();
        l.on_fix(&fix(o(), 0, 5.0));
        assert!(l.display(0).unwrap().snap, "the first fix is a reset");
        l.on_fix(&fix(o(), 1, 5.0));
        assert!(!l.display(1000).unwrap().snap);
    }
```

In `core/ffi/src/engine.rs` tests:

```rust
    #[test]
    fn position_before_any_fix_is_none_and_headings_without_a_game_are_ignored() {
        let dir = std::env::temp_dir().join(format!("apgo-ffi-pos-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        assert!(e.position(0).is_none());
        e.on_heading(HeadingIn { azimuth_deg: 10.0, accuracy: "high".into(), pitch_deg: 0.0, roll_deg: 0.0, t_ms: 1 });
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_display_position_maps_to_its_ffi_record() {
        use apgo_core::loc::{DisplayPosition, DisplaySource, HeadingSource};
        let d = DisplayPosition { lat: 1.0, lon: 2.0, heading_source: HeadingSource::Compass, source: DisplaySource::Bridged, heading_deg: Some(45.0), ..DisplayPosition::default() };
        let out = position_out(&d);
        assert_eq!((out.heading_source.as_str(), out.source.as_str(), out.heading_deg), ("compass", "bridged", Some(45.0)));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc:: && cargo test -p apgo-ffi`
Expected: FAIL to compile: `file not found for module 'heading'`, `no method named 'display'`, `cannot find struct 'PositionOut'`.

- [ ] **Step 3: Write `core/src/loc/heading.rs`** (Task 20 appends the carry-offset estimator to this file)

```rust
//! Compass readings: the "held flat and steady" rule for the map arrow, and (Task 20) the carry offset between where the phone points
//! and where the player walks.

use std::collections::VecDeque;

use crate::loc::{CompassAccuracy, HeadingIn, LocParams};
use crate::num::{count_f64, i64_to_f64};

/// `d` folded into (-180, 180] degrees.
#[must_use]
pub fn wrap_deg(d: f64) -> f64 {
    let w = (d + 180.0).rem_euclid(360.0) - 180.0;
    if w <= -180.0 {
        180.0
    } else {
        w
    }
}

/// The mean direction of `angles` (degrees, 0..360); `None` when they cancel out.
#[must_use]
pub fn circular_mean_deg(angles: &[f64]) -> Option<f64> {
    let (s, c) = angles.iter().fold((0.0, 0.0), |(s, c), a| (s + a.to_radians().sin(), c + a.to_radians().cos()));
    (s.hypot(c) > 1e-9 * count_f64(angles.len().max(1))).then(|| s.atan2(c).to_degrees().rem_euclid(360.0))
}

/// How far the phone's screen is tilted from flat, degrees (0 = lying flat, 90 = upright).
#[must_use]
pub fn tilt_deg(pitch_deg: f64, roll_deg: f64) -> f64 {
    (pitch_deg.to_radians().cos() * roll_deg.to_radians().cos()).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The last few seconds of compass readings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Compass {
    recent: VecDeque<HeadingIn>,
}

impl Compass {
    /// Add a reading (older than the newest is ignored); keeps 20 s.
    pub fn push(&mut self, h: HeadingIn) {
        if self.recent.back().is_some_and(|b| h.t_ms <= b.t_ms) {
            return;
        }
        self.recent.push_back(h);
        while self.recent.front().is_some_and(|f| h.t_ms - f.t_ms > 20_000) {
            self.recent.pop_front();
        }
    }

    /// The newest reading, if it is at most `fresh_ms` old at `now_ms`.
    #[must_use]
    pub fn latest(&self, now_ms: i64, fresh_ms: i64) -> Option<HeadingIn> {
        self.recent.back().filter(|h| now_ms - h.t_ms <= fresh_ms).copied()
    }

    fn window(&self, now_ms: i64, window_ms: i64) -> impl Iterator<Item = &HeadingIn> {
        self.recent.iter().filter(move |h| h.t_ms <= now_ms && now_ms - h.t_ms <= window_ms)
    }

    /// The largest angle between a reading of the last `window_ms` and their mean, degrees.
    #[must_use]
    pub fn spread_deg(&self, now_ms: i64, window_ms: i64) -> Option<f64> {
        let az: Vec<f64> = self.window(now_ms, window_ms).map(|h| h.azimuth_deg).collect();
        let m = circular_mean_deg(&az)?;
        Some(az.iter().map(|a| wrap_deg(a - m).abs()).fold(0.0, f64::max))
    }

    /// How fast the azimuth turned over the last `window_ms`, degrees per second (first to last reading).
    #[must_use]
    pub fn rate_deg_s(&self, now_ms: i64, window_ms: i64) -> Option<f64> {
        let w: Vec<&HeadingIn> = self.window(now_ms, window_ms).collect();
        let (first, last) = (w.first()?, w.last()?);
        let dt = i64_to_f64(last.t_ms - first.t_ms) / 1000.0;
        (dt > 0.0).then(|| wrap_deg(last.azimuth_deg - first.azimuth_deg).abs() / dt)
    }

    /// The compass azimuth to show while standing: only with a fresh reading, accuracy medium or better, the phone within 60 degrees of flat
    /// and the azimuth steady (spread under 10 degrees over 2 s). Otherwise no arrow: an unreliable compass is dropped, not shown.
    #[must_use]
    pub fn held_flat_and_steady(&self, now_ms: i64, p: &LocParams) -> Option<f64> {
        let h = self.latest(now_ms, p.compass_fresh_ms)?;
        let accurate = matches!(h.accuracy, CompassAccuracy::High | CompassAccuracy::Medium);
        let flat = tilt_deg(h.pitch_deg, h.roll_deg) <= p.compass_max_tilt_deg;
        let steady = self.spread_deg(now_ms, p.compass_window_ms).is_some_and(|s| s < p.compass_max_spread_deg);
        (accurate && flat && steady).then(|| circular_mean_deg(&self.window(now_ms, p.compass_window_ms).map(|x| x.azimuth_deg).collect::<Vec<_>>())).flatten()
    }
}
```

(`flatten` on `Option<Option<f64>>`: if clippy prefers it, write `if accurate && flat && steady { circular_mean_deg(..) } else { None }`.)

- [ ] **Step 4: Types, `display`, `on_heading`**

`core/src/loc/mod.rs` (add `pub mod heading;` and):

```rust
/// Where the map arrow's direction comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HeadingSource {
    /// The direction of travel.
    Course,
    /// The compass (standing, phone held flat and steady).
    Compass,
    /// No arrow.
    #[default]
    None,
}

/// What kind of position the map pin shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DisplaySource {
    /// From a recent GPS fix.
    #[default]
    Gps,
    /// From steps and heading in a GPS gap (pin drawn hollow).
    Bridged,
    /// Predicted from the last fix.
    Predicted,
    /// Too old to trust (pin greyed).
    Stale,
}

/// What the map shows for the player right now. Display only: quests never see this.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct DisplayPosition {
    /// Shown latitude (matched, bridged or estimated, predicted up to 3 s ahead).
    pub lat: f64,
    /// Shown longitude.
    pub lon: f64,
    /// The estimate's latitude.
    pub est_lat: f64,
    /// The estimate's longitude.
    pub est_lon: f64,
    /// 68 % radius of the shown position, metres.
    pub uncertainty_m: f64,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// Course, degrees.
    pub course_deg: Option<f64>,
    /// Arrow direction, degrees.
    pub heading_deg: Option<f64>,
    /// Where the arrow comes from.
    pub heading_source: HeadingSource,
    /// Whether the pin sits on a matched street.
    pub matched: bool,
    /// Matching confidence, 0..1.
    pub match_confidence: f64,
    /// What kind of position it is.
    pub source: DisplaySource,
    /// Age of the estimate behind it, ms.
    pub age_ms: i64,
    /// True after a reset or relocation: jump, do not glide.
    pub snap: bool,
}
```

`params.rs`: add the seven fields with docs and the defaults listed in Interfaces. `locator.rs`: add field `compass: Compass` (init
`Compass::default()`), and:

```rust
    /// A compass reading (true north).
    pub fn on_heading(&mut self, h: &HeadingIn) {
        self.compass.push(*h);
    }

    /// What the map shows at `now_ms`: the newest estimate, predicted along its course at most 3 s ahead while moving, aged into
    /// "predicted" and "stale", with the arrow from the course (moving) or the compass (standing, held flat and steady). `None` before the
    /// first fix.
    #[must_use]
    pub fn display(&self, now_ms: i64) -> Option<DisplayPosition> {
        let est = self.last?;
        let p = &self.params;
        let age = (now_ms - est.t_ms).max(0);
        let moving = self.hold.is_none() && est.motion != Motion::Stationary;
        let ahead_s = if moving { i64_to_f64(age.min(p.display_predict_max_ms)) / 1000.0 } else { 0.0 };
        let shown = match est.course_deg {
            Some(c) if ahead_s > 0.0 => destination(est.point(), c, est.speed_mps * ahead_s),
            _ => est.point(),
        };
        let source = if est.source == Source::Bridged {
            DisplaySource::Bridged
        } else if age > p.stale_after_ms {
            DisplaySource::Stale
        } else if age > p.predicted_after_ms {
            DisplaySource::Predicted
        } else {
            DisplaySource::Gps
        };
        let grown = if matches!(source, DisplaySource::Predicted | DisplaySource::Stale) { est.speed_sigma_mps.max(0.5) * i64_to_f64(age) / 1000.0 } else { 0.0 };
        let (heading_deg, heading_source) = match (est.course_deg, moving) {
            (Some(c), _) if source != DisplaySource::Stale => (Some(c), HeadingSource::Course),
            (_, false) => self.compass.held_flat_and_steady(now_ms, p).map_or((None, HeadingSource::None), |a| (Some(a), HeadingSource::Compass)),
            _ => (None, HeadingSource::None),
        };
        Some(DisplayPosition {
            lat: shown.lat,
            lon: shown.lon,
            est_lat: est.lat,
            est_lon: est.lon,
            uncertainty_m: est.uncertainty_m + grown,
            speed_mps: est.speed_mps,
            course_deg: est.course_deg,
            heading_deg,
            heading_source,
            matched: false,
            match_confidence: 0.0,
            source,
            age_ms: age,
            snap: matches!(est.verdict, Verdict::Reset | Verdict::Relocated),
        })
    }
```

(imports: `crate::geo::destination`, `crate::loc::heading::Compass`, `crate::loc::{DisplayPosition, DisplaySource, HeadingIn, HeadingSource}`.)
`bench/run.rs`: `Ev::Head(h) => { loc.on_heading(&h); None }`.

`core/src/game.rs`:

```rust
    /// A compass reading from the phone.
    pub fn on_heading(&mut self, h: &HeadingIn) {
        self.locator.on_heading(h);
    }

    /// What the map shows for the player at `now_ms` (display only).
    #[must_use]
    pub fn position(&self, now_ms: i64) -> Option<DisplayPosition> {
        self.locator.display(now_ms)
    }
```

- [ ] **Step 5: The FFI** (`core/ffi/src/engine.rs`)

```rust
/// One compass reading (azimuth already corrected to true north by the phone).
#[derive(Debug, Clone, uniffi::Record)]
pub struct HeadingIn {
    /// Degrees from true north.
    pub azimuth_deg: f64,
    /// `high`, `medium`, `low` or `unreliable`.
    pub accuracy: String,
    /// Pitch, degrees.
    pub pitch_deg: f64,
    /// Roll, degrees.
    pub roll_deg: f64,
    /// When it was read, Unix ms.
    pub t_ms: i64,
}

/// What the map shows for the player (see `apgo_core::loc::DisplayPosition`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct PositionOut {
    /// Shown latitude.
    pub lat: f64,
    /// Shown longitude.
    pub lon: f64,
    /// Estimate latitude.
    pub est_lat: f64,
    /// Estimate longitude.
    pub est_lon: f64,
    /// 68 % radius, metres.
    pub uncertainty_m: f64,
    /// Speed, m/s.
    pub speed_mps: f64,
    /// Course, degrees.
    pub course_deg: Option<f64>,
    /// Arrow direction, degrees.
    pub heading_deg: Option<f64>,
    /// `course`, `compass` or `none`.
    pub heading_source: String,
    /// Pin on a matched street.
    pub matched: bool,
    /// Matching confidence, 0..1.
    pub match_confidence: f64,
    /// `gps`, `bridged`, `predicted` or `stale`.
    pub source: String,
    /// Age of the estimate, ms.
    pub age_ms: i64,
    /// Jump instead of gliding.
    pub snap: bool,
}

fn position_out(d: &DisplayPosition) -> PositionOut {
    PositionOut {
        lat: d.lat,
        lon: d.lon,
        est_lat: d.est_lat,
        est_lon: d.est_lon,
        uncertainty_m: d.uncertainty_m,
        speed_mps: d.speed_mps,
        course_deg: d.course_deg,
        heading_deg: d.heading_deg,
        heading_source: match d.heading_source {
            HeadingSource::Course => "course",
            HeadingSource::Compass => "compass",
            HeadingSource::None => "none",
        }
        .into(),
        matched: d.matched,
        match_confidence: d.match_confidence,
        source: match d.source {
            DisplaySource::Gps => "gps",
            DisplaySource::Bridged => "bridged",
            DisplaySource::Predicted => "predicted",
            DisplaySource::Stale => "stale",
        }
        .into(),
        age_ms: d.age_ms,
        snap: d.snap,
    }
}
```

and in the exported `impl Engine`:

```rust
    /// A compass reading from the phone (at most 2 Hz).
    pub fn on_heading(&self, h: HeadingIn) {
        let core = apgo_core::loc::HeadingIn {
            t_ms: h.t_ms,
            azimuth_deg: h.azimuth_deg,
            accuracy: apgo_core::loc::CompassAccuracy::parse(&h.accuracy),
            pitch_deg: h.pitch_deg,
            roll_deg: h.roll_deg,
        };
        self.with_game(|g| g.on_heading(&core));
    }

    /// What the map shows for the player at `now_ms`; `None` with no game or before the first fix.
    pub fn position(&self, now_ms: i64) -> Option<PositionOut> {
        self.with_game(|g| g.position(now_ms)).flatten().map(|d| position_out(&d))
    }
```

- [ ] **Step 6: Run the tests and the gates**

Run: `cd core && cargo test -p apgo-core && cargo test -p apgo-ffi`
Expected: PASS.
Run: `just check-rust && ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS (Kotlin does not call the new functions yet; the bindings regenerate).

- [ ] **Step 7: Commit**

```bash
git add core/src core/ffi/src
git commit -m "feat: expose the displayed position and heading" -m "Locator::display predicts at most 3 s ahead, ages into predicted and
stale, and picks the arrow: course when moving, compass only when the
phone is held flat and steady. FFI position() and on_heading()." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 15: The gliding pin: LocationComponent, accuracy circle, heading arrow, `me` from `position()`

**Files:**

- Create: `android/app/src/main/java/dev/apgo2/MePin.kt`
- Modify: `ui/MapMarkers.kt` (`MarkerSpec.Me`, `ME_PIN_PX`, `parse`, `render`), `ui/ApgoIcons.kt` (`Heading`), `ui/Palette.kt` (`meUncertain`),
  `ui/HelpText.kt` (`Help.hollowDot`), `QuestMap.kt` (`me: MePin?`, `MapHolder.showMe`, `SyncPins`, framing), `MapStyle.kt` (drop the `ME`
  source, layer and badge), `AppModel.kt` (`pin`, `mePin`, `me`, `refreshPin`, `onHeading`), `Sensors.kt` (rotation vector),
  `PresenceController.kt` (home-Wi-Fi offer from `position()`, heading start/stop), `RawTrack.kt` (`CompassAccuracies`, heading line),
  `PlayScreen.kt`, `HomePicker.kt`, `RealmEditor.kt` (pass `m.mePin`)
- Test: `android/app/src/test/java/dev/apgo2/MePinTest.kt`, `ui/MapMarkersTest.kt`, `ui/PaletteTest.kt`, `ui/HelpTextTest.kt`, `RawTrackTest.kt`

**Interfaces:**

- Consumes: FFI `position(nowMs): PositionOut?`, `onHeading(HeadingIn)` (Task 14).
- Produces: `MarkerSpec.Me(heading: Boolean, state: String)` (key `me|h|<state>|pin` or `me|n|<state>|pin`; state `gps` / `bridged` / `stale`),
  `MapMarkers.ME_PIN_PX`, `ApgoIcons.Heading`, `ApgoPalette.meUncertain`, `MePin(lat, lon, accuracyM: Float, bearingDeg: Float?, spec, animateMs)`,
  `MePins.from(p: PositionOut, previous: MePin?): MePin`, `MePins.at(lat, lon, accuracyM: Double?): MePin`, `MePins.specs()`,
  `MePins.GLIDE_MS = 1000`, `CompassAccuracies.name(sensorStatus: Int): String`, `RawLines.heading(azimuthDeg, accuracy, pitchDeg, rollDeg)`,
  `AppModel.mePin: MePin?`, `AppModel.onHeading(azimuthDeg, pitchDeg, rollDeg, sensorAccuracy, eventMs)`.
- Plan choice: with no game open (realm editor, home picker) the pin comes from the raw `realLoc` (there is no filter without a game).
  Risk fallback (spec): if `LocationComponent` cannot take these images or the glide misbehaves on the device, draw the pin with a GeoJSON
  symbol layer animated by a frame clock from the same `MePin`; record that in the commit.

- [ ] **Step 1: Write the failing tests**

`android/app/src/test/java/dev/apgo2/MePinTest.kt`:

```kotlin
package dev.apgo2

import dev.apgo2.ui.MarkerSpec
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.apgo_ffi.PositionOut

class MePinTest {
    private fun pos(
        lat: Double = 40.0,
        uncertainty: Double = 6.0,
        headingSource: String = "course",
        heading: Double? = 90.0,
        source: String = "gps",
        snap: Boolean = false,
    ) = PositionOut(lat, -111.0, lat, -111.0, uncertainty, 1.4, heading, heading, headingSource, false, 0.0, source, 500L, snap)

    @Test fun aMovingPinGlidesWithTheCourseArrowAndItsCircle() {
        val p = MePins.from(pos(), previous = MePins.at(40.0, -111.0, null))
        assertEquals(MarkerSpec.Me(heading = true, state = "gps"), p.spec)
        assertEquals(90f, p.bearingDeg)
        assertEquals(6f, p.accuracyM)
        assertEquals(MePins.GLIDE_MS, p.animateMs)
    }

    @Test fun noArrowWithoutAHeadingAndNoCircleUnderThreeMetres() {
        val p = MePins.from(pos(headingSource = "none", uncertainty = 2.5), previous = null)
        assertNull(p.bearingDeg)
        assertEquals(false, p.spec.heading)
        assertEquals(0f, p.accuracyM)
    }

    @Test fun aResetARelocationOrAJumpOverFiftyMetresSnaps() {
        val prev = MePins.at(40.0, -111.0, null)
        assertEquals(0L, MePins.from(pos(snap = true), prev).animateMs)
        assertEquals("111 m away", 0L, MePins.from(pos(lat = 40.001), prev).animateMs)
        assertEquals("the first pin appears in place", 0L, MePins.from(pos(), previous = null).animateMs)
    }

    @Test fun bridgedPinsAreHollowAndStaleOnesGrey() {
        assertEquals("bridged", MePins.from(pos(source = "bridged"), null).spec.state)
        assertEquals("stale", MePins.from(pos(source = "stale"), null).spec.state)
        assertEquals("gps", MePins.from(pos(source = "predicted"), null).spec.state)
    }

    @Test fun everyPinImageIsListedForTheStyle() {
        assertEquals(6, MePins.specs().map { it.key }.toSet().size)
    }
}
```

`MapMarkersTest.everyMarkerSurvivesAKeyRoundTrip`: add `MarkerSpec.Me(heading = true, state = "bridged"), MarkerSpec.Me(heading = false, state = "gps")`
to `specs`. `PaletteTest`: add

```kotlin
    @Test fun theUncertainPinColourDiffersFromTheNormalOne() {
        assertTrue(ApgoPalette.me != ApgoPalette.meUncertain)
    }
```

`HelpTextTest`: add `Help.hollowDot` to `topics`. `RawTrackTest`:

```kotlin
    @Test fun sensorStatusNamesTheCompassAccuracy() {
        assertEquals("high", CompassAccuracies.name(3))
        assertEquals("medium", CompassAccuracies.name(2))
        assertEquals("low", CompassAccuracies.name(1))
        assertEquals("unreliable", CompassAccuracies.name(0))
        assertEquals("unreliable", CompassAccuracies.name(-1))
        assertEquals(mapOf("az" to 270.0, "acc" to "high", "pitch" to 5.0, "roll" to -3.0), RawLines.heading(270.0, "high", 5.0, -3.0))
    }
```

and change the old `RawLines.heading(270.0, "high")` assertion in `stepHeadingAndStateLinesCarryTheirFields` to the four-argument form.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd android && ./gradlew :app:testDebugUnitTest --console=plain -q`
Expected: FAIL to compile: `Unresolved reference 'MePins'`, `Unresolved reference 'Me'` (MarkerSpec), `Unresolved reference 'meUncertain'`,
`Unresolved reference 'CompassAccuracies'`.

- [ ] **Step 3: Design-system pieces**

`ui/Palette.kt` (map section): `val meUncertain = Color(0xFF90A4AE) // a pin whose position is old: greyed blue`.
`ui/ApgoIcons.kt` (navigation section): `val Heading = Lucide.Navigation2` (an arrow pointing up = north before rotation; if the Lucide name
differs, the compiler says so: try `Lucide.Navigation`). `ui/HelpText.kt` in `Help`:

```kotlin
    // ---- the map pin
    val hollowDot =
        HelpTopic(
            "Why is my dot hollow or grey?",
            "A hollow dot means GPS dropped out and the game is guessing where you are from your steps and the streets. A grey dot means " +
                "the last position is more than half a minute old. Neither counts for quests: only a solid dot from GPS does.",
        )
```

`ui/MapMarkers.kt`:

```kotlin
    /** The player on the map: the person pin, or an arrow when there is a heading; hollow while bridged, grey when stale. */
    data class Me(
        val heading: Boolean,
        val state: String,
    ) : MarkerSpec {
        override val key get() = "me|${if (heading) "h" else "n"}|$state|pin"
    }
```

(inside `sealed interface MarkerSpec`), in `MapMarkers`: `const val ME_PIN_PX = 120`, a `parse` branch `"me" -> MarkerSpec.Me(kindId == "h", family)`,
and a `render` branch:

```kotlin
            is MarkerSpec.Me -> {
                val icon = if (spec.heading) ApgoIcons.Heading else ApgoIcons.Me
                when (spec.state) {
                    "bridged" -> renderPin(icon, ME_PIN_PX, fill = ApgoPalette.onMap, glyph = ApgoPalette.me, ring = ApgoPalette.me)
                    "stale" -> renderPin(icon, ME_PIN_PX, fill = ApgoPalette.meUncertain)
                    else -> renderPin(icon, ME_PIN_PX, fill = ApgoPalette.me)
                }
            }
```

- [ ] **Step 4: `MePin.kt` and the compass bits in `RawTrack.kt`**

```kotlin
package dev.apgo2

import android.location.Location
import dev.apgo2.ui.METERS_PER_DEGREE
import dev.apgo2.ui.MarkerSpec
import uniffi.apgo_ffi.PositionOut
import kotlin.math.cos
import kotlin.math.hypot

/** What the map's location component shows: where, how sure (circle radius, 0 = none), which way, which image and how long to glide. */
internal data class MePin(
    val lat: Double,
    val lon: Double,
    val accuracyM: Float,
    val bearingDeg: Float?,
    val spec: MarkerSpec.Me,
    val animateMs: Long,
) {
    /** The pin as an Android location for `forceLocationUpdate`. */
    fun toLocation(): Location =
        Location("apgo").also { l ->
            l.latitude = lat
            l.longitude = lon
            l.accuracy = accuracyM
            bearingDeg?.let { l.bearing = it }
            l.time = System.currentTimeMillis()
        }
}

/** Turns the core's `position()` into a pin (pure, unit-tested). */
internal object MePins {
    /** Positions arrive about once a second; the component interpolates over this. */
    const val GLIDE_MS = 1_000L
    private const val SNAP_M = 50.0
    private const val MIN_CIRCLE_M = 3.0
    private val STATES = listOf("gps", "bridged", "stale")

    fun from(
        p: PositionOut,
        previous: MePin?,
    ): MePin {
        val heading = p.headingSource != "none" && p.headingDeg != null
        val state =
            when (p.source) {
                "bridged" -> "bridged"
                "stale" -> "stale"
                else -> "gps"
            }
        val jump = previous == null || metersBetween(previous.lat, previous.lon, p.lat, p.lon) > SNAP_M
        return MePin(
            lat = p.lat,
            lon = p.lon,
            accuracyM = if (p.uncertaintyM < MIN_CIRCLE_M) 0f else p.uncertaintyM.toFloat(),
            bearingDeg = p.headingDeg?.toFloat()?.takeIf { heading },
            spec = MarkerSpec.Me(heading, state),
            animateMs = if (p.snap || jump) 0L else GLIDE_MS,
        )
    }

    /** A pin straight from a raw fix (no game open, or the simulator). */
    fun at(
        lat: Double,
        lon: Double,
        accuracyM: Double?,
    ): MePin =
        MePin(lat, lon, accuracyM?.takeIf { it >= MIN_CIRCLE_M }?.toFloat() ?: 0f, null, MarkerSpec.Me(false, "gps"), 0L)

    /** Every pin image the style needs. */
    fun specs(): List<MarkerSpec.Me> = listOf(false, true).flatMap { h -> STATES.map { MarkerSpec.Me(h, it) } }

    private fun metersBetween(
        lat1: Double,
        lon1: Double,
        lat2: Double,
        lon2: Double,
    ): Double = hypot((lat2 - lat1) * METERS_PER_DEGREE, (lon2 - lon1) * METERS_PER_DEGREE * cos(Math.toRadians(lat1)))
}
```

`RawTrack.kt`: replace `RawLines.heading` with the four-field version (`"az"`, `"acc"`, `"pitch"`, `"roll"`) and add:

```kotlin
/** `SensorManager.SENSOR_STATUS_ACCURACY_*` (3 high, 2 medium, 1 low, 0 unreliable) as the core's names. */
internal object CompassAccuracies {
    fun name(sensorStatus: Int): String =
        when (sensorStatus) {
            3 -> "high"
            2 -> "medium"
            1 -> "low"
            else -> "unreliable"
        }
}
```

- [ ] **Step 5: Rotation vector, pin refresh, the map**

`Sensors.kt`:

```kotlin
    private var headingListener: SensorEventListener? = null
    private var headingAccuracy = 0

    /** While in a zone: the rotation vector at 5 Hz (batched 1 s) as azimuth, pitch and roll; [AppModel.onHeading] throttles to 2 Hz. */
    fun startHeading() {
        if (headingListener != null) return
        val sensor = sm.getDefaultSensor(Sensor.TYPE_ROTATION_VECTOR) ?: return Diag.warn(TAG, "no rotation vector sensor")
        val rot = FloatArray(ROTATION_MATRIX_SIZE)
        val ori = FloatArray(ORIENTATION_SIZE)
        val l =
            object : SensorEventListener {
                override fun onSensorChanged(e: SensorEvent) {
                    SensorManager.getRotationMatrixFromVector(rot, e.values)
                    SensorManager.getOrientation(rot, ori)
                    val eventMs = FixTime.wallMs(0L, e.timestamp, System.currentTimeMillis(), SystemClock.elapsedRealtimeNanos())
                    model.onHeading(Math.toDegrees(ori[0].toDouble()), Math.toDegrees(ori[1].toDouble()), Math.toDegrees(ori[2].toDouble()), headingAccuracy, eventMs)
                }

                override fun onAccuracyChanged(
                    s: Sensor?,
                    a: Int,
                ) {
                    headingAccuracy = a
                }
            }
        sm.registerListener(l, sensor, HEADING_PERIOD_US, HEADING_BATCH_US)
        headingListener = l
    }

    fun stopHeading() {
        headingListener?.let { sm.unregisterListener(it) }
        headingListener = null
    }
```

(`HEADING_PERIOD_US = 200_000`, `HEADING_BATCH_US = 1_000_000`, `ROTATION_MATRIX_SIZE = 9`, `ORIENTATION_SIZE = 3`.) In
`PresenceController.applyLocation` next to `startGnss`: `if (inZone) model.sensors.startHeading() else model.sensors.stopHeading()`.

`AppModel.kt`:

```kotlin
    /** The pin the Play map shows, from the core's `position()`; null before the first fix of the open game. */
    var pin by mutableStateOf<MePin?>(null)
        private set
    private val headingThrottle = Throttle(HEADING_MIN_MS)
    private val rawHeadingThrottle = Throttle(RAW_HEADING_MS)

    /** The player's pin on any map: the simulator, else the filtered position, else the raw fix (no game open). */
    val mePin: MePin?
        get() = simPos?.let { MePins.at(it.latitude, it.longitude, null) } ?: pin ?: realLoc?.let { MePins.at(it.latitude, it.longitude, it.accuracy.toDouble()) }
    val me: LatLng?
        get() = mePin?.let { LatLng(it.lat, it.lon) }

    /** Ask the core where to draw the player now (after every fix, step batch and heading). */
    fun refreshPin() {
        pin = if (engine.hasGame()) engine.position(now())?.let { MePins.from(it, pin) } else null
    }

    /** A compass reading: corrected to true north with the magnetic declination here, sent at most twice a second. */
    fun onHeading(
        azimuthDeg: Double,
        pitchDeg: Double,
        rollDeg: Double,
        sensorAccuracy: Int,
        eventMs: Long,
    ) {
        if (!engine.hasGame() || !headingThrottle.due(eventMs)) return
        val at = me ?: return
        val declination = android.hardware.GeomagneticField(at.latitude.toFloat(), at.longitude.toFloat(), 0f, eventMs).declination
        val trueAz = (azimuthDeg + declination).mod(FULL_CIRCLE_DEG)
        val acc = CompassAccuracies.name(sensorAccuracy)
        if (rawHeadingThrottle.due(eventMs)) Diag.raw("rawhead", RawLines.heading(trueAz, acc, pitchDeg, rollDeg))
        engine.onHeading(HeadingIn(azimuthDeg = trueAz, accuracy = acc, pitchDeg = pitchDeg, rollDeg = rollDeg, tMs = eventMs))
        refreshPin()
    }
```

(`HEADING_MIN_MS = 500L`, `RAW_HEADING_MS = 1_000L`, `FULL_CIRCLE_DEG = 360.0`; import `uniffi.apgo_ffi.HeadingIn`.) Call `refreshPin()` at the
end of `onFix` and `onSteps`, and in `refreshPlay` when no game is open (it sets `pin = null`). Remove the old `val me` getter.

`PresenceController.checkHomeOffer`: the fix comes from the estimate:

```kotlin
        val pos = if (model.simPos == null) model.engine.position(t) else null
        ...
                fix = pos?.let { GeoFix(it.estLat, it.estLon, it.uncertaintyM) } ?: loc?.let { GeoFix(it.latitude, it.longitude, it.accuracy.toDouble()) },
                fixAgeMs = pos?.ageMs ?: loc?.let { (SystemClock.elapsedRealtimeNanos() - it.elapsedRealtimeNanos) / NANOS_PER_MS },
```

`QuestMap.kt`: the parameter `me: LatLng?` becomes `me: MePin?`; `SyncPins` replaces `LaunchedEffect(style, me) { holder.show(MapSource.ME, ...) }`
with `LaunchedEffect(style, me) { holder.showMe(me) }`; `MapFraming` and `framePoints` take `me?.let { LatLng(it.lat, it.lon) }`. In
`MapHolder` add:

```kotlin
    // The player: MapLibre's location component in custom mode. We push the core's position and it interpolates between pushes (glide);
    // a reset, relocation or big jump arrives with animation 0 (snap). The accuracy circle is the component's own, in the palette colour.
    @SuppressLint("MissingPermission") // the component never asks Android for location (no default engine): positions come from the core
    fun showMe(pin: MePin?) {
        val m = map ?: return
        val s = style ?: return
        val lc = m.locationComponent
        if (!lc.isLocationComponentActivated) {
            MePins.specs().forEach { ensureImage(s, it.key) }
            val opts = LocationComponentActivationOptions.builder(view.context, s).useDefaultLocationEngine(false).locationComponentOptions(meOptions(MarkerSpec.Me(false, "gps"))).build()
            lc.activateLocationComponent(opts)
        }
        if (pin == null) {
            lc.isLocationComponentEnabled = false
            return
        }
        lc.applyStyle(meOptions(pin.spec))
        lc.isLocationComponentEnabled = true
        lc.renderMode = if (pin.bearingDeg != null) RenderMode.GPS else RenderMode.NORMAL
        lc.forceLocationUpdate(LocationUpdate.Builder().location(pin.toLocation()).animationDuration(pin.animateMs).build())
    }

    private fun meOptions(spec: MarkerSpec.Me): LocationComponentOptions =
        LocationComponentOptions
            .builder(view.context)
            .foregroundName(MarkerSpec.Me(false, spec.state).key)
            .backgroundName(MarkerSpec.Me(false, spec.state).key)
            .gpsName(MarkerSpec.Me(true, spec.state).key)
            .accuracyColor(ApgoPalette.me.toArgb())
            .accuracyAlpha(ME_ACCURACY_ALPHA)
            .elevation(0f)
            .build()
```

(imports from `org.maplibre.android.location.*` and `org.maplibre.android.location.modes.RenderMode`; `private const val ME_ACCURACY_ALPHA = 0.15f`;
`ensureImage` already renders any `MarkerSpec` key through `MapMarkers.parse`/`render`.) `MapStyle.kt`: delete `MapSource.ME` (and from
`ALL`), the `BADGE_ME` image, `ME_PIN_PX`, `ME_SIZE` and the `me-layer`. Call sites: `PlayScreen` passes `m.mePin`; `HomePicker` and `RealmEditor`
pass `m.mePin` to `QuestMap` and keep `m.me` (a `LatLng`) for their own logic.

- [ ] **Step 6: Run the tests and the gate**

Run: `ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS (Kover >= 18: `MePins`, `CompassAccuracies` and `MarkerSpec.Me` are covered).

- [ ] **Step 7: Check it on the emulator and the phone**

`just emu-start && just emu-run`, open a game, play a GPX route (`adb emu geo fix` or the emulator's route player): the pin glides between
fixes, the circle shows, the arrow follows the course, and the pin snaps after a teleport. Then on the phone (`just android-run`) stand
still with the phone flat: the compass arrow appears; put it in a pocket: no arrow. Screenshots (standing, walking, a corner) to
`/tmp/claude-screenshots/location-quality/`. If the component cannot show these images or glide, apply the spec's fallback (GeoJSON layer
animated per frame from `MePin`) and say so in the commit body.

- [ ] **Step 8: Commit**

```bash
git add android/app/src/main/java/dev/apgo2 android/app/src/test/java/dev/apgo2
git commit -m "feat: glide the map pin with heading and circle" -m "The pin is MapLibre's location component fed by position(): it glides
between fixes, snaps after a reset, shows the accuracy circle and an
arrow from the course or a flat, steady compass; hollow while bridged,
grey when stale. The home Wi-Fi offer uses the estimate." -m "Closes #86
Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 16: `Atlas::ways`: scans record simplified street geometry with shared nodes

**Files:**

- Modify: `core/src/geo.rs` (`simplify`, `simplify_pinned`), `core/src/fill.rs` (`RawWay`, `raw_ways`, `way_class_of`), `core/src/scan.rs`
  (`way_class`, `WayGeom`, `Atlas::ways`, `ways_from`, `scan_with`, `restrict_to`, `needs_rescan`, tests)

**Interfaces:**

- Produces: `geo::simplify(&[Point], tol_m) -> Vec<Point>`, `geo::simplify_pinned(&[Point], pinned: &[bool], tol_m) -> Vec<Point>`,
  `fill::RawWay { id: i64, class: u8, pts: Vec<Point> }`, `fill::raw_ways(body: &str) -> Result<Vec<RawWay>, Error>`,
  `fill::way_class_of(&BTreeMap<String, String>) -> u8`, `scan::way_class::{FOOT = 1, BIKE = 2, CAR = 4}`,
  `scan::WayGeom { id: i64, class: u8, pts: Vec<Point> }`, `Atlas::ways: Vec<WayGeom>` (`serde(default)`),
  `scan::ways_from(raw: Vec<RawWay>, zone: &Zone) -> Vec<WayGeom>`, `scan::WAY_SIMPLIFY_M = 2.0`.
- Plan choices: vertices shared by two ways (or a way's ends) are pinned and never simplified away, and simplification runs once over the
  whole scan after de-duplicating ways by OSM id (a junction can sit in another tile than the first copy of a way, so per-tile simplification
  would drop it). The scan only fetches walkable highways (`fill::WALKABLE`, up to `secondary`): Drive zones get the car-usable subset of those,
  not primary roads (listed under spec gaps). `needs_rescan()` now also asks for a rescan when streets exist but `ways` is empty (spec), so
  the existing test that set `street_runs` and expected no rescan sets `ways` too.

- [ ] **Step 1: Write the failing tests**

`core/src/geo.rs` tests:

```rust
    #[test]
    fn simplify_drops_points_on_a_straight_line_and_keeps_corners() {
        let o = Point::new(40.0, -111.0);
        let line: Vec<Point> = (0..=10).map(|i| destination(o, 90.0, 10.0 * f64::from(i))).collect();
        assert_eq!(simplify(&line, 2.0), vec![line[0], line[10]]);
        let corner = vec![o, destination(o, 90.0, 50.0), destination(destination(o, 90.0, 50.0), 0.0, 50.0)];
        assert_eq!(simplify(&corner, 2.0).len(), 3);
    }

    #[test]
    fn a_pinned_point_survives_simplification() {
        let o = Point::new(40.0, -111.0);
        let line: Vec<Point> = (0..=10).map(|i| destination(o, 90.0, 10.0 * f64::from(i))).collect();
        let mut pinned = vec![false; 11];
        pinned[4] = true;
        assert_eq!(simplify_pinned(&line, &pinned, 2.0), vec![line[0], line[4], line[10]]);
    }
```

`core/src/fill.rs` tests:

```rust
    #[test]
    fn way_classes_follow_who_may_use_the_way() {
        use crate::scan::way_class::{BIKE, CAR, FOOT};
        let t = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(way_class_of(&t(&[("highway", "residential")])), FOOT | BIKE | CAR);
        assert_eq!(way_class_of(&t(&[("highway", "footway")])), FOOT);
        assert_eq!(way_class_of(&t(&[("highway", "footway"), ("bicycle", "yes")])), FOOT | BIKE);
        assert_eq!(way_class_of(&t(&[("highway", "steps")])), FOOT);
        assert_eq!(way_class_of(&t(&[("highway", "cycleway")])), FOOT | BIKE);
        assert_eq!(way_class_of(&t(&[("highway", "cycleway"), ("foot", "no")])), BIKE);
        assert_eq!(way_class_of(&t(&[("highway", "service"), ("motor_vehicle", "no")])), FOOT | BIKE);
    }

    #[test]
    fn raw_ways_read_id_class_and_geometry() {
        let body = r#"{"elements":[{"type":"way","id":7,"tags":{"highway":"footway"},"geometry":[{"lat":40.0,"lon":-111.0},{"lat":40.001,"lon":-111.0}]},{"type":"node","id":1}]}"#;
        let w = raw_ways(body).unwrap();
        assert_eq!((w.len(), w[0].id, w[0].class, w[0].pts.len()), (1, 7, crate::scan::way_class::FOOT, 2));
        assert!(raw_ways("nope").is_err());
    }
```

`core/src/scan.rs` tests:

```rust
    fn raw_way(id: i64, pts: Vec<Point>) -> crate::fill::RawWay {
        crate::fill::RawWay { id, class: way_class::FOOT | way_class::BIKE, pts }
    }

    #[test]
    fn ways_share_the_exact_junction_point_and_are_kept_once() {
        let o = Point::new(40.0, -111.0);
        // a straight 200 m street with a node every 20 m; a side street starts at its 100 m node
        let main: Vec<Point> = (0..=10).map(|i| destination(o, 90.0, 20.0 * f64::from(i))).collect();
        let side = vec![main[5], destination(main[5], 0.0, 80.0)];
        let zone = Zone::Circle { center: o, radius_m: 1000.0 };
        let ways = ways_from(vec![raw_way(1, main.clone()), raw_way(2, side), raw_way(1, main)], &zone);
        assert_eq!(ways.len(), 2, "way 1 came from two tiles");
        assert_eq!(ways[0].pts.len(), 3, "start, the junction (pinned), end");
        assert!(ways[1].pts.contains(&ways[0].pts[1]), "the junction is the very same rounded point in both ways");
    }

    #[test]
    fn ways_outside_the_zone_are_dropped_and_restrict_to_drops_them_too() {
        let o = Point::new(40.0, -111.0);
        let far = destination(o, 0.0, 5000.0);
        let zone = Zone::Circle { center: o, radius_m: 500.0 };
        let ways = ways_from(vec![raw_way(1, vec![o, destination(o, 90.0, 50.0)]), raw_way(2, vec![far, destination(far, 90.0, 50.0)])], &zone);
        assert_eq!(ways.iter().map(|w| w.id).collect::<Vec<_>>(), [1]);
        let mut a = Atlas { ways: vec![WayGeom { id: 2, class: 1, pts: vec![far, destination(far, 90.0, 50.0)] }], ..Atlas::default() };
        a.restrict_to(&zone);
        assert!(a.ways.is_empty());
    }

    #[test]
    fn an_atlas_saved_before_ways_loads_and_asks_for_a_rescan() {
        let a: Atlas = serde_json::from_str(r#"{"realm_id":"r","scanned_at_ms":0,"features":[],"streets":[{"lat":40.0,"lon":-111.0}],"street_runs":[1],"matches":{}}"#).unwrap();
        assert!(a.ways.is_empty() && a.needs_rescan());
    }

    #[test]
    fn a_scan_records_the_ways_it_fetched() {
        let cat = Catalog::builtin();
        let o = Point::new(40.0095, -111.0);
        let body = format!(
            r#"{{"elements":[{{"type":"way","id":77,"tags":{{"highway":"residential"}},"geometry":[{{"lat":{},"lon":{}}},{{"lat":{},"lon":{}}}]}}]}}"#,
            o.lat,
            o.lon,
            destination(o, 0.0, 300.0).lat,
            destination(o, 0.0, 300.0).lon
        );
        let fetch = |q: &str, _: usize, _: Option<Instant>| -> Result<String, Error> {
            Ok(if q.contains("\"highway\"~") && q.contains("out geom qt") { body.clone() } else { r#"{"elements":[]}"#.to_string() })
        };
        let a = scan_with(&small_realm(o, 1000.0), &cat, 0, &fetch, &quick(), Instant::now() + Duration::from_secs(30), &|_, _| {}).unwrap();
        assert_eq!(a.ways.len(), 1);
        assert_eq!((a.ways[0].id, a.ways[0].class), (77, way_class::FOOT | way_class::BIKE | way_class::CAR));
        assert!(!a.needs_rescan());
    }
```

In `restricting_splits_street_runs_and_an_old_atlas_has_no_links_and_needs_a_rescan`, change

```rust
        a.street_runs = vec![5];
        assert!(!a.needs_rescan());
```

to

```rust
        a.street_runs = vec![5];
        assert!(a.needs_rescan(), "runs but no way geometry: the street graph needs a rescan");
        a.ways = vec![WayGeom { id: 1, class: way_class::FOOT, pts: line.clone() }];
        assert!(!a.needs_rescan());
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core geo:: fill:: scan::`
Expected: FAIL to compile: `cannot find function 'simplify'`, `cannot find function 'way_class_of'`, `no field 'ways' on type 'Atlas'`.

- [ ] **Step 3: Implement**

`core/src/geo.rs`:

```rust
/// Douglas-Peucker: the points of `pts` needed to keep the line within `tol_m` metres (both ends always kept).
#[must_use]
pub fn simplify(pts: &[Point], tol_m: f64) -> Vec<Point> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let far = (a + 1..b).map(|i| (i, distance_to_segment_m(pts[i], pts[a], pts[b]))).max_by(|x, y| x.1.total_cmp(&y.1));
        if let Some((i, d)) = far.filter(|(_, d)| *d > tol_m) {
            let _ = d;
            keep[i] = true;
            stack.push((a, i));
            stack.push((i, b));
        }
    }
    pts.iter().zip(&keep).filter(|(_, k)| **k).map(|(p, _)| *p).collect()
}

/// [`simplify`] that never drops a point marked in `pinned` (a junction shared with another way).
#[must_use]
pub fn simplify_pinned(pts: &[Point], pinned: &[bool], tol_m: f64) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    let mut start = 0;
    for i in 1..pts.len() {
        if i == pts.len() - 1 || pinned.get(i).copied().unwrap_or(false) {
            let piece = simplify(&pts[start..=i], tol_m);
            out.extend(piece.into_iter().skip(usize::from(!out.is_empty())));
            start = i;
        }
    }
    if out.is_empty() {
        out.extend(pts.iter().copied());
    }
    out
}
```

(Replace the `if let Some((i, d)) = far.filter(..) { let _ = d; ...` with `if let Some((i, _)) = far.filter(|(_, d)| *d > tol_m) {`.)

`core/src/fill.rs`:

```rust
/// One street or path as the map server returned it.
#[derive(Debug, Clone, PartialEq)]
pub struct RawWay {
    /// OSM way id.
    pub id: i64,
    /// [`crate::scan::way_class`] bits.
    pub class: u8,
    /// Every point of the way.
    pub pts: Vec<Point>,
}

/// Who may use a way: on foot unless `foot=no`; by bike except steps and footways or pedestrian streets without `bicycle=yes`; by car on
/// the street kinds the scan fetches that carry traffic.
#[must_use]
pub fn way_class_of(tags: &std::collections::BTreeMap<String, String>) -> u8 {
    use crate::scan::way_class::{BIKE, CAR, FOOT};
    let tag = |k: &str| tags.get(k).map_or("", String::as_str);
    let hw = tag("highway");
    let foot = if tag("foot") == "no" { 0 } else { FOOT };
    let bike_ok = matches!(tag("bicycle"), "yes" | "designated" | "permissive");
    let bike = match hw {
        "steps" => 0,
        "footway" | "pedestrian" if !bike_ok => 0,
        _ if tag("bicycle") == "no" => 0,
        _ => BIKE,
    };
    let car = if matches!(hw, "residential" | "living_street" | "service" | "unclassified" | "tertiary" | "secondary") && tag("motor_vehicle") != "no" { CAR } else { 0 };
    foot | bike | car
}

/// Every way of a streets response with its class and full geometry.
///
/// # Errors
/// Returns [`Error::Parse`] if the body is not JSON or has no `elements`.
pub fn raw_ways(body: &str) -> Result<Vec<RawWay>, Error> {
    let v: Value = serde_json::from_str(body).map_err(|e| Error::Parse(e.to_string()))?;
    let elements = v.get("elements").and_then(Value::as_array).ok_or_else(|| Error::Parse("no elements".into()))?;
    Ok(elements
        .iter()
        .filter_map(|e| {
            let id = e.get("id").and_then(Value::as_i64)?;
            let geom = e.get("geometry").and_then(Value::as_array)?;
            let tags: std::collections::BTreeMap<String, String> =
                e.get("tags").and_then(Value::as_object).map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default();
            let pts: Vec<Point> = geom.iter().filter_map(|g| Some(Point::new(g.get("lat")?.as_f64()?, g.get("lon")?.as_f64()?))).collect();
            (pts.len() >= 2).then(|| RawWay { id, class: way_class_of(&tags), pts })
        })
        .collect())
}
```

`core/src/scan.rs` (near `NamedStreet`):

```rust
/// Who may use a way: bits of [`WayGeom::class`].
pub mod way_class {
    /// On foot.
    pub const FOOT: u8 = 1;
    /// By bike.
    pub const BIKE: u8 = 2;
    /// By car.
    pub const CAR: u8 = 4;
}

/// Ways are simplified to this tolerance, metres.
pub const WAY_SIMPLIFY_M: f64 = 2.0;

/// One street or path as simplified geometry for the location filter's street graph. Points are rounded to 1e-7 degrees, so ways that
/// share an OSM node share the exact point.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WayGeom {
    /// OSM way id.
    pub id: i64,
    /// [`way_class`] bits.
    pub class: u8,
    /// The way's points.
    pub pts: Vec<Point>,
}

fn round7(x: f64) -> f64 {
    crate::num::i64_to_f64(crate::num::round_i64(x * 1e7)) / 1e7
}

/// The ways of a whole scan: each OSM way once, kept when any point is in `zone`, simplified with every shared vertex (and each end)
/// pinned, rounded.
#[must_use]
pub fn ways_from(raw: Vec<crate::fill::RawWay>, zone: &Zone) -> Vec<WayGeom> {
    let mut seen = BTreeSet::new();
    let raw: Vec<crate::fill::RawWay> = raw.into_iter().filter(|w| seen.insert(w.id)).collect();
    let key = |p: &Point| (crate::num::round_i64(p.lat * 1e7), crate::num::round_i64(p.lon * 1e7));
    let mut uses: BTreeMap<(i64, i64), u32> = BTreeMap::new();
    for w in &raw {
        for (i, p) in w.pts.iter().enumerate() {
            *uses.entry(key(p)).or_insert(0) += if i == 0 || i + 1 == w.pts.len() { 2 } else { 1 };
        }
    }
    raw.into_iter()
        .filter(|w| w.pts.iter().any(|p| zone.contains(*p)))
        .map(|w| {
            let pinned: Vec<bool> = w.pts.iter().map(|p| uses.get(&key(p)).copied().unwrap_or(0) >= 2).collect();
            let pts = crate::geo::simplify_pinned(&w.pts, &pinned, WAY_SIMPLIFY_M).into_iter().map(|p| Point::new(round7(p.lat), round7(p.lon))).collect();
            WayGeom { id: w.id, class: w.class, pts }
        })
        .collect()
}
```

`Atlas` gains (after `rough_runs`):

```rust
    /// Street and path geometry (simplified, shared nodes kept) for the location filter's street graph. Empty for scans made before it
    /// was recorded, see [`Self::needs_rescan`].
    #[serde(default)]
    pub ways: Vec<WayGeom>,
```

`restrict_to` adds `self.ways.retain(|w| w.pts.iter().any(|p| zone.contains(*p)));`. `needs_rescan` becomes:

```rust
    pub fn needs_rescan(&self) -> bool {
        let has_streets = !self.streets.is_empty() || !self.streets_rough.is_empty();
        (self.street_runs.is_empty() && !self.streets.is_empty()) || (self.rough_runs.is_empty() && !self.streets_rough.is_empty()) || (has_streets && self.ways.is_empty())
    }
```

(and its doc gains: "or before way geometry was recorded (the map pin then matches streets poorly)"). In `scan_with` declare
`let mut raw_ways = Vec::new();`, in the `Job::Streets` arm add `raw_ways.extend(crate::fill::raw_ways(&body).unwrap_or_default());`, and
before `Ok(atlas)` set `atlas.ways = ways_from(raw_ways, &zone);`. `build_atlas` sets `ways: vec![]`.

- [ ] **Step 4: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core`
Expected: PASS.
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/geo.rs core/src/fill.rs core/src/scan.rs
git commit -m "feat: record street geometry in scans" -m "Atlas::ways keeps every scanned street once, simplified to 2 m with
shared junction nodes pinned and rounded so ways meet exactly; old
atlases load without it and ask for a rescan." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 17: `StreetGraph` (full and degraded), attached to the game

**Files:**

- Create: `core/src/loc/graph.rs`
- Modify: `core/src/loc/mod.rs` (`pub mod graph;`), `core/src/loc/locator.rs` (`graph` field, `set_graph`, `graph()`), `core/src/game.rs`
  (`attach_streets`, `create`), `core/src/loc/bench/synth.rs` (`grid_ways`), `core/src/loc/bench.rs` (re-export)

**Interfaces:**

- Consumes: `WayGeom`, `way_class`, `Atlas::{ways, street_links}` (Task 16); `Frame` (Task 2).
- Produces:
  - `graph::Segment { a: usize, b: usize, len_m: f64, class: u8 }`, `graph::Cand { seg: usize, off_m: f64, d_m: f64 }`.
  - `graph::StreetGraph` (`Debug`): `from_ways(&[WayGeom]) -> Option<Self>`, `degraded(&[(Point, Point)]) -> Option<Self>`,
    `for_atlases(&[&Atlas]) -> Option<Self>`, `is_degraded()`, `frame()`, `segment_count()`, `seg(id) -> &Segment`, `node_en(id) -> [f64; 2]`,
    `en_at(seg, off_m) -> [f64; 2]`, `geo_at(seg, off_m) -> Point`, `bearing_deg(seg, dir: f64) -> f64` (dir +1 = from `a` to `b`),
    `leaving(node, mask) -> Vec<(usize, f64)>` (segment, dir away from the node), `candidates(p: Point, radius_m, max, mask) -> Vec<Cand>`
    (nearest first), `dijkstra(from_node, limit_m, mask) -> HashMap<usize, f64>`.
  - `graph::mode_mask(Mode) -> u8`, `graph::JOIN_M = 12.0`, `graph::CELL_M = 30.0`.
  - `Locator::set_graph(Option<Arc<StreetGraph>>)`, `Locator::graph() -> Option<&Arc<StreetGraph>>`.
  - `bench::grid_ways(origin: Point, n: usize, spacing_m: f64) -> Vec<WayGeom>` (an n x n street grid east and north of `origin`, every
    crossing a shared node, all classes).
- Plan choice: `set_graph` takes an `Option` (a realm can have no streets at all). The graph is built in `Game::create` and in
  `Game::attach_streets` (called by the engine when a saved game is opened), so new and loaded games both get it.

- [ ] **Step 1: Write the failing tests** (bottom of `core/src/loc/graph.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;
    use crate::scan::way_class::{BIKE, CAR, FOOT};

    fn o() -> Point {
        Point::new(40.0, -111.0)
    }

    fn way(id: i64, class: u8, pts: Vec<Point>) -> WayGeom {
        WayGeom { id, class, pts }
    }

    fn tee() -> StreetGraph {
        let j = destination(o(), 90.0, 100.0);
        StreetGraph::from_ways(&[way(1, FOOT | BIKE | CAR, vec![o(), j, destination(o(), 90.0, 200.0)]), way(2, FOOT, vec![j, destination(j, 0.0, 100.0)])]).unwrap()
    }

    #[test]
    fn ways_sharing_a_point_share_a_node() {
        let g = tee();
        assert_eq!(g.segment_count(), 3);
        assert_eq!(g.leaving(1, FOOT).len(), 3, "the junction joins three segments");
        assert_eq!(g.leaving(1, CAR).len(), 2, "the footway is not for cars");
    }

    #[test]
    fn candidates_are_projections_nearest_first_filtered_by_mode() {
        let g = tee();
        let p = destination(destination(o(), 90.0, 100.0), 0.0, 6.0); // 6 m up the footway, on the main street's junction too
        let c = g.candidates(p, 30.0, 8, FOOT);
        assert!(c.len() >= 2 && c[0].d_m <= c[1].d_m);
        assert!(c[0].d_m < 0.5, "on the footway: {c:?}");
        let cars = g.candidates(p, 30.0, 8, CAR);
        assert!(cars.iter().all(|x| g.seg(x.seg).class & CAR != 0) && (cars[0].d_m - 6.0).abs() < 0.5);
    }

    #[test]
    fn dijkstra_follows_the_streets() {
        let g = tee();
        let d = g.dijkstra(0, 1000.0, FOOT);
        assert!((d[&3] - 200.0).abs() < 0.5, "from the west end to the footway's end: 100 + 100 m, {d:?}");
        assert!(g.dijkstra(0, 50.0, FOOT).get(&3).is_none(), "bounded");
    }

    #[test]
    fn bearings_point_along_the_segment() {
        let g = tee();
        assert!((g.bearing_deg(0, 1.0) - 90.0).abs() < 0.5 && (g.bearing_deg(0, -1.0) - 270.0).abs() < 0.5);
    }

    #[test]
    fn a_degraded_graph_joins_run_ends_within_twelve_metres_only() {
        let a = [(o(), destination(o(), 90.0, 60.0))];
        let near = destination(o(), 90.0, 68.0);
        let far = destination(o(), 90.0, 90.0);
        let g = StreetGraph::degraded(&[a[0], (near, destination(near, 90.0, 60.0)), (far, destination(far, 0.0, 60.0))]).unwrap();
        assert!(g.is_degraded());
        assert!(g.dijkstra(0, 1000.0, FOOT).len() >= 4, "the 8 m gap is joined");
        let d = g.dijkstra(0, 1000.0, FOOT);
        assert!(d.values().all(|x| *x < 140.0), "the run 30 m away is not reachable");
    }

    #[test]
    fn atlases_without_ways_get_the_degraded_graph_and_empty_ones_none() {
        let with = crate::scan::Atlas { ways: vec![way(1, FOOT, vec![o(), destination(o(), 0.0, 50.0)])], ..crate::scan::Atlas::default() };
        assert!(!StreetGraph::for_atlases(&[&with]).unwrap().is_degraded());
        let pts: Vec<Point> = (0..3).map(|i| destination(o(), 90.0, 60.0 * f64::from(i))).collect();
        let old = crate::scan::Atlas { streets: pts, street_runs: vec![3], street_stride: 1, ..crate::scan::Atlas::default() };
        assert!(StreetGraph::for_atlases(&[&with, &old]).unwrap().is_degraded());
        assert!(StreetGraph::for_atlases(&[&crate::scan::Atlas::default()]).is_none());
    }

    #[test]
    fn a_synthetic_grid_is_connected() {
        let g = StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 5, 100.0)).unwrap();
        assert_eq!(g.segment_count(), 2 * 5 * 4);
        assert_eq!(g.dijkstra(0, 10_000.0, FOOT).len(), 25);
    }

    #[test]
    #[ignore = "timing harness: cargo test --release -p apgo-core graph_build_time -- --ignored --nocapture"]
    fn graph_build_time() {
        let ways = crate::loc::bench::grid_ways(o(), 71, 40.0); // about 5000 vertices
        let t = std::time::Instant::now();
        let g = StreetGraph::from_ways(&ways).unwrap();
        println!("{} segments in {:?}", g.segment_count(), t.elapsed());
    }
}
```

In `core/src/game.rs` tests:

```rust
    #[test]
    fn a_new_game_has_a_street_graph_for_its_filter() {
        let g = game(&reach_only(&[Mode::Walk], 10, "all_trips"), Backend::Solo, 4);
        assert!(g.locator.graph().is_some());
    }
```

(Check `realm()` in the test module builds an atlas with streets; if its atlas has no streets, give it a short street run so the graph
is not empty.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc::graph game::`
Expected: FAIL to compile: `file not found for module 'graph'` / `no method named 'graph'`.

- [ ] **Step 3: Write `core/src/loc/graph.rs`** (above the tests)

```rust
//! The street graph the map matcher and the gap bridge walk on: built from the scans' way geometry (`Atlas::ways`), or, for atlases
//! scanned before that was recorded, a degraded graph from the street sample links with run ends joined within 12 m.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::catalog::Mode;
use crate::geo::Point;
use crate::loc::frame::Frame;
use crate::num::{floor_i64, round_i64, round_u64};
use crate::scan::{way_class, Atlas, WayGeom};

/// Grid cell size of the segment index, metres.
pub const CELL_M: f64 = 30.0;
/// A degraded graph joins a run end to any node this close, metres.
pub const JOIN_M: f64 = 12.0;

/// The way classes a travel mode may use.
#[must_use]
pub fn mode_mask(mode: Mode) -> u8 {
    match mode {
        Mode::Walk | Mode::Run => way_class::FOOT,
        Mode::Bike => way_class::BIKE,
        Mode::Drive => way_class::CAR,
    }
}

/// A straight piece of street between two nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// First node.
    pub a: usize,
    /// Second node.
    pub b: usize,
    /// Length, metres.
    pub len_m: f64,
    /// [`way_class`] bits.
    pub class: u8,
}

/// A point projected onto a segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cand {
    /// The segment.
    pub seg: usize,
    /// Metres from the segment's node `a`.
    pub off_m: f64,
    /// Distance from the point to the projection, metres.
    pub d_m: f64,
}

/// Nodes, segments and a grid index, in a local metric frame.
#[derive(Debug, Clone)]
pub struct StreetGraph {
    frame: Frame,
    nodes: Vec<[f64; 2]>,
    segs: Vec<Segment>,
    adj: Vec<Vec<usize>>,
    grid: HashMap<(i64, i64), Vec<usize>>,
    degraded: bool,
}

fn cell(en: [f64; 2]) -> (i64, i64) {
    (floor_i64(en[0] / CELL_M), floor_i64(en[1] / CELL_M))
}

fn key(p: Point) -> (i64, i64) {
    (round_i64(p.lat * 1e7), round_i64(p.lon * 1e7))
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl StreetGraph {
    fn empty(origin: Point, degraded: bool) -> Self {
        Self { frame: Frame::new(origin), nodes: vec![], segs: vec![], adj: vec![], grid: HashMap::new(), degraded }
    }

    fn node(&mut self, keys: &mut HashMap<(i64, i64), usize>, p: Point) -> usize {
        if let Some(&n) = keys.get(&key(p)) {
            return n;
        }
        self.nodes.push(self.frame.to_enu(p));
        self.adj.push(vec![]);
        keys.insert(key(p), self.nodes.len() - 1);
        self.nodes.len() - 1
    }

    fn add_seg(&mut self, a: usize, b: usize, class: u8) {
        if a == b {
            return;
        }
        let (pa, pb) = (self.nodes[a], self.nodes[b]);
        let id = self.segs.len();
        self.segs.push(Segment { a, b, len_m: dist(pa, pb), class });
        self.adj[a].push(id);
        self.adj[b].push(id);
        let (c0, c1) = (cell([pa[0].min(pb[0]), pa[1].min(pb[1])]), cell([pa[0].max(pb[0]), pa[1].max(pb[1])]));
        for x in c0.0..=c1.0 {
            for y in c0.1..=c1.1 {
                self.grid.entry((x, y)).or_default().push(id);
            }
        }
    }

    /// The graph of `ways` (each consecutive pair of points a segment; equal points are one node). `None` without ways.
    #[must_use]
    pub fn from_ways(ways: &[WayGeom]) -> Option<Self> {
        let origin = *ways.iter().find_map(|w| w.pts.first())?;
        let mut g = Self::empty(origin, false);
        let mut keys = HashMap::new();
        for w in ways {
            let ids: Vec<usize> = w.pts.iter().map(|p| g.node(&mut keys, *p)).collect();
            for pair in ids.windows(2) {
                g.add_seg(pair[0], pair[1], w.class);
            }
        }
        Some(g)
    }

    /// A degraded graph from street sample links (old atlases): each link a segment usable by every mode, and every run end joined to the
    /// nearest other node within [`JOIN_M`]. `None` without links.
    #[must_use]
    pub fn degraded(links: &[(Point, Point)]) -> Option<Self> {
        let all = way_class::FOOT | way_class::BIKE | way_class::CAR;
        let origin = links.first()?.0;
        let mut g = Self::empty(origin, true);
        let mut keys = HashMap::new();
        for (p, q) in links {
            let (a, b) = (g.node(&mut keys, *p), g.node(&mut keys, *q));
            g.add_seg(a, b, all);
        }
        let mut by_cell: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, n) in g.nodes.iter().enumerate() {
            by_cell.entry(cell(*n)).or_default().push(i);
        }
        let ends: Vec<usize> = (0..g.nodes.len()).filter(|&i| g.adj[i].len() == 1).collect();
        for e in ends {
            let pe = g.nodes[e];
            let (cx, cy) = cell(pe);
            let neighbours: Vec<usize> = g.adj[e].iter().map(|&s| if g.segs[s].a == e { g.segs[s].b } else { g.segs[s].a }).collect();
            let best = (cx - 1..=cx + 1)
                .flat_map(|x| (cy - 1..=cy + 1).map(move |y| (x, y)))
                .filter_map(|c| by_cell.get(&c))
                .flatten()
                .copied()
                .filter(|&n| n != e && !neighbours.contains(&n))
                .map(|n| (n, dist(pe, g.nodes[n])))
                .filter(|(_, d)| *d <= JOIN_M)
                .min_by(|x, y| x.1.total_cmp(&y.1));
            if let Some((n, _)) = best {
                g.add_seg(e, n, all);
            }
        }
        Some(g)
    }

    /// The graph of a game's zones: from their ways when every atlas has them, else degraded from their street links. `None` without streets.
    #[must_use]
    pub fn for_atlases(atlases: &[&Atlas]) -> Option<Self> {
        if atlases.iter().all(|a| !a.ways.is_empty()) {
            let mut seen = std::collections::BTreeSet::new();
            let ways: Vec<WayGeom> = atlases.iter().flat_map(|a| a.ways.iter()).filter(|w| seen.insert(w.id)).cloned().collect();
            return Self::from_ways(&ways);
        }
        let links: Vec<(Point, Point)> = atlases.iter().flat_map(|a| a.street_links(false).into_iter().chain(a.street_links(true))).collect();
        Self::degraded(&links)
    }

    /// Whether this is a degraded graph (matching confidence is capped).
    #[must_use]
    pub fn is_degraded(&self) -> bool {
        self.degraded
    }

    /// The graph's metric frame.
    #[must_use]
    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Number of segments.
    #[must_use]
    pub fn segment_count(&self) -> usize {
        self.segs.len()
    }

    /// A segment.
    #[must_use]
    pub fn seg(&self, id: usize) -> &Segment {
        &self.segs[id]
    }

    /// A node's position in the frame.
    #[must_use]
    pub fn node_en(&self, id: usize) -> [f64; 2] {
        self.nodes[id]
    }

    /// The point `off_m` metres from node `a` along a segment, in the frame.
    #[must_use]
    pub fn en_at(&self, seg: usize, off_m: f64) -> [f64; 2] {
        let s = self.segs[seg];
        let (a, b) = (self.nodes[s.a], self.nodes[s.b]);
        let t = if s.len_m > 0.0 { (off_m / s.len_m).clamp(0.0, 1.0) } else { 0.0 };
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
    }

    /// [`Self::en_at`] as a map point.
    #[must_use]
    pub fn geo_at(&self, seg: usize, off_m: f64) -> Point {
        self.frame.to_geo(self.en_at(seg, off_m))
    }

    /// Bearing of travel along a segment (`dir` +1 from `a` to `b`, -1 back), degrees from north.
    #[must_use]
    pub fn bearing_deg(&self, seg: usize, dir: f64) -> f64 {
        let s = self.segs[seg];
        let (a, b) = (self.nodes[s.a], self.nodes[s.b]);
        let (de, dn) = if dir >= 0.0 { (b[0] - a[0], b[1] - a[1]) } else { (a[0] - b[0], a[1] - b[1]) };
        de.atan2(dn).to_degrees().rem_euclid(360.0)
    }

    /// The segments at `node` usable with `mask`, each with the direction that leads away from the node.
    #[must_use]
    pub fn leaving(&self, node: usize, mask: u8) -> Vec<(usize, f64)> {
        self.adj[node].iter().filter(|&&s| self.segs[s].class & mask != 0).map(|&s| (s, if self.segs[s].a == node { 1.0 } else { -1.0 })).collect()
    }

    /// Projections of `p` onto the segments within `radius_m` usable with `mask`, nearest first, at most `max` (one per segment).
    #[must_use]
    pub fn candidates(&self, p: Point, radius_m: f64, max: usize, mask: u8) -> Vec<Cand> {
        let en = self.frame.to_enu(p);
        let (c0, c1) = (cell([en[0] - radius_m, en[1] - radius_m]), cell([en[0] + radius_m, en[1] + radius_m]));
        let mut segs: Vec<usize> = (c0.0..=c1.0).flat_map(|x| (c0.1..=c1.1).map(move |y| (x, y))).filter_map(|c| self.grid.get(&c)).flatten().copied().collect();
        segs.sort_unstable();
        segs.dedup();
        let mut out: Vec<Cand> = segs
            .into_iter()
            .filter(|&s| self.segs[s].class & mask != 0)
            .map(|s| {
                let sg = self.segs[s];
                let (a, b) = (self.nodes[sg.a], self.nodes[sg.b]);
                let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                let len2 = dx * dx + dy * dy;
                let t = if len2 > 0.0 { (((en[0] - a[0]) * dx + (en[1] - a[1]) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
                let q = [a[0] + dx * t, a[1] + dy * t];
                Cand { seg: s, off_m: t * sg.len_m, d_m: dist(en, q) }
            })
            .filter(|c| c.d_m <= radius_m)
            .collect();
        out.sort_by(|x, y| x.d_m.total_cmp(&y.d_m));
        out.truncate(max);
        out
    }

    /// Shortest street distances from `from` to every node within `limit_m` (segments usable with `mask`).
    #[must_use]
    pub fn dijkstra(&self, from: usize, limit_m: f64, mask: u8) -> HashMap<usize, f64> {
        let mut best: HashMap<usize, f64> = HashMap::new();
        let mut heap = BinaryHeap::new();
        heap.push(Reverse((0_u64, from)));
        while let Some(Reverse((mm, n))) = heap.pop() {
            let d = crate::num::i64_to_f64(i64::try_from(mm).unwrap_or(i64::MAX)) / 1000.0;
            if d > limit_m || best.contains_key(&n) {
                continue;
            }
            best.insert(n, d);
            for &s in self.adj[n].iter().filter(|&&s| self.segs[s].class & mask != 0) {
                let sg = self.segs[s];
                let m = if sg.a == n { sg.b } else { sg.a };
                if !best.contains_key(&m) {
                    heap.push(Reverse((round_u64((d + sg.len_m) * 1000.0), m)));
                }
            }
        }
        best
    }
}
```

(`Reverse` around `(u64, usize)` keeps the heap a min-heap without floats.)

`core/src/loc/bench/synth.rs`:

```rust
/// An `n` x `n` grid of streets `spacing_m` apart, east and north of `origin`: every crossing is a shared node, every class may use it.
#[must_use]
pub fn grid_ways(origin: Point, n: usize, spacing_m: f64) -> Vec<crate::scan::WayGeom> {
    use crate::scan::way_class::{BIKE, CAR, FOOT};
    let at = |i: usize, j: usize| {
        let p = destination(destination(origin, 90.0, spacing_m * crate::num::count_f64(i)), 0.0, spacing_m * crate::num::count_f64(j));
        Point::new((p.lat * 1e7).round() / 1e7, (p.lon * 1e7).round() / 1e7)
    };
    let mut out = Vec::new();
    for k in 0..n {
        let id = i64::try_from(k).unwrap_or(0);
        out.push(crate::scan::WayGeom { id: 2 * id, class: FOOT | BIKE | CAR, pts: (0..n).map(|i| at(i, k)).collect() });
        out.push(crate::scan::WayGeom { id: 2 * id + 1, class: FOOT | BIKE | CAR, pts: (0..n).map(|j| at(k, j)).collect() });
    }
    out
}
```

(re-export from `bench.rs`). Caution: a grid crossing must be the same rounded point in both ways; computing it once per `(i, j)` with the
same expression guarantees that.

`locator.rs`: field `graph: Option<Arc<StreetGraph>>` (init `None`), and

```rust
    /// The street graph of the game's zones (map matching and gap bridging use it); `None` without streets.
    pub fn set_graph(&mut self, graph: Option<Arc<StreetGraph>>) {
        self.graph = graph;
    }

    /// The street graph, if any.
    #[must_use]
    pub fn graph(&self) -> Option<&Arc<StreetGraph>> {
        self.graph.as_ref()
    }
```

`game.rs`: in `attach_streets` add `self.locator.set_graph(StreetGraph::for_atlases(atlases).map(Arc::new));`; in `create` build
`let atlases: Vec<&Atlas> = zones.iter().map(|z| z.atlas).collect();`, use it for `street_index(&atlases)`, and after building `Self` (bind it
to `let mut g = Self { .. };`) call `g.locator.set_graph(StreetGraph::for_atlases(&atlases).map(Arc::new)); Ok(g)`.

- [ ] **Step 4: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core && cargo test --release -p apgo-core graph_build_time -- --ignored --nocapture`
Expected: PASS; the timing prints well under 300 ms for about 5000 vertices.
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src
git commit -m "feat: build a street graph for the filter" -m "StreetGraph from Atlas::ways (shared nodes), or degraded from street
links with run ends joined within 12 m for old scans: grid-indexed
candidates by mode and bounded Dijkstra. Built for new and opened
games." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 18: The HMM matcher

**Files:**

- Create: `core/src/loc/matcher.rs`
- Modify: `core/src/loc/mod.rs` (`pub mod matcher;`), `core/src/loc/params.rs` (matcher group)

**Interfaces:**

- Consumes: `StreetGraph`, `Cand`, `mode_mask` (Task 17); `Estimate` (Task 2); `LocParams`.
- Produces: `matcher::MatchOut { point: Point, confidence: f64, seg: Option<usize> }`, `matcher::Matcher` (`Default`, `Clone`):
  `new(Option<Arc<StreetGraph>>, mask)`, `set_graph`, `set_mask`, `restart()`, `push(&Estimate, &LocParams) -> Option<MatchOut>`,
  `best() -> Option<MatchOut>`, `trace() -> &[Point]`, `trace_from_ms() -> Option<i64>`;
  `matcher::street_transition_log(d_gc, d_route, beta, p_off) -> f64`.
- `LocParams` additions (spec values): `match_min_move_m: 5.0`, `match_min_gap_ms: 5000`, `match_sigma_floor_m: 4.07`,
  `match_max_radius_m: 50.0`, `match_max_candidates: 8`, `match_off_road_m: 20.0`, `match_beta_m: 5.0`, `match_uturn_factor: 0.2`,
  `match_to_off_per_s: 0.02`, `match_to_on_per_s: 0.05`, `match_lag: 3`, `match_route_limit_m: 300.0`, `match_degraded_cap: 0.5`,
  `match_show_confidence: 0.7`, `match_show_min_m: 10.0`.
- Plan choices: the "off to street" probability is shared equally among the street candidates of the new input; the bounded Dijkstra
  caches single-source maps per node up to `match_route_limit_m` (cleared beyond 256 entries) and still applies the spec's per-pair bound
  `2 d_gc + 50 m`; without a graph the matcher only builds the trace from the estimates (same 5 m / 5 s input rule).

- [ ] **Step 1: Write the failing tests** (bottom of `core/src/loc/matcher.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;
    use crate::loc::bench::grid_ways;
    use crate::loc::graph::mode_mask;
    use crate::loc::Estimate;
    use crate::catalog::Mode;

    fn o() -> Point {
        Point::new(40.0, -111.0)
    }

    fn grid() -> Arc<StreetGraph> {
        Arc::new(StreetGraph::from_ways(&grid_ways(o(), 5, 100.0)).unwrap())
    }

    fn est(p: Point, t_s: i64) -> Estimate {
        Estimate { uncertainty_m: 5.0, speed_mps: 1.4, ..Estimate::exact(p.lat, p.lon, t_s * 1000) }
    }

    fn walk(m: &mut Matcher, from: Point, bearing: f64, side_m: f64, n: i64, t0: i64) -> Option<MatchOut> {
        let p = LocParams::default();
        let mut out = None;
        for i in 0..n {
            let on = destination(from, bearing, 7.0 * crate::num::i64_to_f64(i));
            out = m.push(&est(destination(on, bearing + 90.0, side_m), t0 + i * 5), &p);
        }
        out
    }

    #[test]
    fn a_walk_beside_a_street_is_matched_onto_it_with_confidence() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let out = walk(&mut m, destination(o(), 0.0, 100.0), 90.0, 4.0, 10, 0).unwrap();
        assert!(out.seg.is_some() && out.confidence >= 0.7, "{out:?}");
        let on_street = (out.point.lat - destination(o(), 0.0, 100.0).lat).abs() * 111_195.0;
        assert!(on_street < 0.5, "the pin sits on the east-west street: {on_street} m off");
    }

    #[test]
    fn turning_at_a_crossing_switches_to_the_new_street() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        walk(&mut m, destination(o(), 0.0, 100.0), 90.0, 2.0, 15, 0); // east along y = 100 m to x = 100 m
        let corner = destination(destination(o(), 0.0, 100.0), 90.0, 100.0);
        let out = walk(&mut m, corner, 0.0, 2.0, 6, 75).unwrap(); // then north
        let off_line = (out.point.lon - corner.lon).abs() * 111_195.0 * corner.lat.to_radians().cos();
        assert!(off_line < 0.5, "matched onto the north street: {out:?}");
    }

    #[test]
    fn far_from_every_street_it_is_off_network() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let out = walk(&mut m, destination(destination(o(), 0.0, 150.0), 90.0, 150.0), 0.0, 0.0, 3, 0).unwrap(); // middle of a block, 50 m from streets
        assert!(out.seg.is_none(), "{out:?}");
    }

    #[test]
    fn a_degraded_graph_caps_the_confidence() {
        let links: Vec<(Point, Point)> = (0..4).map(|i| (destination(o(), 90.0, 60.0 * f64::from(i)), destination(o(), 90.0, 60.0 * f64::from(i + 1)))).collect();
        let mut m = Matcher::new(StreetGraph::degraded(&links).map(Arc::new), mode_mask(Mode::Walk));
        let out = walk(&mut m, o(), 90.0, 1.0, 8, 0).unwrap();
        assert!(out.confidence <= 0.5 + 1e-9, "{out:?}");
    }

    #[test]
    fn standing_and_tiny_moves_do_not_feed_the_lattice_and_the_trace_lags_three_inputs() {
        let mut m = Matcher::new(Some(grid()), mode_mask(Mode::Walk));
        let p = LocParams::default();
        let still = Estimate { motion: crate::loc::Motion::Stationary, ..est(o(), 0) };
        assert!(m.push(&still, &p).is_none());
        walk(&mut m, destination(o(), 0.0, 100.0), 90.0, 2.0, 10, 1);
        assert_eq!(m.trace().len(), 10 - 3);
        let before = m.trace().len();
        m.push(&est(destination(destination(o(), 0.0, 100.0), 90.0, 64.0), 47), &p); // 1 m and 1 s after the last input
        assert_eq!(m.trace().len(), before);
    }

    #[test]
    fn without_a_graph_the_trace_is_the_estimates() {
        let mut m = Matcher::default();
        assert!(walk(&mut m, o(), 90.0, 0.0, 5, 0).is_none());
        assert_eq!(m.trace().len(), 5);
    }

    #[test]
    fn the_street_transition_prefers_a_route_as_long_as_the_straight_line() {
        let same = street_transition_log(50.0, 50.0, 5.0, 0.0);
        let detour = street_transition_log(50.0, 150.0, 5.0, 0.0);
        assert!(same > detour + 15.0, "{same} vs {detour}");
        assert!((same - (-(5.0_f64).ln())).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc::matcher`
Expected: FAIL to compile: `file not found for module 'matcher'`.

- [ ] **Step 3: Write `core/src/loc/matcher.rs`** (above the tests)

```rust
//! Layer 2: online HMM map matching (Newson and Krumm 2009) of the IMM estimates, with an off-network state. Display and the trace only:
//! quests never see a matched position.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use crate::geo::{distance_m, Point};
use crate::loc::graph::{Cand, StreetGraph};
use crate::loc::{Estimate, LocParams, Motion, ACC_TO_SIGMA};
use crate::num::i64_to_f64;

/// The matcher's answer for the newest input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchOut {
    /// The matched point (the estimate itself off-network).
    pub point: Point,
    /// Normalised forward probability of the chosen state, 0..1 (capped on a degraded graph).
    pub confidence: f64,
    /// The street segment; `None` off-network.
    pub seg: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
struct State {
    cand: Option<Cand>,
    point: Point,
    input: Point,
    logv: f64,
    fwd: f64,
    back: Option<usize>,
    dir: i8,
}

/// `ln` of the street-to-street transition: `(1 - p_off) exp(-|d_gc - d_route| / beta) / beta`.
#[must_use]
pub fn street_transition_log(d_gc: f64, d_route: f64, beta: f64, p_off: f64) -> f64 {
    (1.0 - p_off).ln() - (d_gc - d_route).abs() / beta - beta.ln()
}

/// Online Viterbi (pin: lag 0; trace: fixed lag) over street candidates plus an off-network state.
#[derive(Debug, Clone, Default)]
pub struct Matcher {
    graph: Option<Arc<StreetGraph>>,
    mask: u8,
    columns: VecDeque<Vec<State>>,
    last_input: Option<(Point, i64)>,
    best: Option<MatchOut>,
    trace: Vec<Point>,
    trace_from_ms: Option<i64>,
    cache: HashMap<usize, HashMap<usize, f64>>,
    route_limit_m: f64,
}

impl Matcher {
    /// A matcher on `graph` for the way classes in `mask`.
    #[must_use]
    pub fn new(graph: Option<Arc<StreetGraph>>, mask: u8) -> Self {
        Self { graph, mask, ..Self::default() }
    }

    /// Change the graph (the game's zones were attached).
    pub fn set_graph(&mut self, graph: Option<Arc<StreetGraph>>) {
        self.graph = graph;
        self.cache.clear();
        self.restart();
    }

    /// Change the way classes (the zone's mode changed).
    pub fn set_mask(&mut self, mask: u8) {
        if mask != self.mask {
            self.mask = mask;
            self.cache.clear();
            self.restart();
        }
    }

    /// Forget the lattice (the filter restarted); what it already decided is added to the trace first.
    pub fn restart(&mut self) {
        self.flush();
        self.columns.clear();
        self.last_input = None;
        self.best = None;
    }

    /// The newest answer.
    #[must_use]
    pub fn best(&self) -> Option<MatchOut> {
        self.best
    }

    /// The session's line: matched points where confident, estimate points elsewhere, a few inputs behind.
    #[must_use]
    pub fn trace(&self) -> &[Point] {
        &self.trace
    }

    /// Time of the first input of the session trace.
    #[must_use]
    pub fn trace_from_ms(&self) -> Option<i64> {
        self.trace_from_ms
    }

    fn push_trace(&mut self, p: Point, t_ms: i64) {
        self.trace_from_ms.get_or_insert(t_ms);
        self.trace.push(p);
    }

    fn shown(s: &State, show_confidence: f64) -> Point {
        if s.cand.is_some() && s.fwd >= show_confidence {
            s.point
        } else {
            s.input
        }
    }

    /// Backtrack from the best state of the newest column; returns the chosen state index of every column, oldest first.
    fn path(&self) -> Vec<usize> {
        let Some(last) = self.columns.back() else { return vec![] };
        let mut idx = (0..last.len()).max_by(|&a, &b| last[a].logv.total_cmp(&last[b].logv)).unwrap_or(0);
        let mut out = vec![idx];
        for k in (1..self.columns.len()).rev() {
            idx = self.columns[k][idx].back.unwrap_or(0);
            out.push(idx);
        }
        out.reverse();
        out
    }

    fn flush(&mut self) {
        let path = self.path();
        let pts: Vec<Point> = self.columns.iter().zip(&path).map(|(c, &i)| Self::shown(&c[i], 0.7)).collect();
        for p in pts {
            self.trace.push(p);
        }
    }

    fn dist_from(&mut self, g: &StreetGraph, node: usize) -> &HashMap<usize, f64> {
        if self.cache.len() > 256 {
            self.cache.clear();
        }
        let (mask, limit) = (self.mask, self.route_limit_m);
        self.cache.entry(node).or_insert_with(|| g.dijkstra(node, limit, mask))
    }

    fn route(&mut self, g: &StreetGraph, a: &Cand, b: &Cand, bound: f64) -> Option<f64> {
        if a.seg == b.seg {
            return Some((b.off_m - a.off_m).abs());
        }
        let (sa, sb) = (*g.seg(a.seg), *g.seg(b.seg));
        let mut best = f64::INFINITY;
        for (n, c0) in [(sa.a, a.off_m), (sa.b, sa.len_m - a.off_m)] {
            for (m, c1) in [(sb.a, b.off_m), (sb.b, sb.len_m - b.off_m)] {
                if let Some(d) = self.dist_from(g, n).get(&m) {
                    best = best.min(c0 + d + c1);
                }
            }
        }
        (best <= bound).then_some(best)
    }

    /// Feed an estimate. Only accepted, moving estimates that moved 5 m or came 5 s after the last input count; standing freezes the match.
    pub fn push(&mut self, est: &Estimate, p: &LocParams) -> Option<MatchOut> {
        if !est.accepted || est.motion == Motion::Stationary {
            return self.best;
        }
        let here = est.point();
        if let Some((prev, t)) = self.last_input {
            if distance_m(prev, here) < p.match_min_move_m && est.t_ms - t < p.match_min_gap_ms {
                return self.best;
            }
        }
        self.route_limit_m = p.match_route_limit_m;
        let Some(graph) = self.graph.clone() else {
            self.push_trace(here, est.t_ms);
            self.last_input = Some((here, est.t_ms));
            return None;
        };
        let sigma = (est.uncertainty_m / ACC_TO_SIGMA).max(p.match_sigma_floor_m);
        let radius = p.match_max_radius_m.min(3.0 * sigma + 10.0);
        let emis = |d: f64| -0.5 * (d / sigma).powi(2) - (std::f64::consts::TAU.sqrt() * sigma).ln();
        let mut states: Vec<State> = graph
            .candidates(here, radius, p.match_max_candidates, self.mask)
            .into_iter()
            .map(|c| State { cand: Some(c), point: graph.geo_at(c.seg, c.off_m), input: here, logv: emis(c.d_m), fwd: 0.0, back: None, dir: 0 })
            .collect();
        states.push(State { cand: None, point: here, input: here, logv: emis(p.match_off_road_m), fwd: 0.0, back: None, dir: 0 });
        let emissions: Vec<f64> = states.iter().map(|s| s.logv).collect();
        let linked = match (self.columns.back().cloned(), self.last_input) {
            (Some(prev), Some((prev_pt, prev_t))) => {
                let dt = i64_to_f64(est.t_ms - prev_t) / 1000.0;
                self.link(&graph, &prev, &mut states, &emissions, distance_m(prev_pt, here), dt, est.speed_mps, p)
            }
            _ => false,
        };
        if !linked {
            // First input, or every transition impossible (a break): the lattice restarts here.
            self.restart();
            let m = emissions.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            for (s, e) in states.iter_mut().zip(&emissions) {
                s.logv = *e;
                s.fwd = (e - m).exp();
            }
        }
        let total: f64 = states.iter().map(|s| s.fwd).sum();
        let top = states.iter().map(|s| s.logv).fold(f64::NEG_INFINITY, f64::max);
        for s in &mut states {
            s.fwd = if total > 0.0 { s.fwd / total } else { 0.0 };
            s.logv -= top; // keep the numbers small
        }
        self.trace_from_ms.get_or_insert(est.t_ms);
        self.columns.push_back(states);
        // Fixed lag: once `lag` newer inputs exist, the oldest column's choice is final and goes into the trace.
        if self.columns.len() > p.match_lag {
            let path = self.path();
            if let (Some(oldest), Some(&i)) = (self.columns.pop_front(), path.first()) {
                self.push_trace(Self::shown(&oldest[i], p.match_show_confidence), est.t_ms);
            }
        }
        let last = self.columns.back()?;
        let i = (0..last.len()).max_by(|&a, &b| last[a].logv.total_cmp(&last[b].logv))?;
        let cap = if graph.is_degraded() { p.match_degraded_cap } else { 1.0 };
        self.best = Some(MatchOut { point: last[i].point, confidence: last[i].fwd.min(cap), seg: last[i].cand.map(|c| c.seg) });
        self.last_input = Some((here, est.t_ms));
        self.best
    }

    #[allow(clippy::too_many_arguments)] // one lattice step: the previous column, the new one and the input's facts
    fn link(&mut self, g: &StreetGraph, prev: &[State], states: &mut [State], emissions: &[f64], d_gc: f64, dt: f64, speed: f64, p: &LocParams) -> bool {
        let p_off = (p.match_to_off_per_s * dt).min(0.5);
        let p_on = (p.match_to_on_per_s * dt).min(0.5);
        let n_street = states.iter().filter(|s| s.cand.is_some()).count().max(1);
        let bound = 2.0 * d_gc + 50.0;
        let mut any = false;
        for (j, sj) in states.iter_mut().enumerate() {
            let (mut best, mut back, mut dir, mut fsum) = (f64::NEG_INFINITY, None, 0_i8, 0.0);
            for (i, si) in prev.iter().enumerate() {
                let (lt, d) = match (si.cand, sj.cand) {
                    (Some(a), Some(b)) => match self.route(g, &a, &b, bound) {
                        Some(dr) => {
                            let d = if a.seg == b.seg { if b.off_m > a.off_m { 1 } else if b.off_m < a.off_m { -1 } else { 0 } } else { 0 };
                            let uturn = a.seg == b.seg && si.dir != 0 && d != 0 && d != si.dir && speed >= 0.5;
                            (street_transition_log(d_gc, dr, p.match_beta_m, p_off) + if uturn { p.match_uturn_factor.ln() } else { 0.0 }, d)
                        }
                        None => (f64::NEG_INFINITY, 0),
                    },
                    (Some(_), None) => (p_off.ln(), 0),
                    (None, Some(_)) => ((p_on / crate::num::count_f64(n_street)).ln(), 0),
                    (None, None) => ((1.0 - p_on).ln(), 0),
                };
                if !lt.is_finite() {
                    continue;
                }
                if si.logv + lt > best {
                    (best, back, dir) = (si.logv + lt, Some(i), d);
                }
                fsum += si.fwd * lt.exp();
            }
            sj.logv = best + emissions[j];
            sj.back = back;
            sj.dir = dir;
            sj.fwd = fsum * emissions[j].exp();
            any |= back.is_some();
        }
        any
    }
}
```

`params.rs`: add the fifteen matcher fields (docs naming the spec row) with the defaults listed under Interfaces.

- [ ] **Step 4: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core loc::matcher`
Expected: PASS.
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/loc
git commit -m "feat: match estimates to streets with an HMM" -m "Online Viterbi over street candidates and an off-network state
(Newson and Krumm): emissions from the estimate's uncertainty, route
versus straight-line transitions, U-turn penalty, confidence capped on
degraded graphs, a fixed-lag trace." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 19: The display rule, the matched trace and the turn scenario

**Files:**

- Modify: `core/src/loc/locator.rs` (`matcher` field, `set_graph`, `set_mode`, `reset`, `on_fix`, `display`, `matched`, `trace_matched`),
  `core/src/game.rs` (`trace_matched`), `core/ffi/src/engine.rs` (`TraceOut`, `trace_matched`), `core/src/loc/bench/run.rs`
  (`ReplayOpts::graph`, `Replay::matches`, `Replay::displays`), `core/src/loc/bench/metrics.rs` (`matching_stats`), `core/examples/replay.rs`
  (`--atlas`), `core/tests/loc_scenarios.rs`
- Modify: `android/app/src/main/java/dev/apgo2/AppModel.kt` (`refreshPlay` trace)

**Interfaces:**

- Consumes: `Matcher`, `MatchOut` (Task 18); `StreetGraph`, `mode_mask`, `grid_ways` (Task 17); `display` (Task 14).
- Produces: `Locator::matched() -> Option<MatchOut>`, `Locator::trace_matched() -> (Option<i64>, Vec<Point>)`, `Game::trace_matched()` (same),
  FFI `TraceOut { from_ms: Option<i64>, points: Vec<GeoPoint> }` and `Engine::trace_matched() -> TraceOut` [`traceMatched()`],
  `ReplayOpts.graph: Option<Arc<StreetGraph>>`, `Replay.matches: Vec<Option<MatchOut>>`, `Replay.displays: Vec<DisplayPosition>`,
  `bench::MatchingStats { matched_share, switches_per_min, off_share }`, `bench::matching_stats(&Replay) -> MatchingStats`.
- Display rule (spec): the pin sits at the matched point when the confidence is at least 0.7 and it is within `max(10 m, 2 sigma_out)` of the
  estimate; otherwise at the estimate. A switch between the two glides like any move (the Kotlin side already snaps only on `snap` or > 50 m).

- [ ] **Step 1: Write the failing tests**

`locator.rs` tests:

```rust
    #[test]
    fn a_confident_match_moves_the_pin_onto_the_street_but_never_the_estimate() {
        let mut l = Locator::default();
        l.set_graph(crate::loc::graph::StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 5, 100.0)).map(Arc::new));
        let street = destination(o(), 0.0, 100.0);
        for t in 0..60 {
            l.on_fix(&fix(destination(destination(street, 90.0, 1.4 * i64_to_f64(t)), 0.0, 4.0), t, 4.0));
        }
        let d = l.display(59_000).unwrap();
        assert!(d.matched && d.match_confidence >= 0.7, "{d:?}");
        assert!((d.lat - street.lat).abs() * 111_195.0 < 0.5, "pin on the street");
        assert!((d.est_lat - street.lat).abs() * 111_195.0 > 2.0, "the estimate stays where the filter put it");
        let (from, line) = l.trace_matched();
        assert!(from.is_some() && line.len() >= 5);
    }

    #[test]
    fn far_from_the_streets_the_pin_stays_on_the_estimate() {
        let mut l = Locator::default();
        l.set_graph(crate::loc::graph::StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 5, 100.0)).map(Arc::new));
        let mid = destination(destination(o(), 0.0, 150.0), 90.0, 150.0);
        for t in 0..30 {
            l.on_fix(&fix(destination(mid, 0.0, 1.4 * i64_to_f64(t)), t, 4.0));
        }
        assert!(!l.display(29_000).unwrap().matched);
    }
```

Append to `core/tests/loc_scenarios.rs`:

```rust
use std::sync::Arc;

use apgo_core::loc::bench::{grid_ways, matching_stats};
use apgo_core::loc::graph::StreetGraph;

#[test]
fn at_a_corner_the_matched_street_is_the_new_one_within_five_seconds() {
    let graph = Arc::new(StreetGraph::from_ways(&grid_ways(origin(), 5, 100.0)).unwrap());
    let s = Scenario::walk(origin(), vec![east(100.0, 1.4), Leg::Move { bearing_deg: 0.0, dist_m: 100.0, speed_mps: 1.4 }], 5.0);
    let turn_t = s.t0_ms + 72_000; // 100 m at 1.4 m/s
    let corner = destination(origin(), 90.0, 100.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &ReplayOpts { graph: Some(graph.clone()), ..opts(Mode::Walk) });
        let k = out.shown.iter().position(|x| x.t_ms >= turn_t + 5_000).unwrap();
        let m = out.matches[k].expect("matched after the turn");
        let off_north_street = (m.point.lon - corner.lon).abs() * 111_195.0 * corner.lat.to_radians().cos();
        assert!(off_north_street < 1.0, "seed {seed}: {m:?}");
        assert!(matching_stats(&out).matched_share > 0.8, "seed {seed}");
    }
}
```

`core/ffi/src/engine.rs` tests:

```rust
    #[test]
    fn the_matched_trace_of_no_game_is_empty() {
        let dir = std::env::temp_dir().join(format!("apgo-ffi-trace-{}", std::process::id()));
        let e = Engine::new(dir.to_string_lossy().into_owned());
        let t = e.trace_matched();
        assert!(t.from_ms.is_none() && t.points.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core && cargo test -p apgo-ffi`
Expected: FAIL to compile: `no method named 'trace_matched'`, `no field 'graph' on type 'ReplayOpts'`, `cannot find function 'matching_stats'`.

- [ ] **Step 3: Implement**

`locator.rs`: field `matcher: Matcher` (init `Matcher::new(None, mode_mask(Mode::Walk))`). `set_graph` also calls
`self.matcher.set_graph(graph.clone())` (store the clone in `self.graph`); `set_mode` also calls `self.matcher.set_mask(mode_mask(mode))`;
`reset` also calls `self.matcher.restart()`. In `on_fix`'s accepted branch, after `let e = self.emit(..)`: `self.matcher.push(&e, &self.params);`
then return `e` (write the branch as `let e = self.emit(f.t_ms, if soft { .. }); self.matcher.push(&e, &self.params); e`; same for the `Reset`
and `Relocated` returns, which are accepted too). Add:

```rust
    /// The matcher's newest answer.
    #[must_use]
    pub fn matched(&self) -> Option<MatchOut> {
        self.matcher.best()
    }

    /// The session's display line (matched where confident) and the time it starts.
    #[must_use]
    pub fn trace_matched(&self) -> (Option<i64>, Vec<Point>) {
        (self.matcher.trace_from_ms(), self.matcher.trace().to_vec())
    }
```

In `display`, after computing `est` and before prediction:

```rust
        let sigma_out = est.uncertainty_m / ACC_TO_SIGMA;
        let matched = self.matcher.best().filter(|m| {
            est.source != Source::Bridged
                && m.seg.is_some()
                && m.confidence >= p.match_show_confidence
                && distance_m(m.point, est.point()) <= p.match_show_min_m.max(2.0 * sigma_out)
        });
        let base = matched.map_or(est.point(), |m| m.point);
```

predict from `base` instead of `est.point()`, and set `matched: matched.is_some(), match_confidence: matched.map_or(0.0, |m| m.confidence)`.

`game.rs`:

```rust
    /// The session's display trace (matched where confident) and when it starts; the journal has the estimates before that.
    #[must_use]
    pub fn trace_matched(&self) -> (Option<i64>, Vec<Point>) {
        self.locator.trace_matched()
    }
```

`engine.rs`:

```rust
/// The current session's display line.
#[derive(Debug, Clone, uniffi::Record)]
pub struct TraceOut {
    /// When it starts, Unix ms (the journal's points before this are the older trace).
    pub from_ms: Option<i64>,
    /// The line.
    pub points: Vec<GeoPoint>,
}
```

```rust
    /// The current session's trace, matched to streets where confident.
    pub fn trace_matched(&self) -> TraceOut {
        let (from_ms, pts) = self.with_game(|g| g.trace_matched()).unwrap_or((None, vec![]));
        TraceOut { from_ms, points: pts.into_iter().map(gp).collect() }
    }
```

`bench/run.rs`: `ReplayOpts` gains `pub graph: Option<Arc<StreetGraph>>` (`Default` `None`; `PartialEq` derive dropped: compare fields in
tests if needed), `Replay` gains `pub matches: Vec<Option<MatchOut>>` and `pub displays: Vec<DisplayPosition>`; `run_locator` calls
`loc.set_graph(opts.graph.clone())` and, for every estimate it records, pushes `loc.matched()` and `loc.display(e.t_ms).unwrap_or_default()`.
`metrics.rs`:

```rust
/// Map matching on a replay: share of positions shown on a street, street changes per minute, share off-network.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchingStats {
    /// Share of displays with a confident match.
    pub matched_share: f64,
    /// Segment changes of the match per minute.
    pub switches_per_min: f64,
    /// Share of matches that are off-network.
    pub off_share: f64,
}

/// [`MatchingStats`] of a replay.
#[must_use]
pub fn matching_stats(r: &crate::loc::bench::Replay) -> MatchingStats {
    let n = count_f64(r.displays.len().max(1));
    let matched = count_f64(r.displays.iter().filter(|d| d.matched).count());
    let segs: Vec<Option<usize>> = r.matches.iter().flatten().map(|m| m.seg).collect();
    let switches = count_f64(segs.windows(2).filter(|w| w[0] != w[1]).count());
    let minutes = match (r.shown.first(), r.shown.last()) {
        (Some(a), Some(b)) => (i64_to_f64(b.t_ms - a.t_ms) / 60_000.0).max(1.0 / 60.0),
        _ => 1.0,
    };
    MatchingStats { matched_share: matched / n, switches_per_min: switches / minutes, off_share: count_f64(segs.iter().filter(|s| s.is_none()).count()) / count_f64(segs.len().max(1)) }
}
```

`examples/replay.rs`: repeatable `--atlas <file>` (each a saved `Atlas` JSON, e.g. a pulled `files/atlas/<realm id>.json`):
`let atlases: Vec<Atlas> = a.atlas.iter().map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()).collect();`,
`graph: StreetGraph::for_atlases(&atlases.iter().collect::<Vec<_>>()).map(Arc::new)` in `ReplayOpts`, and print
`matching: {:.0} % matched, {:.1} street changes/min, {:.0} % off-network` from `matching_stats`. Add the matched line to the GeoJSON
(`filtered.displays` where `matched`).

Kotlin `AppModel.refreshPlay`, the trace line becomes:

```kotlin
            if (withTrace) {
                val session = engine.traceMatched()
                val older = engine.track(0L, (session.fromMs ?: Long.MAX_VALUE) - 1).map { seg -> seg.points.map { LatLng(it.lat, it.lon) } }
                trace = (older + listOf(session.points.map { LatLng(it.lat, it.lon) })).filter { it.size >= 2 }
            }
```

- [ ] **Step 4: Run the tests and the gates**

Run: `cd core && cargo test -p apgo-core --release --test loc_scenarios && cargo test -p apgo-core && cargo test -p apgo-ffi`
Expected: PASS (tune only `match_*` parameters within the spec's ranges, e.g. `match_beta_m` 3 to 10, if the corner scenario fails).
Run: `just check-rust && ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core android/app/src/main/java/dev/apgo2/AppModel.kt
git commit -m "feat: show the matched street and trace" -m "The pin sits on the matched street when the match is confident and
close to the estimate; the session trace follows the streets with a
3-input lag. Replay takes --atlas and reports matching stats." -m "Closes #87
Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 20: The carry-offset estimator

**Files:**

- Modify: `core/src/loc/heading.rs` (append `CarryOffset`), `core/src/loc/params.rs` (carry group), `core/src/loc/locator.rs`
  (`carry` field, `last_course_sigma_deg`, learning in `on_fix`, `carry()` getter)

**Interfaces:**

- Consumes: `Compass`, `wrap_deg`, `circular_mean_deg` (Task 14); `Estimate` (Task 2).
- Produces: `heading::CarryOffset` (`Default`, `Clone`): `delta_deg()`, `sigma_deg()`, `steadiness() -> f64` (mean resultant length of the
  last 20 s of residuals), `confidence(&LocParams) -> f64`, `reset()`, `learn(course_deg, course_sigma_deg, &Compass, now_ms, &LocParams) -> bool`,
  `watch_gap(&Compass, now_ms, cadence_changed: bool, &LocParams)`, `heading_for_gap(&Compass, now_ms, last_course_deg: Option<f64>, gap_s, &LocParams) -> Option<(f64, f64)>`
  (bearing and sigma, degrees); `Locator::carry() -> &CarryOffset`.
- `LocParams` additions: `carry_learn_max_unc_m: 10.0`, `carry_learn_min_speed_mps: 0.8`, `carry_learn_max_course_sigma_deg: 15.0`,
  `carry_max_rate_deg_s: 30.0`, `carry_q_deg2_per_s: 4.0`, `carry_reset_var_deg2: 8100.0`, `carry_change_sigmas: 3.0`, `carry_change_ms: 3000`,
  `carry_min_steadiness: 0.6`, `carry_min_residuals: 5`, `carry_min_confidence: 0.5`, `carry_jump_deg: 45.0`, `carry_gap_sigma0_deg: 10.0`,
  `carry_gap_sigma_per_s: 2.0`, `carry_gap_sigma_max_deg: 90.0`, `carry_enabled: true` (false = trust the raw compass, for the bench comparison).

- [ ] **Step 1: Write the failing tests** (append to `heading.rs` tests)

```rust
    fn steady(az: f64, t_ms: i64) -> Compass {
        let mut c = Compass::default();
        (0..5).for_each(|i| c.push(h(t_ms - 2000 + i * 500, az, CompassAccuracy::High, 80.0)));
        c
    }

    #[test]
    fn a_pocket_offset_is_learned_from_good_courses() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            let t = 10_000 + s * 1000;
            assert!(k.learn(90.0, 5.0, &steady(0.0, t), t, &p), "steady compass, good course");
        }
        assert!(wrap_deg(k.delta_deg() - 90.0).abs() < 5.0, "{}", k.delta_deg());
        assert!(k.confidence(&p) >= 0.5, "{}", k.confidence(&p));
        let (theta, sigma) = k.heading_for_gap(&steady(10.0, 80_000), 80_000, Some(0.0), 5.0, &p).unwrap();
        assert!(wrap_deg(theta - 100.0).abs() < 5.0 && sigma < 45.0, "compass + offset: {theta} {sigma}");
    }

    #[test]
    fn a_swinging_or_unreliable_compass_is_not_learned() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        let mut swing = Compass::default();
        (0..5).for_each(|i| swing.push(h(8_000 + i * 500, f64::from(u8::try_from(i).unwrap()) * 40.0, CompassAccuracy::High, 0.0)));
        assert!(!k.learn(90.0, 5.0, &swing, 10_000, &p), "80 deg/s");
        let mut bad = Compass::default();
        (0..5).for_each(|i| bad.push(h(8_000 + i * 500, 0.0, CompassAccuracy::Unreliable, 0.0)));
        assert!(!k.learn(90.0, 5.0, &bad, 10_000, &p));
        assert_eq!(k.confidence(&p), 0.0);
    }

    #[test]
    fn a_carry_change_while_learning_resets_the_confidence() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        for s in 60..64 {
            k.learn(90.0, 5.0, &steady(90.0, 10_000 + s * 1000), 10_000 + s * 1000, &p); // the phone moved: offset now 0
        }
        assert!(k.confidence(&p) < 0.5, "{}", k.confidence(&p));
    }

    #[test]
    fn a_compass_jump_in_a_gap_resets_unless_the_cadence_changed_too() {
        let p = LocParams::default();
        let mut k = CarryOffset::default();
        for s in 0..60 {
            k.learn(90.0, 5.0, &steady(0.0, 10_000 + s * 1000), 10_000 + s * 1000, &p);
        }
        let mut jump = Compass::default();
        jump.push(h(100_000, 0.0, CompassAccuracy::High, 80.0));
        jump.push(h(101_500, 70.0, CompassAccuracy::High, 80.0));
        let mut kept = k.clone();
        kept.watch_gap(&jump, 101_500, true, &p);
        assert!(kept.confidence(&p) >= 0.5, "a cadence change explains it (turning a corner)");
        k.watch_gap(&jump, 101_500, false, &p);
        assert_eq!(k.confidence(&p), 0.0);
    }

    #[test]
    fn without_confidence_the_gap_heading_is_the_last_course_with_a_growing_sigma() {
        let p = LocParams::default();
        let k = CarryOffset::default();
        assert_eq!(k.heading_for_gap(&Compass::default(), 0, Some(45.0), 10.0, &p), Some((45.0, 30.0)));
        assert_eq!(k.heading_for_gap(&Compass::default(), 0, Some(45.0), 100.0, &p), Some((45.0, 90.0)));
        assert_eq!(k.heading_for_gap(&Compass::default(), 0, None, 1.0, &p), None);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc::heading`
Expected: FAIL to compile: `cannot find type 'CarryOffset'`.

- [ ] **Step 3: Implement** (append to `heading.rs` above the tests)

```rust
/// The offset between where the phone points and where the player walks, `delta = course - azimuth`, learned online against good GPS
/// courses (a 1-D circular Kalman filter) with a confidence. It changes whenever the phone moves (hand, pocket, bag).
#[derive(Debug, Clone, PartialEq)]
pub struct CarryOffset {
    delta_deg: f64,
    var_deg2: f64,
    last_ms: Option<i64>,
    resid: VecDeque<(i64, f64)>,
    over_since: Option<i64>,
}

impl Default for CarryOffset {
    fn default() -> Self {
        Self { delta_deg: 0.0, var_deg2: 8100.0, last_ms: None, resid: VecDeque::new(), over_since: None }
    }
}

impl CarryOffset {
    /// The learned offset, degrees.
    #[must_use]
    pub fn delta_deg(&self) -> f64 {
        self.delta_deg
    }

    /// Its sigma, degrees.
    #[must_use]
    pub fn sigma_deg(&self) -> f64 {
        self.var_deg2.max(0.0).sqrt()
    }

    /// Mean resultant length of the last 20 s of residuals (1 = perfectly steady); 0 with too few.
    #[must_use]
    pub fn steadiness(&self) -> f64 {
        if self.resid.len() < 5 {
            return 0.0;
        }
        let (s, c) = self.resid.iter().fold((0.0, 0.0), |(s, c), (_, r)| (s + r.to_radians().sin(), c + r.to_radians().cos()));
        s.hypot(c) / count_f64(self.resid.len())
    }

    /// `clamp(1 - sigma / 45 deg, 0, 1)`, and 0 while the offset is not steady.
    #[must_use]
    pub fn confidence(&self, p: &LocParams) -> f64 {
        if self.resid.len() < p.carry_min_residuals || self.steadiness() < p.carry_min_steadiness {
            return 0.0;
        }
        (1.0 - self.sigma_deg() / 45.0).clamp(0.0, 1.0)
    }

    /// The phone moved: start learning again.
    pub fn reset(&mut self) {
        *self = Self { last_ms: self.last_ms, ..Self::default() };
    }

    /// Learn from one good GPS course while the compass is fresh, reliable and steady (turning slower than 30 deg/s over 2 s). True when
    /// it learned. Three seconds of innovations beyond 3 sigma mean the carry changed: the estimator restarts.
    pub fn learn(&mut self, course_deg: f64, course_sigma_deg: f64, compass: &Compass, now_ms: i64, p: &LocParams) -> bool {
        let Some(h) = compass.latest(now_ms, p.compass_fresh_ms) else { return false };
        let Some(cs) = h.accuracy.sigma_deg() else { return false };
        if compass.rate_deg_s(now_ms, p.compass_window_ms).is_none_or(|r| r >= p.carry_max_rate_deg_s) {
            return false;
        }
        if let Some(t) = self.last_ms {
            self.var_deg2 = (self.var_deg2 + p.carry_q_deg2_per_s * (i64_to_f64(now_ms - t) / 1000.0).max(0.0)).min(p.carry_reset_var_deg2);
        }
        self.last_ms = Some(now_ms);
        let r = course_sigma_deg * course_sigma_deg + cs * cs;
        let innov = wrap_deg(wrap_deg(course_deg - h.azimuth_deg) - self.delta_deg);
        if innov.abs() > p.carry_change_sigmas * (self.var_deg2 + r).sqrt() {
            let since = *self.over_since.get_or_insert(now_ms);
            if now_ms - since >= p.carry_change_ms {
                self.reset();
                self.delta_deg = wrap_deg(course_deg - h.azimuth_deg);
            }
            return true;
        }
        self.over_since = None;
        let k = self.var_deg2 / (self.var_deg2 + r);
        self.delta_deg = wrap_deg(self.delta_deg + k * innov);
        self.var_deg2 *= 1.0 - k;
        self.resid.push_back((now_ms, innov));
        while self.resid.front().is_some_and(|(t, _)| now_ms - t > 20_000) {
            self.resid.pop_front();
        }
        true
    }

    /// During a gap: an azimuth jump over 45 degrees within 2 s while the cadence stays the same means the phone moved in the pocket.
    pub fn watch_gap(&mut self, compass: &Compass, now_ms: i64, cadence_changed: bool, p: &LocParams) {
        let jumped = compass.rate_deg_s(now_ms, p.compass_window_ms).is_some_and(|r| r * (i64_to_f64(p.compass_window_ms) / 1000.0) > p.carry_jump_deg);
        if jumped && !cadence_changed {
            self.reset();
        }
    }

    /// The bearing to bridge a gap with: compass + offset when the offset is confident (and the compass reliable), else the last GPS course
    /// with a sigma growing 2 deg/s from 10 (at most 90); `None` with neither.
    #[must_use]
    pub fn heading_for_gap(&self, compass: &Compass, now_ms: i64, last_course_deg: Option<f64>, gap_s: f64, p: &LocParams) -> Option<(f64, f64)> {
        if self.confidence(p) >= p.carry_min_confidence || !p.carry_enabled {
            if let Some((h, cs)) = compass.latest(now_ms, p.compass_fresh_ms).and_then(|h| h.accuracy.sigma_deg().map(|cs| (h, cs))) {
                let delta = if p.carry_enabled { self.delta_deg } else { 0.0 };
                return Some(((h.azimuth_deg + delta).rem_euclid(360.0), (self.var_deg2.min(p.carry_reset_var_deg2) * f64::from(u8::from(p.carry_enabled)) + cs * cs).sqrt()));
            }
        }
        last_course_deg.map(|c| (c, (p.carry_gap_sigma0_deg + p.carry_gap_sigma_per_s * gap_s).min(p.carry_gap_sigma_max_deg)))
    }
}
```

`params.rs`: the sixteen carry fields with the defaults listed. `locator.rs`: fields `carry: CarryOffset`, `last_course_sigma_deg: Option<f64>`;
`emit` stores the course sigma it computed into `self.last_course_sigma_deg` (`None` when no course); after an accepted GPS estimate in
`on_fix` (next to `self.matcher.push`):

```rust
                if let (Some(c), Some(cs)) = (e.course_deg, self.last_course_sigma_deg) {
                    if e.uncertainty_m <= self.params.carry_learn_max_unc_m && e.speed_mps >= self.params.carry_learn_min_speed_mps && cs <= self.params.carry_learn_max_course_sigma_deg {
                        self.carry.learn(c, cs, &self.compass, f.t_ms, &self.params);
                    }
                }
```

and `pub fn carry(&self) -> &CarryOffset { &self.carry }` (documented).

- [ ] **Step 4: Run the tests and the gate**

Run: `cd core && cargo test -p apgo-core loc::`
Expected: PASS.
Run: `just check-rust`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add core/src/loc
git commit -m "feat: learn the phone's carry offset" -m "A circular Kalman filter learns course minus compass while GPS is good
and the compass steady, with a steadiness-based confidence, a reset on
a carry change, and the course-plus-street fallback for gaps." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 21: Step calibration by source, saved on the phone

**Files:**

- Create: `core/src/loc/calib.rs`, `android/app/src/main/java/dev/apgo2/StepCalStore.kt`
- Modify: `core/src/loc/mod.rs` (`pub mod calib;`), `core/src/loc/params.rs` (calibration group), `core/src/loc/locator.rs` (`calib`,
  `set_step_calibration`, `step_calibration`, learning in `on_fix`), `core/src/loc/locator.rs` `StepHistory` (`latest`, `total_at` public),
  `core/src/game.rs` (pass-through), `core/ffi/src/engine.rs` (`StepCalIn`, `StepCalOut`, two functions), `core/tests/loc_scenarios.rs`
- Modify: `android/app/src/main/java/dev/apgo2/AppModel.kt` (`stepCal`, `saveStepCal`, `onBackground`), `GameLibrary.kt` (`openGame`,
  `pause`), `FieldDiagnostics.kt` (`heartbeat`: every 5 min)
- Test: `core/src/loc/calib.rs`, `android/app/src/test/java/dev/apgo2/StepCalStoreTest.kt`

**Interfaces:**

- Produces: `calib::PHONE_STEP_COUNTER = "phone.step_counter"`, `calib::step_length_m(cadence_hz) -> f64`,
  `calib::StepCal { source: String, k: f64, var_k: f64, samples: u32, updated_ms: i64 }` with `StepCal::default_for(source)`,
  `calib::Calibrator` (`Default` = phone source): `cal() -> StepCal`, `set(StepCal) -> bool` (false when the source is not the active one),
  `k()`, `sigma_k()`, `on_estimate(&Estimate, &StepHistory, &LocParams)`; `Locator::set_step_calibration(StepCal)`, `Locator::step_calibration() -> StepCal`;
  `Game::set_step_calibration`, `Game::step_calibration`; FFI `StepCalIn`/`StepCalOut { source, k, var_k, samples: u32, updated_ms }`,
  `Engine::set_step_calibration(c: StepCalIn)` [`setStepCalibration`], `Engine::step_calibration() -> Option<StepCalOut>` [`stepCalibration()`];
  Kotlin `StepCalCodec.encode(StepCalOut): String`, `StepCalCodec.decode(source, String?): StepCalIn?`, `StepCalStore(ctx).load(source)`, `.save(c)`.
- `LocParams` additions: `calib_window_ms: 20_000`, `calib_min_dist_m: 30.0`, `calib_max_unc_m: 10.0`, `calib_q_per_min: 0.0001`,
  `calib_k_min: 0.6`, `calib_k_max: 1.4`, `calib_session_ms: 180_000`, `calib_session_var: 0.01`, `calib_adapt_var: 0.04`,
  `calib_adapt_sigmas: 3.0`, `calib_cadence_jump: 0.2`, `calib_same_speed: 0.1`, `calib_obs_scale: 2.0` (the "2 x" of
  `sigma_obs = 2 x mean uncertainty / d`, exposed for tuning: see spec gap 21).
- Plan choice: a window closes once it is at least 20 s long and covers at least 30 m (at 1.4 m/s, 20 s is only 28 m).

- [ ] **Step 1: Write the failing tests**

Bottom of `core/src/loc/calib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{destination, Point};

    #[test]
    fn the_cadence_model_is_the_spec_formula_clamped() {
        assert!((step_length_m(1.8) - 0.70).abs() < 1e-9 && (step_length_m(2.8) - 0.95).abs() < 1e-9);
        assert!((step_length_m(0.5) - 0.5).abs() < 1e-9 && (step_length_m(5.0) - 1.1).abs() < 1e-9);
    }

    fn walk(c: &mut Calibrator, scale: f64, from_s: i64, secs: i64, steps0: i64) -> i64 {
        let p = LocParams::default();
        let mut h = StepHistory::default();
        let o = Point::new(40.0, -111.0);
        let f = crate::loc::bench::cadence_for(1.4, scale);
        let mut total = steps0;
        for s in 0..=secs {
            let t = (from_s + s) * 1000;
            if s % 2 == 0 {
                h.push(total, t);
            }
            total = steps0 + crate::num::round_i64(f * crate::num::i64_to_f64(s));
            let at = destination(o, 90.0, 1.4 * crate::num::i64_to_f64(from_s + s));
            c.on_estimate(&Estimate { uncertainty_m: 4.0, speed_mps: 1.4, ..Estimate::exact(at.lat, at.lon, t) }, &h, &p);
        }
        total
    }

    #[test]
    fn walking_with_good_gps_learns_the_scale_and_clamps_it() {
        let mut c = Calibrator::default();
        walk(&mut c, 0.85, 0, 300, 1000);
        assert!((c.k() - 0.85).abs() < 0.05, "{}", c.k());
        assert!(c.cal().samples >= 10);
        let mut tiny = Calibrator::default();
        walk(&mut tiny, 0.4, 0, 300, 1000);
        assert!((tiny.k() - 0.6).abs() < 1e-9, "clamped to 0.6: {}", tiny.k());
    }

    #[test]
    fn a_stored_value_is_rechecked_in_a_new_session_and_a_carry_change_adapts_fast() {
        let mut c = Calibrator::default();
        assert!(c.set(StepCal { var_k: 0.0001, k: 1.0, ..StepCal::default_for(PHONE_STEP_COUNTER) }));
        walk(&mut c, 0.85, 0, 30, 1000);
        assert!(c.sigma_k() >= 0.05, "the first minutes of a session keep var_k >= 0.1^2 or adapt: {}", c.sigma_k());
        walk(&mut c, 0.85, 31, 270, 2000);
        assert!((c.k() - 0.85).abs() < 0.05, "{}", c.k());
    }

    #[test]
    fn calibrations_of_other_sources_never_touch_the_phone_one() {
        let mut c = Calibrator::default();
        assert!(!c.set(StepCal { k: 1.3, ..StepCal::default_for("watch.health_connect") }));
        assert_eq!(c.cal().source, PHONE_STEP_COUNTER);
        assert!((c.k() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn blurry_or_standing_estimates_never_calibrate() {
        let p = LocParams::default();
        let mut c = Calibrator::default();
        let h = StepHistory::default();
        for t in 0..100 {
            c.on_estimate(&Estimate { uncertainty_m: 20.0, ..Estimate::exact(40.0, -111.0 + 1e-5 * f64::from(t), i64::from(t) * 1000) }, &h, &p);
        }
        assert_eq!(c.cal().samples, 0);
    }
}
```

Append to `core/tests/loc_scenarios.rs`:

```rust
#[test]
fn after_a_carry_change_the_step_scale_is_relearned_within_two_minutes() {
    let mut s = Scenario::walk(origin(), vec![east(840.0, 1.4)], 5.0); // 10 min
    s.steps = true;
    s.step_scale = vec![(0, 1.0), (300, 0.85)]; // pocket to bag at 5 min
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let mut l = apgo_core::loc::Locator::default();
        let cut = s.t0_ms + 300_000 + 120_000;
        let mut evs: Vec<(i64, Option<apgo_core::loc::RawFix>, Option<i64>)> = r.fixes.iter().map(|f| (f.t_ms, Some(*f), None)).collect();
        evs.extend(r.steps.iter().map(|(t, n)| (*t, None, Some(*n))));
        evs.sort_by_key(|e| (e.0, e.1.is_some()));
        for (t, f, n) in evs.into_iter().filter(|e| e.0 <= cut) {
            if let Some(n) = n {
                l.on_steps(n, t, None);
            }
            if let Some(f) = f {
                l.on_fix(&f);
            }
        }
        assert!((l.step_calibration().k - 0.85).abs() <= 0.05, "seed {seed}: k {}", l.step_calibration().k);
    }
}
```

`android/app/src/test/java/dev/apgo2/StepCalStoreTest.kt`:

```kotlin
package dev.apgo2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.apgo_ffi.StepCalOut

class StepCalStoreTest {
    @Test fun aCalibrationRoundTripsUnderItsSource() {
        val text = StepCalCodec.encode(StepCalOut("phone.step_counter", 0.92, 0.0004, 17u, 1_800_000_000_000L))
        val back = StepCalCodec.decode("phone.step_counter", text)!!
        assertEquals("phone.step_counter", back.source)
        assertEquals(0.92, back.k, 1e-12)
        assertEquals(0.0004, back.varK, 1e-12)
        assertEquals(17u, back.samples)
        assertEquals(1_800_000_000_000L, back.updatedMs)
    }

    @Test fun nothingStoredOrGarbageIsNull() {
        assertNull(StepCalCodec.decode("phone.step_counter", null))
        assertNull(StepCalCodec.decode("phone.step_counter", "1.0;x"))
        assertNull(StepCalCodec.decode("phone.step_counter", "a;b;c;d"))
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc::calib`
Expected: FAIL to compile: `file not found for module 'calib'`.

- [ ] **Step 3: Write `core/src/loc/calib.rs`** (above the tests)

```rust
//! Step length: the cadence model `L = clamp(0.25 + 0.25 f, 0.5, 1.1)` m times a per-phone, per-source scale `k`, learned against good GPS
//! (1-D Kalman) and saved by the app, never in the game save.

use serde::{Deserialize, Serialize};

use crate::geo::distance_m;
use crate::loc::{Estimate, LocParams, Motion, Source, StepHistory};
use crate::num::i64_to_f64;

/// The step source of the phone's own step counter (Android `TYPE_STEP_COUNTER`; iOS `CMPedometer` will be `phone.pedometer`).
pub const PHONE_STEP_COUNTER: &str = "phone.step_counter";

/// Step length of the cadence model at `cadence_hz` steps per second, metres (before the scale).
#[must_use]
pub fn step_length_m(cadence_hz: f64) -> f64 {
    (0.25 + 0.25 * cadence_hz).clamp(0.5, 1.1)
}

/// A saved step calibration of one step source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepCal {
    /// Step source id.
    pub source: String,
    /// Scale on the cadence model.
    pub k: f64,
    /// Variance of `k`.
    pub var_k: f64,
    /// Windows learned from.
    pub samples: u32,
    /// Last update, Unix ms.
    pub updated_ms: i64,
}

impl StepCal {
    /// The default for `source`: `k = 1`, `var_k = 0.15^2`.
    #[must_use]
    pub fn default_for(source: &str) -> Self {
        Self { source: source.to_string(), k: 1.0, var_k: 0.15 * 0.15, samples: 0, updated_ms: 0 }
    }
}

/// Learns `k` of the phone's step counter from windows of walking with good GPS.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibrator {
    cal: StepCal,
    window: Vec<(i64, crate::geo::Point, f64)>,
    walked_ms: i64,
    misses: u32,
    last_window: Option<(f64, f64)>,
}

impl Default for Calibrator {
    fn default() -> Self {
        Self { cal: StepCal::default_for(PHONE_STEP_COUNTER), window: vec![], walked_ms: 0, misses: 0, last_window: None }
    }
}

impl Calibrator {
    /// The calibration now.
    #[must_use]
    pub fn cal(&self) -> StepCal {
        self.cal.clone()
    }

    /// Load a saved calibration; false (ignored) when it is for another step source.
    pub fn set(&mut self, cal: StepCal) -> bool {
        if cal.source != PHONE_STEP_COUNTER {
            return false;
        }
        self.cal = cal;
        true
    }

    /// The scale.
    #[must_use]
    pub fn k(&self) -> f64 {
        self.cal.k
    }

    /// Its sigma.
    #[must_use]
    pub fn sigma_k(&self) -> f64 {
        self.cal.var_k.max(0.0).sqrt()
    }

    /// Feed an estimate: accepted walking GPS estimates of at most 10 m build a window; a window of 20 s and 30 m gives one observation
    /// `k_obs = d / (steps x L(cadence))` with sigma `2 x mean uncertainty / d`.
    pub fn on_estimate(&mut self, e: &Estimate, steps: &StepHistory, p: &LocParams) {
        let good = e.accepted && e.source == Source::Gps && e.motion == Motion::Walking && e.uncertainty_m <= p.calib_max_unc_m && steps.present();
        if !good {
            self.window.clear();
            return;
        }
        self.window.push((e.t_ms, e.point(), e.uncertainty_m));
        let (Some(first), Some(last)) = (self.window.first().copied(), self.window.last().copied()) else { return };
        let span_ms = last.0 - first.0;
        let d: f64 = self.window.windows(2).map(|w| distance_m(w[0].1, w[1].1)).sum();
        if span_ms < p.calib_window_ms || d < p.calib_min_dist_m {
            return;
        }
        let n = steps.gained(first.0, last.0);
        self.window = vec![last];
        if n <= 0 {
            return;
        }
        let span_s = i64_to_f64(span_ms) / 1000.0;
        let cadence = i64_to_f64(n) / span_s;
        let mean_unc = (first.2 + last.2) / 2.0;
        let k_obs = d / (i64_to_f64(n) * step_length_m(cadence));
        let var_obs = (p.calib_obs_scale * mean_unc / d).powi(2);
        self.update(k_obs, var_obs, cadence, d / span_s, span_ms, last.0, p);
    }

    #[allow(clippy::too_many_arguments)] // one observation and its context
    fn update(&mut self, k_obs: f64, var_obs: f64, cadence: f64, speed: f64, span_ms: i64, t_ms: i64, p: &LocParams) {
        let c = &mut self.cal;
        c.var_k += p.calib_q_per_min * i64_to_f64(span_ms) / 60_000.0;
        if self.walked_ms < p.calib_session_ms {
            c.var_k = c.var_k.max(p.calib_session_var); // re-validate a stored value in every session
        }
        self.walked_ms += span_ms;
        let far = (k_obs - c.k).abs() > p.calib_adapt_sigmas * (c.var_k + var_obs).sqrt();
        self.misses = if far { self.misses + 1 } else { 0 };
        let cadence_jump = self.last_window.is_some_and(|(f0, v0)| (speed - v0).abs() <= p.calib_same_speed * v0 && (cadence - f0).abs() > p.calib_cadence_jump * f0);
        if self.misses >= 2 || cadence_jump {
            c.var_k = c.var_k.max(p.calib_adapt_var); // the carry changed: let the next windows dominate
        }
        let gain = c.var_k / (c.var_k + var_obs);
        c.k = (c.k + gain * (k_obs - c.k)).clamp(p.calib_k_min, p.calib_k_max);
        c.var_k *= 1.0 - gain;
        c.samples += 1;
        c.updated_ms = t_ms;
        self.last_window = Some((cadence, speed));
    }
}
```

`StepHistory`: make `total_at` `pub` (documented) and add `pub fn latest(&self) -> Option<(i64, i64)> { self.pts.back().copied() }`.
`params.rs`: the thirteen calibration fields. `locator.rs`: field `calib: Calibrator`; in the accepted branch of `on_fix` (with the matcher and
carry calls) `self.calib.on_estimate(&e, &self.steps, &self.params);`; and

```rust
    /// Load the phone's saved step calibration (ignored for another source). The filter itself still starts fresh.
    pub fn set_step_calibration(&mut self, c: StepCal) {
        let _ = self.calib.set(c);
    }

    /// The step calibration to save.
    #[must_use]
    pub fn step_calibration(&self) -> StepCal {
        self.calib.cal()
    }
```

`game.rs`: `pub fn set_step_calibration(&mut self, c: StepCal) { self.locator.set_step_calibration(c); }` and
`#[must_use] pub fn step_calibration(&self) -> StepCal { self.locator.step_calibration() }` (documented).

`engine.rs`:

```rust
/// A step calibration the app saved (preferences `stepcal`, one entry per source).
#[derive(Debug, Clone, uniffi::Record)]
pub struct StepCalIn {
    /// Step source id, e.g. `phone.step_counter`.
    pub source: String,
    /// Scale on the cadence model.
    pub k: f64,
    /// Variance of the scale.
    pub var_k: f64,
    /// Windows learned from.
    pub samples: u32,
    /// Last update, Unix ms.
    pub updated_ms: i64,
}

/// The step calibration to save (same fields as [`StepCalIn`]).
#[derive(Debug, Clone, uniffi::Record)]
pub struct StepCalOut {
    /// Step source id.
    pub source: String,
    /// Scale on the cadence model.
    pub k: f64,
    /// Variance of the scale.
    pub var_k: f64,
    /// Windows learned from.
    pub samples: u32,
    /// Last update, Unix ms.
    pub updated_ms: i64,
}
```

```rust
    /// Load the saved step calibration into the open game's filter.
    pub fn set_step_calibration(&self, c: StepCalIn) {
        let cal = StepCal { source: c.source, k: c.k, var_k: c.var_k, samples: c.samples, updated_ms: c.updated_ms };
        self.with_game(|g| g.set_step_calibration(cal));
    }

    /// The open game's step calibration, to save; `None` with no game.
    pub fn step_calibration(&self) -> Option<StepCalOut> {
        self.with_game(|g| g.step_calibration()).map(|c| StepCalOut { source: c.source, k: c.k, var_k: c.var_k, samples: c.samples, updated_ms: c.updated_ms })
    }
```

- [ ] **Step 4: Kotlin persistence**

`StepCalStore.kt`:

```kotlin
package dev.apgo2

import android.content.Context
import uniffi.apgo_ffi.StepCalIn
import uniffi.apgo_ffi.StepCalOut

/** The phone's step source; a watch or Health Connect source would get its own key and calibration. */
internal const val PHONE_STEP_COUNTER = "phone.step_counter"

private const val FIELDS = 4

/** `k;var_k;samples;updated_ms` (pure, unit-tested). */
internal object StepCalCodec {
    fun encode(c: StepCalOut): String = listOf(c.k, c.varK, c.samples, c.updatedMs).joinToString(";")

    fun decode(
        source: String,
        text: String?,
    ): StepCalIn? {
        val p = text?.split(";")?.takeIf { it.size == FIELDS } ?: return null
        return runCatching { StepCalIn(source, p[0].toDouble(), p[1].toDouble(), p[2].toUInt(), p[3].toLong()) }.getOrNull()
    }
}

/** Step calibrations in app preferences (`stepcal`), one entry per step source; never in the game save. */
internal class StepCalStore(
    ctx: Context,
) {
    private val prefs = ctx.getSharedPreferences("stepcal", Context.MODE_PRIVATE)

    fun load(source: String): StepCalIn? = StepCalCodec.decode(source, prefs.getString(source, null))

    fun save(c: StepCalOut) {
        prefs.edit().putString(c.source, StepCalCodec.encode(c)).apply()
    }
}
```

`AppModel`: `val stepCal = StepCalStore(ctx)` and

```kotlin
    /** Save the open game's step calibration (on close, background and every 5 minutes while playing). */
    fun saveStepCal() {
        engine.stepCalibration()?.let(stepCal::save)
    }
```

called first in `onBackground`. `GameLibrary.openGame` success: `model.stepCal.load(PHONE_STEP_COUNTER)?.let(model.engine::setStepCalibration)`
right after `engine.openGame`; `pause` calls `model.saveStepCal()` before closing. `FieldDiagnostics.heartbeat`: keep a minute counter and call
`model.saveStepCal()` every fifth beat.

- [ ] **Step 5: Run the tests and the gates**

Run: `cd core && cargo test -p apgo-core && cargo test -p apgo-core --release --test loc_scenarios`
Expected: PASS. The relearn scenario is the one most likely to fail with the spec's numbers (spec gap 21): a 15 % scale change stays under
the 3-sigma fast-adapt bar when `sigma_obs = 2 x unc / d`, and the cadence moves only about 10 %. Lower `calib_obs_scale` (for example to
0.75; the true window error on the synthetic walk is well under the formula's) until every seed is within 0.05, never the threshold, and
record the value in the commit body. If no value in (0.5, 2] works, stop and report BLOCKED with the per-seed `k`.
Run: `just check-rust && ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add core android/app/src/main/java/dev/apgo2 android/app/src/test/java/dev/apgo2
git commit -m "feat: calibrate step length per phone" -m "A 1-D Kalman scale on the cadence model, learned from 20 s windows of
good walking GPS, re-checked every session and fast to adapt after a
carry change; saved by source in app preferences, not the game." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 22: The particle-filter bridge, bridged fog and squares, the gap scenarios

**Files:**

- Create: `core/src/loc/bridge.rs`
- Modify: `core/src/loc/mod.rs` (`pub mod bridge;`), `core/src/loc/params.rs` (bridge group), `core/src/loc/imm.rs` (`Imm::with_prior`),
  `core/src/loc/locator.rs` (`bridge` field, `on_steps`, re-anchor in `on_fix`, `last_course`), `core/src/loc/bench/metrics.rs` (`reanchor`),
  `core/src/game.rs` (tests), `core/tests/loc_scenarios.rs`

**Interfaces:**

- Consumes: `StreetGraph` (Task 17), `Matcher::best` (Task 18), `CarryOffset::heading_for_gap`, `watch_gap` (Task 20), `Calibrator::k`,
  `sigma_k`, `step_length_m` (Task 21), `gauss` (Task 2), `Game::on_steps` (Task 9: already feeds a bridged estimate to `on_estimate`).
- Produces: `bridge::Steer { theta_deg, sigma_deg }`, `bridge::Bridge` with `start(graph, frame, mask, mean: Point, cov: Mat<2, 2>, sigma_k,
  prefer: Option<(usize, f64)>, course_deg: f64, t_ms, &LocParams) -> Bridge`, `step(&mut self, dist_m, Steer, &LocParams)`,
  `estimate(&self) -> (Point, f64, Option<f64>)` (position, 68 % radius, course), `moments(&self) -> (Point, Mat<2, 2>)`, fields `started_ms`,
  `last_step_ms`; `Imm::with_prior(pos, cov: Mat<2, 2>, vel: [f64; 2], t_ms, Mode, &LocParams) -> Imm`; `Locator::bridging() -> bool`;
  `bench::reanchor(truth: &[TruthPoint], r: &Replay, gap_end_ms: i64) -> Option<(f64, f64)>` (bridged error vs truth, display jump).
- `LocParams` additions: `bridge_after_ms: 10_000`, `bridge_max_ms: 300_000`, `bridge_no_steps_ms: 30_000`, `bridge_particles: 300`,
  `bridge_off_share: 0.1`, `bridge_bias_sigma_deg: 10.0`, `bridge_sigma_k_min: 0.05`, `bridge_roughen_m: 1.0`, `bridge_off_weight: 0.5`,
  `bridge_seed: 0x5eed`, `coast_max_ms: 30_000`.
- Plan choices: "bridged error at re-anchor" is the distance from the last bridged position to the truth at that moment (the first fix is
  noisy too); "display jump" is the distance from the last bridged estimate to the first estimate after the gap. Bike and Drive zones never
  bridge: their pin coasts on the Kalman prediction in `display` (`Predicted` after 6 s, `Stale` after 30 s, Task 14).

- [ ] **Step 1: Write the failing tests**

Bottom of `core/src/loc/bridge.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::{destination, distance_m};
    use crate::loc::bench::grid_ways;
    use crate::loc::graph::mode_mask;
    use crate::catalog::Mode;

    fn o() -> Point {
        Point::new(40.0, -111.0)
    }

    fn grid() -> Arc<StreetGraph> {
        Arc::new(StreetGraph::from_ways(&grid_ways(o(), 5, 100.0)).unwrap())
    }

    fn start_east(p: &LocParams) -> Bridge {
        let at = destination(destination(o(), 0.0, 100.0), 90.0, 20.0);
        Bridge::start(Some(grid()), Frame::new(o()), mode_mask(Mode::Walk), at, [[9.0, 0.0], [0.0, 9.0]], 0.05, None, 90.0, 0, p)
    }

    #[test]
    fn steps_along_a_street_move_the_cloud_along_it() {
        let p = LocParams::default();
        let mut b = start_east(&p);
        for _ in 0..20 {
            b.step(2.0, Steer { theta_deg: 90.0, sigma_deg: 15.0 }, &p);
        }
        let (pos, unc, course) = b.estimate();
        let want = destination(destination(o(), 0.0, 100.0), 90.0, 60.0);
        assert!(distance_m(pos, want) < 8.0, "{} m off", distance_m(pos, want));
        assert!(unc.is_finite() && unc > 0.0 && course.is_some_and(|c| (c - 90.0).abs() < 15.0));
    }

    #[test]
    fn at_a_crossing_the_heading_picks_the_street() {
        let p = LocParams::default();
        let mut b = start_east(&p);
        for _ in 0..40 {
            b.step(2.0, Steer { theta_deg: 90.0, sigma_deg: 15.0 }, &p); // to the crossing at 100 m east
        }
        for _ in 0..20 {
            b.step(2.0, Steer { theta_deg: 0.0, sigma_deg: 15.0 }, &p); // turn north
        }
        let (pos, _, _) = b.estimate();
        let want = destination(destination(destination(o(), 0.0, 100.0), 90.0, 100.0), 0.0, 40.0);
        assert!(distance_m(pos, want) < 12.0, "{} m off", distance_m(pos, want));
    }

    #[test]
    fn the_same_seed_replays_the_same_cloud() {
        let p = LocParams::default();
        let (mut a, mut b) = (start_east(&p), start_east(&p));
        for _ in 0..10 {
            a.step(1.5, Steer { theta_deg: 90.0, sigma_deg: 20.0 }, &p);
            b.step(1.5, Steer { theta_deg: 90.0, sigma_deg: 20.0 }, &p);
        }
        assert_eq!(a.estimate().0, b.estimate().0);
    }

    #[test]
    fn without_a_graph_the_cloud_follows_the_heading() {
        let p = LocParams::default();
        let mut b = Bridge::start(None, Frame::new(o()), mode_mask(Mode::Walk), o(), [[4.0, 0.0], [0.0, 4.0]], 0.05, None, 0.0, 0, &p);
        for _ in 0..10 {
            b.step(2.0, Steer { theta_deg: 0.0, sigma_deg: 10.0 }, &p);
        }
        assert!(distance_m(b.estimate().0, destination(o(), 0.0, 20.0)) < 4.0);
        let (mean, cov) = b.moments();
        assert!(distance_m(mean, b.estimate().0) < 2.0 && cov[0][0] > 0.0 && cov[1][1] > 0.0);
    }
}
```

`core/src/game.rs` tests:

```rust
    #[test]
    fn a_bridged_estimate_uncovers_fog_and_squares_and_nothing_else() {
        let mut g = cartographer_game();
        let before = (g.stats.distance_m, g.counters.steps_last, g.last_pos(), g.done.clone());
        let far = destination(home(), 0.0, 2000.0);
        let b = Estimate { source: Source::Bridged, accepted: false, ..fixat(far, 100) };
        g.on_estimate(&b, None);
        assert!(g.fog.cells.contains(&crate::fog::cell_of(far)), "the square is seen");
        assert!(cartographer_counter(&g) > 0.0, "and Cartographer counts it");
        assert_eq!((g.stats.distance_m, g.counters.steps_last, g.last_pos(), g.done.clone()), before, "no distance, steps, position or quest");
    }
```

(check `cartographer_game` puts zone 1 open and its counter at 0 before; adjust the helper call if it needs a first fix.)

Append to `core/tests/loc_scenarios.rs`:

```rust
use apgo_core::loc::bench::{reanchor, HeadingSim};
use apgo_core::loc::LocParams;

fn gap_walk(heading: HeadingSim) -> Scenario {
    // 2 min east along the y = 100 m street (good GPS, offset learned), then a 60 s gap: 23 s east to the x = 200 m crossing, turn, 37 s
    // north; GPS returns on the north street.
    let start = destination(origin(), 0.0, 100.0);
    let mut s = Scenario::walk(start, vec![east(200.0, 1.4), Leg::Move { bearing_deg: 0.0, dist_m: 140.0, speed_mps: 1.4 }], 5.0);
    s.steps = true;
    s.with_velocity = true;
    s.heading = Some(heading);
    s.gaps = vec![(120, 180)];
    s
}

fn gap_errors(s: &Scenario, params: &LocParams) -> (Vec<f64>, Vec<f64>) {
    let graph = Arc::new(StreetGraph::from_ways(&grid_ways(origin(), 5, 100.0)).unwrap());
    let (mut errs, mut jumps) = (Vec::new(), Vec::new());
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &ReplayOpts { graph: Some(graph.clone()), params: params.clone(), ..opts(Mode::Walk) });
        let (e, j) = reanchor(&r.truth, &out, s.t0_ms + 180_000).expect("bridged through the gap");
        errs.push(e);
        jumps.push(j);
    }
    (errs, jumps)
}

#[test]
fn a_sixty_second_gap_with_a_turn_is_bridged_within_fifteen_metres() {
    let (errs, jumps) = gap_errors(&gap_walk(HeadingSim::in_hand()), &LocParams::default());
    assert!(percentile(&errs, 0.9) <= 15.0, "p90 {:.1} {errs:?}", percentile(&errs, 0.9));
    assert!(jumps.iter().all(|j| *j <= 10.0), "display jumps {jumps:?}");
}

#[test]
fn a_pocketed_phone_is_bridged_with_its_learned_offset_and_beats_the_raw_compass() {
    let s = gap_walk(HeadingSim::pocket(vec![(0, 90.0)]));
    let graph = Arc::new(StreetGraph::from_ways(&grid_ways(origin(), 5, 100.0)).unwrap());
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let before_gap: Vec<_> = r.fixes.iter().filter(|f| f.t_ms < s.t0_ms + 120_000).copied().collect();
        // the learned offset at the start of the gap, from a replay of the first two minutes (fixes and compass)
        let mut probe = apgo_core::loc::Locator::default();
        probe.set_graph(Some(graph.clone()));
        let mut evs: Vec<(i64, u8, usize)> = before_gap.iter().enumerate().map(|(i, f)| (f.t_ms, 2, i)).collect();
        evs.extend(r.headings.iter().enumerate().filter(|(_, h)| h.t_ms < s.t0_ms + 120_000).map(|(i, h)| (h.t_ms, 1, i)));
        evs.sort_unstable();
        for (_, kind, i) in evs {
            if kind == 1 {
                probe.on_heading(&r.headings[i]);
            } else {
                probe.on_fix(&before_gap[i]);
            }
        }
        let d = apgo_core::loc::heading::wrap_deg(probe.carry().delta_deg() - 90.0).abs();
        assert!(d <= 15.0, "seed {seed}: offset off by {d:.1} deg");
    }
    let (with_offset, _) = gap_errors(&s, &LocParams::default());
    let (raw, _) = gap_errors(&s, &LocParams { carry_enabled: false, ..LocParams::default() });
    assert!(percentile(&with_offset, 0.9) <= 20.0, "p90 {:.1}", percentile(&with_offset, 0.9));
    assert!(percentile(&raw, 0.9) > percentile(&with_offset, 0.9), "the raw compass must do worse: {:.1} vs {:.1}", percentile(&raw, 0.9), percentile(&with_offset, 0.9));
}

#[test]
fn a_carry_change_in_the_gap_falls_back_to_course_and_streets() {
    let s = gap_walk(HeadingSim::pocket(vec![(0, 90.0), (130, 0.0)]));
    let (errs, _) = gap_errors(&s, &LocParams::default());
    assert!(percentile(&errs, 0.9) <= 25.0, "p90 {:.1} {errs:?}", percentile(&errs, 0.9));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd core && cargo test -p apgo-core loc::bridge game::`
Expected: FAIL to compile: `file not found for module 'bridge'`, `cannot find function 'reanchor'`.

- [ ] **Step 3: Write `core/src/loc/bridge.rs`** (above the tests)

```rust
//! Layer 3: a particle filter that keeps the position going through a GPS gap from steps and heading, on the street graph (Walk and Run).
//! Its positions feed the map, fog and Cartographer squares only.

use std::sync::Arc;

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};

use crate::geo::Point;
use crate::loc::bench::gauss;
use crate::loc::frame::Frame;
use crate::loc::graph::StreetGraph;
use crate::loc::heading::{circular_mean_deg, wrap_deg};
use crate::loc::mat::Mat;
use crate::loc::{LocParams, ACC_TO_SIGMA};
use crate::num::{count_f64, floor_usize};

/// The bearing the bridge steers by, and its sigma, degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Steer {
    /// Bearing, degrees from north.
    pub theta_deg: f64,
    /// Sigma, degrees.
    pub sigma_deg: f64,
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    seg: Option<usize>,
    off: f64,
    dir: f64,
    en: [f64; 2],
    s: f64,
    b_deg: f64,
    course_deg: f64,
    w: f64,
}

/// The particle cloud of one GPS gap.
#[derive(Debug, Clone)]
pub struct Bridge {
    graph: Option<Arc<StreetGraph>>,
    frame: Frame,
    mask: u8,
    ps: Vec<Particle>,
    rng: StdRng,
    /// When the gap started being bridged, Unix ms.
    pub started_ms: i64,
    /// The last step batch, Unix ms.
    pub last_step_ms: i64,
}

fn heading_weight(d_deg: f64, sigma_deg: f64) -> f64 {
    (-0.5 * (d_deg / sigma_deg.max(1.0)).powi(2)).exp()
}

impl Bridge {
    /// Start a cloud from the filter's `N(mean, cov)` (`cov` in m^2, east/north): 90 % projected onto streets within 3 sigma (preferring the
    /// matcher's confident segment `prefer`), 10 % off-network; each particle draws a step scale `N(1, sigma_k)` and a heading bias `N(0, 10)`.
    #[allow(clippy::too_many_arguments)] // the gap's starting facts
    #[must_use]
    pub fn start(graph: Option<Arc<StreetGraph>>, frame: Frame, mask: u8, mean: Point, cov: Mat<2, 2>, sigma_k: f64, prefer: Option<(usize, f64)>, course_deg: f64, t_ms: i64, p: &LocParams) -> Self {
        let frame = graph.as_ref().map_or(frame, |g| *g.frame());
        let mut rng = StdRng::seed_from_u64(p.bridge_seed ^ t_ms.unsigned_abs());
        let m = frame.to_enu(mean);
        let l00 = cov[0][0].max(1e-6).sqrt();
        let l10 = cov[1][0] / l00;
        let l11 = (cov[1][1] - l10 * l10).max(1e-6).sqrt();
        let sigma = (0.5 * (cov[0][0] + cov[1][1])).max(1.0).sqrt();
        let n = p.bridge_particles.max(10);
        let n_off = floor_usize(count_f64(n) * p.bridge_off_share);
        let mut ps = Vec::with_capacity(n);
        for i in 0..n {
            let (z0, z1) = (gauss(&mut rng), gauss(&mut rng));
            let en = [m[0] + l00 * z0, m[1] + l10 * z0 + l11 * z1];
            let mut part = Particle {
                seg: None,
                off: 0.0,
                dir: 1.0,
                en,
                s: (1.0 + sigma_k.max(p.bridge_sigma_k_min) * gauss(&mut rng)).max(0.3),
                b_deg: p.bridge_bias_sigma_deg * gauss(&mut rng),
                course_deg,
                w: 1.0,
            };
            if let (Some(g), true) = (graph.as_ref(), i >= n_off) {
                let here = frame.to_geo(en);
                let cands = g.candidates(here, 3.0 * sigma, 8, mask);
                let pick = prefer.filter(|_| i % 2 == 0).and_then(|(seg, _)| cands.iter().find(|c| c.seg == seg)).or_else(|| cands.first());
                if let Some(c) = pick {
                    let forward = wrap_deg(g.bearing_deg(c.seg, 1.0) - course_deg).abs() <= 90.0;
                    part.seg = Some(c.seg);
                    part.off = c.off_m;
                    part.dir = if forward { 1.0 } else { -1.0 };
                    part.en = g.en_at(c.seg, c.off_m);
                    part.course_deg = g.bearing_deg(c.seg, part.dir);
                }
            }
            ps.push(part);
        }
        let mut b = Self { graph, frame, mask, ps, rng, started_ms: t_ms, last_step_ms: t_ms };
        b.normalize();
        b
    }

    /// Move every particle `dist_m` (times its own scale): along its street, choosing at each node by heading; off-network along the heading.
    /// Weights follow the heading likelihood (off-network ones at half weight); resample when `N_eff < N/2`.
    pub fn step(&mut self, dist_m: f64, steer: Steer, p: &LocParams) {
        let graph = self.graph.clone();
        for i in 0..self.ps.len() {
            let mut part = self.ps[i];
            let d = dist_m * part.s;
            match (part.seg, graph.as_ref()) {
                (Some(_), Some(g)) => {
                    self.walk_graph(g, &mut part, d, steer);
                    part.w *= heading_weight(wrap_deg(part.course_deg - steer.theta_deg), steer.sigma_deg);
                }
                _ => {
                    let th = steer.theta_deg + part.b_deg + 5.0 * gauss(&mut self.rng);
                    let (s, c) = th.to_radians().sin_cos();
                    part.en = [part.en[0] + d * s, part.en[1] + d * c];
                    part.course_deg = th.rem_euclid(360.0);
                    part.w *= p.bridge_off_weight;
                }
            }
            self.ps[i] = part;
        }
        self.normalize();
        let n_eff = 1.0 / self.ps.iter().map(|x| x.w * x.w).sum::<f64>();
        if n_eff < count_f64(self.ps.len()) / 2.0 {
            self.resample(p);
        }
    }

    fn walk_graph(&mut self, g: &StreetGraph, part: &mut Particle, d: f64, steer: Steer) {
        let mut left = d;
        for _ in 0..20 {
            let Some(seg) = part.seg else { return };
            let len = g.seg(seg).len_m;
            let room = if part.dir > 0.0 { len - part.off } else { part.off };
            if left <= room {
                part.off += part.dir * left;
                break;
            }
            left -= room;
            let node = if part.dir > 0.0 { g.seg(seg).b } else { g.seg(seg).a };
            let options: Vec<(usize, f64)> = g.leaving(node, self.mask).into_iter().filter(|(s, _)| *s != seg).collect();
            let options = if options.is_empty() { vec![(seg, -part.dir)] } else { options }; // dead end: turn back
            let target = steer.theta_deg + part.b_deg;
            let weights: Vec<f64> = options.iter().map(|(s, dir)| heading_weight(wrap_deg(g.bearing_deg(*s, *dir) - target), steer.sigma_deg) + 1e-9).collect();
            let total: f64 = weights.iter().sum();
            let mut u = self.rng.random::<f64>() * total;
            let mut pick = options[0];
            for (o, w) in options.iter().zip(&weights) {
                u -= w;
                if u <= 0.0 {
                    pick = *o;
                    break;
                }
            }
            part.seg = Some(pick.0);
            part.dir = pick.1;
            part.off = if pick.1 > 0.0 { 0.0 } else { g.seg(pick.0).len_m };
        }
        if let Some(seg) = part.seg {
            part.en = g.en_at(seg, part.off);
            part.course_deg = g.bearing_deg(seg, part.dir);
        }
    }

    fn normalize(&mut self) {
        let total: f64 = self.ps.iter().map(|x| x.w).sum();
        let n = count_f64(self.ps.len());
        for x in &mut self.ps {
            x.w = if total > 0.0 && total.is_finite() { x.w / total } else { 1.0 / n };
        }
    }

    fn resample(&mut self, p: &LocParams) {
        let n = self.ps.len();
        let step = 1.0 / count_f64(n);
        let mut u = self.rng.random::<f64>() * step;
        let (mut c, mut i) = (self.ps[0].w, 0);
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            while u > c && i + 1 < n {
                i += 1;
                c += self.ps[i].w;
            }
            let mut q = self.ps[i];
            q.w = step;
            if let (Some(seg), Some(g)) = (q.seg, self.graph.as_ref()) {
                q.off = (q.off + p.bridge_roughen_m * gauss(&mut self.rng)).clamp(0.0, g.seg(seg).len_m);
                q.en = g.en_at(seg, q.off);
            }
            out.push(q);
            u += step;
        }
        self.ps = out;
    }

    /// Weighted mean of the heaviest cluster (particles on the same segment, or off-network), its 68 % radius over the whole cloud, and the
    /// cluster's course.
    #[must_use]
    pub fn estimate(&self) -> (Point, f64, Option<f64>) {
        let mut by: std::collections::BTreeMap<Option<usize>, f64> = std::collections::BTreeMap::new();
        for x in &self.ps {
            *by.entry(x.seg).or_insert(0.0) += x.w;
        }
        let heaviest = by.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(k, _)| k).unwrap_or(None);
        let cluster: Vec<&Particle> = self.ps.iter().filter(|x| x.seg == heaviest).collect();
        let wsum: f64 = cluster.iter().map(|x| x.w).sum::<f64>().max(1e-12);
        let mean = [cluster.iter().map(|x| x.w * x.en[0]).sum::<f64>() / wsum, cluster.iter().map(|x| x.w * x.en[1]).sum::<f64>() / wsum];
        let spread = self.ps.iter().map(|x| x.w * ((x.en[0] - mean[0]).powi(2) + (x.en[1] - mean[1]).powi(2))).sum::<f64>();
        let course = circular_mean_deg(&cluster.iter().map(|x| x.course_deg).collect::<Vec<_>>());
        (self.frame.to_geo(mean), ACC_TO_SIGMA * (spread / 2.0).max(0.0).sqrt(), course)
    }

    /// The whole cloud moment-matched to a Gaussian (mean, covariance m^2): the filter's prior when GPS returns.
    #[must_use]
    pub fn moments(&self) -> (Point, Mat<2, 2>) {
        let mean = [self.ps.iter().map(|x| x.w * x.en[0]).sum::<f64>(), self.ps.iter().map(|x| x.w * x.en[1]).sum::<f64>()];
        let mut cov = [[0.0; 2]; 2];
        for x in &self.ps {
            let d = [x.en[0] - mean[0], x.en[1] - mean[1]];
            for (r, dr) in cov.iter_mut().zip(d) {
                r[0] += x.w * dr * d[0];
                r[1] += x.w * dr * d[1];
            }
        }
        (self.frame.to_geo(mean), cov)
    }
}
```

`imm.rs`:

```rust
    /// A filter whose prior is a known position Gaussian (the bridge's cloud) and velocity, at `t_ms`.
    #[must_use]
    pub fn with_prior(pos: [f64; 2], cov: Mat<2, 2>, vel: [f64; 2], t_ms: i64, mode: Mode, p: &LocParams) -> Self {
        let mut imm = Self::new(pos, 1.0, t_ms, mode, p);
        for g in &mut imm.models {
            g.x = [pos[0], pos[1], vel[0], vel[1]];
            g.p[0][0] = cov[0][0].max(1.0);
            g.p[0][1] = cov[0][1];
            g.p[1][0] = cov[1][0];
            g.p[1][1] = cov[1][1].max(1.0);
        }
        imm.mu = [0.1, 0.85, 0.05]; // coming out of a walked gap: walking is by far the likeliest
        imm
    }
```

- [ ] **Step 4: Bridge in the `Locator`**

Fields: `bridge: Option<Bridge>`, `last_course: Option<f64>` (set in the accepted branch of `on_fix` when `e.course_deg` is `Some`).
`on_steps` becomes:

```rust
    pub fn on_steps(&mut self, total: i64, t_ms: i64, cadence: Option<f64>) -> Option<Estimate> {
        let before = self.steps.latest().map(|(_, n)| n);
        self.steps.push(total, t_ms);
        if self.moving_steps(t_ms) {
            self.hold = None;
        }
        let new = before.map_or(0, |b| (total - b).max(0));
        if self.bridge.is_none() && new > 0 && self.may_bridge(t_ms) {
            self.start_bridge(t_ms);
        }
        let p = self.params.clone();
        let b = self.bridge.as_mut()?;
        if t_ms - b.started_ms > p.bridge_max_ms {
            self.bridge = None;
            self.reset(); // 5 min of bridging: the next fix starts fresh, like a gap
            return None;
        }
        if new == 0 {
            if t_ms - b.last_step_ms > p.bridge_no_steps_ms {
                self.bridge = None; // no steps for 30 s: the pin ages into "predicted"
            }
            return None;
        }
        let gap_s = crate::num::i64_to_f64(t_ms - self.last_accepted_ms.unwrap_or(t_ms)) / 1000.0;
        let f = cadence.or_else(|| self.steps.cadence(t_ms)).unwrap_or(1.8);
        let cadence_changed = self.steps.cadence(t_ms - 2_000).is_some_and(|c0| (f - c0).abs() > 0.2 * c0);
        self.carry.watch_gap(&self.compass, t_ms, cadence_changed, &p);
        let (theta, sigma) = self.carry.heading_for_gap(&self.compass, t_ms, self.last_course, gap_s, &p)?;
        let dist = crate::num::i64_to_f64(new) * step_length_m(f) * self.calib.k();
        let b = self.bridge.as_mut()?;
        b.step(dist, Steer { theta_deg: theta, sigma_deg: sigma }, &p);
        b.last_step_ms = t_ms;
        let (at, uncertainty_m, course_deg) = b.estimate();
        let est = Estimate {
            t_ms,
            lat: at.lat,
            lon: at.lon,
            uncertainty_m,
            speed_mps: dist / 2.0, // one step batch is about 2 s
            course_deg,
            motion: Motion::Walking,
            mode_probs: [0.0, 1.0, 0.0],
            source: Source::Bridged,
            verdict: Verdict::Used,
            accepted: false,
            ..Estimate::default()
        };
        self.last = Some(est);
        Some(est)
    }

    fn may_bridge(&self, t_ms: i64) -> bool {
        matches!(self.mode, Mode::Walk | Mode::Run)
            && self.imm.is_some()
            && self.steps.present()
            && self.last_accepted_ms.is_some_and(|a| t_ms - a > self.params.bridge_after_ms)
    }

    fn start_bridge(&mut self, t_ms: i64) {
        let (Some(imm), Some(frame)) = (&self.imm, self.frame) else { return };
        // the filter predicted to now: the gap is noticed 10 s after the last fix, the walker kept going meanwhile
        let (preds, c) = imm.predict_to(t_ms, self.mode, &self.params);
        let out = imm::combine(&preds, &c);
        let mean = frame.to_geo([out.x[0], out.x[1]]);
        let prefer = self.matcher.best().filter(|m| m.confidence >= self.params.match_show_confidence).and_then(|m| m.seg).map(|s| (s, 1.0));
        let mask = crate::loc::graph::mode_mask(self.mode);
        self.bridge = Some(Bridge::start(self.graph.clone(), frame, mask, mean, imm::pos_block(&out.p), self.calib.sigma_k(), prefer, self.last_course.unwrap_or(0.0), t_ms, &self.params));
    }

    /// Whether a GPS gap is being bridged.
    #[must_use]
    pub fn bridging(&self) -> bool {
        self.bridge.is_some()
    }
```

`reset` also clears the bridge (`self.bridge = None;`). In `on_fix`, right after
the unusable check and `self.last_t_ms = ...`, re-anchor:

```rust
        if let (Some(b), Some(frame)) = (self.bridge.take(), self.frame) {
            let (mean, cov) = b.moments();
            let vel = self.last_course.map_or([0.0, 0.0], |c| [1.2 * c.to_radians().sin(), 1.2 * c.to_radians().cos()]);
            self.imm = Some(Imm::with_prior(frame.to_enu(mean), cov, vel, b.last_step_ms, self.mode, &self.params));
            // the fix below is gated against the cloud; a gated one goes to the relocation rule
        }
```

`params.rs`: the eleven bridge fields. `metrics.rs`:

```rust
/// Bridging at the end of a gap: the error of the last bridged position against the truth at its time, and the display jump to the first
/// estimate after `gap_end_ms`. `None` if nothing was bridged.
#[must_use]
pub fn reanchor(truth: &[TruthPoint], r: &crate::loc::bench::Replay, gap_end_ms: i64) -> Option<(f64, f64)> {
    let last_b = r.estimates.iter().filter(|e| e.source == crate::loc::Source::Bridged && e.t_ms <= gap_end_ms).next_back()?;
    let first_after = r.estimates.iter().find(|e| e.t_ms >= gap_end_ms && e.source == crate::loc::Source::Gps)?;
    Some((distance_m(last_b.point(), interp(truth, last_b.t_ms)), distance_m(last_b.point(), first_after.point())))
}
```

- [ ] **Step 5: Run the tests and the gates**

Run: `cd core && cargo test -p apgo-core && cargo test -p apgo-core --release --test loc_scenarios`
Expected: PASS: bridge unit tests, the bridged-fog game test, and the three gap scenarios. Tune only `bridge_*` and `carry_*` within reason if a
p90 misses; never a threshold. If the raw-compass comparison does not come out worse, the offset is not being used: fix the code, not the
test.
Run: `just check-rust`
Expected: PASS (coverage >= 84; run `cargo llvm-cov -p apgo-core -p apgo-ffi --summary-only` and raise the floor in the `justfile` if the
measured line coverage, rounded down, is higher than 84).

- [ ] **Step 6: Commit**

```bash
git add core justfile
git commit -m "feat: bridge GPS gaps with steps on streets" -m "A seeded 300-particle filter on the street graph moves by steps times
the calibrated step length and steers by the carry-corrected compass or
the last course. Bridged positions show on the map and uncover fog and
squares only; the cloud becomes the filter prior when GPS returns." -m "Closes #88
Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Task 23: Tune `LocParams` on the real walks and record the scorecards

**Files:**

- Modify: `scripts/pull_diag.sh` (pull `files/atlas/` and the internal `diag/raw/` fallback), `scripts/diag_report.py` (raw and GNSS
  sections), `core/src/loc/params.rs` (defaults, only if the bench says so), `core/tests/loc_scenarios.rs` (thresholds only tighter),
  `justfile` / `android/app/build.gradle.kts` (coverage floors, only up)
- Never committed: scorecards, GeoJSON, parameter files and walk data (scratchpad only)

**Interfaces:**

- Consumes: the replay CLI with `--compare baseline`, `--params`, `--atlas`, `--game`, `--geojson` (Tasks 4, 8, 19); `rawfix`/`gnss` lines
  (Tasks 1, 12).
- Produces: `pull_diag.sh` also writes `<out>/files/atlas/*.json` and `<out>/diag/raw/*.jsonl`; `diag_report.py` prints `== raw track ==` and
  `== gnss ==` sections.

Data available today (both recorded before this program, so journal fallback only: positions, time and accuracy, no speed, bearing, steps
or compass, and no atlases, so no map matching on real walks yet):

| Walk | Where | Window |
| --- | --- | --- |
| 2026-10-07 (good GPS, median 5 m) | `/home/rasbandit/Documents/code-projects/Archipela-Go/diag/20261007-190430/files/journal.db` | whole file |
| 2026-10-08 (BALANCED bug, median 24.9 m) | `/tmp/claude-1000/-home-rasbandit-Documents-code-projects-Archipela-Go/2ebb9a6e-09f0-458e-9145-e238b2943d4d/scratchpad/diag-2001/files/journal.db` | `--from-ms 1791509000000` (the same file also holds the 10-07 game from 1791420614741) |

- [ ] **Step 1: Pull atlases and the raw track** (`scripts/pull_diag.sh`, after the games loop)

```bash
# Realm atlases (street graph for map matching in the replay bench).
mkdir -p "$out/files/atlas"
for a in $(adb shell run-as "$pkg" ls files/atlas 2>/dev/null | tr -d '\r'); do
  adb exec-out run-as "$pkg" cat "files/atlas/$a" > "$out/files/atlas/$a"
done
```

and in the internal-storage fallback branch also copy `files/diag/raw/*` into `$out/diag/raw/` (`mkdir -p "$out/diag/raw"` and the same
`ls`/`cat` loop on `files/diag/raw`). The external branch already pulls `diag/` recursively (`adb pull "$ext/."`).

- [ ] **Step 2: Report the raw track and GNSS lines** (`scripts/diag_report.py`, at the end of `main`)

```python
    raw = []
    for f in sorted((root / "diag" / "raw").glob("raw-*.jsonl")):
        for line in f.read_text(errors="replace").splitlines():
            with contextlib.suppress(json.JSONDecodeError):
                raw.append(json.loads(line))
    fixes = [e for e in raw if e.get("tag") == "rawfix"]
    print(f"\n== raw track: {len(raw)} lines, {len(fixes)} fixes ==")
    if fixes:
        accs = sorted(e["acc"] for e in fixes if e.get("acc") is not None)
        provs: dict[str, int] = {}
        for e in fixes:
            provs[e.get("prov", "?")] = provs.get(e.get("prov", "?"), 0) + 1
        print(f"providers {provs}, mock {sum(1 for e in fixes if e.get('mock'))}, with speed {sum(1 for e in fixes if e.get('spd') is not None)}")
        if accs:
            print(f"accuracy m: median {accs[len(accs) // 2]:.1f}, p90 {accs[int(len(accs) * 0.9)]:.1f}")
        print(f"steps lines {sum(1 for e in raw if e.get('tag') == 'rawsteps')}, heading lines {sum(1 for e in raw if e.get('tag') == 'rawhead')}")
    gnss = [e for e in entries if e["tag"] == "gnss"]
    status = [e for e in gnss if e["msg"] == "status"]
    print(f"\n== gnss: {len(status)} status lines ==")
    for e in gnss:
        if e["msg"] == "hardware":
            print(f"hardware: {e.get('model')} {e.get('capabilities')}")
    if status:
        print(f"used satellites: mean {sum(e['used'] for e in status) / len(status):.1f}, dual frequency in {sum(1 for e in status if e.get('dual_freq'))} of {len(status)}")
```

Run: `just lint` (ruff on `scripts/*.py`) and `python3 scripts/diag_report.py /tmp/claude-1000/-home-rasbandit-Documents-code-projects-Archipela-Go/2ebb9a6e-09f0-458e-9145-e238b2943d4d/scratchpad/diag-2001`
Expected: lint clean; the report ends with `== raw track: 0 lines, 0 fixes ==` and `== gnss: 0 status lines ==` for this old pull.

- [ ] **Step 3: Score both walks with the defaults** (`S` = the scratchpad, `M` and `D` as in Task 4 Step 8)

```bash
cd core
cargo run --release --example replay -- "$M/files/journal.db" --mode walk --compare baseline \
  --game "$M/files/games/$(ls "$M/files/games" | head -1)" --geojson "$S/walk-1007.geojson" | tee "$S/scorecard-1007-default.txt"
cargo run --release --example replay -- "$D/files/journal.db" --mode walk --from-ms 1791509000000 --compare baseline \
  --game "$D/files/games/0fe19fe8-d3da-42b3-9a9c-0a91955d2ae4.json" --geojson "$S/walk-1008.geojson" | tee "$S/scorecard-1008-default.txt"
```

Expected: the filter column beats legacy on standing jitter, false jumps and odometer wobble; arrival lag p90 within a few seconds of
legacy on the 10-07 walk. Open the GeoJSON files in any viewer (geojson.io offline copy, QGIS) and look at corners and stops.

- [ ] **Step 4: Tune, one parameter group at a time**

Write candidate parameter sets as JSON in the scratchpad (missing fields keep their defaults), for example
`echo '{"sigma_a_walk": 0.4, "hold_exit_min_m": 4.0}' > "$S/p-walk-04.json"`, and replay with `--params "$S/p-walk-04.json"`. Order:
`sigma_a_walk` (0.3 to 0.8), `pi_slow` rows, `hold_*`, `gate_*` (only the soft bound), then `match_beta_m` (3 to 10, once atlases exist).
Keep a table in `$S/tuning.md` (not committed) of parameter set -> RMS-to-reference, jitter, false jumps, arrival lag p50/p90, rejected
share. A set is better only if it improves the real walks without breaking a CI scenario:
`cargo test -p apgo-core --release --test loc_scenarios` must stay green with it as the default.

- [ ] **Step 5: Apply the winners, tighten thresholds that now have room, raise floors**

Change only the defaults in `params.rs` that won. Where a scenario passes with a wide margin on every seed (for example straight-walk RMS
well under 3 m), the threshold may be tightened (thresholds only move toward stricter). Measure coverage
(`cd core && cargo llvm-cov -p apgo-core -p apgo-ffi --summary-only`, `cd android && ./gradlew :app:koverLogDebug`) and raise the floors to
the measured values rounded down.

Run: `just check-rust && ANDROID_HOME=/home/rasbandit/Android/Sdk just check-android && just check-hygiene`
Expected: PASS.

- [ ] **Step 6: Record the final scorecards for the PR** (scratchpad only)

Re-run Step 3 with the new defaults into `scorecard-1007-final.txt` and `scorecard-1008-final.txt`, and write `$S/pr-scorecards.md`
with both `--compare baseline` tables and a line per parameter changed. Nothing under `diag/` or the scratchpad is staged: check
`git status --short`.

- [ ] **Step 7: Commit**

```bash
git add scripts/pull_diag.sh scripts/diag_report.py core/src/loc/params.rs core/tests/loc_scenarios.rs justfile android/app/build.gradle.kts
git commit -m "chore: tune location parameters on walks" -m "Defaults chosen on the 2026-10-07 and 2026-10-08 replays against the
legacy baseline; the CI scenarios stay green. pull_diag also pulls the
atlases and the raw track, diag_report summarises them." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

**Re-running with new data** (the HIGH_ACCURACY debug build records `diag/raw/`): walk with the debug build, then

```bash
scripts/pull_diag.sh "$S/diag-$(date +%m%d)"
python3 scripts/diag_report.py "$S/diag-$(date +%m%d)"
cd core && cargo run --release --example replay -- "$S/diag-$(date +%m%d)" --mode walk --compare baseline \
  $(for a in "$S/diag-$(date +%m%d)"/files/atlas/*.json; do printf -- '--atlas %s ' "$a"; done) \
  --game "$S/diag-$(date +%m%d)/files/games/<id>.json" --geojson "$S/walk-new.geojson"
```

The pulled directory has `diag/raw/`, so the replay reads raw fixes with speed, bearing, steps and compass (bridging and carry offset are
then measured too). Repeat Steps 4 to 7 with it; it is the reference walk from then on.

---

## Task 24: Docs: the context doc, the architecture status, the design system, the iOS plan

**Files:**

- Create: `docs/context/location-estimation.md`
- Modify: `docs/context/v1-architecture-and-status.md` (the "Fix quality" bullet of the journal section, "Verified", "Not done"),
  `docs/context/ui-design-system.md` (the pin rule gains `Me`), `docs/context/working-in-this-repo.md` (replay command and the mock flag)
- Not modified: `CLAUDE.md` (owner rule): the report asks the owner to add the index line
  `If you need how the location filter, map matching and gap bridging work (and how to tune them), see docs/context/location-estimation.md`.

- [ ] **Step 1: Write `docs/context/location-estimation.md`**

Sections (each short, facts only, pointing at code rather than repeating it):

1. *What it is*: the three layers and what each may feed (copy the spec's layer table), `Locator` as a `#[serde(skip)]` field of `Game`.
2. *Where things are*: the file table of this plan's File structure (core `loc/` files, FFI records, Android files).
3. *Verdicts and `accepted`*: the seven verdicts, the accepted rule, what the near-miss log says for each.
4. *Parameters*: `LocParams` groups and that the replay reads them as JSON (`--params`); the values that were tuned in Task 23 and why.
5. *Bench*: the scenario list (`core/tests/loc_scenarios.rs`, 20 seeds), the replay command (both input kinds), the scorecard metrics, and the
   rule that real walks are never committed.
6. *Phone side*: request table (GMS, no GMS, Android 8-11), the 1 s / 5 s screen rule, raw recording in debug builds (`diag/raw/`), the
   `files/allow_mock` flag, GNSS lines, heartbeat perf fields, the battery step.
7. *iOS integration plan*: the spec's iOS table as the checklist for the Swift host (same `Engine`, `FixIn` with `None` for negative values,
   `CMPedometer` as `phone.pedometer`, `trueHeading`), plus "no Swift in this PR".
8. *Gotchas found while building* (fill from the implementation; at least): `Pi(dt)` rows are capped at 0.9 off-diagonal; a 3-fix burst
   that agrees with itself over 2 s is a relocation by the spec's rule (the CI burst is scattered); junction nodes must be pinned before
   Douglas-Peucker and ways simplified once per scan; `Arc<StreetGraph>` must stay `Send + Sync` (route cache lives in the matcher); the
   simulator's clock runs ahead of the wall clock (the first real fix after it resets the filter); `LocationComponent` images come from
   the style (`foregroundName`/`gpsName`), so every `MarkerSpec.Me` variant is added before activation.

- [ ] **Step 2: Update the other docs**

`v1-architecture-and-status.md`: replace the "Fix quality" bullet with one pointing at the new doc ("estimates from the IMM filter, see
location-estimation.md"); under "Verified" add what Tasks 13, 15 and 23 actually verified (emulator, phone, replays); under "Not done" add
what is not (no outdoor walk with the new build yet unless one happened, map matching unscored on real walks until atlases are pulled,
iOS). `ui-design-system.md`: in the pin rule add "`Me` (the player: person or arrow, hollow while bridged, grey when stale; drawn by the
LocationComponent from these images)". `working-in-this-repo.md`: the replay command and `adb shell run-as dev.apgo2.app touch files/allow_mock`.

- [ ] **Step 3: Run the gate**

Run: `just check-hygiene`
Expected: PASS (typos, gitleaks, markdownlint, shellcheck, script tests).

- [ ] **Step 4: Commit**

```bash
git add docs/context
git commit -m "docs: document location estimation" -m "How the filter, matcher and bridge fit together, the bench and tuning
loop, the phone request rules and the iOS checklist; status and design
system docs updated." -m "Refs #89

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Self-review against the spec

Spec coverage, section by section:

| Spec section | Task(s) |
| --- | --- |
| Decisions table (every row) | Global Constraints; HIGH_ACCURACY done in `ef927d8`; rate 11; journal 10; pin 15; matching input 18/19; street graph 16/17; gap bridging 22; bridged feeds 9/22; step length and source 21; carry offset 20; arrow 14/15; real walks 4/23; no GMS 11; simulated fixes 7/9 |
| Layer 1: state, models, measurement, IMM cycle, `Pi(dt)`, initial state | 5 (models, cycle), 7 (measurement, step evidence) |
| Gating, outliers, relocation, reset, effect on quests | 6, 7, 9 (dwell/away pause) |
| Stationary hold and adaptive smoothing | 7, scenarios 8 |
| Outputs (`Estimate`, `accepted`) | 2, 7 |
| Layer 2: street graph (source, topology, graph, edges by mode, old atlases, size) | 16, 17 |
| Layer 2: model (inputs, sigma, candidates, emissions, transitions, beta, U-turn, on/off, decoding, break, confidence) | 18 |
| Display rule | 19 |
| Layer 3: particle filter (start, end, particles, state, init, step length, heading, propagate, weights, resampling, output, re-anchor, Bike/Drive) | 22 (Bike/Drive coasting via 14's display) |
| What bridged positions feed | 9 (`on_estimate`), 22 (test) |
| Step calibration | 21 |
| Carry offset | 20 |
| Display (#86): pin, glide, circle, arrow, bridged/stale, drawables, trace, camera, text, ME layer removed | 14, 15, 19 (trace) |
| Best request (#85): table, rate decision and tests, GMS detection, batching, fix fields, mock, GNSS status, steps, step calibration I/O, heading sensor, battery guidance | 1 (fields, batching), 11, 12, 13, 15 (heading), 21 (calibration I/O) |
| iOS integration plan | 24 (context doc checklist; no Swift) |
| Data flow and API (core types, `Game::on_fix` replacements, FFI table) | 2, 7, 9, 10, 14, 19, 21 |
| Replay bench: recording, CLI, metrics, CI scenarios, real walks | 1, 3, 4, 8, 19, 21, 22, 23 |
| Performance budget | timing harnesses 8 and 17, heartbeat perf 12, replay cost line 8 |
| Testing strategy (unit, game, scenarios, property, FFI, Android unit, device) | every task; property test 7; FFI 10, 14, 19; device checks 13, 15, 23 |
| Migration and compatibility | 9 (saves, not saved), 16 (atlases), 10 (journal), 10/14 (FFI) |
| Sequenced tasks 0 to 11 | mapped onto Tasks 1 to 24 in the owner's order |

Placeholder scan: no TBD/TODO; every code step carries code; the two "if clippy prefers" notes give the exact alternative.

Type consistency checked: `Estimate`, `RawFix`, `Verdict`, `Source`, `Motion` (Task 2) are used unchanged later; `Locator::on_steps(total, t_ms,
cadence)` keeps one signature from Task 7 to 22; `ReplayOpts` gains only defaulted fields; `Game::on_steps(total, t_ms, cadence)` from Task 9
matches the FFI in Task 10; `StreetGraph::candidates(p, radius_m, max, mask)` is called with that order in Tasks 18 and 22; `MarkerSpec.Me(heading,
state)` keys parse back in Task 15's test.

Review Focus: each line has its test in the named task (1: Tasks 1 and 6; 2, 3, 4, 5: Task 7).

## Spec gaps and contradictions found while planning (listed, not changed)

1. **Burst vs relocation.** "3 consecutive gated fixes that agree ... and span >= 2 s -> reset" also fires on a "3-fix 80 m burst" at 1 Hz if
   the three fixes agree with each other, yet the scenario requires the burst to be gated. The plan's burst scatters its three fixes
   (bearings 0/120/240); a same-direction burst would relocate. Owner decision needed if same-direction bursts matter.
2. **`Pi(dt)` goes negative.** Per-second rates x `min(dt, 10)` make the Bike/Drive W row sum to 2.0 at 10 s (and the Walk F row to 1.0). The
   plan caps a row's off-diagonal at 0.9 (`max_offdiag_share`).
3. **4-dof soft gate missing.** Only the 2-dof soft bound (9.21) is given; the plan uses 13.28 (chi-square 99 %, 4 dof).
4. **Unstated `sigma_a`.** The W model in Bike/Drive zones and the F model in Walk/Run zones have no value; the plan uses 0.5 and 1.5.
5. **TOML parameters.** The replay reads `--params` as JSON: TOML would need a new crate, which the Linear algebra decision's spirit and
   `deny.toml` discourage.
6. **RTS "IMM" reference.** Implemented as an RTS smoother of the constant-velocity model, not a full IMM smoother.
7. **`pull_diag.sh` and atlases.** It does not pull `files/atlas/`, and the internal-storage fallback is not recursive; neither existing pull
   has atlases, so map matching cannot be scored on the two real walks (Task 23 fixes the script for the next pull). The 2026-10-08
   journal holds only the fixes the old rules accepted (218 of the walk), not the 88 rejected ones.
8. **Drive street graph.** Scans fetch only walkable highways up to `secondary`; Drive edges are the car-usable subset, without primary or
   trunk roads. Drive is also not in `Mode::PLAY` today.
9. **`rawstate.zone_mode`.** Kotlin does not know the zone's mode; the plan logs `zone` (proximity) and the replay takes `--mode`.
10. **Fog from non-accepted estimates.** "Any estimate with source != Predicted" would let gated or blurry estimates uncover fog; the plan
    uses accepted or bridged estimates only (consistent with the bridged-feeds decision).
11. **"Predicted" threshold.** No age is given for a GPS estimate to become `predicted`; the plan uses 6 s (above the 5 s screen-off
    interval) and grows the uncertainty by `max(speed sigma, 0.5 m/s)` per second.
12. **Calibration window.** "20 s windows ... d >= 30 m" cannot both hold at 1.4 m/s (28 m); the plan closes a window at >= 20 s and >= 30 m.
13. **Synthetic cadence.** "steps (cadence 1.8 Hz)" with 1.4 m/s walking does not satisfy the cadence model (1.8 Hz gives 0.70 m, 1.26 m/s);
    the generator solves the cadence from the speed and the model.
14. **Compass "app visible, screen on".** The core cannot see it; it is implied because the map (the only place the arrow shows) is visible.
15. **`Engine::set_record_raw`.** The spec lists it but says the core does nothing with it; the plan leaves it out (Kotlin decides by build type).
16. **Coverage floor wording.** The spec says `check-rust` 80 %; the live floors are rust 84 and kotlin 18 (CLAUDE.md says kotlin 7), and only
    rise.
17. **`set_graph` signature.** The spec's `set_graph(g: Arc<StreetGraph>)` becomes `Option<Arc<StreetGraph>>` (a realm can have no streets).
18. **File layout.** The plan adds `loc/locator.rs`, `loc/heading.rs`, `loc/calib.rs` and `bench/` submodules beside the spec's file table, to
    keep files small; the spec's `mod.rs` still exports everything it lists.
19. **Sim fixes and distance.** A simulated fix resets the filter (spec), so its estimate starts "stationary" and adds no odometer distance;
    today's teleports added straight-line distance.
20. **"No discovery twice" index line.** `docs/context/location-estimation.md` needs a line in `CLAUDE.md`, which this program may not edit.
21. **Step-calibration scenario vs the calibration rule.** With `sigma_obs = 2 x mean uncertainty / d` (about 0.2 for a 30 m window at 3 m)
    the fast-adapt test `abs(k_obs - k) > 3 sqrt(var_k + sigma_obs^2)` needs a jump of about 0.6, and a 1.0 to 0.85 scale change moves the
    cadence only about 10 % (under the 20 % rule), so "within 0.05 of the new scale after 2 min" is unlikely with the spec's formula. The plan
    exposes the factor as `calib_obs_scale` (default 2.0, the spec) for Task 21/23 to tune; owner may prefer to change the rule instead.

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-10-08-location-quality.md`. Execution method (owner): subagent-driven, using
superpowers:subagent-driven-development. Please review the plan and confirm it captures what you want before implementation starts.
