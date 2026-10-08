# Archipela-Go 2

Our own successor to Archipela-Go! (upstream `aki665/react-native-archipelago`, stale): an Archipelago multiworld game
where checks are real-world places reached by walking, biking or driving. Android first, iOS later. Upstream is
inspiration only; we write our own apworld and client.

## Status

Working Android app + Rust core + apworld 0.3.0 (slot_data schema 3), verified on an emulator (real phone: Pixel 8 Pro over wireless adb). **Read
`docs/context/working-in-this-repo.md` (preferences, commands, gotchas, backlog) and `docs/context/v1-architecture-and-status.md` (what exists, what is
verified, what is NOT) at the start of every session.** Never work on main; branch per task. Specs: `docs/superpowers/specs/`.

## Commands

`just check` (everything; also `just check-py|check-rust|check-android|check-hygiene`), `just android-run` (phone), `just emu-start && just emu-run && just e2e` (emulator),
`just ap-host` (dev Archipelago server), `just build` (apworld artifact). Pre-push runs `scripts/prepush.sh` (only the recipes for touched paths). Coverage floors (py 99, rust 80, kotlin 17) only go up.
`mise.toml` pins tools; `committed`, `gitleaks`, `cargo-deny`, `cargo-llvm-cov` and `markdownlint-cli2` need installing locally (`CONTRIBUTING.md`).

## Stack (decided and built)

Rust core (AP protocol via `archipelago_rs`, location generation, geofence, SQLite) + Kotlin/Compose Android UI
via UniFFI. iOS later with SwiftUI over the same core. Apworld in Python. Monorepo: `apworld/`, `core/`, `android/`,
`docs/`. Run an Android build + connect spike for Rust before committing to the stack.

## Sub-projects (each: spec, plan, build)

1. Apworld + client contract + root/Python tooling (spec done)
2. Rust core  3. Android app  4. Rust/Kotlin tooling  5. iOS

## Key decisions (full log: `docs/context/project-decisions.md`)

- Own apworld, game `Archipela-Go 2: Electric Boogaloo`, new ID offset. Archipelago's own conventions are the golden rule.
- Realms are places; **travel mode is chosen per zone in a game**; zones are gated by keys and tools; quests come from a 76-kind catalog on real finds.
- Several win conditions (any / all / at least N) are client-evaluated and reported with `StatusUpdate`; slot_data is schema 3.
- Scans use a global tile grid and a shared cache; the editor autosaves with undo/redo; one design system (`ui/`) and one help-text file.
- Licence is MIT today but the owner plans to monetize: decide before going public (`working-in-this-repo.md`).

## Conventions

- Conventional commits (`feat:`, `fix:`, `docs:`), subject under 50 chars. Small, tightly scoped steps.
- TDD: failing tests first; never edit tests to fit bad code.
- Upstream code is MIT (keep notice if copying); upstream apworld has NO license: reimplement, never copy.
- Web research: use Perplexity, Firecrawl and GitHub MCPs, not built-in WebSearch/WebFetch.
- Work tracking: GitHub issues on `Rasbandit/archipela-go` (`gh issue list`) are the backlog and todo list. Ideas, bugs and follow-ups become issues (label `enhancement` or `bug`); close them from the commit or PR that finishes them. Do not keep a TODO.md.
- Life OS / work-log tagging is intentionally skipped in this project (owner decision).
- apworld package code uses relative imports (Archipelago loads it as `worlds.ap_go2`); tests import `worlds.ap_go2`.
- Parallel agents must not edit this file concurrently (two agents once overwrote each other's index lines).
- Every session works in its own git worktree under `.claude/worktrees/`; never switch branches in the main checkout (see `working-in-this-repo.md`).

## Context Docs

If you need the project decision log, risks and unverified items, see `docs/context/project-decisions.md`
If you need info on the Archipelago websocket protocol (packets, items_handling, DataStorage), see `docs/context/archipelago-network-protocol.md`
If you need info on Archipelago concepts (worlds, apworlds, YAML, hints, classification), see `docs/context/archipelago-concepts.md`
If you need info on Archipelago client libraries (Rust, JVM, TS, C#), see `docs/context/archipelago-client-libraries.md`
If you need info on Archipela-Go! game design (apworld, items, slot_data), see `docs/context/archipela-go-game-design.md`
If you need info on the upstream React Native app architecture and license, see `docs/context/archipela-go-upstream-architecture.md`
If you need info on location generation (upstream bug and Overpass redesign), see `docs/context/archipela-go-location-generation.md`
If you need how to write an Archipelago apworld (skeleton, options, regions, tests, packaging), see `docs/context/apworld-development-guide.md`
If you need apworld gotchas (item pool size, Python version, manifest, Victory event in tests), see `docs/context/apworld-pitfalls.md`
If you need example worlds to copy patterns from, see `docs/context/apworld-reference-implementations.md`
If you need info on POI data sources, map tiles or POI architecture, see `docs/context/poi-data-sources.md`, `docs/context/map-rendering-and-tiles.md`, `docs/context/poi-atlas-and-server-options.md`
If you need Spike B results (bulk POI generation timings, rural gaps, next fixes), see `docs/context/spike-b-location-generation-results.md`
If you need the Android build/run loop, versions and gotchas, see `docs/context/android-dev-workflow.md`
If you need the dev Archipelago server loop (`just ap-host`) and Android-to-server gotchas, see `docs/context/android-dev-workflow.md`
If you need how our game maps onto Archipelago (YAML, apworld, fill, phone) or the Zelda reference worlds, see `docs/context/archipelago-game-model.md`
If you need quest-type ideas, phone sensor/health API permissions and the quest table, see `docs/context/quest-types-and-phone-apis.md`
If you need progression design (zones as regions, trail quests, Freeze trap, OSM trail data findings; Bike/Car purged), see `docs/context/progression-zones-and-tools.md`
If you need the economy/progression direction (walk/run, money tiers, backpack/bank, shop, eggs, buddies, treehouse), see `docs/context/economy-eggs-buddies.md`
If you need measured OpenStreetMap tag coverage (surface/lit/parks/trails), the surface-filter design and Fog of War design, see `docs/context/map-data-capabilities.md`
If you need the v1 architecture, verified results, known gaps and how to run the emulator/e2e, see `docs/context/v1-architecture-and-status.md`
If you need the full list of quest kinds (names, map filters, proof, modes), see `docs/context/quest-catalog.md` (generated)
If you need the Android UI design system (palette, theme, shared components, rules) or the Archipelago logo/icon licence finding, see `docs/context/ui-design-system.md`
If you need how scans, the tile grid, the shared cache, pacing/retries and the scan cooldown work, see `docs/context/scan-and-tile-cache.md`
If you need the outdoor test steps and how to pull/read the diagnostics, see `docs/context/outdoor-test-plan.md`
If you need to start working (owner preferences, build/install/test commands, gotchas, backlog, licence/publishing), see `docs/context/working-in-this-repo.md`
