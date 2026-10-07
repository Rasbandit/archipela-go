# Contributing

## Setup

```bash
mise install      # pinned toolchain (python, uv, just, lefthook, gitleaks, typos, committed)
just setup        # python env, pinned Archipelago checkout in .ap/, git hooks
```

## Workflow

- Branch off `main`: `feat/...`, `fix/...`, `docs/...`. Never commit to `main` directly.
- Write the failing test first, then the code. Do not edit a test to make bad code pass.
- `just check` runs lint, format check, type check, tests and spell check. It must pass before you push.
- Conventional commits (`feat:`, `fix:`, `docs:`, `build:`), subject under 50 characters.
- `just build` produces `dist/ap_go2.apworld`.

## Layout

- `apworld/` Python apworld (`ap_go2/` package, `tests/`, `docs/contract.md`)
- `docs/` specs, plans and context docs
- `scripts/` build and setup helpers

If you change `slot_data`, update `apworld/docs/contract.md` and `apworld/docs/slot_data.schema.json`
in the same pull request.
