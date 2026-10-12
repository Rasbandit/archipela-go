//! Feed fixes, steps and headings through a `Locator` in time order, the way the phone does, with the shared odometer.

use std::sync::Arc;

use crate::catalog::Mode;
use crate::loc::bench::{Run, Shown};
use crate::loc::graph::StreetGraph;
use crate::loc::matcher::MatchOut;
use crate::loc::{DisplayPosition, Estimate, HeadingIn, LocParams, Locator, Odometer, RawFix};

/// How to replay.
#[derive(Debug, Clone)]
pub struct ReplayOpts {
    /// Travel mode of the zone.
    pub mode: Mode,
    /// Filter parameters.
    pub params: LocParams,
    /// Streets to match on; `None` replays without map matching.
    pub graph: Option<Arc<StreetGraph>>,
    /// How the phone hands the streams over; `None` delivers every event the moment it was taken.
    pub delivery: Option<Delivery>,
}

impl Default for ReplayOpts {
    fn default() -> Self {
        Self { mode: Mode::Walk, params: LocParams::default(), graph: None, delivery: None }
    }
}

/// Hardware batching per stream, ms (0: delivered at once): an event taken at `t` arrives at the next multiple of its batch, and
/// the locator sees events in arrival order, as on the phone (late batched fixes after newer steps and headings).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Delivery {
    /// Location fixes.
    pub fix_batch_ms: i64,
    /// Step counter readings.
    pub steps_batch_ms: i64,
    /// Compass readings.
    pub heading_batch_ms: i64,
}

// When an event taken at `t` reaches the app under a `batch_ms` delivery.
fn arrival(t: i64, batch_ms: i64) -> i64 {
    if batch_ms <= 0 {
        t
    } else {
        t.div_euclid(batch_ms) * batch_ms + if t.rem_euclid(batch_ms) == 0 { 0 } else { batch_ms }
    }
}

/// What a replay produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Replay {
    /// Shown positions, one per fix (and per bridged step batch).
    pub shown: Vec<Shown>,
    /// The estimates behind them.
    pub estimates: Vec<Estimate>,
    /// The matcher's answer after each estimate.
    pub matches: Vec<Option<MatchOut>>,
    /// What the map showed at each estimate's own time.
    pub displays: Vec<DisplayPosition>,
}

/// An estimate as a shown position.
#[must_use]
pub fn shown(e: &Estimate, odometer_m: f64) -> Shown {
    Shown { t_ms: e.t_ms, p: e.point(), uncertainty_m: e.uncertainty_m, accepted: e.accepted, verdict: e.verdict, course_deg: e.course_deg, odometer_m }
}

enum Ev {
    Steps(i64),
    Head(HeadingIn),
    Fix(RawFix),
}

/// Replay a recording through a fresh `Locator`. Events are merged by arrival ([`ReplayOpts::delivery`], by default their own time);
/// at equal arrival steps come first, then headings, then fixes.
#[must_use]
pub fn run_locator(fixes: &[RawFix], steps: &[(i64, i64)], headings: &[HeadingIn], opts: &ReplayOpts) -> Replay {
    let d = opts.delivery.unwrap_or_default();
    let mut evs: Vec<(i64, u8, i64, Ev)> = steps.iter().map(|(t, n)| (arrival(*t, d.steps_batch_ms), 0, *t, Ev::Steps(*n))).collect();
    evs.extend(headings.iter().map(|h| (arrival(h.t_ms, d.heading_batch_ms), 1, h.t_ms, Ev::Head(*h))));
    evs.extend(fixes.iter().map(|f| (arrival(f.t_ms, d.fix_batch_ms), 2, f.t_ms, Ev::Fix(*f))));
    // Arrival order; within one arrival, steps, then headings, then fixes, each stream oldest first.
    evs.sort_by_key(|(at, order, t, _)| (*at, *order, *t));
    let mut loc = Locator::new(opts.params.clone());
    loc.set_mode(opts.mode);
    loc.set_graph(opts.graph.clone());
    let (mut odo, mut total) = (Odometer::new(crate::loc::imm::mode_cap_mps(opts.mode)), 0.0);
    let mut out = Replay::default();
    for (_, _, t, ev) in evs {
        let est = match ev {
            Ev::Fix(f) => Some(loc.on_fix(&f)),
            Ev::Steps(n) => loc.on_steps(n, t, None),
            Ev::Head(h) => {
                loc.on_heading(&h);
                None
            }
        };
        if let Some(e) = est {
            total += odo.step_with(&e, loc.hold_relocation_speed());
            out.shown.push(shown(&e, total));
            out.estimates.push(e);
            out.matches.push(loc.matched());
            out.displays.push(loc.display(e.t_ms).unwrap_or_default());
        }
    }
    out
}

/// Replay a synthetic run.
#[must_use]
pub fn run_scenario(r: &Run, opts: &ReplayOpts) -> Replay {
    run_locator(&r.fixes, &r.steps, &r.headings, opts)
}

#[cfg(test)]
mod tests {
    use super::arrival;

    #[test]
    fn an_event_arrives_at_the_next_batch_boundary() {
        assert_eq!(arrival(12_345, 0), 12_345, "no batching: at once");
        assert_eq!(arrival(20_000, 20_000), 20_000, "on a boundary: with that batch");
        assert_eq!(arrival(20_001, 20_000), 40_000);
        assert_eq!(arrival(1, 5_000), 5_000);
        assert_eq!(arrival(-1, 5_000), 0, "times before the epoch round the same way");
    }
}
