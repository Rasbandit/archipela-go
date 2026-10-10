# Context Doc: location estimation (filter, map matching, gap bridging)

_Last verified: 2026-10-09 (branch `feat/location-quality`, tracker #89). Unit tests, CI scenarios and host replays only: no outdoor
walk with this build yet (#117)._

Spec: `docs/superpowers/specs/2026-10-08-location-quality-design.md` (binding; where the build differs, "Changed from the spec"
below and the parameter table say so). Plan and per-task rulings: `docs/superpowers/plans/2026-10-08-location-quality.md`. Library research:
`location-libraries-research.md`.

## What it is

Three layers in `apgo-core` module `loc`, behind one facade, `loc::Locator`. `Game` owns it as a `#[serde(skip)]` field: it is never
saved, so a restart (or a counting toggle, or a game open) starts a fresh filter, and Android and iOS reach it through the same FFI.

| Layer | Purpose | Feeds | Never feeds |
| --- | --- | --- | --- |
| 1 IMM Kalman (`imm.rs`) | Best estimate of position, velocity and motion state | Quests, fog, chains, odometer, journal, near-miss, map (when unmatched) | |
| 2 HMM matcher (`matcher.rs`, `graph.rs`) | Which street the player is on, how sure | Map pin (when confident), trace line, diag | Quests |
| 3 Particle filter (`bridge.rs`) | Position during a GPS gap from steps and heading on the street graph | Map pin, fog and Cartographer squares, IMM prior at re-anchor | Quests, other chains, odometer, journal |
| Bench (`bench/`, `examples/replay.rs`) | Record, replay, score | Parameter tuning, CI thresholds | Release builds (recording) |

`Game::on_fix(raw, steps)` runs `locator.on_fix` and then `Game::on_estimate`: quests see only the estimate, never a raw fix or
the matched position. Bridged estimates go through `Game::on_steps` to fog only.

## Where things are

Core (`core/src/loc/`):

| File | Holds |
| --- | --- |
| `mod.rs` | Public types: `Provider`, `RawFix`, `Estimate`, `Verdict`, `Motion`, `Source`, `HeadingIn`, `CompassAccuracy`, `DisplayPosition`; `mode_at`, `MAX_UNCERTAINTY_M` |
| `locator/mod.rs` | `Locator`: wires every layer (`on_fix`, `filter_fix`, `emit`, restarts) |
| `locator/{steps,odometer,maneuver,hold,display,gap,carry}.rs` | `StepHistory` and step evidence; `Odometer`; line-fit course; stationary hold; pin and trace; gap bridging and the compass in a gap; carry learning. `test_util.rs`: shared test helpers |
| `frame.rs` | ENU anchor, `to_enu` / `to_geo`, re-anchor at 5 km |
| `mat.rs` | Fixed-size matrices, inverse, Joseph-form update, 2x2 eigenvalue (no linear algebra crate) |
| `params.rs` | `LocParams`: every tunable, `Default`, serde (the replay reads it as JSON) |
| `imm.rs` | Three models, `Pi(dt)`, mixing, predict, update, gate, relocation, reset, unusable rules |
| `graph.rs` | `StreetGraph` (compact CSR, 80 m cell grid, full or degraded), candidates, bounded Dijkstra |
| `matcher.rs` | Online Viterbi with an off-network state, route cache, fixed-lag matched trace |
| `heading.rs` | Compass history, "held flat and steady" rule, carry-offset estimator |
| `calib.rs` | Step-length model and per-source step calibration |
| `bridge.rs` | Particle filter for GPS gaps |
| `bench.rs` + `bench/{synth,metrics,legacy,record,reference,run}.rs` | Synthetic walks, scorecard, `LegacyRules` (the old rules, bench only), recording readers, RTS reference, replay driver |

Elsewhere in the core: `game.rs` (`on_estimate`, `explain_near`, `attach_streets`/`set_streets`), `scan.rs` (`Atlas::ways`, encoded
polyline at 1e-7), `geo.rs` (`simplify` via `geo::SimplifyIdx`, `simplify_pinned`), `ffi/src/engine.rs` (`FixIn`, `HeadingIn`,
`PositionOut`, `StepCalIn`/`Out`, `on_fix`, `on_steps`, `on_heading`, `position`, `trace_matched[_since]`, `refresh_streets`),
`tests/loc_scenarios.rs`, `examples/replay.rs`.

Android (`android/app/src/main/java/dev/apgo2/`):

| File | Holds |
| --- | --- |
| `GpsPolicy.kt` | Rate by presence and screen (`forDecision`, `inZone`), `request`, `gmsRequest`, `gmsColdStart`, `providers(enabled, gms)` |
| `Sensors.kt` | Facade the app calls; Play services detection, GNSS callback, steps (2 s batch in a zone) |
| `LocationSource.kt`, `HeadingSource.kt` | Play services or LocationManager listener (batched, oldest first), cold start, fall-back; compass (fused or rotation vector), heading compare |
| `Compass.kt` | `CompassRate`, `CompassSource` (fused or rotation vector), `FusedHeadingWatch` (falls back from a silent fused compass), `HeadingCompare` |
| `RawTrack.kt` | `FixSample`, `FixTime` (fix time from `elapsedRealtimeNanos`), `RawLines` (raw-track line formats), `MockPolicy` |
| `DiagLog.kt` | `Diag.initRaw` / `Diag.raw`: the debug-only raw track |
| `Gnss.kt`, `FieldDiagnostics.kt` | GNSS status summary; heartbeat with `perf_n`, `perf_p50_us`, `perf_p99_us` of `engine.onFix` |
| `PinFeed.kt`, `MePin.kt` | `position()` to the pin: glide length, snap, circle, arrow, image (`MarkerSpec.Me`) |
| `QuestMap.kt` (`MapHolder.showMe`) | MapLibre `LocationComponent` in custom-location mode |
| `TraceBuffer.kt` | Matched trace on the map from `trace_matched_since` deltas |
| `StepCalStore.kt` | Step calibration in preferences `stepcal`, one entry per source |
| `BatteryGuide.kt`, `SetupSteps.kt` | "Keep tracking alive" setup step (OEM text in `ui/HelpText.kt`) |

## Verdicts and `accepted`

| Verdict | Meaning | Near-miss reason (`Game::explain_near`) |
| --- | --- | --- |
| `Used` | Fix used as measured | Today's reasons (zone locked, fog, trap, too fast, distance, "in range: counting") |
| `Soft` | Used with `R` inflated (a bit far from the prediction) | Same as `Used` |
| `Gated` | Rejected as a jump; models keep their prediction | "ignored as a GPS jump" |
| `Blurry` | A filter update (`Used` or `Soft`) whose estimate is over 35 m | "GPS uncertain (N m, needs 35 m)" |
| `Relocated` | 3 agreeing gated fixes: restart at the newest | Same as `Used` |
| `Reset` | Fresh filter (first fix, gap over 5 min, lost, counting toggle, game open, a fix over 5 min older than the last: a clock jump) | Same as `Used` |
| `Unusable` | Dropped before the filter: accuracy over 100 m, not newer than the last fix (up to 5 min older), out of range, NaN, mock, network after GNSS, dated before 2000-01-01 or after 2100-01-01 | "GPS fix unusable (too coarse, stale or from a mock app)" |

A bridged estimate (`Source::Bridged`) reads "position estimated from steps (GPS gap)" whatever its verdict. `accepted` is
`Verdict::may_count()` (`Used`, `Soft`, `Relocated`, `Reset`) with `uncertainty_m <= 35`, only for `Source::Gps`; bridged estimates
and estimates from a `Provider::Network` fix are always `accepted = false`. Quest trackers run only on accepted estimates. A restart over 35 m keeps its verdict (ruling FR-I1):
the pin snaps, the trace breaks, the matcher restarts, trackers pause and the odometer forgets its point, but it does not count
(`Estimate::uncertain()`: "GPS uncertain"). The reset-gap clock is the last fix the filter took; the bridge's clocks are the last
accepted estimate. A reset forgets the fix clock too, and a fix more than `reset_gap_ms` older than the last one restarts the filter
(a wall clock stepped back, or one fix dated in the future), so a wrong clock never locks a session out (adversarial review I1).
A forward-dated fix costs at most `reset_gap_ms` of dropped fixes. Steps and compass readings are no clock: they stop while the player
sits still (a bus outside zones, a resume from home, a garage), and a rule against them dropped every fix until the next step
(adversarial re-review N1, N3).

A fix after a bridged gap re-anchors the filter on the cloud (the pin stays continuous), but that prior is no evidence (adversarial
review C1). The fix is gated against the cloud itself, so a multipath fix 100 m off is still `Gated` (re-review N2: gating against
the widened prior took it, and alternating multipath then counted estimates about 100 m off). Only a fix that passes updates a prior
whose position variance is widened to at least the fix's accuracy squared per axis. The estimates stay `accepted = false` until 3
fixes (`Used` or `Soft`) have been taken since the re-anchor, about 3 s of quest delay at 1 Hz. The first taken fix ends the bridge.
The first accepted estimate after a bridged gap pauses dwell timers, as a restart does (`Locator::resumed_after_bridge`), so no
fix-less gap time counts toward a dwell (adversarial re-review). Time away is event-driven (main's Wanderlust rework): it runs from
leaving home whether fixes arrive or not, so a bridged gap counts as time away like any other stretch.

A non-accepted or bridged estimate sets `unseen_since_good`: until the next accepted estimate, a scheduled tick (`Game::tick`,
`next_due_ms`) does not finish a dwell the player may have left. Presence's zone proximity (`Engine::last_zone_proximity`) is worked
out in the core in the same pass as the fix, at this fix's own estimate (ruling E2; never a bridged one; the raw fix only before the
first estimate), so a jump the filter gates never moves presence.

## Parameters

Every number lives in `LocParams` (`params.rs`), grouped by layer: filter (models, `Pi`, gates, relocation, reset, hold, step
evidence, course, maneuver), display (predict, predicted/stale ages, compass rule), matching (`match_*`), carry offset (`carry_*`),
step calibration (`calib_*`), gap bridging (`bridge_*`). The replay takes `--params p.json`; missing fields keep their defaults, so
a candidate file holds only what it changes (`{"sigma_a_walk": 0.4}`).

Task 23 tuned on both journal walks and changed no default: no candidate improved both walks without breaking a CI scenario, and the
walks are sparse, journal-only data (no speed, steps, compass or atlas). The defaults that differ from the spec were set while
building, each by a ruling in the plan's ledger:

| Parameter | Spec | Now | Why |
| --- | --- | --- | --- |
| `sigma_a_walk` | 0.5 | 0.3 | In the spec's range; straight walks pass the gate |
| `max_offdiag_share` | none | 0.9 | `Pi(dt)` rows went negative at long `dt` |
| `gate_soft_4` | none | 13.28 | Chi-square 99 %, 4 dof (the spec gave only the 2-dof bound) |
| `course_max_sigma_deg`, `course_min_speed_mps` | 25, 0.8 | 35, 0.5 | Course from the W and F models only; walkers kept no course otherwise |
| `match_min_move_m`, `match_min_gap_ms` | 5 m / 5 s | 2 m / 1 s | Owner: feed every moving estimate; corners match within 3 s median |
| `match_stay_confidence` | none | 0.4 | Hysteresis: show at 0.7, keep showing down to 0.4 (node ambiguity at 1 Hz) |
| `calib_window_ms`, `calib_min_dist_m`, `calib_obs_scale` | 20 s / 30 m / 2.0 | 60 s / 80 m / 1.5 | The spec's windows never adapted; sigma from the step-predicted distance |
| `calib_path_step_m`, `calib_cusum_*` | none | 15 m, h 3, drift 0.25 | Path decimated against jitter; CUSUM change detection |
| `hold_quiet_max_m`, `hold_quiet_sigmas` | none | 15 m, 3.5 | A quiet step counter hides at most this much movement (FR-C1; 3 sigma let GNSS drift end standing holds) |
| `steps_present_ms` | none | 10 min | Only for a host that sends totals with fixes alone; a counter that sent a step event stays present (FR-N1) |
| `steps_fallback_ms` | none | 10 s | The step total sent with a fix is used only without a step event this long (FR-I3) |

## Changed from the spec

- Play services first (owner, 2026-10-09): `play-services-location` is added, fixes and the on-screen compass come from it (spec
  rows updated).
- `LocParams` is read as JSON (`--params p.json`), not TOML (no new crate). `Locator::set_graph` takes `Option<Arc<StreetGraph>>`.
  `Engine::set_record_raw` is not built (Kotlin decides by build type).
- The RTS reference smooths the constant-velocity model, not a full IMM smoother.
- Fog takes accepted or bridged estimates only (not every non-predicted one). An estimate turns `predicted` after 6 s without a fix.
- The arrow may use the compass while moving when there is no course yet (sparse fixes), not only when standing.
- Matcher input is every moving estimate (2 m / 1 s), not 5 m / 5 s; the street graph cell is 80 m, not 30 m.
- Bike and Drive have no 30 s coast cap of their own: the display ages them to `predicted` and then `stale` at 30 s.
- A simulated fix resets the filter, so it adds no odometer distance (the old teleports added straight-line distance).
- Only the FFI's `simulated` argument makes a simulated fix, and only in a debug build of the core (`cfg!(debug_assertions)`, so a
  release library never believes a fix whatever the host passes); a provider named `sim` is `Other`. A simulated fix still needs a
  finite, in-range position and a finite accuracy (adversarial review I3).
- CI gates that differ from the spec's table: straight walk RMS <= 2.5 m, p95 <= 4.5 m (tightened from 3 / 6 m, Task 23); corner turn
  lag median <= 7 s, max <= 10 s from the filter's course (ruling T8-R19 fallback; spec 4 s), and the matched street at a corner
  median <= 3 s, max <= 8 s (ruling T19-input; spec "within 5 s").
- Quiet step counter (rulings T8-R2, R18, FR-C1): in a Walk or Run zone, a hold with a present but quiet counter ends only on fixes
  `max(15 m, 3.5 sigma)` from the held point (two in a row), on steps, or by relocation; a counter is present once a step event came
  and stays present for the session (FR-N1: Android's counter reports only on change, so a 10 min window made a standing player's
  counter absent and the odometer gained 240 m in 25 min). A player moving without steps (wheelchair, stroller) is held at most about
  15 m: at 1 to 2.5 m/s the hold ends within 30 s (CI), at 0.5 m/s within about 45 s, and the shown position stays within 25 m.
- On long stands at poor accuracy the pin may step up to about 15 m when GNSS drift passes the quiet bound and the hold is re-placed;
  this is by design (FR-C1 bound, final review N2), and the odometer stays at 0.
- Steps reach the filter only as step events at their sensor time (`on_steps`, ruling FR-I3). The total Kotlin sends with a fix is a
  fallback reading, kept only when no step event came for 10 s, never counted as a step event, and replaced by a later-delivered event
  for an earlier time.
- The matched trace is simplified as it settles (final review F3), not at a cap: see Gotchas.

## Bench

- CI scenarios: `core/tests/loc_scenarios.rs`, 20 seeds each (`SEEDS`); a threshold holds on every seed unless it says p90. It covers
  straight walk (RMS 2.5 m, p95 4.5 m), corner, standing, spike and burst, relocation, bike stops, BALANCED stress, walk-in arrival,
  driving in a walk zone, matched corner, step relearn after a carry change, steady walk and fix gap and stop keeping the step scale,
  60 s gap with a turn, pocket offset, carry change in the gap, a mover without steps leaving a quiet hold, among others. Three
  `#[ignore]` timing harnesses: `timing_of_on_fix`, `timing_of_a_bridge_step_batch` and (in `matcher.rs`)
  `trace_work_per_input_stays_under_a_millisecond` (0.08 ms worst on the host).
- Replay: `cd core && cargo run --release --example replay -- <input> [--mode walk|run|bike|drive] [--from-ms N] [--to-ms N]
  [--game games/<id>.json] [--atlas files/atlas/<realm>.json]... [--params p.json] [--compare baseline] [--geojson out.geojson]
  [--csv out.csv]`. Input is a pulled directory with `diag/raw/*.jsonl` (raw fixes with speed, bearing, steps and compass), one
  raw `.jsonl`, or a `journal.db` (position, time, accuracy only). Run it on a copy: opening the journal touches its `-shm`.
- Scorecard (`bench/metrics.rs`, against the RTS-smoothed track of fixes at 15 m or better): fixes, error RMS / p95, false jumps,
  standing jitter RMS and path per minute, held share, arrival lag p50 / p90, missed arrivals, turn lag p50, verdicts, odometer.
  The replay adds pickups gained / lost / both (with `--game`), the reference path, cost per fix and matching share (with
  `--atlas`). Bridging error is scored only by the CI gap scenarios.
- Pull a walk: `scripts/pull_diag.sh <out>` (journal, games, atlases, diag, raw track), then `python3 scripts/diag_report.py <out>`
  (`== raw track ==` and `== gnss ==` sections).
- **Real walks are never committed** (they show the owner's home): keep pulls in the main checkout's `diag/` (gitignored) or the
  session scratchpad, and no real coordinates in tests or fixtures.

## Phone side (Android)

| Phone | Fixes from | Cold start |
| --- | --- | --- |
| Play services available (`GoogleApiAvailability` `SUCCESS` and the package enabled), any Android 8+ | `FusedLocationProviderClient`, `PRIORITY_HIGH_ACCURACY` in a zone (waits for an accurate `GRANULARITY_FINE` fix), BALANCED idle; fixes tagged `fused` | `getCurrentLocation` once per session (max age 2 min) |
| No Play services, Android 12+ | LocationManager `gps`, `LocationRequest` with quality, interval and batching | `network` once (`getCurrentLocation`) |
| No Play services, Android 8 to 11 | LocationManager `gps`, `requestLocationUpdates(gps, interval, 0f)` | `network` once: `getCurrentLocation` on 11, the last-known fix (at most 2 min old) on 8 to 10 |

The core inflates a network fix's `R` 4x and never accepts its estimates (a Wi-Fi position can be spoofed without the mock flag,
adversarial review I2). They are display only until the session's first GNSS fix; from then on network fixes are dropped
(`Unusable`), not shown. So indoors in a zone, once GPS was seen, a fused fix tagged `network` leaves the pin stale instead of following
Wi-Fi (re-review N6). A fused fix is sent as `network` when a
`GnssStatus` reported within the last 30 s with no satellite used in a fix (`GnssEvidence`); with no `GnssStatus` data (it runs only
in a zone) fused is trusted. If a Play services request fails, the session falls back to the LocationManager plan until the next game open.

- Rate: in a zone, 1 s with the screen on, 5 s (batched up to 10 s) with it off, high accuracy in both (`GpsPolicy.inZone`), with no
  distance filter: this replaces main's 5 s / 10 m in-zone rate (`PresencePolicy.ZONE_MOVE_M`), because the filter needs a steady
  stream (stationary hold, gaps, resets) and a standing player would otherwise look like a GPS gap. Idle 15 s / 20 m; outside zones
  main's 90 s / 50 m. No location while a game waits to learn whether you are home (`holding`). A screen on/off broadcast re-applies
  the decision.
- Compass: 5 Hz on screen, 1 Hz off. Play services' Fused Orientation Provider only while the map is on screen (it delivers only in
  the foreground); rotation vector otherwise. Readings go to `on_heading` with `error_deg` from the fused provider.
- Steps: `TYPE_STEP_COUNTER` only, 2 s batches in a zone (10 s otherwise), event time sent so cadence is known.
- Step calibration: loaded from preferences `stepcal` on game open and on a solo or Archipelago start, saved on close, background and
  every 5th heartbeat (heartbeats come with fixes, at most one a minute: no timer wakes the phone); a calibration with no samples is never saved over a stored one.
- Network fixes (LocationManager `network`, or fused without satellites) count double sigma and never count for quests; after the
  session's first GNSS fix they are dropped, not shown. While the newest fix is one, the Play panel shows "Wi-Fi location only: quests do not count" with `Help.networkOnly`.
- Raw track (debug builds only): `files/diag/raw/raw-NNNN.jsonl` (10 x 2 MB, internal storage like the diag log, pulled with
  `run-as` by `scripts/pull_diag.sh`; adversarial review M4), lines `rawfix` (`tf`, `ert`, `lat`, `lon`, `acc`, `spd`,
  `spd_acc`, `brg`, `brg_acc`, `alt`, `valt`, `prov`, `mock`), `rawsteps` (`total`, `te`), `rawhead` (`te`, `az`, `acc`, `pitch`,
  `roll`, optional `err`), `rawstate` (`presence`, `counting`, `zone`, `app_visible`). Release builds never store raw positions.
- Mock fixes are `Unusable`. A debuggable build lets them through after `adb shell run-as dev.apgo2.app touch files/allow_mock`
  (read once at start: force-stop the app after adding or removing the file).
- GNSS lines (all builds, no positions): `gnss status` every 10 s (in view / used per constellation, C/N0 of the top 4, bands,
  `dual_freq`) and `gnss hardware` once. Debug builds also log one `heading compare` line (fused vs rotation vector north).
- Heartbeat (debug builds): `perf_n`, `perf_p50_us`, `perf_p99_us` of `engine.onFix` per minute.
- Battery: the "Keep tracking alive" setup step, shown while the app is not exempt from battery optimisation, with OEM text and a
  button to the battery optimisation list (no `REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` permission).

## Map pin

`Engine::position(now_ms)` after every fix, step batch and heading change; `MePins.from` turns it into a pin and `MapHolder.showMe`
pushes it to the LocationComponent (`forceLocationUpdate`, look-ahead). It glides for the time since the last refresh (200 ms to
1 s) and snaps on a jump over 50 m or once per fix that restarted the filter (`snap` is a level until the next fix). Circle =
`uncertainty_m`, hidden under 3 m. Arrow: course when moving; compass when standing (or moving without a course) with the phone
held flat and steady and the compass at medium or better; otherwise none. Image: `MarkerSpec.Me(heading, state)`, person or arrow,
hollow while bridged, grey when stale (over 30 s). Android never derives a zone from a position with `bridged_origin`.
Game logic on Android (the position Archipelago traps are placed around) takes `Engine::last_accepted_pos` (`AppModel.acceptedHere`),
never the pin; only UI defaults (the realm editor's and home picker's centre) use the pin (`AppModel.here`, adversarial review M3).
That position outlives a counting pause (home, car: `Game::last_accepted_pos`, unlike the checks' `last_pos`), so a trap arriving in
the car lands around where the player last was; the game's home is the fallback only before the first accepted estimate (re-review N5).

## iOS integration plan (no Swift yet)

Platform services first, ours on top, our own code only where Apple has no equivalent. The same UniFFI `Engine` (Swift bindings)
and the same `Locator` and parameters.

| Concern | iOS | Maps to |
| --- | --- | --- |
| Fixes in a zone | `CLLocationUpdate.liveUpdates(.fitness)` for Walk/Run, `.otherNavigation` for Bike; **never** `.automotiveNavigation` (Core Location road-snaps it before our filter) | `on_fix(FixIn, ..)`, provider `ios` |
| Background | `CLBackgroundActivitySession` (iOS 17+), `UIBackgroundModes: location` | foreground service |
| Precise location | `requestTemporaryFullAccuracyAuthorization(withPurposeKey:)` when `accuracyAuthorization == .reducedAccuracy` | fine location permission |
| Outside zones, home, car | Coarser updates or none; presence rules stay in the host | `PresencePolicy` |
| Fix fields | `coordinate`, `timestamp`, `horizontalAccuracy`, `speed`, `speedAccuracy`, `course`, `courseAccuracy`, `altitude`, `verticalAccuracy`; **negative = unknown: send `None`**; `sourceInformation.isSimulatedBySoftware` as `mock` | `FixIn` |
| Compass | `CLHeading.trueHeading` as `azimuth_deg`, `headingAccuracy` as `error_deg` (negative = invalid: `None`), `headingFilter = 2` | `on_heading(HeadingIn)` |
| Steps | `CMPedometer.startUpdates(from:)`: `numberOfSteps` (cumulative, the core uses differences), `currentCadence` | `on_steps(total, t_ms, cadence)`; source `phone.pedometer` |
| Step calibration | `UserDefaults` `stepcal`, one entry per source | `set_step_calibration` / `step_calibration` |
| Motion | `CMMotionActivityManager`: optional soft hint for the IMM mode probabilities, never a hard switch (no core API yet) | |
| Display | MapLibre iOS user-location annotation driven by `Engine.position()`, same glide, snap, circle and image rules | `LocationComponent` |

Core work iOS needs first: `Calibrator::load` accepts only `phone.step_counter` today (`calib.rs`), so the step source must become a
`Locator` setting; check whether `headingAccuracy` (a maximum deviation) should be halved like the fused provider's 95 % cone.

## Gotchas found while building

- `Pi(dt)` rows: per-second rates times `min(dt, 10)` make some rows negative, so a row's off-diagonal is capped at 0.9
  (`max_offdiag_share`).
- A 3-fix burst that agrees with itself over 2 s is a relocation by the spec's rule (owner ruling G1): the CI burst scatters its
  fixes (bearings 0/120/240) to stay gated.
- Synthetic GNSS error is drift-dominated (AR(1)): the scenarios score position against truth plus drift, which the filter cannot
  remove. CI therefore misses drift regressions; real walks catch them.
- Corners: the NIS maneuver detector cannot see walking turns, so a line fit of the last fixes gives the course, and only when it
  is statistically significant and the moving models dominate (otherwise standing produces noise courses).
- Without a step counter the IMM walking speed reads 20 to 30 % low and "no new steps for 10 s" would hold a walker: the hold
  waits until the filter has run 10 s, and a quiet step counter raises the far-fix hold exit to 15 m only in Walk/Run zones.
- The odometer adds the chord after a gap reset or a hold-ending relocation when its speed is plausible (judged from the agreeing
  fixes, not the frozen point's age); otherwise sparse walks lost most of their distance.
- HMM confidence must sum co-located (`clamp(1.5 sigma_z, 1.5, 6.5) m`) same-kind states: at a node the mass splits across edges
  and the pin flickered off the street; summing street with off-network boosted both.
- The matched trace breaks after a 2 min gap (from the last accepted estimate, so standing keeps one run), at Reset/Relocated and
  on a simulator teleport over 50 m. Decided points go through a streaming simplifier (final review F3): a window of at most 64
  points waits while the line from the last kept point stays within 2 m of them; a corner or a full window is kept (Douglas-Peucker
  on the window only), and the window goes to the map as part of the tail. Kept points are never rewritten, so a delta only appends
  (a break keeps the window as it is). The 20 000-point cap is a memory backstop: it thins the kept points to every other one, O(n),
  and forces one reload. The old whole-trace DP took about 15 ms per cap on the host and overflowed the stack on a zigzag.
- A quiet step counter is no proof of standing: without the 15 m bound (FR-C1) a wheelchair or stroller user was frozen for good.
- The dev simulator's clock runs ahead: the first real fix after it also clears the step history and the compass buffer, or real
  readings looked stale until real time caught up (final review M1).
- A realm outline edit refreshes the zone shapes (in-zone, proximity, filter mode) together with the streets (`refresh_streets`).
- Street geometry: junction nodes are pinned before Douglas-Peucker (else a junction is simplified away and the graph splits), and
  each way is simplified once per scan; a way seen in two atlases keeps both atlases' junctions.
- `StreetGraph` is immutable and shared as `Arc<StreetGraph>` across Engine threads (it must stay `Send + Sync`); the route cache
  lives in the matcher. The graph is built outside the game lock and swapped in with a generation check (owner: game open never
  waits for it; `Diag` "street graph built" with ms).
- The simulator's virtual clock runs ahead of the wall clock: the first real fix after simulated ones resets the filter, and the
  journal and reject-log throttles restart when time goes backwards.
- `LocationComponent` images come from the style (`foregroundName` / `gpsName`), so every `MarkerSpec.Me` variant
  (`MePins.specs()`) is added before activation. `RenderMode.GPS` hides the accuracy circle, so the pin uses `NORMAL` with the
  arrow as its own image; `enableStaleState(false)` because staleness comes from `position()`.
- Step calibration: window distance from the raw path grew 18 % from jitter, so the path is decimated (15 m); the spec's adapt
  rules never fired, so change detection is a CUSUM. The CI stop scenario passes on 18 of 20 seeds within 0.05 and all within 0.15.
- Bridge: a stop pauses the cloud (no restart); while paused, compass jumps are explained, and on resume after a rotation over
  `bridge_split_deg` the street directions are re-split.
- Fused Orientation Provider is foreground only, so it runs only with the map on screen; pocket learning off screen stays on the
  rotation vector. Its heading is true north only when Play services knows the declination (no per-sample flag).
- Play services `LocationRequest` defaults to `waitForAccurateLocation = true`: the idle request sets it false explicitly.

## Not done / follow-ups

- No outdoor walk with this build; map matching is unscored on real walks until a pull includes atlases; iOS not started. All
  three: #117 (device walk, with the list of parked tuning items from Task 23).
- Not checked on a device: glide, snap, arrow and circle together, pin images, landscape compass, battery step, GMS path and its
  fallbacks, `heading compare` line, street graph build time on the phone.
- #91 raw-track step lines (done here, closes with the PR); #114 fused orientation (done here).
- #99 show the hollow-pin help (`Help.hollowDot` exists, nothing opens it); #107 per-fix quest checks re-derive unchanged state;
  #112 Display path setting (Long / Medium / Short); #103 stale zone shapes after a realm edit (closed).
- FR-C1 at 0.5 m/s: the hold ends after up to about 47 s (15 m takes 30 s at that speed); the CI bound there is 60 s.
- Spec items not built: bridging metrics in the replay scorecard; Activity Recognition prior.
