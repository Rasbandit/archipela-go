# Contributing

## Setup

```bash
mise install      # every pinned tool in mise.toml (python, uv, just, lefthook, gitleaks, typos, committed,
                  # actionlint, shellcheck, cargo-deny, cargo-llvm-cov, cargo-mutants, markdownlint-cli2, node, java)
just setup        # python env, pinned Archipelago checkout in .ap/, git hooks
```

Also needed, outside mise:

- rustup: `core/rust-toolchain.toml` pins Rust 1.99.0 plus the Android targets; rustup installs them on first use.
- Android SDK with `ANDROID_HOME` set (default `~/Android/Sdk`), and JDK 25 with `JAVA_HOME` (mise installs the JDK).
- `just check` includes `check-android`, which needs the SDK. `just check-py` and `just check-rust` work without it.

Without mise, install the pinned versions yourself: lefthook, typos, gitleaks, committed, actionlint, shellcheck
(versions in `mise.toml`), then:

```bash
cargo install --locked cargo-deny@0.20.2 cargo-llvm-cov@0.9.1 cargo-mutants@27.1.0
npm install -g --prefix ~/.local markdownlint-cli2@0.23.3
```

## Workflow

- Branch off `main`: `feat/...`, `fix/...`, `docs/...`. Never commit to `main` directly.
- Write the failing test first, then the code. Do not edit a test to make bad code pass.
- `just check` runs every gate below. It must pass before you push.
- Conventional commits (`feat:`, `fix:`, `docs:`, `chore:`), subject under 50 characters, body lines 72 or less.
  The `committed` hook rejects `build` and `ci`: use `chore`.
- `just build` produces `dist/ap_go2.apworld`.

## Recipes

- `just check-hygiene`: typos, gitleaks, actionlint, shellcheck (`scripts/`), markdownlint-cli2, pre-push dispatcher tests.
- `just check-py` (needs `.ap/`): ruff (ALL, also over `scripts/*.py` via `scripts/ruff.toml`), pyright strict, pytest with a coverage floor of 99.
- `just check-rust`: rustfmt, clippy (pedantic, deny), rustdoc `-D warnings`, cargo deny, cargo llvm-cov floor of 80.
- `just check-android`: host-built bindings, Spotless/ktlint, detekt, Android Lint (warnings are errors), unit tests,
  Kover floor of 17.
- Coverage floors only go up (see `docs/context/working-in-this-repo.md`).
- Mutation testing (slow, not part of `check`): `just mutate-py` (mutmut over `apworld/ap_go2`, staged in `.mutate-py/`)
  and `just mutate-rust` (cargo-mutants over `apgo-core`, in place; `-f src/goal.rs` for one file, all of it takes hours).
  Before a PR that touches `core/`, `just mutate-rust-diff` mutates only the lines the branch changed.
  A surviving mutant is a change no test notices: add the test that kills it.

## Hooks

- pre-commit: ruff, rustfmt, ktlint (`spotlessCheck`), typos, gitleaks.
- commit-msg: `committed`.
- pre-push: `scripts/prepush.sh` reads the pushed refs, maps changed paths to the recipes above and runs only those
  (plus hygiene). It falls back to the upstream or merge-base diff, and runs everything when unsure.
- CI (`.github/workflows/ci.yml`) runs the same four jobs: hygiene, python, rust, android. Actions are pinned by SHA;
  Dependabot covers uv, github-actions, cargo and gradle. It can also be started manually (`workflow_dispatch`).

## Layout

- `apworld/` Python apworld (`ap_go2/` package, `tests/`, `docs/contract.md`)
- `docs/` specs, plans and context docs
- `scripts/` build and setup helpers

If you change `slot_data`, update `apworld/docs/contract.md` and `apworld/docs/slot_data.schema.json`
in the same pull request.
