set shell := ["bash", "-euo", "pipefail", "-c"]

default: check

setup: setup-ap
    lefthook install

setup-ap:
    bash scripts/setup_ap.sh

lint:
    uv run --project apworld ruff check apworld
    uv run --project apworld ruff format --check apworld
    uv run --project apworld ruff check --config scripts/ruff.toml scripts/*.py
    uv run --project apworld ruff format --config scripts/ruff.toml --check scripts/*.py

fmt:
    uv run --project apworld ruff format apworld
    uv run --project apworld ruff check --fix apworld
    uv run --project apworld ruff format --config scripts/ruff.toml scripts/*.py
    uv run --project apworld ruff check --config scripts/ruff.toml --fix scripts/*.py

typecheck:
    uv run --project apworld pyright --project apworld

test: ap-present
    uv run --project apworld pytest apworld --cov --cov-config=apworld/pyproject.toml --cov-report=term-missing:skip-covered -q

spell:
    typos

secrets:
    gitleaks detect --no-banner

# Also fails while .ap/worlds/ap_go2 points anywhere but apworld/ap_go2 (a killed `just mutate-py` leaves it on the mutants).
[private]
ap-present:
    @test -d .ap || { echo "Archipelago checkout missing: run 'just setup-ap'"; exit 1; }
    @[ "$(readlink -f .ap/worlds/ap_go2)" = "$(readlink -f apworld/ap_go2)" ] || { echo ".ap/worlds/ap_go2 does not point at apworld/ap_go2 (an interrupted 'just mutate-py'?): run 'just setup-ap'"; exit 1; }

# Fails while a mutation from an interrupted `just mutate-rust` is left in the core (cargo-mutants marks the line).
[private]
core-unmutated:
    @! grep -rn "changed by cargo-mutants" core/src core/ffi/src || { echo "a mutation from an interrupted 'just mutate-rust' is left in: git restore core/src"; exit 1; }

# apworld: lint, types, tests
check-py: ap-present lint typecheck test

# Repo-wide hygiene: spelling, secrets, workflows, shell, docs, dispatcher tests
check-hygiene: spell secrets
    actionlint
    shellcheck scripts/*.sh scripts/tests/*.sh
    markdownlint-cli2 "**/*.md" "#**/node_modules" "#.ap" "#.mutate-py" "#core/vendor" "#core/target"
    bash scripts/tests/prepush_test.sh
    bash scripts/tests/git_env_test.sh
    bash scripts/tests/java_home_test.sh
    bash scripts/tests/pull_diag_test.sh

check: check-hygiene check-py check-rust check-android

build:
    bash scripts/build_apworld.sh

# Rust core: format, lint, docs, supply chain, tests with line-coverage floor
check-rust: core-unmutated
    cd core && cargo fmt --all --check
    cd core && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings
    cd core && RUSTDOCFLAGS="-D warnings" cargo doc -p apgo-core -p apgo-ffi --no-deps -q
    cd core && cargo deny check
    cd core && cargo llvm-cov -p apgo-core -p apgo-ffi --fail-under-lines 91

# --- Mutation testing (slow, not in `check`): a surviving mutant is logic no test pins down ---
# Python: `just mutate-py` (all), `just mutate-py run "worlds.ap_go2.zones*"`, `just mutate-py results`
[positional-arguments]
mutate-py *args: ap-present
    bash scripts/mutate_py.sh "$@"

# Rust core: `just mutate-rust -f src/goal.rs` (one file), `just mutate-rust` (all ~2000 mutants: hours). In place, because yaml.rs
# and slot.rs include files outside core/ so the default temp copy cannot build: do not edit core/ while it runs. An interrupted
# run can leave one mutant behind (marked `~ changed by cargo-mutants ~`; the check and Android recipes fail on it).
[positional-arguments]
mutate-rust *args: core-unmutated
    @[ -z "$(git status --porcelain -- core)" ] || { echo "core/ has uncommitted or untracked changes: commit them first, the run mutates core/ in place"; exit 1; }
    cd core && cargo mutants -p apgo-core --in-place "$@"

# Rust core, only the lines this branch changed against origin/main: the quick one to run before a PR.
mutate-rust-diff:
    git fetch -q origin main
    git -C core diff --relative origin/main... > core/mutants.diff
    just mutate-rust --in-diff mutants.diff

# --- Android dev loop (phone paired over adb) ---
export JAVA_HOME := shell('bash "$1"', justfile_directory() / "scripts/java_home.sh")
export ANDROID_HOME := env("ANDROID_HOME", env("HOME") + "/Android/Sdk")
export PATH := env("HOME") + "/.cargo/bin:" + env("PATH")
apk := "android/app/build/outputs/apk/debug/app-debug.apk"
app := "dev.apgo2.app"

# Rust core for Android + Kotlin bindings (profile: debug|release)
android-core profile="debug": core-unmutated
    bash scripts/android_core.sh {{profile}}

# Android: bindings (host build), format, static analysis, lint, unit tests
check-android: core-unmutated
    bash scripts/check_color_tokens.sh
    bash scripts/android_bindings.sh
    cd android && ./gradlew :app:spotlessCheck :app:detekt :app:lintDebug :app:testDebugUnitTest :app:koverVerifyDebug --console=plain -q
    bash scripts/tests/android_release_core_test.sh

android-build:
    cd android && ./gradlew assembleDebug --console=plain -q

android-install:
    adb install -r {{apk}}
    adb shell pm grant {{app}} android.permission.ACCESS_FINE_LOCATION || true

android-start:
    adb shell am force-stop {{app}}
    adb shell am start -n {{app}}/dev.apgo2.MainActivity

# Release APK: Gradle's release variant builds the core in the release profile first (`android_core.sh release`, re-review N4)
android-release:
    cd android && ./gradlew assembleRelease --console=plain -q

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
emu-run: core-unmutated
    APGO_ABIS="arm64-v8a x86_64" bash scripts/android_core.sh debug
    cd android && ./gradlew assembleDebug --console=plain -q
    ANDROID_SERIAL=emulator-5554 adb install -r {{apk}}
    ANDROID_SERIAL=emulator-5554 adb shell pm grant {{app}} android.permission.ACCESS_FINE_LOCATION

# Full regression on the emulator: realm -> real scan -> solo game -> autoplay to the win.
e2e:
    bash scripts/e2e_emulator.sh
