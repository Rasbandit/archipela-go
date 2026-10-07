# Context Doc: Android Dev Workflow (Rust core + Kotlin/Compose on a real phone)

_Last verified: 2026-10-07_

## Status
Working end to end on a Pixel 8 Pro (Android 17, arm64) over wireless debugging. Spike A proven: the Rust core (`core/ffi`, UniFFI) loads
and runs in a Compose app. On device: 100 trips offline cells 1.07 s; streets (live Overpass) 24.7 s cold; debug build.

## Daily loop (about 8 s from edit to running app)
`just android-run` = `android-core` (cargo-ndk arm64 + UniFFI Kotlin bindings) -> `android-build` (Gradle) -> `android-install` (adb, also grants
location) -> `android-start`. Also: `just android-logs`, `just android-shot`, `scripts/android_tap.sh "<label>"` (tap a button by its text via
uiautomator, no coordinates needed). Rust-only logic is faster to iterate on desktop: `cd core && cargo test` and `cargo run --example gen_zone`.
`APGO_ABIS="arm64-v8a x86_64"` also builds for an emulator.

## Setup (once)
- JDK: `sudo dnf install java-25-openjdk-devel` (Gradle needs javac; the default headless JRE has none). `JAVA_HOME=/usr/lib/jvm/java-25-openjdk` is set in the justfile.
- Rust: official `rustup` (Fedora's rustc cannot add Android targets); `rustup target add aarch64-linux-android x86_64-linux-android`; `cargo install cargo-ndk`.
- SDK (user space, no sudo): command-line tools zip into `~/Android/Sdk/cmdline-tools/latest`; `yes | sdkmanager --licenses` (owner accepts);
  install `platforms;android-37.0`, `build-tools;36.0.0`, `ndk;29.0.14206865`. `android/local.properties` has `sdk.dir` (git-ignored).
- Phone: Developer options -> Wireless debugging -> `adb pair ip:port` (code) then it appears in `adb devices`. A charge-only USB cable shows nothing in `lsusb`.

## Versions that worked (Oct 2026)
AGP 9.4.1, Gradle 9.8.0, Kotlin 2.4.20, Compose BOM 2026.09.00, UniFFI 0.32.2, cargo-ndk 4.1.2, JNA 5.19.1 (aar), minSdk 26, compileSdk 37, targetSdk 36.

## Gotchas hit
- Compose BOM 2026.09 requires `compileSdk = 37` (AAR metadata check fails otherwise).
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
