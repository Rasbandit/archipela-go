# Strict quality gates — design

## Intent

Owner wants the repo in top-tier shape: the strictest sane lint, type, format and test standards for every language,
enforced locally (git hooks) and in CI, with no grandfathered violations.

Decided with the owner:

- **Pre-push is a smart split:** it runs only the checks for languages touched by the pushed commits. CI runs everything.
- **Coverage is a ratchet:** the floor is today's measured number per language; it can only go up.
- **Existing violations are fixed now**, not baselined. Allows are narrow, local and carry a reason comment.
- Clippy `pedantic` is denied; `nursery` and `restriction` lints are cherry-picked, never enabled wholesale.
- Tool versions in `mise.toml` and GitHub Actions are pinned (Actions by commit SHA); Dependabot bumps them.

Success: `just check` green; a deliberate violation in any language fails the matching hook and CI job; CI has one
job per language plus hygiene; coverage below the floor fails.

## Scope

Our code only: `apworld/`, `core/` (crates `apgo-core`, `apgo-ffi`), `android/`, `scripts/`, workflows.
Vendored `core/vendor/archipelago_rs` is excluded from lints.

Out of scope (issues filed): emulator e2e in CI (needs a KVM runner), mutation testing, GitHub branch protection
(owner sets it; checklist in the PR).

## Steps (one branch, one commit per step, in order)

### 1. Rust strict

- `core/Cargo.toml` `[workspace.lints]`; each of our crates sets `lints.workspace = true`.
  - `rust`: `unsafe_code = "forbid"` (`apgo-ffi` overrides to `deny` with local allows only if UniFFI needs it),
    `missing_docs = "deny"` on public items, `unused_qualifications`, `rust_2018_idioms`.
  - `clippy`: `pedantic = deny` (priority -1); from nursery: `use_self`, `redundant_clone`, `significant_drop_tightening`,
    `derive_partial_eq_without_eq`; from restriction: `unwrap_used`, `expect_used` (allowed in tests via
    `clippy.toml` `allow-unwrap-in-tests`/`allow-expect-in-tests`), `dbg_macro`, `todo`, `print_stdout`.
  - Allow-list with reasons: `module_name_repetitions`; others only if justified in the commit body.
- Cast lints are fixed with `try_from`/checked conversions; a local `#[allow]` with a comment only where loss is
  intended (geo math).
- `RUSTFLAGS=-D warnings` everywhere; `just check-rust` also runs `cargo test` for `apgo-ffi` and `cargo doc` with
  `-D warnings`.
- `cargo-deny` (`core/deny.toml`): advisories, licence allow-list (MIT/Apache/BSD/ISC/Unicode/Zlib/MPL), bans
  duplicate major versions as warnings, sources restricted to crates.io. Matters for the future monetisation decision.

### 2. Python strict

- Pyright `typeCheckingMode = "strict"` for all of `ap_go2` and `tests`; remove the per-file strict list.
- Ruff `select = ["ALL"]` with an explicit, commented ignore list (formatter conflicts `COM812`, `ISC001`; one docstring
  convention via `pydocstyle.convention = "google"`; `D` relaxed for tests).
- Fix all hits.

### 3. Android strict

- Spotless + ktlint (format check, Compose rules via `io.nlopez.compose.rules:ktlint`).
- detekt with `buildUponDefaultConfig`, `allRules = true`, a committed `config/detekt.yml` turning off only rules that
  fight Compose (e.g. `FunctionNaming` for `@Composable`, `LongParameterList` for composables) with reasons.
- Android Lint: `warningsAsErrors = true`, `abortOnError = true`, `checkDependencies = true`.
- Kotlin `allWarningsAsErrors = true`.
- `just android-check` = spotlessCheck + detekt + lintDebug + testDebugUnitTest.

### 4. Coverage ratchet

- Rust `cargo llvm-cov`, Python `pytest-cov`, Kotlin Kover.
- Floors live in each tool's config (`--fail-under-lines`, `fail_under`, Kover `minBound`), set to today's value
  rounded down to the integer. Raising the floor is a normal commit; lowering one needs a reason in the commit body.

### 5. Hooks, CI, hygiene

- `justfile`: `check` = `check-py check-rust check-android check-hygiene`; each runnable alone.
- `lefthook.yml`:
  - pre-commit (staged files, fast): ruff, `cargo fmt --check`, ktlint via Spotless on staged Kotlin, typos, gitleaks.
  - pre-push: one command per language gated by `glob` over `{push_files}` (`apworld/**` → `check-py`,
    `core/**` → `check-rust`, `android/**` or `core/ffi/**` → `check-android`); `check-hygiene` always.
  - commit-msg: `committed` (unchanged).
- CI (`ci.yml`): parallel jobs `python`, `rust` (incl. cargo-deny, llvm-cov), `android` (JDK 25, Gradle cache, NDK,
  builds the Rust core for the app), `hygiene` (actionlint, gitleaks, typos, markdownlint). All Actions pinned by SHA.
- `mise.toml`: every tool pinned to an exact version; add `cargo-deny`, `cargo-llvm-cov`, `markdownlint-cli2`.
- Dependabot: add `cargo` (`/core`) and `gradle` (`/android`) ecosystems.
- Update `CONTRIBUTING.md`, `docs/context/working-in-this-repo.md`, and CLAUDE.md Commands with the new recipes.

## Verification (the "failing test" for tooling)

For each gate: introduce one deliberate violation, confirm the recipe/hook/CI job fails with a clear message, remove
it, confirm green. Each step ends with a green full `just check`.

## Risks

- Android CI needs the NDK and Rust Android targets; first run may need cache tuning. Fallback: build only the JVM
  unit tests + lint without the native library if the tests don't load it.
- `{push_files}` is empty on a new branch's first push in some lefthook versions; the pre-push then falls back to
  running every language (safe default).
- Large mechanical diffs in step 1/2; kept to `--fix` output plus hand fixes, reviewed per lint group.

## Deviations (as built)

- Android CI builds the UniFFI bindings on the host (`scripts/android_bindings.sh`); no NDK or Rust Android targets.
- Pre-push is a tested script (`scripts/prepush.sh`) that reads git's pushed refs on stdin, instead of lefthook
  `{push_files}` globs. It falls back to upstream/merge-base, and to everything (`__ALL__`) when unsure.
- `unsafe_code` is denied at the workspace and `#![forbid(unsafe_code)]` is set in `apgo-core`.
- Commit types `build`/`ci` are rejected by the `committed` hook; use `chore`.
- Cast lints use saturating helpers in `core/src/num.rs` instead of propagating `try_from` errors.
- detekt 2.0.0-alpha.6: no stable 2.x supports Kotlin 2.4. The Compose `UnstableCollections` rule is disabled
  (strong skipping).
- Ruff's copyright-header rule is ignored (licence undecided).
- Release automation uses release-please-action v5.
- `just core-check` was renamed `just check-rust`.
