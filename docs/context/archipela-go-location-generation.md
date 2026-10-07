# Context Doc: Archipela-Go! Location Generation (upstream algorithm, failure, redesign)

_Last verified: 2026-10-07_

## Status
Upstream 0.7.0 generation is broken/slow (issue #16: 90 min for 20 locations, 4 km radius, dense city). Section "Proposed redesign" is OUR design, not implemented yet. Statements tagged (UNVERIFIED) were not tested live.

## What This Is
How the client turns slot_data trips (`distance_tier`, `key_needed`) into real lat/lon points. Source: upstream `utils/getLocations.ts` (+ caller in `screens/MapScreen.tsx`, commit 125d11f, release 0.7.0).

## Environment
React Native `fetch`; Overpass endpoint hard-coded `https://overpass.private.coffee/api/interpreter`; header `user-agent: archipela-go/0.7.0`. Nominatim code (`lookupApi`, `getOSMTypeAndIdAPI`) still present but unused after 0.7.0.

## Upstream algorithm (exact)
Per trip (sorted by `key_needed`; `theta` re-rolled when key group changes), `MapScreen.getCoordinatesForLocations` loops `getLocations` up to `LOCATION_RETRIES+1` times until lat/lon is not already used:
1. `maxDist = maximum_distance/10 * distance_tier`; `minDist = minimum_distance`; `correction` tweaks (see dead code below). If `maxDist < min` -> `min*1.1`.
2. `theta = calculateTheta(MIN_RADIAN, MAX_RADIAN)` (user angle sector; circular-slider radians are flipped).
3. `r = (max-min)*sqrt(rand)+min` (NOT area-uniform; correct is `sqrt(min^2 + u*(max^2-min^2))`). Offset by `dx=r cosθ, dy=r sinθ` using 111.19 km/deg and cos(lat) for lon.
4. `fetchOverpassInfo`: POST `data=` OQL: `way(around:200,lat,lon)->.a;` union of 18 highway/tracktype filters (residential, living_street, pedestrian, track, footway, bridleway, steps, cycleway, service, grade1-3 tracks, secondary/tertiary with urban/low maxspeed) + banned-location clauses; `>; out skel;` (returns the ways' nodes, no tags).
5. Take `elements[0]` (first node, arbitrary among all nodes of all matched ways; can be hundreds of metres from the target on a long way). osmID = `"N"+id`. Distance recomputed with haversine from home.
6. Empty result / any exception -> `{osmID:"0", lat:0, lon:0}`; caller recursion retries immediately.
So: ONE Overpass request per candidate point, >= 1 per trip, plus retries.

## Why it fails (root causes, file refs)
- `getLocations.ts` `wait()` is `setTimeout(...)` inside an async fn without awaiting a Promise: returns immediately. No delay ever happens (`await wait(125)` is a no-op).
- No HTTP status check: on 429/504 the body is HTML/text -> `res.json()` throws -> catch -> osmID "0" -> immediate retry. Unbounded tight retry loop against a busy server = request storm (matches #16; private.coffee turbo returned `Dispatcher_Client::request_read_and_idx::timeout` on 2026-06-06).
- Overpass can also answer HTTP 200 with a `remark` ("runtime error: ...") and empty `elements` (UNVERIFIED here, known Overpass behavior): treated as "no road here" -> retry.
- Recursion: `if (res.osmID==="0") res = await getLocationCoordinates(...same loop_count...)` never increments `loop_count` -> no bound, unbounded promise-chain depth.
- Dead code: the min/max distance validation requires `loop_count > 5`, but `loop_count` starts at 0 and only increments inside that same branch, so it never runs. The `correction` logic and the `1000 * 1 + DISTANCE_LENIENCY` precedence bug are unreachable. Result: distance bounds are never enforced after snapping; only the random target obeys them.
- Banned locations clause `way.a["id"!="N"]` matches all ways (no `id` tag) -> broadens union to every way near the point (buildings fences etc.), degrading quality (see architecture doc).
- No `fetch` timeout/AbortController (RN fetch has none by default) -> can hang on a dead socket.
- Single endpoint, no fallback, no caching: re-roll and regeneration re-query from scratch. `zoom`/`NEAR_ZOOM` params are vestigial (leftover from Nominatim). Heavy per-point `console.log`.
- `forEach(async)` re-gen of osmID "0" trips in MapScreen not awaited before saving.
- Maintainer note (#14 comment): attempting an all-nodes query for the full max radius failed ("maximum distance in the apworld is so large"), so they kept per-point calls. The fix is tiling, not one giant query.

## Proposed redesign (bulk Overpass, local sampling)
Goal: <= ~5-15 requests per generation instead of hundreds; works with partial failure; resumable; offline after first generation.
1. Plan targets locally first: for each trip choose `(r, θ)` (seeded PRNG stored per session; r area-uniform in `[min, effMax(tier)]`, θ within user sector). Map target -> tile.
2. Tile grid: fixed lat/lon grid (e.g. 0.02 deg ~2.2 km; size tunable) keyed `z/x/y`-style string. Fetch only tiles that contain >=1 target (or its 1-ring neighbours as fallback), not the whole disc. For a 5 km max radius that is ~at most 20-80 tiles but typically far fewer needed; sequential, 1 in flight.
3. Per-tile query (bbox is much cheaper than huge `around`):
   `[out:json][timeout:25][maxsize:20000000]; way["highway"~"^(residential|living_street|pedestrian|track|footway|bridleway|steps|cycleway|service|path|unclassified|tertiary|secondary)$"]["access"!~"^(private|no)$"]["foot"!="no"]({s},{w},{n},{e}); out geom qt;` (use `out center` for a tiny-payload mode; geom gives vertices to snap to; exclude motorway/trunk/primary for safety). Annulus alternative (Overpass has no annulus primitive): `( way(around:Rout,lat,lon)[F]; - way(around:Rin,lat,lon)[F]; ); out geom qt;` drops ways crossing the inner circle and is slower than bbox; use only as one-shot "overview" for small R.
4. Local sampling: from cached tile ways build candidate vertices (way id, node index, lat, lon). For each target pick nearest candidate satisfying: |dist(home,c) - r| <= tol, `min <= dist <= effMax`, not banned (store banned as way/node ids and filter locally -- no OQL clauses), not within 2*markerRadius of another chosen point (replaces exact-equality duplicate check), optionally cap per way unless "multiple per road" enabled. Widen tol / pick neighbour tile if none; after N failed passes accept nearest and flag `degraded`.
5. Cache: tiles in AsyncStorage/MMKV/SQLite (key `ov:{tileKey}:{filterVersion}`, store compact arrays, TTL ~30-90 days; roads change slowly). Cache the chosen trip list per session (already upstream). Re-rolls reuse cached tiles = zero extra requests.
6. HTTP client wrapper: `AbortController` timeout 30 s; require `res.ok` and `content-type` json; reject JSON with `remark` containing "runtime error"/"timed out" as failure; classify: 429/406 -> pause >=30 s (OSM wiki guidance) honoring `Retry-After` if present; 504/timeout -> halve tile size / split query and rotate endpoint; network error -> offline path. Backoff exponential with jitter (2,4,8..60 s), max attempts per tile ~4 per endpoint, global circuit breaker, visible progress UI with "retry later". Never retry in a tight loop; `await new Promise(r => setTimeout(r, ms))`.
7. Endpoint fallback list (configurable in settings + remotely updatable; don't hard-code, per Nominatim-style "must be switchable without update" ethos): `https://overpass.private.coffee/api/interpreter` (primary), `https://overpass-api.de/api/interpreter` (main, strict fair-use), `https://maps.mail.ru/osm/tools/overpass/api/interpreter` (listed on OSM wiki, reliability UNVERIFIED), optional user-supplied URL (issue #14 idea). Regional/keyed instances (Geofabrik, nextgis, mapsource, ...) are paid/keyed: out of scope. Health-check via `GET {base}/status` before use (overpass-api.de style; availability on other hosts UNVERIFIED).
8. Offline behavior: generation requires network once per uncached tile; if offline, keep already-chosen trips, mark remaining as "pending", retry on connectivity (NetInfo) or manual refresh; checks/geofencing never need Overpass. Optional "predownload region" button. Sending checks queued locally and flushed on reconnect.
9. Fallback of last resort: if all endpoints fail, offer geometric point (no road snapping) clearly flagged "unverified", or "try later"; never place at 0,0 (upstream has multiple guards for this bug).
10. Quality filters: unlit/private/gated (`access`), `foot=no`, exclude `highway=service` + `service~"driveway|parking_aisle"` optionally, minimum way length, prefer ways with `surface`; avoid water by using ways only (no raw node geometry).

## Rate limits / usage policy (verified by scraping 2026-10-07)
- OSM wiki Overpass_API page, public instances table:
  - overpass-api.de (FOSSGIS): fair use < 10,000 queries/day AND < 1 GB/day AND < 10 min total processing per day; for regular/app use divide by 100 (< 100 queries, < 10 MB/day); an app's usage is the SUM over all its users. Pause 30 s on HTTP 429/406. Do not deploy via instant-AI-app platforms.
  - overpass.private.coffee (formerly overpass.kumi.systems): "no rate limit in place", asks to be notified in advance for large-scale projects; contact support@private.coffee. Upstream maintainer cited "notify us if over ten requests a second". It was timing out for a user in June 2026 (#16).
  - Per general Overpass docs (via search, medium confidence): per-IP slots (e.g. 2), `/api/status` shows slots, request queues up to ~15 s then 429; 504 = query too heavy or server overloaded -> shrink query, don't hammer.
- Nominatim (operations.osmfoundation.org/policies/nominatim): max 1 req/s absolute; valid User-Agent/Referer identifying the app; display attribution; apps must be able to switch service without a software update; cache results; periodic app requests = bulk geocoding, strongly discouraged; "systematic queries" incl. reverse geocoding in a grid are banned. Reverse geocoding returns only named/indexed roads (bad for rural, #14). Recommendation: do NOT use Nominatim for generation; at most for one-off user-typed home-address search.
- Data license ODbL: show "© OpenStreetMap contributors" in app (map screen + about). Share-alike applies to derived databases; cached tiles are fine for in-app use.
- Set a real UA with contact: `ArchipelaGoClone/<ver> (contact url/email)`; upstream uses `archipela-go/0.7.0`.

## Failed Approaches / Dead Ends
- Nominatim reverse geocode per point (pre-0.7): misses unnamed paths (#14), 1 req/s.
- OSRM `nearest` (#14 reporter): inconsistent snapping, "off by miles".
- Single huge `around`/bbox for max radius (maintainer, #14): fails/timeouts; use tiles.
- Per-point Overpass (0.7.0): this doc's root causes.

## Gotchas
- `out skel` returns nodes without way context; use `out geom` to keep way ids for ban lists and per-road limits.
- Overpass `around` on large radius with union filters is expensive; bbox + tag filter is cheap and cacheable.
- Lat/lon math: use cos(lat) for lon; at high lat/`>50 km` prefer geodesic (haversine) distance checks.
- #14 reporter's note: Google map style hides small tracks at low zoom; draw our own polylines or use OSM-based tiles if showing paths.

## References
- Upstream: `utils/getLocations.ts`, `screens/MapScreen.tsx` (branch `archipela-go`), issues #14 (closed), #16 (open)
- https://wiki.openstreetmap.org/wiki/Overpass_API ; https://operations.osmfoundation.org/policies/nominatim/
- Related: `docs/context/archipela-go-game-design.md`, `docs/context/archipela-go-upstream-architecture.md`
