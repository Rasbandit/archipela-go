//! What the map shows: the pin (predicted, matched, aged) and the matched trace.

use crate::geo::{destination, distance_m, Point};
use crate::loc::graph::{mode_mask, StreetGraph};
use crate::loc::matcher::{MatchOut, TraceDelta};
use crate::loc::params::LocParams;
use crate::loc::{DisplayPosition, DisplaySource, Estimate, HeadingSource, Motion, Source, ACC_TO_SIGMA};
use crate::num::i64_to_f64;

use super::Locator;

/// Whether the map shows the match (ruling T19-hyst, a display-only refinement of the spec's single 0.7): the segment shown, if any. A
/// street match within `max(match_show_min_m, 2 sigma)` of the estimate is shown from `match_show_confidence`, and stays shown while
/// its confidence is above `match_stay_confidence` (0.4, ruling T19-hyst2) on the same or a connected segment, within `match_stay_gate_factor` times that
/// distance. Never for a bridged position or off-network.
pub(super) fn show_match(prev: Option<usize>, m: Option<MatchOut>, est: &Estimate, graph: Option<&StreetGraph>, p: &LocParams) -> Option<usize> {
    let m = m.filter(|_| est.source != Source::Bridged)?;
    let seg = m.seg?;
    let gate = p.match_show_min_m.max(2.0 * est.uncertainty_m / ACC_TO_SIGMA);
    let d = distance_m(m.point, est.point());
    let connected = |a: usize, b: usize| {
        a == b
            || graph.is_some_and(|g| {
                a < g.segment_count() && b < g.segment_count() && {
                    let (sa, sb) = (g.seg(a), g.seg(b));
                    [sa.a, sa.b].iter().any(|n| *n == sb.a || *n == sb.b)
                }
            })
    };
    let enter = m.confidence >= p.match_show_confidence && d <= gate;
    let stay = prev.is_some_and(|s| connected(s, seg)) && m.confidence > p.match_stay_confidence && d <= p.match_stay_gate_factor * gate;
    (enter || stay).then_some(seg)
}

impl Locator {
    /// The matcher's newest answer.
    #[must_use]
    pub fn matched(&self) -> Option<MatchOut> {
        self.matcher.best()
    }

    /// The session's display line (matched where confident) as runs that break where the filter reset or relocated, ending in the
    /// matcher's provisional tail so it reaches the pin, and the time it starts. The app draws from [`Self::trace_since`] deltas; this
    /// whole line is kept for the tests, the bench and the full-load FFI call (`Engine::trace_matched`).
    #[must_use]
    pub fn trace_matched(&self) -> (Option<i64>, Vec<Vec<Point>>) {
        (self.matcher.trace_from_ms(), self.matcher.runs_with_tail())
    }

    /// What changed in [`Self::trace_matched`] since `cursor` (see [`Matcher::trace_since`](crate::loc::matcher::Matcher::trace_since)).
    #[must_use]
    pub fn trace_since(&self, cursor: u64) -> TraceDelta {
        self.matcher.trace_since(cursor)
    }

    /// What the map shows at `now_ms`: the newest estimate, predicted along its course at most 3 s ahead while moving and placed on the
    /// matched street while the match is shown (decided per fix with hysteresis, ruling T19-hyst; never for a bridged position), its
    /// uncertainty growing with age, aged into "predicted" and "stale", with the arrow from the course (while it is not stale) or else
    /// the compass (held flat and steady: standing, or moving with no course yet). `None` before the first fix.
    #[must_use]
    pub fn display(&self, now_ms: i64) -> Option<DisplayPosition> {
        // While the cloud runs, a fix the filter did not accept is no news for the pin (review I2: no flicker between the two).
        let est = match (self.last, self.bridged) {
            (Some(l), Some(b)) if b.t_ms > l.t_ms || (self.bridge.is_some() && !l.accepted) => b,
            (l, b) => l.or(b)?,
        };
        let p = &self.params;
        let age = now_ms.saturating_sub(est.t_ms).max(0);
        let moving = self.hold.is_none() && est.motion != Motion::Stationary;
        let ahead_s = if moving { i64_to_f64(age.min(p.display_predict_max_ms)) / 1000.0 } else { 0.0 };
        let matched = self.matcher.best().filter(|m| est.source != Source::Bridged && self.shown_seg.is_some() && m.seg == self.shown_seg);
        let ahead = match est.course_deg {
            Some(c) if ahead_s > 0.0 => destination(est.point(), c, est.speed_mps * ahead_s),
            _ => est.point(),
        };
        // On a confident match the pin is the current (predicted) estimate placed on the matched street (ruling T19-R2), so it glides
        // with the estimate instead of waiting for the matcher's next input.
        let shown = match (matched.and_then(|m| m.seg), &self.graph) {
            (Some(seg), Some(g)) if seg < g.segment_count() => g.along(seg, ahead, mode_mask(self.mode)),
            _ => ahead,
        };
        let bridged = est.source == Source::Bridged;
        let source = if age > p.stale_after_ms {
            DisplaySource::Stale
        } else if age > p.predicted_after_ms {
            DisplaySource::Predicted
        } else if bridged {
            DisplaySource::Bridged
        } else {
            DisplaySource::Gps
        };
        // Grows from age 0 (no step when the label turns "predicted"); a bridged position carries its own uncertainty while steps come
        // in, and grows once they stop (Task 22 note 2), from the moment it turns "predicted".
        let grown_ms = if bridged { age.saturating_sub(p.predicted_after_ms).max(0) } else { age };
        let grown = est.speed_sigma_mps.max(0.5) * i64_to_f64(grown_ms) / 1000.0;
        let (heading_deg, heading_source) = match est.course_deg {
            Some(c) if source != DisplaySource::Stale => (Some(c), HeadingSource::Course),
            _ => self.compass.held_flat_and_steady(now_ms, p).map_or((None, HeadingSource::None), |a| (Some(a), HeadingSource::Compass)),
        };
        Some(DisplayPosition {
            lat: shown.lat,
            lon: shown.lon,
            est_lat: est.lat,
            est_lon: est.lon,
            uncertainty_m: est.uncertainty_m + grown,
            speed_mps: est.speed_mps,
            course_deg: est.course_deg,
            heading_deg,
            heading_source,
            matched: matched.is_some(),
            match_confidence: matched.map_or(0.0, |m| m.confidence),
            source,
            age_ms: age,
            snap: self.restarted,
            bridged_origin: bridged,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use crate::catalog::Mode;
    use crate::geo::{destination, distance_m, Point};

    use crate::loc::graph::StreetGraph;

    use crate::loc::matcher::MatchOut;
    use crate::loc::params::LocParams;
    use crate::loc::{DisplaySource, Estimate, HeadingIn, HeadingSource, Motion, Provider, RawFix, Source, Verdict};
    use crate::num::i64_to_f64;

    use crate::loc::locator::test_util::*;
    use crate::loc::locator::Locator;

    #[test]
    fn nothing_is_shown_before_the_first_fix() {
        assert!(Locator::default().display(0).is_none());
    }

    #[test]
    fn the_shown_position_predicts_at_most_three_seconds_ahead_and_ages() {
        let mut l = Locator::default();
        for t in 0..=30 {
            l.on_fix(&fix(destination(o(), 90.0, 1.4 * i64_to_f64(t)), t, 4.0));
        }
        let e = l.last().unwrap();
        let d = |now_s: i64| l.display(now_s * 1000).unwrap();
        assert_eq!(d(30).source, DisplaySource::Gps);
        let ahead = distance_m(Point::new(d(32).lat, d(32).lon), e.point());
        assert!((ahead - 2.8).abs() < 0.6, "2 s at 1.4 m/s: {ahead}");
        let capped = distance_m(Point::new(d(40).lat, d(40).lon), e.point());
        assert!(capped < 1.4 * 3.0 + 0.6, "never more than 3 s ahead: {capped}");
        assert_eq!(d(37).source, DisplaySource::Predicted);
        assert!(d(37).uncertainty_m > e.uncertainty_m);
        assert_eq!(d(61).source, DisplaySource::Stale);
        assert_eq!(d(30).heading_source, HeadingSource::Course);
        assert!((d(30).heading_deg.unwrap() - 90.0).abs() < 10.0);
    }

    #[test]
    fn standing_still_shows_the_compass_only_when_held_flat() {
        let mut l = Locator::default();
        let t = stand(&mut l, 0, 40);
        assert_eq!(l.display(t * 1000).unwrap().heading_source, HeadingSource::None);
        for i in 0..5 {
            l.on_heading(&HeadingIn {
                t_ms: t * 1000 - 2000 + i * 500,
                azimuth_deg: 45.0,
                accuracy: crate::loc::CompassAccuracy::High,
                pitch_deg: 5.0,
                roll_deg: 5.0,
                error_deg: None,
            });
        }
        let d = l.display(t * 1000).unwrap();
        assert_eq!((d.heading_source, d.heading_deg.map(f64::round)), (HeadingSource::Compass, Some(45.0)));
    }

    #[test]
    fn a_reset_or_relocation_snaps_instead_of_gliding() {
        let mut l = Locator::default();
        l.on_fix(&fix(o(), 0, 5.0));
        assert!(l.display(0).unwrap().snap, "the first fix is a reset");
        l.on_fix(&fix(o(), 1, 5.0));
        assert!(!l.display(1000).unwrap().snap);
    }

    #[test]
    fn the_uncertainty_grows_without_a_step_at_the_predicted_threshold() {
        // Review M1: the circle grows from age 0, so crossing 6 s changes only the label.
        let walking = Estimate { motion: Motion::Walking, speed_mps: 1.4, course_deg: Some(90.0), ..Estimate::exact(o().lat, o().lon, 0) };
        let l = Locator { last: Some(walking), ..Locator::default() };
        let (below, above) = (l.display(5_999).unwrap(), l.display(6_001).unwrap());
        assert_eq!((below.source, above.source), (DisplaySource::Gps, DisplaySource::Predicted));
        assert!((above.uncertainty_m - below.uncertainty_m).abs() < 0.01, "{} -> {}", below.uncertainty_m, above.uncertainty_m);
        assert!(below.uncertainty_m > l.display(0).unwrap().uncertainty_m + 2.9, "0.5 m/s for 6 s");
        assert!((l.display(0).unwrap().uncertainty_m - 3.0).abs() < 1e-9, "a fresh fix keeps its own uncertainty");
    }

    #[test]
    fn a_simulated_teleport_snaps() {
        // Review M3: simulated fixes restart the filter with verdict Used; the pin must still jump.
        let mut l = Locator::default();
        l.on_fix(&fix(o(), 0, 5.0));
        l.on_fix(&fix(o(), 1, 5.0));
        assert!(!l.display(1000).unwrap().snap);
        l.on_fix(&RawFix { provider: Provider::Sim, ..fix(destination(o(), 0.0, 2000.0), 2, 5.0) });
        assert!(l.restarted() && l.display(2000).unwrap().snap, "a teleport jumps");
        l.on_fix(&RawFix { provider: Provider::Sim, ..fix(destination(o(), 0.0, 2001.0), 3, 5.0) });
        assert!(l.display(3000).unwrap().snap, "every simulated fix restarts the filter");
        l.on_fix(&fix(destination(o(), 0.0, 2001.0), 4, 5.0));
        l.on_fix(&fix(destination(o(), 0.0, 2001.0), 5, 5.0));
        assert!(!l.display(5000).unwrap().snap, "real fixes glide again");
    }

    #[test]
    fn the_prediction_follows_a_course_across_north() {
        // Review M8: courses of 359 and 1 degrees both predict north, not around the compass.
        for course in [359.0, 1.0, 0.0] {
            let walking = Estimate { motion: Motion::Walking, speed_mps: 2.0, course_deg: Some(course), ..Estimate::exact(o().lat, o().lon, 0) };
            let l = Locator { last: Some(walking), ..Locator::default() };
            let d = l.display(2_000).unwrap();
            let shown = Point::new(d.lat, d.lon);
            assert!((distance_m(shown, o()) - 4.0).abs() < 0.05, "{course}: 2 s at 2 m/s");
            assert!(crate::loc::heading::wrap_deg(crate::geo::bearing_deg(o(), shown) - course).abs() < 0.5, "{course}");
            assert_eq!(d.heading_deg, Some(course));
        }
    }

    #[test]
    fn moving_without_a_course_falls_back_to_a_steady_compass() {
        // Controller note 5: on sparse walks the course is often None; the compass draws the arrow then, under the same steadiness rules.
        let walking = Estimate { motion: Motion::Walking, speed_mps: 1.2, course_deg: None, ..Estimate::exact(o().lat, o().lon, 10_000) };
        let mut l = Locator { last: Some(walking), ..Locator::default() };
        assert_eq!(l.display(10_000).unwrap().heading_source, HeadingSource::None, "no compass yet");
        let head = |t_ms: i64, az: f64| HeadingIn {
            t_ms,
            azimuth_deg: az,
            accuracy: crate::loc::CompassAccuracy::Medium,
            pitch_deg: 20.0,
            roll_deg: 0.0,
            error_deg: None,
        };
        (0..5).for_each(|i| l.on_heading(&head(8_000 + i * 500, 300.0)));
        let d = l.display(10_000).unwrap();
        assert_eq!((d.heading_source, d.heading_deg.map(f64::round)), (HeadingSource::Compass, Some(300.0)));
        assert_eq!(d.course_deg, None);
        l.on_heading(&head(10_200, 200.0));
        assert_eq!(l.display(10_200).unwrap().heading_source, HeadingSource::None, "a swinging compass draws no arrow");
    }

    #[test]
    fn a_stale_course_is_no_arrow_and_bridged_positions_say_so() {
        let walking = Estimate { motion: Motion::Walking, speed_mps: 1.2, course_deg: Some(90.0), ..Estimate::exact(o().lat, o().lon, 0) };
        let mut l = Locator { last: Some(walking), ..Locator::default() };
        let d = l.display(31_000).unwrap();
        assert_eq!((d.source, d.heading_source, d.heading_deg, d.age_ms), (DisplaySource::Stale, HeadingSource::None, None, 31_000));
        assert!(d.uncertainty_m > 3.0 + 0.5 * 30.0, "grows at least 0.5 m/s: {}", d.uncertainty_m);
        l.last = Some(Estimate { source: Source::Bridged, ..Estimate::exact(o().lat, o().lon, 0) });
        let b = l.display(2_000).unwrap();
        assert_eq!(b.source, DisplaySource::Bridged);
        assert!((b.uncertainty_m - 3.0).abs() < 1e-9, "a bridged position carries its own uncertainty");
        assert_eq!(l.display(-5).unwrap().age_ms, 0, "a clock behind the estimate is age 0");
    }

    #[test]
    fn a_confident_match_moves_the_pin_onto_the_street_but_never_the_estimate() {
        let mut l = Locator::default();
        l.set_graph(StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 5, 100.0)).map(Arc::new));
        let street = destination(o(), 0.0, 100.0);
        for t in 0..60 {
            l.on_fix(&fix(destination(destination(street, 90.0, 1.4 * i64_to_f64(t)), 0.0, 4.0), t, 4.0));
        }
        let d = l.display(59_000).unwrap();
        assert!(d.matched && d.match_confidence >= 0.7, "{d:?}");
        assert!((d.lat - street.lat).abs() * 111_195.0 < 0.5, "pin on the street");
        assert!((d.est_lat - street.lat).abs() * 111_195.0 > 2.0, "the estimate stays where the filter put it");
        let (from, line) = l.trace_matched();
        assert!(from.is_some() && points(&line) >= 5);
    }

    #[test]
    fn far_from_the_streets_the_pin_stays_on_the_estimate() {
        let mut l = Locator::default();
        l.set_graph(StreetGraph::from_ways(&crate::loc::bench::grid_ways(o(), 5, 100.0)).map(Arc::new));
        // Through the middle of a block, 50 m from its streets at the centre and never nearer than 29 m: walking 42 m north from the
        // centre itself would end 8 m from the next street, where the spec's rule rightly shows the street.
        let mid = destination(destination(o(), 0.0, 150.0), 90.0, 150.0);
        for t in 0..30 {
            l.on_fix(&fix(destination(mid, 0.0, 1.4 * i64_to_f64(t) - 21.0), t, 4.0));
        }
        assert!(!l.display(29_000).unwrap().matched);
    }

    #[test]
    fn a_reset_a_relocation_a_new_mode_and_a_counting_toggle_restart_the_matcher() {
        // Controller note 1: the off-network state links every step, so only these restart the lattice.
        let lag = LocParams::default().match_lag;
        let mut l = Locator::default();
        l.set_graph(grid());
        let t = walk_the_street(&mut l, 0, 40);
        assert_eq!(undecided(&l), lag);
        assert!(l.matched().is_some_and(|m| m.seg.is_some()));
        // A gap over five minutes: Reset. The old lattice is decided into the trace, which then ends at the newest matched input
        // (without the restart it would end `lag` inputs earlier).
        let ends_at_newest = |l: &Locator, newest: Point| l.trace_matched().1.iter().any(|r| r.last().is_some_and(|q| distance_m(*q, newest) < 0.5));
        let newest = l.matched().unwrap().point;
        assert_eq!(l.on_fix(&fix(destination(o(), 0.0, 100.0), t + 400, 4.0)).verdict, Verdict::Reset);
        assert!(ends_at_newest(&l, newest), "the old lattice was decided into the trace");
        // Three far fixes: Relocated.
        let t = walk_the_street(&mut l, t + 401, 40);
        assert_eq!(undecided(&l), lag);
        let newest = l.matched().unwrap().point;
        let far = destination(o(), 90.0, 5000.0);
        let v: Vec<Verdict> = (0..3).map(|i| l.on_fix(&fix(far, t + i, 4.0)).verdict).collect();
        assert_eq!(v, [Verdict::Gated, Verdict::Gated, Verdict::Relocated]);
        assert!(ends_at_newest(&l, newest), "relocating decided the old lattice");
        assert!(l.matched().is_none_or(|m| distance_m(m.point, far) < 50.0), "nothing of the old street survives: {:?}", l.matched());
        // A new mode, then a counting toggle.
        let t = walk_the_street(&mut l, t + 3, 40);
        assert_eq!(undecided(&l), lag);
        l.set_mode(Mode::Drive);
        assert_eq!((undecided(&l), l.matched()), (0, None));
        let _ = walk_the_street(&mut l, t, 40);
        assert!(undecided(&l) > 0);
        l.reset();
        assert_eq!((undecided(&l), l.matched()), (0, None));
    }

    #[test]
    fn without_a_graph_nothing_is_matched_and_the_trace_is_the_estimates() {
        let mut l = Locator::default();
        walk_the_street(&mut l, 0, 30);
        assert!(l.matched().is_none() && !l.display(29_000).unwrap().matched);
        let (from, line) = l.trace_matched();
        assert!(from.is_some() && points(&line) >= 5);
    }

    #[test]
    fn a_bridged_position_is_never_moved_onto_a_street() {
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_the_street(&mut l, 0, 60);
        assert!(l.display(59_000).unwrap().matched);
        l.last = l.last.map(|e| Estimate { source: Source::Bridged, ..e });
        assert!(!l.display(59_000).unwrap().matched);
    }

    #[test]
    fn walking_a_street_the_matched_pin_glides_with_the_estimate() {
        // Ruling T19-R2: the pin is the current estimate projected onto the matched street, so it advances on every display call, also
        // through crossings, instead of hopping from one matcher input to the next.
        // 2 m beside the street: at 3 m the matcher's confidence dips just under 0.7 a few metres past a crossing (Task 18 behaviour,
        // reported apart), and the pin then rightly drops back to the estimate.
        let mut l = Locator::default();
        l.set_graph(grid());
        let street = destination(o(), 0.0, 100.0);
        let mut last: Option<Point> = None;
        for t in 0..150 {
            l.on_fix(&fix(destination(destination(street, 90.0, 1.4 * i64_to_f64(t)), 180.0, 2.0), t, 4.0));
            let d = l.display(t * 1000).unwrap();
            let at = Point::new(d.lat, d.lon);
            if t >= 20 {
                assert!(d.matched, "t {t}: {d:?} {:?}", l.matched());
                let step = distance_m(last.unwrap(), at);
                assert!(step > 0.0 && step <= 2.0, "t {t}: a {step} m step");
            }
            last = Some(at);
        }
    }

    #[test]
    fn the_trace_breaks_where_the_filter_relocates_but_not_on_a_new_mode() {
        // Ruling T19-R3: no straight connector where the player reappeared elsewhere.
        let mut l = Locator::default();
        l.set_graph(grid());
        let t = walk_the_street(&mut l, 0, 40);
        let far = destination(o(), 90.0, 5000.0);
        for i in 0..3 {
            l.on_fix(&fix(far, t + i, 4.0));
        }
        assert_eq!(l.last().unwrap().verdict, Verdict::Relocated);
        for i in 3..30 {
            l.on_fix(&fix(destination(far, 0.0, 1.4 * i64_to_f64(i)), t + i, 4.0));
        }
        l.set_mode(Mode::Run);
        for i in 30..60 {
            l.on_fix(&fix(destination(far, 0.0, 1.4 * i64_to_f64(i)), t + i, 4.0));
        }
        let (_, runs) = l.trace_matched();
        assert_eq!(runs.len(), 2, "{runs:?}");
        assert!(runs.iter().all(|r| r.len() >= 2));
        assert!(runs[0].iter().all(|p| distance_m(*p, o()) < 600.0) && runs[1].iter().all(|p| distance_m(*p, far) < 200.0));
    }

    #[test]
    fn the_drawn_trace_reaches_the_pin() {
        // Ruling T19-R4: with the provisional tail the line ends at the newest matcher input, at most one input step behind the pin.
        let mut l = Locator::default();
        l.set_graph(grid());
        let t = walk_the_street(&mut l, 0, 60);
        let d = l.display((t - 1) * 1000).unwrap();
        let (_, runs) = l.trace_matched();
        let end = *runs.last().and_then(|r| r.last()).unwrap();
        let gap = distance_m(end, Point::new(d.lat, d.lon));
        assert!(gap <= l.params().match_min_move_m + 1.0, "the line ends {gap} m behind the pin");
    }

    #[test]
    fn on_a_bendy_street_of_short_segments_the_pin_glides() {
        // Review I1: the matched segment is the last matcher input's, up to 5 m behind; the pin goes on across several short segments.
        let street = bendy_street();
        let way = crate::scan::WayGeom { id: 1, class: crate::scan::way_class::FOOT, pts: street.clone() };
        let mut l = Locator::default();
        l.set_graph(StreetGraph::from_ways(&[way]).map(Arc::new));
        let mut last: Option<Point> = None;
        for t in 0..300 {
            l.on_fix(&fix(at_along(&street, 1.4 * i64_to_f64(t)), t, 4.0));
            let d = l.display(t * 1000).unwrap();
            let at = Point::new(d.lat, d.lon);
            if t >= 20 {
                assert!(d.matched, "t {t}: {d:?} {:?}", l.matched());
                let step = distance_m(last.unwrap(), at);
                assert!(step <= 2.0, "t {t}: a {step} m step");
            }
            last = Some(at);
        }
    }

    #[test]
    fn the_shown_match_enters_at_0_7_and_stays_through_a_dip_on_the_same_or_a_connected_street() {
        // Rulings T19-hyst, hyst2: enter at >= 0.7 within the gate; stay above 0.4 on the same or a connected segment within 1.5 times
        // the gate.
        let g = grid().unwrap();
        let p = LocParams::default();
        let row = destination(o(), 0.0, 100.0);
        let seg = g.candidates(destination(row, 90.0, 50.0), 5.0, 1, way_class_foot())[0].seg;
        let next = g.candidates(destination(row, 90.0, 150.0), 5.0, 1, way_class_foot())[0].seg; // shares the x = 100 m node
        let far = g.candidates(destination(destination(o(), 0.0, 300.0), 90.0, 350.0), 5.0, 1, way_class_foot())[0].seg;
        let on = destination(row, 90.0, 50.0);
        let est = |side_m: f64| Estimate { uncertainty_m: 3.0, ..Estimate::exact(destination(on, 180.0, side_m).lat, on.lon, 0) };
        let m = |seg: usize, confidence: f64| Some(MatchOut { point: on, confidence, seg: Some(seg) });
        let show = |prev: Option<usize>, out: Option<MatchOut>, side_m: f64| show_match(prev, out, &est(side_m), Some(&g), &p);
        assert_eq!(show(None, m(seg, 0.7), 2.0), Some(seg), "enters at 0.7");
        assert_eq!(show(None, m(seg, 0.6), 2.0), None, "does not enter at 0.6");
        assert_eq!(show(Some(seg), m(seg, 0.6), 2.0), Some(seg), "a dip to 0.6 keeps the pin on the street");
        assert_eq!(show(Some(seg), m(seg, 0.45), 2.0), Some(seg), "a dip to 0.45 at a crossing keeps it");
        assert_eq!(show(Some(seg), m(seg, 0.4), 2.0), None, "a drop to 0.4 leaves");
        assert_eq!(show(Some(seg), m(next, 0.6), 2.0), Some(next), "a connected segment keeps it");
        assert_eq!(show(Some(seg), m(far, 0.6), 2.0), None, "an unconnected one does not");
        assert_eq!(show(Some(seg), m(seg, 0.6), 0.0), Some(seg));
        assert_eq!(show(None, m(seg, 0.9), 12.0), None, "outside the 10 m gate it does not enter ...");
        assert_eq!(show(Some(seg), m(seg, 0.9), 12.0), Some(seg), "... but a shown match stays within 15 m");
        assert_eq!(show(Some(seg), m(seg, 0.9), 16.0), None, "and leaves past 15 m");
        assert_eq!(show(Some(seg), Some(MatchOut { point: on, confidence: 0.9, seg: None }), 2.0), None, "off-network is never shown");
        let bridged = Estimate { source: Source::Bridged, ..est(2.0) };
        assert_eq!(show_match(Some(seg), m(seg, 0.9), &bridged, Some(&g), &p), None);
    }

    #[test]
    fn a_new_graph_forgets_the_shown_match() {
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_the_street(&mut l, 0, 40);
        assert!(l.display(39_000).unwrap().matched);
        assert!(l.shown_seg.is_some());
        l.set_graph(grid()); // a rebuilt graph: its segment ids are new
        assert!(l.shown_seg.is_none() && !l.display(39_000).unwrap().matched);
    }

    #[test]
    fn a_matcher_restart_forgets_the_shown_match() {
        // Task 19 re-review: after any restart the pin must re-enter at 0.7, not stay on through the hysteresis.
        let mut l = Locator::default();
        l.set_graph(grid());
        walk_the_street(&mut l, 0, 40);
        assert!(l.shown_seg.is_some());
        l.reset();
        assert!(l.shown_seg.is_none(), "a reset forgets it");
        let t = walk_the_street(&mut l, 40, 40);
        assert!(l.shown_seg.is_some());
        l.set_mode(Mode::Drive); // other streets
        assert!(l.shown_seg.is_none(), "a new mode forgets it");
        walk_the_street(&mut l, t, 5);
        l.set_mode(Mode::Drive);
        l.shown_seg = Some(0);
        l.set_mode(Mode::Drive); // the same mode restarts nothing
        assert_eq!(l.shown_seg, Some(0), "the same mode keeps it");
    }

    #[test]
    fn a_counting_toggle_restarts_the_matcher_but_keeps_one_line() {
        // Review M7: pausing counting (presence rules) is no jump; the player is where they were.
        let mut l = Locator::default();
        l.set_graph(grid());
        let t = walk_the_street(&mut l, 0, 30);
        l.reset();
        assert!(l.matched().is_none());
        walk_the_street(&mut l, t, 30);
        assert_eq!(l.trace_matched().1.len(), 1);
    }

    #[test]
    fn a_dev_walk_draws_one_line_and_real_gps_after_it_starts_another() {
        // Review M6: each simulated fix resets the filter, but the dev walk is one line; the real fix after it may be far away.
        let mut l = Locator::default();
        l.set_graph(grid());
        let street = destination(o(), 0.0, 100.0);
        for t in 0..30 {
            let at = destination(street, 90.0, 1.4 * i64_to_f64(t));
            l.on_fix(&RawFix { provider: Provider::Sim, ..fix(at, t, 3.0) });
        }
        let (_, runs) = l.trace_matched();
        assert_eq!(runs.len(), 1, "{runs:?}");
        assert!(runs[0].len() >= 5, "the dev walk draws: {runs:?}");
        let far = destination(o(), 90.0, 3000.0);
        for t in 30..40 {
            l.on_fix(&fix(destination(far, 0.0, 1.4 * i64_to_f64(t)), t, 4.0));
        }
        assert_eq!(l.trace_matched().1.len(), 2);
    }

    #[test]
    fn at_one_hertz_the_pin_stays_on_the_street_through_every_crossing() {
        // Ruling T19-hyst2: at 1 Hz the crossing is ambiguous for an input or two (confidence about 0.48 seen); the pin stays on.
        for side_m in [0.0, 2.0, 3.0] {
            let mut l = Locator::default();
            l.set_graph(grid());
            let street = destination(o(), 0.0, 100.0);
            for t in 0..280 {
                l.on_fix(&fix(destination(destination(street, 90.0, 1.4 * i64_to_f64(t)), 180.0, side_m), t, 4.0));
                if t >= 20 {
                    let d = l.display(t * 1000).unwrap();
                    assert!(d.matched, "{side_m} m beside, t {t} ({:.0} m along): {:?}", 1.4 * i64_to_f64(t), l.matched());
                }
            }
        }
    }

    #[test]
    fn standing_over_two_minutes_keeps_one_line_but_losing_gps_that_long_breaks_it() {
        // Fix round 3: the 2 min gap counts from the last accepted estimate, not the last matcher input (standing feeds none).
        let street = destination(o(), 0.0, 100.0);
        let walk = |l: &mut Locator, from_s: i64, at_m: f64, n: i64| {
            for k in 0..n {
                l.on_fix(&fix(destination(street, 90.0, at_m + 1.4 * i64_to_f64(k)), from_s + k, 4.0));
            }
        };
        let mut l = Locator::default();
        l.set_graph(grid());
        walk(&mut l, 0, 0.0, 30);
        let here = destination(street, 90.0, 42.0);
        for k in 0..180 {
            l.on_fix(&fix(destination(here, f64::from(u16::try_from(k * 97 % 360).unwrap()), 1.0), 30 + k, 4.0));
        }
        walk(&mut l, 210, 42.0, 30);
        assert_eq!(l.trace_matched().1.len(), 1, "standing 3 min is one line");
        let mut l = Locator::default();
        l.set_graph(grid());
        walk(&mut l, 0, 0.0, 30);
        walk(&mut l, 210, 42.0, 30); // no fix for 3 min
        assert_eq!(l.trace_matched().1.len(), 2, "3 min without GPS breaks it");
    }

    #[test]
    fn a_dev_teleport_breaks_the_line_but_a_dev_walk_does_not() {
        // Fix round 3: a simulated fix over 50 m from the previous position is a teleport.
        let mut l = Locator::default();
        l.set_graph(grid());
        let sim = |l: &mut Locator, at: Point, t: i64| {
            l.on_fix(&RawFix { provider: Provider::Sim, ..fix(at, t, 3.0) });
        };
        let street = destination(o(), 0.0, 100.0);
        for t in 0..20 {
            sim(&mut l, destination(street, 90.0, 2.5 * i64_to_f64(t)), t); // a quick dev walk: 2.5 m per fix
        }
        assert_eq!(l.trace_matched().1.len(), 1);
        let away = destination(o(), 0.0, 300.0);
        for t in 20..40 {
            sim(&mut l, destination(away, 90.0, 2.5 * i64_to_f64(t - 20)), t);
        }
        let (_, runs) = l.trace_matched();
        assert_eq!(runs.len(), 2, "{runs:?}");
        assert!(runs[1].iter().all(|p| distance_m(*p, away) < 60.0));
    }
}
