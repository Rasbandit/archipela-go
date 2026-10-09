# UI design system (Android)

Everything visual comes from `android/app/src/main/java/dev/apgo2/ui/`. Screens and the map compose these; they never restyle Material
widgets or write colours themselves.

| File | Holds |
| -- | -- |
| `Palette.kt` | `ApgoPalette`: every colour (brand, quest states, map/editor, on-colours, feedback, the light/dark Material schemes) plus `Color.hex()` for MapLibre |
| `Theme.kt` | `ApgoTheme` (wraps the app in `MainActivity`): picks the light or dark scheme from the palette |
| `Components.kt` | `ApgoChip`, `ChoiceChips`, `ModeChips`, `MapOverlayCard`, `MapBubble` (+ `BubblePlacement`), `FeedbackText`/`Tone`, `MarkToggle`, `IconLabel`, `MODES`, `modeLabel()` |
| `MapMarkers.kt` | `MapMarkers`/`MarkerSpec`: the one definition of a map pin (see below) |
| `Icons.kt` | `ApgoIcons`: every icon named by meaning, backed by Lucide |

## Rules

- A colour is added to `ApgoPalette`, never inlined (`Color(0x...)`, `Color.White` or `"#rrggbb"`); `scripts/check_color_tokens.sh` (in `just check-android`)
  fails on literals outside `Palette.kt`. Name tokens by role (`onPin`, `onMap`, `onBrand`) so one edit changes every use. Quest state has one colour set, `ApgoPalette.quest(state)`: the list icon tint
  and the map pin's state badge both use it, so they cannot drift apart. What a quest *is* is shown by family colour and icon (`ApgoPalette.kind`), on the map and in the realm editor alike.
- A map pin is only ever built by `MapMarkers.render(MarkerSpec)`: `Find` (realm editor: family colour, ring for favorite, grey for banned) and `Quest` (Play map: family colour,
  corner badge for state: none = open, amber dot = in progress, green check = done, lock = locked; locked pins are grey). Size by difficulty (`iconScale`), draw order by state (`drawOrder`).
- Pins are never hidden by collision. Zooming out first shrinks them (full size from zoom 16, half by 13: `shrinkWhenZoomedOut`); below
  zoom 16 pins still too close merge into a numbered cluster (MapLibre source clustering, `MapSource.CLUSTERED`). A quest cluster is a ring split by
  how many quests inside are in each state (`MarkerSpec.Ring`, drawn on demand via the style's missing-image listener); a find cluster is a plain teal
  disc. Tapping a cluster zooms in until it splits. The selected pin lives in its own unclustered, unshrunk source (`MapFeatures.splitSelected`).
- **Map colour = state, icon = kind.** Your walked trace is a thin, solid pale lavender (`ApgoPalette.trace`; never transparent, since overlaps would darken; no quest state uses it) and the realm boundary a faint neutral grey:
  both are background. Quests, park outlines, trails and cluster rings use the state colours: blue doable, amber in progress, green done, grey locked (`ApgoPalette.quest`,
  `MapMarkers.questFill`), so a glance shows which parts of the map are finished. A quest pin's white icon says what kind it is; family colours are
  for the realm editor's finds only. Red never means "not done": it is for errors and bans.
- **Play screen layout.** The map fills the area under the header and never resizes; the panel slides over its bottom. The panel is
  shown or hidden (no free size): the goal and the summary line always show, the progress list only when shown. Its grip is drawn thin
  but grabbed over a tall area; a tap toggles it, a drag follows the finger and snaps past `SNAP_DP`. The map's bottom padding follows
  the panel frame by frame, so the middle of what you see stays in the middle of what is visible. A touch anywhere on the panel never
  reaches the map. Portrait only (`android:screenOrientation`).
- **The Play map is kept alive** under the other tabs (`AppRoot`: composed but unplaced, `MapLife` stops and hides the view), so coming
  back has no reload or camera jump. While hidden it reads no new data (`held` in `PlayScreen.kt`); a different game gets a fresh map
  (`key(gameId)`). A new map starts near what it will frame (or the last place, saved in the core's settings) and stays covered until
  framed, so the world view never flashes.
- **Map taps**: within `TAP_SLOP_DP` a pin wins, then a trail or park outline, then anywhere inside a park (the core's `park_at`; with
  a popup open such a tap closes it instead). A trail or park shows its details where it was touched. Selecting scrolls the map only as
  far as it takes to show the pin and its callout (`FocusShift`), never zooming.
- Line widths scale with zoom on the GPU side (a MapLibre zoom expression built from the core's `line_width` stops; z16 is the reference look).
  Tune a line kind in `core/src/line_width.rs`, never with fixed widths in Kotlin.
- Parks are a thin outline, always dashed (only its state colour tells the state; a solid/dashed split read as a bug), with a fill that shows
  progress (empty, light, faint). Trails are a solid line in the state colour on a
  white casing, never dashed (the base map draws footpaths dashed) and without direction arrows (coverage counts either way); done trails fade.
- A callout attached to a pin is a `MapBubble` (placement in `BubblePlacement`, unit-tested): the realm editor's find callout and the Play quest popup both use it. Quests with no pin
  (steps, new squares, time away) show the same content in a `MapOverlayCard` at the bottom of the map.
- Every distance, area and percentage the player sees goes through `ui/Units.kt` (`distance`, `area`, `percent`). Distances and
  areas are formatted by the core (`core/src/units.rs`, exported as `formatDistance`/`formatArea`), the same formatter the core
  uses for quest, goal, trap and near-miss text: clean numbers, at most one decimal (`50 m`, `1.4 km`, `60 ft`, `0.3 mi`,
  `0.4 km²`). Core text rounds limits down and amounts to go up (`Round` in `units.rs`), so it never promises more room than the
  check allows. The units are `Units.system`, set by `UnitSettings` from the engine (Settings tab: Auto/Kilometres/Miles; Auto
  follows the region the app passes to `Engine.setRegion`, re-sent on every return to the foreground). Never format a shown
  distance in Kotlin or with `"%.1f".format(...)`.
  Always a decimal point and Western digits. Dates and times stay localized.
- JVM unit tests load the host build of the core (`jna.library.path` in `app/build.gradle.kts`), so tests can call FFI functions.
- Compose text colours come from `MaterialTheme.colorScheme` roles or `FeedbackText(Tone.*)`.
- A pattern used twice becomes a component in `Components.kt`. Selected chips are a solid `primary` fill because the Material default
  (`secondaryContainer`) blended into the card behind it.
- Map layers read the palette once when the style loads, so map colours are not theme-reactive on purpose (the basemap is light).

## Vocabulary

A **find** is a scanned spot a realm can use for a quest (a bench, a park, a trail start, a fountain). The UI says "finds"; core code says
feature/place. FFI: `FindOut`, `realm_finds`, `set_find_mark`. A **mark** is a favorite or a ban on a find (per realm).

## Realm editor (no Save, no Cancel)

Edits save as they finish (a drag ends, a corner is tapped, a name pauses for 600 ms, an icon is picked). A new realm is created by its first
edit and named `Realm N` (lowest unused N). One `History<EditSnap>` (`ui/History.kt`) holds shape, name and icon, so Undo/Redo cover them all.
Left toolbar = modes: a Circle/Polygon pill (one or the other = editing the area) and a Details pill. Right = Undo, Redo, Done (X). Area has no
panel, only a hint; Details has the half-height panel. Finds are fetched when Details opens (or the editor closes) after the outline changed.
Read editor state inside click handlers, not from vals captured at composition (they can be stale by the time the lambda runs).
Android reads a swipe that starts on a screen edge as Back: do not start drag gestures there (tests too).

## Help and tooltips (ui/Help.kt, ui/HelpText.kt)

All explanatory copy lives in `HelpText.kt` as `HelpTopic(title, body)` values under `object Help`; screens only point at a topic.
`HelpTip(topic)` is a small ⓘ; `LabelWithHelp(text, topic)` is a label plus its ⓘ (the label can also be pressed and held). Tips are persistent
Material rich tooltips, open one at a time, and close on a tap elsewhere. To explain something new: add a topic, then use one of the two components.
Never write help text inline in a screen.

## Icons

Lucide (<https://lucide.dev>, ISC) via `com.composables:icons-lucide-android` in `libs.versions.toml`. No emoji or glyph characters in UI text:
add the icon to `ApgoIcons` (named by meaning, e.g. `Favorite`, not `Star`) and use it through `Icon`, `ApgoChip(icon=)`, `IconLabel` or
`MarkToggle`. Screens never import the icon library. The ISC notice lives in `THIRD_PARTY_NOTICES.md` and must stay with the app.
Chosen over Material Icons Extended for a friendlier, more distinctive look.
Quest kinds and finds get their own icon: `ApgoIcons.forKind(kindId, family)` (about 60 kind overrides, falling back to the family icon).
Map symbols are bitmaps, so `ui/MapIcons.kt` draws the same Lucide `ImageVector`s into images (`renderPin` for finds, `renderQuestPin` for quests; call them through `MapMarkers`).

## Where the colours come from

Archipelago's web theme, ArchipelagoMW/Archipelago `WebHostLib/static/styles` (MIT): ocean theme `#11233e` panels, `#93dcff` headings,
`#fffc95` links, `#83a8e1`/`#4c658b` code boxes; header teals `#2f6b83`/`#699ca8`, borders `#d0ebe6`; grass theme `#5aff6a`, `#b5e9a4`.
The old React Native app's item colours (`styles/Colors.tsx`): `#00D168` green, `#00BDBD` cyan, `#6D8BE8` blue, `#AF99EF` purple,
`#FA8072` salmon. Colours are facts, so using them is fine.

## Launcher icon

Our own original artwork (no Archipelago logo shapes): a sky map pin with a navy four-point star, landing on three butter island dots,
on a navy background. Adaptive icon (minSdk 26, so `mipmap-anydpi` with no `-v26` qualifier and no PNG fallbacks): `res/mipmap-anydpi/ic_launcher{,_round}.xml` with
`res/drawable/ic_launcher_foreground.xml` (inside the 66 dp safe zone) and `ic_launcher_monochrome.xml` (themed icons, star cut out).
Colours are in `res/values/colors.xml`, each commented with the `ApgoPalette` entry it mirrors; change both together.

## Icons and the Archipelago logo: do NOT bundle it

- The logo is (c) 2022 Krista Corkos and Christopher Wilson, **CC BY-NC 4.0** (stated in the old app's `assets/LICENSE.txt`). The old
  app's `icon.png`, `adaptive-icon.png`, `splash.png`, `*-icon.png` are copies of it, so they carry the same licence.
- Archipelago's own `WebHostLib/static/static/branding/LICENSE` says "Copyright 2022 LegendaryLinux (Chris Wilson). All rights reserved."
  and its `LICENSE` exempts that folder from the repo's MIT.
- Consequences: non-commercial only, attribution required, cannot sit under our MIT licence, and as a launcher icon it would imply an
  official app. So the launcher/adaptive icon is our own artwork. Showing the logo on a "Connected to Archipelago" screen with
  attribution is possible for a free fan app but needs an explicit owner decision.
