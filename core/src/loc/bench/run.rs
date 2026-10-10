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
}

impl Default for ReplayOpts {
    fn default() -> Self {
        Self { mode: Mode::Walk, params: LocParams::default(), graph: None }
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

/// Replay a recording through a fresh `Locator`. Events are merged by time; at equal times steps come first, then headings, then the fix.
#[must_use]
pub fn run_locator(fixes: &[RawFix], steps: &[(i64, i64)], headings: &[HeadingIn], opts: &ReplayOpts) -> Replay {
    let mut evs: Vec<(i64, u8, Ev)> = steps.iter().map(|(t, n)| (*t, 0, Ev::Steps(*n))).collect();
    evs.extend(headings.iter().map(|h| (h.t_ms, 1, Ev::Head(*h))));
    evs.extend(fixes.iter().map(|f| (f.t_ms, 2, Ev::Fix(*f))));
    evs.sort_by_key(|(t, order, _)| (*t, *order));
    let mut loc = Locator::new(opts.params.clone());
    loc.set_mode(opts.mode);
    loc.set_graph(opts.graph.clone());
    let (mut odo, mut total) = (Odometer::new(crate::loc::imm::mode_cap_mps(opts.mode)), 0.0);
    let mut out = Replay::default();
    for (t, _, ev) in evs {
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
