# Context Doc: Android Dev Workflow (Rust core + Kotlin/Compose on a real phone)

_Last verified: 2026-10-08_

## Status

Working end to end on a Pixel 8 Pro (Android 17, arm64) over wireless debugging. Spike A proven: the Rust core (`core/ffi`, UniFFI) loads
and runs in a Compose app. On device: 100 trips offline cells 1.07 s; streets (live Overpass) 24.7 s cold; debug build.

## Quality gates

`just check-android` = `scripts/android_bindings.sh` (host-built UniFFI bindings, no NDK) then Gradle `spotlessCheck detekt lintDebug testDebugUnitTest koverVerifyDebug`.
Generated `uniffi/` is excluded. Needs a JDK: `scripts/java_home.sh` finds it (`$JAVA_HOME`, macOS `java_home`, or the JDK owning `javac`).

## Daily loop (about 8 s from edit to running app)

`just android-run` = `android-core` (cargo-ndk arm64 + UniFFI Kotlin bindings) -> `android-build` (Gradle) -> `android-install` (adb, also grants
location) -> `android-start`. Also: `just android-logs`, `just android-shot`, `scripts/android_tap.sh "<label>"` (tap a button by its text via
uiautomator, no coordinates needed). Rust-only logic is faster to iterate on desktop: `cd core && cargo test` and `cargo run --example gen_zone`.
`APGO_ABIS="arm64-v8a x86_64"` also builds for an emulator.

## Archipelago connection (Spike C, proven 2026-10-07)

Phone joined a local server running OUR apworld, received `slot_data` (contract v1), sent location checks, got items back (server log:
`Tester sent Take a Breather! to Tester (Trip #1)`). Loop: `just ap-host` (generates a 100-trip seed for slot `Tester`, hosts on :38281,
sets `adb reverse` so the phone uses `localhost:38281`), then `just android-run`, tap Connect / Check next trip. `just ap-log`, `just ap-stop`.
Archipelago uses ONE persistent WebSocket (server pushes items, prints, bounces); the Rust crate `archipelago_rs` is non-blocking: Kotlin calls `poll()`
every 250 ms to drain already-read events. No push channel exists when the app is dead; plan a foreground service + reconnect/`Sync` item resync.

Gotchas for the connection:

- `archipelago_rs 3.0.1` does not compile for Android (no cache-dir fallback). Vendored patched copy in `core/vendor/archipelago_rs` via `[patch.crates-io]`
  (see its `PATCHES.md`; candidate upstream PR). Pass `Cache::path(app cacheDir)` so it never needs a platform dir.
- `Connection::new(url, name, game, options)`: the 3rd arg is the GAME name (must equal `Archipela-Go 2: Electric Boogaloo`), not the password (use `options.password`).
- Two rustls backends get linked (ring + aws-lc-rs): install `rustls::crypto::ring::default_provider()` once or the first poll panics ("Could not automatically determine the process-level CryptoProvider").
- Disable crate default features (`native-tls`) to avoid OpenSSL on Android; `rustls` only. Plain `ws://` works for local dev (server has no TLS).
- Don't use `pkill -f` in scripts (matches your own shell); `ap_host.sh stop` kills the port listener instead.
- Server prints "client does not support compressed websocket connections": harmless warning for now.

## Map + game loop (Spike D, proven 2026-10-07)

MapLibre Native 13.6.1 + OpenFreeMap `liberty` style in Compose (`TripMap.kt`, GeoJSON sources, circle layers: red open, grey locked, green done, blue me).
Flow: Connect -> `slot_data.trips` (location_id, distance_tier, key_needed) -> `generate_trips_for` (FFI) places a point per trip in its tier band around you
-> live location (`requestLocationUpdates`, 1 s) -> geofence 40 m sends `LocationChecks` for OPEN trips -> items arrive -> `Progressive Key` count unlocks trips with
`key_needed <= keys`. Measured on the Pixel: 100 trips placed in 1.08 s (cells), 77 locked at start; after 20 checks a key arrived and locked fell 77 -> 52.
Dev testing without walking: "DEV: teleport to next" injects a simulated position (real code path from position to check); "Use real GPS" restores. Header shows SIMULATED/real.
Play zone (Spike E): "Draw zone" then tap the map to add polygon points (Undo / Clear zone); Fill uses the polygon if 3+ points, else a circle around you.
FFI `generate_trips_for(ZoneIn::Polygon{vertices}|Circle{center, step_m}, specs, seed, mode, cache)`; polygon tiers are tenths of the polygon's extent from its centroid
(`slot_data.tier_step_m` only applies to circle mode). Streets mode sends the polygon to Overpass as a `poly:` filter. Verified on the Pixel by adb-tapping four corners:
100 trips placed in 480 ms, 96 in band, all inside. Observed: trips crowd toward the centroid (one per tier per band, bands grow with radius): add blue-noise/area-weighted spread.
Gotcha: pass `zonePts.toList()` (a copy) to the map composable; the same mutable list object never retriggers `LaunchedEffect`.
Known gaps: geofence/lock logic is in Kotlin (move to the Rust core with tests before iOS); trips are not persisted (regenerate each launch, seed 1 is deterministic for cells);
checked state is local only (should come from `Client::checked_locations` on reconnect); no foreground service or background location; no reroll/ban; foreground-only.

## Update 2026-10-08: emulator workflow and the v1 app

The app was rewritten (Realms / New Game / Play). See `docs/context/v1-architecture-and-status.md` for architecture, verified results and gaps.
Repeatable testing without a phone: `scripts/emu.sh create|start|stop`, `just emu-run`, `just e2e` (fresh data -> real scan -> solo game -> autoplay win),
`scripts/android_ui.py` (tap/type/wait by visible text, honors `ANDROID_SERIAL`), `scripts/e2e_autoplay.sh`. The phone locks overnight: UI tests need the emulator.

## Setup (once)

- JDK: `sudo dnf install java-25-openjdk-devel` (Gradle needs javac; the default headless JRE has none). `scripts/java_home.sh` picks `$JAVA_HOME` (mise/CI) or the JDK owning `javac` on PATH; the justfile, lefthook and `emu.sh` use it.
- Rust: official `rustup` (Fedora's rustc cannot add Android targets); `rustup target add aarch64-linux-android x86_64-linux-android`; `cargo install cargo-ndk`.
- SDK (user space, no sudo): command-line tools zip into `~/Android/Sdk/cmdline-tools/latest`; `yes | sdkmanager --licenses` (owner accepts);
  install `platforms;android-37.0`, `build-tools;36.0.0`, `ndk;29.0.14206865`. `android/local.properties` has `sdk.dir` (git-ignored).
- Phone: Developer options -> Wireless debugging -> `adb pair ip:port` (code) then it appears in `adb devices`. A charge-only USB cable shows nothing in `lsusb`.

## Versions that worked (Oct 2026)

AGP 9.4.1, Gradle 9.8.1, Kotlin 2.4.20, Compose BOM 2026.09.00, UniFFI 0.32.2, cargo-ndk 4.1.2, JNA 5.19.1 (aar), minSdk 26, compileSdk 37, targetSdk 37.

## Gotchas hit

- Compose BOM 2026.09 requires `compileSdk = 37` (AAR metadata check fails otherwise).
- targetSdk 37 (Android 17) blocks LAN traffic without `ACCESS_LOCAL_NETWORK` (a TCP connect just times out, Rust sockets included).
  `LocalNetwork.kt` asks for it on Connect only when the server is a LAN address/name; `localhost` via `adb reverse` is loopback and not gated.
- UniFFI error variant fields must not be named `message` (collides with Kotlin `Throwable.message`); use `detail`.
- AGP 9 built-in Kotlin ignores `build/generated` via `java.srcDir`; generate bindings into `android/app/src/main/kotlin/uniffi` (git-ignored).
- `Display` + `std::error::Error` must be implemented on the Rust error enum for `#[derive(uniffi::Error)]` returned in `Result`.
- Debug `.so` was 57 MB with symbols; `[profile.dev] debug = 0` + arm64-only brings it to ~8 MB.
- App draws edge-to-edge on Android 15+: add `statusBarsPadding()`.
- sdkmanager prints "SDK XML version 4" warning with these tools: harmless.

## Privacy note

Streets mode sends the player's rounded zone (center, radius) to a public Overpass server. The atlas plan (`poi-atlas-and-server-options.md`) removes this.

## References

`docs/context/spike-b-location-generation-results.md`, `scripts/android_core.sh`, `justfile`
