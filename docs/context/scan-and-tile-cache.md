# Scanning, the tile grid and the shared cache

## Why scans were slow and flaky

The app asks the free public Overpass servers for map data. They are shared and slow (5 to 60 s per request, plus 504/"busy" answers). Old
scans cut tiles relative to each realm's bounding box, so no two realms ever shared a tile and moving a realm re-fetched everything. A
request that missed the budget showed a scary warning.

## How it works now (core/src/tilegrid.rs, scan.rs, overpass.rs)

- **Fixed global grid**: 0.02 degree cells (about 2.2 x 1.5 km). A tile's three queries (places, streets, trail/park geometry) depend only on
  its cell, and the on-disk query cache (`http-cache/q-<hash>.json`, 30 days) is keyed by query text. So overlapping realms share tiles and
  moving or resizing a realm fetches only the cells it newly touches (measured: 1 download out of 12 after a nudge).
- **Only cells the shape touches** (`tiles_for`), not its bounding box.
- **`plan()`** says how many tiles/requests a scan needs and how many are already cached (`overpass::is_cached`); the app uses it for the
  cooldown and the "big area" confirmation.
- **Pacing and retries** (`Pacing`): 2 workers, a 250 ms gap after each request, failed requests retried up to 3 rounds with growing waits,
  a 240 s budget. Pieces that never arrive give a partial atlas with a note (`warnings`), never an error; only "nothing arrived" fails.
- **Progress**: `Engine::scan_progress()` (done/total) is polled by the app while a scan runs.
- The realm's `atlas/<id>.json` is a derived per-realm index built from the shared tiles; the raw data is stored once. Reads always
  restrict it to the realm's current shape (`Atlas::restrict_to`).
- `Engine::scan_realm` re-reads the realm before stamping the scan time: a realm edited during a scan (autosave) is not overwritten.

## Trail stitching (scan.rs)

- `stitch` joins ways whose ends are within `JOIN_M` (8 m) and drops the joined way's first point; `link` joins chains whose ends are
  within `LINK_M` (30 m) and keeps every point. So a way glued in by `stitch` may keep only its far vertices in the chain.
- `chain_ways` therefore counts a way as part of a chain when it owns a chain edge OR a vertex no other way has. A branch that only
  touches the trail at a junction vertex owns neither and is not counted.
- Synthetic `L:` trail features carry the merged way tags but deliberately no `highway` tag, because `highway` feeds `is_rough`.

## Place details (PR #136; UI in the quest card spec)

- `Assignment.feature_id` links a quest to its atlas feature. `QuestOut.has_details` is only `feature_id.is_some()`, so building the
  quest list never reads the atlas.
- `Engine::poi_details` reads the atlas JSON lazily with the game lock released: call it off the main thread.
- Games saved before `feature_id` get it back in `Engine::refresh_streets`: `Game::relink_places` uses `assign::place_of`, which
  picks a find of the same kind (any find for the boss) with the same name, nearest the quest (every point of a line quest),
  within 100 m; an unnamed find only stands in when the quest's place is the kind's own name. It reuses the atlases loaded for
  the street build and saves once.
- Atlases scanned before the trail tag merge have trails with no merged tags (surface, lit, difficulty) until the realm is
  scanned again.

## App rules (AppModel.scan)

- At most one network scan per realm every 20 s; a request inside the window is deferred to the end of it, not dropped.
- A scan needing 60+ downloads asks first ("A big area").
- Failures and partial scans retry quietly (45 s apart, 4 times); the player only sees a gentle message, never a raw error.
- Editor triggers: opening Details after the outline changed (or on a never-scanned realm) and leaving the editor after an outline change.

## Not done

- A self-hosted Overpass (Docker on FastRaid) as the primary source is the real reliability fix; the app needs a configurable server address.
- Tiles are refreshed only when their cache file is older than 30 days; there is no manual refresh button.
- `scripts/e2e_emulator.sh` still waits for "Scan done" / taps old labels and is stale.
