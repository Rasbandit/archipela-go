//! Win conditions. Evaluated by the client (Archipelago logic cannot see these); see spec section 7.

use std::collections::{BTreeMap, BTreeSet};

use crate::num::{count_f32, count_f64, to_f32};
use crate::slot::{GoalMode, GoalSpec, SlotData};

/// How far along a win condition is.
#[derive(Debug, Clone, PartialEq)]
pub struct GoalStatus {
    /// 0.0..1.0
    pub progress: f32,
    /// Whether the condition is met.
    pub achieved: bool,
    /// Short text describing the progress, for the UI.
    pub label: String,
}

/// Everything a win-condition check needs to know about the game.
pub struct GoalCtx<'a> {
    /// The slot data holding the goal settings.
    pub slot: &'a SlotData,
    /// Location ids of completed quests.
    pub done: &'a BTreeSet<i64>,
    /// Names of items received so far.
    pub items: &'a [String],
}

/// How a goal is written in an Archipelago YAML's `goal_selection` (the apworld's `GOAL_NAMES`).
#[must_use]
pub fn goal_yaml_name(id: &str) -> &'static str {
    match id {
        "treasure_hunt" => "Treasure Hunt",
        "all_trips" => "Completionist",
        _ => "",
    }
}

fn status(have: f64, need: f64, label: String) -> GoalStatus {
    let progress = if need <= 0.0 { 1.0 } else { to_f32((have / need).clamp(0.0, 1.0)) };
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
    (f64::from(got), count_f64(word.len()))
}

/// Every goal of the game with its own progress, for the UI.
#[must_use]
pub fn evaluate_each(c: &GoalCtx<'_>) -> Vec<(GoalSpec, GoalStatus)> {
    c.slot
        .goal_list()
        .into_iter()
        .map(|g| {
            let st = evaluate_one(c, &g.id);
            (g, st)
        })
        .collect()
}

/// The game's win condition: its goal, or several goals combined by the slot's rule (any, all, or at least N).
#[must_use]
pub fn evaluate(c: &GoalCtx<'_>) -> GoalStatus {
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
    let shown = progress.iter().take(need).sum::<f32>() / count_f32(need); // the closest `need` goals are what count
    let header = match c.slot.goal_mode {
        GoalMode::Any => format!("Finish any one of {n} goals"),
        GoalMode::All => format!("Finish all {n} goals"),
        GoalMode::AtLeast => format!("Finish {need} of {n} goals"),
    };
    let parts: Vec<&str> = each.iter().map(|(_, s)| s.label.as_str()).collect();
    GoalStatus { progress: shown, achieved: done >= need, label: format!("{header} ({done} done): {}", parts.join(" · ")) }
}

fn evaluate_one(c: &GoalCtx<'_>, g: &str) -> GoalStatus {
    let trips_total = count_f64(c.slot.trips.len());
    let boss_done = c.slot.boss.as_ref().is_some_and(|b| c.done.contains(&b.location_id));
    match g {
        "all_trips" => {
            let n = count_f64(c.slot.all_quests().iter().filter(|q| c.done.contains(&q.location_id)).count());
            status(n, trips_total + f64::from(u8::from(c.slot.boss.is_some())), format!("Complete every quest: {n:.0}/{}", c.slot.all_quests().len()))
        }
        "treasure_hunt" => {
            let (have, need) = letter_progress(c.items, "APGO");
            let total = need + 1.0;
            let got = have + f64::from(u8::from(boss_done && have >= need));
            status(got, total, format!("Collect APGO ({have:.0}/{need:.0}), then claim the treasure"))
        }
        other => GoalStatus { progress: 0.0, achieved: false, label: format!("Unknown goal {other}") },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Mode;
    use crate::slot::{GoalMode, GoalSpec, QuestSlot, ZoneSlot};

    const APGO: [&str; 4] = ["Letter A", "Letter P", "Letter G", "Letter O"];

    /// Four trips (ids 1-4) and the Boss Quest (id 9), over two zones.
    fn slot(goal: &str) -> SlotData {
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
            goal_target: 0,
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
    fn multi(goals: &[&str], mode: GoalMode, need: u32) -> SlotData {
        let mut s = slot(goals[0]);
        s.goals = goals.iter().map(|id| GoalSpec { id: (*id).into(), target: 0 }).collect();
        s.goal_mode = mode;
        s.goal_need = need;
        s
    }

    fn ctx_parts(done: &[i64], items: &[&str]) -> (BTreeSet<i64>, Vec<String>) {
        (done.iter().copied().collect(), items.iter().map(ToString::to_string).collect())
    }

    fn eval(s: &SlotData, done: &[i64], items: &[&str]) -> GoalStatus {
        let (d, it) = ctx_parts(done, items);
        evaluate(&GoalCtx { slot: s, done: &d, items: &it })
    }

    #[test]
    fn a_slot_without_a_goals_list_plays_its_single_goal_exactly_as_before() {
        let s = slot("all_trips");
        assert_eq!(s.goal_list(), vec![GoalSpec { id: "all_trips".into(), target: 0 }]);
        let st = eval(&s, &[1, 2, 3, 4, 9], &[]);
        assert!(st.achieved);
        assert_eq!(st.label, "Complete every quest: 5/5", "a single goal keeps its own label");
    }

    #[test]
    fn any_of_several_goals_is_won_by_the_first_one_finished() {
        let s = multi(&["treasure_hunt", "all_trips"], GoalMode::Any, 0);
        assert!(!eval(&s, &[], &[]).achieved);
        assert!(eval(&s, &[9], &APGO).achieved, "the treasure alone is enough");
        assert!(eval(&s, &[1, 2, 3, 4, 9], &[]).achieved, "so is every quest");
    }

    #[test]
    fn all_of_several_goals_needs_every_one_and_shows_the_average_progress() {
        let s = multi(&["treasure_hunt", "all_trips"], GoalMode::All, 0);
        let part = eval(&s, &[9], &APGO); // treasure done (1.0), 1 of 5 quests (0.2)
        assert!(!part.achieved);
        assert!((part.progress - 0.6).abs() < 1e-6, "mean of 1.0 and 0.2, got {}", part.progress);
        assert!(eval(&s, &[1, 2, 3, 4, 9], &APGO).achieved);
    }

    #[test]
    fn at_least_n_of_several_goals_counts_finished_goals() {
        let s = multi(&["treasure_hunt", "all_trips"], GoalMode::AtLeast, 2);
        assert!(!eval(&s, &[9], &APGO).achieved, "only one goal is done");
        assert!(eval(&s, &[1, 2, 3, 4, 9], &APGO).achieved, "both are done");
    }

    #[test]
    fn each_goal_reports_its_own_status_for_the_ui() {
        let s = multi(&["treasure_hunt", "all_trips"], GoalMode::All, 0);
        let (d, it) = ctx_parts(&[9], &APGO);
        let each = evaluate_each(&GoalCtx { slot: &s, done: &d, items: &it });
        assert_eq!(each.len(), 2);
        assert_eq!((each[0].0.id.as_str(), each[0].1.achieved), ("treasure_hunt", true));
        assert_eq!((each[1].0.id.as_str(), each[1].1.achieved), ("all_trips", false));
    }

    #[test]
    fn an_unusable_combination_is_refused() {
        assert!(multi(&["all_trips"], GoalMode::AtLeast, 2).check_goals().is_err(), "needs 2 of 1 goals");
        assert!(multi(&["nonsense"], GoalMode::All, 0).check_goals().is_err());
        assert!(multi(&["all_trips", "all_trips"], GoalMode::All, 0).check_goals().is_err(), "the same goal twice");
        assert!(multi(&["treasure_hunt", "all_trips"], GoalMode::AtLeast, 2).check_goals().is_ok());
    }

    #[test]
    fn treasure_needs_every_letter_then_the_boss() {
        let s = slot("treasure_hunt");
        assert!(!eval(&s, &[9], &["Letter A", "Letter P", "Letter G"]).achieved, "missing the O");
        assert!(!eval(&s, &[], &APGO).achieved, "letters alone do not claim the treasure");
        assert!(eval(&s, &[9], &APGO).achieved);
        assert!(eval(&s, &[9], &["Letter A", "Letter A", "Letter P", "Letter G", "Letter O"]).achieved, "a duplicate letter is harmless");
    }

    #[test]
    fn all_trips_counts_the_boss() {
        assert!(!eval(&slot("all_trips"), &[1, 2, 3, 4], &[]).achieved, "the boss counts for all_trips");
        assert!(eval(&slot("all_trips"), &[1, 2, 3, 4, 9], &[]).achieved);
    }

    #[test]
    fn a_removed_goal_is_unknown() {
        for id in ["boss", "marathon", "macguffin_short", "nonsense"] {
            assert_eq!(eval(&slot(id), &[9], &APGO).label, format!("Unknown goal {id}"));
        }
    }
}
