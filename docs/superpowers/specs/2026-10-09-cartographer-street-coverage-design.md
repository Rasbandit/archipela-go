# Cartographer by street coverage — design

## Intent

Cartographer asks players to "visit N new map cells". Nobody knows what a map cell is (a 150 m grid square), and the quest
finishes far too early:

- the starting square counts at once;
- squares walked before count again;
- walking along a grid line credits both rows;
- GPS jitter at an edge adds squares without moving.

Owner wants it measured in something people understand: **% of the streets walked**.

Decided with the owner:

- **The area is the zone**: the region the player already set up. No OSM neighbourhood outlines are needed (they are rare: 2 in
  3.5 km around the owner's home, `map-data-capabilities.md`).
- **Each game starts at 0 % walked.** The record is per game, not lifetime.
- **Goals are milestones of the game %** on the existing progressive chain bar ("Walk 5 % of Riverside's streets", then 10 %,
  then 20 %). Everything walked this game counts, including what was walked before the quest appeared.
- **Map:** when a Cartographer quest is selected, streets walked this game in its zone light up in its state colour. Nothing
  changes on the map otherwise.
- **Build on the location-quality work** (`feat/location-quality`, spec `2026-10-08-location-quality-design.md`), agreed with
  that session: it owns the street graph, the locator and map matching; this work consumes them. Building starts after that
  branch merges.

Success:

- A Cartographer quest says how much of the zone to walk and how far that is ("3.1 % walked · about 0.8 km more").
- It cannot be finished by standing still, by re-walking the same street or by GPS jitter.
- Selecting it shows which streets are already walked.

## Dependencies (from `feat/location-quality`, not built here)

| Piece | Used for |
| -- | -- |
| `Atlas::ways` (`WayGeom { id, class, pts }`): walkable highways, simplified to 2 m, junctions shared, access tags honoured | the street network |
| `loc::StreetGraph` (`core/src/loc/graph.rs`): built per game from all its atlases; segments deduped by node pair; `Segment { a, b, len_m, class }`; `candidates(p, radius_m, max, mask) -> Vec<Cand { seg, off_m, d_m }>`; `mode_mask(Mode)`; `geo_at(seg, off_m)` | the total street length, finding the piece under a position, drawing walked pieces |
| `Estimate { lat, lon, uncertainty_m, source, … }` from the `Locator` | what counts as a position |
| HMM matched segment (task 19 there) | preferred over nearest-segment snapping when present |
| `Game::reveal`: accepted and bridged estimates, where fog and Cartographer are credited today | the hook for crediting |

Until that branch merges, do not edit `scan.rs`, `loc/graph.rs`, `loc/locator.rs` or `Game::check` / `Game::reveal`.

## Street pieces and the walked store (core)

- **Piece:** each graph segment is cut into pieces of `PIECE_M = 20` m (the last piece of a segment is shorter). A piece is
  identified by a **location key**: its midpoint rounded to about 1 m (lat/lon × 1e5, as integers). Segment ids change on every
  rescan and every graph rebuild; the key does not.
- **Crediting**, in `Game::reveal`, for each estimate:
  - Skip the estimate when `uncertainty_m > CREDIT_MAX_UNCERTAINTY_M` (20 m).
  - Bridged estimates count (owner decision in the location-quality spec).
  - Piece choice: if the matcher gives a matched segment and offset, use that piece. Otherwise use the nearest candidate within
    `CREDIT_RADIUS_M` (15 m) on any walkable class (`reveal` is not tied to one zone): `candidates(p, 15.0, 1, all_classes)`,
    piece index `floor(off_m / 20)`.
  - Insert `(game_id, key, len_m, class)` into the walked store if it is not already there. A piece is credited once per game.
- **Store:** a new SQLite table `walked_piece(game_id, key_lat, key_lon, len_m, class, PRIMARY KEY(game_id, key_lat, key_lon))`.
  - It lives in the database, not in memory: the graph already uses most of the memory budget.
  - Walked metres for a zone are summed over the keys whose point is inside the zone and whose class is on the zone's mode mask.
- **Zone totals:** `zone_street_m(zone)` is the summed length of the pieces on the zone's mode mask whose midpoint is inside the
  zone. That is the same rule as the walked sum, so the % cannot pass 100. It is computed when the graph is attached and cached
  per game.
- **Walked %** is `walked_m / zone_street_m`, computed when asked (UI, quest check), never on a timer.

## The quest (core)

- **Catalog:** `cartographer` changes from `verify: cover_cells { cells, cell_m }` to `verify: street_share`.
  - It keeps `geom: none` and modes walk, run, bike.
  - It is not offered in a zone with `zone_street_m < MIN_ZONE_STREET_M` (2 km).
  - Regenerate `docs/context/quest-catalog.md`.
- **Target:** `Target::Cells { n, cell_m }` is replaced by `Target::StreetShare { pct }`, where `pct` is this member's own step.
- **Sizing** (in `assign.rs`, replacing the `CoverCells` arm):
  - `distance_m = want_min × mode.m_per_min() × 0.7` (the 0.7 allows for doubling back, as today);
  - `pct = 100 × distance_m / zone_street_m`.
  - Effort stays `want_min`. Description: "Walk X % of <zone>'s streets".
- **Chain:** `ChainUnit::Cells` becomes `ChainUnit::StreetPct`.
  - Milestones are the running total of the members' `pct`.
  - The last milestone is capped at `MAX_SHARE_PCT` (50 %), because some streets are private, dead ends or not worth walking.
  - Members whose milestone would pass the cap are scaled down proportionally.
  - Labels: goal "Walk 5 % of Riverside's streets"; progress "3.1 %". Show one decimal below 10 %, whole numbers from 10 %.
- **Progress check:** the tracker reads the zone's walked % (no per-quest state). Done when the walked % ≥ the milestone.
- **"About X km more":** `(milestone_pct − walked_pct) / 100 × zone_street_m`, rounded to 0.1 km (miles when the units setting
  says so). It is shown while not done.
- **Old saves:** a saved `Target::Cells { n, cell_m }` converts on load to `StreetShare` with the same effort
  (`distance_m = n × cell_m`, then the sizing above). Old per-quest cell progress is dropped; the game % starts from the walked
  store, which is empty for old games.

## Display (Android)

- **Quest card and chain bar:** text from the core, for example "Cartographer · Walk 5 % of Riverside's streets" and
  "3.1 % walked · about 0.8 km more".
- **Walked-street highlight:**
  - On quest selection, Kotlin asks the core for `walked_lines(game, zone)`: walked pieces in the zone, with neighbouring pieces
    joined into polylines.
  - Kotlin sets them on a GeoJSON source drawn by one line layer in the quest's state colour.
  - It is cleared on deselection, fetched again when the selected quest's progress changes, and never polled.
- **Line width:** a new `LineKind::WalkedStreet` in `core/src/line_width.rs` (base 4 px at z16, floor 1.5), drawn with
  `scaledWidth` (zoom expression, GPU side).
- **Colour:** the existing quest state token (`ApgoPalette.quest(state)`). No new literal (`check_color_tokens.sh`).

## FFI

- `walked_pct(zone)` / progress text come through the existing quest and chain records (new fields as needed).
- New `walked_lines(zone_id) -> Vec<Vec<GeoPoint>>`.

## Out of scope

- The **Explorer** win goal ("reveal 300 map cells") keeps grid squares. It is set in the apworld YAML and `slot_data`; moving it
  to street % is a separate issue.
- Fog of war keeps its squares.
- A lifetime (cross-game) coverage stat.
- Per-street progress ("Maple Ave 60 %") and street names (`WayGeom` has none; not needed for the % or the highlight).

## Testing (failing tests first)

- Sizing:
  - a 20 min walk in a 40 km zone is about 2.5 %;
  - a small zone gives a bigger %;
  - the last milestone caps at 50 % and members scale down;
  - a zone under 2 km is not offered Cartographer.
- Crediting:
  - a piece is credited once (the same street walked twice adds nothing);
  - estimates with `uncertainty_m` > 20 m credit nothing;
  - a position 16 m from any street credits nothing;
  - the matched segment wins over the nearest candidate;
  - a bridged estimate credits.
- Stability:
  - walked keys still count after the graph is rebuilt from a rescan (segment ids changed);
  - walked metres only count keys inside the zone.
- Progress: a milestone completes exactly at its %; "km more" is right and hidden when done; chain labels use one decimal below
  10 %.
- Old saves: a `Cells` target converts to `StreetShare` with the same effort.
- Display: `walked_lines` joins neighbouring pieces and returns nothing for an empty store.
- Device: walk a few blocks with the quest selected; the highlight follows the walked streets and the % rises; standing still
  adds nothing.
