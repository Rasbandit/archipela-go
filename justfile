set shell := ["bash", "-euo", "pipefail", "-c"]

default: check

setup: setup-ap
    lefthook install

setup-ap:
    bash scripts/setup_ap.sh

lint:
    uv run --project apworld ruff check apworld
    uv run --project apworld ruff format --check apworld

fmt:
    uv run --project apworld ruff format apworld
    uv run --project apworld ruff check --fix apworld

typecheck:
    uv run --project apworld pyright --project apworld

test:
    uv run --project apworld pytest apworld --cov --cov-config=apworld/pyproject.toml --cov-report=term-missing:skip-covered -q

spell:
    typos

secrets:
    gitleaks detect --no-banner

check: lint typecheck test spell check-rust

build:
    bash scripts/build_apworld.sh

# Rust core: format, lint, docs, supply chain, tests with line-coverage floor
check-rust:
    cd core && cargo fmt --all --check
    cd core && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings
    cd core && RUSTDOCFLAGS="-D warnings" cargo doc -p apgo-core -p apgo-ffi --no-deps -q
    cd core && cargo deny check
    cd core && cargo llvm-cov -p apgo-core -p apgo-ffi -q --fail-under-lines 80

# --- Android dev loop (phone paired over adb) ---
export JAVA_HOME := "/usr/lib/jvm/java-25-openjdk"
export ANDROID_HOME := env("HOME") + "/Android/Sdk"
export PATH := env("HOME") + "/.cargo/bin:" + env("PATH")
apk := "android/app/build/outputs/apk/debug/app-debug.apk"
app := "dev.apgo2.app"

# Rust core for Android + Kotlin bindings (profile: debug|release)
android-core profile="debug":
    bash scripts/android_core.sh {{profile}}

# Android: bindings (host build), format, static analysis, lint, unit tests
check-android:
    bash scripts/android_bindings.sh
    cd android && ./gradlew :app:spotlessCheck :app:detekt :app:lintDebug :app:testDebugUnitTest :app:koverVerifyDebug --console=plain -q

android-build:
    cd android && ./gradlew assembleDebug --console=plain -q

android-install:
    adb install -r {{apk}}
    adb shell pm grant {{app}} android.permission.ACCESS_FINE_LOCATION || true

android-start:
    adb shell am force-stop {{app}}
    adb shell am start -n {{app}}/dev.apgo2.MainActivity

# One command: rebuild Rust + app, install on the phone, launch.
android-run: android-core android-build android-install android-start

android-logs:
    adb logcat --pid=$(adb shell pidof {{app}}) -v time

android-shot name="phone":
    adb exec-out screencap -p > /tmp/apgo-{{name}}.png && echo /tmp/apgo-{{name}}.png

android-tap x y:
    adb shell input tap {{x}} {{y}}

# --- Dev Archipelago server with OUR apworld (slot "Tester") ---
ap-host trips="100":
    bash scripts/ap_host.sh start {{trips}}

ap-stop:
    bash scripts/ap_host.sh stop

ap-log:
    bash scripts/ap_host.sh log 40

# --- Emulator (no phone needed): GPS via `adb emu geo fix`, headless ---
emu-start:
    bash scripts/emu.sh start

emu-stop:
    bash scripts/emu.sh stop

# Rebuild for phone + emulator ABIs and install on the emulator.
emu-run:
    APGO_ABIS="arm64-v8a x86_64" bash scripts/android_core.sh debug
    cd android && ./gradlew assembleDebug --console=plain -q
    ANDROID_SERIAL=emulator-5554 adb install -r {{apk}}
    ANDROID_SERIAL=emulator-5554 adb shell pm grant {{app}} android.permission.ACCESS_FINE_LOCATION

# Full regression on the emulator: realm -> real scan -> solo game -> autoplay to the win.
e2e:
    bash scripts/e2e_emulator.sh
