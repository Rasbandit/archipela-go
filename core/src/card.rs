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

/// What the card's progress block shows: a labelled amount with a bar, or a two-step tracker (`steps` non-empty).
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// What is measured ("Time here", "Traveled").
    pub label: String,
    /// How far along, in words ("1:12 of 3:00", "Done").
    pub value: String,
    /// The bar, 0 to 1.
    pub frac: f32,
    /// One extra line when it says something the card does not already.
    pub hint: Option<String>,
    /// A two-step quest's steps; empty for a bar.
    pub steps: Vec<String>,
    /// Steps done (0, 1 or 2).
    pub step_at: u8,
}

/// The progress block of quest `q`; `None` for quests with nothing to measure (reach a spot).
#[must_use]
pub fn card_progress(q: &crate::game::QuestView, now_ms: i64, units: UnitSystem) -> Option<Progress> {
    let done = q.state == crate::game::QuestState::Done;
    let f = if done { 1.0 } else { f64::from(q.progress).clamp(0.0, 1.0) };
    let bar = |label: &str, value: String, hint: Option<String>| Progress {
        label: label.into(),
        value: if done { "Done".into() } else { value },
        frac: crate::num::to_f32(f),
        hint: if done { None } else { hint },
        steps: vec![],
        step_at: 0,
    };
    let steps = |names: [&str; 2], hint: Option<String>| Progress {
        label: String::new(),
        value: String::new(),
        frac: crate::num::to_f32(f),
        hint: if done { None } else { hint },
        steps: names.iter().map(|s| (*s).to_string()).collect(),
        step_at: if done { 2 } else { u8::from(f >= 0.5) },
    };
    Some(match &q.target {
        Target::Point { .. } => return None,
        Target::Dwell { minutes, .. } => bar("Time here", format!("{} of {}", clock(minutes * f), clock(*minutes)), None),
        Target::DwellArea { minutes, .. } => bar("Time inside", format!("{} of {}", clock(minutes * f), clock(*minutes)), None),
        Target::Line { pts, coverage, .. } => {
            let want = polyline_len_m(pts) * coverage;
            let target = distance(want, units);
            let (num, unit) = target.rsplit_once(' ').unwrap_or((target.as_str(), ""));
            bar("Traveled", format!("{} of {num} {unit}", amount_like(want * f, unit, units)), None)
        }
        Target::Steps { n } => bar("Steps", format!("{} of {}", thousands(crate::num::round_u64(f64::from(*n) * f)), thousands(u64::from(*n))), None),
        Target::Away { minutes } => bar("Time away", format!("{} of {minutes:.0} min", crate::num::round_u64(minutes * f)), None),
        Target::Cells { .. } => bar("New ground", format!("{}%", crate::num::round_u64(f * 100.0)), None),
        Target::Collect { need, theme, .. } => {
            let (banked, carried) = q.collected.as_ref().map_or((0, 0), |c| (c.banked, c.carried));
            let mut p = bar("Banked", format!("{banked} of {need} {theme}"), (carried > 0).then(|| format!("Carrying {carried}")));
            p.frac = if *need == 0 { 1.0 } else { crate::num::to_f32((f64::from(banked) / f64::from(*need)).min(1.0)) };
            p
        }
        Target::Courier { time_limit_min, .. } => {
            let left = q.picked_at_ms.filter(|_| f >= 0.5).map(|t0| (time_limit_min * 60_000.0 - crate::num::i64_to_f64(now_ms - t0)) / 60_000.0);
            match left {
                // The limit passed but no fix has reset the tracker yet (it resets on the next fix): back to step one now.
                Some(m) if m <= 0.0 => {
                    let mut p = steps(["Pick up", "Deliver"], Some("Time ran out: pick it up again".into()));
                    (p.step_at, p.frac) = (0, 0.0);
                    p
                }
                Some(m) => steps(["Pick up", "Deliver"], Some(format!("{} left to deliver", clock(m)))),
                None => steps(["Pick up", "Deliver"], Some(format!("{time_limit_min:.0} min to deliver once picked up"))),
            }
        }
        Target::RoundTrip { .. } => steps(["Reach the far point", "Back home"], None),
    })
}

/// "1:12" from 1.2 minutes.
fn clock(minutes: f64) -> String {
    let s = crate::num::round_u64(minutes.max(0.0) * 60.0);
    format!("{}:{:02}", s / 60, s % 60)
}

/// `m` metres as a bare number in `unit` ("mi", "ft", "km", "m"), rounded the way the shared formatter rounds that unit.
fn amount_like(m: f64, unit: &str, units: UnitSystem) -> String {
    match unit {
        "mi" => format!("{:.1}", m / 1609.344),
        "km" => format!("{:.1}", m / 1000.0),
        "ft" => (crate::num::round_u64(m / 0.3048 / 10.0) * 10).to_string(),
        "m" => (crate::num::round_u64(m / 10.0) * 10).to_string(),
        _ => distance(m, units),
    }
}

/// The card's place row: the place's name (quoted for titled works) and what it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceRow {
    /// The place's own name; `None` for an unnamed place.
    pub name: Option<String>,
    /// What it is ("Bench", "Trail"); `None` before the place type is known (an old game before its relink).
    pub kind: Option<String>,
    /// Whether the name is a title of a work, shown in quotes.
    pub quoted: bool,
}

/// The place row of a quest of kind `kind_id` on `place` (its `place_type` if known); `None` for quests with no place.
#[must_use]
pub fn place_row(kind_id: &str, place: &str, place_type: Option<&str>, linked: bool, catalog: &crate::catalog::Catalog) -> Option<PlaceRow> {
    let kind = catalog.kind(kind_id);
    let on_a_place = kind.is_some_and(|k| k.geom != crate::catalog::Geom::None && !k.any_of.is_empty()) || (kind_id == "the_big_one" && linked);
    if !on_a_place {
        return None;
    }
    // An unnamed place's quest is named after its kind (see `assign`), so that name is no place name.
    let name = (kind.is_none_or(|k| k.name != place) && !place.is_empty()).then(|| place.to_string());
    let quoted = name.is_some() && TITLED_KINDS.contains(&kind_id);
    // Nothing to say (an unnamed place of no known type): no row rather than an empty one.
    (name.is_some() || place_type.is_some()).then(|| PlaceRow { name, kind: place_type.map(str::to_string), quoted })
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

    fn view(target: Target, state: crate::game::QuestState, progress: f32) -> crate::game::QuestView {
        crate::game::QuestView {
            place_type: None,
            has_about: false,
            location_id: 1,
            zone: 1,
            name: "Q".into(),
            place: "P".into(),
            feature_id: None,
            family: "x".into(),
            kind_id: "k".into(),
            difficulty: "easy".into(),
            tier: 1,
            effort_min: 10.0,
            mode: Mode::Walk,
            state,
            progress,
            anchor: None,
            target,
            fallback: false,
            boss: false,
            blurb: String::new(),
            reward: None,
            chain_id: None,
            collected: None,
            picked_at_ms: None,
        }
    }
    fn prog(q: &crate::game::QuestView) -> Progress {
        card_progress(q, 1_000_000, Imperial).expect("a progress block")
    }
    use crate::game::QuestState::{Done, InProgress, Open};

    #[test]
    fn timed_quests_count_minutes_and_seconds() {
        let dwell = Target::Dwell { p: home(), r: 40.0, minutes: 3.0 };
        let p = prog(&view(dwell.clone(), InProgress, 0.4));
        assert_eq!((p.label.as_str(), p.value.as_str(), p.hint), ("Time here", "1:12 of 3:00", None));
        assert_eq!(prog(&view(dwell.clone(), Open, 0.0)).value, "0:00 of 3:00");
        let done = prog(&view(dwell, Done, 1.0));
        assert_eq!((done.value.as_str(), done.frac), ("Done", 1.0));
        let area = Target::DwellArea { poly: vec![], center: home(), r: 60.0, minutes: 1.0 };
        assert_eq!(prog(&view(area, InProgress, 0.4)).label, "Time inside");
    }

    #[test]
    fn lines_count_distance_in_one_unit() {
        let p = prog(&view(line(4506.0, 0.5), InProgress, 0.4));
        assert_eq!((p.label.as_str(), p.value.as_str()), ("Traveled", "0.6 of 1.4 mi"));
        assert_eq!(prog(&view(line(200.0, 0.5), InProgress, 0.5)).value, "160 of 330 ft");
    }

    #[test]
    fn counts_steps_time_away_and_new_ground() {
        assert_eq!(prog(&view(Target::Steps { n: 2000 }, InProgress, 0.4)).value, "800 of 2,000");
        assert_eq!(prog(&view(Target::Away { minutes: 60.0 }, InProgress, 0.4)).value, "24 of 60 min");
        let cells = prog(&view(Target::Cells { n: 8, cell_m: 150.0 }, InProgress, 0.375));
        assert_eq!((cells.label.as_str(), cells.value.as_str()), ("New ground", "38%"));
    }

    #[test]
    fn forager_counts_what_is_banked_and_mentions_what_is_carried() {
        let mut q = view(Target::Collect { pts: vec![home()], need: 5, r: 25.0, theme: "acorns".into() }, InProgress, 0.2);
        q.collected = Some(crate::verify::Collected { picked: std::collections::BTreeSet::default(), carried: 2, banked: 1 });
        let p = prog(&q);
        assert_eq!((p.label.as_str(), p.value.as_str(), p.hint.as_deref()), ("Banked", "1 of 5 acorns", Some("Carrying 2")));
    }

    #[test]
    fn two_step_quests_track_their_step_and_the_delivery_clock() {
        let courier = Target::Courier { a: home(), b: home(), r: 40.0, time_limit_min: 10.0 };
        let open = prog(&view(courier.clone(), Open, 0.0));
        assert_eq!(
            (open.steps.clone(), open.step_at, open.hint.as_deref()),
            (vec!["Pick up".to_string(), "Deliver".to_string()], 0, Some("10 min to deliver once picked up"))
        );
        let mut carrying = view(courier.clone(), InProgress, 0.5);
        carrying.picked_at_ms = Some(1_000_000 - 200_000);
        let c = prog(&carrying);
        assert_eq!((c.step_at, c.hint.as_deref()), (1, Some("6:40 left to deliver")));
        // The limit passed but no fix has reset the tracker yet (standing still): no "0:00 left", back to step one.
        let mut late = view(courier.clone(), InProgress, 0.5);
        late.picked_at_ms = Some(1_000_000 - 11 * 60_000);
        let l = prog(&late);
        assert_eq!((l.step_at, l.hint.as_deref()), (0, Some("Time ran out: pick it up again")));
        // Time ran out: the tracker forgot the pick-up, so the card is back at step one with no stale clock.
        let timed_out = prog(&view(courier, InProgress, 0.0));
        assert_eq!((timed_out.step_at, timed_out.hint.as_deref()), (0, Some("10 min to deliver once picked up")));
        let rt = Target::RoundTrip { far: home(), r: 50.0 };
        let steps: Vec<u8> = [(Open, 0.0), (InProgress, 0.5), (Done, 1.0)].iter().map(|(s, p)| prog(&view(rt.clone(), *s, *p)).step_at).collect();
        assert_eq!(steps, [0, 1, 2]);
        assert_eq!(prog(&view(rt, Open, 0.0)).steps, ["Reach the far point", "Back home"]);
    }

    #[test]
    fn reaching_a_spot_has_no_progress_block() {
        assert!(card_progress(&view(Target::Point { p: home(), r: 40.0 }, Open, 0.0), 0, Imperial).is_none());
    }

    #[test]
    fn the_place_row_names_the_place_and_what_it_is() {
        let cat = crate::catalog::Catalog::builtin();
        let row = |k: &str, p: &str, t: Option<&str>, linked: bool| place_row(k, p, t, linked, &cat);
        assert_eq!(
            row("gallery_walls", "Allow Me", Some("Artwork"), true),
            Some(PlaceRow { name: Some("Allow Me".into()), kind: Some("Artwork".into()), quoted: true })
        );
        assert_eq!(
            row("bench_warmer", "Bench Warmer", Some("Bench"), true),
            Some(PlaceRow { name: None, kind: Some("Bench".into()), quoted: false }),
            "unnamed: the type alone"
        );
        assert_eq!(
            row("trail_boss", "Sensei Trail", None, false),
            Some(PlaceRow { name: Some("Sensei Trail".into()), kind: None, quoted: false }),
            "old game before relink: the name alone"
        );
        assert_eq!(row("step_up", "Anywhere", None, false), None, "no place");
        assert_eq!(row("the_big_one", "Liberty Park", Some("Park"), true).and_then(|r| r.name).as_deref(), Some("Liberty Park"), "a linked boss has a place");
        assert_eq!(row("the_big_one", "Around home", None, false), None);
        assert_eq!(row("hydrant_hunter", "Hydrant Hunter", None, true), None, "nothing to say: no empty row");
    }
}
