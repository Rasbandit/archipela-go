# UI design system (Android)

Everything visual comes from `android/app/src/main/java/dev/apgo2/ui/`. Screens and the map compose these; they never restyle Material
widgets or write colours themselves.

| File | Holds |
|--|--|
| `Palette.kt` | `ApgoPalette`: every colour (brand, quest states, map/editor, feedback) plus `Color.hex()` for MapLibre style expressions |
| `Theme.kt` | light and dark `ColorScheme`s built from the palette, and `ApgoTheme` (wraps the app in `MainActivity`) |
| `Components.kt` | `ApgoChip`, `ChoiceChips`, `ModeChips`, `MapOverlayCard`, `FeedbackText`/`Tone`, `MODES`, `modeLabel()` |

## Rules
- A colour is added to `ApgoPalette`, never inlined (`Color(0x...)` or `"#rrggbb"`). The quest list dot and the map marker use the same
  `ApgoPalette.quest(state)`, so they cannot drift apart.
- Compose text colours come from `MaterialTheme.colorScheme` roles or `FeedbackText(Tone.*)`.
- A pattern used twice becomes a component in `Components.kt`. Selected chips are a solid `primary` fill because the Material default
  (`secondaryContainer`) blended into the card behind it.
- Map layers read the palette once when the style loads, so map colours are not theme-reactive on purpose (the basemap is light).

## Where the colours come from
Archipelago's web theme, ArchipelagoMW/Archipelago `WebHostLib/static/styles` (MIT): ocean theme `#11233e` panels, `#93dcff` headings,
`#fffc95` links, `#83a8e1`/`#4c658b` code boxes; header teals `#2f6b83`/`#699ca8`, borders `#d0ebe6`; grass theme `#5aff6a`, `#b5e9a4`.
The old React Native app's item colours (`styles/Colors.tsx`): `#00D168` green, `#00BDBD` cyan, `#6D8BE8` blue, `#AF99EF` purple,
`#FA8072` salmon. Colours are facts, so using them is fine.

## Icons and the Archipelago logo: do NOT bundle it
- The logo is (c) 2022 Krista Corkos and Christopher Wilson, **CC BY-NC 4.0** (stated in the old app's `assets/LICENSE.txt`). The old
  app's `icon.png`, `adaptive-icon.png`, `splash.png`, `*-icon.png` are copies of it, so they carry the same licence.
- Archipelago's own `WebHostLib/static/static/branding/LICENSE` says "Copyright 2022 LegendaryLinux (Chris Wilson). All rights reserved."
  and its `LICENSE` exempts that folder from the repo's MIT.
- Consequences: non-commercial only, attribution required, cannot sit under our MIT licence, and as a launcher icon it would imply an
  official app. So the launcher/adaptive icon is our own artwork. Showing the logo on a "Connected to Archipelago" screen with
  attribution is possible for a free fan app but needs an explicit owner decision.
