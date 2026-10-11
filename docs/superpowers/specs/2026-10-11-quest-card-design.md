# Quest card — design

Supersedes the ⓘ button and separate details card of `2026-10-11-poi-details-design.md`. The core parts of that spec
(feature link, relink, trail tag merge, tag → field mapping) stay. Mock agreed with the owner, iterated in the Quest Card Lab
artifact: <https://claude.ai/artifact/KF1TE2MuULmkm9Wre2SCVx> (version 23).

## Intent

The Play popup must say, at a glance, **what to do**, **where**, and **how far along** you are. The map's facts about
the place come second and stay out of the way until asked for.

Owner rules from the review:

- The requirement is the headline, not the quest's name. A headline is **one line** (cut with … if ever too long).
- Measurements go on one grey detail line under the headline, never in the headline.
- Colour means **quest state**, as on the map: blue open, amber in progress, green done, grey locked. Quest type is never a
  colour; it is the icon and the quest name.
- Map facts fold out per place ("About this place") and only exist when there is something to show.
- No OpenStreetMap link. A Wikipedia link when the place has one.
- Words the player can't see on the map are not used: no "squares", "cells" or "blocks".

## Card layout (top to bottom)

```text
[■]  Walk half the trail                       ✕     ← badge (state colour, white kind icon) + headline (h2, 19sp, one line)
     Trail Boss                                       ← quest name (h3, 14sp, muted), centred with the headline on the badge
medium · ~18 min · walk                               ← difficulty pill, time, mode icon + word; full width under the badge
1.4 mi of 2.8 mi · either direction                   ← detail line (one line, muted)
Sensei Trail · trail                                  ← place row: bold name · light type; full width (quests on a place only)
Traveled                          0.6 of 1.4 mi       ← progress block (quests that measure something; always shown)
▓▓▓▓▓▓░░░░░░░░░
▾ About this place                                    ← only when the place has facts, a description or Wikipedia
```

- **Badge:** 40dp rounded square, fill = state colour (`ApgoPalette.quest(state)`), white kind icon (`ApgoIcons.forKind`).
  Done adds a small check in the corner. In progress uses dark ink on the amber fill.
- **Close (✕):** pinned to the card's top-right corner (6dp inset), not in the header row.
- **Place row:** names that are titles of works (artworks, murals, memorials) are in quotes: **"Allow Me"** · artwork.
  No name: the type alone (bench).
- Colours are palette tokens only (new ones added to `ApgoPalette`); Kotlin draws, the core writes every word.

## Headline and detail line, per quest kind

| Verify type | Headline | Detail line |
| -- | -- | -- |
| reach (on a place) | Get here | Within 130 ft |
| dwell | Sit 3 min (bench, picnic, shelter) / Stay 3 min (others) | Within 130 ft · leaving pauses the timer |
| dwell_in_area | Hang out 1 min | Anywhere inside the outline |
| follow_line | Walk {a quarter of / a third of / half / two thirds of / three quarters of / the whole} the trail; verb and noun per kind (Ride … the path, Walk … the ridge, Follow … the river, Walk … the towpath, Walk … the loop); stairs: Climb all the stairs | {share} of {length} · either direction (stairs: All {length} · up or down) |
| street spot | Get here | Within 130 ft of the pin |
| compass | Head north-east | 0.8 mi out, then within 130 ft |
| courier | Deliver the package | Pick up at A, drop at B in 10 min |
| round trip | Out and back | Touch the far point, then come home |
| collect | Collect 10 acorns | Bring them home to bank |
| cover_cells | Explore 2% more | Walk where you haven't been yet |
| steps | Walk 2,000 steps | Any pace, anywhere |
| away | Be out 1 hour | Time away from home |
| boss | Face the boss | The hardest thing your realm offers |

Numbers come from the quest (radius, minutes, share, lengths) in the player's units. `take_five` is not listed: it is
being removed (#141).

## Progress block

Shown for every quest that measures something, at zero too, in the state colour. Label left, amount right, bar below,
optional one-line hint (only when it adds something not on the detail line). Done shows a green **Done** and a full bar.

| Verify type | Label | Amount |
| -- | -- | -- |
| dwell | Time here | 1:12 of 3:00 |
| dwell_in_area | Time inside | 0:24 of 1:00 |
| follow_line | Traveled | 0.6 of 1.4 mi (same unit for both numbers) |
| steps | Steps | 800 of 2,000 |
| away | Time away | 25 of 60 min |
| cover_cells | {Realm} explored | 34.8% → 36% — layered bar: already explored (muted), this quest's gain (state colour), target tick (#143) |
| collect | Banked | 3 of 10 acorns |
| courier, round trip | two-step tracker instead of a bar: ① Pick up → ② Deliver / ① Reach the far point → ② Back home; hint "6:40 left to deliver" | |
| reach | none (you are there or not) | |

### Forager backpack (#142)

Above the Banked bar, a backpack row: header `Backpack · {n}` left, load word right (**Empty**, **Light**, **Full**,
**Heavy · steps −5%** in amber). Below it, slots flow left to right and wrap: one per bag space, carried items show the
item icon, free spaces are dashed circles, items past the bag size are amber. No divider line and no "+" slot. Hints:
full "More fits, but each extra slows your steps"; over "Extra items slow your step count until you bank". The map total
is never shown.

Open: **one central backpack for all quests (recommended, as in `economy-eggs-buddies.md`)** or capacity per item type.
Central means the card also shows other carried things as grey dots. To confirm with the owner before #142.

## About this place

A fold-out row at the bottom ("About this place ▾", closed by default, per card). Absent when the place has no
description, no Wikipedia and no fact rows.

- Description first (a short paragraph).
- Then a labelled fact sheet, fixed order, at most 6 rows: Kind, Artist, Made, Faith, Food, Species, Material, Surface,
  Difficulty, Elevation, Height, Lighting, Hours, Entry (fee · private access · wheelchair in one line), Run by, Address.
  A row that repeats the place row's type is skipped.
- Footer under a divider: "Read more on Wikipedia ↗" (only `http(s)` links built by the core from `wikipedia=lang:Title`).

## Core and FFI

All wording and choices live in the core (cross-platform rule); Kotlin only lays out what `QuestOut` carries:

- `headline`, `detail` (strings, player's units).
- `place_name`, `place_type`, `place_quoted` (bool) — from the linked feature, stored with the assignment when the quest is
  placed (and by the relink for old games), so a quest refresh reads no atlas.
- `progress: Option<ProgressOut>` — `label`, `value`, `frac`, optional `base`/`target` (layered bar), optional
  `steps` + `step_at` (two-step tracker), optional `hint`.
- `bag: Option<BagOut>` for forager (after #142).
- `about: Option<AboutOut>` — `description`, `rows: Vec<(label, value)>`, `wikipedia_url` — built lazily by
  `Engine::poi_details` when the row is opened (atlas read off the main thread), so `QuestOut` only carries
  `has_about: bool`.

Removed from the PR #136 build: the ⓘ button, `PoiDetailsCard`, link rows, phone/website/OSM links, `osm_url`.

## Follow-ups (own issues, not this card)

- #140 line share snapped to friendly fractions (the headline states the rule exactly).
- #141 dwell only at real lingering places; remove `take_five`.
- #142 forager respawn + soft carry limit (backpack).
- #143 Cartographer as % of the realm explored; explored % on the realm card/HUD.

Until those land, the card shows today's rule honestly (e.g. a line quest whose share is not a friendly fraction says
"Walk 37% of the trail"; Cartographer keeps a count until #143).

## Testing

- Core: headline/detail per verify type and kind (units, fractions, quoted titles, stairs); progress labels and amounts
  at open / in progress / done; about sheet order, 6-row cap, skipped duplicate kind row, empty → `None`; Wikipedia URL.
- Android: `QuestCardFormat`-style unit tests only where Kotlin decides layout (none expected); device check of each
  family on the phone.
