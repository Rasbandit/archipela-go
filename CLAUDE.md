# Archipela-Go 2

Our own successor to Archipela-Go! (upstream `aki665/react-native-archipelago`, stale): an Archipelago multiworld game
where checks are real-world places reached by walking, biking or driving. Android first, iOS later. Upstream is
inspiration only; we write our own apworld and client.

## Status
Sub-project 1 (apworld + contract + Python tooling) is implemented on branch `feat/apworld`: spec
`docs/superpowers/specs/2026-10-07-apworld-design.md`, plan `docs/superpowers/plans/2026-10-07-apworld.md`,
contract `apworld/docs/contract.md`. Next: Rust core spike and spec. Never work on main.

## Commands
`just setup` (env, pinned Archipelago in `.ap/`, hooks), `just check` (lint, types, tests, typos), `just build`
(`dist/ap_go2.apworld`). `mise.toml` pins the tools; `committed` and `gitleaks` need installing locally.

## Planned stack (lean, not final until spiked)
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
