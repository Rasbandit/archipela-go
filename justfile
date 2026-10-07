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
    uv run --project apworld pytest apworld

spell:
    typos

secrets:
    gitleaks detect --no-banner

check: lint typecheck test spell

build:
    bash scripts/build_apworld.sh

# --- Android dev loop (phone paired over adb) ---
export JAVA_HOME := "/usr/lib/jvm/java-25-openjdk"
export ANDROID_HOME := env("HOME") + "/Android/Sdk"
export PATH := env("HOME") + "/.cargo/bin:" + env("PATH")
apk := "android/app/build/outputs/apk/debug/app-debug.apk"
app := "dev.apgo2.app"

# Rust core for Android + Kotlin bindings (profile: debug|release)
android-core profile="debug":
    bash scripts/android_core.sh {{profile}}

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
