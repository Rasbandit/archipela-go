# Apworld + Repo Tooling Design (Sub-project 1)

Date: 2026-10-07 · Status: implemented (see "Implementation notes")

## Context

We are building our own successor to Archipela-Go! (upstream: `aki665/react-native-archipelago`), an Archipelago
multiworld game where checks are real-world places you travel to. Upstream is stale (maintainer inactive ~9 months),
its location generation is broken (see `docs/context/archipela-go-location-generation.md`), and its apworld has no
license, no manifest, and known logic bugs (`docs/context/archipela-go-game-design.md`).

Product direction (decided with the owner):
- Android first, iOS later (not testable by owner yet). Rust core + native UI is the intended client stack and is
  specced in later sub-projects; it is **not** decided here.
- Our **own apworld**, clean-room, not compatible with upstream's `slot_data`.
- Decomposition: (1) apworld + contract + repo tooling **(this spec)**, (2) Rust core, (3) Android app,
  (4) per-stack tooling (arrives with 2 and 3), (5) iOS.

## Goals

1. A licensed, tested, 0.6.x- and 0.7-compatible apworld that generates valid seeds.
2. A versioned client contract (`slot_data` + item semantics) the Rust core can build against.
3. A repo scaffold that enforces quality from the first commit (lint, format, types, tests, CI, commit rules).

## Non-goals (v1)

- Any client code (Rust core, Android).
- Challenge types other than `reach_point` (backlog below).
- One Hard Travel goal (never implemented upstream).
- Compatibility with upstream's apworld or saved data.

## 1. Identity and packaging

- Game name: `Archipela-Go 2: Electric Boogaloo`. The exact string is locked at spec review (owner typed
  "Archippela-Go 2 electric Boogaloo"; the double "p" is assumed to be a typo). It is baked into datapackages
  and YAMLs, so it is cheap to change only before first release.
- ID offset: `8_902_400_000_000` (items and locations are separate namespaces). Implementation must verify no
  collision with other published worlds before release.
- Package `apworld/ap_go2/` with `archipelago.json` manifest (fixes upstream issue #17) and the 0.6.x layout.
- License: MIT. Third-party attributions in `THIRD_PARTY_NOTICES.md`.
- Upstream apworld source is **not** copied (no license). Ideas (regions gated by keys, distance reductions) are
  reimplemented from our own definitions below.

## 2. Locations (compact pool)

- Fixed pool `Trip #1 .. Trip #1000`. `number_of_trips` (1-1000) selects how many are active per seed.
- Datapackage stays tiny and new axes (mode, type) never multiply it.
- Per-seed attributes live in `slot_data`, not names. Hints in generic text clients read `Trip #17`; the app shows
  the full detail. This is an accepted trade-off.
- Regions: `Menu -> Area 0 -> ... -> Area N`. Entrance `Area i-1 -> Area i` requires Progressive Key count >= i.
  A trip sits in region `Area {key_needed}`. `N = number_of_locks` (0-10, clamped to `trips // 2`).
- Every key tier 0..N and every distance tier 1..10 (capped by trip count) gets at least one trip.

## 3. slot_data v1 (client contract)

```json
{
  "schema_version": 1,
  "goal": "all_trips | macguffin_short | macguffin_long",
  "min_distance_m": 500,
  "max_distance_m": 5000,
  "allowed_modes": ["walk", "bike", "drive"],
  "return_home": false,
  "death_link": false,
  "reduction_percent": 8,
  "tier_step_m": 758.6,
  "trips": [
    { "location_id": 8902400000001, "type": "reach_point",
      "distance_tier": 3, "key_needed": 1, "mode": "walk" }
  ]
}
```

- `type` is the extension hook for future challenge types. Clients must refuse seeds with an unknown
  `schema_version` and ignore (display as unsupported) trips with an unknown `type`.
- `tier_step_m` is meters per distance tier: base trip distance is `distance_tier * tier_step_m`, effective
  distance is that times `(1 - reduction_percent/100) ** reductions_received`.
- Coordinates are never in `slot_data`; the client derives real-world points per trip (see the location-generation
  context doc). Distances are meters.
- Speed bands per mode are **constants in the contract doc** (not per-seed), versioned with `schema_version`:
  initial values walk 0-9 km/h, bike 8-35 km/h, drive 25+ km/h (to be tuned during client work; changing them bumps
  the version).
- `apworld/docs/contract.md` is the normative description; the Rust core is built against it.

## 4. Items

| Item | Class | Notes |
|--|--|--|
| Progressive Key | progression | gates Areas |
| Progressive Distance Reduction | progression | optional, see Logic |
| Progressive Scouting Distance | useful | optional; client reveals nearby locations' contents |
| Progressive Collection Distance | useful | optional; client enlarges check radius |
| Traps: Shuffle, Silence, Fog Of War | trap | app traps, client-implemented |
| Traps: Push Up, Socializing, Sit Up, Jumping Jack, Touch Grass | trap | honor system, notification only |
| Fillers: Hydrate!, Take a Breather! | filler | honor system |
| Letter A, R, C, H, I, P, E, L, G, O | progression | macguffin goal; short = APGO, long = ARCHIPELAGO |

- Creation order: goal letters, `number_of_locks` keys, traps up to `(trips - items) * trap_rate / 100`, then
  filler from the enabled pools. Reductions in the pool = `min(max(5, floor(0.15 * trips)), free slots)` (free slots = trips minus letters
  and keys); the tier step uses this actual count so logic always holds.
- Letter items use duplicate counts where needed (long set has two `Letter A`).

## 5. Logic and distance reductions

- Reduction multiplier per item: `(1 - reduction_percent/100)`.
- Tier base distance `d_t = t * R` where `R = max_distance * (1/(1-p))^E / 10`, `p = reduction_percent/100`,
  `E` = expected reductions (the cap from section 4).
- A trip with `d_t > max_distance` requires `ceil(log(max/d_t) / log(1-p))` reductions to be reachable in logic.
- With reductions disabled, `R = max_distance / 10` and no trip needs any.
- Speed/mode is **not** logic-gating in v1 (client-enforced only).

## 6. Goals

- `all_trips`: victory location requires all active trips (not distance reductions; this fixes upstream's
  copy/paste bug).
- `macguffin_short` / `macguffin_long`: victory requires all letters of the chosen set.
- Victory event item locked on location `Goal`, which sits in `Menu` with a per-goal access rule (all keys and
  max reductions for `all_trips`, all letters for macguffin goals).

## 7. Options

Kept: `goal`, `number_of_trips`, `minimum_distance`, `maximum_distance` (named ranges: 2k, 5k, 10k, half_marathon,
marathon, 50k, 100k), `number_of_locks`, `trap_rate`, `enable_distance_reductions`,
`enable_scouting_distance_bonuses`, `enable_collection_distance_bonuses`, `death_link`.
New: `allowed_modes` (OptionSet; at least one required), `return_home` (Toggle), `reduction_percent` (Range 1-25,
default 8).
Ranges: `number_of_trips` 1-1000 (default **100**, so even short-range games feel substantial),
`minimum_distance` 100-5000 m, `maximum_distance` 1000-100000 m, `number_of_locks` 0-10 (default 3).
Validation (`OptionError`): `maximum_distance > minimum_distance`; at least one mode; `number_of_trips` at
least goal letters + locks (macguffin_long needs 11 + locks); locks clamped to `trips // 2`.

## 8. Repo and tooling (monorepo)

Layout: `apworld/` (Python), `core/` (Rust workspace, later), `android/` (Kotlin, later), `docs/`.

Delivered with this spec (root + Python):
- `mise.toml` pinning Python (and later Rust, JDK); `justfile` with `check`, `test`, `lint`, `fmt`, `build`.
- Python: `uv`, `ruff` (lint + format, strict ruleset), `pyright` strict, `pytest` with Archipelago `WorldTestBase`.
- Repo-wide: `lefthook` (pre-commit: format/lint; pre-push: tests), `committed` (conventional commits; a single binary, chosen over `commitlint` to avoid a Node toolchain),
  `gitleaks`, `typos`, `.editorconfig`, markdownlint.
- GitHub Actions: per-path jobs (apworld first), required checks on PRs, build of the `.apworld` artifact,
  Dependabot.
- `release-please` for SemVer and changelog from conventional commits.
- Governance: `LICENSE` (MIT), `CONTRIBUTING.md`, `SECURITY.md`, PR template, `THIRD_PARTY_NOTICES.md`.

Delivered with later sub-projects: Rust (`rustfmt`, `clippy -D warnings`, `cargo-nextest`, `cargo-deny`),
Kotlin (`ktlint`, `detekt`, Android Lint, Gradle version catalogs), signed Android release builds.

## Testing strategy

- `WorldTestBase` suites: default seed beatable for each goal; reductions on/off; locks 0, 1, 10; trips 1, 2, 1000;
  every mode subset; trap_rate 0 and 100; min/max validation errors.
- Unit tests for the reduction math (tier distance, reductions needed, monotonicity, no trip unreachable).
- Contract test: generated `slot_data` validates against a JSON Schema checked into `apworld/docs/`.
- Edge cases: empty pools, maximum distance equal to minimum (rejected), one mode only.

## Backlog (out of v1, each its own spec)

`key_mode` option (`tiered`: a separate key per tier, tier-N key unlocks only tier-N checks, so players cannot
collect most points early), timed run (A to B within X minutes, difficulty-scaled), ordered-point sequences, daily step counter challenge,
elevation-gain challenge, One Hard Travel goal, per-mode separate trip pools, enforced speed bands in logic.

## Implementation notes

- Package code uses **relative imports**: Archipelago loads worlds as `worlds.<name>` and from `.apworld` zips.
  Tests import via `worlds.ap_go2` against a pinned Archipelago checkout in `.ap/` (`just setup-ap`).
- `WorldTestBase.collect_all_but` also collects the pre-placed Victory event, so goal tests assert
  reachability of the `Goal` location on a fresh `CollectionState`.
- Verified: `just check` (76 tests incl. Archipelago default fill/reachability tests), `just build`, and a real
  `Generate.py` run with a 100-trip YAML on Archipelago 0.6.8.

## Open items for review

1. Final game-name spelling (assumed single "p").
2. Speed-band constants (initial guesses, tuned with the client).
3. ID-offset collision check against published worlds before first release.
4. Local `gitleaks` pre-commit hook pending a sudo install; CI runs it regardless.
