# Context Doc: Project Decisions and Direction

_Last verified: 2026-10-07_

## Status

Living decision log. Sub-project 1 (apworld + contract + Python tooling) implemented on branch `feat/apworld`; 76 tests green, real Archipelago 0.6.8 generation verified. Next: Rust core spike and spec.

## What This Is

Our own successor to Archipela-Go! (upstream `aki665/react-native-archipelago`, branch `archipela-go`): an Archipelago multiworld game where checks are real-world places reached by walking, biking or driving. Upstream is inspiration only (maintainer ~9 months inactive; app errors out on location generation).

## Decisions (owner-approved unless marked)

| Topic | Decision | Why |
| -- | -- | -- |
| Platforms | Android first, iOS later | Owner has no easy iOS access |
| Stack (lean, not final) | Rust core (AP protocol, location gen, geofence, SQLite) + native Android UI (Kotlin/Compose) via UniFFI; SwiftUI later over same core | Owner asked for most performant, close-to-machine-code, cross-platform; app is network/GPS/battery-bound, so shared testable core + best background-location control. Needs an Android spike (see risks) |
| Apworld | Write our own, clean-room, not upstream-compatible | Upstream apworld has no license, no manifest, logic bugs |
| Game name | `Archipela-Go 2: Electric Boogaloo` (owner typed "Archippela", double p assumed typo; confirm at spec review) | Must not collide with upstream's `Archipela-Go!` |
| Location model | Compact pool `Trip #1..#1000`, attributes in `slot_data` | Tiny datapackage; new axes don't multiply names |
| Travel mode | Player-declared modes (walk/bike/drive), speed-band enforced by client | Owner wants walk/drive; upstream speed never implemented |
| v1 mechanics | Distance reductions, scouting + collection distance, traps (app + honor), DeathLink + return-home | Owner selected all four |
| v1 challenge types | `reach_point` only; `type` field is the extension hook | Keep v1 shippable |
| Backlog | Timed A-to-B run, ordered points, daily steps, elevation gain, One Hard Travel goal | Owner ideas, each its own spec |
| Decomposition | (1) apworld + contract + repo tooling, (2) Rust core, (3) Android app, (4) per-stack tooling, (5) iOS | Each gets its own spec, plan, build |
| First spec | Game contract + apworld | Everything else builds on the contract |
| Tooling | Monorepo; mise, just, uv, ruff, pyright, pytest, lefthook, commitlint, gitleaks, typos, GitHub Actions, release-please, Dependabot; Rust and Kotlin tooling arrive with their sub-projects | Owner: "linting, formatting, best practice enforcement, all the things" |
| Trip count default | 100 trips by default (configurable 1-1000) | Owner: even short trips should feel like a real contribution to the group |
| Key locks | Keys gate whole areas; default 3 locks. Backlog: `key_mode` tiered keys (tier-N key unlocks only tier-N checks) | Owner idea: prevents grabbing most points early |
| Life OS | Intentionally skipped for this project (owner decision) | Global work-log skill expects it; ignore here |

## Key Findings Behind the Decisions

- Upstream generation bug: one Overpass request per candidate point, unbounded recursion, no backoff or HTTP status check, `wait()` never awaits, min/max validation is dead code. Full detail: `docs/context/archipela-go-location-generation.md`.
- Our fix: plan targets locally, fetch by bounding box tiles with one bulk query, cache, sample locally, endpoint fallback + offline path.
- Upstream license MIT (copying code allowed with notice); apworld has no license (reimplement only).
- Other upstream bugs to avoid: key count counts all items, banned-location Overpass clause matches every way, distance reductions counted from wrong item id, saved `apInfo` not refreshed on edit (#18), allsanity wrongly needs all distance reductions.
- Rust AP client: `archipelago_rs` (MIT, v3.0.1, `nex3/archipelago_rs`, reports protocol 0.6.6). Archipelago stable is 0.6.8; 0.7.0 unreleased (as of 2026-10-07). Details: `docs/context/archipelago-client-libraries.md`.
- Overpass etiquette: overpass-api.de limits (<10k queries/day, divided across app users); private.coffee wants notice for large projects; Nominatim 1 req/s and no grid reverse-geocoding.

## Risks and Unverified

- Rust-on-Android via UniFFI with `archipelago_rs` is untested: run a build + connect spike first (TLS `wss` only, `native-tls` off).
- Speed-band constants (walk 0-9, bike 8-35, drive 25+ km/h) are guesses.
- ID offset `8_902_400_000_000` needs a collision check against published worlds.
- crates.io listing and Archipelago main repo license not verified.

## Failed Approaches / Dead Ends

- No standalone upstream apworld repo exists; only release assets.
- Work-log/Life OS skill blocked: `~/.claude/lifeos-reference.md` missing and `mcp__engram__*` unauthenticated. Owner chose to drop Life OS here.

## References

- Spec: `docs/superpowers/specs/2026-10-07-apworld-design.md`
- Upstream: <https://github.com/aki665/react-native-archipelago>
- Related: all other docs in `docs/context/`

## 2026-10-07 UI and goals session (decisions)

| Decision | Why |
| -- | -- |
| Travel mode moves from realm to zone/game; car hidden for now | A place is not a way of moving; the same downtown can be a walk zone and a bike zone |
| Realm editor autosaves with Undo/Redo and a Done button, no Save/Cancel | An X felt like discarding; autosave + visible "All changes saved" + history is safer |
| Finds are favorited/banned per realm (not global) | Owner choice; stored in `marks/<realm>.json`, survives rescans |
| Global 0.02 degree tile grid + shared cache; manual cooldown (20 s/realm), big-area confirm, quiet retries | Reliability against slow public Overpass; overlap costs nothing |
| Lucide icons via `ApgoIcons` (own runner icon); Archipelago logo NOT bundled (CC BY-NC) | Fun look; licence safety |
| Several goals via `goal_selection` OptionSet + `goal_requirement` Choice (Satisfactory pattern); slot_data schema 3 | Follow Archipelago standards; client reports the goal |
| Realm preview = real MapLibre snapshot + live overlay; Home is a separate green card | Distinct, instant, mostly offline |
| Self-hosted Overpass deferred | Owner: not now |
