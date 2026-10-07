//! Win conditions. Evaluated by the client (Archipelago logic cannot see these); see spec section 7.

use std::collections::{BTreeMap, BTreeSet};

use crate::assign::Assignment;
use crate::slot::{GoalMode, GoalSpec, SlotData};

#[derive(Debug, Clone, PartialEq)]
pub struct GoalStatus {
    /// 0.0..1.0
    pub progress: f32,
    pub achieved: bool,
    pub label: String,
}

pub struct GoalCtx<'a> {
    pub slot: &'a SlotData,
    pub assignments: &'a [Assignment],
    pub done: &'a BTreeSet<i64>,
    pub items: &'a [String],
    pub distance_m: f64,
    pub cells_discovered: usize,
    pub streak_days: u32,
}

fn status(have: f64, need: f64, label: String) -> GoalStatus {
    let progress = if need <= 0.0 { 1.0 } else { (have / need).clamp(0.0, 1.0) as f32 };
    GoalStatus { progress, achieved: have >= need, label }
}

fn letters(items: &[String]) -> BTreeMap<char, u32> {
    let mut m = BTreeMap::new();
    for i in items {
        if let Some(c) = i.strip_prefix("Letter ").and_then(|s| s.chars().next()) {
            *m.entry(c).or_insert(0) += 1;
        }
    }
    m
}

fn letters_needed(word: &str) -> BTreeMap<char, u32> {
    let mut m = BTreeMap::new();
    for c in word.chars() {
        *m.entry(c).or_insert(0) += 1;
    }
    m
}

fn letter_progress(items: &[String], word: &str) -> (f64, f64) {
    let have = letters(items);
    let need = letters_needed(word);
    let got: u32 = need.iter().map(|(c, n)| (*n).min(*have.get(c).unwrap_or(&0))).sum();
    (f64::from(got), word.len() as f64)
}

fn or_default(target: u32, default: u32) -> u32 {
    if target == 0 {
        default
    } else {
        target
    }
}

/// Every goal of the game with its own progress, for the UI.
pub fn evaluate_each(c: &GoalCtx) -> Vec<(GoalSpec, GoalStatus)> {
    c.slot
        .goal_list()
        .into_iter()
        .map(|g| {
            let st = evaluate_one(c, &g.id, g.target);
            (g, st)
        })
        .collect()
}

/// The game's win condition: its goal, or several goals combined by the slot's rule (any, all, or at least N).
pub fn evaluate(c: &GoalCtx) -> GoalStatus {
    let mut each = evaluate_each(c);
    if each.len() == 1 {
        return each.remove(0).1; // one goal keeps its own label and progress
    }
    let n = each.len();
    let need = match c.slot.goal_mode {
        GoalMode::Any => 1,
        GoalMode::All => n,
        GoalMode::AtLeast => (c.slot.goal_need as usize).clamp(1, n),
    };
    let done = each.iter().filter(|(_, s)| s.achieved).count();
    let mut progress: Vec<f32> = each.iter().map(|(_, s)| s.progress).collect();
    progress.sort_by(|a, b| b.total_cmp(a));
    let shown = progress.iter().take(need).sum::<f32>() / need as f32; // the closest `need` goals are what count
    let header = match c.slot.goal_mode {
        GoalMode::Any => format!("Finish any one of {n} goals"),
        GoalMode::All => format!("Finish all {n} goals"),
        GoalMode::AtLeast => format!("Finish {need} of {n} goals"),
    };
    let parts: Vec<&str> = each.iter().map(|(_, s)| s.label.as_str()).collect();
    GoalStatus { progress: shown, achieved: done >= need, label: format!("{header} ({done} done): {}", parts.join(" · ")) }
}

fn evaluate_one(c: &GoalCtx, g: &str, t: u32) -> GoalStatus {
    let trips_total = c.slot.trips.len() as f64;
    let boss_done = c.slot.boss.as_ref().is_some_and(|b| c.done.contains(&b.location_id));
    let done_assign = || c.assignments.iter().filter(|a| c.done.contains(&a.location_id));
    match g {
        "macguffin_short" | "macguffin_long" => {
            let word = if g == "macguffin_long" { "ARCHIPELAGO" } else { "APGO" };
            let (have, need) = letter_progress(c.items, word);
            status(have, need, format!("Collect the letters of {word}: {have:.0}/{need:.0}"))
        }
        "all_trips" => {
            let n = c.slot.all_quests().iter().filter(|q| c.done.contains(&q.location_id)).count() as f64;
            status(n, trips_total + f64::from(u8::from(c.slot.boss.is_some())), format!("Complete every quest: {n:.0}/{}", c.slot.all_quests().len()))
        }
        "boss" => status(f64::from(u8::from(boss_done)), 1.0, "Defeat The Big One".into()),
        "treasure_hunt" => {
            let (have, need) = letter_progress(c.items, "APGO");
            let total = need + 1.0;
            let got = have + f64::from(u8::from(boss_done && have >= need));
            status(got, total, format!("Collect APGO ({have:.0}/{need:.0}), then claim the treasure"))
        }
        "zone_conqueror" => {
            let pct = f64::from(or_default(t, 60));
            let worst = c
                .slot
                .zones
                .iter()
                .map(|z| {
                    let all: Vec<_> = c.slot.trips.iter().filter(|q| q.zone == z.id).collect();
                    let d = all.iter().filter(|q| c.done.contains(&q.location_id)).count() as f64;
                    if all.is_empty() {
                        100.0
                    } else {
                        100.0 * d / all.len() as f64
                    }
                })
                .fold(f64::MAX, f64::min);
            status(worst, pct, format!("Finish {pct:.0}% of every zone (weakest zone: {worst:.0}%)"))
        }
        "well_rounded" => {
            let want: BTreeSet<&str> = c.slot.trips.iter().map(|q| q.family.as_str()).collect();
            let got: BTreeSet<&str> = done_assign().filter(|a| a.family != "boss").map(|a| a.family.as_str()).filter(|f| want.contains(f)).collect();
            status(got.len() as f64, want.len() as f64, format!("Do one quest of every type: {}/{}", got.len(), want.len()))
        }
        "quest_dex" => {
            let need = or_default(t, 15);
            let kinds: BTreeSet<&str> = done_assign().map(|a| a.kind_id.as_str()).collect();
            status(kinds.len() as f64, f64::from(need), format!("Complete {need} different kinds of quest: {}", kinds.len()))
        }
        "marathon" => {
            let km = or_default(t, 42);
            status(c.distance_m / 1000.0, f64::from(km), format!("Travel {km} km on quests: {:.1} km", c.distance_m / 1000.0))
        }
        "explorer" => {
            let need = or_default(t, 300);
            status(c.cells_discovered as f64, f64::from(need), format!("Reveal {need} map cells: {}", c.cells_discovered))
        }
        "streak" => {
            let need = or_default(t, 7);
            status(f64::from(c.streak_days), f64::from(need), format!("Quest {need} days in a row: {}", c.streak_days))
        }
        "boss_rush" => {
            let need = or_default(t, 5);
            let hard = c.slot.all_quests().iter().filter(|q| q.difficulty == "hard" && c.done.contains(&q.location_id)).count();
            status(hard as f64, f64::from(need), format!("Finish {need} hard quests: {hard}"))
        }
        other => GoalStatus { progress: 0.0, achieved: false, label: format!("Unknown goal {other}") },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assign::Target;
    use crate::catalog::Mode;
    use crate::geo::Point;
    use crate::slot::{GoalMode, GoalSpec, QuestSlot, ZoneSlot};

    fn slot(goal: &str, target: u32) -> SlotData {
        let q = |id: i64, zone: u32, diff: &str, fam: &str| QuestSlot {
            location_id: id,
            zone,
            mode: Mode::Walk,
            difficulty: diff.into(),
            effort_tier: 2,
            family: fam.into(),
        };
        SlotData {
            schema_version: 2,
            goal: goal.into(),
            goal_target: target,
            minutes_per_tier: 10,
            reduction_percent: 8,
            min_distance_m: 150,
            fog_of_war: false,
            return_home: false,
            death_link: false,
            enabled_traps: vec![],
            zones: vec![
                ZoneSlot { id: 1, mode: Mode::Walk, zone_keys_needed: 0, tool: None },
                ZoneSlot { id: 2, mode: Mode::Walk, zone_keys_needed: 1, tool: None },
            ],
            trips: vec![q(1, 1, "easy", "reach"), q(2, 1, "hard", "dwell"), q(3, 2, "hard", "landmark"), q(4, 2, "hard", "reach")],
            boss: Some(QuestSlot { location_id: 9, zone: 2, mode: Mode::Walk, difficulty: "hard".into(), effort_tier: 10, family: "boss".into() }),
            goals: vec![],
            goal_mode: GoalMode::All,
            goal_need: 0,
        }
    }

    /// A slot whose win condition is several goals combined by `mode`.
    fn multi(goals: &[(&str, u32)], mode: GoalMode, need: u32) -> SlotData {
        let mut s = slot(goals[0].0, goals[0].1);
        s.goals = goals.iter().map(|(id, t)| GoalSpec { id: (*id).into(), target: *t }).collect();
        s.goal_mode = mode;
        s.goal_need = need;
        s
    }

    fn assigns(slot: &SlotData) -> Vec<Assignment> {
        slot.all_quests()
            .iter()
            .map(|q| Assignment {
                location_id: q.location_id,
                zone: q.zone,
                mode: q.mode,
                family: q.family.clone(),
                kind_id: format!("kind{}", q.location_id),
                quest_name: "x".into(),
                blurb: "".into(),
                place: "".into(),
                tier: 2,
                effort_min: 10.0,
                target: Target::Steps { n: 1 },
                fallback: false,
                boss: q.family == "boss",
            })
            .collect()
    }

    fn eval(s: &SlotData, done: &[i64], items: &[&str], dist: f64, cells: usize, streak: u32) -> GoalStatus {
        let a = assigns(s);
        let d: BTreeSet<i64> = done.iter().copied().collect();
        let it: Vec<String> = items.iter().map(|x| x.to_string()).collect();
        let _ = Point::new(0.0, 0.0);
        evaluate(&GoalCtx { slot: s, assignments: &a, done: &d, items: &it, distance_m: dist, cells_discovered: cells, streak_days: streak })
    }

    #[test]
    fn a_slot_without_a_goals_list_plays_its_single_goal_exactly_as_before() {
        let s = slot("boss", 0);
        assert_eq!(s.goal_list(), vec![GoalSpec { id: "boss".into(), target: 0 }]);
        let st = eval(&s, &[9], &[], 0.0, 0, 0);
        assert!(st.achieved);
        assert_eq!(st.label, "Defeat The Big One", "a single goal keeps its own label");
    }

    #[test]
    fn any_of_several_goals_is_won_by_the_first_one_finished() {
        let s = multi(&[("boss", 0), ("quest_dex", 3)], GoalMode::Any, 0);
        assert!(!eval(&s, &[], &[], 0.0, 0, 0).achieved);
        assert!(eval(&s, &[9], &[], 0.0, 0, 0).achieved, "the boss alone is enough");
        assert!(eval(&s, &[1, 2, 3], &[], 0.0, 0, 0).achieved, "so are three kinds of quest");
    }

    #[test]
    fn all_of_several_goals_needs_every_one_and_shows_the_average_progress() {
        let s = multi(&[("boss", 0), ("quest_dex", 2)], GoalMode::All, 0);
        let half = eval(&s, &[9], &[], 0.0, 0, 0); // boss done, 1 of 2 kinds
        assert!(!half.achieved);
        assert!((half.progress - 0.75).abs() < 1e-6, "mean of 1.0 and 0.5, got {}", half.progress);
        assert!(eval(&s, &[9, 1, 2], &[], 0.0, 0, 0).achieved);
    }

    #[test]
    fn at_least_n_of_several_goals_counts_finished_goals() {
        let s = multi(&[("boss", 0), ("quest_dex", 2), ("marathon", 5)], GoalMode::AtLeast, 2);
        assert!(!eval(&s, &[9], &[], 0.0, 0, 0).achieved, "only one goal is done");
        assert!(eval(&s, &[9], &[], 5000.0, 0, 0).achieved, "boss and 5 km are two");
        assert!(eval(&s, &[1, 2], &[], 5000.0, 0, 0).achieved, "so are two kinds and 5 km");
    }

    #[test]
    fn each_goal_reports_its_own_status_for_the_ui() {
        let s = multi(&[("boss", 0), ("quest_dex", 2)], GoalMode::All, 0);
        let a = assigns(&s);
        let d: BTreeSet<i64> = [9].into_iter().collect();
        let each = evaluate_each(&GoalCtx { slot: &s, assignments: &a, done: &d, items: &[], distance_m: 0.0, cells_discovered: 0, streak_days: 0 });
        assert_eq!(each.len(), 2);
        assert_eq!((each[0].0.id.as_str(), each[0].1.achieved), ("boss", true));
        assert_eq!((each[1].0.id.as_str(), each[1].1.achieved), ("quest_dex", false));
    }

    #[test]
    fn an_unusable_combination_is_refused() {
        assert!(multi(&[("boss", 0)], GoalMode::AtLeast, 2).check_goals().is_err(), "needs 2 of 1 goals");
        assert!(multi(&[("nonsense", 0)], GoalMode::All, 0).check_goals().is_err());
        assert!(multi(&[("boss", 0), ("boss", 0)], GoalMode::All, 0).check_goals().is_err(), "the same goal twice");
        assert!(multi(&[("boss", 0), ("quest_dex", 3)], GoalMode::AtLeast, 2).check_goals().is_ok());
    }

    #[test]
    fn letters_goals_count_duplicates() {
        let s = slot("macguffin_long", 0);
        let all: Vec<&str> = "ARCHIPELAGO".chars().map(|_| "").collect();
        assert_eq!(all.len(), 11);
        let mut items: Vec<String> = "ARCHIPELAGO".chars().map(|c| format!("Letter {c}")).collect();
        let st = eval(&s, &[], &items.iter().map(String::as_str).collect::<Vec<_>>(), 0.0, 0, 0);
        assert!(st.achieved);
        items.pop(); // missing the final O
        let st = eval(&s, &[], &items.iter().map(String::as_str).collect::<Vec<_>>(), 0.0, 0, 0);
        assert!(!st.achieved && st.progress > 0.9);
        let one_a = eval(&slot("macguffin_short", 0), &[], &["Letter A", "Letter P", "Letter G"], 0.0, 0, 0);
        assert!(!one_a.achieved);
    }

    #[test]
    fn all_trips_boss_and_treasure() {
        assert!(!eval(&slot("all_trips", 0), &[1, 2, 3, 4], &[], 0.0, 0, 0).achieved, "boss counts for all_trips");
        assert!(eval(&slot("all_trips", 0), &[1, 2, 3, 4, 9], &[], 0.0, 0, 0).achieved);
        assert!(eval(&slot("boss", 0), &[9], &[], 0.0, 0, 0).achieved);
        assert!(!eval(&slot("treasure_hunt", 0), &[9], &["Letter A"], 0.0, 0, 0).achieved, "treasure needs the letters first");
        assert!(eval(&slot("treasure_hunt", 0), &[9], &["Letter A", "Letter P", "Letter G", "Letter O"], 0.0, 0, 0).achieved);
    }

    #[test]
    fn counting_goals_use_target_or_their_default() {
        assert!(eval(&slot("zone_conqueror", 50), &[1, 3], &[], 0.0, 0, 0).achieved);
        assert!(!eval(&slot("zone_conqueror", 0), &[1, 3], &[], 0.0, 0, 0).achieved, "default 60% needs more than half");
        assert!(eval(&slot("well_rounded", 0), &[1, 2, 3], &[], 0.0, 0, 0).achieved);
        assert!(!eval(&slot("well_rounded", 0), &[1, 2], &[], 0.0, 0, 0).achieved);
        assert!(eval(&slot("quest_dex", 3), &[1, 2, 3], &[], 0.0, 0, 0).achieved);
        assert!(eval(&slot("marathon", 0), &[], &[], 42_500.0, 0, 0).achieved);
        assert!(!eval(&slot("marathon", 0), &[], &[], 41_900.0, 0, 0).achieved);
        assert!(eval(&slot("explorer", 10), &[], &[], 0.0, 10, 0).achieved);
        assert!(eval(&slot("streak", 0), &[], &[], 0.0, 0, 7).achieved);
        assert!(eval(&slot("boss_rush", 3), &[2, 3, 4], &[], 0.0, 0, 0).achieved);
        assert!(!eval(&slot("boss_rush", 0), &[2, 3, 4], &[], 0.0, 0, 0).achieved, "default 5 hard quests");
        assert!(eval(&slot("nonsense", 0), &[], &[], 0.0, 0, 0).label.contains("Unknown"));
    }
}
