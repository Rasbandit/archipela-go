//! Map line widths that scale with zoom, per line kind.
//!
//! The core gives each kind a list of `zoom → width` stops; the platform turns them into one renderer expression
//! (`MapLibre` `interpolate(exponential(CURVE_BASE), zoom, …)`), so the GPU side scales lines every frame with no app work.

/// Base of the exponential interpolation between stops (map scale doubles per zoom level, so linear feels wrong).
pub const CURVE_BASE: f32 = 1.5;

/// A kind of line drawn on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// The walked trace.
    Trace,
    /// A trail or other route quest line.
    Trail,
    /// The white casing under a trail line.
    TrailCasing,
    /// A park outline.
    ParkOutline,
    /// A realm outline.
    RealmOutline,
    /// The editor's draft line.
    Draft,
    /// The editor's radius ring.
    RadiusRing,
}

/// One stop: at `zoom`, the line is `width` pixels wide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WidthStop {
    /// Map zoom level.
    pub zoom: f32,
    /// Line width in pixels.
    pub width: f32,
}

/// The shared curve: at each zoom, a line is this factor of its street-level width (z16 = today's look).
const SHAPE: [(f32, f32); 3] = [(11.0, 0.3), (16.0, 1.0), (19.0, 1.5)];

impl LineKind {
    /// Width at street level (z16) and the floor that keeps the line visible zoomed out. Tune a kind here.
    const fn street_and_floor(self) -> (f32, f32) {
        match self {
            Self::Trace | Self::RadiusRing => (2.5, 1.0),
            Self::Trail => (4.0, 1.5),
            Self::TrailCasing => (7.0, 3.0),
            Self::ParkOutline => (2.0, 1.0),
            Self::RealmOutline => (1.8, 0.75),
            Self::Draft => (3.0, 1.5),
        }
    }
}

/// The line widths for `kind`, ordered by zoom. The renderer clamps to the first and last stop outside their range.
#[must_use]
pub fn width_stops(kind: LineKind) -> Vec<WidthStop> {
    let (street, floor) = kind.street_and_floor();
    SHAPE.iter().map(|&(zoom, factor)| WidthStop { zoom, width: (street * factor).max(floor) }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [LineKind; 7] =
        [LineKind::Trace, LineKind::Trail, LineKind::TrailCasing, LineKind::ParkOutline, LineKind::RealmOutline, LineKind::Draft, LineKind::RadiusRing];

    /// Today's fixed widths, which stay the look at street level (z16).
    fn street_width(kind: LineKind) -> f32 {
        match kind {
            LineKind::Trace | LineKind::RadiusRing => 2.5,
            LineKind::Trail => 4.0,
            LineKind::TrailCasing => 7.0,
            LineKind::ParkOutline => 2.0,
            LineKind::RealmOutline => 1.8,
            LineKind::Draft => 3.0,
        }
    }

    #[test]
    fn every_kind_has_stops_with_rising_zoom() {
        for kind in ALL {
            let stops = width_stops(kind);
            assert!(stops.len() >= 2, "{kind:?}");
            assert!(stops.windows(2).all(|w| w[0].zoom < w[1].zoom), "{kind:?}");
        }
    }

    #[test]
    fn street_level_keeps_todays_width() {
        for kind in ALL {
            let at16 = width_stops(kind).into_iter().find(|s| s.zoom.to_bits() == 16f32.to_bits());
            assert_eq!(at16.map(|s| s.width), Some(street_width(kind)), "{kind:?}");
        }
    }

    #[test]
    fn lines_are_thinner_zoomed_out_and_bolder_zoomed_in() {
        for kind in ALL {
            let stops = width_stops(kind);
            assert!(stops.windows(2).all(|w| w[0].width < w[1].width), "{kind:?}");
            let (first, last) = (stops[0], stops[stops.len() - 1]);
            assert!(first.width < street_width(kind) && last.width > street_width(kind), "{kind:?}");
        }
    }

    #[test]
    fn floors_keep_lines_visible_zoomed_out() {
        assert_eq!(width_stops(LineKind::ParkOutline)[0].width, 1.0);
        assert_eq!(width_stops(LineKind::RealmOutline)[0].width, 0.75);
        assert_eq!(width_stops(LineKind::Trail)[0].width, 1.5);
        for kind in ALL {
            assert!(width_stops(kind).iter().all(|s| s.width >= 0.75), "{kind:?}");
        }
    }

    #[test]
    fn casing_is_wider_than_trail_at_every_stop() {
        for (casing, line) in width_stops(LineKind::TrailCasing).into_iter().zip(width_stops(LineKind::Trail)) {
            assert_eq!(casing.zoom.to_bits(), line.zoom.to_bits());
            assert!(casing.width > line.width, "z{}", casing.zoom);
        }
    }
}
