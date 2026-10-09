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
| `Estimate { lat, lon, uncertainty_m, source, accepted, … }` from the `Locator` | what counts as a position |
| `Game::reveal`: accepted and bridged estimates, where fog and Cartographer are credited today | the hook for crediting |
| `way_class::SIDE` (requested from that session): set on sidewalks and crossings | keeping sidewalks out of coverage |

Until that branch merges, do not edit `scan.rs`, `loc/graph.rs`, `loc/locator.rs` or `Game::check` / `Game::reveal`. That
branch is based on the local `main` (forager work), not `origin/main`; re-check every signature in the plan against main after
both merge.

There is no matched position for quests: the location-quality spec has quests use the IMM estimate, never the matched point. So
coverage snaps to the nearest segment itself.

## Street pieces and the walked store (core)

- **Piece:** each graph segment is cut into pieces of `PIECE_M = 20` m (the last piece of a segment is shorter). A piece is
  identified by a **location key**: its midpoint rounded to about 1 m (lat/lon × 1e5, as integers). Segment ids change on every
  rescan and every graph rebuild; the key does not.
- **Crediting**, in `Game::reveal`, for each estimate. Today `reveal(pos, ev)` gets only a point; change it to also get
  `uncertainty_m` (both callers have the `Estimate`). This is ours to change after the location-quality merge.
  - Skip the estimate when `uncertainty_m > CREDIT_MAX_UNCERTAINTY_M` (20 m).
  - Bridged estimates count (owner decision in the location-quality spec).
  - Piece choice: the nearest segment without the `SIDE` bit within `CREDIT_RADIUS_M` (15 m), on any walkable class (`reveal`
    is not tied to one zone): `candidates(p, 15.0, 4, FOOT | BIKE | CAR)`, first one without `SIDE`. The piece index is
    `floor(off_m / 20)`.
  - Add `(key, len_m, class)` to the game's walked set if it is not already there. A piece is credited once per game.
- **Store:** the walked set lives in the saved game (`Game.walked`, `#[serde(default)]`), because games are saved as JSON files
  and the only database is the journal. In memory it is `BTreeMap<(i32, i32), (f32, u8)>` (key → length, class). It is saved as a
  flat list of about 20 bytes per piece: 3,000 walked pieces (60 km) is about 60 KB.
- **Zone totals:** `zone_street_m(zone)` is the summed length of the pieces on the zone's mode mask, without `SIDE`, whose
  midpoint is inside the zone. It is computed lazily on first use and cached in memory (`#[serde(skip)]`) together with the graph
  it came from (`Arc` pointer). The graph is rebuilt off the main thread on open, rescan and realm edit (`refresh_streets`), so a
  different graph throws the cache away.
- **Walked metres** for a zone sum the walked keys whose point is inside the zone and whose class is on the zone's mode mask: the
  same rule as the total, so the % cannot pass 100. It is computed when asked (UI, quest check), never on a timer.
- **Sidewalks:** sidewalks and crossings are mapped as their own ways next to streets. Counting them would triple a street's
  share and leave walkers with about a third of the credit, so pieces with `SIDE` are left out of both the total and crediting.

## The quest (core)

- **Catalog:** `cartographer` changes from `verify: cover_cells { cells, cell_m }` to `verify: street_share`.
  - It keeps `geom: none` and modes walk, run, bike.
  - It is not offered in a zone with `zone_street_m < MIN_ZONE_STREET_M` (2 km).
  - Regenerate `docs/context/quest-catalog.md`.
- **Target:** a new `Target::Streets { distance_m }`, this member's own step as a **distance**. A % needs the zone's street total,
  which is only known once the graph is built when the game opens, so the % is derived when shown.
  - `Target::Cells` stays in the enum so old saves load (`Target` is saved with serde's default tagging), and is converted on
    load (below).
- **Sizing** (in `assign.rs`, replacing the `CoverCells` arm): `distance_m = want_min × mode.m_per_min() × 0.7` (the 0.7 allows
  for doubling back, as today). Effort stays `want_min`.
  - A zone whose atlas `walkable_m()` is under `MIN_ZONE_STREET_M` (2 km) does not get Cartographer: the arm returns `None`.
    (`min_features` does not gate assignment, and the graph is not built yet when quests are assigned.)
- **Chain:** `ChainUnit::Cells` becomes `ChainUnit::Streets`. Its counter and marks are in **metres**; the UI shows them as %.
  - The counter is the zone's walked metres, read live (like steps), not added up in `counters.progress`.
  - Marks are the running total of the members' `distance_m`, capped so the last mark is at most `MAX_SHARE` (50 %) of
    `zone_street_m`. If the last would pass it, every mark is scaled by `cap / last`. Some streets are private, dead ends or not
    worth walking.
  - `zone_street_m` is only known after the graph is attached. Until then (or with no graph), the chain shows its marks as
    distances and cannot complete.
  - Text from the core:
    - goal: "Walk 5% of <realm name>'s streets";
    - amount: "3.1%" (one decimal below 10 %, whole numbers from 10 %);
    - next: "about 0.8 km more", which is `mark − walked` metres, formatted on the phone (`Units.distance`, which already picks
      km or miles by locale).
- **Old saves** (in `normalize_counters`, which runs on load):
  - Every `Target::Cells { n, cell_m }` becomes `Target::Streets { distance_m: n × cell_m }`. That is the same distance it was
    sized from.
  - The old `counters.progress` entry for each converted chain is removed: it counted squares, and its floor at the highest done
    mark would be read as metres.
  - Marks already done stay done.

## Display (Android)

- **Quest card and chain bar:** text from the core, for example "Cartographer · Walk 5 % of Riverside's streets" and
  "3.1 % walked · about 0.8 km more".
- **Walked-street highlight:**
  - Cartographer is a chain, so "selected" means the chain is selected (`AppModel.selectedChain`). Today that is not passed to
    `QuestMap`; it has to be.
  - On chain selection, Kotlin asks the core for `walked_lines(zone)`: walked pieces in the zone, with neighbouring pieces
    joined into polylines.
  - Kotlin sets them on a GeoJSON source drawn by one line layer in the quest's state colour.
  - It is cleared on deselection, fetched again when the selected quest's progress changes, and never polled.
- **Line width:** a new `LineKind::WalkedStreet` in `core/src/line_width.rs` (base 4 px at z16, floor 1.5), drawn with
  `scaledWidth` (zoom expression, GPU side).
- **Colour:** the existing quest state token (`ApgoPalette.quest(state)`). No new literal (`check_color_tokens.sh`).

## FFI

- `ChainOut.unit` gets `"streets"`. The counter and marks are in metres, and a new `ChainOut.total_street_m` (0 when unknown)
  lets the phone show %. The phone replaces the `"cells"` cases (`ChainFormat`, `PlayLayout.OFF_MAP`, `DevSimulator`).
- New `Engine::walked_lines(zone: u32) -> Vec<Vec<GeoPoint>>`.

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
