# Context Doc: POI Atlas and Server Architecture Options

_Last verified: 2026-10-07_

## Status

Decision proposal (not yet adopted; log in `project-decisions.md` once the owner chooses). Facts come from `poi-data-sources.md` and `map-rendering-and-tiles.md`. Sizes marked **[est]** are my estimates, not measured.

## What This Is

Should Archipela-Go run a server, and how should the app get genuine POIs for "trips"? Compares four architectures and recommends a phased path.

## Core insight

Upstream failed because POI discovery was a live, per-player, per-candidate query against a shared free service. POI data changes slowly, so it should be computed once (CI, monthly) and served as static files. Live traffic is only needed for things that are truly dynamic: submissions, votes, bans, leaderboards.

## Options

| | A: No server, public APIs only | B: Static atlas on CDN (CI-built) | C: B + thin serverless backend | D: Full server |
| --- | --- | --- | --- | --- |
| POI source | Overpass/Wikipedia GeoSearch live | OSM + Overture + Wikidata (+NPS/GNIS) pre-merged, scored, bucketed by H3 | Same as B plus community POIs | Same, plus live queries |
| Monthly cost | $0 | ~$0-5 (R2) **[est]** | ~$0-25 (Workers/D1 free tiers cover small scale) **[est]** | $20-100+ VPS plus ops |
| Privacy | Every query leaks fine location to third parties | Client sends only a coarse cell ID (static file name); no account | Backend sees only submitted data/votes; no raw location needed | Server sees locations unless carefully designed |
| Ops burden | None, but constant breakage | Low: scheduled CI job, versioned release | Low-medium: schema, abuse handling | High: patching, uptime, backups, DDoS |
| Offline | Poor | Excellent (download cells) | Excellent (+ sync later) | Needs client cache anyway |
| Failure modes | 429s, mirror outages, ToU blocks, Google/Mapbox terms | Stale data up to a month; bad POI in source; CI failure (old atlas still served) | Spam/vandalism, free-tier limits, vendor lock-in | Downtime, bills, security incidents |
| Fit | Prototype only; Overpass doc says apps must not rely on public instances | Strong baseline | Needed only for UGC/social features | Overkill now |

## Option B design

- Pipeline (GitHub Actions or a cheap scheduled box, monthly + on demand):
  1. `osmium tags-filter` on Geofabrik/planet PBF for allow-listed tags, export POIs.
  2. DuckDB over Overture Places (filter confidence, taxonomy; pin release path, e.g. `release/2026-09-23.1`).
  3. Wikidata dump or SPARQL subset (P625 + sitelinks) for notability.
  4. Dedupe/merge within ~50 m + fuzzy name; compute `score` (see `poi-data-sources.md`); drop below threshold; stamp `sources` and license flags.
  5. Bucket by H3 and write one compressed file per cell (e.g. `/v3/h3r5/8528...pbf.zst` or Parquet/FlatGeobuf), plus an index and `manifest.json` (version, hash, attribution).
  6. Upload to R2; keep previous version for rollback; app reads `manifest.json` first.
- Cell size: H3 res 5 (~253 km2, ~9.9 km edge) as the download unit; res 7-9 (~5 km2 to 0.1 km2) as client-side sub-index to pick trips. Res 4-5 gives roughly 100 KB-1 MB per cell **[est]** in dense cities. Choose after measuring real counts; split very dense cells to res 6.
- Alternative: one PMTiles file for POIs (vector tile layer) with range requests; simpler hosting, no cell naming, but requires tile decoding. H3 files give cleaner "coarse ID only" privacy and easy caching.
- Deterministic generation: seed + cell + atlas version gives the same candidate POIs for the multiworld seed. Must pin atlas version into the slot data, or generated checks break when the atlas updates. Version is part of the Archipelago seed.
- Library: H3 Java (Apache-2.0); watch pentagons, native `.so` ABIs.
- Alternatives to H3: S2 (exact hierarchy), geohash (string prefix, simple, lopsided cells). H3 chosen for uniform neighbor rings (walking radius rings).

## Privacy

- Client downloads files named by public cell ID; the host sees only the res-5 cell (~10 km) plus IP. Cloudflare logs can be disabled/short-retained.
- Fetch the 7 neighbors too (gridDisk k=1) so the request pattern does not pinpoint the player's cell edge. Prefetch on Wi-Fi.
- Do all distance math on device. No lat/lng ever sent to our backend; submissions send POI coordinates only (not the player's track).
- Do not call Wikipedia GeoSearch with exact coordinates; use atlas-embedded Wikipedia titles and fetch summaries by title.

## Anti-spoofing (high level, Android)

- Per fix: reject `Location.isMock()` (API 31+) / `isFromMockProvider()` (older). Bypassable on rooted devices.
- Plausibility: speed/teleport limits, accuracy thresholds, require several consecutive fixes inside the radius, dwell time, and cell-tower/Wi-Fi sanity (optional).
- Play Integrity: 10k requests/day default quota; attests app/device, **not** location truth. Use only on sensitive actions (claiming leaderboard-eligible checks) and needs a server to verify the token (so Option C).
- Reality: the game is cooperative/Archipelago; strict anti-cheat is low value. Offer an honor-system mode and a "verified" badge tier instead of blocking.

## Option C (when needed)

Cloudflare Workers + D1/KV (or Supabase free tier) **[U: re-check limits]** for: POI submissions (name, coords, category, photo URL), votes, flags/bans, leaderboards. Moderation: community threshold (N independent votes), rate limits, auto-hide on flags. Accepted POIs are folded into the next monthly atlas build (so clients never query the backend for POIs). Anonymous install ID, no PII.

## Recommendation

Build B now, add C later only if community features are wanted; never A beyond the prototype; skip D.

## Phased path

| Phase | Deliver | Exit check |
| --- | --- | --- |
| 0 | Replace per-candidate Overpass with one-off offline extract for 1-2 pilot cities (osmium + Overture via DuckDB), static JSON in repo | Trips generate with no live API |
| 1 | CI pipeline, scoring, H3 cell files on R2, manifest/versioning, attribution file | App downloads cells, works offline |
| 2 | Basemap: OpenFreeMap online, then self-hosted Protomaps region packs | No third-party tile dependency |
| 3 | Optional serverless: submissions/votes/bans/leaderboard, Play Integrity for verified tier | Abuse controls tested |
| 4 | iOS (same atlas, MapLibre iOS, DeviceCheck/App Attest in place of Play Integrity) | Parity |

## Failed Approaches / Dead Ends

- Per-candidate live Overpass (upstream); live Wikipedia GeoSearch per tap with exact coords (privacy + 429).
- Google/Mapbox/HERE/Foursquare APIs as POI source: storage and display restrictions; free caps are tiny.
- Waymarking, Geocaching, HMDB.org: no reusable license.

## Gotchas

- Atlas updates must not silently change an existing seed's locations: pin `atlas_version` in slot data.
- ODbL: publishing the atlas openly satisfies the derivative-database duty; keep per-row `source` so non-OSM rows could be relicensed.
- Overture Places is mostly shops; for "adventure" quality weight OSM tourism/historic/natural and Wikidata-linked items higher.
- R2 Class B reads are cheap, but 1 request per cell per user adds up only at millions of users; ensure `Cache-Control: immutable` on versioned paths.

## References

- `docs/context/poi-data-sources.md`, `docs/context/map-rendering-and-tiles.md`, `docs/context/archipela-go-location-generation.md`
- <https://h3geo.org/docs/core-library/restable/>
- <https://developer.android.com/google/play/integrity/overview>
