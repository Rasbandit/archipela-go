//! Replay a recorded walk through the location filters and print the scorecard.
//! Usage: `cargo run --release --example replay -- <pulled-dir | raw.jsonl | journal.db> [--mode walk|run|bike|drive] [--from-ms N]
//! [--to-ms N] [--game games/<id>.json] [--atlas files/atlas/<realm>.json]... [--params p.json] [--compare baseline] [--geojson out.geojson]
//! [--csv out.csv]`
//! `--params` is a `LocParams` JSON (missing fields keep their defaults); `--compare baseline` also prints today's rules; each `--atlas`
//! (repeatable, a saved `Atlas` JSON) adds its streets to the map matching and prints its stats.
#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines, clippy::cast_precision_loss)] // CLI example: prints and fails fast; cost per fix

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use apgo_core::catalog::Mode;
use apgo_core::game::Game;
use apgo_core::geo::Point;
use apgo_core::loc::bench::{
    columns, completes, matching_stats, read_journal, read_raw_dir, rts_reference, run_locator, score, virtual_targets, LegacyRules, Recording, Replay,
    ReplayOpts, Shown,
};
use apgo_core::loc::graph::StreetGraph;
use apgo_core::loc::LocParams;
use apgo_core::scan::Atlas;

struct Args {
    input: PathBuf,
    mode: Mode,
    from_ms: i64,
    to_ms: i64,
    game: Option<PathBuf>,
    atlas: Vec<PathBuf>,
    geojson: Option<PathBuf>,
    csv: Option<PathBuf>,
    params: LocParams,
    compare: bool,
}

fn args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut a = Args {
        input: PathBuf::new(),
        mode: Mode::Walk,
        from_ms: 0,
        to_ms: i64::MAX,
        game: None,
        atlas: vec![],
        geojson: None,
        csv: None,
        params: LocParams::default(),
        compare: false,
    };
    while let Some(x) = it.next() {
        match x.as_str() {
            "--mode" => a.mode = Mode::parse(&it.next().expect("--mode needs a value")).expect("walk, run, bike or drive"),
            "--from-ms" => a.from_ms = it.next().expect("--from-ms N").parse().expect("a number"),
            "--to-ms" => a.to_ms = it.next().expect("--to-ms N").parse().expect("a number"),
            "--game" => a.game = it.next().map(PathBuf::from),
            "--atlas" => a.atlas.push(PathBuf::from(it.next().expect("--atlas files/atlas/<realm>.json"))),
            "--geojson" => a.geojson = it.next().map(PathBuf::from),
            "--csv" => a.csv = it.next().map(PathBuf::from),
            "--params" => a.params = serde_json::from_str(&std::fs::read_to_string(it.next().expect("--params p.json")).unwrap()).expect("LocParams JSON"),
            "--compare" => a.compare = compare_value(it.next().as_deref()),
            _ => a.input = PathBuf::from(x),
        }
    }
    assert!(!a.input.as_os_str().is_empty(), "usage: replay <pulled-dir | raw.jsonl | journal.db> [options]");
    a
}

/// The value of `--compare`: only `baseline` (today's rules) exists; anything else is an error, not a silent "no comparison".
fn compare_value(v: Option<&str>) -> bool {
    assert!(v == Some("baseline"), "--compare takes only baseline, got {v:?}");
    true
}

fn load(a: &Args) -> Recording {
    let p = &a.input;
    let raw_dir = p.join("diag").join("raw");
    let mut rec = if raw_dir.is_dir() {
        read_raw_dir(&raw_dir).unwrap()
    } else if p.extension().is_some_and(|e| e == "jsonl") {
        let mut r = Recording::default();
        apgo_core::loc::bench::parse_raw_lines(&std::fs::read_to_string(p).unwrap(), &mut r);
        r
    } else {
        let db = if p.is_dir() { p.join("files").join("journal.db") } else { p.clone() };
        Recording { fixes: read_journal(&db, a.from_ms, a.to_ms).unwrap(), ..Recording::default() }
    };
    rec.fixes.retain(|f| (a.from_ms..=a.to_ms).contains(&f.t_ms));
    rec
}

fn targets(game: Option<&Path>) -> Vec<(Point, f64)> {
    let Some(path) = game else { return vec![] };
    let (dir, id) = (path.parent().and_then(Path::parent).expect("files/games/<id>.json"), path.file_stem().unwrap().to_string_lossy());
    Game::load(dir, &id).expect("a game save").reach_targets().into_iter().map(|(_, p, r)| (p, r)).collect()
}

fn geojson(rec: &Recording, cols: &[(&str, &[Shown])], filtered: &Replay) -> String {
    let line = |pts: Vec<Point>| serde_json::json!({"type": "LineString", "coordinates": pts.iter().map(|p| [p.lon, p.lat]).collect::<Vec<_>>()});
    let mut features = vec![
        serde_json::json!({"type": "Feature", "properties": {"name": "raw"}, "geometry": line(rec.fixes.iter().map(apgo_core::loc::RawFix::point).collect())}),
    ];
    for (name, shown) in cols {
        features.push(serde_json::json!({"type": "Feature", "properties": {"name": name}, "geometry": line(shown.iter().map(|s| s.p).collect())}));
        for s in shown.iter().filter(|s| !s.accepted) {
            features.push(serde_json::json!({"type": "Feature", "properties": {"name": format!("{name} {:?}", s.verdict)}, "geometry": {"type": "Point", "coordinates": [s.p.lon, s.p.lat]}}));
        }
    }
    let matched: Vec<Point> = filtered.displays.iter().filter(|d| d.matched).map(|d| Point::new(d.lat, d.lon)).collect();
    if !matched.is_empty() {
        features.push(serde_json::json!({"type": "Feature", "properties": {"name": "matched"}, "geometry": line(matched)}));
    }
    serde_json::json!({"type": "FeatureCollection", "features": features}).to_string()
}

fn csv(cols: &[(&str, &[Shown])]) -> String {
    let mut out = String::from("filter,t_ms,lat,lon,uncertainty_m,accepted,verdict,odometer_m\n");
    for (name, shown) in cols {
        for s in *shown {
            let _ = writeln!(out, "{name},{},{:.7},{:.7},{:.1},{},{:?},{:.1}", s.t_ms, s.p.lat, s.p.lon, s.uncertainty_m, s.accepted, s.verdict, s.odometer_m);
        }
    }
    out
}

/// Spec scorecard row "Pickup hits" (ruling E5): the game's targets that complete with the filter vs the legacy rules.
fn pickups(quests: &[(Point, f64)], legacy: &[Shown], filter: &[Shown]) -> String {
    let hits: Vec<(bool, bool)> = quests.iter().map(|(p, r)| (completes(legacy, *p, *r), completes(filter, *p, *r))).collect();
    let count = |want: (bool, bool)| hits.iter().filter(|h| **h == want).count();
    format!(
        "pickup hits of {} game targets: gained {} (filter only), lost {} (legacy only), both {}",
        quests.len(),
        count((false, true)),
        count((true, false)),
        count((true, true))
    )
}

fn main() {
    let a = args();
    let rec = load(&a);
    println!(
        "{} fixes, {} step readings, {} headings ({} lines skipped), mode {}",
        rec.fixes.len(),
        rec.steps.len(),
        rec.headings.len(),
        rec.skipped,
        a.mode.name()
    );
    let reference = rts_reference(&rec.fixes, 15.0, if matches!(a.mode, Mode::Bike | Mode::Drive) { 1.5 } else { 0.5 });
    let quests = targets(a.game.as_deref());
    let mut targets = quests.clone();
    targets.extend(virtual_targets(&reference, 200.0, 25.0));
    let atlases: Vec<Atlas> = a.atlas.iter().map(|p| serde_json::from_str(&std::fs::read_to_string(p).unwrap()).expect("an Atlas JSON")).collect();
    let graph = StreetGraph::for_atlases(&atlases.iter().collect::<Vec<_>>()).map(Arc::new);
    let opts = ReplayOpts { mode: a.mode, params: a.params.clone(), graph };
    let t0 = std::time::Instant::now();
    let filtered = run_locator(&rec.fixes, &rec.steps, &rec.headings, &opts);
    let per_fix_us = t0.elapsed().as_secs_f64() * 1e6 / rec.fixes.len().max(1) as f64;
    let filter_card = score("filter", &reference, &filtered.shown, &targets);
    let mut legacy = LegacyRules::new();
    let legacy_shown: Vec<Shown> = rec.fixes.iter().map(|f| legacy.feed(f)).collect();
    let legacy_card = score("legacy", &reference, &legacy_shown, &targets);
    if a.compare {
        println!("{}", columns(&[&legacy_card, &filter_card]));
        println!("{}", pickups(&quests, &legacy_shown, &filtered.shown));
    } else {
        println!("{}", columns(&[&filter_card]));
    }
    let reference_m: f64 = reference.windows(2).map(|w| apgo_core::geo::distance_m(w[0].p, w[1].p)).sum();
    println!("reference (RTS) path {reference_m:.0} m; odometer legacy {:.0} m, filter {:.0} m", legacy_card.odometer_m, filter_card.odometer_m);
    println!("cost: {per_fix_us:.1} us per fix (host, release)");
    if opts.graph.is_none() && !a.atlas.is_empty() {
        println!("matching: the atlases hold no streets");
    } else if opts.graph.is_some() {
        let m = matching_stats(&filtered);
        println!("matching: {:.0} % matched, {:.1} segment changes/min, {:.0} % off-network", 100.0 * m.matched_share, m.switches_per_min, 100.0 * m.off_share);
    }
    let cols: Vec<(&str, &[Shown])> = vec![("legacy", &legacy_shown), ("filter", &filtered.shown)];
    if let Some(p) = &a.geojson {
        std::fs::write(p, geojson(&rec, &cols, &filtered)).unwrap();
    }
    if let Some(p) = &a.csv {
        std::fs::write(p, csv(&cols)).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::compare_value;

    #[test]
    fn compare_takes_only_baseline() {
        // Finding M5: a typo must not silently turn the comparison off.
        assert!(compare_value(Some("baseline")));
        assert!(std::panic::catch_unwind(|| compare_value(Some("baselin"))).is_err());
        assert!(std::panic::catch_unwind(|| compare_value(None)).is_err());
    }
}
