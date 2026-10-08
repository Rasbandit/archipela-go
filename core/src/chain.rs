//! Progressive quests: the quests of one kind in one zone that share a counter (steps, minutes away, map squares),
//! shown as one bar with a mark for each check they unlock.

use std::collections::BTreeMap;

use crate::assign::{Assignment, Target};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainUnit {
    Steps,
    Minutes,
    Cells,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Milestone {
    pub location_id: i64,
    /// The counter value at which this check unlocks (a running total of the members' own amounts).
    pub at: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    /// `"{zone}:{kind_id}"`.
    pub id: String,
    pub zone: u32,
    pub kind_id: String,
    pub name: String,
    pub unit: ChainUnit,
    pub marks: Vec<Milestone>,
}

/// The unit and amount a quest contributes to a chain; `None` for quests that are not progressive.
pub fn amount_of(t: &Target) -> Option<(ChainUnit, f64)> {
    match t {
        Target::Steps { n } => Some((ChainUnit::Steps, f64::from(*n))),
        Target::Away { minutes, .. } => Some((ChainUnit::Minutes, *minutes)),
        Target::Cells { n, .. } => Some((ChainUnit::Cells, f64::from(*n))),
        _ => None,
    }
}

pub fn is_chain_target(t: &Target) -> bool {
    amount_of(t).is_some()
}

/// 30000 -> "30,000".
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// 270 -> "4 h 30 min", 45 -> "45 min", 120 -> "2 h".
pub fn minutes_text(m: f64) -> String {
    let m = m.max(0.0).round() as u64;
    match (m / 60, m % 60) {
        (0, r) => format!("{r} min"),
        (h, 0) => format!("{h} h"),
        (h, r) => format!("{h} h {r} min"),
    }
}

/// 850 -> "850 m", 1200 -> "1.2 km".
pub fn distance_text(m: f64) -> String {
    if m >= 1000.0 {
        format!("{:.1} km", m / 1000.0)
    } else {
        format!("{m:.0} m")
    }
}

impl Chain {
    pub fn total(&self) -> f64 {
        self.marks.last().map_or(0.0, |m| m.at)
    }

    /// Location ids whose mark is at or below `counter`, in mark order.
    pub fn reached(&self, counter: f64) -> Vec<i64> {
        self.marks.iter().filter(|m| m.at <= counter + 1e-9).map(|m| m.location_id).collect()
    }

    /// 1-based position of a member among the marks ("milestone 3 of 5").
    pub fn position_of(&self, location_id: i64) -> Option<usize> {
        self.marks.iter().position(|m| m.location_id == location_id).map(|i| i + 1)
    }

    /// "Take 30,000 steps" / "Spend 4 h 30 min at least 1.2 km from home" / "Visit 60 new map squares".
    pub fn rule_text(&self, away_m: f64) -> String {
        let t = self.total();
        match self.unit {
            ChainUnit::Steps => format!("Take {} steps", thousands(t.round() as u64)),
            ChainUnit::Minutes => format!("Spend {} at least {} from home", minutes_text(t), distance_text(away_m)),
            ChainUnit::Cells => format!("Visit {} new map squares", t.round() as u64),
        }
    }

    /// The amount at a mark: "8,500 steps", "1 h 30 min", "40 squares".
    pub fn amount_text(&self, at: f64) -> String {
        match self.unit {
            ChainUnit::Steps => format!("{} steps", thousands(at.round() as u64)),
            ChainUnit::Minutes => minutes_text(at),
            ChainUnit::Cells => format!("{} squares", at.round() as u64),
        }
    }
}

/// One quest in a chain before sorting: location id, quest name, unit, amount.
type Member = (i64, String, ChainUnit, f64);

/// Group the progressive quests by zone and kind. Members are ordered by their own amount (ties by location id); each mark is the running total.
pub fn derive(assignments: &[Assignment]) -> Vec<Chain> {
    let mut groups: BTreeMap<(u32, String), Vec<Member>> = BTreeMap::new();
    for a in assignments {
        if let Some((unit, amount)) = amount_of(&a.target) {
            groups.entry((a.zone, a.kind_id.clone())).or_default().push((a.location_id, a.quest_name.clone(), unit, amount));
        }
    }
    groups
        .into_iter()
        .map(|((zone, kind_id), mut members)| {
            members.sort_by(|a, b| a.3.total_cmp(&b.3).then(a.0.cmp(&b.0)));
            let (name, unit) = (members[0].1.clone(), members[0].2);
            let mut running = 0.0;
            let marks = members
                .iter()
                .map(|(location_id, _, _, amount)| {
                    running += amount;
                    Milestone { location_id: *location_id, at: running }
                })
                .collect();
            Chain { id: format!("{zone}:{kind_id}"), zone, kind_id, name, unit, marks }
        })
        .collect()
}

// Used by the tests of other modules.
#[cfg(test)]
pub(crate) mod tests_support {
    pub(crate) use super::tests::member;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Mode;
    use crate::geo::Point;

    pub(crate) fn member(id: i64, zone: u32, kind: &str, target: Target) -> Assignment {
        Assignment {
            location_id: id,
            zone,
            mode: Mode::Walk,
            family: "steps".into(),
            kind_id: kind.into(),
            quest_name: kind.replace('_', " "),
            blurb: String::new(),
            place: "Anywhere".into(),
            tier: 1,
            effort_min: 10.0,
            target,
            fallback: false,
            boss: false,
        }
    }

    fn steps(id: i64, n: u32) -> Assignment {
        member(id, 1, "step_up", Target::Steps { n })
    }

    #[test]
    fn amount_of_maps_only_the_three_progressive_targets() {
        assert_eq!(amount_of(&Target::Steps { n: 500 }), Some((ChainUnit::Steps, 500.0)));
        assert_eq!(amount_of(&Target::Away { min_distance_m: 900.0, minutes: 45.0 }), Some((ChainUnit::Minutes, 45.0)));
        assert_eq!(amount_of(&Target::Cells { n: 12, cell_m: 150.0 }), Some((ChainUnit::Cells, 12.0)));
        assert_eq!(amount_of(&Target::Point { p: Point::new(0.0, 0.0), r: 40.0 }), None);
        assert!(is_chain_target(&Target::Steps { n: 1 }));
    }

    #[test]
    fn marks_are_running_totals_of_the_members_sorted_by_amount() {
        let chains = derive(&[steps(30, 5500), steps(10, 500), steps(20, 2500)]);
        assert_eq!(chains.len(), 1);
        let c = &chains[0];
        assert_eq!(c.id, "1:step_up");
        assert_eq!((c.zone, c.unit, c.name.as_str()), (1, ChainUnit::Steps, "step up"));
        assert_eq!(
            c.marks,
            vec![Milestone { location_id: 10, at: 500.0 }, Milestone { location_id: 20, at: 3000.0 }, Milestone { location_id: 30, at: 8500.0 }]
        );
        assert_eq!(c.total(), 8500.0);
    }

    #[test]
    fn equal_amounts_are_ordered_by_location_id() {
        let c = &derive(&[steps(7, 1000), steps(3, 1000)])[0];
        assert_eq!(c.marks.iter().map(|m| m.location_id).collect::<Vec<_>>(), vec![3, 7]);
        assert_eq!(c.marks.iter().map(|m| m.at).collect::<Vec<_>>(), vec![1000.0, 2000.0]);
    }

    #[test]
    fn zones_and_kinds_make_separate_chains_and_other_quests_are_ignored() {
        let p = Point::new(40.0, -111.0);
        let list = vec![
            steps(1, 500),
            member(2, 2, "step_up", Target::Steps { n: 500 }),
            member(3, 1, "wanderlust", Target::Away { min_distance_m: 900.0, minutes: 30.0 }),
            member(4, 1, "street_smarts", Target::Point { p, r: 40.0 }),
        ];
        let ids: Vec<String> = derive(&list).into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["1:step_up", "1:wanderlust", "2:step_up"]);
    }

    #[test]
    fn no_progressive_quests_means_no_chains_and_one_member_is_a_one_mark_bar() {
        assert!(derive(&[]).is_empty());
        let c = &derive(&[steps(1, 800)])[0];
        assert_eq!(c.marks.len(), 1);
        assert_eq!(c.total(), 800.0);
    }

    #[test]
    fn reached_lists_the_marks_at_or_below_the_counter() {
        let c = &derive(&[steps(10, 500), steps(20, 2500), steps(30, 5500)])[0];
        assert!(c.reached(499.0).is_empty());
        assert_eq!(c.reached(3000.0), vec![10, 20]);
        assert_eq!(c.reached(1e9), vec![10, 20, 30]);
        assert_eq!(c.position_of(20), Some(2));
        assert_eq!(c.position_of(99), None);
    }

    #[test]
    fn texts_read_naturally() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(950), "950");
        assert_eq!(thousands(30_000), "30,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(minutes_text(45.0), "45 min");
        assert_eq!(minutes_text(120.0), "2 h");
        assert_eq!(minutes_text(270.0), "4 h 30 min");
        assert_eq!(distance_text(850.0), "850 m");
        assert_eq!(distance_text(1200.0), "1.2 km");
        let s = &derive(&[steps(10, 500), steps(20, 29_500)])[0];
        assert_eq!(s.rule_text(0.0), "Take 30,000 steps");
        assert_eq!(s.amount_text(8500.0), "8,500 steps");
        let a = &derive(&[member(1, 1, "wanderlust", Target::Away { min_distance_m: 1.0, minutes: 270.0 })])[0];
        assert_eq!(a.rule_text(1200.0), "Spend 4 h 30 min at least 1.2 km from home");
        assert_eq!(a.amount_text(90.0), "1 h 30 min");
        let c = &derive(&[member(1, 1, "cartographer", Target::Cells { n: 60, cell_m: 150.0 })])[0];
        assert_eq!(c.rule_text(0.0), "Visit 60 new map squares");
        assert_eq!(c.amount_text(40.0), "40 squares");
    }
}
