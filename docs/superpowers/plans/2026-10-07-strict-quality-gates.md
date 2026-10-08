# Strict Quality Gates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce the strictest sane lint/type/format/test/coverage gates for Rust, Python and Kotlin, locally (lefthook) and in CI, with zero grandfathered violations.

**Architecture:** Each language gets one `just check-<lang>` recipe that is the single source of truth; lefthook pre-push calls a tested dispatcher script that picks recipes from the changed files; CI runs every recipe in parallel jobs. Lint config lives next to the code (`core/Cargo.toml`, `apworld/pyproject.toml`, `android/config/detekt.yml`).

**Tech Stack:** clippy, rustfmt, cargo-deny, cargo-llvm-cov; ruff, pyright, pytest-cov; Spotless+ktlint (+compose rules), detekt, Android Lint, Kover; lefthook, just, mise, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-10-07-strict-quality-gates-design.md`

## Global Constraints

- Branch: `chore/strict-quality-gates`. Conventional commits, subject < 50 chars, end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Never baseline: fix violations. Any `allow`/`ignore`/`@Suppress` is local (smallest scope) and has a reason comment, except the repo-level allow-lists named in this plan.
- Clippy `pedantic` = deny; nursery/restriction only the lints listed in Task 1.
- Vendored `core/vendor/archipelago_rs` is never edited or linted.
- Generated `android/app/src/main/kotlin/uniffi/**` is never edited or linted.
- Tool versions pinned exactly (no `latest`); GitHub Actions pinned by full commit SHA with a `# vX.Y.Z` comment.
- Coverage floors = today's measured line coverage rounded down to an integer.
- Web lookups (latest versions, action SHAs): Perplexity/Firecrawl/`gh api`, never built-in WebSearch/WebFetch.
- Behaviour must not change: every existing test (`cargo test`, `pytest`, `testDebugUnitTest`) stays green after each task.

## Review Focus

1. **First push of a new branch** (no upstream): pre-push must run every language, not skip. Pinned by `scripts/tests/prepush_test.sh` case `no-range`.
2. **Push touching only `core/ffi/**`**: changes the Kotlin bindings, so Android checks must run too. Pinned by test case `ffi-only`.
3. **Generated `uniffi/` Kotlin**: ktlint, detekt and Android Lint must ignore it or every gate fails on code we don't own. Pinned by Task 4/5/6 verification steps that run on a tree with bindings generated.
4. **Fresh clone** (no `.ap/`, no bindings): `just check-android` generates bindings itself; `just check-py` fails with the existing `setup-ap` hint, not a cryptic import error. Pinned by Task 4 Step 1 and Task 8 Step 6.
5. **Docs-only / deletion-only push**: only hygiene runs; a push that deletes a `.py` file still runs `check-py`. Pinned by test cases `docs-only` and `deleted-py`.

---

## File Structure

| File | Responsibility |
|------|----------------|
| `core/Cargo.toml` | `[workspace.lints]` (rust + clippy), crate lint opt-in |
| `core/ffi/Cargo.toml` | `lints.workspace = true` |
| `core/clippy.toml` | test allowances for `unwrap`/`expect`/`print` |
| `core/rust-toolchain.toml` | pin `1.99.0` + components |
| `core/deny.toml` | cargo-deny policy |
| `apworld/pyproject.toml` | ruff ALL, pyright strict, coverage floor |
| `android/build.gradle.kts`, `android/app/build.gradle.kts` | Spotless, detekt, Lint, Kover, warnings-as-errors |
| `android/gradle/libs.versions.toml` | plugin versions |
| `android/config/detekt.yml` | detekt overrides with reasons |
| `android/.editorconfig` | ktlint settings for Compose |
| `scripts/android_bindings.sh` | host-only build + Kotlin bindings (no NDK) |
| `scripts/prepush.sh` | changed files → recipes dispatcher |
| `scripts/tests/prepush_test.sh` | tests for the dispatcher |
| `justfile` | `check-py`, `check-rust`, `check-android`, `check-hygiene`, `check` |
| `lefthook.yml` | pre-commit staged checks, pre-push dispatcher |
| `.github/workflows/ci.yml` | parallel jobs per language |
| `.github/dependabot.yml` | + cargo, gradle |
| `mise.toml` | exact pins + new tools |

---

### Task 1: Rust strict lints

**Files:**
- Modify: `core/Cargo.toml`, `core/ffi/Cargo.toml`, `core/src/lib.rs` (top), all `core/src/**`, `core/ffi/src/**`, `core/tests/**`, `core/examples/**` as lint fixes require
- Create: `core/clippy.toml`, `core/rust-toolchain.toml`

**Interfaces:**
- Produces: workspace lint policy consumed by Task 2 (`check-rust`) and Task 9 (CI).

- [ ] **Step 1: Pin the toolchain**

`core/rust-toolchain.toml`:
```toml
[toolchain]
channel = "1.99.0"
components = ["rustfmt", "clippy", "llvm-tools-preview"]
targets = ["aarch64-linux-android", "x86_64-linux-android"]
```

- [ ] **Step 2: Add the lint policy (the "failing test")**

Append to `core/Cargo.toml` (after `[workspace]` block):
```toml
[workspace.lints.rust]
unsafe_code = "deny"          # uniffi macros need a local allow in apgo-ffi; apgo-core forbids in lib.rs
missing_docs = "deny"
unused_qualifications = "deny"
rust_2018_idioms = { level = "deny", priority = -1 }

[workspace.lints.clippy]
pedantic = { level = "deny", priority = -1 }
# nursery, cherry-picked
use_self = "deny"
redundant_clone = "deny"
significant_drop_tightening = "deny"
derive_partial_eq_without_eq = "deny"
# restriction, cherry-picked
unwrap_used = "deny"
expect_used = "deny"
dbg_macro = "deny"
todo = "deny"
print_stdout = "deny"
# allow-list (reason each)
module_name_repetitions = "allow"  # `poi::PoiCache` style names read better than renamed ones

[lints]
workspace = true
```
Add to `core/ffi/Cargo.toml`:
```toml
[lints]
workspace = true
```
Add as the first line of `core/src/lib.rs`: `#![forbid(unsafe_code)]`

`core/clippy.toml`:
```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-print-in-tests = true
```

- [ ] **Step 3: Run clippy, confirm it fails**

Run: `cd core && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings 2>&1 | grep -c '^error'`
Expected: several hundred errors (≈700 seen in planning).

- [ ] **Step 4: Apply machine fixes**

Run: `cd core && cargo clippy --fix --allow-dirty -p apgo-core -p apgo-ffi --all-targets && cargo fmt --all && cargo test -p apgo-core -p apgo-ffi -q`
Expected: tests PASS. Commit:
```bash
git add core && git commit -m "refactor(core): apply clippy pedantic autofixes"
```

- [ ] **Step 5: Hand-fix remaining groups, one commit per group**

Get the list: `cargo clippy -p apgo-core -p apgo-ffi --all-targets --message-format=short -- -D warnings 2>&1 | grep '^[a-z].*error' | sed -E 's/.*error: //' | sort | uniq -c | sort -rn`

Rules per group:
- `must_use_candidate`: add `#[must_use]` to the function.
- `missing_errors_doc` / `missing_panics_doc`: add `/// # Errors` (or `# Panics`) section naming the real failure cases.
- `missing_docs`: one-line `///` doc on each pub item saying what it is for.
- Casts (`cast_possible_truncation`, `cast_precision_loss`, `cast_sign_loss`, `cast_possible_wrap`): use `u32::try_from(x)` / `i64::try_from` and propagate the error; where loss is intended (geo/distance math, percentages) add `#[allow(clippy::cast_precision_loss)] // f64 metres; values < 2^52` at the smallest scope.
- `unwrap_used`/`expect_used` in non-test code: return `Result` with the crate's existing error type; for mutex poisoning use the existing `PoisonError::into_inner` pattern.
- `unsafe_code` in `apgo-ffi`: add `#![allow(unsafe_code)] // uniffi::setup_scaffolding! expands to extern "C" fns` at the top of `core/ffi/src/lib.rs` only if clippy reports it.
- `print_stdout` in `core/examples/**`: add `#![allow(clippy::print_stdout)] // CLI example output` at the top of each example file.

After each group: `cargo test -p apgo-core -p apgo-ffi -q` PASS, then
```bash
git add core && git commit -m "refactor(core): fix clippy <group> lints"
```

- [ ] **Step 6: Verify green**

Run: `cd core && cargo fmt --all --check && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings && cargo test -p apgo-core -p apgo-ffi -q`
Expected: no errors, tests PASS.

- [ ] **Step 7: Verify the gate bites**

Add `pub fn probe() -> Option<u8> { Some(1).map(|x| x) }` to `core/src/lib.rs`; clippy must fail with `missing_docs` and `redundant_closure`/`map_identity`. Remove it; clippy green.

- [ ] **Step 8: Commit config**

```bash
git add core/Cargo.toml core/ffi/Cargo.toml core/clippy.toml core/rust-toolchain.toml core/src/lib.rs
git commit -m "build(core): deny clippy pedantic and pin toolchain"
```

---

### Task 2: Rust supply chain, ffi tests, docs

**Files:**
- Create: `core/deny.toml`
- Modify: `justfile` (`core-check` → `check-rust`), `mise.toml`

**Interfaces:**
- Produces: `just check-rust` (used by Tasks 8, 9).

- [ ] **Step 1: Install tools and pin**

Run: `cargo install --locked cargo-deny cargo-llvm-cov` then record versions (`cargo deny --version`, `cargo llvm-cov --version`).
Add to `mise.toml` `[tools]`: `"cargo:cargo-deny" = "<version>"`, `"cargo:cargo-llvm-cov" = "<version>"` using the printed versions.

- [ ] **Step 2: Write `core/deny.toml`**

```toml
[graph]
all-features = true

[advisories]
version = 2
yanked = "deny"

[licenses]
version = 2
allow = ["MIT", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause",
         "ISC", "Unicode-3.0", "Zlib", "MPL-2.0", "CDLA-Permissive-2.0"]
confidence-threshold = 0.9

[bans]
multiple-versions = "warn"
wildcards = "deny"
allow-wildcard-paths = true

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

- [ ] **Step 3: Run it**

Run: `cd core && cargo deny check`
Expected: PASS. If a licence fails, check the crate's licence; add it to `allow` only if it is permissive, with a comment naming the crate. If an advisory fails, upgrade the crate (`cargo update -p <crate>`); never add an `ignore` without an issue link.

- [ ] **Step 4: Replace `core-check` in `justfile`**

```just
# Rust core: format, lint, docs, supply chain, tests
check-rust:
    cd core && cargo fmt --all --check
    cd core && cargo clippy -p apgo-core -p apgo-ffi --all-targets -- -D warnings
    cd core && RUSTDOCFLAGS="-D warnings" cargo doc -p apgo-core -p apgo-ffi --no-deps -q
    cd core && cargo deny check
    cd core && cargo test -p apgo-core -p apgo-ffi -q
```
Update `check:` to reference `check-rust` instead of `core-check`; grep for `core-check` in `docs/`, `CLAUDE.md`, `CONTRIBUTING.md`, `.github/` and replace.

- [ ] **Step 5: Verify**

Run: `just check-rust` → PASS. Break a doc link (`/// see [`Nope`]`) → `cargo doc` fails; revert.

- [ ] **Step 6: Commit**

```bash
git add core/deny.toml justfile mise.toml docs CLAUDE.md CONTRIBUTING.md .github
git commit -m "build(core): add cargo-deny, doc and ffi test gates"
```

---

### Task 3: Python strict

**Files:**
- Modify: `apworld/pyproject.toml`, `apworld/ap_go2/**`, `apworld/tests/**` as fixes require

- [ ] **Step 1: Tighten config**

In `apworld/pyproject.toml` replace `[tool.ruff.lint]` and `[tool.pyright]`:
```toml
[tool.ruff.lint]
select = ["ALL"]
ignore = [
  "COM812", "ISC001",  # conflict with ruff format
  "D203", "D213",      # pick D211/D212 (google convention)
  "S311",              # random used for game generation, not crypto
  "FIX002", "TD003",   # TODOs reference issues by text
]

[tool.ruff.lint.pydocstyle]
convention = "google"

[tool.ruff.lint.per-file-ignores]
"tests/**" = ["S101", "D", "PLR2004", "ANN", "INP001", "SLF001"]
"ap_go2/options.py" = ["RUF012"]  # Archipelago option classes use plain class attributes

[tool.pyright]
include = ["ap_go2", "tests"]
extraPaths = ["../.ap"]
typeCheckingMode = "strict"
reportMissingTypeStubs = false  # Archipelago core ships no stubs
```

- [ ] **Step 2: Confirm it fails**

Run: `just lint typecheck`
Expected: FAIL with ruff `D`/other hits and pyright strict errors.

- [ ] **Step 3: Autofix, then hand-fix**

Run: `uv run --project apworld ruff check --fix apworld && uv run --project apworld ruff format apworld`
Then fix the rest by hand: add google-style docstrings to public modules/classes/functions; type all untyped Archipelago-facing params using Archipelago's real types (`BaseClasses.MultiWorld`, `Region`, `Location`, `Item`); where Archipelago returns `Any`, narrow with `isinstance` or `cast` with a comment. `# type: ignore[...]`/`# noqa: X` only with a reason comment.

- [ ] **Step 4: Verify**

Run: `just lint typecheck test`
Expected: PASS, same test count as before (`uv run --project apworld pytest apworld -q | tail -1`).

- [ ] **Step 5: Verify the gate bites**

Add `def probe(x): return x` to `apworld/ap_go2/constants.py`; `just lint typecheck` fails (`ANN001`, `D103`, pyright `reportUnknownParameterType`). Remove.

- [ ] **Step 6: Commit**

```bash
git add apworld && git commit -m "build(apworld): enable ruff ALL and pyright strict"
```

---

### Task 4: Android host bindings + Spotless/ktlint

**Files:**
- Create: `scripts/android_bindings.sh`, `android/.editorconfig`
- Modify: `android/gradle/libs.versions.toml`, `android/build.gradle.kts`, `android/app/build.gradle.kts`, `android/app/src/**/*.kt` (format)

**Interfaces:**
- Produces: `scripts/android_bindings.sh` (no args; writes `android/app/src/main/kotlin/uniffi/`), Gradle task `spotlessCheck`/`spotlessApply`.

- [ ] **Step 1: Host-only bindings script**

`scripts/android_bindings.sh`:
```bash
#!/usr/bin/env bash
# Generate the Kotlin UniFFI bindings from a host build (no NDK). Enough for
# lint, detekt and JVM unit tests; the phone build still uses android_core.sh.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root/core"
cargo build -q -p apgo-ffi
out="$root/android/app/src/main/kotlin"
rm -rf "$out/uniffi"
cargo run -q -p apgo-ffi --bin uniffi-bindgen -- generate \
  --library target/debug/libapgo_ffi.so --language kotlin --no-format --out-dir "$out"
```
`chmod +x scripts/android_bindings.sh`. Verify on a tree with bindings deleted: `rm -rf android/app/src/main/kotlin/uniffi && bash scripts/android_bindings.sh && ls android/app/src/main/kotlin/uniffi/apgo_ffi` → file listed.

- [ ] **Step 2: Look up versions**

Use Perplexity to find the latest stable versions of: Spotless Gradle plugin (`com.diffplug.spotless`), ktlint, `io.nlopez.compose.rules:ktlint`, detekt Gradle plugin (`dev.detekt` 2.x, which supports Kotlin 2.4), `io.nlopez.compose.rules:detekt`, Kover (`org.jetbrains.kotlinx.kover`). Confirm each supports Kotlin 2.4.20 and AGP 9.4. Add them to `libs.versions.toml` `[versions]`/`[plugins]`/`[libraries]` as `spotless`, `ktlint`, `composeRulesKtlint`, `detekt`, `composeRulesDetekt`, `kover`.

- [ ] **Step 3: Configure Spotless**

`android/build.gradle.kts` add `alias(libs.plugins.spotless) apply false`. `android/app/build.gradle.kts` add plugin `alias(libs.plugins.spotless)` and:
```kotlin
spotless {
    kotlin {
        target("src/**/*.kt")
        targetExclude("src/main/kotlin/uniffi/**") // generated by uniffi-bindgen
        ktlint(libs.versions.ktlint.get())
            .customRuleSets(listOf("io.nlopez.compose.rules:ktlint:${libs.versions.composeRulesKtlint.get()}"))
    }
    kotlinGradle {
        target("*.gradle.kts")
        ktlint(libs.versions.ktlint.get())
    }
}
```
`android/.editorconfig`:
```ini
[*.{kt,kts}]
ktlint_code_style = ktlint_official
ktlint_function_naming_ignore_when_annotated_with = Composable
max_line_length = 140
```

- [ ] **Step 4: Confirm it fails, then apply**

Run: `cd android && ./gradlew :app:spotlessCheck --console=plain` → FAIL (unformatted files).
Run: `./gradlew :app:spotlessApply` then hand-fix rules ktlint cannot autofix (compose rules: `ModifierMissing`, `ModifierReused`, etc. — fix the composable, don't suppress).
Run: `./gradlew :app:spotlessCheck :app:testDebugUnitTest --console=plain` → PASS.

- [ ] **Step 5: Commit**

```bash
git add scripts/android_bindings.sh android && git commit -m "build(android): add Spotless ktlint formatting gate"
```

---

### Task 5: detekt

**Files:**
- Create: `android/config/detekt.yml`
- Modify: `android/build.gradle.kts`, `android/app/build.gradle.kts`, Kotlin sources as fixes require

- [ ] **Step 1: Configure**

Add plugin alias `libs.plugins.detekt` (root `apply false`, app applied) and in `android/app/build.gradle.kts`:
```kotlin
detekt {
    buildUponDefaultConfig = true
    allRules = true
    config.setFrom(rootProject.file("config/detekt.yml"))
    source.setFrom("src/main/java", "src/main/kotlin", "src/test/java")
}
tasks.withType<dev.detekt.gradle.Detekt>().configureEach {
    exclude("**/uniffi/**") // generated by uniffi-bindgen
}
dependencies { detektPlugins(libs.compose.rules.detekt) }
```
`android/config/detekt.yml` (only Compose-driven relaxations):
```yaml
naming:
  FunctionNaming:
    ignoreAnnotated: ['Composable']  # composables are PascalCase by convention
complexity:
  LongParameterList:
    ignoreAnnotated: ['Composable']  # composables take state + modifier + callbacks
style:
  MagicNumber:
    ignorePropertyDeclaration: true
    ignoreCompanionObjectPropertyDeclaration: true
    ignoreAnnotated: ['Preview']
  UnusedPrivateMember:
    ignoreAnnotated: ['Preview']     # previews are used by tooling only
```
(If the detekt 2.x package/task name differs from `dev.detekt.gradle.Detekt`, use the one in its docs.)

- [ ] **Step 2: Confirm it fails**

Run: `cd android && ./gradlew :app:detekt --console=plain` → FAIL with findings.

- [ ] **Step 3: Fix all findings**

Fix by refactoring (extract functions, named constants, early returns). A `@Suppress("RuleName") // reason` is allowed only on the single declaration and only when the fix would hurt readability; list each in the commit body.

- [ ] **Step 4: Verify**

Run: `./gradlew :app:detekt :app:testDebugUnitTest --console=plain` → PASS. Add `val probe = 12345` in a non-Composable file → detekt fails `MagicNumber`/`UnusedPrivateProperty`; remove.

- [ ] **Step 5: Commit**

```bash
git add android && git commit -m "build(android): add strict detekt gate"
```

---

### Task 6: Android Lint + warnings as errors + `check-android`

**Files:**
- Modify: `android/app/build.gradle.kts`, `justfile`, Kotlin sources/resources as fixes require

- [ ] **Step 1: Configure**

In `android { }`:
```kotlin
lint {
    warningsAsErrors = true
    abortOnError = true
    checkDependencies = true
    checkReleaseBuilds = true
}
```
At top level:
```kotlin
kotlin { compilerOptions { allWarningsAsErrors.set(true) } }
```

- [ ] **Step 2: Confirm it fails**

Run: `cd android && ./gradlew :app:lintDebug :app:compileDebugKotlin --console=plain` → FAIL (lint warnings and/or compiler warnings).

Generated `uniffi` code can't be edited or suppressed in-file, and disabling an issue globally (`lint { disable += ... }`) is not allowed. For each lint issue id that fires only in generated code, add a path ignore to `android/app/lint.xml`:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<lint>
    <!-- uniffi-bindgen output; regenerated every build -->
    <issue id="ISSUE_ID">
        <ignore path="src/main/kotlin/uniffi/**" />
    </issue>
</lint>
```
If a Kotlin *compiler* warning comes from generated code, the `allWarningsAsErrors` gate can't be scoped to a path: report it to the owner instead of weakening the flag.

- [ ] **Step 3: Fix findings**

Fix real issues (deprecated APIs, unused resources, hardcoded strings → `strings.xml`, missing content descriptions). Newer-dependency warnings (`GradleDependency`, `NewerVersionAvailable`): bump the version in `libs.versions.toml` rather than disable.

- [ ] **Step 4: Add the recipe**

`justfile`:
```just
# Android: bindings (host build), format, static analysis, lint, unit tests
check-android:
    bash scripts/android_bindings.sh
    cd android && ./gradlew :app:spotlessCheck :app:detekt :app:lintDebug :app:testDebugUnitTest --console=plain -q
```

- [ ] **Step 5: Verify**

Run: `just check-android` → PASS. Run `rm -rf android/app/src/main/kotlin/uniffi && just check-android` → PASS (fresh-clone case, Review Focus 4).

- [ ] **Step 6: Commit**

```bash
git add android justfile && git commit -m "build(android): enforce lint and Kotlin warnings"
```

---

### Task 7: Coverage ratchet

**Files:**
- Modify: `apworld/pyproject.toml`, `justfile`, `android/build.gradle.kts`, `android/app/build.gradle.kts`, `android/gradle/libs.versions.toml`

- [ ] **Step 1: Measure today**

- Python: `uv add --project apworld --dev pytest-cov` then `uv run --project apworld pytest apworld --cov=ap_go2 --cov-report=term -q | tail -3`
- Rust: `cd core && cargo llvm-cov -p apgo-core -p apgo-ffi --summary-only | tail -1`
- Kotlin: apply Kover (`alias(libs.plugins.kover)` in app), then `cd android && ./gradlew :app:koverLogDebug -q`

Record the three line-coverage percentages; floor = each rounded down.

- [ ] **Step 2: Set floors**

Python, `apworld/pyproject.toml`:
```toml
[tool.coverage.run]
source = ["ap_go2"]
branch = true

[tool.coverage.report]
fail_under = <PY_FLOOR>
show_missing = true
```
and `justfile` `test:` → `uv run --project apworld pytest apworld --cov --cov-report=term-missing:skip-covered -q`.

Rust, `check-rust` last line becomes:
`cd core && cargo llvm-cov -p apgo-core -p apgo-ffi -q --fail-under-lines <RS_FLOOR>`
(replaces the plain `cargo test` line; llvm-cov runs the tests).

Kotlin, `android/app/build.gradle.kts`:
```kotlin
kover {
    reports {
        filters { excludes { packages("uniffi.*") } } // generated bindings
        verify { rule { minBound(<KT_FLOOR>) } }
    }
}
```
and append `:app:koverVerifyDebug` to the `check-android` Gradle call.

- [ ] **Step 3: Verify the gate bites**

Temporarily raise each floor to 100 → each recipe fails with a coverage message. Restore.

- [ ] **Step 4: Commit**

```bash
git add apworld justfile android && git commit -m "test: add coverage ratchet floors"
```
Commit body lists the three measured numbers.

---

### Task 8: Pre-push dispatcher + lefthook

**Files:**
- Create: `scripts/prepush.sh`, `scripts/tests/prepush_test.sh`
- Modify: `justfile`, `lefthook.yml`

**Interfaces:**
- Produces: `scripts/prepush.sh select` — reads newline-separated paths on stdin, prints space-separated recipe names; `scripts/prepush.sh` (no args) — computes the range, runs `just <recipes>`.

- [ ] **Step 1: Write the failing test**

`scripts/tests/prepush_test.sh`:
```bash
#!/usr/bin/env bash
# Tests for scripts/prepush.sh select: changed paths -> just recipes.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
sut="$here/../prepush.sh"
fail=0
expect() {
  local name="$1" input="$2" want="$3" got
  got="$(printf '%s' "$input" | bash "$sut" select)"
  if [ "$got" != "$want" ]; then echo "FAIL $name: want '$want' got '$got'"; fail=1; else echo "ok $name"; fi
}
ALL="check-hygiene check-py check-rust check-android"
expect no-range    "__ALL__"                                  "$ALL"
expect docs-only   "docs/a.md"                                "check-hygiene"
expect py          "apworld/ap_go2/x.py"                      "check-hygiene check-py"
expect deleted-py  "apworld/ap_go2/gone.py"                   "check-hygiene check-py"
expect rust-core   "core/src/lib.rs"                          "check-hygiene check-rust check-android"
expect ffi-only    "core/ffi/src/lib.rs"                      "check-hygiene check-rust check-android"
expect kotlin      "android/app/src/main/java/dev/apgo2/A.kt" "check-hygiene check-android"
expect justfile    "justfile"                                 "$ALL"
expect mixed       $'apworld/a.py\nandroid/b.kt'              "check-hygiene check-py check-android"
expect empty       ""                                         "check-hygiene"
exit $fail
```
(`core/src` → android too: the Kotlin bindings expose core types through ffi. `justfile`, `lefthook.yml`, `mise.toml`, `scripts/**`, `.github/**` → all.)

- [ ] **Step 2: Run it, confirm fail**

Run: `bash scripts/tests/prepush_test.sh`
Expected: FAIL (`prepush.sh` missing).

- [ ] **Step 3: Implement**

`scripts/prepush.sh`:
```bash
#!/usr/bin/env bash
# Pre-push: run only the `just check-*` recipes for what the pushed commits touch.
# `select` mode: paths on stdin -> recipe names on stdout ("__ALL__" = everything).
set -euo pipefail

select_recipes() {
  local py=0 rs=0 an=0 all=0 path
  while IFS= read -r path || [ -n "$path" ]; do
    case "$path" in
      "") ;;
      __ALL__|justfile|lefthook.yml|mise.toml|scripts/*|.github/*) all=1 ;;
      apworld/*) py=1 ;;
      core/*) rs=1; an=1 ;;
      android/*) an=1 ;;
    esac
  done
  if [ "$all" = 1 ]; then py=1; rs=1; an=1; fi
  local out="check-hygiene"
  [ "$py" = 1 ] && out+=" check-py"
  [ "$rs" = 1 ] && out+=" check-rust"
  [ "$an" = 1 ] && out+=" check-android"
  printf '%s\n' "$out"
}

changed_files() {
  local base
  if base="$(git rev-parse --verify -q '@{upstream}')"; then :
  elif base="$(git merge-base HEAD origin/main 2>/dev/null)"; then :
  else echo "__ALL__"; return; fi
  local files
  files="$(git diff --name-only "$base" HEAD)"
  # New branch whose upstream equals HEAD (nothing new) still pushes; check everything to be safe.
  if [ -z "$files" ] && [ "$(git rev-parse HEAD)" = "$base" ]; then echo "__ALL__"; return; fi
  printf '%s\n' "$files"
}

if [ "${1:-}" = select ]; then select_recipes; exit; fi
recipes="$(changed_files | select_recipes)"
echo "pre-push: just $recipes"
# shellcheck disable=SC2086  # word-splitting is the recipe list
exec just $recipes
```
Note: on a brand-new branch `@{upstream}` doesn't exist, so the base is `merge-base HEAD origin/main` — this checks every file the branch changed, which is the correct set.

- [ ] **Step 4: Run tests, confirm pass**

Run: `bash scripts/tests/prepush_test.sh` → all `ok`, exit 0.

- [ ] **Step 5: Split `just check` and wire lefthook**

`justfile`:
```just
check-py: lint typecheck test

check-hygiene:
    typos
    gitleaks detect --no-banner
    actionlint
    markdownlint-cli2 "**/*.md" "#**/node_modules" "#.ap" "#core/vendor" "#core/target"
    bash scripts/tests/prepush_test.sh

check: check-hygiene check-py check-rust check-android
```
Make `check-py` fail clearly when `.ap/` is missing: prepend to `check-py` a dependency `ap-present` recipe:
```just
[private]
ap-present:
    @test -d .ap || { echo "Archipelago checkout missing: run 'just setup-ap'"; exit 1; }
```
and `check-py: ap-present lint typecheck test`.

`lefthook.yml`:
```yaml
pre-commit:
  parallel: true
  commands:
    ruff-format:
      glob: "apworld/**/*.py"
      run: uv run --project apworld ruff format --check {staged_files}
    ruff-lint:
      glob: "apworld/**/*.py"
      run: uv run --project apworld ruff check {staged_files}
    rustfmt:
      glob: "core/**/*.rs"
      exclude: ["core/vendor/**"]
      run: cd core && cargo fmt --all --check
    ktlint:
      glob: "android/**/*.{kt,kts}"
      run: cd android && ./gradlew :app:spotlessCheck --console=plain -q
    typos:
      run: typos --force-exclude {staged_files}
    gitleaks:
      run: gitleaks protect --staged --no-banner
pre-push:
  commands:
    checks:
      run: bash scripts/prepush.sh
commit-msg:
  commands:
    committed:
      run: committed --commit-file {1}
```
Run: `lefthook install`.

- [ ] **Step 6: Verify end to end**

- `mv .ap .ap.bak && just check-py; mv .ap.bak .ap` → prints the `setup-ap` hint, non-zero.
- `lefthook run pre-push` → prints `pre-push: just check-hygiene check-py check-rust check-android` (branch touches all) and passes.
- Stage a Kotlin file with bad indentation → `git commit` blocked by `ktlint`; restore.

- [ ] **Step 7: Commit**

```bash
git add scripts justfile lefthook.yml && git commit -m "build: run per-language checks on pre-push"
```

---

### Task 9: CI, pins, Dependabot

**Files:**
- Modify: `.github/workflows/ci.yml`, `.github/workflows/release-please.yml`, `.github/dependabot.yml`, `mise.toml`

- [ ] **Step 1: Pin `mise.toml`**

Replace every `latest` with the installed version: `uv = "0.11.2"`, `just = "1.58.0"`, `lefthook = "2.1.17"`, `gitleaks = "8.30.0"`, `typos = "1.51.1"`, `actionlint = "1.7.12"`, `"aqua:crate-ci/committed" = "1.1.11"`; keep `python = "3.12"`; add `java = "temurin-25"` (matches local JDK 25) and `"npm:markdownlint-cli2" = "<latest stable, looked up>"` (plus `node = "<current LTS>"`). Run `markdownlint-cli2` once and fix any hits in existing docs within this commit. Cargo tools were pinned in Task 2.

- [ ] **Step 2: Resolve action SHAs**

For each action used (`actions/checkout`, `jdx/mise-action`, `actions/upload-artifact`, `actions/setup-java`, `gradle/actions/setup-gradle`, `Swatinem/rust-cache`, `googleapis/release-please-action`): find the latest release tag and its commit SHA with
`gh api repos/<owner>/<repo>/releases/latest --jq .tag_name` then `gh api repos/<owner>/<repo>/commits/<tag> --jq .sha`.

- [ ] **Step 3: Rewrite `ci.yml`**

```yaml
name: ci
on:
  push:
    branches: [main]
  pull_request:
permissions:
  contents: read
concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true
jobs:
  hygiene:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@<SHA> # <tag>
        with:
          fetch-depth: 0
      - uses: jdx/mise-action@<SHA> # <tag>
      - run: just check-hygiene
  python:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@<SHA> # <tag>
      - uses: jdx/mise-action@<SHA> # <tag>
      - run: just setup-ap
      - run: just check-py
      - run: just build
      - uses: actions/upload-artifact@<SHA> # <tag>
        with:
          name: ap_go2.apworld
          path: dist/ap_go2.apworld
  rust:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@<SHA> # <tag>
      - uses: jdx/mise-action@<SHA> # <tag>
      - uses: Swatinem/rust-cache@<SHA> # <tag>
        with:
          workspaces: core
      - run: just check-rust
  android:
    runs-on: ubuntu-24.04
    steps:
      - uses: actions/checkout@<SHA> # <tag>
      - uses: jdx/mise-action@<SHA> # <tag>
      - uses: Swatinem/rust-cache@<SHA> # <tag>
        with:
          workspaces: core
      - uses: gradle/actions/setup-gradle@<SHA> # <tag>
      - run: just check-android
```
Each `<SHA> # <tag>` is the real value from Step 2. The justfile's hardcoded `JAVA_HOME := "/usr/lib/jvm/java-25-openjdk"` would break CI: change it to `export JAVA_HOME := env("JAVA_HOME", "/usr/lib/jvm/java-25-openjdk")` (mise sets `JAVA_HOME`). Same for `ANDROID_HOME`: `env("ANDROID_HOME", env("HOME") + "/Android/Sdk")` (runner sets `ANDROID_HOME`). Apply the same SHA pinning to `release-please.yml`.

- [ ] **Step 4: Dependabot**

Append to `.github/dependabot.yml`:
```yaml
  - package-ecosystem: cargo
    directory: /core
    schedule:
      interval: weekly
  - package-ecosystem: gradle
    directory: /android
    schedule:
      interval: weekly
```

- [ ] **Step 5: Verify locally**

Run: `actionlint && just check` → PASS.

- [ ] **Step 6: Commit, push, watch CI**

```bash
git add .github mise.toml justfile && git commit -m "ci: split jobs per language and pin actions"
git push -u origin chore/strict-quality-gates
gh run watch --exit-status
```
Expected: all four jobs green. If `android` fails on missing SDK components, add `sdkmanager --install "platforms;android-37" "build-tools;<agp default>"` step and re-push. Open a draft PR only when the owner asks.

---

### Task 10: Docs and follow-up issues

**Files:**
- Modify: `CONTRIBUTING.md`, `docs/context/working-in-this-repo.md`, `CLAUDE.md` (Commands line only), `docs/superpowers/specs/2026-10-07-strict-quality-gates-design.md` (note host-bindings + dispatcher deviations)

- [ ] **Step 1: Update docs**

- CLAUDE.md `## Commands`: `just check` (everything), `just check-py|check-rust|check-android|check-hygiene`; pre-push runs `scripts/prepush.sh`; coverage floors only go up.
- `working-in-this-repo.md`: new gotchas — generated `uniffi/` excluded from all Kotlin gates; `scripts/android_bindings.sh` for host bindings; how to raise a coverage floor; allow/suppress needs a reason comment.
- `CONTRIBUTING.md`: setup (`just setup`, `cargo install --locked cargo-deny cargo-llvm-cov`), recipes, hook behaviour.
- Spec: add a "Deviations" section: host-built bindings in CI (no NDK); pre-push via tested script instead of lefthook `{push_files}` globs; `unsafe_code` deny at workspace + forbid in `apgo-core`.

- [ ] **Step 2: File issues**

```bash
gh issue create -R Rasbandit/archipela-go -l enhancement -t "Run emulator e2e in CI" -b "Needs a KVM-capable runner. Out of scope of strict-quality-gates spec."
gh issue create -R Rasbandit/archipela-go -l enhancement -t "Add mutation testing" -b "cargo-mutants / mutmut once coverage floors rise."
gh issue create -R Rasbandit/archipela-go -l enhancement -t "Enable branch protection on main" -b "Require ci jobs hygiene, python, rust, android; require linear history; block force pushes."
```

- [ ] **Step 3: Verify and commit**

Run: `just check-hygiene` (typos over docs) → PASS.
```bash
git add CONTRIBUTING.md CLAUDE.md docs && git commit -m "docs: document strict quality gates"
```
