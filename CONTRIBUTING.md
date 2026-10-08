# Contributing

## Setup

```bash
mise install      # pinned toolchain (python, uv, just, lefthook, gitleaks, typos, committed, actionlint, java)
just setup        # python env, pinned Archipelago checkout in .ap/, git hooks
cargo install --locked cargo-deny cargo-llvm-cov
npm install -g --prefix ~/.local markdownlint-cli2@0.23.3
```

Without mise, install lefthook, typos, gitleaks, committed and actionlint yourself.

## Workflow

- Branch off `main`: `feat/...`, `fix/...`, `docs/...`. Never commit to `main` directly.
- Write the failing test first, then the code. Do not edit a test to make bad code pass.
- `just check` runs every gate below. It must pass before you push.
- Conventional commits (`feat:`, `fix:`, `docs:`, `chore:`), subject under 50 characters, body lines 72 or less.
  The `committed` hook rejects `build` and `ci`: use `chore`.
- `just build` produces `dist/ap_go2.apworld`.

## Recipes

- `just check-hygiene`: typos, gitleaks, actionlint, markdownlint-cli2, pre-push dispatcher tests.
- `just check-py` (needs `.ap/`): ruff (ALL), pyright strict, pytest with a coverage floor of 99.
- `just check-rust`: rustfmt, clippy (pedantic, deny), rustdoc `-D warnings`, cargo deny, cargo llvm-cov floor of 80.
- `just check-android`: host-built bindings, Spotless/ktlint, detekt, Android Lint (warnings are errors), unit tests,
  Kover floor of 7.
- Coverage floors only go up (see `docs/context/working-in-this-repo.md`).

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
