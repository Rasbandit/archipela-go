# Working in this repo (read this first in a new chat)

## Owner preferences (these were stated repeatedly)

- Small, tightly scoped steps; commit after each meaningful step (conventional commits, subject under 50 chars, imperative mood, body lines under 72; the
  `committed` hook rejects otherwise). Use the git MCP tools for git. Never work on `main`; branch (`feat/...`) and merge.
- TDD for core/apworld logic (failing test first, never edit a test to fit bad code). UI is checked on the emulator with screenshots.
- Android UI feedback is visual and iterative: build, install, screenshot, adjust. The owner tests on a real Pixel 8 Pro.
- Never use built-in WebSearch/WebFetch: use Perplexity/Firecrawl/GitHub MCPs. Life OS / work-log tagging is skipped here.
- **Archipelago standards are the golden rule**: copy patterns from official worlds (see `archipelago-game-model.md`) and verify with real generation.
- Be honest about what was verified; say plainly when something was only checked on the emulator.
- **Android and iOS alike** (hard rule, `CLAUDE.md`): game logic, state and persistence in the Rust core; Kotlin/Swift only draw and wrap platform APIs.
- **Events over polling** (hard rule, `CLAUDE.md`): battery matters most. Drive logic from events (presence changes, fixes that arrive anyway, one scheduled
  wake-up when something falls due: core `next_due_ms`/`tick`, app `DueTimer`) and compute derived values when asked. No ticking loops; GPS only reports
  movement while playing. The polling audit of 2026-10-09 removed the AP 300 ms full refresh, the Activity 3 s reload, the 60 s service heartbeat, presence
  re-check polls, the 5 s step refresh, scan-progress polling and GPS-for-timing.

## Parallel sessions: always work in a worktree

Several Claude sessions and agents run against this repo at once. The main checkout (`Archipela-Go/`) stays on `main` for the
owner: never switch branches or edit files there. A `git switch` by one session changes the files under every other session
(on 2026-10-08 one session switched the main checkout to its branch while another was mid-task in it).

- New work: Claude Code's `EnterWorktree` (creates `.claude/worktrees/<name>` on a new branch from `origin/main`), then
  `git branch -m feat/<name>`. Existing branch: `git worktree add .claude/worktrees/<name> <branch>`, then `EnterWorktree` with
  that `path`. `.claude/worktrees/` is gitignored.
- A fresh worktree has none of the gitignored state: run `just setup-ap` before `just check-py`; `just check-android` builds the
  uniffi bindings itself; Gradle finds the SDK through `ANDROID_HOME` (the justfile exports it).
- Shared across worktrees: branches, hooks and the stash stack. Never use a bare `git stash`/`git stash pop` (you can pop another
  session's entry); use a WIP commit. A branch can be checked out in only one worktree at a time.
- Git hooks export `GIT_DIR` (pointing at the real repo, in any checkout). Any script or test that builds a scratch repo must
  start with `unset $(git rev-parse --local-env-vars)`, or its `git init`/`commit` lands in the real repo: on 2026-10-08 a
  pre-push run committed test fixtures onto a pushed branch and set `core.bare=true` for every checkout.
  `scripts/tests/git_env_test.sh` (in `check-hygiene`) catches this for every `*_test.sh`.
- After the PR merges: `git worktree remove .claude/worktrees/<name>` and `git branch -d <branch>`.

## The loop

| Task | Command |
| -- | -- |
| All gates | `just check` (or `just check-hygiene`, `check-py`, `check-rust`, `check-android`); pre-push runs only what your changes touch |
| Core tests / lint | `cd core && cargo test && cargo fmt && cargo clippy --all-targets -- -D warnings` (or `just check-rust`) |
| Apworld tests / lint | `uv run --project apworld pytest apworld -q`, `ruff check apworld`, `ruff format apworld`, `pyright --project apworld` (or `just check-py`) |
| Rebuild native libs + Kotlin bindings | `APGO_ABIS="arm64-v8a x86_64" bash scripts/android_core.sh debug` (needed after ANY change to `core/ffi`; x86_64 is the emulator) |
| Build APK | `cd android && ./gradlew assembleDebug --console=plain -q` |
| Emulator | `adb -s emulator-5554 install -r android/app/build/outputs/apk/debug/app-debug.apk`; start with `just emu-start` (headless, `-gpu swangle_indirect`) |
| Phone | `adb connect 10.0.20.151:40843` then `ANDROID_SERIAL=10.0.20.151:40843 adb install -r ...`. When both are attached always set `ANDROID_SERIAL` |
| Screenshot | `adb exec-out screencap -p > /tmp/x.png` then Read the PNG |
| UI driver | `python3 scripts/android_ui.py texts \| tap "Label" [exact] \| tapn \| type \| wait` (honors `ANDROID_SERIAL`; prefix with `timeout 20`) |
| Dev Archipelago server | `APGO_GOALS="Letter Hunt,The Big One" APGO_REQ=require_all_goals scripts/ap_host.sh start 60`, `scripts/ap_host.sh stop` |
| Parse a slot_data file with the app's reader | `cd core && cargo run -q --example parse_slot -- file.json` |

## Gotchas that cost time before

- **The phone sleeps and wireless adb dies** ("No route to host"). Ping it a few times (`ping -c1 10.0.20.151`) then `adb connect` again; retry up to 4 times. Ask the
  owner to wake/unlock it and toggle Wireless debugging if it persists. Never try to bypass its lock screen. USB is the sure fallback.
- `pkill -f` kills your own shell; stop servers via pid file / port listener. `adb shell input swipe` from a screen edge triggers Android's Back gesture.
- Do not start swipes at x<60 or x>1020 on the 1080-wide emulator. UI coordinates in screenshots are scaled by 1.2 (the Read tool says so).
- MapLibre does not always repaint after a GeoJSON change on a still camera: call `map.triggerRepaint()`. A bounds fit REPLACES the map padding: include overlay padding.
- Read editor state inside click handlers (not from vals captured at composition) or undo/redo saves stale values.
- UniFFI: a record field named `message` clashes with Kotlin's Throwable.message; rustls needs the ring provider installed explicitly; generated Kotlin lives in `src/main/kotlin`.
- Compose icons: Lucide names differ from memory (e.g. no `CloudCheck`; lucide 2 renamed `CircleHelp` to `CircleQuestionMark`). A wrong name is a compile error: fix by trying the compiler.
- The commit hook wants imperative subjects (`feat: show finds`, not `feat: finds ...`) and lines under 72 chars in the body.
- Generated `android/app/src/main/kotlin/uniffi/` is excluded from every Kotlin gate (Spotless, detekt, Lint, Kover); never edit or lint it. `core/vendor/` is likewise untouched.
- `bash scripts/android_bindings.sh` builds the bindings on the host (no NDK) for `just check-android` and CI; `android_core.sh` is for device builds.
- Raising a coverage floor: python `fail_under` in `apworld/pyproject.toml`, rust `--fail-under-lines` in the `check-rust` justfile recipe, kotlin `minBound` in `android/app/build.gradle.kts`.
  Set it to the measured line coverage rounded down. Floors only go up.
- Every `allow` / `ignore` / `@Suppress` / `noqa` must be as local as possible and carry a reason comment.
- Mutation testing: mutmut must see the world as `worlds.ap_go2` (its keys come from the file path), hence the staging in
  `scripts/mutate_py.sh`. cargo-mutants runs in place (`yaml.rs`, `slot.rs` include files outside `core/`); a stopped run can leave a
  `~ changed by cargo-mutants ~` line in `core/src` (check and Android recipes refuse it; `git restore core/src`). A killed
  `just mutate-py run` can leave `.ap/worlds/ap_go2` on the mutants (`check-py` refuses it; `just setup-ap`).
- rustfmt width is 160; after `cargo fmt` literals may be reformatted, so re-read before scripted edits.

## Where things are decided (pointers, do not duplicate)

`project-decisions.md` (decision log), `ui-design-system.md` (palette, components, icons, help, editor model), `scan-and-tile-cache.md`,
`archipelago-game-model.md` (incl. several goals), `quest-catalog.md` (generated), `map-data-capabilities.md`, `progression-zones-and-tools.md`.

## Backlog

**Tracking lives in GitHub issues** (`gh issue list` on `Rasbandit/archipela-go`); new ideas, bugs and follow-ups are filed there (label `enhancement` or `bug`). The list below is the
older pre-issues backlog: file an issue when one of these is picked up, then delete it here.

1. ~~Foreground service + background location~~ (done), **real outdoor test and retest** (see `outdoor-test-plan.md`). Activity Recognition for mode proof is still open. Street snapping of the displayed position/trace is an idea (after the retest).
2. Rewrite `scripts/e2e_emulator.sh` for the current flows (editor, New Game zones, scan wait) and make it pass cleanly.
3. Re-test the full Archipelago session (connect, checks, items, goal, several goals) against `ap_host.sh`; apply `return_home`, DeathLink, Effort Reduction items; chat/hints.
4. About/attribution screen (OSM, OpenFreeMap, Lucide) and an own launcher icon; signing; release build size.
5. Self-hosted Overpass (FastRaid) as the primary map source; a manual "refresh tiles" action; a legend/family filter if wanted.
6. Trail polish (tune min lengths), per-zone quest types in the apworld if Archipelago players want them, timed/ordered quests, iOS.

## Publishing and licence (decided 2026-10-08)

The owner wants to monetize the app and also show it on a portfolio, so the licence is split: `android/` is **PolyForm Noncommercial 1.0.0**
(`android/LICENSE`, source-available, only the owner may sell it); everything else (`apworld/`, `core/`, `docs/`, `scripts/`) stays **MIT** (root `LICENSE`).
The repo went **public** on 2026-10-08 after a full-history gitleaks and personal-data scan (clean; only private LAN IPs and the author email are visible).
`main` is protected: PR required (0 approvals), required check `ci-ok`, enforced for admins, no force pushes; merge commits allowed (no linear history).
The root `LICENSE` must stay the plain MIT template so GitHub detects it; the per-directory split is explained in `README.md`.
Outside contributions to `android/` need a CLA or rights assignment before merge, or the owner cannot sell that code (#82). Upstream `aki665/react-native-archipelago` is MIT and no code was copied;
its apworld has no licence and was reimplemented.
