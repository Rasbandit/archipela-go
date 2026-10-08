# Forager: collect and bank quest (issue #5)

## Goal

A scavenger-hunt quest. The map shows more collectibles than the quest needs. The player walks to some of them to pick them
up, and the quest completes once enough have been brought home. It rewards wandering around the area rather than one trip.

## Decisions (owner, 2026-10-08)

| Topic | Decision |
| --- | --- |
| Where items come from | Points on the walkable street and path network of the zone (not OSM features) |
| Path rule | Every item lies within 30 m of a road or path (same rule as #51 for all quest points) |
| How many | Need N by tier: 3 / 5 / 7 / 10 for tiers 1 to 4. The map shows 2N |
| Banking | Partial: whatever is carried is banked on each arrival home and adds to a saved total |
| Done when | Banked total reaches N |
| Pickup distance | 25 m |
| Modes | Walk, run, bike (not drive) |
| Family | Existing `courier` family: no apworld, YAML or slot_data change |
| Model | Each quest owns its own items (no shared inventory) |
| Shuffle trap | Banked and carried counts are kept; only the unpicked points move |
| Theme | Random per quest (pinecones, shells, mushrooms, acorns, ...), flavour only |

## Catalog

A new kind `forager` in family `courier`, modes walk, run and bike. A new verify type:

`Collect { need_by_tier: [3, 5, 7, 10], spare_factor: 2, pick_r_m: 25 }`

The title names the theme and the count, for example "Forager: bring home 5 pinecones". The theme comes from a short list,
picked with the quest's seeded random number generator. `Kind::is_progressive()` stays false for it.

## Generation (core/src/assign.rs)

A new target `Target::Collect { pts: Vec<Point>, need: u32, r: f64, theme: String }`.

- Place `2 * need` points from the zone's street pool (`atlas.streets` and `streets_rough`), never from the grid fallback.
- Each point is at most 30 m from a street point, at least 60 m from every other item, and inside the zone.
- Spread outward from home: the farthest item sits at about the tier's effort distance (the same effort model as other
  quests), the rest between home and there.
- If the pool cannot supply `2 * need` points under these rules, do not place a forager quest in that slot: fall back to
  another courier kind, as other kinds do when they do not fit.

## Verification and banking (core/src/verify.rs)

A new tracker state `State::Collect { picked: BTreeSet<u16>, carried: u32, banked: u32 }`, saved with the game like
Courier's (serde default so older saves load).

On each accepted fix:

1. Each unpicked item within `r` of the fix is picked: add its index to `picked`, `carried += 1`. One pickup per item.
2. If the fix is within the home radius (the one round trips use), bank: `banked += carried`, `carried = 0`.
3. Done when `banked >= need`.

Progress for the quest row and chain-free views: `min(1, (banked + 0.5 * carried) / need)`.

Same rules as other quests: fixes that are too inaccurate or imply an impossible jump are ignored; nothing counts while
presence has counting off (home Wi-Fi, car); a Freeze trap that blocks checks also blocks pickups and banking.

## Shuffle trap

A Shuffle trap re-places only the unpicked items (new points under the same generation rules). `carried`,
`banked` and `need` are unchanged, and items already picked stay picked. The theme stays the same.

## UI (Android)

- Play map: each unpicked item is a pin with the theme's icon in the courier family colour. Picked items disappear.
- Quest row: "carrying 2 · banked 3 / 5".
- Quest details: the item list with picked or not, and the banked total.
- Icons come from `ApgoIcons` (Lucide), named by meaning; text lives in `ui/HelpText.kt`.

## Archipelago

One location per quest, evaluated on the client, as today. The kind is in the `courier` family, so YAML options,
slot_data (schema 3) and the apworld do not change. Players who enable courier get forager quests mixed in.

## Testing

Core (written first):

- Placement: `2 * need` items, spacing at least 60 m, every item within 30 m of a street point, all in the zone.
- Sparse zone: too few street points means no forager quest is placed (fallback kind), never grid points.
- Pickup: one pickup per item, the 25 m boundary, inaccurate fixes ignored.
- Banking: partial banking over several outings, done exactly when `banked >= need`, nothing banked away from home.
- Counting off and Freeze trap: no pickups or banking.
- Shuffle trap: unpicked items move, `carried` and `banked` kept, picked items do not come back.
- Save round trip, and an old save without the new state loads.

Android: a unit test for the progress text formatter. Device check: a short walk that picks up two items, banks at home,
goes out again and completes the quest.

## Out of scope

A shared inventory across quests, a separate `collect` family or YAML toggle, items tied to real OSM features, and timed
variants.
