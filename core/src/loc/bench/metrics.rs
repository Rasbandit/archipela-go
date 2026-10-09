//! Scorecard metrics of a shown track against a reference (the truth of a synthetic walk, or the smoothed track of a real one).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::geo::{bearing_deg, distance_m, distance_to_segment_m, Point};
use crate::loc::bench::{truth_at, TruthPoint};
use crate::loc::Verdict;
use crate::num::{count_f64, i64_to_f64};

/// One position as a filter (or the legacy rules) showed it after a fix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown {
    /// Time, Unix ms.
    pub t_ms: i64,
    /// Shown position.
    pub p: Point,
    /// Its 68 % radius, metres.
    pub uncertainty_m: f64,
    /// Whether quests could use it.
    pub accepted: bool,
    /// What became of the fix.
    pub verdict: Verdict,
    /// Shown course, degrees.
    pub course_deg: Option<f64>,
    /// Odometer after this fix, metres.
    pub odometer_m: f64,
}

/// How still the shown position stays while the reference stands still.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jitter {
    /// RMS distance from each window's median position, metres.
    pub rms_m: f64,
    /// Shown path length per minute standing, metres.
    pub path_m_per_min: f64,
    /// Worst first-to-last drift of a window per minute, metres.
    pub drift_m_per_min: f64,
    /// Share of consecutive shown positions that did not move at all.
    pub held_share: f64,
}

/// The `q` quantile (0..1, linear between ranks) of `v`; NaN when empty.
#[must_use]
pub fn percentile(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    let mut sorted = v.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = q.clamp(0.0, 1.0) * count_f64(sorted.len() - 1);
    let (lo, hi) = (rank.floor(), rank.ceil());
    let (below, above) = (sorted[crate::num::floor_usize(lo)], sorted[crate::num::floor_usize(hi)]);
    below + (above - below) * (rank - lo)
}

/// The reference position at `t_ms`, linear between its points.
#[must_use]
pub fn interp(reference: &[TruthPoint], t_ms: i64) -> Point {
    let i = reference.partition_point(|t| t.t_ms <= t_ms);
    if i == 0 || i >= reference.len() {
        return truth_at(reference, t_ms).p;
    }
    let (a, b) = (reference[i - 1], reference[i]);
    let f = i64_to_f64(t_ms - a.t_ms) / i64_to_f64((b.t_ms - a.t_ms).max(1));
    Point::new(a.p.lat + (b.p.lat - a.p.lat) * f, a.p.lon + (b.p.lon - a.p.lon) * f)
}

/// RMS and 95th percentile of the distance from each shown position to the reference at the same time, metres.
#[must_use]
pub fn position_error(reference: &[TruthPoint], shown: &[Shown]) -> (f64, f64) {
    let e: Vec<f64> = shown.iter().map(|s| distance_m(s.p, interp(reference, s.t_ms))).collect();
    let rms = (e.iter().map(|x| x * x).sum::<f64>() / count_f64(e.len().max(1))).sqrt();
    (rms, percentile(&e, 0.95))
}

/// Shown steps longer than `max(15 m, 3 x (reference speed x dt + uncertainty))` while the reference moves slower than 2 m/s.
#[must_use]
pub fn false_jumps(reference: &[TruthPoint], shown: &[Shown]) -> usize {
    shown
        .windows(2)
        .filter(|w| {
            let ref_speed = truth_at(reference, w[1].t_ms).speed_mps;
            let dt = i64_to_f64(w[1].t_ms - w[0].t_ms) / 1000.0;
            ref_speed < 2.0 && distance_m(w[0].p, w[1].p) > (3.0 * (ref_speed * dt + w[1].uncertainty_m)).max(15.0)
        })
        .count()
}

/// Time ranges (ms) of at least 20 s in which the reference does not move.
fn still_windows(reference: &[TruthPoint]) -> Vec<(i64, i64)> {
    let mut out = Vec::new();
    let mut start: Option<i64> = None;
    for t in reference {
        // 0.2 m/s: a smoothed real track never reads exactly 0 while the player stands.
        match (t.speed_mps < 0.2, start) {
            (true, None) => start = Some(t.t_ms),
            (false, Some(s)) => {
                if t.t_ms - s >= 20_000 {
                    out.push((s, t.t_ms));
                }
                start = None;
            }
            _ => {}
        }
    }
    if let (Some(s), Some(last)) = (start, reference.last()) {
        if last.t_ms - s >= 20_000 {
            out.push((s, last.t_ms));
        }
    }
    out
}

fn median_point(ps: &[Point]) -> Point {
    let lat: Vec<f64> = ps.iter().map(|p| p.lat).collect();
    let lon: Vec<f64> = ps.iter().map(|p| p.lon).collect();
    Point::new(percentile(&lat, 0.5), percentile(&lon, 0.5))
}

/// Jitter of the shown position over every still window of the reference; `None` if the reference never stands still for 20 s.
#[must_use]
pub fn stationary_jitter(reference: &[TruthPoint], shown: &[Shown]) -> Option<Jitter> {
    let (mut sq, mut n, mut path, mut minutes, mut held, mut pairs, mut drift) = (0.0, 0usize, 0.0, 0.0, 0usize, 0usize, 0.0_f64);
    for (a, b) in still_windows(reference) {
        let w: Vec<&Shown> = shown.iter().filter(|s| (a..=b).contains(&s.t_ms)).collect();
        if w.len() < 2 {
            continue;
        }
        let ps: Vec<Point> = w.iter().map(|s| s.p).collect();
        let m = median_point(&ps);
        sq += ps.iter().map(|p| distance_m(*p, m).powi(2)).sum::<f64>();
        n += ps.len();
        let len: f64 = ps.windows(2).map(|x| distance_m(x[0], x[1])).sum();
        let mins = i64_to_f64(b - a) / 60_000.0;
        path += len;
        minutes += mins;
        held += ps.windows(2).filter(|x| x[0] == x[1]).count();
        pairs += ps.len() - 1;
        drift = drift.max(distance_m(ps[0], ps[ps.len() - 1]) / mins);
    }
    (n > 0).then(|| Jitter {
        rms_m: (sq / count_f64(n)).sqrt(),
        path_m_per_min: path / minutes,
        drift_m_per_min: drift,
        held_share: count_f64(held) / count_f64(pairs.max(1)),
    })
}

/// Seconds from the reference entering the circle (`target`, `r`) to the first accepted shown position inside it (negative = early). `None`
/// when the reference never enters; infinity when the shown track never does.
#[must_use]
pub fn arrival_lag_s(reference: &[TruthPoint], shown: &[Shown], target: Point, r: f64) -> Option<f64> {
    let t_ref = reference.iter().find(|t| distance_m(t.p, target) <= r)?.t_ms;
    Some(shown.iter().find(|s| s.accepted && distance_m(s.p, target) <= r).map_or(f64::INFINITY, |s| i64_to_f64(s.t_ms - t_ref) / 1000.0))
}

/// Whether an accepted shown position ever enters the circle: the quest would complete.
#[must_use]
pub fn completes(shown: &[Shown], target: Point, r: f64) -> bool {
    shown.iter().any(|s| s.accepted && distance_m(s.p, target) <= r)
}

fn wrap_deg(d: f64) -> f64 {
    (d + 540.0).rem_euclid(360.0) - 180.0
}

/// Turns of the reference (course change over 60 degrees between the 10 m before and the 10 m after a point): (time, point, new course).
/// Neighbouring candidates within 20 m form one cluster, and the point with the largest course change (the corner) represents it.
fn turns(reference: &[TruthPoint]) -> Vec<(i64, Point, f64)> {
    let mut out: Vec<(i64, Point, f64)> = Vec::new();
    let mut best_change = 0.0_f64;
    for (i, t) in reference.iter().enumerate() {
        let before = reference[..i].iter().rev().find(|b| distance_m(b.p, t.p) >= 10.0);
        let after = reference[i + 1..].iter().find(|a| distance_m(a.p, t.p) >= 10.0);
        let (Some(b), Some(a)) = (before, after) else { continue };
        let (c_in, c_out) = (bearing_deg(b.p, t.p), bearing_deg(t.p, a.p));
        let change = wrap_deg(c_out - c_in).abs();
        if change <= 60.0 {
            continue;
        }
        match out.last_mut() {
            Some(last) if distance_m(last.1, t.p) <= 20.0 => {
                if change > best_change {
                    *last = (t.t_ms, t.p, c_out);
                    best_change = change;
                }
            }
            _ => {
                out.push((t.t_ms, t.p, c_out));
                best_change = change;
            }
        }
    }
    out
}

/// Seconds from each turn of the reference until the shown course is within 20 degrees of the new course (infinity if never).
#[must_use]
pub fn turn_lags_s(reference: &[TruthPoint], shown: &[Shown]) -> Vec<f64> {
    turns(reference)
        .into_iter()
        .map(|(t, _, c)| {
            shown
                .iter()
                .filter(|s| s.t_ms >= t)
                .find(|s| s.course_deg.is_some_and(|sc| wrap_deg(sc - c).abs() <= 20.0))
                .map_or(f64::INFINITY, |s| i64_to_f64(s.t_ms - t) / 1000.0)
        })
        .collect()
}

/// Worst distance, in the 10 s after any turn, of a shown position from the reference path after the turn, metres.
#[must_use]
pub fn overshoot_m(reference: &[TruthPoint], shown: &[Shown]) -> f64 {
    let mut worst = 0.0_f64;
    for (t, p, _) in turns(reference) {
        let out_end = interp(reference, t + 20_000);
        for s in shown.iter().filter(|s| (t..=t + 10_000).contains(&s.t_ms)) {
            worst = worst.max(distance_to_segment_m(s.p, p, out_end));
        }
    }
    worst
}

/// How many fixes got each verdict, by name.
#[must_use]
pub fn verdict_counts(shown: &[Shown]) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for s in shown {
        *m.entry(format!("{:?}", s.verdict)).or_insert(0) += 1;
    }
    m
}

/// Virtual targets of radius `r` every `every_m` metres along the reference.
#[must_use]
pub fn virtual_targets(reference: &[TruthPoint], every_m: f64, r: f64) -> Vec<(Point, f64)> {
    let mut out = Vec::new();
    let mut since = 0.0;
    for w in reference.windows(2) {
        since += distance_m(w[0].p, w[1].p);
        if since >= every_m {
            out.push((w[1].p, r));
            since = 0.0;
        }
    }
    out
}

/// The scorecard of one shown track.
#[derive(Debug, Clone, PartialEq)]
pub struct Scorecard {
    /// Which filter.
    pub name: String,
    /// Fixes fed.
    pub fixes: usize,
    /// RMS and p95 position error, metres.
    pub error_m: (f64, f64),
    /// False jumps.
    pub false_jumps: usize,
    /// Standing-still jitter.
    pub jitter: Option<Jitter>,
    /// Arrival lags at targets the shown track reached, seconds.
    pub arrival_lag_s: Vec<f64>,
    /// Targets the reference reached and the shown track never did.
    pub missed: usize,
    /// Turn lags, seconds.
    pub turn_lag_s: Vec<f64>,
    /// Verdict counts.
    pub verdicts: BTreeMap<String, usize>,
    /// Final odometer, metres.
    pub odometer_m: f64,
}

/// Score `shown` against `reference` with `targets` (quests, plus virtual targets if the caller adds them).
#[must_use]
pub fn score(name: &str, reference: &[TruthPoint], shown: &[Shown], targets: &[(Point, f64)]) -> Scorecard {
    let lags: Vec<Option<f64>> = targets.iter().map(|(p, r)| arrival_lag_s(reference, shown, *p, *r)).collect();
    Scorecard {
        name: name.to_string(),
        fixes: shown.len(),
        error_m: position_error(reference, shown),
        false_jumps: false_jumps(reference, shown),
        jitter: stationary_jitter(reference, shown),
        arrival_lag_s: lags.iter().flatten().copied().filter(|l| l.is_finite()).collect(),
        missed: lags.iter().flatten().filter(|l| l.is_infinite()).count(),
        turn_lag_s: turn_lags_s(reference, shown).into_iter().filter(|l| l.is_finite()).collect(),
        verdicts: verdict_counts(shown),
        odometer_m: shown.last().map_or(0.0, |s| s.odometer_m),
    }
}

fn rows(s: &Scorecard) -> Vec<(String, String)> {
    let j = s.jitter;
    vec![
        ("fixes".into(), s.fixes.to_string()),
        ("error rms / p95 m".into(), format!("{:.1} / {:.1}", s.error_m.0, s.error_m.1)),
        ("false jumps".into(), s.false_jumps.to_string()),
        ("jitter rms m".into(), j.map_or("-".into(), |j| format!("{:.2}", j.rms_m))),
        ("standing path m/min".into(), j.map_or("-".into(), |j| format!("{:.1}", j.path_m_per_min))),
        ("held share".into(), j.map_or("-".into(), |j| format!("{:.2}", j.held_share))),
        ("arrival lag p50 / p90 s".into(), format!("{:.1} / {:.1}", percentile(&s.arrival_lag_s, 0.5), percentile(&s.arrival_lag_s, 0.9))),
        ("missed arrivals".into(), s.missed.to_string()),
        ("turn lag p50 s".into(), format!("{:.1}", percentile(&s.turn_lag_s, 0.5))),
        ("verdicts".into(), format!("{:?}", s.verdicts)),
        ("odometer m".into(), format!("{:.0}", s.odometer_m)),
    ]
}

/// Scorecards as aligned columns, one per card, in order.
#[must_use]
pub fn columns(cards: &[&Scorecard]) -> String {
    let mut out = format!("{:<26}", "metric");
    for c in cards {
        let _ = write!(out, "{:<36}", c.name);
    }
    out.push('\n');
    let all: Vec<Vec<(String, String)>> = cards.iter().map(|c| rows(c)).collect();
    for (i, (k, _)) in all.first().map(Vec::as_slice).unwrap_or_default().iter().enumerate() {
        let _ = write!(out, "{k:<26}");
        for r in &all {
            let _ = write!(out, "{:<36}", r[i].1);
        }
        out.push('\n');
    }
    out
}

/// Map matching on a replay: share of positions shown on a street, segment changes per minute, share off-network.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchingStats {
    /// Share of displays with a shown match, each display sampled at its estimate's own time (age 0: no prediction ahead, review M5).
    pub matched_share: f64,
    /// Segment changes of the match per minute.
    pub switches_per_min: f64,
    /// Share of matches that are off-network.
    pub off_share: f64,
}

/// [`MatchingStats`] of a replay.
#[must_use]
pub fn matching_stats(r: &crate::loc::bench::Replay) -> MatchingStats {
    let n = count_f64(r.displays.len().max(1));
    let matched = count_f64(r.displays.iter().filter(|d| d.matched).count());
    let segs: Vec<Option<usize>> = r.matches.iter().flatten().map(|m| m.seg).collect();
    let switches = count_f64(segs.windows(2).filter(|w| w[0] != w[1]).count());
    let minutes = match (r.shown.first(), r.shown.last()) {
        (Some(a), Some(b)) => (i64_to_f64(b.t_ms - a.t_ms) / 60_000.0).max(1.0 / 60.0),
        _ => 1.0,
    };
    MatchingStats {
        matched_share: matched / n,
        switches_per_min: switches / minutes,
        off_share: count_f64(segs.iter().filter(|s| s.is_none()).count()) / count_f64(segs.len().max(1)),
    }
}

/// Bridging at the end of a gap: the error of the last bridged position against the truth at its time, and the display jump to the first
/// GPS estimate at or after `gap_end_ms`. `None` if nothing was bridged or GPS never came back.
#[must_use]
pub fn reanchor(truth: &[TruthPoint], r: &crate::loc::bench::Replay, gap_end_ms: i64) -> Option<(f64, f64)> {
    let last_b = r.estimates.iter().rfind(|e| e.source == crate::loc::Source::Bridged && e.t_ms <= gap_end_ms)?;
    let first_after = r.estimates.iter().find(|e| e.t_ms >= gap_end_ms && e.source == crate::loc::Source::Gps)?;
    Some((distance_m(last_b.point(), interp(truth, last_b.t_ms)), distance_m(last_b.point(), first_after.point())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::destination;

    fn line(n: i64, speed: f64) -> Vec<TruthPoint> {
        let o = Point::new(40.0, -111.0);
        (0..n).map(|i| TruthPoint { t_ms: i * 1000, p: destination(o, 90.0, speed * i64_to_f64(i)), speed_mps: speed, course_deg: 90.0 }).collect()
    }

    fn shown_from(reference: &[TruthPoint], delay_s: i64) -> Vec<Shown> {
        reference
            .iter()
            .map(|t| Shown {
                t_ms: t.t_ms,
                p: truth_at(reference, t.t_ms - delay_s * 1000).p,
                uncertainty_m: 3.0,
                accepted: true,
                verdict: Verdict::Used,
                course_deg: Some(90.0),
                odometer_m: 0.0,
            })
            .collect()
    }

    #[test]
    fn a_perfect_track_has_no_error_no_jumps_and_no_lag() {
        let r = line(120, 1.4);
        let s = shown_from(&r, 0);
        let (rms, p95) = position_error(&r, &s);
        assert!(rms < 1e-6 && p95 < 1e-6);
        assert_eq!(false_jumps(&r, &s), 0);
        assert_eq!(arrival_lag_s(&r, &s, r[80].p, 25.0), Some(0.0));
    }

    #[test]
    fn a_shown_track_three_seconds_behind_arrives_three_seconds_late() {
        let r = line(120, 1.4);
        let lag = arrival_lag_s(&r, &shown_from(&r, 3), r[80].p, 25.0).unwrap();
        assert!((lag - 3.0).abs() < 1e-9, "{lag}");
    }

    #[test]
    fn a_target_the_reference_never_reaches_has_no_lag_and_one_never_shown_is_infinite() {
        let r = line(60, 1.4);
        let far = destination(r[0].p, 0.0, 500.0);
        assert_eq!(arrival_lag_s(&r, &shown_from(&r, 0), far, 25.0), None);
        let mut s = shown_from(&r, 0);
        for x in &mut s {
            x.accepted = false;
        }
        assert_eq!(arrival_lag_s(&r, &s, r[30].p, 25.0), Some(f64::INFINITY), "only accepted positions count");
    }

    #[test]
    fn a_hundred_metre_hop_while_walking_is_a_false_jump() {
        let r = line(60, 1.4);
        let mut s = shown_from(&r, 0);
        s[30].p = destination(s[30].p, 0.0, 100.0);
        assert_eq!(false_jumps(&r, &s), 2, "out and back");
    }

    #[test]
    fn a_frozen_position_while_standing_has_no_jitter_and_is_held() {
        let o = Point::new(40.0, -111.0);
        let r: Vec<TruthPoint> = (0..120).map(|i| TruthPoint { t_ms: i * 1000, p: o, speed_mps: 0.0, course_deg: 0.0 }).collect();
        let j = stationary_jitter(&r, &shown_from(&r, 0)).unwrap();
        assert!(j.rms_m < 1e-9 && j.path_m_per_min < 1e-9 && (j.held_share - 1.0).abs() < 1e-9);
    }

    #[test]
    fn turn_lag_counts_until_the_shown_course_is_within_twenty_degrees() {
        let o = Point::new(40.0, -111.0);
        let mut r = Vec::new();
        for i in 0..60_i64 {
            let (p, c) = if i < 30 {
                (destination(o, 90.0, 1.4 * i64_to_f64(i)), 90.0)
            } else {
                (destination(destination(o, 90.0, 42.0), 0.0, 1.4 * i64_to_f64(i - 30)), 0.0)
            };
            r.push(TruthPoint { t_ms: i * 1000, p, speed_mps: 1.4, course_deg: c });
        }
        let s: Vec<Shown> = r
            .iter()
            .map(|t| Shown {
                t_ms: t.t_ms,
                p: t.p,
                uncertainty_m: 3.0,
                accepted: true,
                verdict: Verdict::Used,
                course_deg: Some(if t.t_ms < 32_000 { 90.0 } else { 0.0 }),
                odometer_m: 0.0,
            })
            .collect();
        let lags = turn_lags_s(&r, &s);
        assert_eq!(lags.len(), 1, "{lags:?}");
        assert!((lags[0] - 2.0).abs() < 1.01, "{lags:?}");
    }

    #[test]
    fn percentiles_and_verdict_counts() {
        assert!((percentile(&[1.0, 2.0, 3.0, 4.0], 0.5) - 2.5).abs() < 1e-9);
        assert!(percentile(&[], 0.9).is_nan());
        let mut s = shown_from(&line(3, 1.0), 0);
        s[1].verdict = Verdict::Gated;
        let c = verdict_counts(&s);
        assert_eq!((c["Used"], c["Gated"]), (2, 1));
    }

    #[test]
    fn targets_scores_and_columns_summarise_a_track() {
        let r = line(120, 1.4);
        let s = shown_from(&r, 2);
        let targets = virtual_targets(&r, 50.0, 25.0);
        assert!(targets.len() >= 2, "a target every 50 m along 165 m");
        assert!(completes(&s, targets[0].0, 25.0));
        assert!(!completes(&[], targets[0].0, 25.0));
        let card = score("lagged", &r, &s, &targets);
        assert_eq!((card.name.as_str(), card.fixes, card.missed), ("lagged", 120, 0));
        assert!(card.arrival_lag_s.iter().all(|l| (l - 2.0).abs() < 1e-9), "{:?}", card.arrival_lag_s);
        assert!(card.jitter.is_none() && card.turn_lag_s.is_empty());
        assert!(overshoot_m(&r, &s) < 1e-9, "no turns, no overshoot");
        let table = columns(&[&card, &score("again", &r, &s, &targets)]);
        assert!(table.starts_with("metric") && table.contains("lagged") && table.contains("again"));
        assert!(table.contains("arrival lag p50 / p90 s"), "{table}");
    }

    #[test]
    fn matching_stats_of_an_empty_replay_are_zero_and_count_street_changes() {
        use crate::loc::bench::Replay;
        use crate::loc::matcher::MatchOut;
        use crate::loc::{DisplayPosition, Estimate};
        assert_eq!(matching_stats(&Replay::default()), MatchingStats { matched_share: 0.0, switches_per_min: 0.0, off_share: 0.0 });
        let at = |seg: Option<usize>| Some(MatchOut { point: Point::new(40.0, -111.0), confidence: 0.9, seg });
        let shown = |t_ms: i64| Shown {
            t_ms,
            p: Point::new(40.0, -111.0),
            uncertainty_m: 5.0,
            accepted: true,
            verdict: Verdict::Used,
            course_deg: None,
            odometer_m: 0.0,
        };
        let r = Replay {
            shown: (0..4).map(|k| shown(k * 20_000)).collect(),
            estimates: vec![Estimate::default(); 4],
            matches: vec![at(Some(1)), None, at(Some(2)), at(None)],
            displays: [true, true, false, true].map(|matched| DisplayPosition { matched, ..DisplayPosition::default() }).to_vec(),
        };
        let s = matching_stats(&r);
        assert!((s.matched_share - 0.75).abs() < 1e-9, "{s:?}");
        assert!((s.switches_per_min - 2.0).abs() < 1e-9, "two changes in one minute: {s:?}");
        assert!((s.off_share - 1.0 / 3.0).abs() < 1e-9, "one of the three answers is off-network: {s:?}");
    }
}
