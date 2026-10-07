//! End-to-end check on REAL map data: scan a realm, generate a solo game, autoplay every quest to the goal.
//! Usage: cargo run --release --example play_sim -- <lat> <lon> <radius_m> <trips> <goal> [mode]

use std::collections::BTreeMap;
use std::time::Instant;

use apgo_core::assign::Target;
use apgo_core::catalog::{Catalog, Mode};
use apgo_core::game::{Backend, Event, Game, NewGame, QuestState};
use apgo_core::geo::{destination, Point};
use apgo_core::realm::{Realm, Shape};
use apgo_core::scan::scan_realm;
use apgo_core::solo::{generate, SoloOptions};
use apgo_core::verify::Fix;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (lat, lon): (f64, f64) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    let radius: f64 = a[3].parse().unwrap();
    let trips: u32 = a[4].parse().unwrap();
    let goal = a[5].clone();
    let mode = a.get(6).and_then(|m| Mode::parse(m)).unwrap_or(Mode::Walk);
    let home = Point::new(lat, lon);
    let catalog = Catalog::builtin();
    let cache = std::env::temp_dir().join("apgo-playsim-cache");

    let realm = Realm {
        id: "r".into(),
        name: "Sim realm".into(),
        modes: vec![mode],
        shape: Shape::Circle { center: home, radius_m: radius },
        spare: None,
        scanned_at_ms: None,
    };
    let t0 = Instant::now();
    let atlas = scan_realm(&realm, &catalog, Some(&cache), 0).expect("scan");
    println!(
        "scan: {:.1}s, {} places, {} street points, warnings: {:?}",
        t0.elapsed().as_secs_f64(),
        atlas.features.len(),
        atlas.streets.len(),
        atlas.warnings
    );
    let offers = atlas.offers(&catalog, &[mode]);
    let mut top: Vec<_> = offers.iter().filter(|(_, n)| **n > 0).collect();
    top.sort_by(|a, b| b.1.cmp(a.1));
    println!(
        "offers ({} kinds with places): {}",
        top.len(),
        top.iter().take(14).map(|(k, n)| format!("{}x{}", catalog.kind(k).unwrap().name, n)).collect::<Vec<_>>().join(", ")
    );

    let opts = SoloOptions { zone_modes: vec![mode], number_of_trips: trips, goal: goal.clone(), ..SoloOptions::default() };
    let sg = generate(&opts, 42).expect("generate");
    let realms = vec![(realm, atlas)];
    let mut g = Game::create(
        NewGame {
            id: "sim".into(),
            name: "sim".into(),
            backend: Backend::Solo,
            seed_name: "sim".into(),
            slot: sg.slot,
            zone_realms: vec!["r".into()],
            realms: &realms,
            home,
            seed: 42,
            solo_rewards: sg.rewards,
            surface: apgo_core::assign::SurfacePref::Any,
            avoid_stairs: false,
        },
        &catalog,
    )
    .expect("game");

    let mut by_kind: BTreeMap<String, u32> = BTreeMap::new();
    let (mut fallbacks, mut effort_err) = (0, 0.0);
    for a in &g.assignments {
        *by_kind.entry(a.kind_id.clone()).or_default() += 1;
        fallbacks += u32::from(a.fallback);
        let mpt = f64::from(g.slot.minutes_per_tier);
        effort_err += (a.effort_min - (f64::from(a.tier) - 0.5) * mpt).abs();
    }
    println!("assigned {} quests ({} fallback), mean effort error {:.1} min", g.assignments.len(), fallbacks, effort_err / g.assignments.len() as f64);
    println!("kinds: {}", by_kind.iter().map(|(k, n)| format!("{k}x{n}")).collect::<Vec<_>>().join(", "));

    let mut t: i64 = 1_000_000;
    let mut steps = 100_000i64;
    let mut won = None;
    let mut done_events = 0;
    for _ in 0..4000 {
        let Some(q) = g.quest_views().into_iter().find(|q| matches!(q.state, QuestState::Open | QuestState::InProgress)) else { break };
        let mut fixes: Vec<(Point, i64, Option<i64>)> = Vec::new();
        let mut at = |p: Point, dt_s: i64, st: Option<i64>| {
            t += dt_s * 1000;
            fixes.push((p, t, st));
        };
        match &q.target {
            Target::Point { p, .. } => {
                at(*p, 700, None);
            }
            Target::Dwell { p, minutes, .. } => {
                at(*p, 700, None);
                at(*p, (minutes * 60.0) as i64 + 30, None);
            }
            Target::DwellArea { center, poly, minutes, .. } => {
                let c = if poly.len() >= 3 { apgo_core::geo::centroid(poly) } else { *center };
                at(c, 700, None);
                at(c, (minutes * 60.0) as i64 + 30, None);
            }
            Target::Line { pts, .. } => {
                for d in apgo_core::geo::densify(pts, 15.0) {
                    at(d, 12, None);
                }
            }
            Target::Courier { a, b, .. } => {
                at(*a, 700, None);
                at(*b, 200, None);
            }
            Target::RoundTrip { far, .. } => {
                at(*far, 700, None);
                at(home, 900, None);
            }
            Target::Cells { n, .. } => {
                let mut p = home;
                for _ in 0..(*n + 6) {
                    at(p, 20, None);
                    p = destination(p, 90.0, 200.0);
                }
            }
            Target::Steps { n } => {
                for _ in 0..(*n / 300 + 2) {
                    steps += 300;
                    at(home, 60, Some(steps));
                }
            }
            Target::Away { minutes, .. } => {
                let far = destination(home, 0.0, 3000.0);
                at(far, 700, None);
                for _ in 0..((*minutes as i64) / 4 + 2) {
                    at(far, 240, None);
                }
            }
        }
        for (p, ts, st) in fixes {
            for e in g.on_fix(Fix { lat: p.lat, lon: p.lon, t_ms: ts, accuracy_m: 5.0 }, st) {
                match e {
                    Event::QuestDone { .. } => done_events += 1,
                    Event::GoalAchieved { label } => won = Some(label),
                    _ => {}
                }
            }
        }
        if won.is_some() {
            break;
        }
    }
    let views = g.quest_views();
    let done = views.iter().filter(|v| v.state == QuestState::Done).count();
    println!("autoplay: {} quests done ({} events) in {:.1}s; goal {}: {:?}", done, done_events, t0.elapsed().as_secs_f64(), goal, won);
    for v in views.iter().filter(|v| matches!(v.state, QuestState::Open | QuestState::InProgress)).take(2) {
        println!("STUCK: {} state={:?} progress={} target={:?}", v.name, v.state, v.progress, v.target);
    }
    let stuck: Vec<_> = views
        .iter()
        .filter(|v| matches!(v.state, QuestState::Open | QuestState::InProgress))
        .map(|v| format!("{} ({:?})", v.name, std::mem::discriminant(&v.target)))
        .take(5)
        .collect();
    if !stuck.is_empty() {
        println!("not completed: {}", stuck.join("; "));
    }
    println!("final: {}", g.goal_status(t).label);
}
