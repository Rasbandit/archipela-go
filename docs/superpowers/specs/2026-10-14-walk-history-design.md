# Walk history in the Activity log + forager progress design

Builds on the walk recap v1/v2 specs (same branch, `feat/walk-recap`).

## Goal

Every walk worth a recap is kept. It shows as one row in the Activity log, and tapping the row opens that walk's recap,
animated as on arrival. The recap subtitle also gains the start and end times.

## Decisions (owner, 2026-10-14)

- Keep every recapped walk, per game, for as long as the game exists. Deleting the game deletes them (the journal's
  `clear_game`).
- Opening an old walk animates the bars and confetti every time, like the welcome-home popup.
- Only walks that pass `Recap::worth_showing` get a row. Nothing new is recorded for short or empty outings.
- The subtitle reads "Tue 10/14 · 5:12 – 5:54 PM · 3.1 km · 42 min" (weekday and short date, start and end times in the
  phone's 12/24 h format, distance, time out), both on arrival and from the Activity log.

## Forager progress (owner, 2026-10-14)

Scavenging (forager, `Target::Collect`) quests get a bar each in the "This walk" card, replacing the "Brought home: N items"
section.

- **Which quests.** Only foragers whose banked or picked count changed on the walk get a bar.
- **The bar.**
  - Blue shows what was banked before the walk and orange what this walk banked.
  - The track ends at `need`, with one mark at `need`.
  - Crossing it (the walk completed the forager) bounces the mark and fires confetti, through the same `RecapChainBar`.
- **Text.**
  - Theme plus gain plus totals: "Pinecones · +2 banked · 5 / 7".
  - When items are still carried after arriving (a trap blocked banking): "1 still in your bag".
- **Core.**
  - `Outing.forage_at_start: BTreeMap<i64, (u32 banked, u32 picked)>` (`#[serde(default)]`), set in `open_outing` from
    `collected`.
  - `Recap.foragers: Vec<RecapForager { name, theme, need, banked_before, banked_after, picked, carried }>`
    (`#[serde(default)]`).
  - `picked` is the number of items picked on the walk. A forager without a start entry counts from 0.
  - `banked` (the total) stays, so the notification text is unchanged.
  - `worth_showing` is unchanged: `banked > 0` already counts, and a forager that only picked shows up through the steps it
    took.
- **FFI.** `RecapOut.foragers: Vec<RecapForagerOut>`.
- **Preview.** `RecapSample` gains one forager that completes ("pinecones", need 7, banked 5 → 7, picked 2) and one that
  doesn't ("leaves", need 7, banked 3 → 4).

## Core (`core/src/journal.rs`)

- A new table `walks (id INTEGER PRIMARY KEY, game TEXT NOT NULL, end_ms INTEGER NOT NULL, recap TEXT NOT NULL)` with index
  `(game, end_ms)`, created in `SCHEMA` (`CREATE TABLE IF NOT EXISTS`, so existing journals gain it). `recap` holds the
  `Recap` as JSON (serde).
- New methods:
  - `Journal::add_walk(game, &Recap)`;
  - `Journal::walk(game, end_ms) -> Option<Recap>`;
  - `clear_game` also deletes the game's walks.
- A new event kind `kind::WALK = "walk"`. Its `detail` is a short summary (distance, time, quest count) and its `t_ms` is the
  recap's `end_ms`, so the row sorts at arrival time and keys the walk.

## Engine (`core/ffi/src/engine.rs`)

- `bank_at_home`: when it stores a new recap (the `RecapReady` case), it also calls `add_walk` and logs the `walk` event, in
  one journal batch after the game lock is released (same order as the existing journal writes).
  - A journal failure is noted and never blocks arrival; the popup still works from `Game.recap`.
- `Engine::walk_recap(end_ms: i64) -> Option<RecapOut>` reads one walk of the open game (or the last game, like
  `activity`).
- `dismiss_recap` is unchanged. It only clears `Game.recap` (the popup), never the history.

## Android

- **`ActivityScreen`:** a `walk` row shows its detail with a walk icon. It is a real button (`Role.Button`, 48 dp+); tapping it
  calls `engine.walkRecap(row.tMs)` and shows the recap.
  - `AwayFormat` labels the kind "Walk".
- **`RecapController.showHistory(r)`** shows a stored walk the same way as `preview`: closing it never touches the core.
  `RecapScreen` gets a `closeLabel` ("Done" for the popup, "Close" for history); the animation is the same.
- **`RecapFormat.subtitle`:** the format above, with times from `android.text.format.DateFormat.getTimeFormat(ctx)`. A pure
  overload takes a `java.text.DateFormat` so tests can pin it.

## Testing

- **Journal:**
  - `add_walk`/`walk` round-trip;
  - `walk` of another game or a missing `end_ms` is `None`;
  - `clear_game` removes walks;
  - an old journal file without the table opens and gains it.
- **Engine:**
  - an arrival that stores a recap adds one walk and one `walk` event;
  - a second arrival adds nothing;
  - `walk_recap(end_ms)` returns it;
  - `dismiss_recap` leaves the walk;
  - a walk survives an engine restart.
- **Android:**
  - `subtitle` with a fixed format and zone ("Tue 10/14 · 5:12 – 5:54 PM · 3.1 km · 42 min");
  - `AwayFormat` "walk" label;
  - the history close decision (as for preview).

## Out of scope

- Editing or deleting single walks.
- Totals across walks (a "your week" view).
- Exporting walks.

## Build order

0. Core + FFI + Android: forager progress (section above), with tests:
   - before and after banked per forager;
   - picked on the walk;
   - a forager without a start entry;
   - a forager completed by banking on arrival;
   - carried items left after a trap blocked banking.
1. Journal: `walks` table, `add_walk`, `walk`, `clear_game`, the `walk` kind.
2. Engine: store the walk on arrival, and `walk_recap`.
3. Android: Activity row, history open/close, subtitle times.
