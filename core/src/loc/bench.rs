//! The bench: synthetic walks with known truth, metrics on a shown track, today's rules as a baseline, and readers for recorded walks.
//! Shared by `core/tests/loc_scenarios.rs` and `core/examples/replay.rs`.

mod legacy;
mod metrics;
mod record;
mod reference;
mod run;
mod synth;

pub use legacy::{legacy_implied_speed_kmh, LegacyRules};
pub use metrics::{
    arrival_lag_s, columns, completes, false_jumps, interp, matching_stats, overshoot_m, percentile, position_error, reanchor, score, stationary_jitter,
    turn_lags_s, verdict_counts, virtual_targets, Jitter, MatchingStats, Scorecard, Shown,
};
pub use record::{parse_raw_lines, read_journal, read_raw_dir, Recording};
pub use reference::{good_fix_reference, rts_reference};
pub use run::{run_locator, run_scenario, shown, Replay, ReplayOpts};
pub use synth::{cadence_for, gauss, grid_ways, truth_at, HeadingSim, Leg, Run, Scenario, Spike, TruthPoint};
