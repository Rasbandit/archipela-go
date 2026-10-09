# Zoom-scaled line widths — design

## Intent

Map lines have fixed pixel widths. They look right at street level but feel far too thick when zoomed out. Owner wants
a core module that gives each line kind its own width that scales with zoom.

Decided with the owner:

- **All map lines** move to the core table, not only the three that look wrong today (walked trace, trails, park outlines).
- **Curve:** today's widths are the z16 anchor. Lines thin out when zoomed out and grow a little when zoomed in, so they
  never look spindly up close. Each kind has a floor so it never vanishes.
- **GPU first:** scaling runs inside the map renderer (a MapLibre zoom expression), not in app code. Use GPU / renderer
  APIs wherever possible.

Success: zoomed out to z11–12 the trace, trails and park outlines read as thin strokes; at z16 they look as they do today;
at z19 they are somewhat bolder; pinch-zoom stays smooth with no width jumps.

## Approach

The core owns a table of `zoom → width` stops per line kind. Kotlin turns each kind's stops into one MapLibre
`interpolate(exponential(CURVE_BASE), zoom, stops…)` expression when it builds the style. The renderer then evaluates
the width every frame on the GPU side: there are no camera listeners, no recompute on zoom and no property updates. That
follows the "events over polling" rule. iOS (MapLibre Native) uses the same expression model, so the same table drives it.

Rejected:

- Core `width(kind, zoom)` called on camera idle, with Kotlin pushing new layer properties. Widths would jump after the
  pinch ends, and the camera has to be watched.
- Kotlin-only expressions. This breaks the cross-platform rule, because iOS would have to re-implement the table.

## Core: `core/src/line_width.rs`

- `pub enum LineKind { Trace, Trail, TrailCasing, ParkOutline, RealmOutline, Draft, RadiusRing }`
- `pub struct WidthStop { pub zoom: f32, pub width: f32 }`
- `pub const CURVE_BASE: f32 = 1.5`, the exponential interpolation base.
- `pub fn width_stops(kind: LineKind) -> Vec<WidthStop>`

Each kind has one row: a base width (today's z16 value) and a floor. A shared shape builds the stops as
`width = max(base × factor, floor)`:

| Zoom | Factor |
|------|--------|
| 11   | 0.3    |
| 16   | 1.0    |
| 19   | 1.5    |

Outside 11..19 MapLibre clamps to the nearest stop.

| Kind         | Base (z16) | Floor | Today's Kotlin constant |
|--------------|-----------:|------:|-------------------------|
| Trace        | 2.5        | 1.0   | `TRACE_WIDTH`           |
| Trail        | 4.0        | 1.5   | `QUEST_LINE_WIDTH`      |
| TrailCasing  | 7.0        | 3.0   | `ROUTE_CASING_WIDTH`    |
| ParkOutline  | 2.0        | 1.0   | `PARK_LINE_WIDTH`       |
| RealmOutline | 1.8        | 0.75  | `REALM_LINE_WIDTH`      |
| Draft        | 3.0        | 1.5   | `DRAFT_LINE_WIDTH`      |
| RadiusRing   | 2.5        | 1.0   | `RADIUS_LINE_WIDTH`     |

Values are starting points to be tuned on device. To tune a kind, edit its row.

## FFI: `core/ffi`

Export `LineKind` (`uniffi::Enum`), `WidthStop` (`uniffi::Record`), `width_stops` and `CURVE_BASE` (through a getter
function) as stateless free functions. No engine instance is needed.

## Android: `MapStyle.kt`

- One helper, `scaledWidth(kind: LineKind): Expression`, builds the interpolate expression from `widthStops(kind)`.
- Every `lineWidth(CONST)` becomes `lineWidth(scaledWidth(LineKind.X))`. The seven width constants are deleted.
- Park dashes (`PARK_DASH`) need no change, because MapLibre dash lengths are in line-width units and scale with the line.
- Circle strokes, pins and labels are out of scope (pins already shrink with `shrinkWhenZoomedOut`).

## Testing

Write the core unit tests first (TDD):

- Every `LineKind` has at least 2 stops, with zooms strictly increasing.
- The z16 width equals the base, which pins today's look.
- Width never decreases as zoom increases.
- Every width is at least the kind's floor.
- `TrailCasing` is wider than `Trail` at every stop, so the casing is always visible.

Android: build plus a visual check on the emulator or phone at z12, z16 and z19 for the trace, trails, parks, realms and
the editor draft/radius.

## Out of scope

- Zoom scaling for circle strokes, pin halos and labels (a possible follow-up issue).
- Per-state widths (for example a thicker in-progress trail).
