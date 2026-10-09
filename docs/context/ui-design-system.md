# UI design system (Android)

Everything visual comes from `android/app/src/main/java/dev/apgo2/ui/`. Screens and the map compose these; they never restyle Material
widgets or write colours themselves.

| File | Holds |
| -- | -- |
| `Palette.kt` | `ApgoPalette`: every colour (brand, quest states, map/editor, feedback) plus `Color.hex()` for MapLibre style expressions |
| `Theme.kt` | light and dark `ColorScheme`s built from the palette, and `ApgoTheme` (wraps the app in `MainActivity`) |
| `Components.kt` | `ApgoChip`, `ChoiceChips`, `ModeChips`, `MapOverlayCard`, `MapBubble` (+ `BubblePlacement`), `FeedbackText`/`Tone`, `MarkToggle`, `IconLabel`, `MODES`, `modeLabel()` |
| `MapMarkers.kt` | `MapMarkers`/`MarkerSpec`: the one definition of a map pin (see below) |
| `Icons.kt` | `ApgoIcons`: every icon named by meaning, backed by Lucide |

## Rules

- A colour is added to `ApgoPalette`, never inlined (`Color(0x...)` or `"#rrggbb"`). Quest state has one colour set, `ApgoPalette.quest(state)`: the list icon tint
  and the map pin's state badge both use it, so they cannot drift apart. What a quest *is* is shown by family colour and icon (`ApgoPalette.kind`), on the map and in the realm editor alike.
- A map pin is only ever built by `MapMarkers.render(MarkerSpec)`: `Find` (realm editor: family colour, ring for favorite, grey for banned) and `Quest` (Play map: family colour,
  corner badge for state: none = open, amber dot = in progress, green check = done, lock = locked; locked pins are grey). Size by difficulty (`iconScale`), draw order by state (`drawOrder`).
- Pins are never hidden by collision. Zooming out first shrinks them (full size from zoom 16, half by 13: `shrinkWhenZoomedOut`); below
  zoom 16 pins still too close merge into a numbered cluster (MapLibre source clustering, `MapSource.CLUSTERED`). A quest cluster is a ring split by
  how many quests inside are in each state (`MarkerSpec.Ring`, drawn on demand via the style's missing-image listener); a find cluster is a plain teal
  disc. Tapping a cluster zooms in until it splits. The selected pin lives in its own unclustered, unshrunk source (`MapFeatures.splitSelected`).
- Red never means "not done": it is for errors and bans. Quest state colours are blue (open), amber (in progress), green (done), grey (locked). Area and route
  quests on the map (`MapStyle`) also show state by line and fill, so it reads without colour: dashed and empty = not started (open or locked), solid with a
  light fill = in progress, solid with a faint fill = done.
- A callout attached to a pin is a `MapBubble` (placement in `BubblePlacement`, unit-tested): the realm editor's find callout and the Play quest popup both use it. Quests with no pin
  (steps, new squares, time away) show the same content in a `MapOverlayCard` at the bottom of the map.
- Every distance, area and percentage the player sees goes through `ui/Units.kt` (`distance`, `area`, `percent`): km or mi by
  region, and always `Locale.US` digits and decimal point. Never `"%.1f".format(...)` a shown number. Dates and times stay localized.
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
