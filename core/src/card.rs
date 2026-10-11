//! What the quest card says: a short headline (one line) and one detail line, from the quest itself, in the player's units.

use crate::assign::{Assignment, Target};
use crate::geo::{bearing_deg, distance_m, polyline_len_m, Point};
use crate::units::{distance, distance_rounded, Round, UnitSystem};

/// The card's headline and the grey line under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardText {
    /// What to do, short enough for one line.
    pub headline: String,
    /// The numbers and conditions behind it.
    pub detail: String,
}

/// Dwell kinds where you sit ("Sit 3 min"); other dwells say "Stay".
pub const SIT_KINDS: [&str; 3] = ["bench_warmer", "picnic_break", "under_the_roof"];
/// Kinds whose places are titled works, shown in quotes on the place row.
pub const TITLED_KINDS: [&str; 3] = ["gallery_walls", "mural_mural", "remember_when"];

/// The friendly words for a line share, when it is one of ¼ ⅓ ½ ⅔ ¾ or all.
#[must_use]
pub fn line_share_phrase(coverage: f64) -> Option<&'static str> {
    const SHARES: [(f64, &str); 6] =
        [(0.25, "a quarter of"), (1.0 / 3.0, "a third of"), (0.5, "half"), (2.0 / 3.0, "two thirds of"), (0.75, "three quarters of"), (1.0, "the whole")];
    SHARES.iter().find(|(f, _)| (coverage - f).abs() < 0.005).map(|(_, p)| *p)
}

/// "1 hour", "1 h 30 min", "45 min".
fn duration(minutes: f64) -> String {
    let m = crate::num::round_u64(minutes.max(0.0));
    match (m / 60, m % 60) {
        (0, m) => format!("{m} min"),
        (1, 0) => "1 hour".into(),
        (h, 0) => format!("{h} hours"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// 8 compass words from a bearing in degrees.
fn compass_word(deg: f64) -> &'static str {
    const W: [&str; 8] = ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"];
    let i = crate::num::round_u64(deg.rem_euclid(360.0) / 45.0) % 8;
    W[usize::try_from(i).unwrap_or(0)]
}

/// The card text of quest `a` (with `home` for direction words).
#[must_use]
pub fn card_text(a: &Assignment, home: Point, units: UnitSystem) -> CardText {
    let within = |r: f64| format!("Within {}", distance_rounded(r, units, Round::Down));
    let (headline, detail) = match &a.target {
        Target::Point { p, r } if a.kind_id == "compass_rose" => (
            format!("Head {}", compass_word(bearing_deg(home, *p))),
            format!("{} out, then {}", distance(distance_m(home, *p), units), within(*r).to_lowercase()),
        ),
        Target::Point { r, .. } if a.kind_id == "street_smarts" || a.fallback => ("Get here".into(), format!("{} of the pin", within(*r))),
        Target::Point { r, .. } => ("Get here".into(), within(*r)),
        Target::Dwell { r, minutes, .. } => (
            format!("{} {minutes:.0} min", if SIT_KINDS.contains(&a.kind_id.as_str()) { "Sit" } else { "Stay" }),
            format!("{} · leaving pauses the timer", within(*r)),
        ),
        Target::DwellArea { minutes, .. } => (format!("Hang out {minutes:.0} min"), "Anywhere inside the outline".into()),
        Target::Line { pts, coverage, .. } => line_text(&a.kind_id, polyline_len_m(pts), *coverage, units),
        Target::Courier { time_limit_min, .. } => ("Deliver the package".into(), format!("Pick up at A, drop at B in {time_limit_min:.0} min")),
        Target::RoundTrip { .. } => ("Out and back".into(), "Touch the far point, then come home".into()),
        Target::Cells { .. } => ("Explore somewhere new".into(), "Walk where you haven't been yet".into()),
        Target::Steps { n } => (format!("Walk {} steps", thousands(u64::from(*n))), "Any pace, anywhere".into()),
        Target::Away { minutes } => (format!("Be out {}", duration(*minutes)), "Time away from home".into()),
        Target::Collect { need, theme, .. } => (format!("Collect {need} {theme}"), "Bring them home to bank".into()),
    };
    CardText { headline, detail }
}

fn line_text(kind: &str, len_m: f64, coverage: f64, units: UnitSystem) -> (String, String) {
    if kind == "stairmaster" {
        return ("Climb all the stairs".into(), format!("All {} · up or down", distance(len_m, units)));
    }
    let (verb, noun) = match kind {
        "pedal_pusher" => ("Ride", "path"),
        "full_circle" => ("Walk", "loop"),
        "ridge_runner" => ("Walk", "ridge"),
        "follow_the_flow" => ("Follow", "river"),
        "towpath_tour" => ("Walk", "towpath"),
        _ => ("Walk", "trail"),
    };
    let headline = match line_share_phrase(coverage) {
        Some("the whole") => format!("{verb} the whole {noun}"),
        Some(p) => format!("{verb} {p} the {noun}"),
        None => format!("{verb} {:.0}% of the {noun}", coverage * 100.0),
    };
    let detail = if coverage >= 0.995 {
        format!("All {} · either direction", distance(len_m, units))
    } else {
        format!("{} of {} · either direction", distance(len_m * coverage, units), distance(len_m, units))
    };
    (headline, detail)
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assign::{Assignment, Target};
    use crate::catalog::Mode;
    use crate::geo::{destination, Point};
    use crate::units::UnitSystem::Imperial;

    fn home() -> Point {
        Point::new(40.0, -111.0)
    }

    fn quest(kind: &str, target: Target) -> Assignment {
        Assignment {
            location_id: 1,
            zone: 1,
            mode: Mode::Walk,
            family: "x".into(),
            kind_id: kind.into(),
            quest_name: "Q".into(),
            blurb: String::new(),
            place: "P".into(),
            feature_id: None,
            place_type: None,
            has_about: false,
            tier: 2,
            effort_min: 10.0,
            target,
            fallback: false,
            boss: kind == "the_big_one",
        }
    }
    fn text(a: &Assignment) -> (String, String) {
        let c = card_text(a, home(), Imperial);
        (c.headline, c.detail)
    }
    fn line(len_m: f64, coverage: f64) -> Target {
        Target::Line { pts: vec![home(), destination(home(), 0.0, len_m)], corridor_m: 25.0, coverage }
    }

    #[test]
    fn places_get_short_headlines_and_the_numbers_below() {
        assert_eq!(text(&quest("gallery_walls", Target::Point { p: home(), r: 40.0 })), ("Get here".into(), "Within 130 ft".into()));
        assert_eq!(text(&quest("bench_warmer", Target::Dwell { p: home(), r: 40.0, minutes: 3.0 })).0, "Sit 3 min");
        assert_eq!(
            text(&quest("swing_time", Target::Dwell { p: home(), r: 40.0, minutes: 3.0 })),
            ("Stay 3 min".into(), "Within 130 ft · leaving pauses the timer".into())
        );
        assert_eq!(
            text(&quest("touch_grass", Target::DwellArea { poly: vec![], center: home(), r: 60.0, minutes: 1.0 })),
            ("Hang out 1 min".into(), "Anywhere inside the outline".into())
        );
    }

    #[test]
    fn line_quests_say_a_friendly_share_and_the_distance() {
        assert_eq!(text(&quest("trail_boss", line(4506.0, 0.5))), ("Walk half the trail".into(), "1.4 mi of 2.8 mi · either direction".into()));
        assert_eq!(text(&quest("trail_boss", line(4506.0, 0.25))).0, "Walk a quarter of the trail");
        assert_eq!(text(&quest("pedal_pusher", line(4506.0, 1.0))), ("Ride the whole path".into(), "All 2.8 mi · either direction".into()));
        assert_eq!(text(&quest("full_circle", line(4506.0, 0.75))).0, "Walk three quarters of the loop");
        assert_eq!(text(&quest("stairmaster", line(60.0, 0.8))), ("Climb all the stairs".into(), "All 200 ft · up or down".into()));
    }

    #[test]
    fn a_share_that_is_not_a_friendly_fraction_is_stated_exactly() {
        assert_eq!(text(&quest("trail_boss", line(4506.0, 0.37))).0, "Walk 37% of the trail");
    }

    #[test]
    fn boss_wording_follows_its_target() {
        assert_eq!(text(&quest("the_big_one", line(4506.0, 0.5))).0, "Walk half the trail");
        assert_eq!(text(&quest("the_big_one", Target::Point { p: home(), r: 40.0 })).0, "Get here");
    }

    #[test]
    fn quests_without_a_place_read_plainly() {
        let ne = destination(home(), 45.0, 1300.0);
        assert_eq!(text(&quest("compass_rose", Target::Point { p: ne, r: 40.0 })), ("Head north-east".into(), "0.8 mi out, then within 130 ft".into()));
        assert_eq!(text(&quest("street_smarts", Target::Point { p: ne, r: 40.0 })), ("Get here".into(), "Within 130 ft of the pin".into()));
        assert_eq!(text(&quest("step_up", Target::Steps { n: 2000 })), ("Walk 2,000 steps".into(), "Any pace, anywhere".into()));
        assert_eq!(text(&quest("wanderlust", Target::Away { minutes: 60.0 })), ("Be out 1 hour".into(), "Time away from home".into()));
        assert_eq!(text(&quest("wanderlust", Target::Away { minutes: 90.0 })).0, "Be out 1 h 30 min");
        assert_eq!(
            text(&quest("cartographer", Target::Cells { n: 8, cell_m: 150.0 })),
            ("Explore somewhere new".into(), "Walk where you haven't been yet".into())
        );
        assert_eq!(
            text(&quest("special_delivery", Target::Courier { a: home(), b: ne, r: 40.0, time_limit_min: 10.0 })),
            ("Deliver the package".into(), "Pick up at A, drop at B in 10 min".into())
        );
        assert_eq!(
            text(&quest("there_and_back", Target::RoundTrip { far: ne, r: 50.0 })),
            ("Out and back".into(), "Touch the far point, then come home".into())
        );
        let collect = Target::Collect { pts: vec![ne], need: 5, r: 25.0, theme: "acorns".into() };
        assert_eq!(text(&quest("forager", collect)), ("Collect 5 acorns".into(), "Bring them home to bank".into()));
    }
}
