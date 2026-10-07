# Archipela-Go 2

Our own successor to Archipela-Go! (upstream `aki665/react-native-archipelago`, stale): an Archipelago multiworld game
where checks are real-world places reached by walking, biking or driving. Android first, iOS later. Upstream is
inspiration only; we write our own apworld and client.

## Status
v1 built (2026-10-08): apworld v2 (179 tests), Rust core (78 tests), Android app (Realms / New Game / Play), standalone solo mode, 12 goals, fog, traps,
surface preference, Archipelago play. Verified on an emulator end to end (solo and against a local Archipelago server). Read
`docs/context/v1-architecture-and-status.md` first: it lists what is verified and what is NOT (foreground service, real outdoor GPS, mode proof).
Specs: `docs/superpowers/specs/2026-10-08-v1-quests-realms-design.md` (v1), `2026-10-07-apworld-design.md` (v1 apworld, superseded). Never work on main.

## Commands
`just check` (apworld lint/types/tests), `cd core && cargo test`, `just android-run` (phone), `just emu-start && just emu-run && just e2e` (emulator),
`just ap-host` (dev Archipelago server), `just build` (apworld artifact). `mise.toml` pins tools; `committed` and `gitleaks` need installing locally.

## Stack (decided and built)
Rust core (AP protocol via `archipelago_rs`, location generation, geofence, SQLite) + Kotlin/Compose Android UI
via UniFFI. iOS later with SwiftUI over the same core. Apworld in Python. Monorepo: `apworld/`, `core/`, `android/`,
`docs/`. Run an Android build + connect spike for Rust before committing to the stack.

## Sub-projects (each: spec, plan, build)
1. Apworld + client contract + root/Python tooling (spec done)
2. Rust core  3. Android app  4. Rust/Kotlin tooling  5. iOS

## Key decisions
- Own apworld, game name `Archipela-Go 2: Electric Boogaloo`, new ID offset, MIT license.
- Compact location pool `Trip #1..#1000`; attributes in `slot_data` (`schema_version` gated; `type` hook).
- Player-declared travel modes with client-enforced speed bands.
- v1 mechanics: distance reductions, scouting + collection distance, traps, DeathLink, return-home. v1 challenge
  type: `reach_point` only. Backlog: timed run, ordered points, steps, elevation, One Hard Travel.
- Location generation: bulk Overpass tiles + local sampling + cache + endpoint fallback (fixes upstream's hang).
Full table and rationale: `docs/context/project-decisions.md`.

## Conventions
- Conventional commits (`feat:`, `fix:`, `docs:`), subject under 50 chars. Small, tightly scoped steps.
- TDD: failing tests first; never edit tests to fit bad code.
- Upstream code is MIT (keep notice if copying); upstream apworld has NO license: reimplement, never copy.
- Web research: use Perplexity, Firecrawl and GitHub MCPs, not built-in WebSearch/WebFetch.
- Life OS / work-log tagging is intentionally skipped in this project (owner decision).
- apworld package code uses relative imports (Archipelago loads it as `worlds.ap_go2`); tests import `worlds.ap_go2`.
- Parallel agents must not edit this file concurrently (two agents once overwrote each other's index lines).

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
If you need progression design (zones as regions, Bike/Car tools, trail quests, Freeze trap, OSM trail data findings), see `docs/context/progression-zones-and-tools.md`
If you need measured OpenStreetMap tag coverage (surface/lit/parks/trails), the surface-filter design and Fog of War design, see `docs/context/map-data-capabilities.md`
If you need the v1 architecture, verified results, known gaps and how to run the emulator/e2e, see `docs/context/v1-architecture-and-status.md`
If you need the full list of quest kinds (names, map filters, proof, modes), see `docs/context/quest-catalog.md` (generated)
If you need the Android UI design system (palette, theme, shared components, rules) or the Archipelago logo/icon licence finding, see `docs/context/ui-design-system.md`
If you need how scans, the tile grid, the shared cache, pacing/retries and the scan cooldown work, see `docs/context/scan-and-tile-cache.md`
