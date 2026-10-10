//! Synthetic walks with known truth: the CI thresholds of the location program (spec "CI synthetic walks").

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)] // test-sized counts and seeds

use apgo_core::geo::{distance_m, Point};
use apgo_core::loc::bench::{stationary_jitter, Leg, LegacyRules, Scenario};

const SEEDS: u64 = 20;

fn origin() -> Point {
    Point::new(40.0, -111.0)
}

#[test]
fn the_generator_is_deterministic_per_seed_and_differs_between_seeds() {
    let s = Scenario::walk(origin(), vec![Leg::Move { bearing_deg: 90.0, dist_m: 300.0, speed_mps: 1.4 }], 5.0);
    let (a, b, c) = (s.generate(1), s.generate(1), s.generate(2));
    assert_eq!(a.fixes, b.fixes);
    assert_ne!(a.fixes, c.fixes);
    assert_eq!(a.truth.len(), b.truth.len());
}

#[test]
fn truth_follows_the_legs_at_their_speed() {
    let s = Scenario::walk(origin(), vec![Leg::Move { bearing_deg: 0.0, dist_m: 140.0, speed_mps: 1.4 }, Leg::Stop { secs: 30 }], 5.0);
    let r = s.generate(1);
    assert_eq!(r.truth.len(), 131, "100 s of walking and 30 s standing, one truth point per second plus the start");
    let end = r.truth.last().unwrap();
    assert!((distance_m(origin(), end.p) - 140.0).abs() < 0.5);
    assert!(r.truth[50].speed_mps > 1.3 && end.speed_mps == 0.0);
}

#[test]
fn gnss_error_has_the_scenario_sigma_and_is_correlated_in_time() {
    let s = Scenario::walk(origin(), vec![Leg::Stop { secs: 3600 }], 8.0);
    let (mut sq, mut n, mut lag1) = (0.0, 0.0, 0.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let err: Vec<f64> = r.fixes.iter().map(|f| distance_m(f.point(), origin())).collect();
        sq += err.iter().map(|e| e * e).sum::<f64>();
        n += err.len() as f64;
        lag1 += r.fixes.windows(2).map(|w| distance_m(w[0].point(), w[1].point())).sum::<f64>() / err.len() as f64;
    }
    let rms = (sq / n).sqrt();
    let sigma = 8.0 / 1.515;
    assert!((rms - sigma * 2f64.sqrt()).abs() < sigma * 0.25, "2-D rms {rms} vs per-axis sigma {sigma}");
    assert!(lag1 / (SEEDS as f64) < rms, "consecutive fixes are closer than independent ones would be (AR(1) drift)");
}

#[test]
fn observable_truth_is_the_truth_plus_the_drift() {
    // Ruling T8-R1: the drift (64 % of the variance) is common to every fix; what is left between a fix and the observable truth is the
    // white part (36 %, per-axis 0.6 sigma).
    let s = Scenario::walk(origin(), vec![Leg::Stop { secs: 3600 }], 8.0);
    let r = s.generate(1);
    assert_eq!(r.observed.len(), r.truth.len());
    let (mut sq, mut n) = (0.0, 0.0);
    for f in &r.fixes {
        sq += distance_m(f.point(), apgo_core::loc::bench::truth_at(&r.observed, f.t_ms).p).powi(2);
        n += 1.0;
    }
    let sigma = 8.0 / 1.515;
    let rms = (sq / n).sqrt();
    assert!((rms - 0.6 * sigma * 2f64.sqrt()).abs() < 0.6 * sigma * 0.25, "fix vs observable truth {rms}");
    assert!(r.observed.iter().zip(&r.truth).all(|(o, t)| o.t_ms == t.t_ms && (o.speed_mps - t.speed_mps).abs() < 1e-12));
}

#[test]
fn reported_accuracy_jitters_thirty_percent_around_the_true_one() {
    let r = Scenario::walk(origin(), vec![Leg::Stop { secs: 600 }], 10.0).generate(3);
    assert!(r.fixes.iter().all(|f| (7.0..=13.0).contains(&f.accuracy_m)), "acc 10 m +-30 %");
}

#[test]
fn spikes_gaps_steps_and_headings_appear_where_the_scenario_puts_them() {
    let mut s = Scenario::walk(origin(), vec![Leg::Move { bearing_deg: 90.0, dist_m: 420.0, speed_mps: 1.4 }], 5.0);
    s.spikes = vec![Spike { at_s: 60, offset_m: 150.0, bearing_deg: 0.0, len: 1 }];
    s.gaps = vec![(100, 160)];
    s.steps = true;
    s.heading = Some(HeadingSim::pocket(vec![(0, 90.0)]));
    let r = s.generate(5);
    let spike = r.fixes.iter().find(|f| f.t_ms == s.t0_ms + 60_000).unwrap();
    let truth = r.truth.iter().find(|t| t.t_ms == spike.t_ms).unwrap();
    assert!(distance_m(spike.point(), truth.p) > 120.0);
    assert!(r.fixes.iter().all(|f| !(s.t0_ms + 100_000..s.t0_ms + 160_000).contains(&f.t_ms)), "no fixes in the gap");
    assert!(r.steps.len() > 100 && r.steps.windows(2).all(|w| w[1].1 >= w[0].1), "cumulative steps every 2 s");
    assert!(r.steps.windows(2).all(|w| w[1].1 > w[0].1), "a reading only when the count changes, as Android's counter");
    let walked = (r.steps.last().unwrap().1 - r.steps[0].1) as f64;
    assert!((walked * 0.73 - 420.0).abs() < 30.0, "about 0.73 m a step at 1.4 m/s: {walked} steps");
    let h = &r.headings[200];
    let offset = (truth_course_at(&r, h.t_ms) - h.azimuth_deg).rem_euclid(360.0);
    assert!((offset - 90.0).abs() < 10.0, "a pocketed phone points 90 deg off the walking direction: {offset}");
}

fn truth_course_at(r: &Run, t_ms: i64) -> f64 {
    apgo_core::loc::bench::truth_at(&r.truth, t_ms).course_deg
}
#[test]
fn the_bench_sees_the_standing_jitter_of_the_legacy_rules() {
    // Baseline: the old map showed every raw fix, so standing still wanders by metres. The IMM must beat this (Task 8).
    let s = Scenario::walk(origin(), vec![Leg::Stop { secs: 300 }], 8.0);
    let r = s.generate(1);
    let mut legacy = LegacyRules::new();
    let shown: Vec<_> = r.fixes.iter().map(|f| legacy.feed(f)).collect();
    let j = stationary_jitter(&r.truth, &shown).unwrap();
    assert!(j.rms_m > 2.0, "raw fixes jitter {} m", j.rms_m);
}

use apgo_core::catalog::Mode;
use apgo_core::geo::destination;
use apgo_core::loc::bench::{
    arrival_lag_s, completes, false_jumps, overshoot_m, percentile, position_error, run_scenario, turn_lags_s, ReplayOpts, Run, Shown, Spike,
};
use apgo_core::loc::Verdict;

fn opts(mode: Mode) -> ReplayOpts {
    ReplayOpts { mode, ..ReplayOpts::default() }
}

fn east(m: f64, speed: f64) -> Leg {
    Leg::Move { bearing_deg: 90.0, dist_m: m, speed_mps: speed }
}

/// The raw fixes as shown positions, all accepted: the arrival a zero-lag display of the fixes would have.
fn raw_shown(r: &Run) -> Vec<Shown> {
    r.fixes
        .iter()
        .map(|f| Shown { t_ms: f.t_ms, p: f.point(), uncertainty_m: f.accuracy_m, accepted: true, verdict: Verdict::Used, course_deg: None, odometer_m: 0.0 })
        .collect()
}

/// Seconds of arrival lag the filter adds over the raw fixes' own arrival at the target (ruling T8-R1); `None` when the raw fixes never
/// arrive (nothing to compare with).
fn added_lag_s(r: &Run, shown: &[Shown], target: Point, radius: f64) -> Option<f64> {
    let raw = arrival_lag_s(&r.truth, &raw_shown(r), target, radius).filter(|l| l.is_finite())?;
    Some(arrival_lag_s(&r.truth, shown, target, radius)? - raw)
}

#[test]
fn straight_walk_is_within_three_metres_rms_and_never_jumps() {
    let s = Scenario::walk(origin(), vec![east(420.0, 1.4)], 5.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        // Ruling T8-R1: scored against the observable truth (truth plus the generated drift); the truth-scored error is information.
        let (rms, p95) = position_error(&r.observed, &out.shown);
        let (true_rms, true_p95) = position_error(&r.truth, &out.shown);
        println!("seed {seed}: vs observable truth rms {rms:.2} p95 {p95:.2}; vs truth rms {true_rms:.2} p95 {true_p95:.2}");
        // Task 23: tightened from the spec's 3.0 / 6.0 m (worst seed 2.08 / 3.64 m on the defaults).
        assert!(rms <= 2.5 && p95 <= 4.5, "seed {seed}: rms {rms:.2} p95 {p95:.2}");
        assert_eq!(false_jumps(&r.truth, &out.shown), 0, "seed {seed}");
    }
}

#[test]
fn a_corner_is_turned_within_seven_seconds_median_without_overshooting_eight_metres() {
    let s = Scenario::walk(origin(), vec![east(100.0, 1.4), Leg::Move { bearing_deg: 0.0, dist_m: 100.0, speed_mps: 1.4 }], 5.0);
    let mut all = Vec::new();
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let lags = turn_lags_s(&r.observed, &out.shown); // ruling T8-R5: against the observable truth
        assert!(lags.len() == 1, "seed {seed}: {lags:?}");
        all.push(lags[0]);
        let over = overshoot_m(&r.observed, &out.shown); // ruling T8-R8: against the observable truth
        assert!(over <= 8.0, "seed {seed}: {over}");
    }
    // Ruling T8-R19 fallback: the line fit counts only when significant (R17), so a walking corner is seen by the filter's course:
    // median <= 7 s, worst <= 10 s over the 20 seeds (heading arrow only; the overshoot gate above is unchanged).
    let worst = all.iter().copied().fold(0.0, f64::max);
    assert!(percentile(&all, 0.5) <= 7.0 && worst <= 10.0, "turn lags {all:?}");
}

#[test]
fn standing_five_minutes_holds_still_and_adds_no_distance() {
    let mut s = Scenario::walk(origin(), vec![Leg::Stop { secs: 300 }], 8.0);
    s.steps = true;
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let j = stationary_jitter(&r.observed, &out.shown).unwrap();
        assert!(j.rms_m <= 2.0 && j.drift_m_per_min <= 5.0, "seed {seed}: {j:?}");
        assert!(j.held_share >= 0.95, "seed {seed}: held {:.2}", j.held_share);
        assert!(out.shown.last().unwrap().odometer_m <= 5.0, "seed {seed}: {}", out.shown.last().unwrap().odometer_m);
    }
}

#[test]
fn standing_twenty_five_minutes_with_a_counter_that_reports_on_change_stays_held() {
    // Final review N1 (ruling FR-N1): Android's counter sends nothing while the player stands, so a counter that once reported stays
    // present. Same odometer bound as the five-minute stand; the held share over the last 10 min stays at the five-minute level
    // (base 1.00; 0.9 leaves room for a re-placement when drift passes the quiet bound, review N2).
    let mut s = Scenario::walk(origin(), vec![Leg::Stop { secs: 1_500 }], 8.0);
    s.steps = true;
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        assert_eq!(r.steps.len(), 1, "seed {seed}: one reading at the start, none while standing");
        let out = run_scenario(&r, &opts(Mode::Walk));
        let odo = out.shown.last().unwrap().odometer_m;
        assert!(odo <= 5.0, "seed {seed}: odometer {odo:.0} m");
        let from = r.truth.last().unwrap().t_ms - 600_000;
        let last: Vec<Shown> = out.shown.iter().copied().filter(|x| x.t_ms >= from).collect();
        let j = stationary_jitter(&r.observed, &last).unwrap();
        assert!(j.held_share >= 0.9, "seed {seed}: held {:.2} over the last 10 min", j.held_share);
    }
}

#[test]
fn a_spike_and_a_burst_are_gated_and_complete_nothing() {
    let mut s = Scenario::walk(origin(), vec![east(420.0, 1.4)], 5.0);
    let burst = |at_s, bearing_deg| Spike { at_s, offset_m: 80.0, bearing_deg, len: 1 };
    s.spikes = vec![Spike { at_s: 100, offset_m: 150.0, bearing_deg: 0.0, len: 1 }, burst(200, 0.0), burst(201, 120.0), burst(202, 240.0)];
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        for at_s in [100, 200, 201, 202] {
            let t = s.t0_ms + i64::from(at_s) * 1000;
            let k = out.shown.iter().position(|x| x.t_ms == t).unwrap();
            assert_eq!(out.shown[k].verdict, Verdict::Gated, "seed {seed} at {at_s}");
            let dev = distance_m(out.shown[k].p, out.shown[k - 1].p) - 1.4;
            assert!(dev <= 3.0, "seed {seed} at {at_s}: shown moved {dev:.1} m more than the walker");
        }
        let spike_spot = destination(apgo_core::loc::bench::truth_at(&r.truth, s.t0_ms + 100_000).p, 0.0, 150.0);
        assert!(!completes(&out.shown, spike_spot, 25.0), "seed {seed}: the spike would complete a quest");
    }
}

#[test]
fn a_real_relocation_is_believed_within_three_fixes() {
    let mut s =
        Scenario::walk(origin(), vec![east(84.0, 1.4), Leg::Move { bearing_deg: 0.0, dist_m: 2000.0, speed_mps: 2000.0 / 60.0 }, Leg::Stop { secs: 60 }], 5.0);
    s.gaps = vec![(61, 121)];
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let after: Vec<_> = out.shown.iter().filter(|x| x.t_ms >= s.t0_ms + 121_000).collect();
        let k = after.iter().position(|x| x.verdict == Verdict::Relocated || x.verdict == Verdict::Reset).expect("believed");
        assert!(k < 3 || after[k].t_ms - after[0].t_ms <= 15_000, "seed {seed}: believed at fix {k}");
    }
}

#[test]
fn a_bike_arrives_within_two_seconds_and_holds_at_both_stops() {
    let legs = vec![east(600.0, 5.0), Leg::Stop { secs: 30 }, east(600.0, 5.0), Leg::Stop { secs: 30 }, east(300.0, 5.0)];
    let s = Scenario::walk(origin(), legs, 5.0);
    let mut held = Vec::new();
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Bike));
        for d in [300.0, 600.0, 900.0, 1200.0] {
            let Some(lag) = added_lag_s(&r, &out.shown, destination(origin(), 90.0, d), 25.0) else { continue };
            assert!(lag <= 2.0, "seed {seed} at {d} m: {lag}");
        }
        let j = stationary_jitter(&r.truth, &out.shown).unwrap();
        held.push(j.held_share);
    }
    // Ruling T8-R15: held at the stops, mean >= 0.7 and every seed >= 0.65 (hold_mu_s stays 0.8 for step-less walkers).
    let (mean, min) = (held.iter().sum::<f64>() / held.len() as f64, held.iter().copied().fold(1.0, f64::min));
    assert!(mean >= 0.7 && min >= 0.65, "held mean {mean:.2} min {min:.2}: {held:?}");
}

#[test]
fn balanced_quality_fixes_never_complete_an_off_route_target_and_arrive_within_twenty_seconds_p90() {
    let mut s = Scenario::walk(origin(), vec![east(600.0, 1.4)], 25.0);
    s.acc_spread_m = 15.0;
    s.fix_every_s = 5;
    let off_route = destination(destination(origin(), 90.0, 300.0), 0.0, 60.0);
    let mut lags = Vec::new();
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        assert!(!completes(&out.shown, off_route, 25.0), "seed {seed}");
        for d in [100.0, 200.0, 300.0, 400.0, 500.0] {
            // Ruling T8-R11: lag from the observable truth entering the circle (degraded mode: p90 <= 20 s).
            lags.extend(arrival_lag_s(&r.observed, &out.shown, destination(origin(), 90.0, d), 25.0));
        }
    }
    assert!(percentile(&lags, 0.9) <= 20.0, "p90 {}", percentile(&lags, 0.9));
}

#[test]
fn walking_in_arrives_within_three_seconds_and_a_pass_at_35_m_never_counts() {
    let target = destination(origin(), 90.0, 200.0);
    let walk_in = Scenario::walk(origin(), vec![east(220.0, 1.4)], 5.0);
    let pass = Scenario::walk(destination(origin(), 0.0, 35.0), vec![east(400.0, 1.4)], 5.0);
    for seed in 0..SEEDS {
        let r = walk_in.generate(seed);
        let lag = added_lag_s(&r, &run_scenario(&r, &opts(Mode::Walk)).shown, target, 25.0).unwrap();
        assert!(lag <= 3.0, "seed {seed}: {lag}");
        let p = pass.generate(seed);
        assert!(!completes(&run_scenario(&p, &opts(Mode::Walk)).shown, target, 25.0), "seed {seed}: a 35 m pass counted");
    }
}

#[test]
fn driving_in_a_walk_zone_is_tracked_and_too_fast_to_count() {
    let s = Scenario::walk(origin(), vec![east(2000.0, 50.0 / 3.6)], 5.0);
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &opts(Mode::Walk));
        let mut streak = 0;
        let mut worst = 0;
        for x in &out.shown {
            streak = if x.verdict == Verdict::Gated { streak + 1 } else { 0 };
            worst = worst.max(streak);
        }
        assert!(worst <= 3, "seed {seed}: {worst} gated in a row");
        let late: Vec<_> = out.estimates.iter().filter(|e| e.accepted && e.t_ms > s.t0_ms + 30_000).collect();
        let blocked = late.iter().filter(|e| (e.speed_mps - 2.0 * e.speed_sigma_mps) * 3.6 > 12.0).count();
        assert!(blocked * 10 >= late.len() * 9, "seed {seed}: speed_ok would block {blocked} of {}", late.len());
    }
}

#[test]
#[ignore = "timing harness: cargo test --release --test loc_scenarios timing -- --ignored --nocapture"]
fn timing_of_on_fix() {
    let s = Scenario::walk(origin(), vec![east(4200.0, 1.4)], 5.0);
    let r = s.generate(1);
    let mut l = apgo_core::loc::Locator::default();
    let mut us: Vec<f64> = r
        .fixes
        .iter()
        .map(|f| {
            let t = std::time::Instant::now();
            let _ = l.on_fix(f);
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    us.sort_by(f64::total_cmp);
    println!("on_fix: mean {:.1} us, p99 {:.1} us over {} fixes", us.iter().sum::<f64>() / us.len() as f64, percentile(&us, 0.99), us.len());
}

#[test]
fn a_bike_with_a_quiet_step_counter_stops_and_leaves_without_losing_distance() {
    // Ruling T8-R18: a cyclist's phone has a step counter that stays quiet. That must not freeze the hold at the stops (the quiet-counter
    // suppression is for Walk/Run zones): each departure is followed, the odometer is within 10 % of the truth path and no more than 3
    // fixes are gated in a row.
    let legs = vec![east(600.0, 5.0), Leg::Stop { secs: 30 }, east(600.0, 5.0), Leg::Stop { secs: 30 }, east(300.0, 5.0)];
    let s = Scenario::walk(origin(), legs, 5.0);
    for seed in 0..SEEDS {
        let mut r = s.generate(seed);
        let end = r.truth.last().unwrap().t_ms;
        r.steps = (s.t0_ms..=end).step_by(2_000).map(|t| (t, 10_000)).collect();
        let out = run_scenario(&r, &opts(Mode::Bike));
        let truth_m: f64 = r.truth.windows(2).map(|w| distance_m(w[0].p, w[1].p)).sum();
        let odo = out.shown.last().unwrap().odometer_m;
        let mut streak = 0;
        let mut worst = 0;
        for x in &out.shown {
            streak = if x.verdict == Verdict::Gated { streak + 1 } else { 0 };
            worst = worst.max(streak);
        }
        println!("seed {seed}: odometer {odo:.0} of {truth_m:.0} m, worst gated streak {worst}, {:?}", apgo_core::loc::bench::verdict_counts(&out.shown));
        assert!((odo - truth_m).abs() <= 0.1 * truth_m, "seed {seed}: odometer {odo:.0} vs {truth_m:.0} m");
        assert!(worst <= 3, "seed {seed}: {worst} gated in a row");
    }
}

#[test]
fn a_mover_without_steps_leaves_a_quiet_hold_within_thirty_seconds() {
    // Ruling FR-C1 (final review C1): a wheelchair, a stroller or a cart cup holder moves the phone with the step counter quiet. After
    // a minute standing (held), the player moves off at 0.5 to 2.5 m/s, with and without Doppler speed in the fixes: the hold ends
    // within 30 s of the start and the shown position never falls more than 25 m behind the observable truth. At 0.5 m/s the player
    // needs 30 s just to get `hold_quiet_max_m` (15 m) out, so there the hold ends within 30 s of that (60 s from the start).
    for with_velocity in [false, true] {
        for speed in [0.5, 1.0, 1.4, 2.5] {
            let mut s = Scenario::walk(origin(), vec![Leg::Stop { secs: 60 }, east(300.0 * speed, speed)], 5.0);
            s.with_velocity = with_velocity;
            let mut worst = (0.0_f64, 0.0_f64);
            for seed in 0..SEEDS {
                let r = s.generate(seed);
                let mut l = apgo_core::loc::Locator::default();
                let (start, mut held_at, mut ended_s) = (s.t0_ms + 60_000, None, None);
                let mut max_err = 0.0_f64;
                for f in &r.fixes {
                    if (f.t_ms - s.t0_ms) % 2_000 == 0 {
                        l.on_steps(10_000, f.t_ms, None); // present, never a step
                    }
                    let e = l.on_fix(f);
                    if f.t_ms < start {
                        held_at = l.holding().then(|| e.point());
                        continue;
                    }
                    if ended_s.is_none() && held_at.is_none_or(|h| e.point() != h) {
                        ended_s = Some((f.t_ms - start) as f64 / 1000.0);
                    }
                    if f.t_ms >= start + 5_000 {
                        max_err = max_err.max(distance_m(e.point(), apgo_core::loc::bench::truth_at(&r.observed, f.t_ms).p));
                    }
                }
                assert!(held_at.is_some(), "speed {speed} velocity {with_velocity} seed {seed}: not held after standing");
                let ended = ended_s.unwrap_or(f64::INFINITY);
                worst = (worst.0.max(ended), worst.1.max(max_err));
                let limit_s = if speed < 1.0 { 15.0 / speed + 30.0 } else { 30.0 };
                assert!(ended <= limit_s, "speed {speed} velocity {with_velocity} seed {seed}: hold ended after {ended} s");
                assert!(max_err < 25.0, "speed {speed} velocity {with_velocity} seed {seed}: error {max_err:.1} m");
            }
            println!("speed {speed} velocity {with_velocity}: worst hold end {:.0} s, worst error {:.1} m", worst.0, worst.1);
        }
    }
}

#[test]
fn standing_unheld_shows_no_course() {
    // Ruling T8-R17: noise is no direction of travel. Unheld standing (a phone without a step counter in its first 10 s, where the hold
    // cannot start yet, and a Bike zone) reports no course on >= 95 % of fixes over the 20 seeds.
    let s = Scenario::walk(origin(), vec![Leg::Stop { secs: 120 }], 8.0);
    let (mut walk, mut bike) = ((0_usize, 0_usize), (0_usize, 0_usize));
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let mut l = apgo_core::loc::Locator::default();
        for f in r.fixes.iter().filter(|f| f.t_ms < s.t0_ms + 10_000) {
            walk.0 += usize::from(l.on_fix(f).course_deg.is_none());
            walk.1 += 1;
        }
        let mut l = apgo_core::loc::Locator::default();
        l.set_mode(Mode::Bike);
        for f in &r.fixes {
            let e = l.on_fix(f);
            if !l.holding() {
                bike.0 += usize::from(e.course_deg.is_none());
                bike.1 += 1;
            }
        }
    }
    let share = |(none, n): (usize, usize)| none as f64 / n as f64;
    println!("no course: step-less first 10 s {:.3} of {}, Bike zone unheld {:.3} of {}", share(walk), walk.1, share(bike), bike.1);
    assert!(share(walk) >= 0.95 && share(bike) >= 0.95, "step-less {:.3}, Bike {:.3}", share(walk), share(bike));
}

use std::sync::Arc;

use apgo_core::loc::bench::{grid_ways, matching_stats};
use apgo_core::loc::graph::StreetGraph;

#[test]
fn at_a_corner_the_matched_street_is_the_new_one_within_three_seconds_median() {
    // Ruling T19-R1: the matcher follows the filter's estimate, which itself lags corners (ruling T8-R19) under the scenario's drift.
    // Ruling T19-input (inputs at about 1 Hz) tightened the gate to what it reaches: median 3 s, max 7 s measured; gate median <= 3 s,
    // max <= 8 s over the seeds (the aim, max 6 s, is not reached). The matched share holds on every seed.
    let graph = Arc::new(StreetGraph::from_ways(&grid_ways(origin(), 5, 100.0)).unwrap());
    let s = Scenario::walk(origin(), vec![east(100.0, 1.4), Leg::Move { bearing_deg: 0.0, dist_m: 100.0, speed_mps: 1.4 }], 5.0);
    let turn_t = s.t0_ms + 72_000; // 100 m at 1.4 m/s
    let corner = destination(origin(), 90.0, 100.0);
    let mut lags = vec![];
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &ReplayOpts { graph: Some(graph.clone()), ..opts(Mode::Walk) });
        let on_north_street = |k: usize| {
            out.matches[k]
                .is_some_and(|m| m.point.lat > corner.lat + 1e-6 && (m.point.lon - corner.lon).abs() * 111_195.0 * corner.lat.to_radians().cos() < 1.0)
        };
        let k = (0..out.shown.len())
            .find(|&k| out.shown[k].t_ms > turn_t && on_north_street(k))
            .unwrap_or_else(|| panic!("seed {seed}: never on the north street"));
        lags.push((out.shown[k].t_ms - turn_t) as f64 / 1000.0);
        let share = matching_stats(&out).matched_share;
        assert!(share > 0.8, "seed {seed}: matched share {share}");
    }
    let (median, max) = (percentile(&lags, 0.5), lags.iter().copied().fold(0.0, f64::max));
    println!("matched corner lag: median {median:.1} s, max {max:.1} s ({lags:?})");
    assert!(median <= 3.0 && max <= 8.0, "median {median} s, max {max} s: {lags:?}");
}

/// The locator after feeding `r`'s fixes and step readings in time order up to `cut_ms`.
fn calibrated(r: &Run, cut_ms: i64) -> apgo_core::loc::Locator {
    let mut l = apgo_core::loc::Locator::default();
    let mut evs: Vec<(i64, Option<apgo_core::loc::RawFix>, Option<i64>)> = r.fixes.iter().map(|f| (f.t_ms, Some(*f), None)).collect();
    evs.extend(r.steps.iter().map(|(t, n)| (*t, None, Some(*n))));
    evs.sort_by_key(|e| (e.0, e.1.is_some()));
    for (t, f, n) in evs.into_iter().filter(|e| e.0 <= cut_ms) {
        if let Some(n) = n {
            l.on_steps(n, t, None);
        }
        if let Some(f) = f {
            l.on_fix(&f);
        }
    }
    l
}

#[test]
fn after_a_carry_change_the_step_scale_is_relearned_within_three_minutes() {
    let mut s = Scenario::walk(origin(), vec![east(840.0, 1.4)], 5.0); // 10 min
    s.steps = true;
    s.step_scale = vec![(0, 1.0), (300, 0.85)]; // pocket to bag at 5 min
    for seed in 0..SEEDS {
        let l = calibrated(&s.generate(seed), s.t0_ms + 300_000 + 180_000);
        assert!((l.step_calibration().k - 0.85).abs() <= 0.07, "seed {seed}: k {} (ruling T21-R5)", l.step_calibration().k);
    }
}

#[test]
fn a_steady_ten_minute_walk_never_adapts_falsely() {
    let mut s = Scenario::walk(origin(), vec![east(840.0, 1.4)], 5.0);
    s.steps = true;
    for seed in 0..SEEDS {
        let l = calibrated(&s.generate(seed), i64::MAX);
        let c = l.step_calibrator();
        assert!(c.cusum_fires() <= 1, "seed {seed}: the CUSUM fired {} times (ruling T21-R4)", c.cusum_fires());
        assert!((c.k() - 1.0).abs() <= 0.05, "seed {seed}: k {} (ruling T21-R5)", c.k());
    }
}

#[test]
fn a_fix_gap_mid_walk_keeps_the_step_scale() {
    // Ruling T21-I1: a 30 s gap without fixes breaks the window instead of joining across it. Gate per ruling T21-R7b, as for a stop:
    // at least 18 of 20 seeds within 0.05 and every seed within 0.15. Since adversarial review C1 the first three fixes after the
    // bridged gap do not count, which moves every later window by 3 s; seed 7 then meets two outlier windows (k 1.13 and 1.15) that
    // trip an adapt on their own (k 1.135), the same late-outlier effect T21-R7b accepts for the stop.
    let mut s = Scenario::walk(origin(), vec![east(840.0, 1.4)], 5.0);
    s.steps = true;
    s.gaps = vec![(200, 230)];
    let mut close = 0;
    for seed in 0..SEEDS {
        let k = calibrated(&s.generate(seed), i64::MAX).step_calibration().k;
        assert!((k - 1.0).abs() <= 0.15, "seed {seed}: k {k} (ruling T21-R7b)");
        close += u32::from((k - 1.0).abs() <= 0.05);
    }
    assert!(close >= 18, "{close} of {SEEDS} seeds within 0.05 (ruling T21-R7b)");
}

#[test]
fn a_stop_mid_walk_keeps_the_step_scale() {
    // Ruling T21-I1: a hold (standing, steps quiet) breaks the window instead of joining across it. Gate per ruling T21-R7b: at least
    // 18 of 20 seeds within 0.05 and every seed within 0.15 (a late outlier window can trip an adapt on its own; worst seed 0.139);
    // Task 23 re-checks this on real walks.
    let mut s = Scenario::walk(origin(), vec![east(420.0, 1.4), Leg::Stop { secs: 60 }, east(420.0, 1.4)], 5.0);
    s.steps = true;
    let mut close = 0;
    for seed in 0..SEEDS {
        let k = calibrated(&s.generate(seed), i64::MAX).step_calibration().k;
        assert!((k - 1.0).abs() <= 0.15, "seed {seed}: k {k} (ruling T21-R7b)");
        close += u32::from((k - 1.0).abs() <= 0.05);
    }
    assert!(close >= 18, "{close} of {SEEDS} seeds within 0.05 (ruling T21-R7b)");
}

use apgo_core::loc::bench::{reanchor, HeadingSim};
use apgo_core::loc::LocParams;

fn gap_walk(heading: HeadingSim) -> Scenario {
    // 2 min east along the y = 100 m street (good GPS, offset learned), then a 60 s gap: 23 s east to the x = 200 m crossing, turn, 37 s
    // north; GPS returns on the north street.
    let start = destination(origin(), 0.0, 100.0);
    let mut s = Scenario::walk(start, vec![east(200.0, 1.4), Leg::Move { bearing_deg: 0.0, dist_m: 140.0, speed_mps: 1.4 }], 5.0);
    s.steps = true;
    s.with_velocity = true;
    s.heading = Some(heading);
    s.gaps = vec![(120, 180)];
    s
}

/// The bridged error at re-anchor and the display jump of every seed's gap (ruling E5: printed as a table under `label`).
#[allow(clippy::unwrap_used, clippy::expect_used)] // a test helper: a missing graph or bridge is a failed test
fn gap_errors(label: &str, s: &Scenario, params: &LocParams) -> (Vec<f64>, Vec<f64>) {
    let graph = Arc::new(StreetGraph::from_ways(&grid_ways(origin(), 5, 100.0)).unwrap());
    let (mut errs, mut jumps) = (Vec::new(), Vec::new());
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let out = run_scenario(&r, &ReplayOpts { graph: Some(graph.clone()), params: params.clone(), ..opts(Mode::Walk) });
        let (e, j) = reanchor(&r.truth, &out, s.t0_ms + 180_000).expect("bridged through the gap");
        errs.push(e);
        jumps.push(j);
    }
    eprintln!("{label}: gap {}..{} s\n  seed | bridged error m | display jump m", s.gaps[0].0, s.gaps[0].1);
    for (seed, (e, j)) in errs.iter().zip(&jumps).enumerate() {
        eprintln!("  {seed:>4} | {e:>15.1} | {j:>14.1}");
    }
    (errs, jumps)
}

#[test]
fn a_sixty_second_gap_with_a_turn_is_bridged_within_fifteen_metres() {
    let (errs, jumps) = gap_errors("in hand", &gap_walk(HeadingSim::in_hand()), &LocParams::default());
    println!("in hand: error p90 {:.1} m, max {:.1} m; jump max {:.1} m", percentile(&errs, 0.9), percentile(&errs, 1.0), percentile(&jumps, 1.0));
    assert!(percentile(&errs, 0.9) <= 15.0, "p90 {:.1} {errs:?}", percentile(&errs, 0.9));
    assert!(jumps.iter().all(|j| *j <= 10.0), "display jumps {jumps:?}");
}

#[test]
fn a_pocketed_phone_is_bridged_with_its_learned_offset_and_beats_the_raw_compass() {
    let s = gap_walk(HeadingSim::pocket(vec![(0, 90.0)]));
    let graph = Arc::new(StreetGraph::from_ways(&grid_ways(origin(), 5, 100.0)).unwrap());
    for seed in 0..SEEDS {
        let r = s.generate(seed);
        let before_gap: Vec<_> = r.fixes.iter().filter(|f| f.t_ms < s.t0_ms + 120_000).copied().collect();
        // the learned offset at the start of the gap, from a replay of the first two minutes (fixes and compass)
        let mut probe = apgo_core::loc::Locator::default();
        probe.set_graph(Some(graph.clone()));
        let mut evs: Vec<(i64, u8, usize)> = before_gap.iter().enumerate().map(|(i, f)| (f.t_ms, 2, i)).collect();
        evs.extend(r.headings.iter().enumerate().filter(|(_, h)| h.t_ms < s.t0_ms + 120_000).map(|(i, h)| (h.t_ms, 1, i)));
        evs.sort_unstable();
        for (_, kind, i) in evs {
            if kind == 1 {
                probe.on_heading(&r.headings[i]);
            } else {
                probe.on_fix(&before_gap[i]);
            }
        }
        let d = apgo_core::loc::heading::wrap_deg(probe.carry().delta_deg() - 90.0).abs();
        assert!(d <= 15.0, "seed {seed}: offset off by {d:.1} deg");
    }
    let (with_offset, _) = gap_errors("pocket", &s, &LocParams::default());
    let (raw, _) = gap_errors("pocket, raw compass", &s, &LocParams { carry_enabled: false, ..LocParams::default() });
    println!("pocket: error p90 {:.1} m with the offset, {:.1} m with the raw compass", percentile(&with_offset, 0.9), percentile(&raw, 0.9));
    assert!(percentile(&with_offset, 0.9) <= 20.0, "p90 {:.1}", percentile(&with_offset, 0.9));
    assert!(
        percentile(&raw, 0.9) > percentile(&with_offset, 0.9),
        "the raw compass must do worse: {:.1} vs {:.1}",
        percentile(&raw, 0.9),
        percentile(&with_offset, 0.9)
    );
}

#[test]
fn a_carry_change_in_the_gap_falls_back_to_course_and_streets() {
    let s = gap_walk(HeadingSim::pocket(vec![(0, 90.0), (130, 0.0)]));
    let (errs, _) = gap_errors("carry change", &s, &LocParams::default());
    println!("carry change: error p90 {:.1} m, max {:.1} m", percentile(&errs, 0.9), percentile(&errs, 1.0));
    assert!(percentile(&errs, 0.9) <= 25.0, "p90 {:.1} {errs:?}", percentile(&errs, 0.9));
}

#[test]
#[ignore = "timing harness: cargo test --release --test loc_scenarios timing -- --ignored --nocapture"]
fn timing_of_a_bridge_step_batch() {
    use apgo_core::loc::bridge::{Bridge, Steer};
    let graph = Arc::new(StreetGraph::from_ways(&grid_ways(origin(), 41, 100.0)).unwrap());
    let p = LocParams::default();
    let at = destination(destination(origin(), 0.0, 2000.0), 90.0, 20.0);
    let (mut us, mut cloud, mut start_us) = (Vec::new(), 0, Vec::new());
    for gap in 0..50_i64 {
        let frame = apgo_core::loc::frame::Frame::new(origin());
        let mask = apgo_core::loc::graph::mode_mask(Mode::Walk);
        let t0 = std::time::Instant::now();
        let mut b = Bridge::start(Some(graph.clone()), frame, mask, at, [[25.0, 0.0], [0.0, 25.0]], 0.05, None, Some(90.0), gap, &p);
        start_us.push(t0.elapsed().as_secs_f64() * 1e6);
        for i in 0..60 {
            let theta = if i < 30 { 90.0 } else { 0.0 };
            let t = std::time::Instant::now();
            b.step(2.8, Steer { theta_deg: theta, sigma_deg: 20.0 }, &p);
            let _ = b.estimate();
            us.push(t.elapsed().as_secs_f64() * 1e6);
        }
        cloud = b.memory_bytes();
    }
    us.sort_by(f64::total_cmp);
    start_us.sort_by(f64::total_cmp);
    println!("bridge start: p99 {:.1} us", percentile(&start_us, 0.99));
    println!(
        "bridge step batch + estimate ({} particles): mean {:.1} us, p99 {:.1} us over {} batches; cloud {} bytes",
        p.bridge_particles,
        us.iter().sum::<f64>() / us.len() as f64,
        percentile(&us, 0.99),
        us.len(),
        cloud
    );
}
